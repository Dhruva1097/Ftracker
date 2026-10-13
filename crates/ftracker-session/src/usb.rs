use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use ftracker_ipc::protocol::IpcMsg;

use crate::ui;

const GONE_SECS: u64 = 6;

#[derive(Clone)]
pub struct BlockedUsb {
    pub letter: String,
    pub name: String,
    pub serial: String,
    pub vidpid: String,
}

struct Tracked {
    letter: String,
    last_seen: Instant,
}

fn allow() -> &'static Mutex<HashSet<String>> {
    static ALLOW: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    ALLOW.get_or_init(|| Mutex::new(HashSet::new()))
}

fn tracked() -> &'static Mutex<HashMap<String, Tracked>> {
    static TRACKED: OnceLock<Mutex<HashMap<String, Tracked>>> = OnceLock::new();
    TRACKED.get_or_init(|| Mutex::new(HashMap::new()))
}

fn letters() -> &'static Mutex<HashMap<String, String>> {
    static LETTERS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    LETTERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn notified() -> &'static Mutex<HashSet<String>> {
    static NOTIFIED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    NOTIFIED.get_or_init(|| Mutex::new(HashSet::new()))
}

fn dialogs() -> &'static Mutex<HashSet<String>> {
    static DIALOGS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    DIALOGS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn ejecting() -> &'static Mutex<HashSet<String>> {
    static EJECTING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    EJECTING.get_or_init(|| Mutex::new(HashSet::new()))
}

pub fn is_allowed(serial: &str) -> bool {
    allow()
        .lock()
        .ok()
        .map(|s| s.contains(serial))
        .unwrap_or(false)
}

pub fn allow_serial(serial: &str) {
    if let Ok(mut s) = allow().lock() {
        s.insert(serial.to_string());
    }
}

fn still_tracked(serial: &str) -> bool {
    tracked()
        .lock()
        .ok()
        .map(|t| t.contains_key(serial))
        .unwrap_or(false)
}

fn forget(serial: &str) {
    if let Ok(mut a) = allow().lock() {
        a.remove(serial);
    }
    if let Ok(mut n) = notified().lock() {
        n.remove(serial);
    }
    if let Ok(mut d) = dialogs().lock() {
        d.remove(serial);
    }
    if let Ok(mut l) = letters().lock() {
        l.remove(serial);
    }
    if let Ok(mut t) = tracked().lock() {
        t.remove(serial);
    }
}

/// True the first time this insertion should show the authenticator dialog.
pub fn begin_dialog(serial: &str) -> bool {
    dialogs()
        .lock()
        .map(|mut s| s.insert(serial.to_string()))
        .unwrap_or(false)
}

/// After a failed or cancelled code, ask again on the next watch tick.
pub fn allow_retry(serial: &str) {
    let serial = serial.to_string();
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(8));
        if is_allowed(&serial) || !still_tracked(&serial) {
            return;
        }
        if let Ok(mut d) = dialogs().lock() {
            d.remove(&serial);
        }
        if let Ok(mut n) = notified().lock() {
            n.remove(&serial);
        }
    });
}

pub fn ensure_ejecting(letter: &str, serial: &str) {
    if serial.is_empty() || is_allowed(serial) {
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
            if is_allowed(&serial) || !still_tracked(&serial) {
                break;
            }
            let letter = letters()
                .lock()
                .ok()
                .and_then(|m| m.get(&serial).cloned())
                .unwrap_or_default();
            if !letter.is_empty() {
                eject_letter(&letter);
            }
            thread::sleep(Duration::from_millis(300));
        }
        if let Ok(mut s) = ejecting().lock() {
            s.remove(&serial);
        }
    });
}

pub fn run_watch(tx: Sender<IpcMsg>) {
    ui::log("USB watch started (session)");
    loop {
        scan_and_eject(|dev| {
            let _ = tx.send(IpcMsg::UsbBlocked {
                letter: dev.letter,
                name: dev.name,
                serial: dev.serial,
                vidpid: dev.vidpid,
            });
        });
        thread::sleep(Duration::from_millis(400));
    }
}

