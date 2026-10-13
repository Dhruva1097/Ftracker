use std::sync::atomic::{AtomicBool, Ordering};

use ftracker_core::config::{windows_user, AppConfig};
use ftracker_core::domain::time::format_hm;
use ftracker_core::report::day_totals;
use ftracker_core::telegram::client::TelegramClient;
use ftracker_core::telegram::messages;
use ftracker_core::GeoFix;
use ftracker_core::Store;

use super::dpi;
use super::theme;

const IDOK: i32 = 1;
const IDCANCEL: i32 = 2;
const IDC_TODAY: i32 = 2002;
const IDC_REMAIN: i32 = 2003;
const IDC_RECENT: i32 = 2005;
const IDC_SUBSTATUS: i32 = 2007;
const IDC_Q: i32 = 2008;
const IDC_CHAT: i32 = 2010;
const IDC_HOWTO: i32 = 2011;
const IDC_ERR: i32 = 2012;

const WIN_W: i32 = 540;
const WIN_H: i32 = 620;

static OPEN: AtomicBool = AtomicBool::new(false);

const HOWTO: &str = "Your chat id is a number Telegram gives you (example: 123456789).\n\n\
How to get it:\n\
1. Open Telegram on your phone or PC.\n\
2. In Search, type @userinfobot and open it.\n\
3. Tap Start. It replies with your Id number.\n\
4. Copy that number and paste it in the box above.\n\
5. Also open the Ftracker bot and tap Start so it can message you.";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
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
struct ActionState<'a> {
    cfg: &'a AppConfig,
    store: &'a Store,
    client: &'a TelegramClient,
    loc: Option<GeoFix>,
    subscribed: bool,
    chat: windows::Win32::Foundation::HWND,
    err: windows::Win32::Foundation::HWND,
    btn_yes: windows::Win32::Foundation::HWND,
    btn_no: windows::Win32::Foundation::HWND,
    bg: windows::Win32::Graphics::Gdi::HBRUSH,
    white: windows::Win32::Graphics::Gdi::HBRUSH,
    font_btn: windows::Win32::Graphics::Gdi::HFONT,
    hover_yes: bool,
    hover_no: bool,
    finished: bool,
    yes_label: String,
    no_label: String,
}

fn valid_chat_id(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() || t == "-" {
        return false;
    }
    t.chars()
        .enumerate()
        .all(|(i, c)| c.is_ascii_digit() || (i == 0 && c == '-'))
}

pub fn open(cfg: &AppConfig, store: &Store, loc: Option<&GeoFix>, client: &TelegramClient) {
    if OPEN.swap(true, Ordering::SeqCst) {
        return;
    }
    #[cfg(windows)]
    run(cfg, store, loc, client);
    OPEN.store(false, Ordering::SeqCst);
}

