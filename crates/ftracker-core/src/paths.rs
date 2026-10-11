use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub const DIR_NAME: &str = "Ftracker";

pub fn program_data() -> Result<PathBuf> {
    let base = std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".into());
    Ok(PathBuf::from(base).join(DIR_NAME))
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
    let dir = program_data()?;
    fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    apply_admin_acl(&dir)?;
    Ok(dir)
}

/// SYSTEM + Administrators full; Users deny list/read (best-effort).
pub fn apply_admin_acl(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        let path_str = path.to_string_lossy().to_string();
        let _ = std::process::Command::new("icacls")
            .args([
                &path_str,
                "/inheritance:r",
                "/grant",
                "SYSTEM:(OI)(CI)F",
                "/grant",
                "Administrators:(OI)(CI)F",
                "/deny",
                "Users:(OI)(CI)(R,W,X,D,DC)",
            ])
            .output();
    }
    Ok(())
}
