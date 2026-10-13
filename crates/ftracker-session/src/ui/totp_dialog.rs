use super::dpi;
use super::theme;

fn purpose_copy(purpose: &str) -> (&'static str, &'static str) {
    match purpose {
        "usb" => (
            "External device blocked",
            "Enter the 6-digit code from Google Authenticator to allow this device.",
        ),
        "upload" => (
            "Upload blocked",
            "Enter the admin authenticator code to allow this upload.",
        ),
        "uninstall" => (
            "Uninstall Ftracker",
            "Enter the admin authenticator code to uninstall.",
        ),
        "enroll" => (
            "Confirm setup",
            "Enter the current Google Authenticator code to confirm.",
        ),
        "reregister" => (
            "Change employee",
            "Enter the admin authenticator code to change the registered employee.",
        ),
        _ => (
            "Ftracker",
            "Enter the admin authenticator code.",
        ),
    }
}

pub fn prompt(purpose: &str) -> Option<String> {
    prompt_with(purpose, "")
}

pub fn prompt_with(purpose: &str, detail: &str) -> Option<String> {
    #[cfg(windows)]
    {
        prompt_win(purpose, detail)
    }
    #[cfg(not(windows))]
    {
        let (title, body) = purpose_copy(purpose);
        let msg = if detail.is_empty() {
            body.to_string()
        } else {
            format!("{body}\n{detail}")
        };
        tinyfiledialogs::input_box(title, &msg, "").map(|s| s.trim().to_string())
    }
}

pub fn info(msg: &str) {
    let _ = tinyfiledialogs::message_box_ok("Ftracker", msg, tinyfiledialogs::MessageBoxIcon::Info);
}

pub fn error(msg: &str) {
    let _ = tinyfiledialogs::message_box_ok("Ftracker", msg, tinyfiledialogs::MessageBoxIcon::Error);
}

