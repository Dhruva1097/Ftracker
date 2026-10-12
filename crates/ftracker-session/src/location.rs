use std::sync::Mutex;
use std::time::{Duration, Instant};

use ftracker_core::GeoFix;

struct Cache {
    at: Instant,
    fix: GeoFix,
}

static LAST: Mutex<Option<Cache>> = Mutex::new(None);

/// Win7+ ILocation after asking permission. If the OS has no fix, city-level IP fallback.
pub fn current() -> Option<GeoFix> {
    if let Some(fix) = ilocation_fix() {
        remember(fix.clone());
        return Some(fix);
    }
    if let Ok(g) = LAST.lock() {
        if let Some(c) = g.as_ref() {
            if c.at.elapsed() < Duration::from_secs(30 * 60) {
                return Some(c.fix.clone());
            }
        }
    }
    if let Some(fix) = ip_fix() {
        remember(fix.clone());
        return Some(fix);
    }
    if let Ok(g) = LAST.lock() {
        return g.as_ref().map(|c| c.fix.clone());
    }
    None
}

fn remember(fix: GeoFix) {
    if let Ok(mut g) = LAST.lock() {
        *g = Some(Cache {
            at: Instant::now(),
            fix,
        });
    }
}

/// Modal Windows Location consent (Win7+ ILocation). Call once at process start.
pub fn ask_os() {
    #[cfg(windows)]
    {
        ask_os_win();
    }
}

#[cfg(windows)]
fn ask_os_win() {
    use windows::core::GUID;
    use windows::Win32::Devices::Geolocation::{ILocation, Location};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    };
    const IID_LATLONG: GUID = GUID::from_u128(0x7fed806d_0ef8_4f07_80ac_36a0be16bb3b);
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let Ok(loc) = CoCreateInstance::<_, ILocation>(&Location, None, CLSCTX_INPROC_SERVER) else {
            return;
        };
        let _ = loc.RequestPermissions(HWND(0), &[IID_LATLONG], true);
        let _ = loc.SetReportInterval(&IID_LATLONG, 2000);
    }
}

fn ilocation_fix() -> Option<GeoFix> {
    #[cfg(windows)]
    {
        ilocation_win()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
fn ilocation_win() -> Option<GeoFix> {
    use windows::core::{ComInterface, GUID};
    use windows::Win32::Devices::Geolocation::{ILatLongReport, ILocation, ILocationReport, Location};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    };
    const IID_LATLONG: GUID = GUID::from_u128(0x7fed806d_0ef8_4f07_80ac_36a0be16bb3b);
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let loc: ILocation = CoCreateInstance(&Location, None, CLSCTX_INPROC_SERVER).ok()?;
        let _ = loc.SetReportInterval(&IID_LATLONG, 2000);
        let report: ILocationReport = loc.GetReport(&IID_LATLONG).ok()?;
        let latlong: ILatLongReport = ComInterface::cast(&report).ok()?;
        let lat = latlong.GetLatitude().ok()?;
        let lon = latlong.GetLongitude().ok()?;
        let acc = latlong.GetErrorRadius().unwrap_or(0.0);
        if !lat.is_finite() || !lon.is_finite() {
            return None;
        }
        Some(GeoFix {
            lat,
            lon,
            accuracy_m: acc,
        })
    }
}

/// City-level only. Used when Windows Location is off or has no GPS/Wi-Fi fix.
fn ip_fix() -> Option<GeoFix> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(8))
        .use_rustls_tls()
        .build()
        .ok()?;
    let v: serde_json::Value = client.get("https://ipwho.is/").send().ok()?.json().ok()?;
    if v.get("success").and_then(|x| x.as_bool()) == Some(false) {
        return None;
    }
    let lat = v.get("latitude").and_then(|x| x.as_f64())?;
    let lon = v.get("longitude").and_then(|x| x.as_f64())?;
    if !lat.is_finite() || !lon.is_finite() {
        return None;
    }
    Some(GeoFix {
        lat,
        lon,
        accuracy_m: 25_000.0,
    })
}
