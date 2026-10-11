use ftracker_core::config::{windows_user, AppConfig};
use ftracker_core::report::day_totals;
use ftracker_core::telegram::client::TelegramClient;
use ftracker_core::telegram::messages;
use ftracker_core::GeoFix;
use ftracker_core::Store;

use super::totp_dialog;

pub fn open(cfg: &AppConfig, store: &Store, loc: Option<&GeoFix>, client: &TelegramClient) {
    let lines = day_totals::action_window_lines(store, cfg.daily_hours_required);
    let user = windows_user();
    let sub = store.get_subscriber(&user).ok().flatten();
    let sub_txt = match &sub {
        Some(id) if id.len() > 4 => format!("Linked Telegram: …{}", &id[id.len() - 4..]),
        Some(_) => "Linked Telegram: (set)".into(),
        None => "Not subscribed".into(),
    };
    totp_dialog::info(&format!("{}\n\n{sub_txt}", lines.join("\n")));
    let cmd = tinyfiledialogs::input_box(
        "Ftracker",
        "Type SUBSCRIBE, CHANGE, or UNSUBSCRIBE (or Cancel):",
        "",
    )
    .map(|s| s.trim().to_ascii_uppercase());
    match cmd.as_deref() {
        Some("SUBSCRIBE") => subscribe(cfg, store, loc, client, false),
        Some("CHANGE") => subscribe(cfg, store, loc, client, true),
        Some("UNSUBSCRIBE") => unsubscribe(cfg, store, loc, client),
        _ => {}
    }
}

fn subscribe(
    cfg: &AppConfig,
    store: &Store,
    loc: Option<&GeoFix>,
    client: &TelegramClient,
    change: bool,
) {
    let user = windows_user();
    let chat = match tinyfiledialogs::input_box(
        "Ftracker",
        "Paste your Telegram chat id (open the bot, tap Start, then paste the numeric chat id):",
        "",
    ) {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return,
    };
    if client
        .send_employee(&chat, &messages::employee_subscribed())
        .is_err()
    {
        totp_dialog::error(
            "Could not send a test DM. Start the bot from that Telegram account, then try again.",
        );
        return;
    }
    let _ = store.set_subscriber(&user, &chat);
    let _ = store.persist();
    let admin = if change {
        messages::employee_changed_admin(&cfg.machine_id, &user, loc)
    } else {
        messages::employee_subscribed_admin(&cfg.machine_id, &user, loc)
    };
    let _ = client.send_admin(&cfg.telegram_admin_chat_id, &admin, loc);
    totp_dialog::info("Subscribed. You will get daily tracked time only.");
}

fn unsubscribe(cfg: &AppConfig, store: &Store, loc: Option<&GeoFix>, client: &TelegramClient) {
    let user = windows_user();
    let _ = store.clear_subscriber(&user);
    let _ = store.persist();
    let admin = messages::employee_unsubscribed_admin(&cfg.machine_id, &user, loc);
    let _ = client.send_admin(&cfg.telegram_admin_chat_id, &admin, loc);
    totp_dialog::info("Unsubscribed. This PC will not send you daily time.");
}
