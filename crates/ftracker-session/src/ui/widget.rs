use std::sync::atomic::{AtomicBool, Ordering};

use crate::app::SessionState;

#[cfg(windows)]
use windows::Win32::Foundation::HWND;

const KEY_MAGENTA: u32 = crate::ui::theme::rgb(255, 0, 128);
const REMOVE_HIT: i32 = 32;
const ID_REMOVE: usize = 1;

static HIDDEN: AtomicBool = AtomicBool::new(false);

/// If the session is already running, show the chip again (user launched Ftracker).
pub fn request_show() {
    #[cfg(windows)]
    {
        signal_show();
    }
}

#[cfg(windows)]
fn hidden_file() -> Option<std::path::PathBuf> {
    ftracker_core::paths::widget_hidden_path().ok()
}

#[cfg(windows)]
fn persist_hidden(hidden: bool) {
    if let Some(p) = hidden_file() {
        if hidden {
            let _ = std::fs::write(p, b"1");
        } else {
            let _ = std::fs::remove_file(p);
        }
    }
}

#[cfg(windows)]
fn is_persisted_hidden() -> bool {
    hidden_file().map(|p| p.exists()).unwrap_or(false)
}

#[cfg(windows)]
fn show_event() -> windows::Win32::Foundation::HANDLE {
    use std::sync::OnceLock;
    use windows::core::PCWSTR;
    use windows::Win32::System::Threading::CreateEventW;
    static H: OnceLock<isize> = OnceLock::new();
    let raw = H.get_or_init(|| {
        let name: Vec<u16> = ftracker_core::paths::widget_show_event_name()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        unsafe { CreateEventW(None, false, false, PCWSTR(name.as_ptr())) }
            .map(|h| h.0)
            .unwrap_or(0)
    });
    windows::Win32::Foundation::HANDLE(*raw)
}

#[cfg(windows)]
fn signal_show() {
    use windows::Win32::System::Threading::SetEvent;
    let h = show_event();
    if h.0 != 0 {
        let _ = unsafe { SetEvent(h) };
    }
}

#[cfg(windows)]
fn take_show_signal() -> bool {
    use windows::Win32::Foundation::WAIT_OBJECT_0;
    use windows::Win32::System::Threading::WaitForSingleObject;
    let h = show_event();
    if h.0 == 0 {
        return false;
    }
    unsafe { WaitForSingleObject(h, 0) == WAIT_OBJECT_0 }
}

#[cfg(windows)]
fn hide_chip(hwnd: HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
    HIDDEN.store(true, Ordering::SeqCst);
    persist_hidden(true);
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

#[cfg(windows)]
fn show_chip(hwnd: HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNOACTIVATE};
    HIDDEN.store(false, Ordering::SeqCst);
    persist_hidden(false);
    place_on_desktop(hwnd);
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
}

#[cfg(windows)]
fn popup_remove(hwnd: HWND) {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, TrackPopupMenu, MF_STRING,
        TPM_RETURNCMD, TPM_RIGHTBUTTON,
    };
    unsafe {
        let Ok(menu) = CreatePopupMenu() else {
            return;
        };
        let label: Vec<u16> = "Remove from desktop\0".encode_utf16().collect();
        let _ = AppendMenuW(menu, MF_STRING, ID_REMOVE, PCWSTR(label.as_ptr()));
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD,
            pt.x,
            pt.y,
            0,
            hwnd,
            None,
        );
        let _ = DestroyMenu(menu);
        if cmd.0 as usize == ID_REMOVE {
            hide_chip(hwnd);
        }
    }
}

#[cfg(windows)]
unsafe fn set_user_data(hwnd: HWND, ptr: isize) {
    #[cfg(target_pointer_width = "64")]
    {
        use windows::Win32::UI::WindowsAndMessaging::{SetWindowLongPtrW, GWLP_USERDATA};
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr);
    }
    #[cfg(target_pointer_width = "32")]
    {
        use windows::Win32::UI::WindowsAndMessaging::{SetWindowLongW, GWL_USERDATA};
        SetWindowLongW(hwnd, GWL_USERDATA, ptr as i32);
    }
}

