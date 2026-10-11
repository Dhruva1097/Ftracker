use anyhow::Result;
use rusqlite::Connection;

pub fn apply(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ts INTEGER NOT NULL,
            day TEXT NOT NULL,
            kind TEXT NOT NULL,
            process TEXT NOT NULL DEFAULT '',
            title TEXT NOT NULL DEFAULT '',
            category TEXT NOT NULL DEFAULT 'Other',
            counted INTEGER NOT NULL DEFAULT 1,
            duration_s INTEGER NOT NULL DEFAULT 0,
            lat REAL,
            lon REAL,
            acc REAL,
            extra TEXT NOT NULL DEFAULT ''
        );
        CREATE INDEX IF NOT EXISTS idx_events_day ON events(day);
        CREATE TABLE IF NOT EXISTS locations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ts INTEGER NOT NULL,
            day TEXT NOT NULL,
            lat REAL NOT NULL,
            lon REAL NOT NULL,
            acc REAL NOT NULL
        );
        CREATE TABLE IF NOT EXISTS screenshots (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ts INTEGER NOT NULL,
            day TEXT NOT NULL,
            dhash INTEGER NOT NULL,
            process TEXT NOT NULL DEFAULT '',
            jpeg BLOB NOT NULL
        );
        CREATE TABLE IF NOT EXISTS subscriber (
            windows_user TEXT PRIMARY KEY,
            chat_id TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS hourly_reports (
            hour_start TEXT PRIMARY KEY,
            body TEXT NOT NULL,
            telegram_ok INTEGER NOT NULL DEFAULT 0,
            last_error TEXT NOT NULL DEFAULT ''
        );
        CREATE TABLE IF NOT EXISTS daily_reports (
            day TEXT PRIMARY KEY,
            pdf BLOB,
            telegram_ok INTEGER NOT NULL DEFAULT 0,
            last_error TEXT NOT NULL DEFAULT ''
        );
        CREATE TABLE IF NOT EXISTS weekly_reports (
            week TEXT PRIMARY KEY,
            body TEXT NOT NULL,
            telegram_ok INTEGER NOT NULL DEFAULT 0,
            last_error TEXT NOT NULL DEFAULT ''
        );
        CREATE TABLE IF NOT EXISTS hours_short_sent (
            day TEXT PRIMARY KEY
        );
        CREATE TABLE IF NOT EXISTS telegram_queue (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            kind TEXT NOT NULL,
            payload TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS day_seconds (
            day TEXT PRIMARY KEY,
            counted_s INTEGER NOT NULL DEFAULT 0,
            idle_s INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS hour_seconds (
            hour_start TEXT PRIMARY KEY,
            counted_s INTEGER NOT NULL DEFAULT 0,
            idle_s INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS integrity_alerts (
            file TEXT NOT NULL,
            reason TEXT NOT NULL,
            hour_start TEXT NOT NULL,
            PRIMARY KEY (file, reason, hour_start)
        );
        "#,
    )?;
    Ok(())
}
