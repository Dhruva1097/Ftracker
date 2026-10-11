use anyhow::Result;
use rusqlite::params;

use crate::Store;

impl Store {
    pub fn mark_hourly(&self, hour_start: &str, body: &str, ok: bool, err: &str) -> Result<()> {
        self.conn().execute(
            "INSERT INTO hourly_reports(hour_start, body, telegram_ok, last_error) VALUES(?1,?2,?3,?4)
             ON CONFLICT(hour_start) DO UPDATE SET body=excluded.body, telegram_ok=excluded.telegram_ok, last_error=excluded.last_error",
            params![hour_start, body, ok as i64, err],
        )?;
        Ok(())
    }

    pub fn unsent_hourly(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT hour_start, body FROM hourly_reports WHERE telegram_ok=0")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }

    pub fn save_daily_pdf(&self, day: &str, pdf: &[u8]) -> Result<()> {
        self.conn().execute(
            "INSERT INTO daily_reports(day, pdf, telegram_ok, last_error) VALUES(?1,?2,0,'')
             ON CONFLICT(day) DO UPDATE SET pdf=excluded.pdf",
            params![day, pdf],
        )?;
        Ok(())
    }

    pub fn mark_daily_ok(&self, day: &str, ok: bool, err: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE daily_reports SET telegram_ok=?1, last_error=?2 WHERE day=?3",
            params![ok as i64, err, day],
        )?;
        Ok(())
    }

    pub fn unsent_daily(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT day, pdf FROM daily_reports WHERE telegram_ok=0 AND pdf IS NOT NULL")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }

    pub fn mark_weekly(&self, week: &str, body: &str, ok: bool, err: &str) -> Result<()> {
        self.conn().execute(
            "INSERT INTO weekly_reports(week, body, telegram_ok, last_error) VALUES(?1,?2,?3,?4)
             ON CONFLICT(week) DO UPDATE SET body=excluded.body, telegram_ok=excluded.telegram_ok, last_error=excluded.last_error",
            params![week, body, ok as i64, err],
        )?;
        Ok(())
    }

    pub fn unsent_weekly(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT week, body FROM weekly_reports WHERE telegram_ok=0")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }

    pub fn hours_short_was_sent(&self, day: &str) -> Result<bool> {
        let n: i64 = self
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM hours_short_sent WHERE day=?1",
                [day],
                |r| r.get(0),
            )
            .unwrap_or(0);
        Ok(n > 0)
    }

    pub fn mark_hours_short_sent(&self, day: &str) -> Result<()> {
        self.conn()
            .execute("INSERT OR IGNORE INTO hours_short_sent(day) VALUES(?1)", [day])?;
        Ok(())
    }

    pub fn enqueue_telegram(&self, kind: &str, payload: &str) -> Result<()> {
        let ts = chrono::Local::now().timestamp();
        self.conn().execute(
            "INSERT INTO telegram_queue(kind, payload, created_at) VALUES(?1,?2,?3)",
            params![kind, payload, ts],
        )?;
        Ok(())
    }

    pub fn pending_telegram(&self) -> Result<Vec<(i64, String, String)>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT id, kind, payload FROM telegram_queue ORDER BY id ASC")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }

    pub fn delete_telegram(&self, id: i64) -> Result<()> {
        self.conn()
            .execute("DELETE FROM telegram_queue WHERE id=?1", [id])?;
        Ok(())
    }
}
