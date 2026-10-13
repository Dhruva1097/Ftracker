use ftracker_core::totp;

use super::dpi;
use super::theme;

const IDC_TOKEN: i32 = 1001;
const IDC_CHAT: i32 = 1002;
const IDC_NAME: i32 = 1003;
const IDC_HOURS: i32 = 1004;
const IDC_CODE: i32 = 1005;
const IDC_STATUS: i32 = 1006;
const IDC_SECRET: i32 = 1007;
const IDC_ACCOUNT: i32 = 1008;
const IDC_SUB: i32 = 1010;
const IDOK: i32 = 1;
const IDCANCEL: i32 = 2;

const WIN_W: i32 = 820;
const WIN_H: i32 = 560;
const QR_BOX: i32 = 248;

pub struct Defaults {
    pub token: String,
    pub chat_id: String,
    pub employee_name: String,
    pub daily_hours: f64,
    pub machine: String,
}

pub struct EnrollResult {
    pub token: String,
    pub chat_id: String,
    pub employee_name: String,
    pub daily_hours: f64,
    pub totp_secret_b32: String,
}

#[cfg(windows)]
struct EnrollState {
    token_val: String,
    chat_val: String,
    token: windows::Win32::Foundation::HWND,
    chat: windows::Win32::Foundation::HWND,
    name: windows::Win32::Foundation::HWND,
    hours: windows::Win32::Foundation::HWND,
    code: windows::Win32::Foundation::HWND,
    status: windows::Win32::Foundation::HWND,
    secret_box: windows::Win32::Foundation::HWND,
    account: windows::Win32::Foundation::HWND,
    qr: windows::Win32::Foundation::HWND,
    btn_ok: windows::Win32::Foundation::HWND,
    btn_cancel: windows::Win32::Foundation::HWND,
    bg: windows::Win32::Graphics::Gdi::HBRUSH,
    white: windows::Win32::Graphics::Gdi::HBRUSH,
    font_btn: windows::Win32::Graphics::Gdi::HFONT,
    qr_bmp: windows::Win32::Graphics::Gdi::HBITMAP,
    qr_dim: i32,
    hover_ok: bool,
    hover_cancel: bool,
    secret: String,
    machine: String,
    result: Option<EnrollResult>,
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

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn get_text(hwnd: windows::Win32::Foundation::HWND) -> String {
    use windows::Win32::UI::WindowsAndMessaging::GetWindowTextW;
    let mut buf = vec![0u16; 1024];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    if n <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..n as usize]).trim().to_string()
}

#[cfg(windows)]
fn set_text(hwnd: windows::Win32::Foundation::HWND, s: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::SetWindowTextW;
    let w = wide(s);
    let _ = unsafe { SetWindowTextW(hwnd, PCWSTR(w.as_ptr())) };
}