#[cfg(windows)]
unsafe fn get_user_data(hwnd: HWND) -> isize {
    #[cfg(target_pointer_width = "64")]
    {
        use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, GWLP_USERDATA};
        GetWindowLongPtrW(hwnd, GWLP_USERDATA)
    }
    #[cfg(target_pointer_width = "32")]
    {
        use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongW, GWL_USERDATA};
        GetWindowLongW(hwnd, GWL_USERDATA) as isize
    }
}

#[cfg(windows)]
fn chip_size() -> (i32, i32) {
    (crate::ui::dpi::px(196), crate::ui::dpi::px(44))
}

/// Top-level chip on the work area. Win11 wallpaper WorkerW hides child windows.
#[cfg(windows)]
fn place_on_desktop(hwnd: HWND) {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SystemParametersInfoW, HWND_TOPMOST, SPI_GETWORKAREA, SWP_NOACTIVATE,
        SWP_SHOWWINDOW, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
    if HIDDEN.load(Ordering::SeqCst) {
        return;
    }
    unsafe {
        let mut wa = RECT::default();
        let _ = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some((&mut wa as *mut RECT).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
        let (cw, ch) = chip_size();
        let m = crate::ui::dpi::px(28);
        let x = wa.right - cw - m;
        let y = wa.bottom - ch - m;
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            x,
            y,
            cw,
            ch,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

#[cfg(windows)]
pub fn run_message_loop(state: &mut SessionState, user_launch: bool) -> anyhow::Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::COLORREF;
    use windows::Win32::Graphics::Gdi::{GetStockObject, HBRUSH, NULL_BRUSH};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DispatchMessageW, GetMessageW, LoadCursorW, RegisterClassW, SetLayeredWindowAttributes,
        SetTimer, ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW, IDC_ARROW, LWA_COLORKEY, MSG,
        SW_SHOWNOACTIVATE, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
        WS_POPUP, WS_VISIBLE,
    };

    if user_launch {
        persist_hidden(false);
        HIDDEN.store(false, Ordering::SeqCst);
    } else {
        HIDDEN.store(is_persisted_hidden(), Ordering::SeqCst);
    }
    let _ = show_event();

    let class: Vec<u16> = "FtrackerDesktopWidget\0".encode_utf16().collect();
    let inst = unsafe { GetModuleHandleW(None)? };
    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wndproc),
        hInstance: windows::Win32::Foundation::HINSTANCE(inst.0),
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW)? },
        hbrBackground: HBRUSH(unsafe { GetStockObject(NULL_BRUSH) }.0),
        lpszClassName: PCWSTR(class.as_ptr()),
        ..Default::default()
    };
    unsafe { RegisterClassW(&wc) };

    let (cw, ch) = chip_size();
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
            PCWSTR(class.as_ptr()),
            windows::core::w!("Ftracker"),
            WS_POPUP | WS_VISIBLE,
            0,
            0,
            cw,
            ch,
            HWND(0),
            windows::Win32::UI::WindowsAndMessaging::HMENU(0),
            windows::Win32::Foundation::HINSTANCE(inst.0),
            None,
        )
    };
    if hwnd.0 == 0 {
        anyhow::bail!("widget window failed");
    }
    unsafe {
        let _ = SetLayeredWindowAttributes(
            hwnd,
            COLORREF(KEY_MAGENTA),
            255,
            LWA_COLORKEY,
        );
        set_user_data(hwnd, state as *mut SessionState as isize);
        if HIDDEN.load(Ordering::SeqCst) {
            let _ = ShowWindow(hwnd, windows::Win32::UI::WindowsAndMessaging::SW_HIDE);
        } else {
            place_on_desktop(hwnd);
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        let _ = SetTimer(hwnd, 1, 1000, None);
    }

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
    Ok(())
}

