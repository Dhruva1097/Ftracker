use serde::{Deserialize, Serialize};

use crate::domain::location::GeoFix;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedSend {
    pub chat_id: String,
    pub text: String,
    pub loc: Option<GeoFix>,
    pub employee_only: bool,
}

impl QueuedSend {
    pub fn admin(chat_id: impl Into<String>, text: impl Into<String>, loc: Option<GeoFix>) -> Self {
        Self {
            chat_id: chat_id.into(),
            text: text.into(),
            loc,
            employee_only: false,
        }
    }

    pub fn employee(chat_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            chat_id: chat_id.into(),
            text: text.into(),
            loc: None,
            employee_only: true,
        }
    }
}
