-- Signed-in browsers (SECURITY.md, "Logins"; crates/server/src/http/logins.rs). A browser
-- trades the owner key or an API token for a random cookie once; the dashboard never keeps
-- the key itself. Only the SHA-256 of the cookie's secret is stored. An owner login also keeps
-- `key_proof`, the SHA-256 of the owner key and that secret together: a new owner key signs
-- every owner login out, and nothing here reveals the key. A token's login ends with its token
-- (revoked, expired or removed). `expires_at` moves on with use (30 days from the last renewal).
CREATE TABLE IF NOT EXISTS logins (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    secret_hash TEXT NOT NULL UNIQUE,
    key_proof TEXT,
    token_id INTEGER REFERENCES api_tokens(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,      -- RFC 3339, UTC
    renewed_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    last_used_at TEXT,
    last_used_ip TEXT,
    user_agent TEXT,
    CHECK ((key_proof IS NULL) <> (token_id IS NULL))
);
CREATE INDEX IF NOT EXISTS logins_token ON logins(token_id);
