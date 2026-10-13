use std::sync::Mutex;

use crate::app::SessionState;

#[cfg(windows)]
use windows::Win32::Foundation::HWND;

const TRAY_ID: u32 = 1;
const WM_TRAY: u32 = 0x8001; // WM_APP + 1
const NIN_SELECT: u32 = 0x0400;
const NOTIFYICON_VERSION_4: u32 = 4;

static LAST_TIP: Mutex<String> = Mutex::new(String::new());

/// Second launch while the session is already running: tray is already there.
pub fn request_show() {}

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
fn taskbar_created_msg() -> u32 {
    use std::sync::OnceLock;
    use windows::core::w;
    use windows::Win32::UI::WindowsAndMessaging::RegisterWindowMessageW;
    static M: OnceLock<u32> = OnceLock::new();
    *M.get_or_init(|| unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) })
}

#[cfg(windows)]
fn fill_tip(buf: &mut [u16; 128], tip: &str) {
    buf.fill(0);
    for (i, c) in tip.encode_utf16().take(127).enumerate() {
        buf[i] = c;
    }
}

#[cfg(windows)]
fn nid_base(hwnd: HWND, tip: &str) -> windows::Win32::UI::Shell::NOTIFYICONDATAW {
    use windows::Win32::UI::Shell::{
        NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NOTIFYICONDATAW,
    };
    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
        uCallbackMessage: WM_TRAY,
        hIcon: crate::ui::logo::small(),
        ..Default::default()
    };
    fill_tip(&mut nid.szTip, tip);
    nid
}

#[cfg(windows)]
fn tray_add(hwnd: HWND, tip: &str) {
    use windows::Win32::UI::Shell::{
        Shell_NotifyIconW, NIM_ADD, NIM_SETVERSION, NOTIFYICONDATAW_0,
    };
    let mut nid = nid_base(hwnd, tip);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_ADD, &nid);
        nid.Anonymous = NOTIFYICONDATAW_0 {
            uVersion: NOTIFYICON_VERSION_4,
        };
        let _ = Shell_NotifyIconW(NIM_SETVERSION, &nid);
    }
    if let Ok(mut last) = LAST_TIP.lock() {
        last.clear();
        last.push_str(tip);
    }
}

#[cfg(windows)]
fn tray_modify_tip(hwnd: HWND, tip: &str) {
    use windows::Win32::UI::Shell::{Shell_NotifyIconW, NIM_MODIFY};
    {
        let last = LAST_TIP.lock().unwrap_or_else(|e| e.into_inner());
        if last.as_str() == tip {
            return;
        }
    }
    let nid = nid_base(hwnd, tip);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
    if let Ok(mut last) = LAST_TIP.lock() {
        last.clear();
        last.push_str(tip);
    }
}

#[cfg(windows)]
fn tray_delete(hwnd: HWND) {
    use windows::Win32::UI::Shell::{Shell_NotifyIconW, NIM_DELETE, NOTIFYICONDATAW};
    let nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        ..Default::default()
    };
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

#[cfg(windows)]
fn tip_text(state: Option<&SessionState>) -> String {
    match state {
        Some(s) if !s.chip_label.is_empty() => s.chip_label.clone(),
        _ => "Ftracker".into(),
    }
}

#[cfg(windows)]
fn is_tray_click(lparam: windows::Win32::Foundation::LPARAM) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        WM_CONTEXTMENU, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_RBUTTONUP,
    };
    let ev = (lparam.0 as u32) & 0xffff;
    ev == WM_LBUTTONUP
        || ev == WM_RBUTTONUP
        || ev == WM_LBUTTONDBLCLK
        || ev == NIN_SELECT
        || ev == WM_CONTEXTMENU
}

#[cfg(windows)]
pub fn run_message_loop(state: &mut SessionState, _user_launch: bool) -> anyhow::Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DispatchMessageW, GetMessageW, RegisterClassW, SetTimer,
        TranslateMessage, MSG, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
    };

    crate::ui::log("tray: creating host window");
    let class: Vec<u16> = "FtrackerTrayHost\0".encode_utf16().collect();
    let inst = unsafe { GetModuleHandleW(None)? };
    let wc = WNDCLASSW {
        lpfnWndProc: Some(wndproc),
        hInstance: windows::Win32::Foundation::HINSTANCE(inst.0),
        hIcon: crate::ui::logo::large(),
        lpszClassName: PCWSTR(class.as_ptr()),
        ..Default::default()
    };
    unsafe { RegisterClassW(&wc) };

    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            PCWSTR(class.as_ptr()),
            windows::core::w!("Ftracker"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            HWND(0),
            windows::Win32::UI::WindowsAndMessaging::HMENU(0),
            windows::Win32::Foundation::HINSTANCE(inst.0),
            None,
        )
    };
    if hwnd.0 == 0 {
        anyhow::bail!("tray host window failed");
    }
    crate::ui::log(format!("tray: host hwnd={}", hwnd.0));
    unsafe {
        set_user_data(hwnd, state as *mut SessionState as isize);
        let _ = SetTimer(hwnd, 1, 1000, None);
    }
    crate::ui::logo::apply(hwnd);
    state.on_paint();
    tray_add(hwnd, &tip_text(Some(state)));
    crate::ui::log("tray: notify icon added — look next to the clock (or ^ overflow)");
    let _ = taskbar_created_msg();

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
    use windows::Win32::Foundation::LRESULT;
    use windows::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, PostQuitMessage, WM_DESTROY, WM_ENDSESSION, WM_QUERYENDSESSION, WM_TIMER,
    };
    let ptr = get_user_data(hwnd);
    let state = if ptr != 0 {
        Some(&mut *(ptr as *mut SessionState))
    } else {
        None
    };
    if msg == WM_TRAY {
        if is_tray_click(lparam) {
            if let Some(s) = state {
                s.open_action();
            }
        }
        return LRESULT(0);
    }
    if msg == taskbar_created_msg() {
        tray_add(hwnd, &tip_text(state.as_deref()));
        return LRESULT(0);
    }
    match msg {
        x if x == WM_TIMER => {
            if let Some(s) = state {
                s.on_timer();
                tray_modify_tip(hwnd, &tip_text(Some(s)));
            }
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
                tray_delete(hwnd);
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        x if x == WM_DESTROY => {
            if let Some(s) = state {
                s.flush_for_shutdown_reason("session_end");
            }
            tray_delete(hwnd);
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