#[allow(dead_code)]
pub fn confirm(msg: &str) -> bool {
    tinyfiledialogs::message_box_yes_no(
        "Ftracker",
        msg,
        tinyfiledialogs::MessageBoxIcon::Warning,
        tinyfiledialogs::YesNo::No,
    ) == tinyfiledialogs::YesNo::Yes
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

const IDOK: i32 = 1;
const IDCANCEL: i32 = 2;
const IDC_CODE: i32 = 3001;
const WIN_W: i32 = 420;
const WIN_H: i32 = 280;

#[cfg(windows)]
struct TotpState {
    code: windows::Win32::Foundation::HWND,
    btn_ok: windows::Win32::Foundation::HWND,
    btn_cancel: windows::Win32::Foundation::HWND,
    bg: windows::Win32::Graphics::Gdi::HBRUSH,
    font_btn: windows::Win32::Graphics::Gdi::HFONT,
    hover_ok: bool,
    hover_cancel: bool,
    finished: bool,
    accepted: Option<String>,
}

#[cfg(windows)]
unsafe fn set_user_data(hwnd: windows::Win32::Foundation::HWND, ptr: isize) {
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
unsafe fn get_user_data(hwnd: windows::Win32::Foundation::HWND) -> isize {
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
fn prompt_win(purpose: &str, detail: &str) -> Option<String> {
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Gdi::{CreateFontW, CreateSolidBrush, DeleteObject, HGDIOBJ};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Controls::{InitCommonControlsEx, ICC_WIN95_CLASSES, INITCOMMONCONTROLSEX};
    use windows::Win32::UI::WindowsAndMessaging::{
        AdjustWindowRect, CreateWindowExW, DispatchMessageW, GetMessageW, GetSystemMetrics,
        IsDialogMessageW, LoadCursorW, RegisterClassW, SendMessageW, SetForegroundWindow,
        SetWindowPos, ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW, IDC_ARROW,
        SM_CXSCREEN, SM_CYSCREEN, SW_SHOWNORMAL, WINDOW_STYLE, WM_SETFONT, WNDCLASSW, WS_CAPTION,
        WS_CHILD, WS_CLIPCHILDREN, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
        WS_EX_APPWINDOW, WS_EX_CONTROLPARENT, WS_EX_TOPMOST,
    };

    let (title, body) = purpose_copy(purpose);
    crate::ui::log(format!("TOTP window: {title} {detail}"));

    let _ = unsafe {
        InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_WIN95_CLASSES,
        })
    };

    let inst = match unsafe { GetModuleHandleW(None) } {
        Ok(h) => h,
        Err(_) => return None,
    };
    let cursor = match unsafe { LoadCursorW(None, IDC_ARROW) } {
        Ok(c) => c,
        Err(_) => return None,
    };
    let bg = unsafe { CreateSolidBrush(windows::Win32::Foundation::COLORREF(theme::BG)) };

    let class: Vec<u16> = "FtrackerTotp\0".encode_utf16().collect();
    let field_class: Vec<u16> = "FtrackerTotpField\0".encode_utf16().collect();
    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wndproc),
        hInstance: windows::Win32::Foundation::HINSTANCE(inst.0),
        hCursor: cursor,
        hbrBackground: bg,
        lpszClassName: PCWSTR(class.as_ptr()),
        ..Default::default()
    };
    unsafe { RegisterClassW(&wc) };
    let fwc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(field_proc),
        hInstance: windows::Win32::Foundation::HINSTANCE(inst.0),
        hCursor: cursor,
        hbrBackground: bg,
        lpszClassName: PCWSTR(field_class.as_ptr()),
        ..Default::default()
    };
    unsafe { RegisterClassW(&fwc) };

    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_VISIBLE | WS_CLIPCHILDREN;
    let mut rc = RECT {
        left: 0,
        top: 0,
        right: dpi::px(WIN_W),
        bottom: dpi::px(WIN_H),
    };
    let _ = unsafe { AdjustWindowRect(&mut rc, style, false) };
    let ww = rc.right - rc.left;
    let hh = rc.bottom - rc.top;
    let sx = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let sy = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let title_w = wide(title);
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_APPWINDOW | WS_EX_CONTROLPARENT | WS_EX_TOPMOST,
            PCWSTR(class.as_ptr()),
            PCWSTR(title_w.as_ptr()),
            style,
            ((sx - ww) / 2).max(0),
            ((sy - hh) / 2).max(0),
            ww,
            hh,
            HWND(0),
            windows::Win32::UI::WindowsAndMessaging::HMENU(0),
            windows::Win32::Foundation::HINSTANCE(inst.0),
            None,
        )
    };
    if hwnd.0 == 0 {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(bg.0));
        }
        crate::ui::log("TOTP window: CreateWindowExW failed");
        return None;
    }
    crate::ui::logo::apply(hwnd);

    let font = unsafe { CreateFontW(-dpi::px(15), 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, w!("Segoe UI")) };
    let font_title = unsafe { CreateFontW(-dpi::px(20), 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, 5, 0, w!("Segoe UI")) };
    let font_code = unsafe { CreateFontW(-dpi::px(22), 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, 5, 0, w!("Segoe UI")) };
    let font_btn = unsafe { CreateFontW(-dpi::px(15), 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, 5, 0, w!("Segoe UI")) };

    let pad = dpi::px(24);
    let mut y = dpi::px(20);
    let inner_w = dpi::px(WIN_W) - pad * 2;

    let mut state = TotpState {
        code: HWND(0),
        btn_ok: HWND(0),
        btn_cancel: HWND(0),
        bg,
        font_btn,
        hover_ok: false,
        hover_cancel: false,
        finished: false,
        accepted: None,
    };

    unsafe {
        let t = wide(title);
        let h_title = ctrl(
            w!("STATIC"),
            PCWSTR(t.as_ptr()),
            WS_CHILD | WS_VISIBLE,
            Default::default(),
            pad,
            y,
            inner_w,
            dpi::px(28),
            hwnd,
            0,
            windows::Win32::Foundation::HINSTANCE(inst.0),
        );
        SendMessageW(
            h_title,
            WM_SETFONT,
            windows::Win32::Foundation::WPARAM(font_title.0 as usize),
            windows::Win32::Foundation::LPARAM(1),
        );
        y += dpi::px(36);

        if !detail.is_empty() {
            let d = wide(detail);
            let h_d = ctrl(
                w!("STATIC"),
                PCWSTR(d.as_ptr()),
                WS_CHILD | WS_VISIBLE,
                Default::default(),
                pad,
                y,
                inner_w,
                dpi::px(22),
                hwnd,
                0,
                windows::Win32::Foundation::HINSTANCE(inst.0),
            );
            SendMessageW(
                h_d,
                WM_SETFONT,
                windows::Win32::Foundation::WPARAM(font.0 as usize),
                windows::Win32::Foundation::LPARAM(1),
            );
            y += dpi::px(26);
        }

        let b = wide(body);
        let h_body = ctrl(
            w!("STATIC"),
            PCWSTR(b.as_ptr()),
            WS_CHILD | WS_VISIBLE,
            Default::default(),
            pad,
            y,
            inner_w,
            dpi::px(40),
            hwnd,
            0,
            windows::Win32::Foundation::HINSTANCE(inst.0),
        );
        SendMessageW(
            h_body,
            WM_SETFONT,
            windows::Win32::Foundation::WPARAM(font.0 as usize),
            windows::Win32::Foundation::LPARAM(1),
        );
        y += dpi::px(48);

        state.code = field(
            "",
            pad,
            y,
            inner_w,
            dpi::px(44),
            hwnd,
            IDC_CODE,
            windows::Win32::Foundation::HINSTANCE(inst.0),
        );
        SendMessageW(
            state.code,
            WM_SETFONT,
            windows::Win32::Foundation::WPARAM(font_code.0 as usize),
            windows::Win32::Foundation::LPARAM(1),
        );
        SendMessageW(
            state.code,
            0x00C5,
            windows::Win32::Foundation::WPARAM(8),
            windows::Win32::Foundation::LPARAM(0),
        );
        y += dpi::px(60);

        let btn_w = dpi::px(120);
        let btn_h = dpi::px(40);
        let gap = dpi::px(12);
        let btn_y = y;
        let cancel_x = pad + inner_w - btn_w;
        let ok_x = cancel_x - gap - btn_w;

        use windows::Win32::UI::WindowsAndMessaging::BS_OWNERDRAW;
        state.btn_cancel = ctrl(
            w!("BUTTON"),
            w!("Not now"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
            Default::default(),
            cancel_x,
            btn_y,
            btn_w,
            btn_h,
            hwnd,
            IDCANCEL,
            windows::Win32::Foundation::HINSTANCE(inst.0),
        );
        state.btn_ok = ctrl(
            w!("BUTTON"),
            w!("Allow"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
            Default::default(),
            ok_x,
            btn_y,
            btn_w,
            btn_h,
            hwnd,
            IDOK,
            windows::Win32::Foundation::HINSTANCE(inst.0),
        );
        SendMessageW(
            state.btn_ok,
            WM_SETFONT,
            windows::Win32::Foundation::WPARAM(font_btn.0 as usize),
            windows::Win32::Foundation::LPARAM(1),
        );
        SendMessageW(
            state.btn_cancel,
            WM_SETFONT,
            windows::Win32::Foundation::WPARAM(font_btn.0 as usize),
            windows::Win32::Foundation::LPARAM(1),
        );

        set_user_data(hwnd, &mut state as *mut TotpState as isize);
        let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
        let _ = SetWindowPos(
            hwnd,
            HWND(-1),
            0,
            0,
            0,
            0,
            windows::Win32::UI::WindowsAndMessaging::SET_WINDOW_POS_FLAGS(0x0003), // SWP_NOMOVE|NOSIZE
        );
        let _ = SetForegroundWindow(hwnd);
        let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(state.code);
    }

    let mut msg = windows::Win32::UI::WindowsAndMessaging::MSG::default();
    while !state.finished {
        let ret = unsafe { GetMessageW(&mut msg, HWND(0), 0, 0) };
        if ret.0 == 0 {
            state.finished = true;
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0);
            }
            break;
        }
        if msg.message == 0x0100 && msg.wParam.0 == 0x0D {
            state.accepted = Some(get_text(state.code));
            state.finished = true;
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd);
            }
            continue;
        }
        if msg.message == 0x0100 && msg.wParam.0 == 0x1B {
            state.accepted = None;
            state.finished = true;
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd);
            }
            continue;
        }
        if unsafe { IsDialogMessageW(hwnd, &msg) }.0 != 0 {
            continue;
        }
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    let accepted = state.accepted.take();
    unsafe {
        let _ = DeleteObject(HGDIOBJ(font.0));
        let _ = DeleteObject(HGDIOBJ(font_title.0));
        let _ = DeleteObject(HGDIOBJ(font_code.0));
        let _ = DeleteObject(HGDIOBJ(font_btn.0));
        let _ = DeleteObject(HGDIOBJ(bg.0));
    }
    accepted.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

