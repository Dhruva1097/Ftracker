use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Instant;

use anyhow::Result;
use chrono::{Datelike, Local, Timelike};
use ftracker_core::config::{windows_user, AppConfig};
use ftracker_core::domain::category::AppCategory;
use ftracker_core::domain::dhash;
use ftracker_core::domain::event::TrackEvent;
use ftracker_core::redact;
use ftracker_core::report::day_totals::{self, required_seconds};
use ftracker_core::report::pdf;
use ftracker_core::telegram::client::TelegramClient;
use ftracker_core::telegram::messages;
use ftracker_core::totp;
use ftracker_core::GeoFix;
use ftracker_core::Store;
use ftracker_ipc::protocol::IpcMsg;

use crate::activity;
use crate::keepalive;
use crate::location;
use crate::screenshot;
use crate::ui;

pub struct SessionState {
    pub cfg: AppConfig,
    pub store: Store,
    pub client: TelegramClient,
    pub chip_label: String,
    pub idle: bool,
    pub last_fg: activity::foreground::Foreground,
    pub last_loc: Option<GeoFix>,
    pub ticks: u64,
    pub last_persist: Instant,
    pub hour_key: String,
    pub totp_fails: u32,
    pub totp_lock_until: Option<Instant>,
    ipc_tx: Sender<IpcMsg>,
    ipc_rx: Receiver<IpcMsg>,
    attendance_out_sent: bool,
    pub last_snapshot: Option<Instant>,
    pub last_month_pdf: Option<Instant>,
    pub last_list: Option<Instant>,
    admin_rx: Receiver<crate::admin_cmd::AdminCmd>,
    admin_jobs: crate::admin_cmd::JobHandle,
}

pub fn run(restarted: bool, user_launch: bool) -> Result<()> {
    ui::log(format!(
        "session run restarted={restarted} user_launch={user_launch}"
    ));
    if crate::mutex::already_running() {
        ui::log("already running — not starting a second session");
        if user_launch {
            ui::tray::request_show();
        }
        return Ok(());
    }
    let mut cfg = match AppConfig::load() {
        Ok(c) if c.is_registered() => c,
        other => {
            let corrupt = match &other {
                Err(e) => {
                    let msg = format!("{e:#}");
                    !msg.contains("os error 5")
                        && !msg.contains("Access is denied")
                        && (msg.contains("SECRETS_CORRUPT") || msg.contains("CryptUnprotect"))
                }
                Ok(_) => false,
            };
            if corrupt {
                ui::log("config load: SECRETS_CORRUPT");
                ui::totp_dialog::error("SECRETS_CORRUPT");
                return Err(other.unwrap_err());
            }
            ui::log("not registered — opening enroll");
            let _ = crate::enroll::ensure();
            match AppConfig::load() {
                Ok(c) if c.is_registered() => c,
                Ok(_) => return Ok(()),
                Err(e) => {
                    let msg = format!("{e:#}");
                    if msg.contains("not registered") {
                        ui::log("enroll finished: still not registered (quiet exit)");
                        return Ok(());
                    }
                    if msg.contains("SECRETS_CORRUPT") || msg.contains("CryptUnprotect") {
                        ui::totp_dialog::error("SECRETS_CORRUPT");
                    }
                    return Err(e);
                }
            }
        }
    };
    ui::log("registered — starting session");
    ftracker_core::integrity::refresh_hashes_if_msi_replaced(&mut cfg);
    crate::permissions::ensure_at_start();
    let store = match Store::open(&cfg) {
        Ok(s) => s,
        Err(e) => {
            ui::log(format!("store open failed: {e:#}"));
            let msg = format!("{e:#}");
            if ftracker_core::paths::is_access_denied(&e)
                || msg.contains("0x80070005")
                || msg.contains("db busy")
                || msg.contains("db lock")
            {
                return Err(e);
            }
            let client = TelegramClient::new(&cfg.telegram_bot_token);
            let loc = location::current();
            let _ = client.send_admin(
                &cfg.telegram_admin_chat_id,
                &messages::db_corrupt(&cfg.employee_name, &cfg.machine_id, loc.as_ref()),
                loc.as_ref(),
            );
            return Err(e);
        }
    };
    let client = TelegramClient::new(cfg.telegram_bot_token.clone());
    keepalive::check_service_integrity(&cfg, Some(&store), location::current().as_ref(), &client);
    {
        let loc = location::current();
        let reason = if restarted {
            "restart_after_kill"
        } else {
            "sign_in"
        };
        let body = messages::attendance_in(
            &cfg.employee_name,
            &windows_user(),
            &cfg.machine_id,
            reason,
            loc.as_ref(),
        );
        if client
            .send_admin(&cfg.telegram_admin_chat_id, &body, loc.as_ref())
            .is_err()
        {
            let q = ftracker_core::telegram::QueuedSend::admin(
                cfg.telegram_admin_chat_id.clone(),
                body,
                loc.clone(),
            );
            if let Ok(payload) = serde_json::to_string(&q) {
                let _ = store.enqueue_telegram("admin", &payload);
            }
        }
    }
    let (ipc_tx, ipc_from_thread) = mpsc::channel::<IpcMsg>();
    let (to_ipc, ipc_rx_thread) = mpsc::channel::<IpcMsg>();
    let usb_to_ui = ipc_tx.clone();
    let admin_ipc = to_ipc.clone();
    thread::spawn(move || ipc_loop(ipc_tx, ipc_rx_thread));
    let (admin_rx, admin_jobs) = crate::admin_cmd::start(cfg.clone(), client.clone(), admin_ipc);

    let mut state = SessionState {
        chip_label: day_totals::chip_text(&store, cfg.daily_hours_required),
        last_loc: location::current().or_else(|| {
            store
                .last_location()
                .ok()
                .flatten()
                .filter(location::is_accurate)
        }),
        hour_key: day_totals::current_hour_start(),
        idle: activity::idle::is_idle(cfg.idle_seconds),
        last_fg: activity::foreground::read(),
        last_persist: Instant::now(),
        ticks: 0,
        totp_fails: 0,
        totp_lock_until: None,
        cfg,
        store,
        client,
        ipc_tx: to_ipc,
        ipc_rx: ipc_from_thread,
        attendance_out_sent: false,
        last_snapshot: None,
        last_month_pdf: None,
        last_list: None,
        admin_rx,
        admin_jobs,
    };
    state.on_start()?;
    thread::spawn(move || crate::usb::run_watch(usb_to_ui));
    ui::log("entering tray message loop");
    ui::tray::run_message_loop(&mut state, user_launch)
}

