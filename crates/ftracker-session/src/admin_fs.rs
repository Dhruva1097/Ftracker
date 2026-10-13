//! Session-side listing, copy, delete, and installed-program enumeration.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use ftracker_core::paths;

pub const TG_MAX_DOC: u64 = 50 * 1024 * 1024;
const LIST_CAP: usize = 80;

#[derive(Debug, Clone)]
pub struct DirListing {
    pub path: String,
    pub parent: Option<PathBuf>,
    pub dirs: Vec<PathBuf>,
    pub files: Vec<(PathBuf, u64)>,
    pub truncated: bool,
}

#[derive(Debug, Clone)]
pub struct InstalledProgram {
    pub name: String,
    pub publisher: String,
    pub uninstall: String,
    pub quiet: String,
    pub product_code: String,
}

pub fn refuse_delete(path: &Path) -> Option<&'static str> {
    if paths::is_drive_root(path) {
        return Some("refused: drive root");
    }
    if paths::is_protected_admin_path(path) {
        return Some("refused: Windows or Ftracker path");
    }
    None
}

pub fn refuse_write_dest(path: &Path) -> Option<&'static str> {
    if paths::is_protected_admin_path(path) {
        return Some("refused: Windows or Ftracker path");
    }
    None
}

pub fn is_ftracker_program(display: &str) -> bool {
    let k = display
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect::<String>();
    k == "ftracker" || k.starts_with("ftracker")
}

pub fn list_drives() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        use windows::Win32::Storage::FileSystem::GetLogicalDrives;
        let mask = unsafe { GetLogicalDrives() };
        let mut out = Vec::new();
        for i in 0..26u32 {
            if i < 2 {
                continue;
            }
            if mask & (1 << i) == 0 {
                continue;
            }
            let letter = (b'A' + i as u8) as char;
            out.push(PathBuf::from(format!("{letter}:\\")));
        }
        out
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

pub fn list_dir(path: &Path) -> Result<DirListing> {
    let display = path.display().to_string();
    let parent = if paths::is_drive_root(path) {
        None
    } else {
        path.parent().map(|p| p.to_path_buf())
    };
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    let mut truncated = false;
    let rd = fs::read_dir(path).map_err(|e| anyhow!("{}: {e}", path.display()))?;
    let mut entries: Vec<fs::DirEntry> = rd.filter_map(|e| e.ok()).collect();
    entries.sort_by(|a, b| {
        a.file_name()
            .to_string_lossy()
            .to_ascii_lowercase()
            .cmp(&b.file_name().to_string_lossy().to_ascii_lowercase())
    });
    for e in entries {
        if dirs.len() + files.len() >= LIST_CAP {
            truncated = true;
            break;
        }
        let p = e.path();
        let ft = e.file_type().ok();
        if ft.map(|t| t.is_dir()).unwrap_or(false) || p.is_dir() {
            dirs.push(p);
        } else {
            let sz = e.metadata().map(|m| m.len()).unwrap_or(0);
            files.push((p, sz));
        }
    }
    Ok(DirListing {
        path: display,
        parent,
        dirs,
        files,
        truncated,
    })
}

pub fn format_drives(drives: &[PathBuf]) -> String {
    if drives.is_empty() {
        return "Drives: (none)".into();
    }
    let mut s = String::from("Drives:\n");
    for d in drives {
        s.push_str(&format!("  {}\n", d.display()));
    }
    s
}

pub fn format_listing(list: &DirListing) -> String {
    let mut s = format!("Path: {}\n", list.path);
    if let Some(p) = &list.parent {
        s.push_str(&format!("Parent: {}\n", p.display()));
    }
    s.push('\n');
    for d in &list.dirs {
        let name = d.file_name().unwrap_or_default().to_string_lossy();
        s.push_str(&format!("[dir]  {name}\n"));
    }
    for (f, sz) in &list.files {
        let name = f.file_name().unwrap_or_default().to_string_lossy();
        s.push_str(&format!("[file] {name}  {}\n", format_size(*sz)));
    }
    if list.truncated {
        s.push_str("\n(truncated)\n");
    }
    if s.len() > 3500 {
        s.truncate(3500);
        s.push_str("\n…");
    }
    s
}