#[cfg(windows)]
pub fn run(defaults: &Defaults) -> Option<EnrollResult> {
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Gdi::{
        CreateFontW, CreateSolidBrush, DeleteObject, HBITMAP, HGDIOBJ, HFONT,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Controls::{
        InitCommonControlsEx, ICC_WIN95_CLASSES, INITCOMMONCONTROLSEX,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        AdjustWindowRect, CreateWindowExW, DispatchMessageW, GetMessageW, GetSystemMetrics,
        IsDialogMessageW, LoadCursorW, RegisterClassW, SendMessageW, ShowWindow,
        TranslateMessage, CS_HREDRAW, CS_VREDRAW, IDC_ARROW, MSG, SM_CXSCREEN, SM_CYSCREEN,
        SW_SHOW, WM_SETFONT, WNDCLASSW, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_OVERLAPPED,
        WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, WS_EX_APPWINDOW, WS_EX_CONTROLPARENT,
    };

    let _ = unsafe {
        InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_WIN95_CLASSES,
        })
    };

    let inst = unsafe { GetModuleHandleW(None).ok()? };
    let bg = unsafe { CreateSolidBrush(windows::Win32::Foundation::COLORREF(theme::BG)) };
    let white = unsafe { CreateSolidBrush(windows::Win32::Foundation::COLORREF(theme::SURFACE)) };
    let class: Vec<u16> = "FtrackerEnroll\0".encode_utf16().collect();
    let qr_class: Vec<u16> = "FtrackerQrPane\0".encode_utf16().collect();
    let field_class: Vec<u16> = "FtrackerField\0".encode_utf16().collect();
    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wndproc),
        hInstance: windows::Win32::Foundation::HINSTANCE(inst.0),
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW).ok()? },
        hIcon: crate::ui::logo::large(),
        hbrBackground: bg,
        lpszClassName: PCWSTR(class.as_ptr()),
        ..Default::default()
    };
    unsafe { RegisterClassW(&wc) };
    let qwc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(qr_proc),
        hInstance: windows::Win32::Foundation::HINSTANCE(inst.0),
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW).ok()? },
        hbrBackground: unsafe {
            CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x00FFFFFF))
        },
        lpszClassName: PCWSTR(qr_class.as_ptr()),
        ..Default::default()
    };
    unsafe { RegisterClassW(&qwc) };
    let fwc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(field_proc),
        hInstance: windows::Win32::Foundation::HINSTANCE(inst.0),
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW).ok()? },
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
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_APPWINDOW | WS_EX_CONTROLPARENT,
            PCWSTR(class.as_ptr()),
            w!("Ftracker setup"),
            style,
            (sx - ww) / 2,
            (sy - hh) / 2,
            ww,
            hh,
            HWND(0),
            windows::Win32::UI::WindowsAndMessaging::HMENU(0),
            windows::Win32::Foundation::HINSTANCE(inst.0),
            None,
        )
    };
    if hwnd.0 == 0 {
        return None;
    }
    crate::ui::logo::apply(hwnd);

    let font = unsafe { CreateFontW(-dpi::px(16), 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, w!("Segoe UI")) };
    let font_title = unsafe { CreateFontW(-dpi::px(24), 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, 5, 0, w!("Segoe UI")) };
    let font_small = unsafe { CreateFontW(-dpi::px(13), 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, w!("Segoe UI")) };
    let font_code = unsafe { CreateFontW(-dpi::px(22), 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, 5, 0, w!("Segoe UI")) };
    let font_btn = unsafe { CreateFontW(-dpi::px(16), 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, 5, 0, w!("Segoe UI")) };

    let mut state = EnrollState {
        token_val: defaults.token.clone(),
        chat_val: defaults.chat_id.clone(),
        token: HWND(0),
        chat: HWND(0),
        name: HWND(0),
        hours: HWND(0),
        code: HWND(0),
        status: HWND(0),
        secret_box: HWND(0),
        account: HWND(0),
        qr: HWND(0),
        btn_ok: HWND(0),
        btn_cancel: HWND(0),
        bg,
        white,
        font_btn,
        qr_bmp: HBITMAP(0),
        qr_dim: 0,
        hover_ok: false,
        hover_cancel: false,
        secret: totp::generate_secret_b32(),
        machine: defaults.machine.clone(),
        result: None,
    };
    unsafe { set_user_data(hwnd, &mut state as *mut EnrollState as isize) };

    let inst_h = windows::Win32::Foundation::HINSTANCE(inst.0);
    let hours_s = if defaults.daily_hours.fract() == 0.0 {
        format!("{}", defaults.daily_hours as i32)
    } else {
        format!("{}", defaults.daily_hours)
    };

    unsafe {
        let p = dpi::px;
        let title = ctrl(
            w!("STATIC"),
            w!("Set up Ftracker"),
            WS_CHILD | WS_VISIBLE,
            Default::default(),
            p(28),
            p(16),
            p(500),
            p(32),
            hwnd,
            0,
            inst_h,
        );
        let sub = ctrl(
            w!("STATIC"),
            w!("Admin: scan this QR in Google Authenticator."),
            WS_CHILD | WS_VISIBLE,
            Default::default(),
            p(28),
            p(50),
            p(760),
            p(28),
            hwnd,
            IDC_SUB,
            inst_h,
        );
        let show_token = defaults.token.is_empty();
        let show_chat = defaults.chat_id.is_empty();
        let mut y = p(108);
        if show_token {
            label("Telegram bot token", p(28), y, hwnd, inst_h);
            state.token = field("", p(28), y + p(20), p(430), p(40), hwnd, IDC_TOKEN, inst_h, false, false);
            y += p(80);
        }
        if show_chat {
            label("Admin Telegram chat id", p(28), y, hwnd, inst_h);
            state.chat = field("", p(28), y + p(20), p(430), p(40), hwnd, IDC_CHAT, inst_h, false, false);
            y += p(80);
        }
        label("Employee name", p(28), y, hwnd, inst_h);
        state.name = field(
            &defaults.employee_name,
            p(28),
            y + p(20),
            p(280),
            p(40),
            hwnd,
            IDC_NAME,
            inst_h,
            false,
            false,
        );
        label("Daily hours", p(324), y, hwnd, inst_h);
        state.hours = field(&hours_s, p(324), y + p(20), p(134), p(40), hwnd, IDC_HOURS, inst_h, false, false);
        y += p(80);

        label("Authenticator secret (if you cannot scan)", p(28), y, hwnd, inst_h);
        state.secret_box = field("", p(28), y + p(20), p(430), p(40), hwnd, IDC_SECRET, inst_h, true, false);
        y += p(64);

        state.status = ctrl(
            w!("STATIC"),
            w!("After the admin scans, type the 6-digit code."),
            WS_CHILD | WS_VISIBLE,
            Default::default(),
            p(28),
            y,
            p(430),
            p(72),
            hwnd,
            IDC_STATUS,
            inst_h,
        );

        let qr = p(QR_BOX);
        state.qr = CreateWindowExW(
            Default::default(),
            PCWSTR(qr_class.as_ptr()),
            w!(""),
            WS_CHILD | WS_VISIBLE,
            p(500),
            p(108),
            qr,
            qr,
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::HMENU(0),
            inst_h,
            None,
        );
        state.account = ctrl(
            w!("STATIC"),
            w!(""),
            WS_CHILD | WS_VISIBLE,
            Default::default(),
            p(500),
            p(364),
            p(290),
            p(36),
            hwnd,
            IDC_ACCOUNT,
            inst_h,
        );
        label("6-digit code", p(500), p(404), hwnd, inst_h);
        state.code = field("", p(500), p(424), p(156), p(40), hwnd, IDC_CODE, inst_h, false, true);
        SendMessageW(
            state.code,
            0x00C5,
            windows::Win32::Foundation::WPARAM(6),
            windows::Win32::Foundation::LPARAM(0),
        );

        let owner = windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(
            windows::Win32::UI::WindowsAndMessaging::BS_OWNERDRAW as u32,
        );
        state.btn_cancel = ctrl(
            w!("BUTTON"),
            w!("Cancel"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | owner,
            Default::default(),
            p(500),
            p(474),
            p(120),
            p(40),
            hwnd,
            IDCANCEL,
            inst_h,
        );
        state.btn_ok = ctrl(
            w!("BUTTON"),
            w!("Register this PC"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | owner,
            Default::default(),
            p(632),
            p(474),
            p(156),
            p(40),
            hwnd,
            IDOK,
            inst_h,
        );

        let setf = |h: HWND, f: HFONT| {
            SendMessageW(
                h,
                WM_SETFONT,
                windows::Win32::Foundation::WPARAM(f.0 as usize),
                windows::Win32::Foundation::LPARAM(1),
            );
        };
        setf(title, font_title);
        setf(sub, font_small);
        if state.token.0 != 0 {
            setf(state.token, font);
        }
        if state.chat.0 != 0 {
            setf(state.chat, font);
        }
        setf(state.name, font);
        setf(state.hours, font);
        setf(state.secret_box, font_small);
        setf(state.status, font_small);
        setf(state.account, font_small);
        setf(state.code, font_code);
        setf(state.btn_cancel, font_btn);
        setf(state.btn_ok, font_btn);
        apply_label_fonts(hwnd, font_small);
        setf(title, font_title);
        setf(sub, font_small);
    }

    rebuild_qr(&mut state);
    unsafe {
        set_text(state.secret_box, &state.secret);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let focus = if state.token.0 != 0 {
            state.token
        } else if state.chat.0 != 0 {
            state.chat
        } else {
            state.name
        };
        let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(focus);
        let _ = windows::Win32::UI::WindowsAndMessaging::SetTimer(hwnd, 1, 40, None);
    }

    let mut msg = MSG::default();
    loop {
        let ret = unsafe { GetMessageW(&mut msg, HWND(0), 0, 0) };
        if ret.0 == 0 {
            break;
        }
        if unsafe { IsDialogMessageW(hwnd, &msg) }.0 != 0 {
            continue;
        }
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    unsafe {
        if state.qr_bmp.0 != 0 {
            let _ = DeleteObject(HGDIOBJ(state.qr_bmp.0));
        }
        let _ = DeleteObject(HGDIOBJ(font.0));
        let _ = DeleteObject(HGDIOBJ(font_title.0));
        let _ = DeleteObject(HGDIOBJ(font_small.0));
        let _ = DeleteObject(HGDIOBJ(font_code.0));
        let _ = DeleteObject(HGDIOBJ(font_btn.0));
        let _ = DeleteObject(HGDIOBJ(white.0));
    }
    state.result
}

#[cfg(windows)]
fn apply_label_fonts(parent: windows::Win32::Foundation::HWND, font: windows::Win32::Graphics::Gdi::HFONT) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, SendMessageW, WM_SETFONT};
    let mut child = HWND(0);
    loop {
        child = unsafe { FindWindowExW(parent, child, windows::core::w!("STATIC"), None) };
        if child.0 == 0 {
            break;
        }
        unsafe {
            SendMessageW(
                child,
                WM_SETFONT,
                windows::Win32::Foundation::WPARAM(font.0 as usize),
                windows::Win32::Foundation::LPARAM(1),
            );
        }
    }
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
            crate::ui::chrome::paint(
                hdc,
                rc,
                dpi::px(10),
                theme::SURFACE,
                border,
                t,
                theme::BG,
            );
            EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[cfg(windows)]
fn label(
    text: &str,
    x: i32,
    y: i32,
    parent: windows::Win32::Foundation::HWND,
    inst: windows::Win32::Foundation::HINSTANCE,
) {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{WS_CHILD, WS_VISIBLE};
    let t = wide(text);
    unsafe {
        ctrl(
            windows::core::w!("STATIC"),
            PCWSTR(t.as_ptr()),
            WS_CHILD | WS_VISIBLE,
            Default::default(),
            x,
            y,
            dpi::px(430),
            dpi::px(18),
            parent,
            0,
            inst,
        );
    }
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
    readonly: bool,
    center: bool,
) -> windows::Win32::Foundation::HWND {
    use windows::core::{w, PCWSTR};
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageW, ES_AUTOHSCROLL, ES_CENTER, ES_READONLY, WINDOW_STYLE, WS_CHILD, WS_TABSTOP,
        WS_VISIBLE,
    };
    let shell = unsafe {
        ctrl(
            w!("FtrackerField"),
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
    let mut style = WS_CHILD | WS_VISIBLE | WINDOW_STYLE(ES_AUTOHSCROLL as u32);
    if center {
        style |= WINDOW_STYLE(ES_CENTER as u32);
    }
    if !readonly {
        style |= WS_TABSTOP;
    } else {
        style |= WINDOW_STYLE(ES_READONLY as u32);
    }
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
fn rebuild_qr(state: &mut EnrollState) {
    use windows::Win32::Graphics::Gdi::{DeleteObject, InvalidateRect, HGDIOBJ};
    let name = get_text(state.name);
    let name = if name.is_empty() { "employee".into() } else { name };
    let url = totp::otpauth_url(&name, &state.machine, &state.secret);
    set_text(
        state.account,
        &format!("Admin scan — Google Authenticator\nFtracker:{name}@{}", state.machine),
    );
    let target = (dpi::px(QR_BOX) - dpi::px(24)).max(160) as u32;
    let Ok((dim, bgra)) = totp::qr_bgra_fit(&url, target) else {
        return;
    };
    if state.qr_bmp.0 != 0 {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(state.qr_bmp.0));
        }
        state.qr_bmp = windows::Win32::Graphics::Gdi::HBITMAP(0);
    }
    if let Some(bmp) = dib(&bgra, dim as i32) {
        state.qr_bmp = bmp;
        state.qr_dim = dim as i32;
    }
    unsafe {
        let _ = InvalidateRect(state.qr, None, true);
    }
}

#[cfg(windows)]
fn dib(bgra: &[u8], dim: i32) -> Option<windows::Win32::Graphics::Gdi::HBITMAP> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Graphics::Gdi::{
        CreateDIBSection, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HDC,
    };
    let mut info = BITMAPINFO::default();
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: dim,
        biHeight: -dim,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: 0,
        biSizeImage: (dim * dim * 4) as u32,
        ..Default::default()
    };
    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let hbmp = unsafe {
        CreateDIBSection(
            HDC(0),
            &info,
            DIB_RGB_COLORS,
            &mut bits,
            HANDLE(0),
            0,
        )
        .ok()?
    };
    if !bits.is_null() {
        unsafe {
            std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits as *mut u8, bgra.len());
        }
    }
    Some(hbmp)
}

