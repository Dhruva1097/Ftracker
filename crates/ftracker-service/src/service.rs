use std::ffi::OsString;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use ftracker_core::config::AppConfig;
use ftracker_core::integrity;
use ftracker_core::paths;
use ftracker_core::telegram::client::TelegramClient;
use ftracker_core::telegram::messages;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};

use crate::ipc_server;
use crate::usb;

const SVC: &str = "FtrackerSvc";

pub fn run_as_service() -> Result<()> {
    let (tx, rx) = mpsc::channel();
    let status = service_control_handler::register(SVC, move |event| match event {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            let _ = tx.send(());
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    })?;
    status.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::from_millis(0),
        process_id: None,
    })?;
    let worker = thread::spawn(run_worker);
    let _ = rx.recv();
    let _ = worker;
    status.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::from_millis(0),
        process_id: None,
    })?;
    Ok(())
}

pub fn run_worker() {
    enable_location_platform();
    integrity_loop_start();
    let ipc = thread::spawn(ipc_server::run);
    usb::run_watch();
    let _ = ipc.join();
}

/// Machine-wide location defaults (SYSTEM). User consent still happens in the session.
fn enable_location_platform() {
    #[cfg(windows)]
    {
        use windows::core::{w, PCWSTR};
        use windows::Win32::System::Registry::HKEY_LOCAL_MACHINE;
        use windows::Win32::System::Services::{
            CloseServiceHandle, OpenSCManagerW, OpenServiceW, StartServiceW, SC_MANAGER_CONNECT,
            SERVICE_START,
        };
        set_sz(
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\location",
            "Value",
            "Allow",
        );
        set_sz(
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\location\NonPackaged",
            "Value",
            "Allow",
        );
        set_dword(
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Sensor\Overrides\{BFA794E4-F964-4FDB-90F6-51056BFE4B44}",
            "SensorPermissionState",
            1,
        );
        set_dword(
            HKEY_LOCAL_MACHINE,
            r"SYSTEM\CurrentControlSet\Services\lfsvc\Parameters",
            "DisableStatus",
            0,
        );
        unsafe {
            if let Ok(scm) = OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT) {
                if let Ok(svc) = OpenServiceW(scm, w!("lfsvc"), SERVICE_START) {
                    let _ = StartServiceW(svc, None);
                    let _ = CloseServiceHandle(svc);
                }
                let _ = CloseServiceHandle(scm);
            }
        }
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
    let path_w: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let name_w: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let data: Vec<u8> = path_w_to_sz(value);
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
    let path_w: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let name_w: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
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
fn path_w_to_sz(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(|u| u.to_le_bytes())
        .collect()
}

fn integrity_loop_start() {
    thread::spawn(|| loop {
        check_session_integrity();
        maybe_outbox();
        thread::sleep(Duration::from_secs(60));
    });
}

fn check_session_integrity() {
    let cfg = match AppConfig::load() {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("{e:#}");
            if msg.contains("SECRETS_CORRUPT") || msg.contains("CryptUnprotect") {
                if let Ok(cfg2) = AppConfig::load() {
                    let client = TelegramClient::new(cfg2.telegram_bot_token);
                    let _ = client.send_admin(
                        &cfg2.telegram_admin_chat_id,
                        &messages::secrets_corrupt(&cfg2.machine_id, None),
                        None,
                    );
                }
            }
            return;
        }
    };
    let path = paths::sibling("ftracker.exe");
    if let Some(find) = integrity::check_peer(&path, &cfg.session_sha256, "ftracker.exe") {
        let client = TelegramClient::new(cfg.telegram_bot_token.clone());
        let text = messages::integrity_fail(
            &cfg.employee_name,
            &cfg.machine_id,
            &find.file,
            &find.reason,
            &find.expected,
            &find.got,
            None,
        );
        let _ = client.send_admin(&cfg.telegram_admin_chat_id, &text, None);
    }
}

fn maybe_outbox() {
    let cfg = match AppConfig::load() {
        Ok(c) if c.is_registered() => c,
        _ => return,
    };
    let store = match ftracker_core::Store::try_open(&cfg) {
        Ok(s) => s,
        Err(_) => return,
    };
    let client = TelegramClient::new(cfg.telegram_bot_token.clone());
    for (id, _kind, payload) in store.pending_telegram().unwrap_or_default() {
        let ok = if let Ok(q) = serde_json::from_str::<ftracker_core::telegram::QueuedSend>(&payload)
        {
            if q.employee_only {
                client.send_employee(&q.chat_id, &q.text).is_ok()
            } else {
                client
                    .send_admin(&q.chat_id, &q.text, q.loc.as_ref())
                    .is_ok()
            }
        } else {
            false
        };
        if ok {
            let _ = store.delete_telegram(id);
        }
    }
    for (hour, body) in store.unsent_hourly().unwrap_or_default() {
        let ok = client
            .send_admin(&cfg.telegram_admin_chat_id, &body, None)
            .is_ok();
        let _ = store.mark_hourly(&hour, &body, ok, "");
    }
    let _ = store.persist();
}

#[allow(dead_code)]
fn _os(s: &str) -> OsString {
    OsString::from(s)
}
