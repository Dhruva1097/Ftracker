use ftracker_core::GeoFix;

/// Win7+ ILocation COM. If the location platform is off, returns None (loc=unavailable).
pub fn current() -> Option<GeoFix> {
    #[cfg(windows)]
    {
        ilocation_fix()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
fn ilocation_fix() -> Option<GeoFix> {
    use windows::core::{ComInterface, GUID};
    use windows::Win32::Devices::Geolocation::{ILatLongReport, ILocation, ILocationReport, Location};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    };
    // IID_LatLongReport
    const IID_LATLONG: GUID = GUID::from_u128(0x7fed806d_0ef8_4f07_80ac_36a0be16bb3b);
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let loc: ILocation = CoCreateInstance(&Location, None, CLSCTX_INPROC_SERVER).ok()?;
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
