-- Migration: 009_alarm_webhook.sql
-- Optional webhook that receives roast alarms as JSON (n8n, ntfy, Home Assistant, ...).
INSERT OR IGNORE INTO settings (key, value) VALUES ('alarm_webhook_url', '');
