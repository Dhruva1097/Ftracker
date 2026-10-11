use anyhow::Result;
use chrono::{Local, TimeZone};
use rusqlite::params;

use crate::domain::event::TrackEvent;
use crate::domain::location::GeoFix;
use crate::Store;

impl Store {
    pub fn insert_event(&self, ev: &TrackEvent, duration_s: i64) -> Result<()> {
        let day = Local
            .timestamp_opt(ev.ts, 0)
            .single()
            .unwrap_or_else(Local::now)
            .format("%Y-%m-%d")
            .to_string();
        self.conn().execute(
            "INSERT INTO events(ts,day,kind,process,title,category,counted,duration_s,lat,lon,acc,extra)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![
                ev.ts,
                day,
                ev.kind,
                ev.process,
                ev.title,
                ev.category,
                ev.counted as i64,
                duration_s,
                ev.loc.as_ref().map(|l| l.lat),
                ev.loc.as_ref().map(|l| l.lon),
                ev.loc.as_ref().map(|l| l.accuracy_m),
                ev.extra,
            ],
        )?;
        Ok(())
    }

    pub fn add_seconds(&self, day: &str, counted: i64, idle: i64) -> Result<()> {
        self.conn().execute(
            "INSERT INTO day_seconds(day, counted_s, idle_s) VALUES(?1,?2,?3)
             ON CONFLICT(day) DO UPDATE SET counted_s=counted_s+excluded.counted_s, idle_s=idle_s+excluded.idle_s",
            params![day, counted, idle],
        )?;
        Ok(())
    }

    pub fn day_counted(&self, day: &str) -> Result<i64> {
        let v: i64 = self
            .conn()
            .query_row(
                "SELECT COALESCE(counted_s,0) FROM day_seconds WHERE day=?1",
                [day],
                |r| r.get(0),
            )
            .unwrap_or(0);
        Ok(v)
    }

    pub fn day_idle(&self, day: &str) -> Result<i64> {
        Ok(self
            .conn()
            .query_row(
                "SELECT COALESCE(idle_s,0) FROM day_seconds WHERE day=?1",
                [day],
                |r| r.get(0),
            )
            .unwrap_or(0))
    }

    pub fn insert_location(&self, ts: i64, day: &str, fix: &GeoFix) -> Result<()> {
        self.conn().execute(
            "INSERT INTO locations(ts,day,lat,lon,acc) VALUES(?1,?2,?3,?4,?5)",
            params![ts, day, fix.lat, fix.lon, fix.accuracy_m],
        )?;
        Ok(())
    }

    pub fn last_location(&self) -> Result<Option<GeoFix>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT lat,lon,acc FROM locations ORDER BY ts DESC LIMIT 1")?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            Ok(Some(GeoFix {
                lat: row.get(0)?,
                lon: row.get(1)?,
                accuracy_m: row.get(2)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn category_seconds(&self, day: &str) -> Result<Vec<(String, i64)>> {
        let mut stmt = self.conn().prepare(
            "SELECT category, SUM(duration_s) FROM events WHERE day=?1 AND counted=1 GROUP BY category",
        )?;
        let rows = stmt.query_map([day], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }

    pub fn insert_screenshot(&self, ts: i64, day: &str, dhash: u64, process: &str, jpeg: &[u8]) -> Result<()> {
        self.conn().execute(
            "INSERT INTO screenshots(ts,day,dhash,process,jpeg) VALUES(?1,?2,?3,?4,?5)",
            params![ts, day, dhash as i64, process, jpeg],
        )?;
        Ok(())
    }

    pub fn screenshot_count_day(&self, day: &str) -> Result<u32> {
        let n: i64 = self.conn().query_row(
            "SELECT COUNT(*) FROM screenshots WHERE day=?1",
            [day],
            |r| r.get(0),
        )?;
        Ok(n as u32)
    }

    pub fn last_screenshot_hash(&self, day: &str) -> Result<Option<u64>> {
        let v: Option<i64> = self
            .conn()
            .query_row(
                "SELECT dhash FROM screenshots WHERE day=?1 ORDER BY ts DESC LIMIT 1",
                [day],
                |r| r.get(0),
            )
            .ok();
        Ok(v.map(|x| x as u64))
    }

    pub fn screenshots_day(&self, day: &str) -> Result<Vec<(i64, String, Vec<u8>)>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT ts, process, jpeg FROM screenshots WHERE day=?1 ORDER BY ts ASC")?;
        let rows = stmt.query_map([day], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }
}
