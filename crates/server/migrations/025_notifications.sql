-- Notification channels (docs/notifications.md, crates/server/src/notify.rs).

-- The alarm webhook becomes one channel among several, and carries system events too.
INSERT OR IGNORE INTO settings (key, value, updated_at)
    SELECT 'notify_webhook_url', value, datetime('now') FROM settings
    WHERE key = 'alarm_webhook_url' AND trim(value) != '';
INSERT OR IGNORE INTO settings (key, value, updated_at)
    SELECT 'notify_webhook_enabled', 'true', datetime('now') FROM settings
    WHERE key = 'alarm_webhook_url' AND trim(value) != '';
DELETE FROM settings WHERE key = 'alarm_webhook_url';

-- System events (backup failed, roaster offline, low stock) are sent once when something
-- starts failing and once when it recovers. The state lives here so a restart neither
-- repeats a failure nor forgets one.
CREATE TABLE IF NOT EXISTS notify_state (
    key TEXT PRIMARY KEY,           -- e.g. "backup", "offline:esp32_roaster_01", "stock:<bean id>"
    failing INTEGER NOT NULL,       -- 1 while the problem lasts
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    changed_at TEXT NOT NULL
);
