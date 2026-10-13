use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use super::is_allowed;

fn ejecting() -> &'static Mutex<HashSet<String>> {
    static EJECTING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    EJECTING.get_or_init(|| Mutex::new(HashSet::new()))
}

fn letters() -> &'static Mutex<HashMap<String, String>> {
    static LETTERS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    LETTERS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(windows)]
pub fn eject_letter(letter: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::Ioctl::IOCTL_STORAGE_EJECT_MEDIA;
    use windows::Win32::System::IO::DeviceIoControl;

    let letter = letter.chars().next().unwrap_or('E').to_ascii_uppercase();
    let path: Vec<u16> = format!("\\\\.\\{letter}:\0").encode_utf16().collect();
    let shares = FILE_SHARE_READ | FILE_SHARE_WRITE;
    let accesses = [GENERIC_READ.0 | GENERIC_WRITE.0, GENERIC_READ.0];
    unsafe {
        for access in accesses {
            let h = CreateFileW(
                PCWSTR(path.as_ptr()),
                access,
                shares,
                None,
                OPEN_EXISTING,
                Default::default(),
                None,
            );
            let Ok(h) = h else { continue };
            const FSCTL_DISMOUNT_VOLUME: u32 = 0x00090020;
            const FSCTL_LOCK_VOLUME: u32 = 0x00090018;
            let mut ret = 0u32;
            let _ = DeviceIoControl(h, FSCTL_LOCK_VOLUME, None, 0, None, 0, Some(&mut ret), None);
            let _ = DeviceIoControl(
                h,
                FSCTL_DISMOUNT_VOLUME,
                None,
                0,
                None,
                0,
                Some(&mut ret),
                None,
            );
            let _ = DeviceIoControl(
                h,
                IOCTL_STORAGE_EJECT_MEDIA,
                None,
                0,
                None,
                0,
                Some(&mut ret),
                None,
            );
            let _ = CloseHandle(h);
            return;
        }
    }
}

#[cfg(not(windows))]
pub fn eject_letter(_letter: &str) {}

pub fn keep_ejected(letter: &str, serial: &str) {
    if serial.is_empty() {
        return;
    }
    if let Ok(mut map) = letters().lock() {
        map.insert(serial.to_string(), letter.to_string());
    }
    let mut set = match ejecting().lock() {
        Ok(s) => s,
        Err(_) => return,
    };
    if !set.insert(serial.to_string()) {
        return;
    }
    drop(set);
    let serial = serial.to_string();
    thread::spawn(move || {
        loop {
            if is_allowed(&serial) {
                break;
            }
            let letter = letters()
                .lock()
                .ok()
                .and_then(|m| m.get(&serial).cloned())
                .unwrap_or_default();
            if letter.is_empty() {
                break;
            }
            eject_letter(&letter);
            thread::sleep(Duration::from_millis(300));
        }
        if let Ok(mut s) = ejecting().lock() {
            s.remove(&serial);
        }
        if let Ok(mut m) = letters().lock() {
            m.remove(&serial);
        }
    });
}

pub fn stop_eject(serial: &str) {
    if let Ok(mut m) = letters().lock() {
        m.remove(serial);
    }
}
