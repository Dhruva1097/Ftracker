use anyhow::{anyhow, Result};
use ftracker_core::config::{hostname, windows_user, AppConfig};
use ftracker_core::paths;
use ftracker_core::telegram::client::TelegramClient;
use ftracker_core::telegram::messages;
use ftracker_core::totp;
use ftracker_core::Store;

use crate::keepalive;
use crate::location;
use crate::ui::totp_dialog;

pub fn run(preassigned: bool) -> Result<()> {
    paths::ensure_data_dir()?;
    if let Ok(existing) = AppConfig::load() {
        if existing.is_registered() {
            return reregister(existing);
        }
    }
    if preassigned {
        return preassigned_enroll();
    }
    interactive_enroll()
}

fn interactive_enroll() -> Result<()> {
    totp_dialog::info(
        "This PC records active work time, app titles, and location; samples the work screen while you are active (compressed pictures go only to the admin daily PDF); blocks USB storage; cancels Chrome/Edge uploads over 100MB until an admin authenticator code; starts at sign-in; sends a daily PDF to the admin. Local files are not deleted.",
    );
    let token = ftracker_core::config::env_telegram_token()
        .or_else(|| required_input("Telegram bot token:").ok())
        .ok_or_else(|| anyhow!("Telegram bot token required"))?;
    let chat = ftracker_core::config::env_telegram_chat_id()
        .or_else(|| required_input("Admin Telegram chat id:").ok())
        .ok_or_else(|| anyhow!("Admin Telegram chat id required"))?;
    let name = required_input("Employee name:")?;
    let hours_s = required_input("Daily hours required (e.g. 8):")?;
    let hours: f64 = hours_s.parse().unwrap_or(8.0);
    let mut cfg = AppConfig::default();
    cfg.telegram_bot_token = token;
    cfg.telegram_admin_chat_id = chat;
    cfg.employee_name = name;
    cfg.daily_hours_required = hours;
    cfg.db_key = AppConfig::new_db_key();
    cfg.machine_id = hostname();
    finish_enroll(cfg, true)
}

fn preassigned_enroll() -> Result<()> {
    let payload = paths::payload_path()?;
    if !payload.exists() {
        // MSI may have copied next to the exe
        let alt = paths::sibling("preassign.json");
        if alt.exists() {
            let cfg = AppConfig::from_preassign(&alt)?;
            let _ = std::fs::remove_file(&alt);
            return finish_enroll(cfg, false);
        }
        return Err(anyhow!("preassign payload missing"));
    }
    let cfg = AppConfig::from_preassign(&payload)?;
    let _ = std::fs::remove_file(&payload);
    finish_enroll(cfg, false)
}

fn finish_enroll(mut cfg: AppConfig, show_qr_locally: bool) -> Result<()> {
    cfg.totp_secret_b32 = totp::generate_secret_b32();
    let (s_hash, svc_hash) = keepalive::hash_installed_exes()?;
    cfg.session_sha256 = s_hash;
    cfg.service_sha256 = svc_hash;
    let url = totp::otpauth_url(&cfg.employee_name, &cfg.machine_id, &cfg.totp_secret_b32);
    let qr = totp::qr_unicode(&url);
    let recovery = totp::recovery_codes();
    let client = TelegramClient::new(cfg.telegram_bot_token.clone());
    if show_qr_locally {
        totp_dialog::info(&format!(
            "Admin: scan this in Google Authenticator (or add the otpauth URL).\n\n{url}\n\n{qr}"
        ));
        let code = totp_dialog::prompt("enroll").ok_or_else(|| anyhow!("TOTP cancelled"))?;
        if !totp::verify(&cfg.totp_secret_b32, &code, chrono::Local::now().timestamp()) {
            let loc = location::current();
            let _ = client.send_admin(
                &cfg.telegram_admin_chat_id,
                &messages::install_failed(&cfg.machine_id, "totp_mismatch", loc.as_ref()),
                loc.as_ref(),
            );
            return Err(anyhow!("TOTP did not match"));
        }
    } else {
        let _ = client.send_message(
            &cfg.telegram_admin_chat_id,
            &format!(
                "Ftracker TOTP enroll for {}@{}\nAdd in Google Authenticator:\n{url}\nRecovery codes: {}",
                cfg.employee_name,
                cfg.machine_id,
                recovery.join(" ")
            ),
        );
    }
    cfg.save()?;
    let _ = Store::open(&cfg);
    let loc = location::current();
    let _ = client.send_admin(
        &cfg.telegram_admin_chat_id,
        &messages::registered(
            &cfg.employee_name,
            &cfg.machine_id,
            &windows_user(),
            loc.as_ref(),
        ),
        loc.as_ref(),
    );
    if show_qr_locally {
        let _ = client.send_message(
            &cfg.telegram_admin_chat_id,
            &format!("Recovery codes for {}@{}: {}", cfg.employee_name, cfg.machine_id, recovery.join(" ")),
        );
    }
    totp_dialog::info("This PC is registered. Tracking will start at the next sign-in.");
    Ok(())
}

fn reregister(mut cfg: AppConfig) -> Result<()> {
    let code = totp_dialog::prompt("reregister").ok_or_else(|| anyhow!("cancelled"))?;
    if !totp::verify(&cfg.totp_secret_b32, &code, chrono::Local::now().timestamp()) {
        totp_dialog::error("TOTP did not match. Employee name unchanged.");
        return Ok(());
    }
    let new_name = required_input("New employee name:")?;
    let old = cfg.employee_name.clone();
    cfg.employee_name = new_name;
    cfg.save()?;
    let loc = location::current();
    let client = TelegramClient::new(cfg.telegram_bot_token.clone());
    let _ = client.send_admin(
        &cfg.telegram_admin_chat_id,
        &messages::reregistered(&old, &cfg.employee_name, &cfg.machine_id, loc.as_ref()),
        loc.as_ref(),
    );
    totp_dialog::info("Employee name updated.");
    Ok(())
}

fn required_input(prompt: &str) -> Result<String> {
    tinyfiledialogs::input_box("Ftracker enroll", prompt, "")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("enrol cancelled"))
}