fn ipc_loop(tx: Sender<IpcMsg>, rx: Receiver<IpcMsg>) {
    loop {
        match ftracker_ipc::pipe::connect_client(3000) {
            Ok(mut f) => loop {
                while let Ok(out) = rx.try_recv() {
                    let _ = ftracker_ipc::pipe::write_msg(&mut f, &out);
                }
                if ftracker_ipc::pipe::bytes_available(&f) >= 4 {
                    match ftracker_ipc::pipe::read_msg(&mut f) {
                        Ok(m) => {
                            let _ = tx.send(m);
                        }
                        Err(_) => break,
                    }
                } else {
                    thread::sleep(std::time::Duration::from_millis(80));
                }
            },
            Err(_) => thread::sleep(std::time::Duration::from_secs(2)),
        }
    }
}

impl SessionState {
    fn on_start(&mut self) -> Result<()> {
        self.maybe_hours_short()?;
        self.retry_outbox(false)?;
        if let Some(fix) = &self.last_loc {
            let ts = Local::now().timestamp();
            let day = day_totals::today_iso();
            let _ = self.store.insert_location(ts, &day, fix);
        }
        Ok(())
    }

    pub fn on_paint(&mut self) {
        self.chip_label = day_totals::chip_text(&self.store, self.cfg.daily_hours_required);
        if self.idle {
            self.chip_label.push_str(" idle");
        }
    }

