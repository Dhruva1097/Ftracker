use std::thread;
use std::time::Duration;

use super::is_allowed;

#[cfg(windows)]
pub fn eject_letter(letter: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_NO_BUFFERING, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::System::Ioctl::IOCTL_STORAGE_EJECT_MEDIA;

    let letter = letter.chars().next().unwrap_or('E');
    let path: Vec<u16> = format!("\\\\.\\{letter}:\0").encode_utf16().collect();
    unsafe {
        let h = CreateFileW(
            PCWSTR(path.as_ptr()),
            GENERIC_READ.0 | GENERIC_WRITE.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_NO_BUFFERING,
            None,
        );
        let Ok(h) = h else { return };
        const FSCTL_DISMOUNT_VOLUME: u32 = 0x00090020;
        const FSCTL_LOCK_VOLUME: u32 = 0x00090018;
        let mut ret = 0u32;
        let _ = DeviceIoControl(h, FSCTL_LOCK_VOLUME, None, 0, None, 0, Some(&mut ret), None);
        let _ = DeviceIoControl(h, FSCTL_DISMOUNT_VOLUME, None, 0, None, 0, Some(&mut ret), None);
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
        let _ = HANDLE;
    }
}

#[cfg(not(windows))]
pub fn eject_letter(_letter: &str) {}

pub fn keep_ejected(letter: &str, serial: &str) {
    let letter = letter.to_string();
    let serial = serial.to_string();
    thread::spawn(move || {
        for _ in 0..120 {
            if is_allowed(&serial) {
                break;
            }
            eject_letter(&letter);
            thread::sleep(Duration::from_millis(400));
        }
    });
}
