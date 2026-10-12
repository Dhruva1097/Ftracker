//! Grant Ftracker’s needed OS access at process start, then show the official
//! Windows prompts for anything still gated (location).

use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(windows)]
use std::thread;

static DID: AtomicBool = AtomicBool::new(false);

/// Default-allow location for this user, start the OS location service if
/// possible, then ask Windows for location access (modal). Safe to call twice.
pub fn ensure_at_start() {
    if DID.swap(true, Ordering::SeqCst) {
        return;
    }
    #[cfg(windows)]
    {
        grant_user_defaults();
        start_lfsvc();
        let _ = thread::Builder::new().name("ft-perm".into()).spawn(|| {
            crate::location::ask_os();
            ask_winrt();
        });
    }
}

#[cfg(windows)]
fn grant_user_defaults() {
    use windows::Win32::System::Registry::HKEY_CURRENT_USER;

    set_sz(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\location",
        "Value",
        "Allow",
    );
    set_sz(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\location\NonPackaged",
        "Value",
        "Allow",
    );
    if let Ok(exe) = std::env::current_exe() {
        let packed = exe
            .to_string_lossy()
            .replace(':', "#")
            .replace('\\', "#")
            .replace('/', "#");
        set_sz(
            HKEY_CURRENT_USER,
            &format!(
                r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\location\NonPackaged\{packed}"
            ),
            "Value",
            "Allow",
        );
    }
    set_sz(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\DeviceAccess\Global\{BFA794E4-F964-4FDB-90F6-51056BFE4B44}",
        "Value",
        "Allow",
    );
    set_dword(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows NT\CurrentVersion\Sensor\Permissions\{BFA794E4-F964-4FDB-90F6-51056BFE4B44}",
        "Enable",
        1,
    );
}

#[cfg(windows)]
fn start_lfsvc() {
    use windows::core::{w, PCWSTR};
    use windows::Win32::System::Services::{
        CloseServiceHandle, OpenSCManagerW, OpenServiceW, StartServiceW, SC_MANAGER_CONNECT,
        SERVICE_START,
    };
    unsafe {
        let scm = match OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT) {
            Ok(h) => h,
            Err(_) => return,
        };
        if let Ok(svc) = OpenServiceW(scm, w!("lfsvc"), SERVICE_START) {
            let _ = StartServiceW(svc, None);
            let _ = CloseServiceHandle(svc);
        }
        let _ = CloseServiceHandle(scm);
    }
}

#[cfg(windows)]
fn ask_winrt() {
    use windows::Devices::Geolocation::Geolocator;
    if let Ok(op) = Geolocator::RequestAccessAsync() {
        let _ = op.get();
    }
    if let Ok(geo) = Geolocator::new() {
        let _ = geo.AllowFallbackToConsentlessPositions();
    }
}

#[cfg(windows)]
fn set_sz(
    root: windows::Win32::System::Registry::HKEY,
    path: &str,
    name: &str,
    value: &str,
) {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegSetValueExW, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
    };
    let path_w = wide(path);
    let name_w = wide(name);
    let data = sz_bytes(value);
    unsafe {
        let mut hkey = Default::default();
        if RegCreateKeyExW(
            root,
            PCWSTR(path_w.as_ptr()),
            0,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        )
        .is_err()
        {
            return;
        }
        let _ = RegSetValueExW(hkey, PCWSTR(name_w.as_ptr()), 0, REG_SZ, Some(&data));
        let _ = RegCloseKey(hkey);
    }
}

#[cfg(windows)]
fn set_dword(
    root: windows::Win32::System::Registry::HKEY,
    path: &str,
    name: &str,
    value: u32,
) {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegSetValueExW, KEY_WRITE, REG_DWORD, REG_OPTION_NON_VOLATILE,
    };
    let path_w = wide(path);
    let name_w = wide(name);
    let data = value.to_le_bytes();
    unsafe {
        let mut hkey = Default::default();
        if RegCreateKeyExW(
            root,
            PCWSTR(path_w.as_ptr()),
            0,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        )
        .is_err()
        {
            return;
        }
        let _ = RegSetValueExW(hkey, PCWSTR(name_w.as_ptr()), 0, REG_DWORD, Some(&data));
        let _ = RegCloseKey(hkey);
    }
}

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn sz_bytes(s: &str) -> Vec<u8> {
    wide(s)
        .into_iter()
        .flat_map(|u| u.to_le_bytes())
        .collect()
}
