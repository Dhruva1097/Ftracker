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

#[cfg(windows)]
fn chip_rect() -> windows::Win32::Foundation::RECT {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
    let mut wa = RECT::default();
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some((&mut wa as *mut RECT).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
    }
    let (cw, ch) = chip_size();
    let m = crate::ui::dpi::px(28);
    RECT {
        left: wa.right - cw - m,
        top: wa.bottom - ch - m,
        right: wa.right - m,
        bottom: wa.bottom - m,
    }
}

#[cfg(windows)]
fn rects_overlap(a: windows::Win32::Foundation::RECT, b: windows::Win32::Foundation::RECT) -> bool {
    a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top
}

#[cfg(windows)]
fn is_cloaked(hwnd: HWND) -> bool {
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    let mut cloaked = 0u32;
    unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            (&mut cloaked as *mut u32).cast(),
            4,
        )
        .is_ok()
            && cloaked != 0
    }
}

#[cfg(windows)]
fn window_class(hwnd: HWND) -> String {
    use windows::Win32::UI::WindowsAndMessaging::GetClassNameW;
    let mut buf = [0u16; 96];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) } as usize;
    if n == 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&buf[..n])
    }
}

#[cfg(windows)]
fn window_title(hwnd: HWND) -> String {
    use windows::Win32::UI::WindowsAndMessaging::GetWindowTextW;
    let mut buf = [0u16; 128];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) } as usize;
    if n == 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&buf[..n])
    }
}

#[cfg(windows)]
fn is_shell_desktop(hwnd: HWND) -> bool {
    matches!(
        window_class(hwnd).as_str(),
        "Progman"
            | "WorkerW"
            | "SHELLDLL_DefView"
            | "SysListView32"
            | "Shell_TrayWnd"
            | "Shell_SecondaryTrayWnd"
            | "ForegroundStaging"
            | "ApplicationManager_DesktopShellWindow"
    )
}

#[cfg(windows)]
fn is_ghost_host(hwnd: HWND) -> bool {
    let class = window_class(hwnd);
    matches!(
        class.as_str(),
        "Windows.UI.Core.CoreWindow"
            | "XamlExplorerHostIslandWindow"
            | "Xaml_WindowedPopupClass"
            | "DummyDWMListenerWindow"
            | "EdgeUiInputTopWndClass"
            | "NativeHWNDHost"
            | "IME"
            | "MSCTFIME UI"
            | "NarratorHelperWindow"
            | "ForegroundStaging"
    ) || class.starts_with("Windows.Internal.Shell")
}

#[cfg(windows)]
fn desktop_in_front() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetParent, GetShellWindow, GetWindow, GW_OWNER,
    };
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0 == 0 {
            return true;
        }
        let mut h = fg;
        for _ in 0..6 {
            if h.0 == 0 {
                break;
            }
            if is_shell_desktop(h) || h == GetShellWindow() {
                return true;
            }
            let owner = GetWindow(h, GW_OWNER);
            if owner.0 != 0 && is_shell_desktop(owner) {
                return true;
            }
            h = GetParent(h);
        }
        false
    }
}

#[cfg(windows)]
fn mouse_on_chip(hwnd: HWND) -> bool {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, WindowFromPoint};
    let mut pt = POINT::default();
    unsafe {
        if GetCursorPos(&mut pt).is_err() {
            return false;
        }
        WindowFromPoint(pt) == hwnd
    }
}

/// A real app (Cursor, Chrome, …) covering the chip corner — not shell/ghost hosts.
#[cfg(windows)]
fn app_covers_desktop_chip(self_hwnd: HWND, chip: windows::Win32::Foundation::RECT) -> bool {
    use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowLongW, GetWindowRect, IsIconic, IsWindowVisible, GWL_EXSTYLE,
        GWL_STYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_MINIMIZE, WS_VISIBLE,
    };
    struct Ctx {
        self_hwnd: HWND,
        chip: RECT,
        hit: bool,
    }
    unsafe extern "system" fn cb(hwnd: HWND, lp: LPARAM) -> BOOL {
        let ctx = &mut *(lp.0 as *mut Ctx);
        if ctx.hit || hwnd == ctx.self_hwnd {
            return BOOL(1);
        }
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() || is_cloaked(hwnd) {
            return BOOL(1);
        }
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if (style & WS_VISIBLE.0) == 0 || (style & WS_MINIMIZE.0) != 0 {
            return BOOL(1);
        }
        if (ex & WS_EX_TOOLWINDOW.0) != 0 || (ex & WS_EX_NOACTIVATE.0) != 0 {
            return BOOL(1);
        }
        if is_shell_desktop(hwnd) || is_ghost_host(hwnd) {
            return BOOL(1);
        }
        if window_title(hwnd).is_empty() {
            return BOOL(1);
        }
        let mut rc = RECT::default();
        if GetWindowRect(hwnd, &mut rc).is_err() {
            return BOOL(1);
        }
        if rc.right - rc.left < 80 || rc.bottom - rc.top < 80 {
            return BOOL(1);
        }
        if rects_overlap(rc, ctx.chip) {
            ctx.hit = true;
            return BOOL(0);
        }
        BOOL(1)
    }
    let mut ctx = Ctx {
        self_hwnd,
        chip,
        hit: false,
    };
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(&mut ctx as *mut Ctx as isize));
    }
    ctx.hit
}

/// On the desktop wallpaper when nothing covers that corner. Hidden over apps.
/// TOPMOST only while shown — otherwise Win11 leaves the chip behind the wallpaper.
#[cfg(windows)]
fn place_on_desktop(hwnd: HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, ShowWindow, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE,
        SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE,
    };
    if HIDDEN.load(Ordering::SeqCst) {
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                HWND_NOTOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        return;
    }
    let rc = chip_rect();
    let show = desktop_in_front() || mouse_on_chip(hwnd) || !app_covers_desktop_chip(hwnd, rc);
    if !show {
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                HWND_NOTOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        return;
    }
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            rc.left,
            rc.top,
            rc.right - rc.left,
            rc.bottom - rc.top,
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
        WS_POPUP,
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
            WS_POPUP,
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
        let _ = SetTimer(hwnd, 2, 150, None);
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
            if wparam.0 == 2 {
                if take_show_signal() {
                    show_chip(hwnd);
                } else {
                    place_on_desktop(hwnd);
                }
                return LRESULT(0);
            }
            if take_show_signal() {
                show_chip(hwnd);
            }
            if let Some(s) = state {
                s.on_timer();
                if !HIDDEN.load(Ordering::SeqCst) {
                    let _ = windows::Win32::Graphics::Gdi::InvalidateRect(hwnd, None, false);
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
