-- Each roaster's setup (docs/roaster-setup.md): what kind of roaster it is and
-- what rustRoast may send it. `control`: off (logs only: nothing is sent), manual (heater, fan,
-- heater on/off, emergency stop), full (everything). A roaster with no row is off, so one added
-- or discovered from now on logs only until the owner switches its control on. Every roaster
-- that exists now keeps what it does today: full, as the fluid bed it is.
CREATE TABLE IF NOT EXISTS roaster_setup (
    device_id TEXT PRIMARY KEY,
    kind TEXT NOT NULL DEFAULT 'other',
    control TEXT NOT NULL DEFAULT 'off' CHECK (control IN ('off', 'manual', 'full')),
    updated_at TEXT NOT NULL
);

INSERT OR IGNORE INTO roaster_setup (device_id, kind, control, updated_at)
SELECT device_id, 'fluid_bed_electric', 'full', strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
FROM devices;