#[cfg(windows)]
unsafe fn ctrl(
    class: windows::core::PCWSTR,
    text: windows::core::PCWSTR,
    style: windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE,
    ex: windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    parent: windows::Win32::Foundation::HWND,
    id: i32,
    inst: windows::Win32::Foundation::HINSTANCE,
) -> windows::Win32::Foundation::HWND {
    windows::Win32::UI::WindowsAndMessaging::CreateWindowExW(
        ex,
        class,
        text,
        style,
        x,
        y,
        w,
        h,
        parent,
        windows::Win32::UI::WindowsAndMessaging::HMENU(id as isize),
        inst,
        None,
    )
}

#[cfg(windows)]
fn field(
    text: &str,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    parent: windows::Win32::Foundation::HWND,
    id: i32,
    inst: windows::Win32::Foundation::HINSTANCE,
) -> windows::Win32::Foundation::HWND {
    use windows::core::{w, PCWSTR};
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageW, ES_AUTOHSCROLL, ES_CENTER, ES_NUMBER, WINDOW_STYLE, WS_CHILD, WS_TABSTOP,
        WS_VISIBLE,
    };
    let shell = unsafe {
        ctrl(
            w!("FtrackerTotpField"),
            w!(""),
            WS_CHILD | WS_VISIBLE,
            Default::default(),
            x,
            y,
            w,
            h,
            parent,
            0,
            inst,
        )
    };
    let t = wide(text);
    let style = WS_CHILD
        | WS_VISIBLE
        | WS_TABSTOP
        | WINDOW_STYLE(ES_AUTOHSCROLL as u32)
        | WINDOW_STYLE(ES_CENTER as u32)
        | WINDOW_STYLE(ES_NUMBER as u32);
    let edit = unsafe {
        ctrl(
            w!("EDIT"),
            PCWSTR(t.as_ptr()),
            style,
            Default::default(),
            dpi::px(12),
            dpi::px(8),
            w - dpi::px(24),
            h - dpi::px(16),
            shell,
            id,
            inst,
        )
    };
    unsafe {
        SendMessageW(
            edit,
            0x00D3,
            windows::Win32::Foundation::WPARAM(3),
            windows::Win32::Foundation::LPARAM(0x000A000A),
        );
        set_user_data(shell, edit.0);
    }
    edit
}

