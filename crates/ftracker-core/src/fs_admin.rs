//! Capped, iterative admin file walks. Never follow reparse points.

use std::fs::{self, Metadata};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

pub const LIST_CAP: usize = 240;
pub const MAX_DEPTH: usize = 64;
pub const MAX_VISITS: usize = 100_000;

#[derive(Debug, Clone)]
pub struct DirList {
    pub dirs: Vec<PathBuf>,
    pub files: Vec<(PathBuf, u64)>,
    pub truncated: bool,
}

pub fn list_dir(path: &Path) -> io::Result<DirList> {
    let rd = fs::read_dir(path)?;
    let mut entries: Vec<fs::DirEntry> = Vec::new();
    let mut truncated = false;
    for e in rd {
        let e = match e {
            Ok(e) => e,
            Err(_) => continue,
        };
        if entries.len() >= LIST_CAP {
            truncated = true;
            break;
        }
        entries.push(e);
    }
    entries.sort_by(|a, b| {
        a.file_name()
            .to_string_lossy()
            .to_ascii_lowercase()
            .cmp(&b.file_name().to_string_lossy().to_ascii_lowercase())
    });
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for e in entries {
        let p = e.path();
        let meta = match fs::symlink_metadata(&p) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if is_listed_dir(&meta) {
            dirs.push(p);
        } else {
            files.push((p, meta.len()));
        }
    }
    Ok(DirList {
        dirs,
        files,
        truncated,
    })
}

pub fn dir_size(path: &Path) -> io::Result<u64> {
    let mut total = 0u64;
    let mut stack: Vec<(PathBuf, usize)> = vec![(path.to_path_buf(), 0)];
    let mut visits = 0usize;
    while let Some((p, depth)) = stack.pop() {
        visits = visits.saturating_add(1);
        if visits > MAX_VISITS || depth > MAX_DEPTH {
            break;
        }
        let meta = match fs::symlink_metadata(&p) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if is_reparse(&meta) {
            continue;
        }
        if meta.file_type().is_dir() {
            let rd = match fs::read_dir(&p) {
                Ok(rd) => rd,
                Err(_) => continue,
            };
            for e in rd.flatten() {
                let child = e.path();
                if is_reparse_path(&child) {
                    continue;
                }
                if visits.saturating_add(stack.len()) >= MAX_VISITS {
                    break;
                }
                stack.push((child, depth.saturating_add(1)));
            }
        } else {
            total = total.saturating_add(meta.len());
        }
    }
    Ok(total)
}

pub fn copy_tree(src: &Path, dest: &Path) -> io::Result<u64> {
    let dest = resolve_copy_dest(src, dest);
    let src_meta = fs::symlink_metadata(src)?;
    if is_reparse(&src_meta) {
        return Err(io::Error::new(
            ErrorKind::Other,
            "source is a reparse point",
        ));
    }
    let mut stack = vec![(src.to_path_buf(), dest, 0usize)];
    let mut total = 0u64;
    let mut visits = 0usize;
    while let Some((s, d, depth)) = stack.pop() {
        visits = visits.saturating_add(1);
        if visits > MAX_VISITS || depth > MAX_DEPTH {
            return Err(cap_err("copy"));
        }
        let meta = fs::symlink_metadata(&s)?;
        if is_reparse(&meta) {
            continue;
        }
        if meta.file_type().is_dir() {
            fs::create_dir_all(&d)?;
            for e in fs::read_dir(&s)? {
                let e = e?;
                if is_reparse_path(&e.path()) {
                    continue;
                }
                if visits.saturating_add(stack.len()) >= MAX_VISITS {
                    return Err(cap_err("copy"));
                }
                stack.push((e.path(), d.join(e.file_name()), depth.saturating_add(1)));
            }
        } else {
            if let Some(parent) = d.parent() {
                fs::create_dir_all(parent)?;
            }
            let n = fs::copy(&s, &d)?;
            total = total.saturating_add(n);
        }
    }
    Ok(total)
}

pub fn delete_tree(path: &Path) -> io::Result<()> {
    enum Step {
        Walk(PathBuf, usize),
        RmDir(PathBuf),
    }
    let mut stack = vec![Step::Walk(path.to_path_buf(), 0)];
    let mut visits = 0usize;
    while let Some(step) = stack.pop() {
        match step {
            Step::Walk(p, depth) => {
                visits = visits.saturating_add(1);
                if visits > MAX_VISITS || depth > MAX_DEPTH {
                    return Err(cap_err("delete"));
                }
                let meta = fs::symlink_metadata(&p)?;
                if is_reparse(&meta) {
                    remove_reparse(&p, &meta)?;
                } else if meta.file_type().is_dir() {
                    stack.push(Step::RmDir(p.clone()));
                    if let Ok(rd) = fs::read_dir(&p) {
                        for e in rd.flatten() {
                            if visits.saturating_add(stack.len()) >= MAX_VISITS {
                                return Err(cap_err("delete"));
                            }
                            stack.push(Step::Walk(e.path(), depth.saturating_add(1)));
                        }
                    }
                } else {
                    fs::remove_file(&p)?;
                }
            }
            Step::RmDir(p) => {
                fs::remove_dir(&p)?;
            }
        }
    }
    Ok(())
}

