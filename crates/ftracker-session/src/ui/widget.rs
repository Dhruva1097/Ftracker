use crate::app::SessionState;

#[cfg(windows)]
use windows::Win32::Foundation::HWND;

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
pub fn run_message_loop(state: &mut SessionState) -> anyhow::Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::Graphics::Gdi::{GetStockObject, HBRUSH, WHITE_BRUSH};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DispatchMessageW, GetMessageW, LoadCursorW, RegisterClassW, SetTimer,
        SetWindowPos, ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW, HWND_TOPMOST, IDC_ARROW,
        MSG, SWP_NOACTIVATE, SWP_NOSIZE, SW_SHOWNOACTIVATE,
        WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
    };

    let class: Vec<u16> = "FtrackerChip\0".encode_utf16().collect();
    let inst = unsafe { GetModuleHandleW(None)? };
    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wndproc),
        hInstance: windows::Win32::Foundation::HINSTANCE(inst.0),
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW)? },
        hbrBackground: HBRUSH(unsafe { GetStockObject(WHITE_BRUSH) }.0),
        lpszClassName: PCWSTR(class.as_ptr()),
        ..Default::default()
    };
    unsafe { RegisterClassW(&wc) };

    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            PCWSTR(class.as_ptr()),
            windows::core::w!("Ftracker"),
            WS_POPUP | WS_VISIBLE,
            20,
            20,
            168,
            32,
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
        set_user_data(hwnd, state as *mut SessionState as isize);
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, 20, 20, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
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
        BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, SetBkMode, SetTextColor,
        TextOutW, HBRUSH, TRANSPARENT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, GetClientRect, PostQuitMessage, WM_DESTROY, WM_ENDSESSION, WM_LBUTTONUP,
        WM_PAINT, WM_QUERYENDSESSION, WM_RBUTTONUP, WM_TIMER,
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
            let brush = CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x00221A12));
            FillRect(hdc, &rc, brush);
            let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(brush.0));
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, windows::Win32::Foundation::COLORREF(0x00E8EEF4));
            if let Some(s) = state {
                s.on_paint();
                let text: Vec<u16> = s.chip_label.encode_utf16().collect();
                let _ = TextOutW(hdc, 10, 8, &text);
                let _ = HBRUSH;
            }
            EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        x if x == WM_TIMER => {
            if let Some(s) = state {
                s.on_timer();
                let _ = windows::Win32::Graphics::Gdi::InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }
        x if x == WM_LBUTTONUP || x == WM_RBUTTONUP => {
            if let Some(s) = state {
                s.open_action();
            }
            LRESULT(0)
        }
        x if x == WM_QUERYENDSESSION => {
            if let Some(s) = state {
                s.flush_for_shutdown();
            }
            LRESULT(1)
        }
        x if x == WM_ENDSESSION => {
            if wparam.0 != 0 {
                if let Some(s) = state {
                    s.flush_for_shutdown();
                }
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        x if x == WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[cfg(not(windows))]
pub fn run_message_loop(_state: &mut SessionState) -> anyhow::Result<()> {
    anyhow::bail!("windows only")
}