#[cfg(windows)]
fn get_text(hwnd: windows::Win32::Foundation::HWND) -> String {
    use windows::Win32::UI::WindowsAndMessaging::GetWindowTextW;
    let mut buf = vec![0u16; 64];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    if n <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..n as usize]).trim().to_string()
}

#[cfg(windows)]
unsafe extern "system" fn field_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::Foundation::{LRESULT, RECT};
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, EndPaint, GetStockObject, SetBkColor, SetBkMode, SetTextColor, HDC, OPAQUE,
        WHITE_BRUSH,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, GetClientRect, GetParent, SendMessageW, WM_COMMAND, WM_CTLCOLOREDIT,
        WM_ERASEBKGND, WM_PAINT,
    };
    match msg {
        x if x == WM_ERASEBKGND => LRESULT(1),
        x if x == WM_CTLCOLOREDIT => {
            let hdc = HDC(wparam.0 as isize);
            SetBkMode(hdc, OPAQUE);
            SetTextColor(hdc, windows::Win32::Foundation::COLORREF(theme::TEXT));
            SetBkColor(hdc, windows::Win32::Foundation::COLORREF(theme::SURFACE));
            LRESULT(GetStockObject(WHITE_BRUSH).0)
        }
        x if x == WM_COMMAND => {
            let _ = windows::Win32::Graphics::Gdi::InvalidateRect(hwnd, None, false);
            let parent = GetParent(hwnd);
            SendMessageW(parent, WM_COMMAND, wparam, lparam);
            LRESULT(0)
        }
        x if x == WM_PAINT => {
            let mut ps = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let edit = windows::Win32::Foundation::HWND(get_user_data(hwnd));
            let focused = windows::Win32::UI::Input::KeyboardAndMouse::GetFocus().0 == edit.0
                && edit.0 != 0;
            let border = if focused {
                theme::PRIMARY
            } else {
                theme::LINE
            };
            let t = if focused { dpi::px(2) } else { dpi::px(1) };
            crate::ui::chrome::paint(hdc, rc, dpi::px(10), theme::SURFACE, border, t, theme::BG);
            EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[cfg(windows)]
fn draw_button(state: &TotpState, dis: &windows::Win32::UI::Controls::DRAWITEMSTRUCT) {
    use windows::Win32::Graphics::Gdi::{
        GetTextExtentPoint32W, SelectObject, SetBkMode, SetTextColor, TextOutW, HGDIOBJ,
        TRANSPARENT,
    };
    use windows::Win32::UI::Controls::ODS_SELECTED;
    let hdc = dis.hDC;
    let rc = dis.rcItem;
    let primary = dis.CtlID == IDOK as u32;
    let pressed = dis.itemState.0 & ODS_SELECTED.0 != 0;
    let hover = if primary {
        state.hover_ok
    } else {
        state.hover_cancel
    };
    unsafe {
        let (fill, border, fg, border_w) = if primary {
            let fill = if pressed {
                theme::PRIMARY_DOWN
            } else if hover {
                theme::PRIMARY_HOVER
            } else {
                theme::PRIMARY
            };
            (fill, fill, theme::ON_PRIMARY, 0)
        } else {
            let fill = if hover || pressed {
                theme::PRIMARY_SOFT
            } else {
                theme::SURFACE
            };
            (fill, theme::PRIMARY, theme::PRIMARY, dpi::px(2))
        };
        crate::ui::chrome::paint(hdc, rc, dpi::px(10), fill, border, border_w, theme::BG);
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, windows::Win32::Foundation::COLORREF(fg));
        let _ = SelectObject(hdc, HGDIOBJ(state.font_btn.0));
        let label: Vec<u16> = if primary {
            "Allow".encode_utf16().collect()
        } else {
            "Not now".encode_utf16().collect()
        };
        let mut sz = windows::Win32::Foundation::SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &label, &mut sz);
        let tx = rc.left + (rc.right - rc.left - sz.cx) / 2;
        let ty = rc.top + (rc.bottom - rc.top - sz.cy) / 2;
        let _ = TextOutW(hdc, tx, ty, &label);
    }
}

