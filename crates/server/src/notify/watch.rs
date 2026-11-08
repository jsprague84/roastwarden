//! System events that need watching (`docs/notifications.md`, "System events"): a roaster
//! that went quiet outside a roast, and beans below their low-stock mark. Each problem goes
//! through [`Notifier::problem`] and [`Notifier::resolved`], which send it once.
//! (A failed nightly backup is reported by `backups.rs` itself.)

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use crate::notify::{EventKind, Notifier};
use crate::sim::virtual_roaster::{self, VirtualRoaster};

const EVERY: Duration = Duration::from_secs(30);
const DEFAULT_OFFLINE_MINUTES: i64 = 3;
/// Roasters not heard from in this long are retired, not offline: they are not reported.
const FORGOTTEN_AFTER: chrono::Duration = chrono::Duration::days(7);

pub fn start(db: SqlitePool, notifier: Notifier, virtual_roaster: Arc<VirtualRoaster>) {
    let started = Instant::now();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(EVERY);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            if let Err(e) = offline(&db, &notifier, &virtual_roaster, started).await {
                tracing::warn!(error = %e, "could not check which roasters are offline");
            }
            if let Err(e) = stock(&db, &notifier).await {
                tracing::warn!(error = %e, "could not check the bean stock");
            }
        }
    });
}

/// The offline rule, pure: quiet for `limit` and not forgotten.
pub fn is_offline(last_seen: DateTime<Utc>, now: DateTime<Utc>, limit: chrono::Duration) -> bool {
    let quiet = now - last_seen;
    quiet >= limit && quiet < FORGOTTEN_AFTER
}

fn minutes(d: chrono::Duration) -> String {
    match d.num_minutes() {
        m if m < 120 => format!("{m} min"),
        m => format!("{} h {} min", m / 60, m % 60),
    }
}

async fn offline(
    db: &SqlitePool,
    n: &Notifier,
    vr: &VirtualRoaster,
    started: Instant,
) -> anyhow::Result<()> {
    let limit_min = crate::auth::settings::get(db, "notify_offline_minutes")
        .await
        .and_then(|v| v.trim().parse::<i64>().ok())
        .filter(|m| (1..=1440).contains(m))
        .unwrap_or(DEFAULT_OFFLINE_MINUTES);
    let limit = chrono::Duration::minutes(limit_min);
    // Right after a start every roaster looks quiet: give them the limit to speak up.
    if started.elapsed() < limit.to_std().unwrap_or_default() {
        return Ok(());
    }
    let rows = sqlx::query_as::<_, (String, String, Option<String>, i64)>(
        "SELECT d.device_id, d.name, d.last_seen_at,
                EXISTS (SELECT 1 FROM roast_sessions s
                        WHERE s.device_id = d.device_id AND s.status IN ('active', 'paused'))
         FROM devices d WHERE d.status = 'active'",
    )
    .fetch_all(db)
    .await?;
    let state = vr.state().await;
    let (virtual_on, drum_on) = (state.enabled, state.drum.enabled);
    let now = Utc::now();
    for (device_id, name, last_seen, roasting) in rows {
        let key = format!("offline:{device_id}");
        let last_seen = last_seen
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|t| t.with_timezone(&Utc));
        let Some(last_seen) = last_seen else { continue };
        // Turned off on purpose is not offline.
        // A demo visitor's roaster comes and goes with its visitor (demo.rs).
        let switched_off = crate::ops::demo::is_visitor_roaster(&device_id)
            || (virtual_roaster::is_simulated(&device_id) && !virtual_on)
            || (device_id == crate::sim::virtual_drum::DEVICE_ID && !drum_on);
        if switched_off {
            n.resolved(
                EventKind::Offline,
                &key,
                &format!("{name} is off"),
                "Turned off in Roasters.",
            )
            .await;
            continue;
        }
        if is_offline(last_seen, now, limit) {
            // During a roast the no-telemetry alarm speaks for it (docs/alarms.md).
            if roasting == 0 {
                n.problem(
                    EventKind::Offline,
                    &key,
                    &format!("{name} is offline"),
                    &format!("No telemetry for {}.", minutes(now - last_seen)),
                )
                .await;
            }
        } else if now - last_seen < limit {
            n.resolved(
                EventKind::Offline,
                &key,
                &format!("{name} is back online"),
                "Telemetry is arriving again.",
            )
            .await;
        }
    }
    Ok(())
}

async fn stock(db: &SqlitePool, n: &Notifier) -> anyhow::Result<()> {
    // Only beans that have had stock: a bean just added with nothing in it is not "low".
    // What is on order is said with it (docs/green-coffee.md, "Orders").
    let rows = sqlx::query_as::<_, (String, String, f64, f64, bool, f64)>(
        "SELECT b.id, b.name, COALESCE(SUM(m.delta_g), 0.0), b.low_stock_g, b.archived,
         COALESCE((SELECT SUM(l.weight_g) FROM purchase_order_lines l
           JOIN purchase_orders o ON o.id = l.order_id
           WHERE l.bean_id = b.id AND l.kind = 'coffee' AND o.status IN ('ordered', 'shipped')), 0.0)
         FROM beans b JOIN bean_stock_moves m ON m.bean_id = b.id
         GROUP BY b.id",
    )
    .fetch_all(db)
    .await?;
    for (id, name, stock_g, low_g, archived, on_order_g) in rows {
        let key = format!("stock:{id}");
        if !archived && stock_g < low_g {
            let ordered = if on_order_g > 0.0 {
                format!(" {on_order_g:.0} g on order.")
            } else {
                String::new()
            };
            n.problem(
                EventKind::Stock,
                &key,
                &format!("Low stock: {name}"),
                &format!("{stock_g:.0} g left, below the {low_g:.0} g mark.{ordered}"),
            )
            .await;
        } else {
            n.resolved(
                EventKind::Stock,
                &key,
                &format!("{name} restocked"),
                &format!("{stock_g:.0} g in stock."),
            )
            .await;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_after_the_limit_until_forgotten() {
        let now = Utc::now();
        let limit = chrono::Duration::minutes(3);
        let ago = |m: i64| now - chrono::Duration::minutes(m);
        assert!(!is_offline(ago(2), now, limit));
        assert!(is_offline(ago(3), now, limit));
        assert!(is_offline(ago(60 * 24 * 6), now, limit));
        assert!(!is_offline(ago(60 * 24 * 8), now, limit));
        assert_eq!(minutes(chrono::Duration::minutes(5)), "5 min");
        assert_eq!(minutes(chrono::Duration::minutes(185)), "3 h 5 min");
    }
}