    pub fn on_timer(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
        let admin_cmds: Vec<_> = std::iter::from_fn(|| self.admin_rx.try_recv().ok()).collect();
        if !admin_cmds.is_empty() {
            let jobs = self.admin_jobs.clone();
            for cmd in admin_cmds {
                crate::admin_cmd::handle(self, &jobs, cmd);
            }
        }
        self.poll_ipc();
        if self.ticks % 2 == 0 {
            self.tick_activity();
        }
        if self.ticks % self.cfg.location_interval_seconds.max(2) == 0 {
            self.refresh_location();
        }
        if self.ticks % (self.cfg.screenshot_minutes.max(1) * 60) == 0 && !self.idle {
            self.maybe_screenshot();
        }
        if self.last_persist.elapsed().as_secs() >= 15 {
            let _ = self.store.persist();
            self.last_persist = Instant::now();
        }
        if self.ticks % 60 == 0 {
            keepalive::check_service_integrity(
                &self.cfg,
                Some(&self.store),
                self.last_loc.as_ref(),
                &self.client,
            );
            let _ = self.retry_outbox(false);
            let _ = self.store.trim_if_needed(self.cfg.db_max_mb as f64);
        }
        self.maybe_hour_boundary();
        self.maybe_daily_weekly();
        self.on_paint();
    }

    pub fn open_action(&mut self) {
        ui::action_window::open(&self.cfg, &self.store, self.last_loc.as_ref(), &self.client);
    }

    pub fn flush_for_shutdown_reason(&mut self, reason: &str) {
        if !self.attendance_out_sent {
            self.attendance_out_sent = true;
            let loc = self.last_loc.clone().or_else(location::current);
            let tracked = self
                .store
                .day_counted(&day_totals::today_iso())
                .unwrap_or(0);
            let body = messages::attendance_out(
                &self.cfg.employee_name,
                &windows_user(),
                &self.cfg.machine_id,
                reason,
                tracked,
                loc.as_ref(),
            );
            if self
                .client
                .send_admin(&self.cfg.telegram_admin_chat_id, &body, loc.as_ref())
                .is_err()
            {
                let q = ftracker_core::telegram::QueuedSend::admin(
                    self.cfg.telegram_admin_chat_id.clone(),
                    body,
                    loc.clone(),
                );
                if let Ok(payload) = serde_json::to_string(&q) {
                    let _ = self.store.enqueue_telegram("admin", &payload);
                }
            }
        }
        let _ = self.close_hour(false);
        let _ = self.build_and_queue_daily(&day_totals::today_iso());
        let _ = self.retry_outbox(true);
        let _ = self.store.persist();
    }

    pub fn poll_ipc(&mut self) {
        while let Ok(msg) = self.ipc_rx.try_recv() {
            match msg {
                IpcMsg::UsbBlocked {
                    letter,
                    name,
                    serial,
                    vidpid,
                } => {
                    self.handle_usb_totp(&letter, &name, &serial, &vidpid);
                }
                IpcMsg::TotpRequest { purpose, .. } => {
                    if purpose == "usb" {
                        // handled via UsbBlocked
                    }
                }
                IpcMsg::AdminFsResult { req_id, ok, detail } => {
                    crate::admin_cmd::on_privileged_result(&self.admin_jobs, &req_id, ok, &detail);
                }
                _ => {}
            }
        }
    }

