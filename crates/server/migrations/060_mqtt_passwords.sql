-- Roasters' MQTT passwords added from the dashboard (mqtt_passwords.rs; docs/add-roaster.md,
-- "MQTT passwords"): the built-in broker lets each one sign in as its roaster id, beside those
-- in MQTT_DEVICES, at once and without a restart. Kept as written: the firmware download fills
-- the password into the sketch.
CREATE TABLE mqtt_passwords (
    roaster_id TEXT PRIMARY KEY NOT NULL,
    password   TEXT NOT NULL,
    created_at TEXT NOT NULL,
    changed_at TEXT NOT NULL
);
