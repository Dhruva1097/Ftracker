use std::collections::HashSet;

use super::foreground::process_file_name;

/// Unique process names of visible top-level windows, capped.
pub fn process_names(limit: usize) -> Vec<String> {
    #[cfg(windows)]
    {
        process_names_win(limit)
    }
    #[cfg(not(windows))]
    {
        let _ = limit;
        Vec::new()
    }
}

#[cfg(windows)]
struct Acc {
    names: Vec<String>,
    seen: HashSet<String>,
    limit: usize,
}

#[cfg(windows)]
fn process_names_win(limit: usize) -> Vec<String> {
    use windows::Win32::Foundation::LPARAM;
    use windows::Win32::UI::WindowsAndMessaging::EnumWindows;

    let mut acc = Acc {
        names: Vec::new(),
        seen: HashSet::new(),
        limit,
    };
    unsafe {
        let _ = EnumWindows(Some(enum_cb), LPARAM(&mut acc as *mut Acc as isize));
    }
    acc.names
}

#[cfg(windows)]
unsafe extern "system" fn enum_cb(
    hwnd: windows::Win32::Foundation::HWND,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::BOOL {
    use windows::Win32::Foundation::{CloseHandle, BOOL, MAX_PATH};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindow, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, GW_OWNER,
    };

    let acc = &mut *(lparam.0 as *mut Acc);
    if acc.names.len() >= acc.limit {
        return BOOL(0);
    }
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }
    let owner = GetWindow(hwnd, GW_OWNER);
    if owner.0 != 0 {
        return BOOL(1);
    }
    let mut title = [0u16; 256];
    if GetWindowTextW(hwnd, &mut title) <= 0 {
        return BOOL(1);
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 {
        return BOOL(1);
    }
    let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
        return BOOL(1);
    };
    let mut buf = [0u16; MAX_PATH as usize];
    let mut len = buf.len() as u32;
    let ok = QueryFullProcessImageNameW(
        h,
        PROCESS_NAME_WIN32,
        windows::core::PWSTR(buf.as_mut_ptr()),
        &mut len,
    );
    let _ = CloseHandle(h);
    if ok.is_err() {
        return BOOL(1);
    }
    let path = String::from_utf16_lossy(&buf[..len as usize]);
    let name = process_file_name(&path);
    if name.is_empty() || !acc.seen.insert(name.to_ascii_lowercase()) {
        return BOOL(1);
    }
    acc.names.push(name);
    BOOL(1)
}
