-- Audit log (SECURITY.md, docs/control.md "Audit log"): one row per state-changing API
-- request, refused ones included. Kept a year.
CREATE TABLE IF NOT EXISTS audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    at TEXT NOT NULL,               -- RFC 3339, UTC
    actor TEXT NOT NULL,            -- open | owner key | token | none (refused)
    token_id INTEGER,               -- the API token, when one was used
    token_name TEXT,
    remote_user TEXT,               -- the edge proxy's signed-in user, if it says
    client TEXT,                    -- User-Agent, shortened
    ip TEXT,                        -- X-Forwarded-For (first hop) or X-Real-IP
    method TEXT NOT NULL,
    path TEXT NOT NULL,
    device_id TEXT,                 -- the roaster the request was about, when known
    status INTEGER NOT NULL,
    body TEXT                       -- the JSON body, secrets masked, at most 4 KB
);
CREATE INDEX IF NOT EXISTS idx_audit_log_at ON audit_log(at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_log_device ON audit_log(device_id, at DESC);
