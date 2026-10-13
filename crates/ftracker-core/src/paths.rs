use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const DIR_NAME: &str = "Ftracker";

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn program_data() -> Result<PathBuf> {
    if let Some(d) = DATA_DIR.get() {
        return Ok(d.clone());
    }
    let chosen = pick_data_dir()?;
    match DATA_DIR.set(chosen.clone()) {
        Ok(()) => Ok(chosen),
        Err(_) => Ok(DATA_DIR.get().cloned().unwrap_or(chosen)),
    }
}

fn preferred_program_data() -> PathBuf {
    let base = std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".into());
    PathBuf::from(base).join(DIR_NAME)
}

fn local_data() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .or_else(|_| std::env::var("APPDATA"))
        .unwrap_or_else(|_| r"C:\ProgramData".into());
    PathBuf::from(base).join(DIR_NAME)
}

fn file_readable_or_missing(path: &Path) -> bool {
    if !path.exists() {
        return true;
    }
    fs::read(path).is_ok()
}

fn probe_write(dir: &Path) -> bool {
    let probe = dir.join(".w");
    match fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

fn pick_data_dir() -> Result<PathBuf> {
    let preferred = preferred_program_data();
    let _ = fs::create_dir_all(&preferred);
    apply_admin_acl(&preferred)?;
    let secrets = preferred.join("secrets.bin");
    if secrets.exists() {
        apply_admin_acl(&secrets)?;
    }
    let enc = preferred.join("ftracker.enc");
    if enc.exists() {
        apply_admin_acl(&enc)?;
    }
    if probe_write(&preferred) && file_readable_or_missing(&secrets) {
        return Ok(preferred);
    }
    let local = local_data();
    fs::create_dir_all(&local).with_context(|| format!("create {}", local.display()))?;
    Ok(local)
}

pub fn program_files() -> PathBuf {
    let base = std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".into());
    PathBuf::from(base).join(DIR_NAME)
}

pub fn session_exe() -> PathBuf {
    program_files().join("ftracker.exe")
}

pub fn service_exe() -> PathBuf {
    program_files().join("ftracker-svc.exe")
}

pub fn secrets_blob() -> Result<PathBuf> {
    Ok(program_data()?.join("secrets.bin"))
}

pub fn db_enc_path() -> Result<PathBuf> {
    Ok(program_data()?.join("ftracker.enc"))
}

pub fn payload_path() -> Result<PathBuf> {
    Ok(program_data()?.join("preassign.json"))
}

pub fn telegram_offset_path() -> Result<PathBuf> {
    Ok(program_data()?.join("tg_offset.json"))
}

pub fn mutex_name() -> &'static str {
    "Local\\FtrackerSessionMutex"
}

pub fn this_exe() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| session_exe())
}

pub fn sibling(file_name: &str) -> PathBuf {
    this_exe()
        .parent()
        .map(|p| p.join(file_name))
        .unwrap_or_else(|| program_files().join(file_name))
}

pub fn product_code_reg_key() -> &'static str {
    r"Software\Ftracker"
}

pub fn ensure_data_dir() -> Result<PathBuf> {
    program_data()
}

fn current_account() -> String {
    let user = std::env::var("USERNAME").unwrap_or_default();
    if user.is_empty() {
        return String::new();
    }
    match std::env::var("USERDOMAIN") {
        Ok(d) if !d.is_empty() && !d.eq_ignore_ascii_case(&user) => format!("{d}\\{user}"),
        _ => user,
    }
}

/// SYSTEM + Administrators full, current user modify. Never deny Users — that ACE
/// also blocks the logged-on session (deny wins over Administrators).
///
/// `(OI)(CI)` is directory-only. On files those grants are ignored and `/inheritance:r`
/// can leave an empty DACL (Access is denied).
pub fn apply_admin_acl(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        let path_str = path.to_string_lossy().to_string();
        let dir = path.is_dir();
        let sys = if dir { "SYSTEM:(OI)(CI)F" } else { "SYSTEM:F" };
        let admins = if dir {
            "Administrators:(OI)(CI)F"
        } else {
            "Administrators:F"
        };
        let _ = std::process::Command::new("icacls")
            .args([&path_str, "/remove:d", "Users"])
            .output();
        let mut args = vec![
            path_str.clone(),
            "/inheritance:r".into(),
            "/grant:r".into(),
            sys.into(),
            "/grant:r".into(),
            admins.into(),
        ];
        let account = current_account();
        if !account.is_empty() {
            args.push("/grant:r".into());
            if dir {
                args.push(format!("{account}:(OI)(CI)M"));
            } else {
                args.push(format!("{account}:M"));
            }
        }
        let _ = std::process::Command::new("icacls").args(&args).output();
    }
    Ok(())
}

pub fn is_access_denied(err: &impl std::fmt::Display) -> bool {
    let s = err.to_string();
    s.contains("os error 5") || s.contains("Access is denied")
}

fn norm_admin_path(path: &Path) -> String {
    let s = path.to_string_lossy().replace('/', "\\");
    s.trim().trim_end_matches(['\\', '/']).to_ascii_lowercase()
}

/// Drive root like `C:` or `C:\` — recursive delete would wipe the volume.
pub fn is_drive_root(path: &Path) -> bool {
    let n = norm_admin_path(path);
    n.len() == 2 && n.as_bytes().get(1) == Some(&b':')
}

/// Hard-refuse Telegram delete/overwrite of Windows and the Ftracker install/data dirs.
pub fn is_protected_admin_path(path: &Path) -> bool {
    let n = norm_admin_path(path);
    if n.is_empty() {
        return true;
    }
    let mut roots = vec![
        r"c:\windows".to_string(),
        r"c:\windows\system32".to_string(),
        r"c:\winnt".to_string(),
    ];
    if let Ok(sr) = std::env::var("SystemRoot") {
        let r = norm_admin_path(Path::new(&sr));
        if !r.is_empty() {
            roots.push(r);
        }
    }
    for r in &roots {
        if n == *r || n.starts_with(&format!("{r}\\")) {
            return true;
        }
    }
    let install = norm_admin_path(&program_files());
    if !install.is_empty() && (n == install || n.starts_with(&format!("{install}\\"))) {
        return true;
    }
    if let Some(parent) = this_exe().parent() {
        let e = norm_admin_path(parent);
        if !e.is_empty() && (n == e || n.starts_with(&format!("{e}\\"))) {
            return true;
        }
    }
    if let Ok(data) = program_data() {
        let d = norm_admin_path(&data);
        if !d.is_empty() && (n == d || n.starts_with(&format!("{d}\\"))) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod admin_path_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn refuses_windows_and_ftracker() {
        assert!(is_protected_admin_path(Path::new(r"C:\Windows")));
        assert!(is_protected_admin_path(Path::new(r"C:\Windows\System32")));
        assert!(is_protected_admin_path(Path::new(
            r"C:\Windows\System32\drivers"
        )));
        assert!(is_protected_admin_path(&program_files()));
        assert!(is_protected_admin_path(
            &program_files().join("ftracker.exe")
        ));
        assert!(!is_protected_admin_path(Path::new(r"C:\Users")));
        assert!(!is_protected_admin_path(Path::new(r"C:\Temp\old")));
        assert!(is_drive_root(Path::new(r"C:\")));
        assert!(is_drive_root(Path::new(r"D:")));
        assert!(!is_drive_root(Path::new(r"C:\Users")));
    }
}
