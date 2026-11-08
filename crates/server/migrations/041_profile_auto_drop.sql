-- Auto DROP (docs/replay.md "Auto DROP"): a profile can let the drive mark
-- DROP by itself at its drop, by time, BT or development time after first crack. NULL is off,
-- the default: every profile starts with it off.
ALTER TABLE roast_profiles ADD COLUMN auto_drop TEXT
    CHECK (auto_drop IS NULL OR auto_drop IN ('time', 'bt', 'development'));
