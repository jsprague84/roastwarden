-- Create roast_events table for tracking roasting events
CREATE TABLE roast_events (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    elapsed_seconds REAL NOT NULL,
    temperature REAL,
    notes TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES roast_sessions(id) ON DELETE CASCADE
);

-- Create index for efficient session lookups
CREATE INDEX idx_roast_events_session_id ON roast_events(session_id);

-- Create index for ordering by elapsed time
CREATE INDEX idx_roast_events_elapsed_seconds ON roast_events(session_id, elapsed_seconds);