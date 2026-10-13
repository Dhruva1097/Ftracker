use anyhow::{anyhow, Result};
use reqwest::blocking::multipart;
use serde_json::Value;

use crate::domain::location::GeoFix;

#[derive(Debug, Clone)]
pub struct TgUpdate {
    pub update_id: i64,
    pub chat_id: String,
    pub text: String,
    pub date: i64,
    pub callback_query_id: Option<String>,
    pub callback_data: Option<String>,
    pub message_id: i64,
}

#[derive(Debug, Clone)]
pub struct InlineBtn {
    pub text: String,
    pub data: String,
}

pub fn inline_markup(rows: &[Vec<InlineBtn>]) -> String {
    let keyboard: Vec<Vec<serde_json::Value>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|b| {
                    serde_json::json!({
                        "text": b.text,
                        "callback_data": b.data,
                    })
                })
                .collect()
        })
        .collect();
    serde_json::json!({ "inline_keyboard": keyboard }).to_string()
}

#[derive(Clone)]
pub struct TelegramClient {
    token: String,
    http: reqwest::blocking::Client,
}

impl TelegramClient {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            http: build_http(),
        }
    }

    fn url(&self, method: &str) -> String {
        format!("https://api.telegram.org/bot{}/{method}", self.token)
    }

    fn http(&self) -> &reqwest::blocking::Client {
        &self.http
    }

    fn ensure_ok(v: &Value, method: &str) -> Result<()> {
        if v.get("ok").and_then(|x| x.as_bool()) != Some(true) {
            return Err(anyhow!(
                "telegram {method}: {}",
                v.get("description")
                    .and_then(|x| x.as_str())
                    .unwrap_or("unknown")
            ));
        }
        Ok(())
    }

    pub fn send_message(&self, chat_id: &str, text: &str) -> Result<()> {
        self.send_message_markup(chat_id, text, None)
    }

    pub fn send_message_markup(
        &self,
        chat_id: &str,
        text: &str,
        markup: Option<&str>,
    ) -> Result<()> {
        let mut form: Vec<(&str, &str)> = vec![
            ("chat_id", chat_id),
            ("text", text),
            ("disable_web_page_preview", "true"),
        ];
        if let Some(m) = markup {
            form.push(("reply_markup", m));
        }
        let resp = self
            .http()
            .post(self.url("sendMessage"))
            .form(&form)
            .send()?;
        let v: Value = resp.json().unwrap_or(Value::Null);
        Self::ensure_ok(&v, "sendMessage")
    }

    pub fn answer_callback(&self, callback_query_id: &str, text: &str) -> Result<()> {
        let mut form: Vec<(&str, &str)> = vec![("callback_query_id", callback_query_id)];
        if !text.is_empty() {
            form.push(("text", text));
        }
        let resp = self
            .http()
            .post(self.url("answerCallbackQuery"))
            .form(&form)
            .send()?;
        let v: Value = resp.json().unwrap_or(Value::Null);
        Self::ensure_ok(&v, "answerCallbackQuery")
    }

    pub fn edit_message_text(
        &self,
        chat_id: &str,
        message_id: i64,
        text: &str,
        markup: Option<&str>,
    ) -> Result<()> {
        let mid = message_id.to_string();
        let mut form: Vec<(&str, &str)> = vec![
            ("chat_id", chat_id),
            ("message_id", mid.as_str()),
            ("text", text),
            ("disable_web_page_preview", "true"),
        ];
        if let Some(m) = markup {
            form.push(("reply_markup", m));
        }
        let resp = self
            .http()
            .post(self.url("editMessageText"))
            .form(&form)
            .send()?;
        let v: Value = resp.json().unwrap_or(Value::Null);
        Self::ensure_ok(&v, "editMessageText")
    }

    pub fn send_location(&self, chat_id: &str, fix: &GeoFix) -> Result<()> {
        let lat = format!("{:.6}", fix.lat);
        let lon = format!("{:.6}", fix.lon);
        let acc = format!("{:.0}", fix.accuracy_m.clamp(1.0, 1500.0));
        let resp = self
            .http()
            .post(self.url("sendLocation"))
            .form(&[
                ("chat_id", chat_id),
                ("latitude", lat.as_str()),
                ("longitude", lon.as_str()),
                ("horizontal_accuracy", acc.as_str()),
            ])
            .send()?;
        let v: Value = resp.json().unwrap_or(Value::Null);
        Self::ensure_ok(&v, "sendLocation")
    }

    pub fn send_document(
        &self,
        chat_id: &str,
        filename: &str,
        bytes: &[u8],
        caption: &str,
    ) -> Result<()> {
        let mime = if filename
            .rsplit('.')
            .next()
            .unwrap_or("")
            .eq_ignore_ascii_case("pdf")
        {
            "application/pdf"
        } else if filename
            .rsplit('.')
            .next()
            .unwrap_or("")
            .eq_ignore_ascii_case("jpg")
            || filename
                .rsplit('.')
                .next()
                .unwrap_or("")
                .eq_ignore_ascii_case("jpeg")
        {
            "image/jpeg"
        } else {
            "application/octet-stream"
        };
        let part = multipart::Part::bytes(bytes.to_vec())
            .file_name(filename.to_string())
            .mime_str(mime)
            .unwrap_or_else(|_| {
                multipart::Part::bytes(bytes.to_vec()).file_name(filename.to_string())
            });
        let form = multipart::Form::new()
            .text("chat_id", chat_id.to_string())
            .text("caption", caption.to_string())
            .part("document", part);
        let resp = self
            .http()
            .post(self.url("sendDocument"))
            .multipart(form)
            .send()?;
        let v: Value = resp.json().unwrap_or(Value::Null);
        Self::ensure_ok(&v, "sendDocument")
    }

    pub fn send_photo(
        &self,
        chat_id: &str,
        filename: &str,
        bytes: &[u8],
        caption: &str,
    ) -> Result<()> {
        let part = multipart::Part::bytes(bytes.to_vec())
            .file_name(filename.to_string())
            .mime_str("image/jpeg")
            .unwrap_or_else(|_| {
                multipart::Part::bytes(bytes.to_vec()).file_name(filename.to_string())
            });
        let form = multipart::Form::new()
            .text("chat_id", chat_id.to_string())
            .text("caption", caption.to_string())
            .part("photo", part);
        let resp = self
            .http()
            .post(self.url("sendPhoto"))
            .multipart(form)
            .send()?;
        let v: Value = resp.json().unwrap_or(Value::Null);
        Self::ensure_ok(&v, "sendPhoto")
    }

    /// `timeout_secs` is Telegram long-poll timeout (use 0). HTTP timeout stays short.
    pub fn get_updates(&self, offset: i64, timeout_secs: u64) -> Result<Vec<TgUpdate>> {
        let offset_s = offset.to_string();
        let timeout_s = timeout_secs.to_string();
        let resp = self
            .http()
            .post(self.url("getUpdates"))
            .form(&[
                ("offset", offset_s.as_str()),
                ("timeout", timeout_s.as_str()),
                ("limit", "100"),
                ("allowed_updates", r#"["message","callback_query"]"#),
            ])
            .send()?;
        let v: Value = resp.json().unwrap_or(Value::Null);
        Self::ensure_ok(&v, "getUpdates")?;
        Ok(parse_get_updates_result(&v))
    }

    pub fn delete_webhook(&self) -> Result<()> {
        let resp = self
            .http()
            .post(self.url("deleteWebhook"))
            .form(&[("drop_pending_updates", "false")])
            .send()?;
        let v: Value = resp.json().unwrap_or(Value::Null);
        Self::ensure_ok(&v, "deleteWebhook")
    }

    /// Admin text + maps URL, then native map pin when a fix exists.
    pub fn send_admin(&self, chat_id: &str, text: &str, loc: Option<&GeoFix>) -> Result<()> {
        let mut body = text.to_string();
        let has_loc = has_location_text(&body);
        if let Some(f) = loc {
            if !has_loc {
                body.push('\n');
                body.push_str(&crate::domain::location::location_lines(Some(f)));
            }
            if !body.contains("maps.google.com") {
                body.push('\n');
                body.push_str(&f.maps_url());
            }
        } else if !has_loc {
            body.push('\n');
            body.push_str(&crate::domain::location::location_lines(None));
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

fn build_http() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(90))
        .pool_max_idle_per_host(2)
        .use_rustls_tls()
        .build()
        .unwrap_or_else(|_| reqwest::blocking::Client::new())
}

fn has_location_text(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    lower.contains("loc=") || lower.contains("location:")
}

pub fn parse_get_updates_result(v: &Value) -> Vec<TgUpdate> {
    let Some(arr) = v.get("result").and_then(|x| x.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in arr {
        let update_id = match item.get("update_id").and_then(|x| x.as_i64()) {
            Some(id) => id,
            None => continue,
        };
        if let Some(cq) = item.get("callback_query") {
            let callback_query_id = cq
                .get("id")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            if callback_query_id.is_empty() {
                continue;
            }
            let callback_data = cq
                .get("data")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let msg = cq.get("message");
            let chat_id = msg
                .and_then(|m| m.get("chat"))
                .and_then(|c| c.get("id"))
                .and_then(|id| match id {
                    Value::Number(n) => Some(n.to_string()),
                    Value::String(s) => Some(s.clone()),
                    _ => None,
                });
            let Some(chat_id) = chat_id else {
                continue;
            };
            let message_id = msg
                .and_then(|m| m.get("message_id"))
                .and_then(|x| x.as_i64())
                .unwrap_or(0);
            out.push(TgUpdate {
                update_id,
                chat_id,
                text: String::new(),
                date: chrono::Utc::now().timestamp(),
                callback_query_id: Some(callback_query_id),
                callback_data: Some(callback_data),
                message_id,
            });
            continue;
        }
        let msg = match item.get("message") {
            Some(m) => m,
            None => continue,
        };
        let chat_id = match msg.get("chat").and_then(|c| c.get("id")) {
            Some(Value::Number(n)) => n.to_string(),
            Some(Value::String(s)) => s.clone(),
            _ => continue,
        };
        let text = msg
            .get("text")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let date = msg.get("date").and_then(|x| x.as_i64()).unwrap_or(0);
        let message_id = msg
            .get("message_id")
            .and_then(|x| x.as_i64())
            .unwrap_or(0);
        out.push(TgUpdate {
            update_id,
            chat_id,
            text,
            date,
            callback_query_id: None,
            callback_data: None,
            message_id,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_text_updates_and_skips_non_messages() {
        let v = json!({
            "ok": true,
            "result": [
                {
                    "update_id": 10,
                    "message": {
                        "date": 1700000000,
                        "chat": { "id": 12345 },
                        "text": "@online"
                    }
                },
                {
                    "update_id": 11,
                    "edited_message": { "text": "nope" }
                },
                {
                    "update_id": 12,
                    "message": {
                        "date": 1700000001,
                        "chat": { "id": "-10099" },
                        "text": "@Alice"
                    }
                }
            ]
        });
        let u = parse_get_updates_result(&v);
        assert_eq!(u.len(), 2);
        assert_eq!(u[0].update_id, 10);
        assert_eq!(u[0].chat_id, "12345");
        assert_eq!(u[0].text, "@online");
        assert_eq!(u[1].chat_id, "-10099");
        assert_eq!(u[1].text, "@Alice");
        assert!(u[0].callback_query_id.is_none());
    }

    #[test]
    fn parses_callback_queries() {
        let v = json!({
            "ok": true,
            "result": [
                {
                    "update_id": 20,
                    "callback_query": {
                        "id": "cb1",
                        "data": "o:ab",
                        "message": { "message_id": 77, "chat": { "id": 12345 }, "date": 1 }
                    }
                }
            ]
        });
        let u = parse_get_updates_result(&v);
        assert_eq!(u.len(), 1);
        assert_eq!(u[0].callback_query_id.as_deref(), Some("cb1"));
        assert_eq!(u[0].callback_data.as_deref(), Some("o:ab"));
        assert_eq!(u[0].chat_id, "12345");
        assert_eq!(u[0].message_id, 77);
    }
}
