use crate::config::AppConfig;

/// Registration is stored with DPAPI secrets (employee_name, machine_id, daily_hours_required, totp).
pub fn is_registered(cfg: &AppConfig) -> bool {
    cfg.is_registered()
}