#[cfg(windows)]
fn try_register(state: &mut EnrollState) -> bool {
    let token = if state.token.0 != 0 {
        get_text(state.token)
    } else {
        state.token_val.clone()
    };
    let chat = if state.chat.0 != 0 {
        get_text(state.chat)
    } else {
        state.chat_val.clone()
    };
    let name = get_text(state.name);
    let hours = get_text(state.hours);
    let code = get_text(state.code);
    if token.is_empty() || chat.is_empty() {
        set_text(
            state.status,
            "Fill the bot token and admin chat id (from @userinfobot, or your .env file).",
        );
        return false;
    }
    if name.is_empty() {
        set_text(state.status, "Enter the employee name for this PC.");
        return false;
    }
    let hours: f64 = hours.parse().unwrap_or(8.0);
    if !totp::verify(&state.secret, &code, chrono::Local::now().timestamp()) {
        set_text(
            state.status,
            "That code did not match. Open Google Authenticator, wait for the next 6-digit number, and try again.",
        );
        return false;
    }
    state.result = Some(EnrollResult {
        token,
        chat_id: chat,
        employee_name: name,
        daily_hours: hours,
        totp_secret_b32: state.secret.clone(),
    });
    true
}

#[cfg(windows)]
fn update_hover(hwnd: windows::Win32::Foundation::HWND, state: &mut EnrollState) {
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
fn draw_button(state: &EnrollState, dis: &windows::Win32::UI::Controls::DRAWITEMSTRUCT) {
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
        let label = if primary { "Register this PC" } else { "Cancel" };
        let text: Vec<u16> = label.encode_utf16().collect();
        let mut sz = windows::Win32::Foundation::SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &text, &mut sz);
        let tx = rc.left + (rc.right - rc.left - sz.cx) / 2;
        let ty = rc.top + (rc.bottom - rc.top - sz.cy) / 2;
        let _ = TextOutW(hdc, tx, ty, &text);
    }
}