#[cfg(windows)]
fn update_hover(hwnd: windows::Win32::Foundation::HWND, state: &mut TotpState) {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{InvalidateRect, ScreenToClient};
    use windows::Win32::UI::WindowsAndMessaging::{ChildWindowFromPoint, GetCursorPos};
    let mut pt = POINT::default();
    if unsafe { GetCursorPos(&mut pt) }.is_err() {
        return;
    }
    unsafe {
        let _ = ScreenToClient(hwnd, &mut pt);
    }
    let child = unsafe { ChildWindowFromPoint(hwnd, pt) };
    let over_ok = child.0 == state.btn_ok.0;
    let over_cancel = child.0 == state.btn_cancel.0;
    if over_ok != state.hover_ok {
        state.hover_ok = over_ok;
        unsafe {
            let _ = InvalidateRect(state.btn_ok, None, true);
        }
    }
    if over_cancel != state.hover_cancel {
        state.hover_cancel = over_cancel;
        unsafe {
            let _ = InvalidateRect(state.btn_cancel, None, true);
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
    use windows::Win32::Foundation::LRESULT;
    use windows::Win32::Graphics::Gdi::{SetBkMode, SetTextColor, TRANSPARENT};
    use windows::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, DestroyWindow, GetDlgCtrlID, WM_CLOSE, WM_COMMAND, WM_CTLCOLORSTATIC,
        WM_DESTROY, WM_DRAWITEM, WM_MOUSEMOVE,
    };
    let ptr = get_user_data(hwnd);
    let state = if ptr != 0 {
        Some(&mut *(ptr as *mut TotpState))
    } else {
        None
    };
    match msg {
        x if x == WM_CTLCOLORSTATIC => {
            let hdc = windows::Win32::Graphics::Gdi::HDC(wparam.0 as isize);
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, windows::Win32::Foundation::COLORREF(theme::TEXT));
            if let Some(s) = state {
                return LRESULT(s.bg.0);
            }
            LRESULT(0)
        }
        x if x == WM_MOUSEMOVE => {
            if let Some(s) = state {
                update_hover(hwnd, s);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        x if x == WM_DRAWITEM => {
            if let Some(s) = state {
                let dis = &*(lparam.0 as *const windows::Win32::UI::Controls::DRAWITEMSTRUCT);
                draw_button(s, dis);
            }
            LRESULT(1)
        }
        x if x == WM_COMMAND => {
            let id = GetDlgCtrlID(windows::Win32::Foundation::HWND(lparam.0)).max(0);
            let id = if id == 0 {
                (wparam.0 as u32) & 0xffff
            } else {
                id as u32
            };
            let notify = (wparam.0 as u32) >> 16;
            if let Some(s) = state {
                let _ = notify;
                if id == IDOK as u32 {
                    s.accepted = Some(get_text(s.code));
                    s.finished = true;
                    let _ = DestroyWindow(hwnd);
                } else if id == IDCANCEL as u32 {
                    s.accepted = None;
                    s.finished = true;
                    let _ = DestroyWindow(hwnd);
                }
            }
            LRESULT(0)
        }
        x if x == WM_CLOSE => {
            if let Some(s) = state {
                s.finished = true;
            }
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        x if x == WM_DESTROY => {
            if let Some(s) = state {
                s.finished = true;
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
