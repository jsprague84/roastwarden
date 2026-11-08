-- API tokens for scripts and agents (SECURITY.md, docs/control.md "Tokens";
-- crates/server/src/tokens.rs). A token acts with the access its preset names. Only the
-- SHA-256 of the secret is stored; `prefix` is its first characters, so a list can show which
-- token is which. `expires_at` NULL means it never expires. A revoked token keeps its row
-- (and its name in the audit log) until the owner removes it.
CREATE TABLE IF NOT EXISTS api_tokens (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    prefix TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    preset TEXT NOT NULL,           -- read_only | roast | admin
    created_at TEXT NOT NULL,       -- RFC 3339, UTC
    expires_at TEXT,
    last_used_at TEXT,
    last_used_ip TEXT,
    revoked_at TEXT
);

-- The real-hardware rule: a token may control a roaster other than the virtual one only when
-- the owner switched it on here (Roasters → the roaster → Control). Only the owner key can
-- write this table; it is not in /api/settings, which admin tokens may change.
CREATE TABLE IF NOT EXISTS roaster_token_access (
    device_id TEXT PRIMARY KEY,
    tokens_may_control INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);
