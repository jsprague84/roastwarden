-- Backups (docs/DEPLOY.md "Backups"): one row per attempt, nightly or by hand.
CREATE TABLE IF NOT EXISTS backup_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    trigger TEXT NOT NULL,          -- nightly | manual
    started_at TEXT NOT NULL,
    file_name TEXT,                 -- set when the copy was written
    size_bytes INTEGER,
    removed INTEGER NOT NULL DEFAULT 0,  -- older copies retention deleted
    error TEXT                      -- set when it failed
);
CREATE INDEX IF NOT EXISTS idx_backup_runs_started ON backup_runs(trigger, started_at DESC);

-- The raw telemetry log (every payload, kept 7 days) has no reader left: live views use the
-- 2 h ring in memory and roasts keep their own session_telemetry. It was most of the file.
DROP INDEX IF EXISTS idx_telemetry_device_ts;
DROP TABLE IF EXISTS telemetry;
