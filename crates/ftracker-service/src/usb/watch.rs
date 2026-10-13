use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use super::{on_plugin, on_removed, PendingUsb};

const GONE_SECS: u64 = 6;

struct Tracked {
    letter: String,
    last_seen: Instant,
}

fn tracked() -> &'static Mutex<HashMap<String, Tracked>> {
    static TRACKED: OnceLock<Mutex<HashMap<String, Tracked>>> = OnceLock::new();
    TRACKED.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn run() {
    eprintln!("ftracker-svc: USB poll watch started");
    loop {
        poll();
        thread::sleep(Duration::from_millis(400));
    }
}

fn poll() {
    let vols = list_storage();
    let now = Instant::now();
    let present: HashSet<String> = vols.iter().map(|v| v.serial.clone()).collect();

    let gone: Vec<(String, String)> = {
        let map = match tracked().lock() {
            Ok(m) => m,
            Err(_) => return,
        };
        map.iter()
            .filter(|(serial, t)| {
                !present.contains(*serial) && now.duration_since(t.last_seen).as_secs() >= GONE_SECS
            })
            .map(|(s, t)| (s.clone(), t.letter.clone()))
            .collect()
    };
    for (serial, letter) in gone {
        eprintln!("ftracker-svc: USB removed {letter} {serial}");
        if let Ok(mut map) = tracked().lock() {
            map.remove(&serial);
        }
        on_removed(&serial, &letter);
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
    }

    for vol in vols {
        on_plugin(vol);
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
fn list_storage() -> Vec<PendingUsb> {
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
        out.push(PendingUsb {
            letter,
            name,
            serial,
            vidpid: String::new(),
        });
    }
    out
}

#[cfg(not(windows))]
fn list_storage() -> Vec<PendingUsb> {
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

#[cfg(windows)]
fn is_hotplug_or_port_storage(letter: char) -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::System::Ioctl::{
        IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery, STORAGE_DEVICE_DESCRIPTOR,
        STORAGE_PROPERTY_QUERY, StorageDeviceProperty,
    };
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
            return port || removable_media || hotplug;
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
