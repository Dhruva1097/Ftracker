use anyhow::Result;
use rusqlite::params;

use crate::Store;

impl Store {
    pub fn add_hour_seconds(&self, hour_start: &str, counted: i64, idle: i64) -> Result<()> {
        self.conn().execute(
            "INSERT INTO hour_seconds(hour_start, counted_s, idle_s) VALUES(?1,?2,?3)
             ON CONFLICT(hour_start) DO UPDATE SET counted_s=counted_s+excluded.counted_s, idle_s=idle_s+excluded.idle_s",
            params![hour_start, counted, idle],
        )?;
        Ok(())
    }

    pub fn hour_seconds(&self, hour_start: &str) -> Result<(i64, i64)> {
        Ok(self
            .conn()
            .query_row(
                "SELECT counted_s, idle_s FROM hour_seconds WHERE hour_start=?1",
                [hour_start],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or((0, 0)))
    }

    pub fn hour_categories(
        &self,
        hour_start_ts: i64,
        hour_end_ts: i64,
    ) -> Result<Vec<(String, i64)>> {
        let mut stmt = self.conn().prepare(
            "SELECT category, SUM(duration_s) FROM events
             WHERE ts>=?1 AND ts<?2 AND counted=1 GROUP BY category",
        )?;
        let rows = stmt.query_map([hour_start_ts, hour_end_ts], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }

    pub fn recent_days(&self, limit: i64) -> Result<Vec<(String, i64, i64)>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT day, counted_s, idle_s FROM day_seconds ORDER BY day DESC LIMIT ?1")?;
        let rows = stmt.query_map([limit], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }

    pub fn days_in_range(&self, from: &str, to: &str) -> Result<Vec<(String, i64, i64)>> {
        let mut stmt = self.conn().prepare(
            "SELECT day, counted_s, idle_s FROM day_seconds WHERE day>=?1 AND day<=?2 ORDER BY day",
        )?;
        let rows = stmt.query_map([from, to], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }

    pub fn first_last_loc_day(
        &self,
        day: &str,
    ) -> Result<(Option<(f64, f64, f64)>, Option<(f64, f64, f64)>, i64)> {
        let first = self
            .conn()
            .query_row(
                "SELECT lat,lon,acc FROM locations WHERE day=?1 ORDER BY ts ASC LIMIT 1",
                [day],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .ok();
        let last = self
            .conn()
            .query_row(
                "SELECT lat,lon,acc FROM locations WHERE day=?1 ORDER BY ts DESC LIMIT 1",
                [day],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .ok();
        let n: i64 = self
            .conn()
            .query_row("SELECT COUNT(*) FROM locations WHERE day=?1", [day], |r| {
                r.get(0)
            })
            .unwrap_or(0);
        Ok((first, last, n))
    }

    pub fn timeline_day(
        &self,
        day: &str,
    ) -> Result<
        Vec<(
            i64,
            String,
            String,
            String,
            String,
            Option<f64>,
            Option<f64>,
        )>,
    > {
        let mut stmt = self.conn().prepare(
            "SELECT ts, kind, process, title, category, lat, lon FROM events WHERE day=?1 ORDER BY ts ASC",
        )?;
        let rows = stmt.query_map([day], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        })?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }

    pub fn integrity_alert_logged(
        &self,
        file: &str,
        reason: &str,
        hour_start: &str,
    ) -> Result<bool> {
        let n: i64 = self
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM integrity_alerts WHERE file=?1 AND reason=?2 AND hour_start=?3",
                params![file, reason, hour_start],
                |r| r.get(0),
            )
            .unwrap_or(0);
        Ok(n > 0)
    }

    pub fn log_integrity_alert(&self, file: &str, reason: &str, hour_start: &str) -> Result<()> {
        self.conn().execute(
            "INSERT OR IGNORE INTO integrity_alerts(file, reason, hour_start) VALUES(?1,?2,?3)",
            params![file, reason, hour_start],
        )?;
        Ok(())
    }
}
