use anyhow::{anyhow, Result};
use reqwest::blocking::multipart;
use serde_json::Value;

use crate::domain::location::GeoFix;

#[derive(Clone)]
pub struct TelegramClient {
    token: String,
}

impl TelegramClient {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }

    fn url(&self, method: &str) -> String {
        format!("https://api.telegram.org/bot{}/{method}", self.token)
    }

    fn http() -> Result<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .use_rustls_tls()
            .build()
            .map_err(|e| anyhow!(e))
    }

    pub fn send_message(&self, chat_id: &str, text: &str) -> Result<()> {
        let resp = Self::http()?
            .post(self.url("sendMessage"))
            .form(&[
                ("chat_id", chat_id),
                ("text", text),
                ("disable_web_page_preview", "true"),
            ])
            .send()?;
        let v: Value = resp.json().unwrap_or(Value::Null);
        if v.get("ok").and_then(|x| x.as_bool()) != Some(true) {
            return Err(anyhow!(
                "telegram sendMessage: {}",
                v.get("description")
                    .and_then(|x| x.as_str())
                    .unwrap_or("unknown")
            ));
        }
        Ok(())
    }

    pub fn send_location(&self, chat_id: &str, fix: &GeoFix) -> Result<()> {
        let lat = format!("{:.6}", fix.lat);
        let lon = format!("{:.6}", fix.lon);
        let resp = Self::http()?
            .post(self.url("sendLocation"))
            .form(&[
                ("chat_id", chat_id),
                ("latitude", lat.as_str()),
                ("longitude", lon.as_str()),
            ])
            .send()?;
        let v: Value = resp.json().unwrap_or(Value::Null);
        if v.get("ok").and_then(|x| x.as_bool()) != Some(true) {
            return Err(anyhow!(
                "telegram sendLocation: {}",
                v.get("description")
                    .and_then(|x| x.as_str())
                    .unwrap_or("unknown")
            ));
        }
        Ok(())
    }

    pub fn send_document(&self, chat_id: &str, filename: &str, bytes: &[u8], caption: &str) -> Result<()> {
        let part = multipart::Part::bytes(bytes.to_vec())
            .file_name(filename.to_string())
            .mime_str("application/pdf")
            .unwrap_or_else(|_| multipart::Part::bytes(bytes.to_vec()).file_name(filename.to_string()));
        let form = multipart::Form::new()
            .text("chat_id", chat_id.to_string())
            .text("caption", caption.to_string())
            .part("document", part);
        let resp = Self::http()?
            .post(self.url("sendDocument"))
            .multipart(form)
            .send()?;
        let v: Value = resp.json().unwrap_or(Value::Null);
        if v.get("ok").and_then(|x| x.as_bool()) != Some(true) {
            return Err(anyhow!(
                "telegram sendDocument: {}",
                v.get("description")
                    .and_then(|x| x.as_str())
                    .unwrap_or("unknown")
            ));
        }
        Ok(())
    }

    /// Admin text + maps URL, then native map pin when a fix exists.
    pub fn send_admin(&self, chat_id: &str, text: &str, loc: Option<&GeoFix>) -> Result<()> {
        let mut body = text.to_string();
        if let Some(f) = loc {
            if !body.contains("loc=") {
                body.push('\n');
                body.push_str(&crate::domain::location::loc_text(Some(f)));
            }
            if !body.contains("maps.google.com") {
                body.push('\n');
                body.push_str(&f.maps_url());
            }
        } else if !body.contains("loc=") {
            body.push('\n');
            body.push_str("loc=unavailable");
        }
        self.send_message(chat_id, &body)?;
        if let Some(f) = loc {
            let _ = self.send_location(chat_id, f);
        }
        Ok(())
    }

    /// Employee time-only: never coords or maps.
    pub fn send_employee(&self, chat_id: &str, text: &str) -> Result<()> {
        self.send_message(chat_id, text)
    }
}