fn scan_and_eject(mut on_new: impl FnMut(BlockedUsb)) {
    let vols = list_storage();
    let now = Instant::now();
    let present: HashSet<String> = vols.iter().map(|v| v.serial.clone()).collect();

    let gone: Vec<(String, String)> = match tracked().lock() {
        Ok(map) => map
            .iter()
            .filter(|(serial, t)| {
                !present.contains(*serial) && now.duration_since(t.last_seen).as_secs() >= GONE_SECS
            })
            .map(|(s, t)| (s.clone(), t.letter.clone()))
            .collect(),
        Err(_) => Vec::new(),
    };
    for (serial, letter) in gone {
        ui::log(format!("USB removed {letter} {serial}"));
        forget(&serial);
    }

    if let Ok(mut map) = tracked().lock() {
        for vol in &vols {
            map.insert(
                vol.serial.clone(),
                Tracked {
                    letter: vol.letter.clone(),
                    last_seen: now,
                },
            );
        }
        for (serial, t) in map.iter_mut() {
            if present.contains(serial) {
                t.last_seen = now;
            }
        }
    }

    for vol in vols {
        if is_allowed(&vol.serial) {
            continue;
        }
        ensure_ejecting(&vol.letter, &vol.serial);
        let first = notified()
            .lock()
            .map(|mut s| s.insert(vol.serial.clone()))
            .unwrap_or(false);
        if first {
            ui::log(format!(
                "external device blocked {} {} — authenticator required",
                vol.letter, vol.name
            ));
            crate::ui::tray::poke();
            on_new(vol);
        }
    }
}

#[cfg(windows)]
fn system_drive_letter() -> char {
    std::env::var("SystemDrive")
        .ok()
        .and_then(|s| s.chars().next())
        .unwrap_or('C')
        .to_ascii_uppercase()
}

#[cfg(windows)]
fn list_storage() -> Vec<BlockedUsb> {
    use windows::Win32::Storage::FileSystem::GetLogicalDrives;
    let mask = unsafe { GetLogicalDrives() };
    let sys = system_drive_letter();
    let mut out = Vec::new();
    for i in 0..26u8 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let c = (b'A' + i) as char;
        if c == 'A' || c == 'B' || c == sys {
            continue;
        }
        if !should_block(c) {
            continue;
        }
        let letter = format!("{c}:");
        let name = volume_label(c).unwrap_or_else(|| "External device".into());
        let serial = volume_serial(c).unwrap_or_else(|| letter.clone());
        out.push(BlockedUsb {
            letter,
            name,
            serial,
            vidpid: String::new(),
        });
    }
    out
}

#[cfg(not(windows))]
fn list_storage() -> Vec<BlockedUsb> {
    Vec::new()
}

#[cfg(windows)]
fn should_block(letter: char) -> bool {
    use windows::Win32::Storage::FileSystem::GetDriveTypeW;
    let path: Vec<u16> = format!("{letter}:\\\0").encode_utf16().collect();
    let ty = unsafe { GetDriveTypeW(windows::core::PCWSTR(path.as_ptr())) };
    const DRIVE_UNKNOWN: u32 = 0;
    const DRIVE_NO_ROOT: u32 = 1;
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_REMOTE: u32 = 4;
    const DRIVE_CDROM: u32 = 5;
    if ty == DRIVE_UNKNOWN || ty == DRIVE_NO_ROOT || ty == DRIVE_REMOTE {
        return false;
    }
    if ty == DRIVE_REMOVABLE {
        return true;
    }
    if ty == DRIVE_FIXED || ty == DRIVE_CDROM {
        return is_hotplug_or_port_storage(letter);
    }
    false
}

