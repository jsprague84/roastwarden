//! The `settings` table: which keys are secret, and what each known key accepts.
//!
//! Secret values are never read back through the API, only whether they are set; a form can
//! resend everything with a secret left empty and the stored one stays; values the server
//! reads as numbers, flags or URLs are checked on the way in, so a typo cannot quietly fall
//! back to a default. Unknown keys are accepted as they are (the services keep their own
//! JSON under keys like `fan_assist:<device>`).

use sqlx::SqlitePool;

/// Write-only through the API. A webhook URL usually carries its own secret in the path.
const SECRETS: &[&str] = &[
    "notify_webhook_url",
    "notify_gotify_token",
    "notify_ntfy_token",
    "notify_matrix_token",
    // Home Assistant's broker (home_assistant.rs)
    "ha_password",
];

pub fn is_secret(key: &str) -> bool {
    SECRETS.contains(&key)
}

const FLAGS: &[&str] = &[
    "profile_lookahead_adapt",
    "auto_event_detection",
    "auto_drop",
    "alarm_sound_enabled",
    "notify_gotify_enabled",
    "notify_ntfy_enabled",
    "notify_matrix_enabled",
    "notify_webhook_enabled",
    "notify_event_backup",
    "notify_event_offline",
    "notify_event_stock",
    "ha_enabled",
    "ha_virtual",
];

const URLS: &[&str] = &[
    "notify_webhook_url",
    "notify_gotify_url",
    "notify_ntfy_url",
    "notify_matrix_homeserver",
    // Where links in notifications point (approvals); empty: the address a request came to
    "public_url",
];

/// The other keys the server or the dashboard reads (with [`SECRETS`], [`FLAGS`] and [`URLS`]).
/// A known key with no value is "unset", not missing (`GET /api/settings/{key}`); an unknown
/// one with nothing stored is a 404, so a typo shows. Keys a service makes per roaster
/// (`fan_assist:<device>`) and its own state are not listed. The test below scans the sources
/// for keys read or checked and fails when one is left out.
const KNOWN: &[&str] = &[
    // The RoR, auto events and AUC (telemetry.rs, services.rs)
    "ror_window_seconds",
    "ror_smoothing_algorithm",
    "auto_dry_temp",
    "auc_base_temp",
    "auc_start_event",
    "target_dtr_pct",
    // Profile drive (replay/)
    "profile_lookahead_seconds",
    "follow_hysteresis_c",
    "auto_drop_countdown_seconds",
    "roast_alarms",
    // Backups (backups.rs)
    "backup_keep",
    "backup_hour",
    "currency",
    crate::auth::modules::SETTING,
    crate::ops::zone::SETTING,
    // Home Assistant (home_assistant.rs)
    crate::notify::home_assistant::BROKER,
    crate::notify::home_assistant::PREFIX,
    crate::notify::home_assistant::INTERVAL,
    crate::notify::home_assistant::USERNAME,
    // Notifications (notify.rs)
    "notify_ntfy_topic",
    "notify_matrix_room",
    "notify_offline_minutes",
    // Roasted coffee and labels (roasted.rs, labels/)
    "dose_espresso_g",
    "dose_filter_g",
    "bag_g",
    "rest_days_espresso_light",
    "rest_days_espresso",
    "rest_days_filter_light",
    "rest_days_filter",
    "best_until_days_light",
    "best_until_days",
    crate::coffee::roasted::GROUND_KEY.0,
    crate::coffee::roasted::LOW_KEY.0,
    "label_sheets",
    "label_bags",
    "label_offsets",
    "label_printer_url",
    "label_qr",
    "label_bag",
    "label_brand",
    "label_roaster_name",
    // Reports (reports/)
    "report_paper",
    "report_units",
];

/// Is `key` one the server or the dashboard reads (so with no value it is unset)?
pub fn is_known(key: &str) -> bool {
    KNOWN.contains(&key) || SECRETS.contains(&key) || FLAGS.contains(&key) || URLS.contains(&key)
}

/// A number in `range`, or why not.
fn number(value: &str, lo: f64, hi: f64, what: &str) -> Result<(), String> {
    match value.trim().parse::<f64>() {
        Ok(v) if v.is_finite() && (lo..=hi).contains(&v) => Ok(()),
        _ => Err(format!("{what} is a number from {lo} to {hi}")),
    }
}

fn whole(value: &str, lo: i64, hi: i64, what: &str) -> Result<(), String> {
    match value.trim().parse::<i64>() {
        Ok(v) if (lo..=hi).contains(&v) => Ok(()),
        _ => Err(format!("{what} is a whole number from {lo} to {hi}")),
    }
}

