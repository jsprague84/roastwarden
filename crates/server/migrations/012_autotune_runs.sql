-- Migration: 012_autotune_runs.sql
-- Autotune results per device with the fan they ran at (docs/autotune-2.md,
-- "Stored gains and undo"), and every stored PID gain change with the gains it replaced, so
-- a change can be undone and a gain schedule filled one fan level at a time.

CREATE TABLE autotune_runs (
    id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL,
    created_at DATETIME NOT NULL,
    state TEXT NOT NULL,              -- complete | failed
    quality TEXT,                     -- good | acceptable | poor
    message TEXT,
    fan_native INTEGER,               -- as the device reported it
    fan_pct REAL,
    target_temperature REAL,
    test_temperature REAL,
    ku REAL,                          -- device PID units: 0..255 output per °C
    tu REAL,                          -- s
    bias REAL,                        -- holding power at the test temperature, %
    tuning_method TEXT,
    warnings TEXT NOT NULL DEFAULT '[]', -- JSON array
    payload TEXT NOT NULL             -- the results message, JSON
);
CREATE INDEX idx_autotune_runs_device ON autotune_runs(device_id, created_at);

CREATE TABLE pid_changes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id TEXT NOT NULL,
    created_at DATETIME NOT NULL,
    kp REAL NOT NULL,
    ki REAL NOT NULL,
    kd REAL NOT NULL,
    -- Gains in use before the change; NULL when the server did not know them.
    prev_kp REAL,
    prev_ki REAL,
    prev_kd REAL,
    source TEXT NOT NULL,             -- manual | autotune | autotune_apply | undo
    run_id TEXT,
    rule TEXT,
    undone INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_pid_changes_device ON pid_changes(device_id, id);
