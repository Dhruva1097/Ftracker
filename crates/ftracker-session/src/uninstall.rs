use anyhow::{anyhow, Result};
use ftracker_core::config::{windows_user, AppConfig};
use ftracker_core::report::day_totals;
use ftracker_core::report::pdf;
use ftracker_core::telegram::client::TelegramClient;
use ftracker_core::telegram::messages;
use ftracker_core::totp;
use ftracker_core::Store;

use crate::location;
use crate::ui::totp_dialog;

pub fn run(otp: Option<String>) -> Result<()> {
    let cfg = AppConfig::load().map_err(|_| anyhow!("not registered"))?;
    let code = match otp {
        Some(c) => c,
        None => totp_dialog::prompt("uninstall").ok_or_else(|| anyhow!("cancelled"))?,
    };
    let loc = location::current();
    let client = TelegramClient::new(cfg.telegram_bot_token.clone());
    if !totp::verify(
        &cfg.totp_secret_b32,
        &code,
        chrono::Local::now().timestamp(),
    ) {
        let _ = client.send_admin(
            &cfg.telegram_admin_chat_id,
            &messages::uninstall_denied(&cfg.machine_id, &windows_user(), loc.as_ref()),
            loc.as_ref(),
        );
        totp_dialog::error("TOTP did not match. Ftracker stays installed.");
        return Ok(());
    }
    if let Ok(store) = Store::open(&cfg) {
        let day = day_totals::today_iso();
        if let Ok(bytes) = pdf::build_daily_pdf(&store, &cfg, &day) {
            let _ = store.save_daily_pdf(&day, &bytes);
            let _ = client.send_document(
                &cfg.telegram_admin_chat_id,
                &format!("ftracker-{day}.pdf"),
                &bytes,
                "DAILY_REPORT (uninstall flush)",
            );
        }
        let _ = store.persist();
    }
    let _ = client.send_admin(
        &cfg.telegram_admin_chat_id,
        &messages::uninstalled(&cfg.machine_id, &windows_user(), loc.as_ref()),
        loc.as_ref(),
    );
    std::thread::sleep(std::time::Duration::from_secs(2));
    let product = read_product_code().unwrap_or_default();
    if !product.is_empty() {
        let _ = std::process::Command::new("msiexec")
            .args(["/x", &product, "/qn", "FTRACKER_OTP_DONE=1"])
            .status();
    } else {
        totp_dialog::info("TOTP accepted. Remove Ftracker from Apps & Features if msiexec product code was not found.");
    }
    Ok(())
}

fn read_product_code() -> Option<String> {
    #[cfg(windows)]
    {
        use windows::core::w;
        use windows::Win32::System::Registry::{
            RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_LOCAL_MACHINE, KEY_READ,
        };
        let mut hkey = Default::default();
        unsafe {
            if RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                w!("Software\\Ftracker"),
                0,
                KEY_READ,
                &mut hkey,
            )
            .is_err()
            {
                return None;
            }
            let mut buf = [0u16; 64];
            let mut len = (buf.len() * 2) as u32;
            let mut ty = windows::Win32::System::Registry::REG_SZ;
            let ok = RegQueryValueExW(
                hkey,
                w!("ProductCode"),
                None,
                Some(&mut ty),
                Some(buf.as_mut_ptr() as *mut u8),
                Some(&mut len),
            );
            let _ = RegCloseKey(hkey);
            if ok.is_err() {
                return None;
            }
            let n = (len as usize / 2).saturating_sub(1);
            Some(String::from_utf16_lossy(&buf[..n]))
        }
    }
    #[cfg(not(windows))]
    {
        None
    }
}
