-- Roast properties (purpose, colour, moisture, density, flags, a rating) and tracking
-- against the profile (how far BT ran from it).
ALTER TABLE roast_sessions ADD COLUMN roast_purpose TEXT;      -- espresso | filter | omni | other
ALTER TABLE roast_sessions ADD COLUMN color_whole REAL;        -- on color_scale
ALTER TABLE roast_sessions ADD COLUMN color_ground REAL;
ALTER TABLE roast_sessions ADD COLUMN color_scale TEXT;        -- agtron | tonino | lebrew | other
ALTER TABLE roast_sessions ADD COLUMN moisture_green REAL;     -- %
ALTER TABLE roast_sessions ADD COLUMN moisture_roasted REAL;   -- %
ALTER TABLE roast_sessions ADD COLUMN density_green REAL;      -- g/L
ALTER TABLE roast_sessions ADD COLUMN roast_flags TEXT;        -- comma list: underdeveloped, scorching, tipping, ...
ALTER TABLE roast_sessions ADD COLUMN rating INTEGER;          -- 1..5 quick verdict
ALTER TABLE roast_sessions ADD COLUMN profile_rmse REAL;       -- BT vs profile, TP..DROP, °C
ALTER TABLE roast_sessions ADD COLUMN profile_max_dev REAL;    -- largest |BT - profile|, °C
ALTER TABLE roast_sessions ADD COLUMN profile_bias REAL;       -- mean BT - profile, °C
