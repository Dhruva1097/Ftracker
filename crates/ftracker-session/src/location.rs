use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use ftracker_core::GeoFix;

/// Skip ISP/IP-sized blobs. Wi-Fi/GPS fixes are typically tens of metres.
const MAX_ACCURACY_M: f64 = 1_500.0;

struct Cache {
    at: Instant,
    fix: GeoFix,
}

static LAST: Mutex<Option<Cache>> = Mutex::new(None);
static BUSY: AtomicBool = AtomicBool::new(false);

/// Latest precise fix. Kicks a background refresh when the cache is stale.
pub fn current() -> Option<GeoFix> {
    let cached = LAST.lock().ok().and_then(|g| g.as_ref().map(|c| (c.at, c.fix.clone())));
    let stale = match &cached {
        None => true,
        Some((at, _)) => at.elapsed() > Duration::from_secs(45),
    };
    if stale {
        kick_refresh();
    }
    cached.map(|(_, f)| f)
}

/// Blocking locate (permission thread / tests). Prefer this after OS consent.
pub fn refresh_now() {
    if let Some(fix) = locate_best() {
        remember(fix);
    }
}

fn kick_refresh() {
    if BUSY
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    let _ = std::thread::Builder::new().name("ft-loc".into()).spawn(|| {
        if let Some(fix) = locate_best() {
            remember(fix);
        }
        BUSY.store(false, Ordering::SeqCst);
    });
}

fn remember(fix: GeoFix) {
    if !precise(&fix) {
        return;
    }
    if let Ok(mut g) = LAST.lock() {
        *g = Some(Cache {
            at: Instant::now(),
            fix,
        });
    }
}

pub fn is_accurate(fix: &GeoFix) -> bool {
    precise(fix)
}

fn precise(fix: &GeoFix) -> bool {
    fix.lat.is_finite()
        && fix.lon.is_finite()
        && fix.lat.abs() <= 90.0
        && fix.lon.abs() <= 180.0
        && (fix.lat != 0.0 || fix.lon != 0.0)
        && fix.accuracy_m > 0.0
        && fix.accuracy_m <= MAX_ACCURACY_M
}

fn locate_best() -> Option<GeoFix> {
    let mut best: Option<GeoFix> = None;
    for cand in [winrt_fix(), wifi_fix(), ilocation_fix()] {
        let Some(fix) = cand.filter(precise) else {
            continue;
        };
        match &best {
            None => best = Some(fix),
            Some(cur) if fix.accuracy_m < cur.accuracy_m => best = Some(fix),
            _ => {}
        }
    }
    best
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
        let _ = loc.SetReportInterval(&IID_LATLONG, 1000);
    }
}

