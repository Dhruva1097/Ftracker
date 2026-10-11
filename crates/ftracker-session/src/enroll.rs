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
    interactive_enroll(false)
}

/// First `pnpm dev` / session start: enroll if needed, never fail the process for cancel/missing .env.
pub fn ensure() -> Result<()> {
    let _ = paths::ensure_data_dir();
    if AppConfig::load().ok().filter(|c| c.is_registered()).is_some() {
        return Ok(());
    }
    let pre = paths::payload_path()
        .map(|p| p.exists())
        .unwrap_or(false)
        || paths::sibling("preassign.json").exists();
    let r = if pre {
        preassigned_enroll()
    } else {
        interactive_enroll(true)
    };
    match r {
        Ok(()) => Ok(()),
        Err(e) => {
            eprintln!("Ftracker enroll: {e:#}");
            Ok(())
        }
    }
}

fn interactive_enroll(_auto: bool) -> Result<()> {
    let defaults = crate::ui::enroll_window::Defaults {
        token: ftracker_core::config::env_telegram_token().unwrap_or_default(),
        chat_id: ftracker_core::config::env_telegram_chat_id().unwrap_or_default(),
        employee_name: windows_user(),
        daily_hours: 8.0,
        machine: hostname(),
    };
    let Some(form) = crate::ui::enroll_window::run(&defaults) else {
        return Ok(());
    };
    let mut cfg = AppConfig::default();
    cfg.telegram_bot_token = form.token;
    cfg.telegram_admin_chat_id = form.chat_id;
    cfg.employee_name = form.employee_name;
    cfg.daily_hours_required = form.daily_hours;
    cfg.totp_secret_b32 = form.totp_secret_b32;
    cfg.db_key = AppConfig::new_db_key();
    cfg.machine_id = hostname();
    finish_enroll(cfg)
}

fn preassigned_enroll() -> Result<()> {
    let payload = paths::payload_path()?;
    if !payload.exists() {
        // MSI may have copied next to the exe
        let alt = paths::sibling("preassign.json");
        if alt.exists() {
            let cfg = AppConfig::from_preassign(&alt)?;
            let _ = std::fs::remove_file(&alt);
            return finish_enroll(cfg);
        }
        return Err(anyhow!("preassign payload missing"));
    }
    let cfg = AppConfig::from_preassign(&payload)?;
    let _ = std::fs::remove_file(&payload);
    finish_enroll(cfg)
}

fn finish_enroll(mut cfg: AppConfig) -> Result<()> {
    if cfg.totp_secret_b32.is_empty() {
        cfg.totp_secret_b32 = totp::generate_secret_b32();
    }
    let (s_hash, svc_hash) = keepalive::hash_installed_exes()?;
    cfg.session_sha256 = s_hash;
    cfg.service_sha256 = svc_hash;
    let url = totp::otpauth_url(&cfg.employee_name, &cfg.machine_id, &cfg.totp_secret_b32);
    let recovery = totp::recovery_codes();
    let client = TelegramClient::new(cfg.telegram_bot_token.clone());
    let _ = client.send_message(
        &cfg.telegram_admin_chat_id,
        &format!(
            "Ftracker TOTP enroll for {}@{}\nAdd in Google Authenticator:\n{url}\nRecovery codes: {}",
            cfg.employee_name,
            cfg.machine_id,
            recovery.join(" ")
        ),
    );
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
