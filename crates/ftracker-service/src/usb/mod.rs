pub mod eject;
pub mod watch;

use std::collections::HashSet;
use std::fs::File;
use std::sync::{Arc, Mutex, OnceLock};

use ftracker_core::config::AppConfig;
use ftracker_core::totp;
use ftracker_ipc::pipe;
use ftracker_ipc::protocol::IpcMsg;

static ALLOW: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
static PENDING: OnceLock<Mutex<Option<PendingUsb>>> = OnceLock::new();
static PIPE: OnceLock<Mutex<Option<Arc<Mutex<File>>>>> = OnceLock::new();
static NOTIFIED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

#[derive(Clone)]
pub struct PendingUsb {
    pub letter: String,
    pub name: String,
    pub serial: String,
    pub vidpid: String,
}

fn allow() -> &'static Mutex<HashSet<String>> {
    ALLOW.get_or_init(|| Mutex::new(HashSet::new()))
}
fn pending() -> &'static Mutex<Option<PendingUsb>> {
    PENDING.get_or_init(|| Mutex::new(None))
}
fn pipe_slot() -> &'static Mutex<Option<Arc<Mutex<File>>>> {
    PIPE.get_or_init(|| Mutex::new(None))
}
fn notified() -> &'static Mutex<HashSet<String>> {
    NOTIFIED.get_or_init(|| Mutex::new(HashSet::new()))
}

pub fn set_session_pipe(p: Arc<Mutex<File>>) {
    *pipe_slot().lock().unwrap() = Some(p);
}
pub fn clear_session_pipe() {
    *pipe_slot().lock().unwrap() = None;
}

pub fn is_allowed(serial: &str) -> bool {
    allow().lock().ok().map(|s| s.contains(serial)).unwrap_or(false)
}

pub fn allow_serial(serial: &str) {
    if let Ok(mut s) = allow().lock() {
        s.insert(serial.to_string());
    }
}

pub fn drop_serial(serial: &str) {
    if let Ok(mut s) = allow().lock() {
        s.remove(serial);
    }
    if let Ok(mut n) = notified().lock() {
        n.remove(serial);
    }
    eject::stop_eject(serial);
}

pub fn on_plugin(dev: PendingUsb) {
    if is_allowed(&dev.serial) {
        return;
    }
    *pending().lock().unwrap() = Some(dev.clone());
    eject::keep_ejected(&dev.letter, &dev.serial);
    let first = notified()
        .lock()
        .map(|mut s| s.insert(dev.serial.clone()))
        .unwrap_or(false);
    if !first {
        return;
    }
    eprintln!(
        "ftracker-svc: USB blocked {} {} — waiting for TOTP",
        dev.letter, dev.name
    );
    if let Some(p) = pipe_slot().lock().unwrap().clone() {
        if let Ok(mut f) = p.lock() {
            let _ = pipe::write_msg(
                &mut *f,
                &IpcMsg::UsbBlocked {
                    letter: dev.letter.clone(),
                    name: dev.name.clone(),
                    serial: dev.serial.clone(),
                    vidpid: dev.vidpid.clone(),
                },
            );
        }
    }
}

pub fn on_removed(serial: &str, letter: &str) {
    drop_serial(serial);
    let clear = pending()
        .lock()
        .ok()
        .and_then(|p| p.as_ref().map(|d| d.serial == serial))
        .unwrap_or(false);
    if clear {
        *pending().lock().unwrap() = None;
    }
    if let Ok(cfg) = AppConfig::load() {
        let client = ftracker_core::telegram::client::TelegramClient::new(cfg.telegram_bot_token);
        let text = ftracker_core::telegram::messages::usb(
            "USB_REMOVED",
            &ftracker_core::config::windows_user(),
            &cfg.machine_id,
            letter,
            "",
            serial,
            "",
            "",
            None,
        );
        let _ = client.send_admin(&cfg.telegram_admin_chat_id, &text, None);
    }
}

pub fn on_totp_ok(code: &str) {
    let cfg = match AppConfig::load() {
        Ok(c) => c,
        Err(_) => return,
    };
    if !totp::verify(&cfg.totp_secret_b32, code, chrono::Local::now().timestamp()) {
        on_totp_fail();
        return;
    }
    let serials: Vec<String> = notified()
        .lock()
        .map(|s| s.iter().cloned().collect())
        .unwrap_or_default();
    for s in serials {
        allow_serial(&s);
    }
    *pending().lock().unwrap() = None;
}

pub fn on_totp_fail() {}

pub fn run_watch() {
    watch::run();
}

#[allow(dead_code)]
pub fn current_pending() -> Option<PendingUsb> {
    pending().lock().ok().and_then(|p| p.clone())
}