fn winrt_fix() -> Option<GeoFix> {
    #[cfg(windows)]
    {
        winrt_win()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
fn winrt_win() -> Option<GeoFix> {
    use windows::Devices::Geolocation::{Geolocator, PositionAccuracy, PositionSource};
    use windows::Foundation::TimeSpan;
    let geo = Geolocator::new().ok()?;
    let _ = geo.SetDesiredAccuracy(PositionAccuracy::High);
    let max_age = TimeSpan {
        Duration: 10 * 10_000_000,
    };
    let timeout = TimeSpan {
        Duration: 8 * 10_000_000,
    };
    let pos = geo
        .GetGeopositionAsyncWithAgeAndTimeout(max_age, timeout)
        .ok()?
        .get()
        .ok()?;
    let coord = pos.Coordinate().ok()?;
    if let Ok(src) = coord.PositionSource() {
        if src == PositionSource::IPAddress
            || src == PositionSource::Default
            || src == PositionSource::Obfuscated
        {
            return None;
        }
    }
    let p = coord.Point().ok()?.Position().ok()?;
    let mut acc = coord.Accuracy().unwrap_or(0.0);
    if acc < 1.0 {
        acc = 25.0;
    }
    Some(GeoFix {
        lat: p.Latitude,
        lon: p.Longitude,
        accuracy_m: acc,
    })
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
        let _ = loc.SetReportInterval(&IID_LATLONG, 1000);
        let report: ILocationReport = loc.GetReport(&IID_LATLONG).ok()?;
        let latlong: ILatLongReport = ComInterface::cast(&report).ok()?;
        let lat = latlong.GetLatitude().ok()?;
        let lon = latlong.GetLongitude().ok()?;
        let mut acc = latlong.GetErrorRadius().unwrap_or(0.0);
        if acc < 1.0 {
            acc = 50.0;
        }
        Some(GeoFix {
            lat,
            lon,
            accuracy_m: acc,
        })
    }
}

fn wifi_fix() -> Option<GeoFix> {
    let aps = wifi_aps();
    if aps.len() < 2 {
        return None;
    }
    let body = serde_json::json!({
        "wifiAccessPoints": aps.iter().take(16).map(|(mac, rssi)| {
            serde_json::json!({ "macAddress": mac, "signalStrength": rssi })
        }).collect::<Vec<_>>()
    });
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(8))
        .use_rustls_tls()
        .user_agent("Ftracker/1.0")
        .build()
        .ok()?;
    let v: serde_json::Value = client
        .post("https://api.beacondb.net/v1/geolocate")
        .json(&body)
        .send()
        .ok()?
        .json()
        .ok()?;
    let loc = v.get("location")?;
    let lat = loc.get("lat").and_then(|x| x.as_f64())?;
    let lon = loc
        .get("lng")
        .or_else(|| loc.get("lon"))
        .and_then(|x| x.as_f64())?;
    let acc = v.get("accuracy").and_then(|x| x.as_f64()).unwrap_or(75.0);
    Some(GeoFix {
        lat,
        lon,
        accuracy_m: acc.max(5.0),
    })
}

fn wifi_aps() -> Vec<(String, i32)> {
    #[cfg(windows)]
    {
        wifi_aps_win()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

#[cfg(windows)]
fn wifi_aps_win() -> Vec<(String, i32)> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::NetworkManagement::WiFi::{
        dot11_BSS_type_any, WlanCloseHandle, WlanEnumInterfaces, WlanFreeMemory,
        WlanGetNetworkBssList, WlanOpenHandle, WlanScan, WLAN_BSS_LIST, WLAN_INTERFACE_INFO_LIST,
    };
    let mut out = Vec::new();
    unsafe {
        let mut ver = 0u32;
        let mut handle = HANDLE(0);
        if WlanOpenHandle(2, None, &mut ver, &mut handle) != 0 {
            return out;
        }
        let mut ifaces: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
        if WlanEnumInterfaces(handle, None, &mut ifaces) != 0 || ifaces.is_null() {
            let _ = WlanCloseHandle(handle, None);
            return out;
        }
        let n = (*ifaces).dwNumberOfItems;
        for i in 0..n {
            let iface = &*(*ifaces).InterfaceInfo.as_ptr().add(i as usize);
            let guid = iface.InterfaceGuid;
            let _ = WlanScan(handle, &guid, None, None, None);
            std::thread::sleep(Duration::from_millis(900));
            let mut bss: *mut WLAN_BSS_LIST = std::ptr::null_mut();
            if WlanGetNetworkBssList(
                handle,
                &guid,
                None,
                dot11_BSS_type_any,
                false,
                None,
                &mut bss,
            ) != 0
                || bss.is_null()
            {
                continue;
            }
            let count = (*bss).dwNumberOfItems;
            for j in 0..count {
                let e = &*(*bss).wlanBssEntries.as_ptr().add(j as usize);
                // Skip randomized (locally administered) MACs — they do not geolocate.
                if e.dot11Bssid[0] & 0x02 != 0 {
                    continue;
                }
                let mac = format!(
                    "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                    e.dot11Bssid[0],
                    e.dot11Bssid[1],
                    e.dot11Bssid[2],
                    e.dot11Bssid[3],
                    e.dot11Bssid[4],
                    e.dot11Bssid[5]
                );
                out.push((mac, e.lRssi));
            }
            WlanFreeMemory(bss.cast());
        }
        WlanFreeMemory(ifaces.cast());
        let _ = WlanCloseHandle(handle, None);
    }
    out.sort_by_key(|(_, rssi)| -rssi);
    out.dedup_by(|a, b| a.0 == b.0);
    out
}
