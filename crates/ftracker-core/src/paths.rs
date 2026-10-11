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
    if probe_write(&preferred) {
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
pub fn apply_admin_acl(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        let path_str = path.to_string_lossy().to_string();
        let _ = std::process::Command::new("icacls")
            .args([&path_str, "/remove:d", "Users"])
            .output();
        let mut args = vec![
            path_str.clone(),
            "/inheritance:r".into(),
            "/grant:r".into(),
            "SYSTEM:(OI)(CI)F".into(),
            "/grant:r".into(),
            "Administrators:(OI)(CI)F".into(),
        ];
        let account = current_account();
        if !account.is_empty() {
            args.push("/grant:r".into());
            args.push(format!("{account}:(OI)(CI)M"));
        }
        let _ = std::process::Command::new("icacls").args(&args).output();
    }
    Ok(())
}
