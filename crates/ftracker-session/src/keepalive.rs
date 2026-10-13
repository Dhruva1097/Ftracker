use anyhow::Result;
use ftracker_core::config::AppConfig;
use ftracker_core::integrity;
use ftracker_core::paths;
use ftracker_core::telegram::client::TelegramClient;
use ftracker_core::telegram::messages;
use ftracker_core::GeoFix;
use ftracker_core::Store;

pub fn check_service_integrity(
    cfg: &AppConfig,
    store: Option<&Store>,
    loc: Option<&GeoFix>,
    client: &TelegramClient,
) {
    let path = {
        let svc = paths::sibling("ftracker-svc.exe");
        if svc.exists() {
            svc
        } else {
            paths::this_exe()
        }
    };
    if let Some(find) = integrity::check_peer(&path, &cfg.service_sha256, "ftracker.exe") {
        let hour = ftracker_core::report::day_totals::current_hour_start();
        let already = store
            .and_then(|s| {
                s.integrity_alert_logged(&find.file, &find.reason, &hour)
                    .ok()
            })
            .unwrap_or(false);
        if !already {
            let text = messages::integrity_fail(
                &cfg.employee_name,
                &cfg.machine_id,
                &find.file,
                &find.reason,
                &find.expected,
                &find.got,
                loc,
            );
            let _ = client.send_admin(&cfg.telegram_admin_chat_id, &text, loc);
            if let Some(s) = store {
                let _ = s.log_integrity_alert(&find.file, &find.reason, &hour);
                let _ = s.persist();
            }
        }
    }
}

pub fn hash_installed_exes() -> Result<(String, String)> {
    let session = if paths::this_exe()
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .eq_ignore_ascii_case("ftracker.exe")
    {
        paths::this_exe()
    } else {
        paths::sibling("ftracker.exe")
    };
    let service = paths::sibling("ftracker-svc.exe");
    let a = integrity::sha256_file(&session).unwrap_or_default();
    let b = if service.exists() {
        integrity::sha256_file(&service).unwrap_or_default()
    } else {
        a.clone()
    };
    Ok((a, b))
}
