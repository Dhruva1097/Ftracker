use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;

use crate::app::SessionState;

#[cfg(windows)]
use windows::Win32::Foundation::HWND;

const TRAY_ID: u32 = 1;
const WM_TRAY: u32 = 0x8001; // WM_APP + 1
const NIN_SELECT: u32 = 0x0400;
const NOTIFYICON_VERSION_4: u32 = 4;

static TRAY_ICON: AtomicIsize = AtomicIsize::new(0);
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

/// 16×16 PRIMARY rounded square with a white F (32-bit alpha, no .ico file).
#[cfg(windows)]
fn make_icon() -> windows::Win32::UI::WindowsAndMessaging::HICON {
    use windows::Win32::Foundation::BOOL;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC,
    };
    use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, ICONINFO, HICON};
    const N: i32 = 16;
    let (pr, pg, pb) = crate::ui::theme::split_rgb(crate::ui::theme::PRIMARY);
    let (fr, fg, fb) = crate::ui::theme::split_rgb(crate::ui::theme::ON_PRIMARY);
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: N,
            biHeight: -N,
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
        let hbmp = CreateDIBSection(
            hdc,
            &bmi,
            DIB_RGB_COLORS,
            &mut bits,
            None,
            0,
        )
        .unwrap_or(HBITMAP(0));
        if hbmp.0 == 0 || bits.is_null() {
            let _ = DeleteDC(hdc);
            return HICON(0);
        }
        let px = std::slice::from_raw_parts_mut(bits as *mut u8, (N * N * 4) as usize);
        for y in 0..N {
            for x in 0..N {
                let i = ((y * N + x) * 4) as usize;
                if in_round_square(x, y, N, 3) {
                    if is_f_glyph(x, y) {
                        px[i] = fb;
                        px[i + 1] = fg;
                        px[i + 2] = fr;
                    } else {
                        px[i] = pb;
                        px[i + 1] = pg;
                        px[i + 2] = pr;
                    }
                    px[i + 3] = 255;
                } else {
                    px[i] = 0;
                    px[i + 1] = 0;
                    px[i + 2] = 0;
                    px[i + 3] = 0;
                }
            }
        }
        let mut mask = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: N,
                biHeight: -N,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut mask_bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let hmask = CreateDIBSection(
            hdc,
            &mut mask,
            DIB_RGB_COLORS,
            &mut mask_bits,
            None,
            0,
        )
        .unwrap_or(HBITMAP(0));
        if hmask.0 != 0 && !mask_bits.is_null() {
            let m = std::slice::from_raw_parts_mut(mask_bits as *mut u8, (N * N * 4) as usize);
            m.fill(0);
        }
        let info = ICONINFO {
            fIcon: BOOL(1),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: hmask,
            hbmColor: hbmp,
        };
        let icon = CreateIconIndirect(&info).unwrap_or(HICON(0));
        if hmask.0 != 0 {
            let _ = DeleteObject(hmask);
        }
        let _ = DeleteObject(hbmp);
        let _ = DeleteDC(hdc);
        icon
    }
}

#[cfg(windows)]
fn in_round_square(x: i32, y: i32, n: i32, r: i32) -> bool {
    if x < 0 || y < 0 || x >= n || y >= n {
        return false;
    }
    let cx0 = r;
    let cy0 = r;
    let cx1 = n - 1 - r;
    let cy1 = n - 1 - r;
    if x < cx0 && y < cy0 {
        return (x - cx0) * (x - cx0) + (y - cy0) * (y - cy0) <= r * r;
    }
    if x > cx1 && y < cy0 {
        return (x - cx1) * (x - cx1) + (y - cy0) * (y - cy0) <= r * r;
    }
    if x < cx0 && y > cy1 {
        return (x - cx0) * (x - cx0) + (y - cy1) * (y - cy1) <= r * r;
    }
    if x > cx1 && y > cy1 {
        return (x - cx1) * (x - cx1) + (y - cy1) * (y - cy1) <= r * r;
    }
    true
}

#[cfg(windows)]
fn is_f_glyph(x: i32, y: i32) -> bool {
    (x >= 4 && x <= 6 && y >= 4 && y <= 11)
        || (x >= 4 && x <= 11 && y >= 4 && y <= 5)
        || (x >= 4 && x <= 9 && y >= 7 && y <= 8)
}

#[cfg(windows)]
fn icon_handle() -> windows::Win32::UI::WindowsAndMessaging::HICON {
    use windows::Win32::UI::WindowsAndMessaging::HICON;
    let raw = TRAY_ICON.load(Ordering::SeqCst);
    if raw != 0 {
        return HICON(raw);
    }
    let icon = make_icon();
    TRAY_ICON.store(icon.0, Ordering::SeqCst);
    icon
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
        hIcon: icon_handle(),
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
fn destroy_icon() {
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, HICON};
    let raw = TRAY_ICON.swap(0, Ordering::SeqCst);
    if raw != 0 {
        unsafe {
            let _ = DestroyIcon(HICON(raw));
        }
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

    let class: Vec<u16> = "FtrackerTrayHost\0".encode_utf16().collect();
    let inst = unsafe { GetModuleHandleW(None)? };
    let wc = WNDCLASSW {
        lpfnWndProc: Some(wndproc),
        hInstance: windows::Win32::Foundation::HINSTANCE(inst.0),
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
    unsafe {
        set_user_data(hwnd, state as *mut SessionState as isize);
        let _ = SetTimer(hwnd, 1, 1000, None);
    }
    let _ = icon_handle();
    state.on_paint();
    tray_add(hwnd, &tip_text(Some(state)));
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
                destroy_icon();
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        x if x == WM_DESTROY => {
            if let Some(s) = state {
                s.flush_for_shutdown_reason("session_end");
            }
            tray_delete(hwnd);
            destroy_icon();
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