#[cfg(windows)]
fn run(cfg: &AppConfig, store: &Store, loc: Option<&GeoFix>, client: &TelegramClient) {
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Gdi::{CreateFontW, CreateSolidBrush, DeleteObject, HGDIOBJ, HFONT};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Controls::{InitCommonControlsEx, ICC_WIN95_CLASSES, INITCOMMONCONTROLSEX};
    use windows::Win32::UI::WindowsAndMessaging::{
        AdjustWindowRect, CreateWindowExW, DispatchMessageW, GetMessageW, GetSystemMetrics,
        IsDialogMessageW, LoadCursorW, RegisterClassW, SendMessageW, ShowWindow, TranslateMessage,
        CS_HREDRAW, CS_VREDRAW, IDC_ARROW, MSG, SM_CXSCREEN, SM_CYSCREEN, SW_SHOW, WINDOW_STYLE,
        WM_SETFONT, WNDCLASSW, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN,
        WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, WS_EX_APPWINDOW, WS_EX_CONTROLPARENT,
    };

    let _ = unsafe {
        InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_WIN95_CLASSES,
        })
    };

    let inst = match unsafe { GetModuleHandleW(None) } {
        Ok(h) => h,
        Err(_) => return,
    };
    let cursor = match unsafe { LoadCursorW(None, IDC_ARROW) } {
        Ok(c) => c,
        Err(_) => return,
    };
    let bg = unsafe { CreateSolidBrush(windows::Win32::Foundation::COLORREF(theme::BG)) };
    let white = unsafe { CreateSolidBrush(windows::Win32::Foundation::COLORREF(theme::SURFACE)) };

    let class: Vec<u16> = "FtrackerAction\0".encode_utf16().collect();
    let field_class: Vec<u16> = "FtrackerField\0".encode_utf16().collect();
    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wndproc),
        hInstance: windows::Win32::Foundation::HINSTANCE(inst.0),
        hCursor: cursor,
        hIcon: crate::ui::logo::large(),
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
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_APPWINDOW | WS_EX_CONTROLPARENT,
            PCWSTR(class.as_ptr()),
            w!("Ftracker"),
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
            let _ = DeleteObject(HGDIOBJ(white.0));
        }
        return;
    }
    crate::ui::logo::apply(hwnd);

    let req = day_totals::required_seconds(cfg.daily_hours_required);
    let today = day_totals::today_iso();
    let today_s = store.day_counted(&today).unwrap_or(0);
    let need = (req - today_s).max(0);
    let today_txt = format!("{} of {} required", format_hm(today_s), format_hm(req));
    let remain_txt = if need == 0 {
        "Required hours met today.".into()
    } else {
        format!("Still need {} today.", format_hm(need))
    };
    let lines = day_totals::action_window_lines(store, cfg.daily_hours_required);
    let recent = {
        let body: Vec<_> = lines.iter().skip(1).take(6).cloned().collect();
        if body.is_empty() {
            "No earlier days stored yet.".into()
        } else {
            body.join("\n")
        }
    };
    let user = windows_user();
    let sub = store.get_subscriber(&user).ok().flatten();
    let subscribed = sub.as_ref().map(|s| !s.is_empty()).unwrap_or(false);
    let sub_txt = match &sub {
        Some(id) if id.len() > 4 => format!(
            "You already receive a daily Telegram summary (chat …{}).",
            &id[id.len() - 4..]
        ),
        Some(_) => "You already receive a daily Telegram summary.".into(),
        None => "Not subscribed. You do not get a daily Telegram message.".into(),
    };
    let question = if subscribed {
        "Keep receiving daily updates?"
    } else {
        "Do you want a Telegram message each day with your tracked time?"
    };
    let chat_label = if subscribed {
        "New Telegram chat id (only if you want to change it)"
    } else {
        "Your Telegram chat id"
    };
    let yes_label = if subscribed {
        "Yes, keep them".into()
    } else {
        "Yes, send daily updates".into()
    };
    let no_label = if subscribed {
        "No, stop them".into()
    } else {
        "No thanks".into()
    };
    let how_txt = if subscribed {
        "To change where the daily message goes: get a new chat id with @userinfobot (same steps as below), paste it, then tap Yes.\n\n\
How to get a chat id:\n\
1. Open Telegram. Search @userinfobot and open it.\n\
2. Tap Start. Copy the Id number it shows.\n\
3. Open the Ftracker bot and tap Start so it can message you."
            .into()
    } else {
        HOWTO.to_string()
    };

    let font = unsafe { CreateFontW(-dpi::px(16), 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, w!("Segoe UI")) };
    let font_title = unsafe { CreateFontW(-dpi::px(22), 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, 5, 0, w!("Segoe UI")) };
    let font_num = unsafe { CreateFontW(-dpi::px(20), 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, 5, 0, w!("Segoe UI")) };
    let font_small = unsafe { CreateFontW(-dpi::px(13), 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, w!("Segoe UI")) };
    let font_btn = unsafe { CreateFontW(-dpi::px(15), 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, 5, 0, w!("Segoe UI")) };

    let mut state = ActionState {
        cfg,
        store,
        client,
        loc: loc.cloned(),
        subscribed,
        chat: HWND(0),
        err: HWND(0),
        btn_yes: HWND(0),
        btn_no: HWND(0),
        bg,
        white,
        font_btn,
        hover_yes: false,
        hover_no: false,
        finished: false,
        yes_label,
        no_label,
    };
    unsafe { set_user_data(hwnd, &mut state as *mut ActionState as isize) };

    let inst_h = windows::Win32::Foundation::HINSTANCE(inst.0);
    let p = dpi::px;
    let wrap = WINDOW_STYLE(0x2000); // SS_EDITCONTROL — wrap long lines

    unsafe {
        let title = ctrl(
            w!("STATIC"),
            w!("Today"),
            WS_CHILD | WS_VISIBLE,
            Default::default(),
            p(28),
            p(16),
            p(484),
            p(28),
            hwnd,
            0,
            inst_h,
        );
        let today_h = stat(&today_txt, p(28), p(44), p(484), p(28), hwnd, IDC_TODAY, inst_h, wrap);
        let remain_h = stat(&remain_txt, p(28), p(72), p(484), p(22), hwnd, IDC_REMAIN, inst_h, wrap);
        label("Recent days", p(28), p(104), hwnd, inst_h);
        let recent_h = stat(&recent, p(28), p(124), p(484), p(110), hwnd, IDC_RECENT, inst_h, wrap);
        label("Daily Telegram updates", p(28), p(242), hwnd, inst_h);
        let sub_h = stat(&sub_txt, p(28), p(262), p(484), p(36), hwnd, IDC_SUBSTATUS, inst_h, wrap);
        let q_h = stat(question, p(28), p(300), p(484), p(36), hwnd, IDC_Q, inst_h, wrap);
        label(chat_label, p(28), p(340), hwnd, inst_h);
        state.chat = field("", p(28), p(360), p(484), p(40), hwnd, IDC_CHAT, inst_h);
        let how_h = stat(&how_txt, p(28), p(408), p(484), p(118), hwnd, IDC_HOWTO, inst_h, wrap);
        state.err = stat("", p(28), p(528), p(484), p(22), hwnd, IDC_ERR, inst_h, wrap);

        let owner = windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(
            windows::Win32::UI::WindowsAndMessaging::BS_OWNERDRAW as u32,
        );
        let by = p(556);
        let yes_w = p(220);
        let no_w = p(150);
        let no_wstr = wide(&state.no_label);
        let yes_wstr = wide(&state.yes_label);
        state.btn_no = ctrl(
            w!("BUTTON"),
            PCWSTR(no_wstr.as_ptr()),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | owner,
            Default::default(),
            p(28),
            by,
            no_w,
            p(40),
            hwnd,
            IDCANCEL,
            inst_h,
        );
        state.btn_yes = ctrl(
            w!("BUTTON"),
            PCWSTR(yes_wstr.as_ptr()),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | owner,
            Default::default(),
            p(WIN_W) - p(28) - yes_w,
            by,
            yes_w,
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
        setf(today_h, font_num);
        setf(remain_h, font_small);
        setf(recent_h, font_small);
        setf(sub_h, font_small);
        setf(q_h, font);
        setf(state.chat, font);
        setf(how_h, font_small);
        setf(state.err, font_small);
        setf(state.btn_yes, font_btn);
        setf(state.btn_no, font_btn);
        apply_label_fonts(hwnd, font_small);
        setf(title, font_title);
        setf(today_h, font_num);
        setf(q_h, font);

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(state.chat);
        let _ = windows::Win32::UI::WindowsAndMessaging::SetTimer(hwnd, 1, 40, None);
    }

    let mut msg = MSG::default();
    while !state.finished {
        let ret = unsafe { GetMessageW(&mut msg, HWND(0), 0, 0) };
        if ret.0 == 0 {
            state.finished = true;
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0);
            }
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
        let _ = DeleteObject(HGDIOBJ(font.0));
        let _ = DeleteObject(HGDIOBJ(font_title.0));
        let _ = DeleteObject(HGDIOBJ(font_num.0));
        let _ = DeleteObject(HGDIOBJ(font_small.0));
        let _ = DeleteObject(HGDIOBJ(font_btn.0));
        let _ = DeleteObject(HGDIOBJ(white.0));
        let _ = DeleteObject(HGDIOBJ(bg.0));
    }
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
fn stat(
    text: &str,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    parent: windows::Win32::Foundation::HWND,
    id: i32,
    inst: windows::Win32::Foundation::HINSTANCE,
    style: windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE,
) -> windows::Win32::Foundation::HWND {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{WS_CHILD, WS_VISIBLE};
    let t = wide(text);
    unsafe {
        ctrl(
            windows::core::w!("STATIC"),
            PCWSTR(t.as_ptr()),
            WS_CHILD | WS_VISIBLE | style,
            Default::default(),
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
            dpi::px(484),
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
) -> windows::Win32::Foundation::HWND {
    use windows::core::{w, PCWSTR};
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageW, ES_AUTOHSCROLL, WINDOW_STYLE, WS_CHILD, WS_TABSTOP, WS_VISIBLE,
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
    let edit = unsafe {
        ctrl(
            w!("EDIT"),
            PCWSTR(t.as_ptr()),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
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
fn update_hover(hwnd: windows::Win32::Foundation::HWND, state: &mut ActionState) {
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
    let over_yes = child.0 == state.btn_yes.0;
    let over_no = child.0 == state.btn_no.0;
    if over_yes != state.hover_yes {
        state.hover_yes = over_yes;
        unsafe {
            let _ = InvalidateRect(state.btn_yes, None, true);
        }
    }
    if over_no != state.hover_no {
        state.hover_no = over_no;
        unsafe {
            let _ = InvalidateRect(state.btn_no, None, true);
        }
    }
}

#[cfg(windows)]
fn draw_button(state: &ActionState, dis: &windows::Win32::UI::Controls::DRAWITEMSTRUCT) {
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
        state.hover_yes
    } else {
        state.hover_no
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
        let label = if primary {
            state.yes_label.as_str()
        } else {
            state.no_label.as_str()
        };
        let text: Vec<u16> = label.encode_utf16().collect();
        let mut sz = windows::Win32::Foundation::SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &text, &mut sz);
        let tx = rc.left + (rc.right - rc.left - sz.cx) / 2;
        let ty = rc.top + (rc.bottom - rc.top - sz.cy) / 2;
        let _ = TextOutW(hdc, tx, ty, &text);
    }
}

#[cfg(windows)]
fn close_dialog(hwnd: windows::Win32::Foundation::HWND, state: &mut ActionState) {
    use windows::Win32::UI::WindowsAndMessaging::DestroyWindow;
    state.finished = true;
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

#[cfg(windows)]
fn on_yes(hwnd: windows::Win32::Foundation::HWND, state: &mut ActionState) {
    let chat = get_text(state.chat);
    if state.subscribed {
        if chat.is_empty() {
            close_dialog(hwnd, state);
            return;
        }
        if !valid_chat_id(&chat) {
            set_text(
                state.err,
                "Chat id must be the number from @userinfobot, with no spaces.",
            );
            return;
        }
        if !try_subscribe(state, &chat, true) {
            return;
        }
        close_dialog(hwnd, state);
        return;
    }
    if !valid_chat_id(&chat) {
        set_text(
            state.err,
            "Paste your chat id from @userinfobot first, then tap Yes.",
        );
        return;
    }
    if try_subscribe(state, &chat, false) {
        close_dialog(hwnd, state);
    }
}

#[cfg(windows)]
fn on_no(hwnd: windows::Win32::Foundation::HWND, state: &mut ActionState) {
    if state.subscribed {
        let user = windows_user();
        let loc = state.loc.as_ref();
        let _ = state.store.clear_subscriber(&user);
        let _ = state.store.persist();
        let admin = messages::employee_unsubscribed_admin(&state.cfg.machine_id, &user, loc);
        let _ = state
            .client
            .send_admin(&state.cfg.telegram_admin_chat_id, &admin, loc);
    }
    close_dialog(hwnd, state);
}

#[cfg(windows)]
fn try_subscribe(state: &mut ActionState, chat: &str, change: bool) -> bool {
    let user = windows_user();
    let loc = state.loc.as_ref();
    if state
        .client
        .send_employee(chat, &messages::employee_subscribed())
        .is_err()
    {
        set_text(
            state.err,
            "Could not message that chat. Open the Ftracker bot, tap Start, then try again.",
        );
        return false;
    }
    let _ = state.store.set_subscriber(&user, chat);
    let _ = state.store.persist();
    let admin = if change {
        messages::employee_changed_admin(&state.cfg.machine_id, &user, loc)
    } else {
        messages::employee_subscribed_admin(&state.cfg.machine_id, &user, loc)
    };
    let _ = state
        .client
        .send_admin(&state.cfg.telegram_admin_chat_id, &admin, loc);
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
        DefWindowProcW, DestroyWindow, GetDlgCtrlID, WM_CLOSE, WM_COMMAND, WM_CTLCOLORSTATIC,
        WM_DESTROY,
    };
    let ptr = get_user_data(hwnd);
    let state = if ptr != 0 {
        Some(&mut *(ptr as *mut ActionState))
    } else {
        None
    };
    match msg {
        x if x == WM_CTLCOLORSTATIC => {
            let hdc = HDC(wparam.0 as isize);
            SetBkMode(hdc, TRANSPARENT);
            let child = HWND(lparam.0);
            let id = GetDlgCtrlID(child);
            let color = if id == IDC_TODAY {
                theme::PRIMARY
            } else if id == IDC_REMAIN
                || id == IDC_RECENT
                || id == IDC_SUBSTATUS
                || id == IDC_HOWTO
                || id == IDC_ERR
            {
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
            use windows::Win32::Graphics::Gdi::{
                BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, HGDIOBJ,
            };
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
            if let Some(s) = state {
                if id == IDOK as u32 {
                    on_yes(hwnd, s);
                } else if id == IDCANCEL as u32 {
                    on_no(hwnd, s);
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

#[cfg(not(windows))]
fn run(_cfg: &AppConfig, _store: &Store, _loc: Option<&GeoFix>, _client: &TelegramClient) {}
