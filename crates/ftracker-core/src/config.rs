use anyhow::{anyhow, Context, Result};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::crypto;
use crate::paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub telegram_bot_token: String,
    pub telegram_admin_chat_id: String,
    pub employee_name: String,
    pub daily_hours_required: f64,
    pub idle_seconds: u64,
    pub location_interval_seconds: u64,
    pub upload_max_mb: u64,
    pub report_regen_minutes: u64,
    pub report_send_hour: u32,
    pub summary_hours: u64,
    pub db_max_mb: u64,
    pub screenshot_minutes: u64,
    pub screenshot_max_per_day: u32,
    pub screenshot_jpeg_quality: u8,
    pub screenshot_max_width: u32,
    pub totp_secret_b32: String,
    pub db_key: Vec<u8>,
    pub session_sha256: String,
    pub service_sha256: String,
    pub machine_id: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            telegram_bot_token: String::new(),
            telegram_admin_chat_id: String::new(),
            employee_name: String::new(),
            daily_hours_required: 8.0,
            idle_seconds: 300,
            location_interval_seconds: 120,
            upload_max_mb: 100,
            report_regen_minutes: 30,
            report_send_hour: 18,
            summary_hours: 1,
            db_max_mb: 200,
            screenshot_minutes: 10,
            screenshot_max_per_day: 24,
            screenshot_jpeg_quality: 60,
            screenshot_max_width: 1280,
            totp_secret_b32: String::new(),
            db_key: vec![0; 32],
            session_sha256: String::new(),
            service_sha256: String::new(),
            machine_id: hostname(),
        }
    }
}

impl AppConfig {
    pub fn is_registered(&self) -> bool {
        !self.employee_name.is_empty()
            && !self.telegram_bot_token.is_empty()
            && !self.telegram_admin_chat_id.is_empty()
            && !self.totp_secret_b32.is_empty()
            && self.db_key.len() == 32
    }

    pub fn save(&self) -> Result<()> {
        let json = serde_json::to_vec(self)?;
        crypto::protect_to_file(&paths::secrets_blob()?, &json)
    }

    pub fn load() -> Result<Self> {
        let path = paths::secrets_blob()?;
        if !path.exists() {
            return Err(anyhow!("not registered"));
        }
        let bytes = match crypto::unprotect_from_file(&path) {
            Ok(b) => b,
            Err(e) if paths::is_access_denied(&e) => {
                return Err(e.context("not registered"));
            }
            Err(e) => return Err(e).context("SECRETS_CORRUPT"),
        };
        let cfg: Self = serde_json::from_slice(&bytes).context("SECRETS_CORRUPT")?;
        Ok(cfg)
    }

    pub fn load_or_none() -> Option<Self> {
        Self::load().ok()
    }

    pub fn new_db_key() -> Vec<u8> {
        let mut key = vec![0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        key
    }

    pub fn from_preassign(path: &PathBuf) -> Result<Self> {
        let raw = fs::read_to_string(path)?;
        let v: serde_json::Value = serde_json::from_str(&raw)?;
        let mut cfg = Self::default();
        cfg.employee_name = v
            .get("employee_name")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        cfg.daily_hours_required = v
            .get("daily_hours_required")
            .and_then(|x| x.as_f64())
            .unwrap_or(8.0);
        cfg.telegram_bot_token = v
            .get("telegram_bot_token")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        if let Some(n) = v.get("telegram_admin_chat_id").and_then(|x| x.as_i64()) {
            cfg.telegram_admin_chat_id = n.to_string();
        } else {
            cfg.telegram_admin_chat_id = v
                .get("telegram_admin_chat_id")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
        }
        cfg.db_key = Self::new_db_key();
        cfg.machine_id = hostname();
        Ok(cfg)
    }
}

pub fn hostname() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown-pc".into())
}

pub fn windows_user() -> String {
    std::env::var("USERNAME").unwrap_or_else(|_| "user".into())
}

/// Load `.env` from cwd, exe dir, then parents. Does not override variables already set.
pub fn load_dotenv() {
    let mut dirs = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.to_path_buf());
        }
    }
    let mut visited = std::collections::HashSet::new();
    for mut dir in dirs {
        for _ in 0..6 {
            if !visited.insert(dir.clone()) {
                break;
            }
            let file = dir.join(".env");
            if file.is_file() {
                apply_dotenv_file(&file);
                return;
            }
            match dir.parent() {
                Some(p) => dir = p.to_path_buf(),
                None => break,
            }
        }
    }
}

fn apply_dotenv_file(path: &std::path::Path) {
    let Ok(text) = fs::read_to_string(path) else {
        return;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let k = k.trim();
        if k != "TELEGRAM_BOT_TOKEN" && k != "TELEGRAM_ADMIN_CHAT_ID" {
            continue;
        }
        if std::env::var(k).ok().filter(|s| !s.is_empty()).is_some() {
            continue;
        }
        let v = v.trim().trim_matches('"').trim_matches('\'').to_string();
        if !v.is_empty() {
            std::env::set_var(k, v);
        }
    }
}

pub fn env_telegram_token() -> Option<String> {
    std::env::var("TELEGRAM_BOT_TOKEN")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn env_telegram_chat_id() -> Option<String> {
    std::env::var("TELEGRAM_ADMIN_CHAT_ID")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