    fn handle_usb_totp(&mut self, letter: &str, name: &str, serial: &str, vidpid: &str) {
        crate::usb::ensure_ejecting(letter, serial);
        if !crate::usb::begin_dialog(serial) {
            return;
        }
        let user = windows_user();
        let loc = self.last_loc.clone();
        let blocked = messages::usb(
            "USB_BLOCKED",
            &user,
            &self.cfg.machine_id,
            letter,
            name,
            serial,
            vidpid,
            "waiting_for_totp",
            loc.as_ref(),
        );
        {
            let client = self.client.clone();
            let chat = self.cfg.telegram_admin_chat_id.clone();
            let body = blocked;
            let loc_tg = loc.clone();
            thread::spawn(move || {
                let _ = client.send_admin(&chat, &body, loc_tg.as_ref());
            });
        }
        ui::log(format!("TOTP prompt for {letter} {name}"));
        if let Some(until) = self.totp_lock_until {
            if Instant::now() < until {
                crate::usb::allow_retry(serial);
                let _ = self.ipc_tx.send(IpcMsg::TotpCancel {
                    purpose: "usb".into(),
                });
                return;
            }
        }
        let detail = if name.is_empty() {
            letter.to_string()
        } else {
            format!("{letter}  {name}")
        };
        match ui::totp_dialog::prompt_with("usb", &detail) {
            Some(code)
                if totp::verify(&self.cfg.totp_secret_b32, &code, Local::now().timestamp()) =>
            {
                self.totp_fails = 0;
                crate::usb::allow_serial(serial);
                let _ = self.ipc_tx.send(IpcMsg::TotpResponse {
                    purpose: "usb".into(),
                    code,
                });
                let ok = messages::usb(
                    "USB_ALLOWED",
                    &user,
                    &self.cfg.machine_id,
                    letter,
                    name,
                    serial,
                    vidpid,
                    "by_totp",
                    loc.as_ref(),
                );
                let _ = self
                    .client
                    .send_admin(&self.cfg.telegram_admin_chat_id, &ok, loc.as_ref());
            }
            Some(_) => {
                self.totp_fails += 1;
                let kind = if self.totp_fails >= 5 {
                    self.totp_lock_until =
                        Some(Instant::now() + std::time::Duration::from_secs(300));
                    self.totp_fails = 0;
                    "USB_TOTP_LOCKED"
                } else {
                    "USB_DENIED"
                };
                crate::usb::allow_retry(serial);
                let _ = self.ipc_tx.send(IpcMsg::TotpCancel {
                    purpose: "usb".into(),
                });
                let t = messages::usb(
                    kind,
                    &user,
                    &self.cfg.machine_id,
                    letter,
                    name,
                    serial,
                    vidpid,
                    "",
                    loc.as_ref(),
                );
                let _ = self
                    .client
                    .send_admin(&self.cfg.telegram_admin_chat_id, &t, loc.as_ref());
            }
            None => {
                crate::usb::allow_retry(serial);
                let _ = self.ipc_tx.send(IpcMsg::TotpCancel {
                    purpose: "usb".into(),
                });
                let t = messages::usb(
                    "USB_TIMEOUT",
                    &user,
                    &self.cfg.machine_id,
                    letter,
                    name,
                    serial,
                    vidpid,
                    "",
                    loc.as_ref(),
                );
                let _ = self
                    .client
                    .send_admin(&self.cfg.telegram_admin_chat_id, &t, loc.as_ref());
            }
        }
    }

    fn tick_activity(&mut self) {
        let idle_now = activity::idle::is_idle(self.cfg.idle_seconds);
        let fg = activity::foreground::read();
        let ts = Local::now().timestamp();
        let day = day_totals::today_iso();
        let proc = activity::foreground::process_file_name(&fg.process);
        let cat = AppCategory::from_process(&proc).as_str().to_string();
        let title = redact::redact_title(&fg.title);

        if idle_now && !self.idle {
            let ev = TrackEvent {
                ts,
                kind: "idle_start".into(),
                process: proc.clone(),
                title: title.clone(),
                category: cat.clone(),
                counted: false,
                loc: self.last_loc.clone(),
                extra: String::new(),
            };
            let _ = self.store.insert_event(&ev, 0);
        } else if !idle_now && self.idle {
            let ev = TrackEvent {
                ts,
                kind: "idle_end".into(),
                process: proc.clone(),
                title: title.clone(),
                category: cat.clone(),
                counted: false,
                loc: self.last_loc.clone(),
                extra: format!("idle_secs={}", self.cfg.idle_seconds),
            };
            let _ = self.store.insert_event(&ev, 0);
        }
        self.idle = idle_now;

        if !idle_now && fg.hwnd != self.last_fg.hwnd && fg.hwnd != 0 {
            let ev = TrackEvent {
                ts,
                kind: "focus".into(),
                process: proc.clone(),
                title: title.clone(),
                category: cat.clone(),
                counted: true,
                loc: self.last_loc.clone(),
                extra: String::new(),
            };
            let _ = self.store.insert_event(&ev, 2);
        }
        self.last_fg = fg;

        let hour = day_totals::current_hour_start();
        if idle_now {
            let _ = self.store.add_seconds(&day, 0, 2);
            let _ = self.store.add_hour_seconds(&hour, 0, 2);
        } else {
            let _ = self.store.add_seconds(&day, 2, 0);
            let _ = self.store.add_hour_seconds(&hour, 2, 0);
        }
    }