/// Is `key` a name the API may write?
pub fn check_key(key: &str) -> Result<(), String> {
    if key.is_empty() || key.len() > 100 || key.chars().any(char::is_whitespace) {
        return Err("A setting key is 1 to 100 characters without spaces".into());
    }
    Ok(())
}

/// Check a value for a known key. An empty value is allowed wherever a setting is optional
/// (URLs, tokens, topics): it clears the setting.
pub fn check_value(key: &str, value: &str) -> Result<(), String> {
    check_key(key)?;
    if FLAGS.contains(&key) {
        return match value {
            "true" | "false" => Ok(()),
            _ => Err(format!("{key} is true or false")),
        };
    }
    if URLS.contains(&key) {
        let v = value.trim();
        if v.is_empty() {
            return Ok(());
        }
        let ok = (v.starts_with("https://") || v.starts_with("http://"))
            && reqwest::Url::parse(v).is_ok_and(|u| u.host_str().is_some());
        return if ok {
            Ok(())
        } else {
            Err(format!("{key} must be an http:// or https:// URL"))
        };
    }
    match key {
        "ror_window_seconds" => number(value, 5.0, 120.0, "The RoR window (s)"),
        "ror_smoothing_algorithm" => value
            .parse::<crate::analysis::telemetry::RorAlgorithm>()
            .map(|_| ()),
        "auto_dry_temp" => number(value, 50.0, 250.0, "The DRY end temperature (°C)"),
        "auc_base_temp" => number(value, 0.0, 300.0, "The AUC base temperature (°C)"),
        "profile_lookahead_seconds" => number(value, 0.0, 300.0, "The profile look-ahead (s)"),
        "follow_hysteresis_c" => number(value, 0.0, 20.0, "The follow hysteresis (°C)"),
        "auto_drop_countdown_seconds" => number(
            value,
            crate::replay::auto_drop::COUNTDOWN_RANGE.0,
            crate::replay::auto_drop::COUNTDOWN_RANGE.1,
            "The auto DROP countdown (s)",
        ),
        "backup_keep" => whole(
            value,
            0,
            crate::ops::backups::MAX_KEEP,
            "Backups to keep (0 turns nightly backups off)",
        ),
        "backup_hour" => whole(value, 0, 23, "The backup hour (UTC)"),
        "currency" => {
            let v = value.trim();
            if v.len() == 3 && v.chars().all(|c| c.is_ascii_alphabetic()) {
                Ok(())
            } else {
                Err("The currency is a three-letter code such as USD or EUR".into())
            }
        }
        // Home Assistant discovery (home_assistant.rs, docs/home-assistant.md)
        crate::notify::home_assistant::BROKER => {
            if value.trim().is_empty() {
                Ok(())
            } else {
                crate::notify::home_assistant::parse_broker(value).map(|_| ())
            }
        }
        crate::notify::home_assistant::PREFIX => {
            if value.trim().is_empty() {
                Ok(())
            } else {
                crate::notify::home_assistant::check_prefix(value.trim())
            }
        }
        crate::notify::home_assistant::INTERVAL => whole(
            value,
            crate::notify::home_assistant::INTERVAL_RANGE.0,
            crate::notify::home_assistant::INTERVAL_RANGE.1,
            "The Home Assistant interval (s)",
        ),
        // The dashboard's optional parts (modules.rs, docs/lean-home.md)
        crate::auth::modules::SETTING => crate::auth::modules::parse(value).map(|_| ()),
        // The owner's dates when a request sends no `tz` (zone.rs)
        crate::ops::zone::SETTING => crate::ops::zone::check(value),
        "dose_espresso_g" => number(value, 1.0, 100.0, "The espresso dose (g)"),
        "dose_filter_g" => number(value, 1.0, 100.0, "The filter dose (g)"),
        "bag_g" => number(value, 10.0, 5000.0, "The bag size (g)"),
        "report_paper" => match crate::reports::Paper::parse(value) {
            Some(_) => Ok(()),
            None => Err("report_paper is letter or a4".into()),
        },
        "report_units" => match crate::reports::Units::parse(value) {
            Some(_) => Ok(()),
            None => Err("report_units is C or F".into()),
        },
        // Bag labels (labels.rs, docs/labels.md)
        "rest_days_espresso_light"
        | "rest_days_espresso"
        | "rest_days_filter_light"
        | "rest_days_filter" => whole(value, 0, 90, "A rest (days)"),
        // Roasted coffee (roasted.rs, docs/roasted.md)
        "best_until_days_light" | "best_until_days" => {
            whole(value, 1, 365, "At its best for (days)")
        }
        // A ground bag, from the day it was packed (roasted.rs GROUND_KEY)
        "best_until_days_ground" => whole(value, 1, 60, "Ground, at its best for (days)"),
        "roasted_low_g" => number(value, 0.0, 100_000.0, "Roasted coffee running low (g)"),
        "label_sheets" => crate::labels::check_sheets(value),
        "label_bags" => crate::labels::retail::check_bags(value),
        "label_offsets" => crate::labels::check_offsets(value),
        "label_printer_url" => crate::labels::print::check_url(value),
        // What a label's QR code opens by default (docs/labels.md, "The share page")
        "label_qr" => match value.trim() {
            "" | "app" | "share" | "supplier" | "none" => Ok(()),
            _ => Err("label_qr is app, share, supplier or none".into()),
        },
        "notify_offline_minutes" => {
            whole(value, 1, 1440, "Minutes before a roaster counts as offline")
        }
        "roast_alarms" => match serde_json::from_str::<serde_json::Value>(value) {
            Ok(serde_json::Value::Array(_)) => Ok(()),
            _ => Err("roast_alarms is a JSON list of rules".into()),
        },
        "notify_ntfy_topic" => {
            let ok = value.is_empty()
                || (value.len() <= 64
                    && value
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
            if ok {
                Ok(())
            } else {
                Err("An ntfy topic is up to 64 letters, digits, - and _".into())
            }
        }
        "notify_matrix_room" => {
            if value.is_empty() || matrix_room_id(value) {
                Ok(())
            } else {
                Err(
                    "A Matrix room id starts with ! (Room settings → Advanced): \
                     !abc123:example.org, or in a newer room (version 12) ! and 43 characters"
                        .into(),
                )
            }
        }
        _ => Ok(()),
    }
}

/// Every setting as `(key, value)`, secrets included: the caller leaves those out.
pub async fn all(db: &SqlitePool) -> sqlx::Result<Vec<(String, String)>> {
    sqlx::query_as("SELECT key, value FROM settings")
        .fetch_all(db)
        .await
}

/// One setting's value and when it was written; `None` when nothing is stored under the key.
pub async fn row(db: &SqlitePool, key: &str) -> sqlx::Result<Option<(String, Option<String>)>> {
    sqlx::query_as("SELECT value, updated_at FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(db)
        .await
}

/// Write one setting inside the caller's transaction: an upsert, `updated_at` now.
pub async fn write(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    key: &str,
    value: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, datetime('now'))
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(value)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// One stored value, if any.
pub async fn get(db: &SqlitePool, key: &str) -> Option<String> {
    sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
}

/// Every stored value whose key starts with `prefix`.
pub async fn with_prefix(
    db: &SqlitePool,
    prefix: &str,
) -> sqlx::Result<std::collections::HashMap<String, String>> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT key, value FROM settings WHERE substr(key, 1, length(?1)) = ?1",
    )
    .bind(prefix)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().collect())
}

