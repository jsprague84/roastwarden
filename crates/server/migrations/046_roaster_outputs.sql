-- Each roaster's outputs and fan floor (docs/roaster-setup.md, "Outputs").
-- `outputs`: JSON {"heater": {label, min, max, step} | null, "fan": … | null};
-- `fan_floor_pct`: while the heater is on, the fan never goes below it (0 = none). NULL in
-- either means the roaster's kind's defaults, so every row that exists keeps what it does today:
-- the fluid bed's heater and fan 0–100 % and its 45 % floor.
ALTER TABLE roaster_setup ADD COLUMN outputs TEXT;
ALTER TABLE roaster_setup ADD COLUMN fan_floor_pct REAL;
