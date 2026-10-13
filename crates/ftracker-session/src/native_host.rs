use anyhow::Result;
use ftracker_core::config::{windows_user, AppConfig};
use ftracker_core::telegram::client::TelegramClient;
use ftracker_core::telegram::messages;
use ftracker_core::totp;
use serde_json::{json, Value};
use std::io::{Read, Write};

use crate::location;
use crate::ui::totp_dialog;

pub fn run() -> Result<()> {
    set_binary_stdio();
    let cfg = match AppConfig::load() {
        Ok(c) if c.is_registered() => c,
        _ => {
            reply(&json!({"allow": false, "error": "not_registered"}))?;
            return Ok(());
        }
    };
    let stdin = std::io::stdin();
    let mut lock = stdin.lock();
    loop {
        let mut lenb = [0u8; 4];
        if lock.read_exact(&mut lenb).is_err() {
            break;
        }
        let n = u32::from_le_bytes(lenb) as usize;
        if n == 0 || n > 1024 * 1024 {
            break;
        }
        let mut buf = vec![0u8; n];
        lock.read_exact(&mut buf)?;
        let v: Value = serde_json::from_slice(&buf).unwrap_or(json!({}));
        let allow = handle(&cfg, &v);
        reply(&json!({"allow": allow}))?;
    }
    Ok(())
}

fn handle(cfg: &AppConfig, v: &Value) -> bool {
    let name = v.get("name").and_then(|x| x.as_str()).unwrap_or("file");
    let site = v.get("site").and_then(|x| x.as_str()).unwrap_or("");
    let browser = v
        .get("browser")
        .and_then(|x| x.as_str())
        .unwrap_or("chrome.exe");
    let size = v.get("size").and_then(|x| x.as_u64()).unwrap_or(0);
    let size_mb = size / (1024 * 1024);
    if size_mb <= cfg.upload_max_mb {
        return true;
    }
    let loc = location::current();
    let client = TelegramClient::new(cfg.telegram_bot_token.clone());
    let user = windows_user();
    let blocked = messages::upload(
        "UPLOAD_BLOCKED",
        &user,
        &cfg.machine_id,
        browser,
        site,
        name,
        size_mb,
        "waiting_for_totp",
        loc.as_ref(),
    );
    let _ = client.send_admin(&cfg.telegram_admin_chat_id, &blocked, loc.as_ref());
    match totp_dialog::prompt("upload") {
        Some(code)
            if totp::verify(
                &cfg.totp_secret_b32,
                &code,
                chrono::Local::now().timestamp(),
            ) =>
        {
            let ok = messages::upload(
                "UPLOAD_ALLOWED",
                &user,
                &cfg.machine_id,
                browser,
                site,
                name,
                size_mb,
                "by_totp",
                loc.as_ref(),
            );
            let _ = client.send_admin(&cfg.telegram_admin_chat_id, &ok, loc.as_ref());
            totp_dialog::info("Allowed. Pick the file again to attach it once.");
            true
        }
        Some(_) => {
            let t = messages::upload(
                "UPLOAD_DENIED",
                &user,
                &cfg.machine_id,
                browser,
                site,
                name,
                size_mb,
                "",
                loc.as_ref(),
            );
            let _ = client.send_admin(&cfg.telegram_admin_chat_id, &t, loc.as_ref());
            false
        }
        None => {
            let t = messages::upload(
                "UPLOAD_TIMEOUT",
                &user,
                &cfg.machine_id,
                browser,
                site,
                name,
                size_mb,
                "",
                loc.as_ref(),
            );
            let _ = client.send_admin(&cfg.telegram_admin_chat_id, &t, loc.as_ref());
            false
        }
    }
}

fn reply(v: &Value) -> Result<()> {
    let bytes = serde_json::to_vec(v)?;
    let mut out = std::io::stdout().lock();
    out.write_all(&(bytes.len() as u32).to_le_bytes())?;
    out.write_all(&bytes)?;
    out.flush()?;
    Ok(())
}

fn set_binary_stdio() {
    #[cfg(windows)]
    {
        extern "C" {
            fn _setmode(fd: i32, mode: i32) -> i32;
        }
        const O_BINARY: i32 = 0x8000;
        unsafe {
            _setmode(0, O_BINARY);
            _setmode(1, O_BINARY);
        }
    }
}
