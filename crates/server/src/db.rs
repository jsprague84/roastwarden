//! SQLite schema management.
//!
//! Migrations are plain SQL files applied once each and recorded in
//! `schema_migrations`. Each file is executed as a single multi-statement
//! script inside one transaction, so trigger bodies with inner semicolons
//! work and a failed migration leaves no half-applied state.
//!
//! Databases created before this runner existed have no `schema_migrations`
//! table. For those, each legacy migration is probed for a marker object it
//! creates; present markers are recorded as applied without re-running, and
//! anything missing is applied normally.

use sqlx::{Executor, Row, SqlitePool};
use tracing::{debug, info, warn};

/// One migration: a stable name and the SQL script.
pub struct Migration {
    pub name: &'static str,
    pub sql: &'static str,
    /// For migrations that predate tracking: how to tell whether a legacy
    /// database already has it. `None` means "never baseline, always run".
    legacy_marker: Option<Marker>,
}

enum Marker {
    Table(&'static str),
    Column(&'static str, &'static str),
}

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        name: "001_roast_sessions",
        sql: include_str!("../migrations/001_roast_sessions.sql"),
        legacy_marker: Some(Marker::Table("roast_sessions")),
    },
    Migration {
        name: "002_roast_events",
        sql: include_str!("../migrations/002_roast_events.sql"),
        legacy_marker: Some(Marker::Table("roast_events")),
    },
    Migration {
        name: "003_device_configuration",
        sql: include_str!("../migrations/003_device_configuration.sql"),
        legacy_marker: Some(Marker::Table("devices")),
    },
    Migration {
        name: "004_session_statistics",
        sql: include_str!("../migrations/004_session_statistics.sql"),
        legacy_marker: Some(Marker::Column("roast_sessions", "weight_loss_pct")),
    },
    Migration {
        name: "005_auc_value",
        sql: include_str!("../migrations/005_auc_value.sql"),
        legacy_marker: Some(Marker::Column("roast_sessions", "auc_value")),
    },
    Migration {
        name: "006_cupping_scores",
        sql: include_str!("../migrations/006_cupping_scores.sql"),
        legacy_marker: Some(Marker::Table("cupping_scores")),
    },
    Migration {
        name: "007_profile_env_temp",
        sql: include_str!("../migrations/007_profile_env_temp.sql"),
        legacy_marker: Some(Marker::Column("profile_points", "target_env_temp")),
    },
    Migration {
        name: "008_core_tables_and_triggers",
        sql: include_str!("../migrations/008_core_tables_and_triggers.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "009_alarm_webhook",
        sql: include_str!("../migrations/009_alarm_webhook.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "010_profile_v2",
        sql: include_str!("../migrations/010_profile_v2.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "011_replay_log",
        sql: include_str!("../migrations/011_replay_log.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "012_autotune_runs",
        sql: include_str!("../migrations/012_autotune_runs.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "013_percent_units",
        sql: include_str!("../migrations/013_percent_units.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "014_fluid_bed_profiles",
        sql: include_str!("../migrations/014_fluid_bed_profiles.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "015_fluid_bed_profiles_v2",
        sql: include_str!("../migrations/015_fluid_bed_profiles_v2.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "016_roast_properties",
        sql: include_str!("../migrations/016_roast_properties.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "017_profile_metadata",
        sql: include_str!("../migrations/017_profile_metadata.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "018_batches_beans",
        sql: include_str!("../migrations/018_batches_beans.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "019_roast_guidance",
        sql: include_str!("../migrations/019_roast_guidance.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "020_backups",
        sql: include_str!("../migrations/020_backups.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "021_more_seed_profiles",
        sql: include_str!("../migrations/021_more_seed_profiles.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "022_audit_log",
        sql: include_str!("../migrations/022_audit_log.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "023_api_tokens",
        sql: include_str!("../migrations/023_api_tokens.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "024_push",
        sql: include_str!("../migrations/024_push.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "025_notifications",
        sql: include_str!("../migrations/025_notifications.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "026_approvals",
        sql: include_str!("../migrations/026_approvals.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "027_purchases",
        sql: include_str!("../migrations/027_purchases.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "028_bean_details",
        sql: include_str!("../migrations/028_bean_details.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "029_shopping_list",
        sql: include_str!("../migrations/029_shopping_list.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "030_label_studio",
        sql: include_str!("../migrations/030_label_studio.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "031_label_runs",
        sql: include_str!("../migrations/031_label_runs.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "032_label_printers",
        sql: include_str!("../migrations/032_label_printers.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "033_share_links",
        sql: include_str!("../migrations/033_share_links.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "034_cupping_defects",
        sql: include_str!("../migrations/034_cupping_defects.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "035_drive_setpoint",
        sql: include_str!("../migrations/035_drive_setpoint.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "036_landmarks_from_charge",
        sql: include_str!("../migrations/036_landmarks_from_charge.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "037_telemetry_extras",
        sql: include_str!("../migrations/037_telemetry_extras.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "038_autotune_server_runs",
        sql: include_str!("../migrations/038_autotune_server_runs.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "039_dedupe_firmware_runs",
        sql: include_str!("../migrations/039_dedupe_firmware_runs.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "040_drop_firmware_autotune_messages",
        sql: include_str!("../migrations/040_drop_firmware_autotune_messages.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "041_profile_auto_drop",
        sql: include_str!("../migrations/041_profile_auto_drop.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "042_roaster_models",
        sql: include_str!("../migrations/042_roaster_models.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "043_roasted_stock",
        sql: include_str!("../migrations/043_roasted_stock.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "044_bean_label_text",
        sql: include_str!("../migrations/044_bean_label_text.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "045_roaster_setup",
        sql: include_str!("../migrations/045_roaster_setup.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "046_roaster_outputs",
        sql: include_str!("../migrations/046_roaster_outputs.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "047_register_roles",
        sql: include_str!("../migrations/047_register_roles.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "048_roaster_channels",
        sql: include_str!("../migrations/048_roaster_channels.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "049_roast_channels",
        sql: include_str!("../migrations/049_roast_channels.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "050_packed_bags",
        sql: include_str!("../migrations/050_packed_bags.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "051_profile_roaster_kind",
        sql: include_str!("../migrations/051_profile_roaster_kind.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "052_logins",
        sql: include_str!("../migrations/052_logins.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "053_modbus_register_unit",
        sql: include_str!("../migrations/053_modbus_register_unit.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "054_purchase_orders",
        sql: include_str!("../migrations/054_purchase_orders.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "055_wanted_quantity",
        sql: include_str!("../migrations/055_wanted_quantity.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "056_ror_warnings",
        sql: include_str!("../migrations/056_ror_warnings.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "057_roastwarden_helpers",
        sql: include_str!("../migrations/057_roastwarden_helpers.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "058_profile_share_links",
        sql: include_str!("../migrations/058_profile_share_links.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "059_backup_runs_renamed",
        sql: include_str!("../migrations/059_backup_runs_renamed.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "060_mqtt_passwords",
        sql: include_str!("../migrations/060_mqtt_passwords.sql"),
        legacy_marker: None,
    },
    Migration {
        name: "061_combined_lots",
        sql: include_str!("../migrations/061_combined_lots.sql"),
        legacy_marker: None,
    },
];

/// Apply all pending migrations. Safe to call on every start.
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            name TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .execute(pool)
    .await?;

    let recorded: Vec<String> = sqlx::query("SELECT name FROM schema_migrations")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| r.get::<String, _>("name"))
        .collect();
    let legacy_db = recorded.is_empty() && table_exists(pool, "roast_sessions").await?;
    if legacy_db {
        info!("Existing database without migration history detected; baselining");
    }

    let mut applied = 0usize;
    let mut reclaim = false;
    for m in MIGRATIONS {
        if recorded.iter().any(|r| r == m.name) {
            continue;
        }

        if legacy_db {
            if let Some(marker) = &m.legacy_marker {
                let present = match marker {
                    Marker::Table(t) => table_exists(pool, t).await?,
                    Marker::Column(t, c) => column_exists(pool, t, c).await?,
                };
                if present {
                    debug!(migration = m.name, "already present, recording as applied");
                    record(pool, m.name).await?;
                    continue;
                }
            }
        }

        info!(migration = m.name, "applying migration");
        let mut tx = pool.begin().await?;
        if let Err(e) = tx.execute(m.sql).await {
            warn!(migration = m.name, error = %e, "migration failed; rolling back");
            return Err(e);
        }
        sqlx::query("INSERT INTO schema_migrations (name) VALUES (?)")
            .bind(m.name)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        applied += 1;
        // 020 drops the raw telemetry log, most of the file: give the space back once.
        reclaim |= m.name == "020_backups";
    }

    if applied > 0 {
        info!(count = applied, "migrations applied");
    }
    if reclaim {
        info!("reclaiming the space of the dropped telemetry log (VACUUM)");
        if let Err(e) = sqlx::query("VACUUM").execute(pool).await {
            warn!(error = %e, "VACUUM after migration 020 failed; the file stays its old size");
        }
    }
    Ok(())
}

async fn record(pool: &SqlitePool, name: &str) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT OR IGNORE INTO schema_migrations (name) VALUES (?)")
        .bind(name)
        .execute(pool)
        .await?;
    Ok(())
}

/// Whether the database answers: the health probes' `SELECT 1`.
pub async fn is_up(pool: &SqlitePool) -> bool {
    sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(pool)
        .await
        .is_ok()
}

/// The newest migration applied, by name; `None` when the table cannot be read.
pub async fn latest_migration(pool: &SqlitePool) -> Option<String> {
    sqlx::query_scalar("SELECT name FROM schema_migrations ORDER BY name DESC LIMIT 1")
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

pub async fn table_exists(pool: &SqlitePool, table: &str) -> Result<bool, sqlx::Error> {
    let n: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(table)
            .fetch_one(pool)
            .await?;
    Ok(n > 0)
}

pub async fn column_exists(
    pool: &SqlitePool,
    table: &str,
    column: &str,
) -> Result<bool, sqlx::Error> {
    if !table_exists(pool, table).await? {
        return Ok(false);
    }
    // PRAGMA table_info cannot be parameterised; table names come from this file only.
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await?;
    Ok(rows
        .iter()
        .any(|r| r.get::<String, _>("name").eq_ignore_ascii_case(column)))
}

#[cfg(test)]
pub async fn trigger_exists(pool: &SqlitePool, trigger: &str) -> Result<bool, sqlx::Error> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name = ?",
    )
    .bind(trigger)
    .fetch_one(pool)
    .await?;
    Ok(n > 0)
}

/// The connect options every pool uses: foreign keys on for each connection the pool opens
/// (a PRAGMA run on the pool reaches one connection only) and a 5 s wait for the write lock.
/// sqlx 0.8 already defaults to both; naming them keeps them if that default changes.
fn connect_options(url: &str) -> Result<sqlx::sqlite::SqliteConnectOptions, sqlx::Error> {
    use std::str::FromStr;
    Ok(sqlx::sqlite::SqliteConnectOptions::from_str(url)?
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_secs(5)))
}

/// A transaction that reads, then writes: it takes the write lock at BEGIN. A deferred one
/// (`pool.begin()`) fails at once with "database is locked" when another writer commits
/// between its read and its first write (WAL cannot move a stale read on to a write); this
/// one waits for the busy timeout instead. A transaction whose first statement writes is fine
/// deferred.
pub async fn begin_write(
    pool: &SqlitePool,
) -> Result<sqlx::Transaction<'static, sqlx::Sqlite>, sqlx::Error> {
    pool.begin_with("BEGIN IMMEDIATE").await
}

/// Open (or create) the SQLite database at `path` and bring the schema up to date.
pub async fn open(path: &str) -> Result<SqlitePool, sqlx::Error> {
    use sqlx::sqlite::SqlitePoolOptions;

    if let Some(parent) = std::path::Path::new(path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let url = format!("sqlite://{path}?mode=rwc");
    // No fallback: a server that cannot open its database file must not run on a database in
    // memory, where every roast is lost at the next restart without a word (it did, on a fresh
    // install whose data folder the server could not write).
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(connect_options(&url)?)
        .await?;
    // WAL is stored in the file: once is enough.
    let _ = sqlx::query("PRAGMA journal_mode=WAL;").execute(&pool).await;
    run_migrations(&pool).await?;
    Ok(pool)
}

/// In-memory database with the full schema, for tests.
#[cfg(test)]
pub async fn open_memory() -> SqlitePool {
    use sqlx::sqlite::SqlitePoolOptions;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(connect_options("sqlite::memory:").expect("options"))
        .await
        .expect("in-memory sqlite");
    run_migrations(&pool).await.expect("migrations");
    pool
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> (std::path::PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("roastwarden-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.db").to_str().unwrap().to_string();
        (dir, path)
    }

    /// Every connection the pool opens enforces foreign keys, not only the first.
    #[tokio::test]
    async fn every_pooled_connection_enforces_foreign_keys() {
        let (dir, path) = temp_path("fk");
        let pool = open(&path).await.unwrap();
        let mut held = Vec::new();
        for _ in 0..5 {
            let mut c = pool.acquire().await.unwrap();
            let on: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
                .fetch_one(&mut *c)
                .await
                .unwrap();
            assert_eq!(on, 1, "connection {} has foreign keys off", held.len());
            held.push(c);
        }
        drop(held);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Why `begin_write`: a deferred transaction that read before another writer committed
    /// cannot write (`SQLITE_BUSY_SNAPSHOT`, at once, no busy wait). One that took the write
    /// lock at BEGIN makes the other writer wait instead, and both land.
    #[tokio::test]
    async fn read_then_write_needs_the_write_lock_first() {
        let (dir, path) = temp_path("tx");
        let pool = open(&path).await.unwrap();
        let other = |key: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query("INSERT INTO settings (key, value) VALUES (?, 'x')")
                    .bind(key)
                    .execute(&pool)
                    .await
            }
        };
        let read = "SELECT COUNT(*) FROM settings";
        let write = "INSERT INTO settings (key, value) VALUES (?, 'x')";

        let mut tx = pool.begin().await.unwrap();
        let _: i64 = sqlx::query_scalar(read).fetch_one(&mut *tx).await.unwrap();
        other("test_other_1").await.unwrap();
        let err = sqlx::query(write)
            .bind("test_deferred")
            .execute(&mut *tx)
            .await
            .expect_err("a stale read cannot write");
        assert!(err.to_string().contains("locked"), "{err}");
        drop(tx);

        let mut tx = begin_write(&pool).await.unwrap();
        let _: i64 = sqlx::query_scalar(read).fetch_one(&mut *tx).await.unwrap();
        let waiting = tokio::spawn(other("test_other_2"));
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        sqlx::query(write)
            .bind("test_immediate")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        waiting
            .await
            .unwrap()
            .expect("the other writer waited, then wrote");
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM settings WHERE key LIKE 'test_%'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn fresh_database_gets_tables_triggers_and_settings() {
        let pool = open_memory().await;
        for t in [
            "roast_sessions",
            "roast_events",
            "devices",
            "cupping_scores",
            "backup_runs",
            "autotune_runs",
            "settings",
        ] {
            assert!(table_exists(&pool, t).await.unwrap(), "missing table {t}");
        }
        for tr in [
            "update_roast_sessions_updated_at",
            "update_roast_profiles_updated_at",
            "update_devices_updated_at",
            "update_device_profiles_updated_at",
            "update_device_connections_updated_at",
        ] {
            assert!(
                trigger_exists(&pool, tr).await.unwrap(),
                "missing trigger {tr}"
            );
        }
        let lookahead: String = sqlx::query_scalar(
            "SELECT value FROM settings WHERE key = 'profile_lookahead_seconds'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(lookahead, "20");

        // Running again is a no-op.
        run_migrations(&pool).await.unwrap();
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM schema_migrations")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n as usize, MIGRATIONS.len());
    }

    #[tokio::test]
    async fn legacy_database_is_baselined_and_repaired() {
        use sqlx::sqlite::SqlitePoolOptions;
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        // Reproduce what the old runner left behind: every statement split on
        // ';' and errors ignored, so tables exist but triggers do not.
        for m in MIGRATIONS.iter().filter(|m| m.legacy_marker.is_some()) {
            for statement in m.sql.split(';') {
                let statement = statement.trim();
                if !statement.is_empty() {
                    let _ = sqlx::query(statement).execute(&pool).await;
                }
            }
        }
        assert!(table_exists(&pool, "roast_sessions").await.unwrap());
        assert!(!trigger_exists(&pool, "update_roast_sessions_updated_at")
            .await
            .unwrap());
        // Seed data from 001 is already there; it must not be duplicated.
        let seeds = "SELECT COUNT(*) FROM roast_profiles WHERE id LIKE 'default-%'";
        let profiles_before: i64 = sqlx::query_scalar(seeds).fetch_one(&pool).await.unwrap();

        run_migrations(&pool).await.unwrap();

        assert!(trigger_exists(&pool, "update_roast_sessions_updated_at")
            .await
            .unwrap());
        assert!(table_exists(&pool, "settings").await.unwrap());
        let profiles_after: i64 = sqlx::query_scalar(seeds).fetch_one(&pool).await.unwrap();
        assert_eq!(profiles_before, profiles_after);
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM schema_migrations")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n as usize, MIGRATIONS.len());
    }

    #[tokio::test]
    async fn saved_designs_import_the_renamed_helpers() {
        let pool = open_memory().await;
        sqlx::query(
            "INSERT INTO label_designs (id, name, version, created_at, updated_at)
             VALUES ('mine', 'Mine', 1, 'x', 'x')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO label_design_versions (design_id, version, source, created_by, created_at)
             VALUES ('mine', 1, '#import \"rustroast.typ\": fit, clamp\n#fit[a]', 'owner', 'x')",
        )
        .execute(&pool)
        .await
        .unwrap();
        // As an install from before 0.25.0: 057 not yet applied
        sqlx::query("DELETE FROM schema_migrations WHERE name = '057_roastwarden_helpers'")
            .execute(&pool)
            .await
            .unwrap();
        run_migrations(&pool).await.unwrap();
        let source: String = sqlx::query_scalar("SELECT source FROM label_design_versions")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(source, "#import \"roastwarden.typ\": fit, clamp\n#fit[a]");
    }

    #[tokio::test]
    async fn backup_runs_name_the_renamed_copies() {
        let pool = open_memory().await;
        for name in [
            "rustroast-20261103-030024Z.db",
            "roastwarden-20261104-030000Z.db",
        ] {
            sqlx::query(
                "INSERT INTO backup_runs (trigger, started_at, file_name, size_bytes, removed)
                 VALUES ('nightly', 'x', ?, 1, 0)",
            )
            .bind(name)
            .execute(&pool)
            .await
            .unwrap();
        }
        // As an install from before this release: 059 not yet applied
        sqlx::query("DELETE FROM schema_migrations WHERE name = '059_backup_runs_renamed'")
            .execute(&pool)
            .await
            .unwrap();
        run_migrations(&pool).await.unwrap();
        let names: Vec<String> =
            sqlx::query_scalar("SELECT file_name FROM backup_runs ORDER BY file_name")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            names,
            [
                "roastwarden-20261103-030024Z.db",
                "roastwarden-20261104-030000Z.db"
            ]
        );
    }

    #[tokio::test]
    async fn updated_at_trigger_fires() {
        let pool = open_memory().await;
        sqlx::query(
            "INSERT INTO roast_sessions (id, name, device_id, status, created_at, updated_at)
             VALUES ('s1', 'n', 'd', 'planning', '2020-01-01 00:00:00', '2020-01-01 00:00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE roast_sessions SET name = 'renamed' WHERE id = 's1'")
            .execute(&pool)
            .await
            .unwrap();
        let updated: String =
            sqlx::query_scalar("SELECT updated_at FROM roast_sessions WHERE id = 's1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_ne!(updated, "2020-01-01 00:00:00");
    }
}
