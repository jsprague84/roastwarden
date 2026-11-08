//! The owner's time zone, for the dates on labels, share pages, roasted coffee and reports.
//!
//! A request that says its offset (`tz`, minutes east of UTC: the dashboard always sends the
//! browser's) gets that. One that does not (the `roast` CLI on a host in UTC, an agent, a
//! script) gets the `time_zone` setting, an IANA name such as `America/Chicago`, with the
//! offset that zone had at the instant in question (so a roast in July and one in December
//! each get their own side of DST). With neither, UTC.

use chrono::{DateTime, FixedOffset, NaiveDate, Offset, TimeZone, Utc};
use sqlx::SqlitePool;

/// The settings key: an IANA zone name; empty or unset is UTC.
pub const SETTING: &str = "time_zone";

/// The widest offset a request may give, minutes (UTC−14 to UTC+14).
const MAX_MINUTES: i32 = 14 * 60;

/// A fixed offset (the request's) or a named zone (the setting's).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    Fixed(FixedOffset),
    Named(chrono_tz::Tz),
}

impl Default for Zone {
    fn default() -> Self {
        Self::Fixed(FixedOffset::east_opt(0).expect("UTC"))
    }
}

impl Zone {
    /// A fixed offset, minutes east of UTC (clamped to ±14 h).
    pub fn minutes(m: i32) -> Self {
        let m = m.clamp(-MAX_MINUTES, MAX_MINUTES);
        Self::Fixed(FixedOffset::east_opt(m * 60).expect("clamped to ±14 h"))
    }

    /// A zone by its IANA name (`America/Chicago`); `UTC` works too.
    pub fn named(name: &str) -> Option<Self> {
        name.trim().parse::<chrono_tz::Tz>().ok().map(Self::Named)
    }

    /// The offset from UTC at the instant `t`.
    pub fn offset_at(self, t: DateTime<Utc>) -> FixedOffset {
        match self {
            Self::Fixed(o) => o,
            Self::Named(z) => z.offset_from_utc_datetime(&t.naive_utc()).fix(),
        }
    }

    /// The offset at `t`, minutes east (what a share link stores).
    pub fn minutes_at(self, t: DateTime<Utc>) -> i32 {
        self.offset_at(t).local_minus_utc() / 60
    }

    /// `t` on the owner's clock.
    pub fn local(self, t: DateTime<Utc>) -> DateTime<FixedOffset> {
        t.with_timezone(&self.offset_at(t))
    }

    /// The owner's date at `t`.
    pub fn date(self, t: DateTime<Utc>) -> NaiveDate {
        self.local(t).date_naive()
    }

    /// The instant the owner's day `d` begins (local midnight; where a DST change skips
    /// midnight, the first instant of the day).
    pub fn day_start(self, d: NaiveDate) -> DateTime<Utc> {
        let midnight = d.and_hms_opt(0, 0, 0).expect("midnight");
        let at = |t: chrono::LocalResult<DateTime<FixedOffset>>| t.earliest();
        let found = match self {
            Self::Fixed(o) => at(o.from_local_datetime(&midnight)),
            Self::Named(z) => {
                // Midnight itself, else the first whole hour after it that exists
                (0..3).find_map(|h| {
                    z.from_local_datetime(&(midnight + chrono::Duration::hours(h)))
                        .earliest()
                        .map(|t| t.fixed_offset())
                })
            }
        };
        found.map_or_else(|| midnight.and_utc(), |t| t.with_timezone(&Utc))
    }
}

/// The value check for the `time_zone` setting: empty (UTC) or an IANA name.
pub fn check(value: &str) -> Result<(), String> {
    let v = value.trim();
    if v.is_empty() || Zone::named(v).is_some() {
        Ok(())
    } else {
        Err(format!(
            "time_zone is an IANA zone name such as America/Chicago or Europe/Berlin, not {v:?}"
        ))
    }
}

/// The zone for a request: its own offset if it sent one, else the `time_zone` setting,
/// else UTC.
pub async fn resolve(db: &SqlitePool, tz: Option<i32>) -> Zone {
    match tz {
        Some(m) => Zone::minutes(m),
        None => setting(db).await.unwrap_or_default(),
    }
}

