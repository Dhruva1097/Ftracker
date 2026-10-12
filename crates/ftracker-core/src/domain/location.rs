use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoFix {
    pub lat: f64,
    pub lon: f64,
    pub accuracy_m: f64,
}

impl GeoFix {
    pub fn fmt(&self) -> String {
        format!("{:.4},{:.4} ±{:.0}m", self.lat, self.lon, self.accuracy_m)
    }

    pub fn maps_url(&self) -> String {
        format!("https://maps.google.com/?q={:.6},{:.6}", self.lat, self.lon)
    }
}

pub fn loc_text(fix: Option<&GeoFix>) -> String {
    match fix {
        Some(f) => format!("loc={}", f.fmt()),
        None => "loc=unavailable".into(),
    }
}

/// Human-readable location block for admin Telegram messages.
pub fn location_lines(fix: Option<&GeoFix>) -> String {
    match fix {
        Some(f) => format!("Location: {}\n{}", f.fmt(), loc_text(Some(f))),
        None => "Location: unavailable".into(),
    }
}