#[cfg(windows)]
unsafe extern "system" fn wndproc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::Foundation::{LRESULT, RECT};
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, EndPaint, SetBkMode, SetTextColor, TextOutW, TRANSPARENT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, GetClientRect, PostQuitMessage, WM_DESTROY, WM_DISPLAYCHANGE, WM_ENDSESSION,
        WM_LBUTTONUP, WM_PAINT, WM_QUERYENDSESSION, WM_RBUTTONUP, WM_SETTINGCHANGE, WM_TIMER,
    };
    let ptr = get_user_data(hwnd);
    let state = if ptr != 0 {
        Some(&mut *(ptr as *mut SessionState))
    } else {
        None
    };
    match msg {
        x if x == WM_PAINT => {
            let mut ps = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let idle = state
                .as_ref()
                .map(|s| s.chip_label.contains("idle"))
                .unwrap_or(false);
            let fill = if idle {
                crate::ui::theme::CHIP_IDLE
            } else {
                crate::ui::theme::CHIP
            };
            crate::ui::chrome::paint(
                hdc,
                rc,
                crate::ui::dpi::px(12),
                fill,
                fill,
                0,
                KEY_MAGENTA,
            );
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(
                hdc,
                windows::Win32::Foundation::COLORREF(crate::ui::theme::CHIP_TEXT),
            );
            if let Some(s) = state {
                s.on_paint();
                let text: Vec<u16> = s.chip_label.encode_utf16().collect();
                let _ = TextOutW(hdc, crate::ui::dpi::px(16), crate::ui::dpi::px(12), &text);
                let xmark: Vec<u16> = "×".encode_utf16().collect();
                let x = (rc.right - crate::ui::dpi::px(22)).max(0);
                let _ = TextOutW(hdc, x, crate::ui::dpi::px(12), &xmark);
            }
            EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        x if x == WM_TIMER => {
            if take_show_signal() {
                show_chip(hwnd);
            }
            if let Some(s) = state {
                s.on_timer();
                if !HIDDEN.load(Ordering::SeqCst) {
                    let _ = windows::Win32::Graphics::Gdi::InvalidateRect(hwnd, None, false);
                }
            }
            if wparam.0 == 1 && !HIDDEN.load(Ordering::SeqCst) {
                static mut TICKS: u32 = 0;
                TICKS = TICKS.wrapping_add(1);
                if TICKS % 5 == 0 {
                    place_on_desktop(hwnd);
                }
            }
            LRESULT(0)
        }
        x if x == WM_DISPLAYCHANGE || x == WM_SETTINGCHANGE => {
            if !HIDDEN.load(Ordering::SeqCst) {
                place_on_desktop(hwnd);
            }
            LRESULT(0)
        }
        x if x == WM_LBUTTONUP => {
            let x = ((lparam.0 as u32) & 0xffff) as i16 as i32;
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            if x >= rc.right - crate::ui::dpi::px(REMOVE_HIT) {
                hide_chip(hwnd);
            } else if let Some(s) = state {
                s.open_action();
            }
            LRESULT(0)
        }
        x if x == WM_RBUTTONUP => {
            popup_remove(hwnd);
            LRESULT(0)
        }
        x if x == WM_QUERYENDSESSION => {
            if let Some(s) = state {
                s.flush_for_shutdown_reason("logoff_or_shutdown");
            }
            LRESULT(1)
        }
        x if x == WM_ENDSESSION => {
            if wparam.0 != 0 {
                if let Some(s) = state {
                    s.flush_for_shutdown_reason("logoff_or_shutdown");
                }
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        x if x == WM_DESTROY => {
            if let Some(s) = state {
                s.flush_for_shutdown_reason("session_end");
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[cfg(not(windows))]
pub fn run_message_loop(_state: &mut SessionState, _user_launch: bool) -> anyhow::Result<()> {
    anyhow::bail!("windows only")
}
