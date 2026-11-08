-- Roast guidance (docs/roast-guidance.md): what a profile is for and what it
-- expects. Levels are the SCA colour tiles on ground Agtron Gourmet.
ALTER TABLE roast_profiles ADD COLUMN roast_level TEXT;    -- extremely_light | very_light | light | medium_light | medium | medium_dark | dark
ALTER TABLE roast_profiles ADD COLUMN roast_purpose TEXT;  -- espresso | filter | omni | other
ALTER TABLE roast_profiles ADD COLUMN expectations TEXT;   -- JSON: overrides of the level defaults, {"weight_loss_pct":[14,16],...}

-- The seed profiles say what they are for.
UPDATE roast_profiles SET roast_level = 'very_light', roast_purpose = 'filter' WHERE id = 'fluidbed-light';
UPDATE roast_profiles SET roast_level = 'medium', roast_purpose = 'filter' WHERE id = 'fluidbed-medium';
UPDATE roast_profiles SET roast_level = 'light', roast_purpose = 'espresso' WHERE id = 'fluidbed-espresso';
UPDATE roast_profiles SET roast_level = 'light' WHERE id = 'default-light';
UPDATE roast_profiles SET roast_level = 'medium' WHERE id = 'default-medium';
UPDATE roast_profiles SET roast_level = 'dark' WHERE id = 'default-dark';