    fn refresh_location(&mut self) {
        if let Some(fix) = location::current() {
            self.last_loc = Some(fix.clone());
            let ts = Local::now().timestamp();
            let day = day_totals::today_iso();
            let _ = self.store.insert_location(ts, &day, &fix);
            let _ = self.ipc_tx.send(IpcMsg::Ping);
        }
    }

    fn maybe_screenshot(&mut self) {
        let day = day_totals::today_iso();
        let count = self.store.screenshot_count_day(&day).unwrap_or(0);
        if count >= self.cfg.screenshot_max_per_day {
            return;
        }
        let last_hash = self.store.last_screenshot_hash(&day).unwrap_or(None);
        if let Ok(Some((jpeg, hash))) = screenshot::capture_jpeg(
            self.cfg.screenshot_max_width,
            self.cfg.screenshot_jpeg_quality,
        ) {
            if let Some(prev) = last_hash {
                if dhash::similar(prev, hash, 10) {
                    return;
                }
            }
            let proc = activity::foreground::process_file_name(&self.last_fg.process);
            let ts = Local::now().timestamp();
            let _ = self.store.insert_screenshot(ts, &day, hash, &proc, &jpeg);
        }
    }

    fn maybe_hour_boundary(&mut self) {
        let now = day_totals::current_hour_start();
        if now != self.hour_key {
            let prev = self.hour_key.clone();
            self.hour_key = now;
            let _ = self.send_hour(&prev);
        }
    }

    fn close_hour(&mut self, _shutdown: bool) -> Result<()> {
        let key = self.hour_key.clone();
        self.send_hour(&key)
    }

    fn send_hour(&mut self, hour_key: &str) -> Result<()> {
        let (active, idle) = self.store.hour_seconds(hour_key).unwrap_or((0, 0));
        if active == 0 && idle == 0 {
            return Ok(());
        }
        let loc = self.last_loc.clone();
        let hour_ts =
            chrono::NaiveDateTime::parse_from_str(&format!("{hour_key}:00"), "%Y-%m-%dT%H:%M:%S")
                .ok()
                .and_then(|n| n.and_local_timezone(Local).single())
                .map(|d| d.timestamp())
                .unwrap_or_else(|| Local::now().timestamp());
        let cats = self
            .store
            .hour_categories(hour_ts, hour_ts + 3600)
            .unwrap_or_default();
        let range = day_totals::hour_range_label(hour_key);
        let body = messages::summary(
            &self.cfg.employee_name,
            &self.cfg.machine_id,
            &range,
            active,
            idle,
            &cats,
            loc.as_ref(),
        );
        let ok = self
            .client
            .send_admin(&self.cfg.telegram_admin_chat_id, &body, loc.as_ref())
            .is_ok();
        self.store
            .mark_hourly(hour_key, &body, ok, if ok { "" } else { "send failed" })?;
        self.store.persist()?;
        Ok(())
    }

    fn maybe_daily_weekly(&mut self) {
        let now = Local::now();
        if now.hour() >= self.cfg.report_send_hour {
            let day = day_totals::today_iso();
            let _ = self.build_and_queue_daily(&day);
            if now.weekday() == chrono::Weekday::Fri {
                let _ = self.build_weekly(now.date_naive());
            }
        }
        if now.weekday() == chrono::Weekday::Sat || now.weekday() == chrono::Weekday::Sun {
            let last_fri = now.date_naive()
                - chrono::Duration::days(now.weekday().num_days_from_monday() as i64 - 4);
            let _ = self.build_weekly(last_fri);
        }
    }

    fn build_and_queue_daily(&mut self, day: &str) -> Result<()> {
        let pdf_bytes = pdf::build_daily_pdf(&self.store, &self.cfg, day)?;
        self.store.save_daily_pdf(day, &pdf_bytes)?;
        self.store.persist()?;
        Ok(())
    }

    fn build_weekly(&mut self, on: chrono::NaiveDate) -> Result<()> {
        let (mon, fri) = day_totals::week_bounds(on);
        let key = day_totals::iso_week_key(fri);
        let body = day_totals::weekly_body(
            &self.store,
            &self.cfg.employee_name,
            &self.cfg.machine_id,
            &key,
            mon,
            fri,
            self.cfg.daily_hours_required,
        );
        let loc = self.last_loc.clone();
        let ok = self
            .client
            .send_admin(&self.cfg.telegram_admin_chat_id, &body, loc.as_ref())
            .is_ok();
        self.store
            .mark_weekly(&key, &body, ok, if ok { "" } else { "send failed" })?;
        self.store.persist()?;
        Ok(())
    }