pub fn format_size(n: u64) -> String {
    if n >= 1024 * 1024 {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    } else if n >= 1024 {
        format!("{} KB", n / 1024)
    } else {
        format!("{n} B")
    }
}

pub fn read_file_capped(path: &Path) -> Result<(Vec<u8>, u64)> {
    let meta = fs::metadata(path)?;
    if meta.is_dir() {
        return Err(anyhow!("path is a folder"));
    }
    let n = meta.len();
    if n > TG_MAX_DOC {
        return Err(anyhow!("file is {} (Telegram max 50 MB)", format_size(n)));
    }
    let bytes = fs::read(path)?;
    Ok((bytes, n))
}

pub fn copy_any(src: &Path, dest: &Path) -> Result<u64> {
    if let Some(msg) = refuse_write_dest(dest) {
        return Err(anyhow!("{msg}"));
    }
    copy_recursive(src, dest).map_err(|e| anyhow!("{} -> {}: {e}", src.display(), dest.display()))
}

fn copy_recursive(src: &Path, dest: &Path) -> io::Result<u64> {
    if src.is_dir() {
        fs::create_dir_all(dest)?;
        let mut total = 0u64;
        for e in fs::read_dir(src)? {
            let e = e?;
            total += copy_recursive(&e.path(), &dest.join(e.file_name()))?;
        }
        Ok(total)
    } else {
        let dest = if dest.is_dir() {
            dest.join(src.file_name().unwrap_or_default())
        } else {
            dest.to_path_buf()
        };
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dest)
    }
}

pub fn delete_any(path: &Path) -> Result<u64> {
    if let Some(msg) = refuse_delete(path) {
        return Err(anyhow!("{msg}"));
    }
    let size = dir_size(path).unwrap_or(0);
    if path.is_dir() {
        fs::remove_dir_all(path).map_err(|e| anyhow!("{}: {e}", path.display()))?;
    } else {
        fs::remove_file(path).map_err(|e| anyhow!("{}: {e}", path.display()))?;
    }
    Ok(size)
}

fn dir_size(path: &Path) -> io::Result<u64> {
    if path.is_file() {
        return Ok(fs::metadata(path)?.len());
    }
    let mut n = 0u64;
    if let Ok(rd) = fs::read_dir(path) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                n = n.saturating_add(dir_size(&p).unwrap_or(0));
            } else {
                n = n.saturating_add(e.metadata().map(|m| m.len()).unwrap_or(0));
            }
        }
    }
    Ok(n)
}

pub fn format_programs(rows: &[InstalledProgram]) -> String {
    let mut s = String::from("Installed programs\n\n");
    for p in rows.iter().take(LIST_CAP) {
        if p.publisher.is_empty() {
            s.push_str(&format!("• {}\n", p.name));
        } else {
            s.push_str(&format!("• {} ({})\n", p.name, p.publisher));
        }
    }
    if rows.len() > LIST_CAP {
        s.push_str("\n(truncated)\n");
    }
    if s.len() > 3500 {
        s.truncate(3500);
        s.push_str("\n…");
    }
    s
}

pub fn find_program(display: &str) -> Option<InstalledProgram> {
    let want = display.trim().to_ascii_lowercase();
    let rows = list_programs();
    rows.iter()
        .find(|p| p.name.to_ascii_lowercase() == want)
        .cloned()
        .or_else(|| {
            rows.into_iter()
                .find(|p| p.name.to_ascii_lowercase().contains(&want))
        })
}