/// The `time_zone` setting, if set to a zone this server knows.
pub async fn setting(db: &SqlitePool) -> Option<Zone> {
    crate::auth::settings::get(db, SETTING)
        .await
        .and_then(|v| Zone::named(&v))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn chicago() -> Zone {
        Zone::named("America/Chicago").unwrap()
    }

    #[test]
    fn a_named_zone_has_its_offset_for_the_date_in_question() {
        let z = chicago();
        // Summer (CDT, UTC−5) and winter (CST, UTC−6)
        assert_eq!(z.minutes_at(utc("2026-07-15T12:00:00Z")), -300);
        assert_eq!(z.minutes_at(utc("2026-12-15T12:00:00Z")), -360);
        // Either side of the November change (2026-11-01 07:00 UTC, 02:00 CDT → 01:00 CST)
        assert_eq!(z.minutes_at(utc("2026-11-01T06:59:00Z")), -300);
        assert_eq!(z.minutes_at(utc("2026-11-01T07:00:00Z")), -360);
        // Either side of the March change (2026-03-08 08:00 UTC, 02:00 CST → 03:00 CDT)
        assert_eq!(z.minutes_at(utc("2026-03-08T07:59:00Z")), -360);
        assert_eq!(z.minutes_at(utc("2026-03-08T08:00:00Z")), -300);
    }

    #[test]
    fn a_late_roast_is_the_owners_day_not_utcs() {
        // 22:05 CDT on 2026-10-29 is 03:05 UTC on the 30th
        let t = utc("2026-10-30T03:05:00Z");
        assert_eq!(chicago().date(t).to_string(), "2026-10-29");
        // 22:05 CST on 2026-12-01 is 04:05 UTC on the 2nd
        let t = utc("2026-12-02T04:05:00Z");
        assert_eq!(chicago().date(t).to_string(), "2026-12-01");
        assert_eq!(Zone::default().date(t).to_string(), "2026-12-02");
    }

    #[test]
    fn a_day_starts_at_the_owners_midnight() {
        let d = |s: &str| s.parse::<NaiveDate>().unwrap();
        assert_eq!(
            chicago().day_start(d("2026-07-01")),
            utc("2026-07-01T05:00:00Z")
        );
        assert_eq!(
            chicago().day_start(d("2026-12-01")),
            utc("2026-12-01T06:00:00Z")
        );
        assert_eq!(
            Zone::minutes(60).day_start(d("2026-12-01")),
            utc("2026-11-30T23:00:00Z")
        );
        // A zone whose DST change skips midnight (Havana: 00:00 → 01:00 in March)
        let havana = Zone::named("America/Havana").unwrap();
        let start = havana.day_start(d("2026-03-08"));
        assert_eq!(havana.date(start), d("2026-03-08"));
        assert!(havana.date(start - chrono::Duration::seconds(1)) < d("2026-03-08"));
    }

    #[test]
    fn the_setting_takes_a_zone_name_or_nothing() {
        assert!(check("America/Chicago").is_ok());
        assert!(check(" Europe/Berlin ").is_ok());
        assert!(check("UTC").is_ok());
        assert!(check("").is_ok());
        assert!(check("Central").is_err());
        assert!(check("-0500").is_err());
        assert!(crate::auth::settings::check_value(SETTING, "America/Mars").is_err());
        assert!(crate::auth::settings::check_value(SETTING, "America/Chicago").is_ok());
    }

    #[test]
    fn offsets_are_clamped() {
        assert_eq!(Zone::minutes(-5000).minutes_at(Utc::now()), -840);
        assert_eq!(Zone::minutes(-300).minutes_at(Utc::now()), -300);
    }

    #[tokio::test]
    async fn the_request_wins_then_the_setting_then_utc() {
        let db = crate::db::open_memory().await;
        let t = utc("2026-10-30T03:05:00Z");
        // Unset: UTC
        assert_eq!(resolve(&db, None).await.minutes_at(t), 0);
        sqlx::query("INSERT INTO settings (key, value) VALUES ('time_zone', 'America/Chicago')")
            .execute(&db)
            .await
            .unwrap();
        assert_eq!(resolve(&db, None).await, chicago());
        assert_eq!(resolve(&db, None).await.minutes_at(t), -300);
        // The request's own offset wins, UTC included
        assert_eq!(resolve(&db, Some(60)).await.minutes_at(t), 60);
        assert_eq!(resolve(&db, Some(0)).await.minutes_at(t), 0);
    }

    /// A roast at 22:05 in Chicago is 03:05 UTC the
    /// next day; a label, its roasted coffee and its report asked for with no `tz` (the CLI on
    /// a UTC host, an agent) are dated by the `time_zone` setting.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_late_roast_is_dated_by_the_setting_when_no_tz_is_sent() {
        use serde_json::json;
        let h = crate::test_support::Harness::new().await;
        let s = h
            .owner(
                "POST",
                "/api/sessions",
                Some(json!({ "name": "late", "device_id": "bench_roaster",
                    "roast_purpose": "espresso", "target_roast_level": "light" })),
            )
            .await
            .json();
        let id = s["id"].as_str().unwrap().to_string();
        let at = utc("2026-10-30T03:05:00Z");
        sqlx::query(
            "UPDATE roast_sessions SET status = 'completed', start_time = ?, end_time = ?,
             roasted_weight = 86 WHERE id = ?",
        )
        .bind(at - chrono::Duration::minutes(10))
        .bind(at)
        .bind(&id)
        .execute(&h.state.db)
        .await
        .unwrap();
        let roasted_on = |tz: &'static str| {
            let (h, id) = (&h, &id);
            async move {
                let r = h
                    .owner("GET", &format!("/api/roasted/{id}{tz}"), None)
                    .await;
                assert!(r.status.is_success(), "{:?}", r.json());
                r.json()["roasted_on"].as_str().unwrap().to_string()
            }
        };
        let label_date = |tz: Option<i32>| {
            let (h, id) = (&h, &id);
            async move {
                let r = h
                    .owner(
                        "POST",
                        "/api/labels/check",
                        Some(json!({ "design": "plain", "roast": id, "tz": tz })),
                    )
                    .await;
                assert!(r.status.is_success(), "{:?}", r.json());
                r.json()["report"]["lines"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|l| l["text"].as_str())
                    .find(|t| t.contains("Oct 2026"))
                    .unwrap_or_else(|| panic!("a date line: {:?}", r.json()["report"]["lines"]))
                    .to_string()
            }
        };
        let report_date = |tz: &'static str| {
            let (h, id) = (&h, &id);
            async move {
                let r = h
                    .owner(
                        "GET",
                        &format!("/api/reports/roast/{id}?format=json{tz}"),
                        None,
                    )
                    .await;
                assert!(r.status.is_success(), "{:?}", r.json());
                let roast = &r.json()["roast"];
                format!(
                    "{} {}",
                    roast["date"].as_str().unwrap(),
                    roast["time"].as_str().unwrap()
                )
            }
        };

        // No setting: UTC, the next day
        assert_eq!(roasted_on("").await, "2026-10-30");
        assert!(label_date(None).await.contains("30 Oct 2026"));
        assert_eq!(report_date("").await, "Friday 30 October 2026 02:55");

        let r = h
            .owner(
                "PUT",
                "/api/settings/time_zone",
                Some(json!({ "value": "Europe/Nowhere" })),
            )
            .await;
        assert_eq!(r.status, axum::http::StatusCode::BAD_REQUEST);
        let r = h
            .owner(
                "PUT",
                "/api/settings/time_zone",
                Some(json!({ "value": "America/Chicago" })),
            )
            .await;
        assert!(r.status.is_success());

        // The setting: the owner's day
        assert_eq!(roasted_on("").await, "2026-10-29");
        assert!(label_date(None).await.contains("29 Oct 2026"));
        assert_eq!(report_date("").await, "Thursday 29 October 2026 21:55");
        // A request's own offset still wins
        assert_eq!(roasted_on("?tz=60").await, "2026-10-30");
        assert!(label_date(Some(60)).await.contains("30 Oct 2026"));
        assert_eq!(report_date("&tz=60").await, "Friday 30 October 2026 03:55");
    }
}
