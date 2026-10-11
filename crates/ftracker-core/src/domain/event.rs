use serde::{Deserialize, Serialize};

use super::location::GeoFix;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackEvent {
    pub ts: i64,
    pub kind: String,
    pub process: String,
    pub title: String,
    pub category: String,
    pub counted: bool,
    pub loc: Option<GeoFix>,
    pub extra: String,
}