pub fn list_programs() -> Vec<InstalledProgram> {
    #[cfg(windows)]
    {
        list_programs_win()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

#[cfg(windows)]
fn list_programs_win() -> Vec<InstalledProgram> {
    use windows::Win32::System::Registry::{KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY};

    let mut rows = Vec::new();
    let keys = [
        (
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            KEY_READ.0,
        ),
        (
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            KEY_READ.0 | KEY_WOW64_64KEY.0,
        ),
        (
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            KEY_READ.0 | KEY_WOW64_32KEY.0,
        ),
        (
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
            KEY_READ.0,
        ),
    ];
    for (path, access) in keys {
        enum_uninstall(path, access, &mut rows);
    }
    rows.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });
    rows.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    rows
}

#[cfg(windows)]
fn enum_uninstall(subkey: &str, access: u32, out: &mut Vec<InstalledProgram>) {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, HKEY_LOCAL_MACHINE, REG_SAM_FLAGS,
    };

    let wide: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
    let mut hkey = Default::default();
    unsafe {
        if RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(wide.as_ptr()),
            0,
            REG_SAM_FLAGS(access),
            &mut hkey,
        )
        .is_err()
        {
            return;
        }
    }
    for i in 0..4096u32 {
        let mut name = [0u16; 256];
        let mut nlen = name.len() as u32;
        let st = unsafe {
            RegEnumKeyExW(
                hkey,
                i,
                windows::core::PWSTR(name.as_mut_ptr()),
                &mut nlen,
                None,
                windows::core::PWSTR::null(),
                None,
                None,
            )
        };
        if st.is_err() {
            break;
        }
        let key_name = String::from_utf16_lossy(&name[..nlen as usize]);
        let child_path = format!("{subkey}\\{key_name}");
        let child_wide: Vec<u16> = child_path
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut child = Default::default();
        unsafe {
            if RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                PCWSTR(child_wide.as_ptr()),
                0,
                REG_SAM_FLAGS(access),
                &mut child,
            )
            .is_err()
            {
                continue;
            }
        }
        let display = reg_sz(child, "DisplayName");
        if display.is_empty() {
            unsafe {
                let _ = RegCloseKey(child);
            }
            continue;
        }
        let publisher = reg_sz(child, "Publisher");
        let uninstall = reg_sz(child, "UninstallString");
        let quiet = reg_sz(child, "QuietUninstallString");
        let product_code = if key_name.starts_with('{') {
            key_name.clone()
        } else {
            String::new()
        };
        unsafe {
            let _ = RegCloseKey(child);
        }
        out.push(InstalledProgram {
            name: display,
            publisher,
            uninstall,
            quiet,
            product_code,
        });
    }
    unsafe {
        let _ = RegCloseKey(hkey);
    }
}

#[cfg(windows)]
fn reg_sz(hkey: windows::Win32::System::Registry::HKEY, name: &str) -> String {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{RegQueryValueExW, REG_SZ};
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut buf = [0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    let mut ty = REG_SZ;
    let ok = unsafe {
        RegQueryValueExW(
            hkey,
            PCWSTR(wide.as_ptr()),
            None,
            Some(&mut ty),
            Some(buf.as_mut_ptr() as *mut u8),
            Some(&mut len),
        )
    };
    if ok.is_err() || len < 2 {
        return String::new();
    }
    let n = (len as usize / 2).saturating_sub(1).min(buf.len());
    String::from_utf16_lossy(&buf[..n])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_delete_rejected() {
        assert!(refuse_delete(Path::new(r"C:\Windows")).is_some());
        assert!(refuse_delete(Path::new(r"C:\Windows\System32")).is_some());
        assert!(refuse_delete(Path::new(r"C:\")).is_some());
        assert!(refuse_delete(Path::new(r"D:\")).is_some());
        assert!(refuse_delete(&paths::program_files()).is_some());
        assert!(refuse_delete(Path::new(r"C:\Temp\old")).is_none());
        assert!(refuse_write_dest(Path::new(r"C:\Windows\System32\foo")).is_some());
        assert!(is_ftracker_program("Ftracker"));
        assert!(is_ftracker_program("Ftracker 1.0"));
        assert!(!is_ftracker_program("Google Chrome"));
    }
}
