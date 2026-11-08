-- Migration: 008_core_tables_and_triggers.sql
--
-- Two jobs:
--  1. The tables the server used to create ad hoc in init_db (raw telemetry,
--     autotune status/results, settings) now live in a migration.
--  2. The updated_at triggers from 001 and 003 were never created on databases
--     built by the old statement-splitting runner (it cut BEGIN ... END bodies
--     at the inner semicolon and swallowed the error). Recreate them here with
--     IF NOT EXISTS so both fresh and existing databases end up identical.

CREATE TABLE IF NOT EXISTS telemetry (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id TEXT NOT NULL,
    ts INTEGER NOT NULL,
    payload TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_telemetry_device_ts ON telemetry(device_id, ts DESC);

CREATE TABLE IF NOT EXISTS autotune_status (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id TEXT NOT NULL,
    ts INTEGER NOT NULL,
    payload TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_autotune_status_device_ts ON autotune_status(device_id, ts DESC);

CREATE TABLE IF NOT EXISTS autotune_results (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id TEXT NOT NULL,
    ts INTEGER NOT NULL,
    payload TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_autotune_results_device_ts ON autotune_results(device_id, ts DESC);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT OR IGNORE INTO settings (key, value) VALUES ('profile_lookahead_seconds', '20');
INSERT OR IGNORE INTO settings (key, value) VALUES ('auc_base_temp', '0');
INSERT OR IGNORE INTO settings (key, value) VALUES ('auc_start_event', 'charge');
INSERT OR IGNORE INTO settings (key, value) VALUES ('ror_window_seconds', '30');
INSERT OR IGNORE INTO settings (key, value) VALUES ('ror_smoothing_algorithm', 'moving_average');
INSERT OR IGNORE INTO settings (key, value) VALUES ('auto_dry_temp', '150');
INSERT OR IGNORE INTO settings (key, value) VALUES ('auto_event_detection', 'true');
INSERT OR IGNORE INTO settings (key, value) VALUES ('alarm_sound_enabled', 'true');
INSERT OR IGNORE INTO settings (key, value) VALUES ('follow_hysteresis_c', '1.0');
INSERT OR IGNORE INTO settings (key, value) VALUES ('roast_alarms', '[{"name":"High Temp Warning","condition_type":"temp_above","threshold":230,"enabled":true},{"name":"FC Approaching","condition_type":"temp_above","threshold":195,"enabled":true},{"name":"Low RoR Warning","condition_type":"ror_below","threshold":5.0,"reference_event":"first_crack_start","enabled":true}]');

CREATE TRIGGER IF NOT EXISTS update_roast_sessions_updated_at
    AFTER UPDATE ON roast_sessions
    FOR EACH ROW
BEGIN
    UPDATE roast_sessions SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS update_roast_profiles_updated_at
    AFTER UPDATE ON roast_profiles
    FOR EACH ROW
BEGIN
    UPDATE roast_profiles SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS update_devices_updated_at
    AFTER UPDATE ON devices
    FOR EACH ROW
BEGIN
    UPDATE devices SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS update_device_profiles_updated_at
    AFTER UPDATE ON device_profiles
    FOR EACH ROW
BEGIN
    UPDATE device_profiles SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS update_device_connections_updated_at
    AFTER UPDATE ON device_connections
    FOR EACH ROW
BEGIN
    UPDATE device_connections SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;
