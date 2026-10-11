//! System DPI. Call `enable` before any window so Windows does not bitmap-stretch the UI.

use std::sync::atomic::{AtomicI32, Ordering};

static DPI: AtomicI32 = AtomicI32::new(96);

pub fn enable() {
    #[cfg(windows)]
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::SetProcessDPIAware();
        let hdc = windows::Win32::Graphics::Gdi::GetDC(windows::Win32::Foundation::HWND(0));
        let d = windows::Win32::Graphics::Gdi::GetDeviceCaps(
            hdc,
            windows::Win32::Graphics::Gdi::LOGPIXELSX,
        );
        let _ = windows::Win32::Graphics::Gdi::ReleaseDC(windows::Win32::Foundation::HWND(0), hdc);
        if d > 0 {
            DPI.store(d, Ordering::SeqCst);
        }
    }
}

pub fn px(logical: i32) -> i32 {
    let d = DPI.load(Ordering::Relaxed);
    if d <= 0 || d == 96 {
        logical
    } else {
        (logical * d + 48) / 96
    }
}
