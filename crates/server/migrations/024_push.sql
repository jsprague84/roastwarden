-- Web Push (docs/push.md, crates/server/src/push.rs). The server's VAPID key pair, made on
-- first use: the private key is a secret, so it lives here and not in `settings`, which the
-- API lists.
CREATE TABLE IF NOT EXISTS push_vapid (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    private_key TEXT NOT NULL,      -- base64url, 32 bytes (P-256 scalar)
    public_key TEXT NOT NULL,       -- base64url, 65 bytes (uncompressed point)
    created_at TEXT NOT NULL
);

-- Browsers that turned alarm notifications on. The endpoint and keys let anyone push to the
-- device, so the API never shows them.
CREATE TABLE IF NOT EXISTS push_subscriptions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint TEXT NOT NULL UNIQUE,
    p256dh TEXT NOT NULL,           -- base64url, the browser's P-256 public key
    auth TEXT NOT NULL,             -- base64url, 16 bytes
    name TEXT NOT NULL,             -- e.g. "Chrome on Android"
    service TEXT NOT NULL,          -- the push service's host
    subject TEXT NOT NULL,          -- VAPID contact: the dashboard's https origin
    created_at TEXT NOT NULL,
    last_ok_at TEXT,
    last_error TEXT
);