/// External / hot-plugged storage on USB, SD, Thunderbolt, 1394, eSATA, UASP, etc.
/// Internal SATA/NVMe and HDMI/display adapters are not volumes and are not blocked.
#[cfg(windows)]
fn is_hotplug_or_port_storage(letter: char) -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::Ioctl::{
        PropertyStandardQuery, StorageDeviceProperty, IOCTL_STORAGE_QUERY_PROPERTY,
        STORAGE_DEVICE_DESCRIPTOR, STORAGE_PROPERTY_QUERY,
    };
    use windows::Win32::System::IO::DeviceIoControl;
    #[repr(C)]
    struct HotplugInfo {
        size: u32,
        media_removable: u8,
        media_hotplug: u8,
        device_hotplug: u8,
        write_cache: u8,
    }
    const IOCTL_STORAGE_GET_HOTPLUG_INFO: u32 = 0x002D0C14;
    const BUS_USB: u32 = 7;
    const BUS_1394: u32 = 4;
    const BUS_SD: u32 = 12;
    const BUS_MMC: u32 = 13;
    const BUS_UFS: u32 = 19;
    const BUS_VIRTUAL: u32 = 14;
    const BUS_FILE_VIRTUAL: u32 = 15;

    let path: Vec<u16> = format!("\\\\.\\{letter}:\0").encode_utf16().collect();
    let shares = FILE_SHARE_READ | FILE_SHARE_WRITE;
    let accesses = [0u32, GENERIC_READ.0];
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
            let mut query = STORAGE_PROPERTY_QUERY {
                PropertyId: StorageDeviceProperty,
                QueryType: PropertyStandardQuery,
                AdditionalParameters: [0; 1],
            };
            let mut buf = [0u8; 1024];
            let mut ret = 0u32;
            let ok = DeviceIoControl(
                h,
                IOCTL_STORAGE_QUERY_PROPERTY,
                Some((&mut query as *mut STORAGE_PROPERTY_QUERY).cast()),
                std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
                Some(buf.as_mut_ptr().cast()),
                buf.len() as u32,
                Some(&mut ret),
                None,
            );
            let mut bus = 0u32;
            let mut removable_media = false;
            if ok.is_ok() {
                let desc = &*(buf.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR);
                bus = desc.BusType.0 as u32;
                removable_media = desc.RemovableMedia.0 != 0;
            }
            let mut hot = HotplugInfo {
                size: std::mem::size_of::<HotplugInfo>() as u32,
                media_removable: 0,
                media_hotplug: 0,
                device_hotplug: 0,
                write_cache: 0,
            };
            let mut ret2 = 0u32;
            let hot_ok = DeviceIoControl(
                h,
                IOCTL_STORAGE_GET_HOTPLUG_INFO,
                None,
                0,
                Some((&mut hot as *mut HotplugInfo).cast()),
                std::mem::size_of::<HotplugInfo>() as u32,
                Some(&mut ret2),
                None,
            );
            let _ = CloseHandle(h);
            if bus == BUS_VIRTUAL || bus == BUS_FILE_VIRTUAL {
                return false;
            }
            let hotplug = hot_ok.is_ok()
                && (hot.device_hotplug != 0 || hot.media_hotplug != 0 || hot.media_removable != 0);
            let port = bus == BUS_USB
                || bus == BUS_1394
                || bus == BUS_SD
                || bus == BUS_MMC
                || bus == BUS_UFS;
            if port || removable_media || hotplug {
                ui::log(format!(
                    "external volume {letter}: bus={bus} removable={removable_media} hotplug={hotplug}"
                ));
                return true;
            }
            if ok.is_ok() {
                return false;
            }
        }
    }
    false
}

#[cfg(windows)]
fn volume_serial(letter: char) -> Option<String> {
    use windows::Win32::Storage::FileSystem::GetVolumeInformationW;
    let path: Vec<u16> = format!("{letter}:\\\0").encode_utf16().collect();
    let mut serial = 0u32;
    let ok = unsafe {
        GetVolumeInformationW(
            windows::core::PCWSTR(path.as_ptr()),
            None,
            Some(&mut serial),
            None,
            None,
            None,
        )
    };
    if ok.is_ok() && serial != 0 {
        Some(format!("{serial:08X}"))
    } else {
        None
    }
}

#[cfg(windows)]
fn volume_label(letter: char) -> Option<String> {
    use windows::Win32::Storage::FileSystem::GetVolumeInformationW;
    let path: Vec<u16> = format!("{letter}:\\\0").encode_utf16().collect();
    let mut label = [0u16; 64];
    let ok = unsafe {
        GetVolumeInformationW(
            windows::core::PCWSTR(path.as_ptr()),
            Some(&mut label),
            None,
            None,
            None,
            None,
        )
    };
    if ok.is_err() {
        return None;
    }
    let end = label.iter().position(|&c| c == 0).unwrap_or(label.len());
    let name = String::from_utf16_lossy(&label[..end]).trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

#[cfg(windows)]
fn eject_letter(letter: &str) {
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
