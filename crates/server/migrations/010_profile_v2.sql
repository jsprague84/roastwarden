-- Migration: 010_profile_v2.sql
-- Profile model v2: heater feedforward on points, replayable events, and
-- full background curves.

-- Points become feedforward curves: BT/ET target, fan %, and now heater %.
ALTER TABLE profile_points ADD COLUMN heater_pct INTEGER; -- 0-100

-- Artisan-style event replay. Each event can carry both references:
-- `at_seconds` (seconds from CHARGE) and `at_bt` (bean temperature in °C,
-- fires once on the way up, after the turning point). `trigger_kind` says
-- which one the event uses by default; a replay run may choose the other.
-- `value` carries numbers (setpoint °C, fan %, heater %); `label` carries
-- text (mode `auto`/`manual`, landmark event type, note text, or a display
-- label for numeric actions).
CREATE TABLE profile_events (
    id TEXT PRIMARY KEY,
    profile_id TEXT NOT NULL,
    trigger_kind TEXT NOT NULL CHECK (trigger_kind IN ('time', 'bt')),
    at_seconds REAL,
    at_bt REAL,
    action TEXT NOT NULL CHECK (action IN ('setpoint', 'fan', 'heater', 'mode', 'landmark', 'note')),
    value REAL,
    label TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CHECK ((trigger_kind = 'time' AND at_seconds IS NOT NULL)
        OR (trigger_kind = 'bt' AND at_bt IS NOT NULL)),
    FOREIGN KEY (profile_id) REFERENCES roast_profiles(id) ON DELETE CASCADE
);
CREATE INDEX idx_profile_events_profile ON profile_events(profile_id);

-- One background curve set per profile, sampled every `interval_seconds`
-- from CHARGE (t = 0). Arrays are JSON with nulls for gaps.
CREATE TABLE profile_curves (
    profile_id TEXT PRIMARY KEY,
    source_session_id TEXT,
    interval_seconds REAL NOT NULL DEFAULT 1,
    bt TEXT NOT NULL,
    et TEXT,
    ror TEXT,
    heater TEXT,
    fan TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (profile_id) REFERENCES roast_profiles(id) ON DELETE CASCADE,
    FOREIGN KEY (source_session_id) REFERENCES roast_sessions(id) ON DELETE SET NULL
);
