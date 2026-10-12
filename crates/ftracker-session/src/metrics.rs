#[derive(Debug, Clone)]
pub struct HostMetrics {
    pub cpu_pct: u32,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
    pub disk_root: String,
    pub disk_free_gb: f64,
    pub disk_total_gb: f64,
}

impl Default for HostMetrics {
    fn default() -> Self {
        Self {
            cpu_pct: 0,
            ram_used_mb: 0,
            ram_total_mb: 0,
            disk_root: "C:".into(),
            disk_free_gb: 0.0,
            disk_total_gb: 0.0,
        }
    }
}

pub fn collect() -> HostMetrics {
    #[cfg(windows)]
    {
        collect_win()
    }
    #[cfg(not(windows))]
    {
        HostMetrics::default()
    }
}

#[cfg(windows)]
fn collect_win() -> HostMetrics {
    let mut m = HostMetrics::default();
    m.cpu_pct = cpu_pct();
    ram(&mut m);
    disk(&mut m);
    m
}

#[cfg(windows)]
fn ft_u64(ft: windows::Win32::Foundation::FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64
}

#[cfg(windows)]
fn cpu_times() -> Option<(u64, u64)> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::GetSystemTimes;
    let mut idle = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }.ok()?;
    let idle_t = ft_u64(idle);
    let total = ft_u64(kernel).saturating_add(ft_u64(user));
    Some((idle_t, total))
}

#[cfg(windows)]
fn cpu_pct() -> u32 {
    use std::sync::Mutex;
    use std::time::Instant;
    struct Sample {
        idle: u64,
        total: u64,
        at: Instant,
        pct: u32,
    }
    static LAST: Mutex<Option<Sample>> = Mutex::new(None);
    let now = match cpu_times() {
        Some(v) => v,
        None => return 0,
    };
    let mut g = match LAST.lock() {
        Ok(g) => g,
        Err(_) => return 0,
    };
    if let Some(prev) = g.as_ref() {
        let dt_wall = prev.at.elapsed();
        if dt_wall < std::time::Duration::from_millis(80) {
            return prev.pct;
        }
        if dt_wall < std::time::Duration::from_secs(3) {
            let pct = cpu_from_delta(prev.idle, prev.total, now.0, now.1);
            *g = Some(Sample {
                idle: now.0,
                total: now.1,
                at: Instant::now(),
                pct,
            });
            return pct;
        }
    }
    drop(g);
    std::thread::sleep(std::time::Duration::from_millis(40));
    let later = match cpu_times() {
        Some(v) => v,
        None => return 0,
    };
    let pct = cpu_from_delta(now.0, now.1, later.0, later.1);
    if let Ok(mut g) = LAST.lock() {
        *g = Some(Sample {
            idle: later.0,
            total: later.1,
            at: Instant::now(),
            pct,
        });
    }
    pct
}

#[cfg(windows)]
fn cpu_from_delta(i0: u64, t0: u64, i1: u64, t1: u64) -> u32 {
    let di = i1.saturating_sub(i0);
    let dt = t1.saturating_sub(t0);
    if dt == 0 {
        return 0;
    }
    let busy = dt.saturating_sub(di);
    ((busy.saturating_mul(100)) / dt).min(100) as u32
}

#[cfg(windows)]
fn ram(m: &mut HostMetrics) {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut mem = MEMORYSTATUSEX::default();
    mem.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut mem) }.is_err() {
        return;
    }
    m.ram_total_mb = mem.ullTotalPhys / (1024 * 1024);
    let avail = mem.ullAvailPhys / (1024 * 1024);
    m.ram_used_mb = m.ram_total_mb.saturating_sub(avail);
}

#[cfg(windows)]
fn disk(m: &mut HostMetrics) {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    let letter = drive.chars().next().unwrap_or('C');
    let path = format!("{letter}:\\");
    m.disk_root = format!("{letter}:");
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let mut free = 0u64;
    let mut total = 0u64;
    let mut total_free = 0u64;
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(wide.as_ptr()),
            Some(&mut free),
            Some(&mut total),
            Some(&mut total_free),
        )
    };
    if ok.is_err() {
        return;
    }
    m.disk_free_gb = free as f64 / (1024.0 * 1024.0 * 1024.0);
    m.disk_total_gb = total as f64 / (1024.0 * 1024.0 * 1024.0);
}

#[cfg(windows)]
pub fn removable_volumes() -> Vec<String> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetLogicalDriveStringsW, GetVolumeInformationW,
    };
    const DRIVE_REMOVABLE: u32 = 2;
    let mut buf = [0u16; 512];
    let n = unsafe { GetLogicalDriveStringsW(Some(&mut buf)) };
    if n == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut i = 0usize;
    let end = (n as usize).min(buf.len());
    while i < end {
        if buf[i] == 0 {
            i += 1;
            continue;
        }
        let start = i;
        while i < end && buf[i] != 0 {
            i += 1;
        }
        let dtype = unsafe { GetDriveTypeW(PCWSTR(buf[start..].as_ptr())) };
        if dtype == DRIVE_REMOVABLE {
            let mut label = [0u16; 64];
            let _ = unsafe {
                GetVolumeInformationW(
                    PCWSTR(buf[start..].as_ptr()),
                    Some(&mut label),
                    None,
                    None,
                    None,
                    None,
                )
            };
            let letter = String::from_utf16_lossy(&buf[start..i]);
            let name_end = label.iter().position(|&c| c == 0).unwrap_or(label.len());
            let name = String::from_utf16_lossy(&label[..name_end]).trim().to_string();
            if name.is_empty() {
                out.push(letter.trim_end_matches('\\').to_string());
            } else {
                out.push(format!("{} {}", letter.trim_end_matches('\\'), name));
            }
        }
        i += 1;
    }
    out
}

#[cfg(not(windows))]
pub fn removable_volumes() -> Vec<String> {
    Vec::new()
}
