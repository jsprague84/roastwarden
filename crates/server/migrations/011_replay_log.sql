-- Migration: 011_replay_log.sql
-- Replay engine (docs/replay.md): one row per replay run and a log of every
-- discrete action it took, so a replayed roast can be reproduced and compared.

CREATE TABLE replay_runs (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    profile_id TEXT,
    profile_name TEXT,
    mode TEXT NOT NULL,          -- setpoint | feedforward | feedforward_pid
    trigger_policy TEXT NOT NULL, -- profile | time | bt
    align_on TEXT NOT NULL,      -- charge | turning_point | drying_end | first_crack_start
    ghost INTEGER NOT NULL DEFAULT 0,
    config TEXT NOT NULL,        -- full run configuration, JSON
    started_at DATETIME NOT NULL,
    ended_at DATETIME,
    end_reason TEXT,

    FOREIGN KEY (session_id) REFERENCES roast_sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (profile_id) REFERENCES roast_profiles(id) ON DELETE SET NULL
);
CREATE INDEX idx_replay_runs_session ON replay_runs(session_id);

-- `elapsed` is seconds from the session start, `live_t` from CHARGE,
-- `profile_t` the profile time after alignment.
CREATE TABLE replay_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    at DATETIME NOT NULL,
    elapsed REAL NOT NULL,
    live_t REAL NOT NULL,
    profile_t REAL NOT NULL,
    kind TEXT NOT NULL,
    channel TEXT,
    event_id TEXT,
    action TEXT,
    value REAL,
    label TEXT,
    detail TEXT,
    ghost INTEGER NOT NULL DEFAULT 0,

    FOREIGN KEY (run_id) REFERENCES replay_runs(id) ON DELETE CASCADE,
    FOREIGN KEY (session_id) REFERENCES roast_sessions(id) ON DELETE CASCADE
);
CREATE INDEX idx_replay_log_session ON replay_log(session_id, id);