fn resolve_copy_dest(src: &Path, dest: &Path) -> PathBuf {
    match fs::symlink_metadata(dest) {
        Ok(m) if m.file_type().is_dir() && !is_reparse(&m) => {
            dest.join(src.file_name().unwrap_or_default())
        }
        _ => dest.to_path_buf(),
    }
}

fn cap_err(op: &str) -> io::Error {
    io::Error::new(ErrorKind::Other, format!("{op} exceeded visit or depth cap"))
}

fn is_listed_dir(meta: &Metadata) -> bool {
    meta.file_type().is_dir() || is_reparse(meta)
}

fn is_reparse_path(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| is_reparse(&m))
        .unwrap_or(false)
}

fn is_reparse(meta: &Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn has_dir_attr(meta: &Metadata) -> bool {
    if meta.file_type().is_dir() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
        return meta.file_attributes() & FILE_ATTRIBUTE_DIRECTORY != 0;
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn remove_reparse(path: &Path, meta: &Metadata) -> io::Result<()> {
    if has_dir_attr(meta) {
        match fs::remove_dir(path) {
            Ok(()) => Ok(()),
            Err(_) => fs::remove_file(path),
        }
    } else {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(_) => fs::remove_dir(path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let n = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let p = std::env::temp_dir().join(format!(
                "ft{tag}{}{n}",
                std::process::id()
            ));
            fs::create_dir_all(&p).unwrap();
            Scratch(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = delete_tree(&self.0);
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn list_caps_without_loading_all() {
        let s = Scratch::new("cap");
        for i in 0..300 {
            fs::write(s.path().join(format!("f{i:03}.txt")), b"x").unwrap();
        }
        let list = list_dir(s.path()).unwrap();
        assert!(list.truncated);
        assert_eq!(list.files.len() + list.dirs.len(), LIST_CAP);
    }

    #[test]
    fn deep_nest_size_copy_delete() {
        let s = Scratch::new("deep");
        let mut p = s.path().to_path_buf();
        for _ in 0..40 {
            p.push("d");
            fs::create_dir(&p).unwrap();
        }
        fs::write(p.join("f.txt"), b"abcd").unwrap();
        assert_eq!(dir_size(s.path()).unwrap(), 4);

        let dest = Scratch::new("deep-copy");
        let n = copy_tree(s.path(), dest.path()).unwrap();
        assert_eq!(n, 4);
        assert_eq!(dir_size(dest.path()).unwrap(), 4);

        delete_tree(s.path()).unwrap();
        assert!(fs::symlink_metadata(s.path()).is_err());
    }

    #[test]
    fn eighty_deep_does_not_overflow() {
        let s = Scratch::new("eighty");
        let mut p = s.path().to_path_buf();
        for _ in 0..80 {
            p.push("x");
            fs::create_dir(&p).unwrap();
        }
        fs::write(p.join("z.txt"), b"zz").unwrap();
        let sz = dir_size(s.path());
        assert!(sz.is_ok());
        let dest = Scratch::new("eighty-copy");
        assert!(copy_tree(s.path(), dest.path()).is_err());
    }

    #[test]
    fn reparse_is_not_descended() {
        let s = Scratch::new("reparse");
        let target = s.path().join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("secret.bin"), b"hello").unwrap();
        let link = s.path().join("link");
        let made = make_dir_link(&target, &link);
        if !made {
            return;
        }
        let list = list_dir(s.path()).unwrap();
        assert!(list.dirs.iter().any(|d| d.file_name().unwrap() == "link"));
        assert_eq!(dir_size(&link).unwrap(), 0);
        assert_eq!(dir_size(s.path()).unwrap(), 5);
        assert!(copy_tree(&link, &s.path().join("copied-link")).is_err());
        delete_tree(&link).unwrap();
        assert!(fs::symlink_metadata(&link).is_err());
        assert!(target.join("secret.bin").is_file());
    }

    fn make_dir_link(target: &Path, link: &Path) -> bool {
        #[cfg(windows)]
        {
            if std::os::windows::fs::symlink_dir(target, link).is_ok() {
                return true;
            }
            std::process::Command::new("cmd")
                .args([
                    "/C",
                    "mklink",
                    "/J",
                    &link.to_string_lossy(),
                    &target.to_string_lossy(),
                ])
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        }
        #[cfg(not(windows))]
        {
            std::os::unix::fs::symlink(target, link).is_ok()
        }
    }
}
