use anyhow::Result;
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Authenticode {
    Valid,
    Unsigned,
    Invalid,
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    let mut h = Sha256::new();
    h.update(&bytes);
    Ok(hex::encode(h.finalize()))
}

pub fn sha256_file_or_err(path: &Path) -> Result<String, String> {
    if !path.exists() {
        return Err("missing".into());
    }
    sha256_file(path).map_err(|e| format!("unreadable:{e}"))
}

/// Best-effort Authenticode. Unsigned is allowed (dev). Invalid after a signed PE is a fail.
pub fn authenticode_status(path: &Path) -> Authenticode {
    #[cfg(windows)]
    {
        return authenticode_windows(path);
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Authenticode::Unsigned
    }
}

#[cfg(windows)]
fn authenticode_windows(path: &Path) -> Authenticode {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, INVALID_HANDLE_VALUE};
    use windows::Win32::Security::WinTrust::{
        WinVerifyTrust, WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_FILE_INFO,
        WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY,
        WTD_UI_NONE,
    };

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut file_info = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(wide.as_ptr()),
        hFile: HANDLE::default(),
        pgKnownSubject: std::ptr::null_mut(),
    };
    let mut data = WINTRUST_DATA::default();
    data.cbStruct = std::mem::size_of::<WINTRUST_DATA>() as u32;
    data.dwUIChoice = WTD_UI_NONE;
    data.fdwRevocationChecks = WTD_REVOKE_NONE;
    data.dwUnionChoice = WTD_CHOICE_FILE;
    data.Anonymous.pFile = &mut file_info;
    data.dwStateAction = WTD_STATEACTION_VERIFY;
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let status = unsafe {
        WinVerifyTrust(
            HWND(0),
            &mut action,
            &mut data as *mut _ as *mut core::ffi::c_void,
        )
    };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    unsafe {
        let _ = WinVerifyTrust(
            HWND(0),
            &mut action,
            &mut data as *mut _ as *mut core::ffi::c_void,
        );
        if file_info.hFile != HANDLE::default() && file_info.hFile != INVALID_HANDLE_VALUE {
            let _ = CloseHandle(file_info.hFile);
        }
    }
    // 0 = trusted; TRUST_E_NOSIGNATURE = 0x800B0100
    match status {
        0 => Authenticode::Valid,
        -2146762496 => Authenticode::Unsigned, // TRUST_E_NOSIGNATURE
        _ => {
            if status == 0x800B0100u32 as i32 {
                Authenticode::Unsigned
            } else {
                Authenticode::Invalid
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct IntegrityFinding {
    pub file: String,
    pub reason: String,
    pub expected: String,
    pub got: String,
}

/// MSI writes `rehash.flag` beside the exe. Refresh stored hashes so an
/// intended upgrade is not reported as tampering.
pub fn refresh_hashes_if_msi_replaced(cfg: &mut crate::config::AppConfig) {
    let flag = crate::paths::sibling("rehash.flag");
    if !flag.exists() {
        return;
    }
    let this = crate::paths::this_exe();
    let session = if this
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .eq_ignore_ascii_case("ftracker.exe")
    {
        this
    } else {
        crate::paths::session_exe()
    };
    let service = crate::paths::sibling("ftracker-svc.exe");
    let a = sha256_file(&session).unwrap_or_default();
    let b = if service.exists() {
        sha256_file(&service).unwrap_or_default()
    } else {
        a.clone()
    };
    if a.is_empty() {
        return;
    }
    cfg.session_sha256 = a;
    cfg.service_sha256 = b;
    let _ = cfg.save();
    let _ = std::fs::remove_file(&flag);
}

/// Cross-check the peer exe against the hash stored at enroll.
pub fn check_peer(path: &Path, expected_sha: &str, file_label: &str) -> Option<IntegrityFinding> {
    if expected_sha.is_empty() {
        return None;
    }
    match sha256_file_or_err(path) {
        Ok(got) => {
            if got.eq_ignore_ascii_case(expected_sha) {
                match authenticode_status(path) {
                    Authenticode::Invalid => Some(IntegrityFinding {
                        file: file_label.into(),
                        reason: "authenticode_invalid".into(),
                        expected: expected_sha.into(),
                        got,
                    }),
                    _ => None,
                }
            } else {
                Some(IntegrityFinding {
                    file: file_label.into(),
                    reason: "hash_mismatch".into(),
                    expected: expected_sha.into(),
                    got,
                })
            }
        }
        Err(reason) => Some(IntegrityFinding {
            file: file_label.into(),
            reason,
            expected: expected_sha.into(),
            got: String::new(),
        }),
    }
}

fn alert_latch_path() -> Option<std::path::PathBuf> {
    crate::paths::program_data()
        .ok()
        .map(|d| d.join("integrity-sent.txt"))
}

fn finding_key(find: &IntegrityFinding) -> String {
    format!(
        "{}|{}|{}|{}",
        find.file, find.reason, find.expected, find.got
    )
}

/// Telegram: one message per distinct mismatch. Not every minute.
pub fn should_send_alert(find: &IntegrityFinding) -> bool {
    let key = finding_key(find);
    let Some(path) = alert_latch_path() else {
        return true;
    };
    if let Ok(prev) = std::fs::read_to_string(&path) {
        if prev.trim() == key {
            return false;
        }
    }
    let _ = std::fs::write(&path, &key);
    true
}

pub fn clear_alert_latch() {
    if let Some(path) = alert_latch_path() {
        let _ = std::fs::remove_file(path);
    }
}
