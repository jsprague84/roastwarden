-- The share page (docs/labels.md, "The share page"): what a bag's QR
-- code opens, a public page per roast at /R/<code>. The code is 25 base32 characters (125
-- random bits); revoking one makes it a 404 like an unknown code. Scans are counted without
-- addresses.
CREATE TABLE share_links (
    code TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES roast_sessions (id) ON DELETE CASCADE,
    message TEXT,                      -- a line for the person it is for, on the page
    tz_minutes INTEGER NOT NULL DEFAULT 0, -- the owner's offset from UTC, for the dates shown
    created_at TEXT NOT NULL,
    created_by TEXT NOT NULL,
    revoked_at TEXT,
    scans INTEGER NOT NULL DEFAULT 0,
    last_scan_at TEXT
);
CREATE INDEX share_links_session ON share_links (session_id);