#[cfg(windows)]
unsafe extern "system" fn wndproc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::Foundation::{HWND, LRESULT};
    use windows::Win32::Graphics::Gdi::{SetBkMode, SetTextColor, HDC, TRANSPARENT};
    use windows::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, DestroyWindow, GetDlgCtrlID, PostQuitMessage, WM_CLOSE, WM_COMMAND,
        WM_CTLCOLORSTATIC, WM_DESTROY, EN_CHANGE,
    };
    let ptr = get_user_data(hwnd);
    let state = if ptr != 0 {
        Some(&mut *(ptr as *mut EnrollState))
    } else {
        None
    };
    match msg {
        x if x == WM_CTLCOLORSTATIC => {
            let hdc = HDC(wparam.0 as isize);
            SetBkMode(hdc, TRANSPARENT);
            let child = HWND(lparam.0);
            let id = GetDlgCtrlID(child);
            let color = if id == IDC_SUB || id == IDC_STATUS || id == IDC_ACCOUNT {
                theme::MUTED
            } else {
                theme::TEXT
            };
            SetTextColor(hdc, windows::Win32::Foundation::COLORREF(color));
            if let Some(s) = state {
                return LRESULT(s.bg.0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        x if x == windows::Win32::UI::WindowsAndMessaging::WM_CTLCOLOREDIT => {
            let hdc = HDC(wparam.0 as isize);
            SetBkMode(hdc, windows::Win32::Graphics::Gdi::OPAQUE);
            SetTextColor(hdc, windows::Win32::Foundation::COLORREF(theme::TEXT));
            windows::Win32::Graphics::Gdi::SetBkColor(
                hdc,
                windows::Win32::Foundation::COLORREF(theme::SURFACE),
            );
            if let Some(s) = state {
                return LRESULT(s.white.0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        x if x == windows::Win32::UI::WindowsAndMessaging::WM_PAINT => {
            use windows::Win32::Foundation::RECT;
            use windows::Win32::Graphics::Gdi::{BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, HGDIOBJ};
            use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
            let mut ps = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            rc.bottom = dpi::px(3);
            let bar = CreateSolidBrush(windows::Win32::Foundation::COLORREF(theme::PRIMARY));
            FillRect(hdc, &rc, bar);
            let _ = DeleteObject(HGDIOBJ(bar.0));
            EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        x if x == windows::Win32::UI::WindowsAndMessaging::WM_TIMER => {
            if let Some(s) = state {
                update_hover(hwnd, s);
            }
            LRESULT(0)
        }
        x if x == windows::Win32::UI::WindowsAndMessaging::WM_MOUSEMOVE => {
            if let Some(s) = state {
                update_hover(hwnd, s);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        x if x == windows::Win32::UI::WindowsAndMessaging::WM_DRAWITEM => {
            if let Some(s) = state {
                let dis = &*(lparam.0 as *const windows::Win32::UI::Controls::DRAWITEMSTRUCT);
                draw_button(s, dis);
            }
            LRESULT(1)
        }
        x if x == WM_COMMAND => {
            let id = (wparam.0 as u32) & 0xffff;
            let note = ((wparam.0 as u32) >> 16) & 0xffff;
            if let Some(s) = state {
                if note == EN_CHANGE && id == IDC_NAME as u32 {
                    rebuild_qr(s);
                } else if id == IDOK as u32 {
                    if try_register(s) {
                        let _ = DestroyWindow(hwnd);
                    }
                } else if id == IDCANCEL as u32 {
                    let _ = DestroyWindow(hwnd);
                }
            }
            LRESULT(0)
        }
        x if x == WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        x if x == WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[cfg(windows)]
unsafe extern "system" fn qr_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::Foundation::{LRESULT, RECT};
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, BitBlt, CreateCompatibleDC, DeleteDC, EndPaint, SelectObject, SRCCOPY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, GetClientRect, GetParent, WM_PAINT,
    };
    if msg != WM_PAINT {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let mut ps = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let parent = GetParent(hwnd);
    let ptr = get_user_data(parent);
    crate::ui::chrome::paint(
        hdc,
        rc,
        dpi::px(16),
        theme::SURFACE,
        theme::PRIMARY,
        dpi::px(2),
        theme::BG,
    );
    if ptr != 0 {
        let state = &*(ptr as *mut EnrollState);
        if state.qr_bmp.0 != 0 {
            let mem = CreateCompatibleDC(hdc);
            let old = SelectObject(mem, windows::Win32::Graphics::Gdi::HGDIOBJ(state.qr_bmp.0));
            let inset = dpi::px(12);
            let room = (rc.right - rc.left - inset * 2).max(1);
            let size = state.qr_dim.min(room);
            let x = rc.left + (rc.right - rc.left - size) / 2;
            let y = rc.top + (rc.bottom - rc.top - size) / 2;
            let _ = BitBlt(hdc, x, y, size, size, mem, 0, 0, SRCCOPY);
            SelectObject(mem, old);
            let _ = DeleteDC(mem);
        }
    }
    EndPaint(hwnd, &ps);
    LRESULT(0)
}

#[cfg(not(windows))]
pub fn run(_defaults: &Defaults) -> Option<EnrollResult> {
    None
}
