//! Privileged file copy/delete/stage and program uninstall as LocalSystem.

use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

use ftracker_core::paths;

pub fn execute(op: &str, path: &str, dest: &str) -> (bool, String) {
    match op {
        "delete" => match delete_any(Path::new(path)) {
            Ok(n) => (true, format!("deleted {} ({})", path, fmt_size(n))),
            Err(e) => (false, e),
        },
        "copy" => match copy_any(Path::new(path), Path::new(dest)) {
            Ok(n) => (true, format!("copied {} bytes", n)),
            Err(e) => (false, e),
        },
        "stage" => match stage_file(Path::new(path), Path::new(dest)) {
            Ok(n) => (true, format!("{n}")),
            Err(e) => (false, e),
        },
        "uninstall" => uninstall(path),
        _ => (false, format!("unknown op {op}")),
    }
}

fn refuse_delete(path: &Path) -> Option<&'static str> {
    if paths::is_drive_root(path) {
        return Some("refused: drive root");
    }
    if paths::is_protected_admin_path(path) {
        return Some("refused: Windows or Ftracker path");
    }
    None
}

fn delete_any(path: &Path) -> Result<u64, String> {
    if let Some(msg) = refuse_delete(path) {
        return Err(msg.into());
    }
    let size = dir_size(path).unwrap_or(0);
    let r = if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    r.map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(size)
}

fn copy_any(src: &Path, dest: &Path) -> Result<u64, String> {
    if paths::is_protected_admin_path(dest) {
        return Err("refused: Windows or Ftracker path".into());
    }
    copy_recursive(src, dest).map_err(|e| format!("{} -> {}: {e}", src.display(), dest.display()))
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

fn stage_file(src: &Path, dest: &Path) -> Result<u64, String> {
    if src.is_dir() {
        return Err("path is a folder".into());
    }
    let n = fs::metadata(src).map(|m| m.len()).unwrap_or(0);
    if n > 50 * 1024 * 1024 {
        return Err(format!("file is {} (Telegram max 50 MB)", fmt_size(n)));
    }
    if let Some(parent) = dest.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::copy(src, dest).map_err(|e| format!("stage: {e}"))?;
    grant_users_read(dest);
    Ok(n)
}

fn grant_users_read(path: &Path) {
    let s = path.to_string_lossy().to_string();
    let _ = Command::new("icacls")
        .args([&s, "/grant", "Users:R"])
        .output();
}

fn fmt_size(n: u64) -> String {
    if n >= 1024 * 1024 {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    } else if n >= 1024 {
        format!("{} KB", n / 1024)
    } else {
        format!("{n} B")
    }
}

fn is_ftracker_program(display: &str) -> bool {
    let k = display
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect::<String>();
    k == "ftracker" || k.starts_with("ftracker")
}

fn uninstall(display: &str) -> (bool, String) {
    if is_ftracker_program(display) {
        return (
            false,
            "refused: Ftracker cannot be uninstalled this way".into(),
        );
    }
    let Some(prog) = find_program(display) else {
        return (false, format!("program not found: {display}"));
    };
    if is_ftracker_program(&prog.name)
        || prog.uninstall.to_ascii_lowercase().contains("ftracker")
        || prog.quiet.to_ascii_lowercase().contains("ftracker")
    {
        return (
            false,
            "refused: Ftracker cannot be uninstalled this way".into(),
        );
    }
    let cmdline = if !prog.quiet.is_empty() {
        prog.quiet.clone()
    } else if !prog.product_code.is_empty() {
        format!("msiexec /x {} /qn /norestart", prog.product_code)
    } else if let Some(code) = msi_code_from_uninstall(&prog.uninstall) {
        format!("msiexec /x {code} /qn /norestart")
    } else if !prog.uninstall.is_empty() {
        prog.uninstall.clone()
    } else {
        return (false, "no uninstall command".into());
    };
    match run_hidden(&cmdline) {
        Ok(code) if code == 0 => (true, format!("uninstalled {}", prog.name)),
        Ok(code) => (false, format!("uninstall exit {code}: {cmdline}")),
        Err(e) => (false, e),
    }
}

#[derive(Clone)]
struct Prog {
    name: String,
    uninstall: String,
    quiet: String,
    product_code: String,
}

fn find_program(display: &str) -> Option<Prog> {
    let want = display.trim().to_ascii_lowercase();
    let all = list_programs();
    all.iter()
        .find(|p| p.name.to_ascii_lowercase() == want)
        .cloned()
        .or_else(|| {
            all.into_iter()
                .find(|p| p.name.to_ascii_lowercase().contains(&want))
        })
}

fn list_programs() -> Vec<Prog> {
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
fn list_programs_win() -> Vec<Prog> {
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
fn enum_uninstall(subkey: &str, access: u32, out: &mut Vec<Prog>) {
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
        out.push(Prog {
            name: display,
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

fn msi_code_from_uninstall(s: &str) -> Option<String> {
    let lower = s.to_ascii_lowercase();
    if !lower.contains("msiexec") {
        return None;
    }
    let bytes = s.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] == b'{' {
            if let Some(rel) = s[i..].find('}') {
                let code = &s[i..=i + rel];
                if code.len() > 10 {
                    return Some(code.to_string());
                }
            }
        }
    }
    None
}

fn run_hidden(cmdline: &str) -> Result<i32, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let status = Command::new("cmd")
            .args(["/C", cmdline])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map_err(|e| format!("spawn: {e}"))?;
        Ok(status.code().unwrap_or(-1))
    }
    #[cfg(not(windows))]
    {
        let _ = cmdline;
        Err("not windows".into())
    }
}