/// A flag, `default` when unset.
pub async fn flag(db: &SqlitePool, key: &str, default: bool) -> bool {
    get(db, key).await.map_or(default, |v| v == "true")
}

/// A Matrix room id: `!` then an opaque id, with `:server` only in rooms before version 12
/// (spec v1.16 on: a v12 room's id is its create event's hash, no domain). Clients treat it as
/// opaque, so only its shape is checked: not an alias (`#`), no spaces or control characters,
/// at most 255 bytes, and a localpart before any `:`.
fn matrix_room_id(value: &str) -> bool {
    let Some(rest) = value.strip_prefix('!') else {
        return false;
    };
    let local = rest.split(':').next().unwrap_or("");
    !local.is_empty()
        && value.len() <= 255
        && !value.chars().any(|c| c.is_whitespace() || c.is_control())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_the_tokens_and_the_webhook() {
        assert!(is_secret("notify_webhook_url"));
        assert!(is_secret("notify_matrix_token"));
        assert!(!is_secret("notify_gotify_url"));
        assert!(!is_secret("roast_alarms"));
    }

    #[test]
    fn known_keys_are_checked() {
        for (k, ok) in [
            ("ror_window_seconds", "30"),
            ("ror_smoothing_algorithm", "savitzky_golay"),
            ("backup_keep", "0"),
            ("backup_hour", "23"),
            ("auto_drop", "false"),
            ("notify_gotify_url", "https://gotify.example.org"),
            ("notify_gotify_url", ""),
            ("notify_ntfy_topic", "roast-alarms_1"),
            ("notify_matrix_room", "!abc123:example.org"),
            // a room version 12 id has no server part (Matrix spec v1.16, MSC4291)
            (
                "notify_matrix_room",
                "!31hneApxJ_1o-63DmFrpeqnkFfWppnzWso1JvH3ogLM",
            ),
            ("notify_offline_minutes", "5"),
            ("roast_alarms", "[]"),
            ("fan_assist:esp32_roaster_01", "{\"anything\": 1}"),
        ] {
            assert!(check_value(k, ok).is_ok(), "{k}={ok}");
        }
        for (k, bad) in [
            ("ror_window_seconds", "2"),
            ("ror_window_seconds", "abc"),
            ("ror_smoothing_algorithm", "fast"),
            ("backup_keep", "-1"),
            ("backup_hour", "24"),
            ("backup_hour", "1.5"),
            ("auto_drop", "yes"),
            ("notify_webhook_url", "ftp://x"),
            ("notify_webhook_url", "https://"),
            ("notify_gotify_url", "gotify.local"),
            ("notify_ntfy_topic", "has space"),
            ("notify_matrix_room", "#alias:example.org"),
            ("notify_matrix_room", "!"),
            ("notify_matrix_room", "!:example.org"),
            ("notify_matrix_room", "!has space"),
            ("notify_matrix_room", "abc123:example.org"),
            ("notify_offline_minutes", "0"),
            ("roast_alarms", "{}"),
            ("", "x"),
            ("two words", "x"),
        ] {
            assert!(check_value(k, bad).is_err(), "{k}={bad}");
        }
    }

    /// Every key the sources read or check is known: `settings::get`/`flag` with a literal,
    /// SQL on one key, a rule in `check_value`, a `"ha_…"`/`"notify_…"` literal, and the
    /// dashboard's `settings.set('…')`.
    #[test]
    fn every_key_read_or_checked_is_known() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut found = std::collections::BTreeSet::new();
        let mut scan = |dir: std::path::PathBuf, exts: &[&str]| {
            let mut stack = vec![dir];
            while let Some(d) = stack.pop() {
                for e in std::fs::read_dir(&d).unwrap().flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        stack.push(p);
                        continue;
                    }
                    if !exts.iter().any(|x| p.extension().is_some_and(|e| e == *x)) {
                        continue;
                    }
                    let text = std::fs::read_to_string(&p).unwrap();
                    let key = |s: &str| {
                        s.chars()
                            .take_while(|c| {
                                c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_'
                            })
                            .collect::<String>()
                    };
                    let mut after = |marker: &str, quote: char| {
                        for (i, _) in text.match_indices(marker) {
                            let rest = &text[i + marker.len()..];
                            // a read: the literal after the first comma
                            let rest = if marker.starts_with("settings::") {
                                let Some(c) = rest.find(',') else { continue };
                                rest[c + 1..].trim_start()
                            } else {
                                rest
                            };
                            if let Some(r) = rest.strip_prefix(quote) {
                                let k = key(r);
                                if !k.is_empty() && r[k.len()..].starts_with(quote) {
                                    found.insert(k);
                                }
                            }
                        }
                    };
                    after("settings::get(", '"');
                    after("settings::flag(", '"');
                    after("FROM settings WHERE key = ", '\'');
                    after("settings.set(", '\'');
                    for m in ["\"ha_", "\"notify_"] {
                        for (i, _) in text.match_indices(m) {
                            let k = key(&text[i + 1..]);
                            // `with_prefix(db, "ha_")` is a prefix, not a key
                            if !k.ends_with('_') && text[i + 1 + k.len()..].starts_with('"') {
                                found.insert(k);
                            }
                        }
                    }
                }
            }
        };
        scan(root.join("src"), &["rs"]);
        scan(root.join("../../apps/dashboard/src"), &["ts", "svelte"]);
        // check_value's rules: a quoted key, then `=>` or `|`
        let me = include_str!("settings.rs");
        let body = me
            .split("pub fn check_value")
            .nth(1)
            .and_then(|b| b.split("pub async fn get").next())
            .unwrap();
        for (i, _) in body.match_indices('"') {
            let rest = &body[i + 1..];
            let k: String = rest
                .chars()
                .take_while(|c| c.is_ascii_lowercase() || *c == '_')
                .collect();
            let tail = rest[k.len()..].trim_start_matches('"').trim_start();
            if k.contains('_')
                && rest[k.len()..].starts_with('"')
                && (tail.starts_with("=>") || tail.starts_with('|'))
            {
                found.insert(k);
            }
        }
        assert!(found.len() > 40, "the scan found too little: {found:?}");
        let missing: Vec<_> = found.iter().filter(|k| !is_known(k)).collect();
        assert!(
            missing.is_empty(),
            "read or checked but not in KNOWN: {missing:?}"
        );
        // A per-roaster key is not one of them
        assert!(!is_known("fan_assist:esp32_roaster_01"));
    }
}
