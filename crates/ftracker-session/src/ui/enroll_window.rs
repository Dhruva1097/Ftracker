use ftracker_core::totp;

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
const BG: u32 = 0x00F8F6F3; // BGR #F3F6F8
const TEXT: u32 = 0x00271811; // BGR #111827
const MUTED: u32 = 0x0063554B; // BGR #4B5563

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
    token: windows::Win32::Foundation::HWND,
    chat: windows::Win32::Foundation::HWND,
    name: windows::Win32::Foundation::HWND,
    hours: windows::Win32::Foundation::HWND,
    code: windows::Win32::Foundation::HWND,
    status: windows::Win32::Foundation::HWND,
    secret_box: windows::Win32::Foundation::HWND,
    account: windows::Win32::Foundation::HWND,
    qr: windows::Win32::Foundation::HWND,
    bg: windows::Win32::Graphics::Gdi::HBRUSH,
    qr_bmp: windows::Win32::Graphics::Gdi::HBITMAP,
    qr_dim: i32,
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
        WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, WS_EX_APPWINDOW, WS_EX_CLIENTEDGE, WS_EX_CONTROLPARENT,
    };

    let _ = unsafe {
        InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_WIN95_CLASSES,
        })
    };

    let inst = unsafe { GetModuleHandleW(None).ok()? };
    let bg = unsafe { CreateSolidBrush(windows::Win32::Foundation::COLORREF(BG)) };
    let class: Vec<u16> = "FtrackerEnroll\0".encode_utf16().collect();
    let qr_class: Vec<u16> = "FtrackerQrPane\0".encode_utf16().collect();
    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wndproc),
        hInstance: windows::Win32::Foundation::HINSTANCE(inst.0),
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW).ok()? },
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

    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_VISIBLE | WS_CLIPCHILDREN;
    let mut rc = RECT {
        left: 0,
        top: 0,
        right: WIN_W,
        bottom: WIN_H,
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

    let font = unsafe { CreateFontW(-15, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, w!("Segoe UI")) };
    let font_title = unsafe { CreateFontW(-22, 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, 5, 0, w!("Segoe UI")) };
    let font_small = unsafe { CreateFontW(-13, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, w!("Segoe UI")) };
    let font_code = unsafe { CreateFontW(-20, 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, 5, 0, w!("Segoe UI")) };

    let mut state = EnrollState {
        token: HWND(0),
        chat: HWND(0),
        name: HWND(0),
        hours: HWND(0),
        code: HWND(0),
        status: HWND(0),
        secret_box: HWND(0),
        account: HWND(0),
        qr: HWND(0),
        bg,
        qr_bmp: HBITMAP(0),
        qr_dim: 0,
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
        let title = ctrl(
            w!("STATIC"),
            w!("Set up Ftracker"),
            WS_CHILD | WS_VISIBLE,
            Default::default(),
            28,
            16,
            500,
            32,
            hwnd,
            0,
            inst_h,
        );
        let sub = ctrl(
            w!("STATIC"),
            w!("This PC records active work, apps, location, and occasional screen stills for the admin PDF. USB storage is blocked; large uploads and uninstall need the authenticator code."),
            WS_CHILD | WS_VISIBLE,
            Default::default(),
            28,
            50,
            760,
            40,
            hwnd,
            IDC_SUB,
            inst_h,
        );
        label("Telegram bot token", 28, 108, hwnd, inst_h);
        state.token = edit(&defaults.token, 28, 128, 430, 26, hwnd, IDC_TOKEN, inst_h, false);
        label("Admin Telegram chat id", 28, 168, hwnd, inst_h);
        state.chat = edit(&defaults.chat_id, 28, 188, 430, 26, hwnd, IDC_CHAT, inst_h, false);
        label("Employee name", 28, 228, hwnd, inst_h);
        state.name = edit(
            &defaults.employee_name,
            28,
            248,
            280,
            26,
            hwnd,
            IDC_NAME,
            inst_h,
            false,
        );
        label("Daily hours", 324, 228, hwnd, inst_h);
        state.hours = edit(&hours_s, 324, 248, 134, 26, hwnd, IDC_HOURS, inst_h, false);

        label("Authenticator secret (if you cannot scan)", 28, 292, hwnd, inst_h);
        state.secret_box = edit("", 28, 312, 430, 26, hwnd, IDC_SECRET, inst_h, true);

        state.status = ctrl(
            w!("STATIC"),
            w!("Scan the QR in Google Authenticator, then enter the 6-digit code."),
            WS_CHILD | WS_VISIBLE,
            Default::default(),
            28,
            360,
            430,
            72,
            hwnd,
            IDC_STATUS,
            inst_h,
        );

        state.qr = CreateWindowExW(
            WS_EX_CLIENTEDGE,
            PCWSTR(qr_class.as_ptr()),
            w!(""),
            WS_CHILD | WS_VISIBLE,
            500,
            108,
            QR_BOX,
            QR_BOX,
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
            500,
            364,
            290,
            36,
            hwnd,
            IDC_ACCOUNT,
            inst_h,
        );
        label("6-digit code", 500, 404, hwnd, inst_h);
        state.code = edit("", 500, 424, 140, 32, hwnd, IDC_CODE, inst_h, false);
        SendMessageW(
            state.code,
            0x00C5,
            windows::Win32::Foundation::WPARAM(6),
            windows::Win32::Foundation::LPARAM(0),
        );

        let cancel = ctrl(
            w!("BUTTON"),
            w!("Cancel"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            Default::default(),
            500,
            478,
            120,
            32,
            hwnd,
            IDCANCEL,
            inst_h,
        );
        let ok = ctrl(
            w!("BUTTON"),
            w!("Register this PC"),
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(
                    windows::Win32::UI::WindowsAndMessaging::BS_DEFPUSHBUTTON as u32,
                ),
            Default::default(),
            632,
            478,
            156,
            32,
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
        setf(state.token, font);
        setf(state.chat, font);
        setf(state.name, font);
        setf(state.hours, font);
        setf(state.secret_box, font_small);
        setf(state.status, font_small);
        setf(state.account, font_small);
        setf(state.code, font_code);
        setf(cancel, font);
        setf(ok, font);
        apply_label_fonts(hwnd, font_small);
        setf(title, font_title);
        setf(sub, font_small);
    }

    rebuild_qr(&mut state);
    unsafe {
        set_text(state.secret_box, &state.secret);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(state.token);
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
            430,
            18,
            parent,
            0,
            inst,
        );
    }
}

#[cfg(windows)]
fn edit(
    text: &str,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    parent: windows::Win32::Foundation::HWND,
    id: i32,
    inst: windows::Win32::Foundation::HINSTANCE,
    readonly: bool,
) -> windows::Win32::Foundation::HWND {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{
        ES_AUTOHSCROLL, ES_READONLY, WINDOW_STYLE, WS_CHILD, WS_TABSTOP, WS_VISIBLE,
        WS_EX_CLIENTEDGE,
    };
    let t = wide(text);
    let mut style = WS_CHILD | WS_VISIBLE | WINDOW_STYLE(ES_AUTOHSCROLL as u32);
    if !readonly {
        style |= WS_TABSTOP;
    } else {
        style |= WINDOW_STYLE(ES_READONLY as u32);
    }
    unsafe {
        ctrl(
            windows::core::w!("EDIT"),
            PCWSTR(t.as_ptr()),
            style,
            WS_EX_CLIENTEDGE,
            x,
            y,
            w,
            h,
            parent,
            id,
            inst,
        )
    }
}

#[cfg(windows)]
fn rebuild_qr(state: &mut EnrollState) {
    use windows::Win32::Graphics::Gdi::{DeleteObject, InvalidateRect, HGDIOBJ};
    let name = get_text(state.name);
    let name = if name.is_empty() { "employee".into() } else { name };
    let url = totp::otpauth_url(&name, &state.machine, &state.secret);
    set_text(
        state.account,
        &format!("Google Authenticator account\nFtracker:{name}@{}", state.machine),
    );
    let Ok((dim, bgra)) = totp::qr_bgra(&url) else {
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
    let token = get_text(state.token);
    let chat = get_text(state.chat);
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
                MUTED
            } else {
                TEXT
            };
            SetTextColor(hdc, windows::Win32::Foundation::COLORREF(color));
            if let Some(s) = state {
                return LRESULT(s.bg.0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
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
        BeginPaint, BitBlt, CreateCompatibleDC, DeleteDC, EndPaint, FillRect, SelectObject,
        HBRUSH, SRCCOPY,
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
    let white = windows::Win32::Graphics::Gdi::GetStockObject(
        windows::Win32::Graphics::Gdi::WHITE_BRUSH,
    );
    FillRect(hdc, &rc, HBRUSH(white.0));
    let parent = GetParent(hwnd);
    let ptr = get_user_data(parent);
    if ptr != 0 {
        let state = &*(ptr as *mut EnrollState);
        if state.qr_bmp.0 != 0 {
            let mem = CreateCompatibleDC(hdc);
            let old = SelectObject(mem, windows::Win32::Graphics::Gdi::HGDIOBJ(state.qr_bmp.0));
            let x = ((rc.right - state.qr_dim) / 2).max(0);
            let y = ((rc.bottom - state.qr_dim) / 2).max(0);
            let _ = BitBlt(hdc, x, y, state.qr_dim, state.qr_dim, mem, 0, 0, SRCCOPY);
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
