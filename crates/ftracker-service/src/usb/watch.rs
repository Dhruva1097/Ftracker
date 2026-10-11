use super::{on_plugin, on_removed, PendingUsb};

#[cfg(windows)]
pub fn run() {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DispatchMessageW, GetMessageW, RegisterClassW, TranslateMessage,
        CS_HREDRAW, HWND_MESSAGE, MSG, WNDCLASSW, WS_OVERLAPPED,
    };

    let class: Vec<u16> = "FtrackerUsbWnd\0".encode_utf16().collect();
    let inst = unsafe { GetModuleHandleW(None).unwrap_or_default() };
    let wc = WNDCLASSW {
        style: CS_HREDRAW,
        lpfnWndProc: Some(wndproc),
        hInstance: windows::Win32::Foundation::HINSTANCE(inst.0),
        lpszClassName: PCWSTR(class.as_ptr()),
        ..Default::default()
    };
    unsafe { RegisterClassW(&wc) };
    let hwnd = unsafe {
        CreateWindowExW(
            Default::default(),
            PCWSTR(class.as_ptr()),
            windows::core::w!("FtrackerUsb"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            windows::Win32::UI::WindowsAndMessaging::HMENU(0),
            windows::Win32::Foundation::HINSTANCE(inst.0),
            None,
        )
    };
    let _ = hwnd;
    let mut msg = MSG::default();
    loop {
        let ret = unsafe { GetMessageW(&mut msg, HWND(0), 0, 0) };
        if ret.0 == 0 {
            break;
        }
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

#[cfg(windows)]
unsafe extern "system" fn wndproc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, DBT_DEVICEARRIVAL, DBT_DEVICEREMOVECOMPLETE, WM_DEVICECHANGE,
    };
    use windows::Win32::Foundation::LRESULT;
    if msg == WM_DEVICECHANGE {
        let ev = wparam.0 as u32;
        if ev == DBT_DEVICEARRIVAL {
            handle_arrival(lparam);
        } else if ev == DBT_DEVICEREMOVECOMPLETE {
            handle_remove(lparam);
        }
        return LRESULT(1);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

#[cfg(windows)]
fn handle_arrival(lparam: windows::Win32::Foundation::LPARAM) {
    if let Some(letter) = volume_letter(lparam) {
        if !is_usb_volume(&letter) {
            return;
        }
        let serial = volume_serial(&letter).unwrap_or_else(|| letter.clone());
        on_plugin(PendingUsb {
            letter: format!("{letter}:"),
            name: "USB storage".into(),
            serial,
            vidpid: String::new(),
        });
    }
}

#[cfg(windows)]
fn handle_remove(lparam: windows::Win32::Foundation::LPARAM) {
    if let Some(letter) = volume_letter(lparam) {
        let serial = volume_serial(&letter).unwrap_or_else(|| letter.clone());
        on_removed(&serial, &format!("{letter}:"));
    }
}

#[cfg(windows)]
fn volume_letter(lparam: windows::Win32::Foundation::LPARAM) -> Option<String> {
    // DEV_BROADCAST_HDR + DEV_BROADCAST_VOLUME
    if lparam.0 == 0 {
        return None;
    }
    unsafe {
        let hdr = lparam.0 as *const u32;
        // dbch_size, dbch_devicetype, dbch_reserved
        let dtype = *hdr.offset(1);
        const DBT_DEVTYP_VOLUME: u32 = 0x00000002;
        if dtype != DBT_DEVTYP_VOLUME {
            return None;
        }
        let unitmask = *((lparam.0 as *const u8).add(12) as *const u32);
        for i in 0..26 {
            if unitmask & (1 << i) != 0 {
                let c = (b'A' + i) as char;
                return Some(c.to_string());
            }
        }
    }
    None
}

#[cfg(windows)]
fn is_usb_volume(letter: &str) -> bool {
    use windows::Win32::Storage::FileSystem::GetDriveTypeW;
    let path: Vec<u16> = format!("{letter}:\\\0").encode_utf16().collect();
    let ty = unsafe { GetDriveTypeW(windows::core::PCWSTR(path.as_ptr())) };
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_CDROM: u32 = 5;
    if ty == DRIVE_CDROM {
        return false;
    }
    if ty != DRIVE_REMOVABLE && ty != DRIVE_FIXED {
        return false;
    }
    bus_is_usb(letter)
}

#[cfg(windows)]
fn bus_is_usb(letter: &str) -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::System::Ioctl::{
        IOCTL_STORAGE_QUERY_PROPERTY, STORAGE_DEVICE_DESCRIPTOR, STORAGE_PROPERTY_QUERY,
        StorageDeviceProperty, PropertyStandardQuery,
    };
    let path: Vec<u16> = format!("\\\\.\\{letter}:\0").encode_utf16().collect();
    unsafe {
        let h = CreateFileW(
            PCWSTR(path.as_ptr()),
            GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            Default::default(),
            None,
        );
        let Ok(h) = h else { return false };
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
        let _ = CloseHandle(h);
        if ok.is_err() {
            return false;
        }
        // STORAGE_BUS_TYPE BusType at offset of descriptor
        let desc = &*(buf.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR);
        const BUS_TYPE_USB: u32 = 7;
        desc.BusType.0 as u32 == BUS_TYPE_USB
    }
}

#[cfg(windows)]
fn volume_serial(letter: &str) -> Option<String> {
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

#[cfg(not(windows))]
pub fn run() {
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}
