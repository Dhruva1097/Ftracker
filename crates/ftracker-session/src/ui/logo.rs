//! Standard Ftracker mark: blue rounded square, white **FT**. Used on tray, title bars, and taskbar.

#[cfg(windows)]
use std::sync::OnceLock;
#[cfg(windows)]
use windows::Win32::Foundation::HWND;
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::HICON;

include!("logo_paint.rs");

#[cfg(windows)]
fn hicon(n: i32) -> HICON {
    use windows::Win32::Foundation::BOOL;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC,
    };
    use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, ICONINFO};
    let bgra = paint_bgra(n);
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: n,
            biHeight: -n,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    unsafe {
        let hdc = CreateCompatibleDC(HDC(0));
        if hdc.0 == 0 {
            return HICON(0);
        }
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let hbmp = CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap_or(HBITMAP(0));
        if hbmp.0 == 0 || bits.is_null() {
            crate::ui::log("logo: color DIB failed");
            let _ = DeleteDC(hdc);
            return HICON(0);
        }
        std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits as *mut u8, bgra.len());
        let hmask = windows::Win32::Graphics::Gdi::CreateBitmap(n, n, 1, 1, None);
        if hmask.0 == 0 {
            let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(hbmp.0));
            let _ = DeleteDC(hdc);
            crate::ui::log("logo: 1-bit mask bitmap failed");
            return HICON(0);
        }
        let info = ICONINFO {
            fIcon: BOOL(1),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: hmask,
            hbmColor: hbmp,
        };
        let icon = CreateIconIndirect(&info).unwrap_or(HICON(0));
        if icon.0 == 0 {
            crate::ui::log("logo: CreateIconIndirect failed");
        }
        let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(hmask.0));
        let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(hbmp.0));
        let _ = DeleteDC(hdc);
        icon
    }
}

#[cfg(windows)]
fn cached(n: i32) -> HICON {
    match n {
        16 => {
            static I: OnceLock<isize> = OnceLock::new();
            HICON(*I.get_or_init(|| hicon(16).0))
        }
        _ => {
            static I: OnceLock<isize> = OnceLock::new();
            HICON(*I.get_or_init(|| hicon(32).0))
        }
    }
}

#[cfg(windows)]
pub fn small() -> HICON {
    cached(16)
}

#[cfg(windows)]
pub fn large() -> HICON {
    cached(32)
}

/// Title-bar and taskbar icon for a top-level Ftracker window.
#[cfg(windows)]
pub fn apply(hwnd: HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, ICON_BIG, ICON_SMALL, WM_SETICON};
    if hwnd.0 == 0 {
        return;
    }
    let sm = small();
    let lg = large();
    if sm.0 == 0 || lg.0 == 0 {
        crate::ui::log("logo: skip WM_SETICON (icon handle is 0)");
        return;
    }
    unsafe {
        SendMessageW(
            hwnd,
            WM_SETICON,
            windows::Win32::Foundation::WPARAM(ICON_SMALL as usize),
            windows::Win32::Foundation::LPARAM(sm.0),
        );
        SendMessageW(
            hwnd,
            WM_SETICON,
            windows::Win32::Foundation::WPARAM(ICON_BIG as usize),
            windows::Win32::Foundation::LPARAM(lg.0),
        );
    }
}

#[cfg(not(windows))]
pub fn apply(_hwnd: ()) {}