    fn maybe_hours_short(&mut self) -> Result<()> {
        let y = day_totals::yesterday_iso();
        if !day_totals::is_iso_weekday(&y) {
            return Ok(());
        }
        if self.store.hours_short_was_sent(&y).unwrap_or(false) {
            return Ok(());
        }
        let got = self.store.day_counted(&y).unwrap_or(0);
        let req = required_seconds(self.cfg.daily_hours_required);
        if got >= req {
            return Ok(());
        }
        let loc = self.last_loc.clone();
        let admin =
            messages::hours_short_admin(&self.cfg.employee_name, &y, req, got, loc.as_ref());
        let _ = self
            .client
            .send_admin(&self.cfg.telegram_admin_chat_id, &admin, loc.as_ref());
        if let Some(chat) = self.store.get_subscriber(&windows_user()).ok().flatten() {
            let _ = self
                .client
                .send_employee(&chat, &messages::hours_short_employee(got, req));
        }
        self.store.mark_hours_short_sent(&y)?;
        self.store.persist()?;
        Ok(())
    }

    fn retry_outbox(&mut self, shutdown: bool) -> Result<()> {
        let loc = self.last_loc.clone();
        for (hour, body) in self.store.unsent_hourly().unwrap_or_default() {
            let ok = self
                .client
                .send_admin(&self.cfg.telegram_admin_chat_id, &body, loc.as_ref())
                .is_ok();
            self.store
                .mark_hourly(&hour, &body, ok, if ok { "" } else { "retry" })?;
        }
        for (day, pdf_bytes) in self.store.unsent_daily().unwrap_or_default() {
            let counted = self.store.day_counted(&day).unwrap_or(0);
            let (a, b, n) = self
                .store
                .first_last_loc_day(&day)
                .unwrap_or((None, None, 0));
            let caption = messages::daily_caption(
                &self.cfg.employee_name,
                &self.cfg.machine_id,
                &day,
                counted,
                &day_totals::fmt_loc_opt(a),
                &day_totals::fmt_loc_opt(b),
                n,
                true,
            );
            let send = self.client.send_document(
                &self.cfg.telegram_admin_chat_id,
                &format!("ftracker-{day}.pdf"),
                &pdf_bytes,
                &caption,
            );
            let ok = if send.is_ok() {
                let _ = self.client.send_admin(
                    &self.cfg.telegram_admin_chat_id,
                    &caption,
                    loc.as_ref(),
                );
                true
            } else {
                false
            };
            self.store
                .mark_daily_ok(&day, ok, if ok { "" } else { "retry" })?;
            if ok {
                if let Some(chat) = self.store.get_subscriber(&windows_user()).ok().flatten() {
                    let req = required_seconds(self.cfg.daily_hours_required);
                    let pretty = day_totals::pretty_day(&day);
                    let _ = self
                        .client
                        .send_employee(&chat, &messages::employee_daily(&pretty, counted, req));
                }
            }
        }
        for (week, body) in self.store.unsent_weekly().unwrap_or_default() {
            let ok = self
                .client
                .send_admin(&self.cfg.telegram_admin_chat_id, &body, loc.as_ref())
                .is_ok();
            self.store
                .mark_weekly(&week, &body, ok, if ok { "" } else { "retry" })?;
        }
        for (id, _kind, payload) in self.store.pending_telegram().unwrap_or_default() {
            let ok = match serde_json::from_str::<ftracker_core::telegram::QueuedSend>(&payload) {
                Ok(q) if q.employee_only => self.client.send_employee(&q.chat_id, &q.text).is_ok(),
                Ok(q) => self
                    .client
                    .send_admin(&q.chat_id, &q.text, q.loc.as_ref())
                    .is_ok(),
                Err(_) => false,
            };
            if ok {
                let _ = self.store.delete_telegram(id);
            }
        }
        let _ = shutdown;
        self.store.persist()?;
        Ok(())
    }
}
