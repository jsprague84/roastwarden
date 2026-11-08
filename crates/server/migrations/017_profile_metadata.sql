-- What a profile was made for, as Artisan profiles carry it (beans,
-- weight in, roastertype): imported from and exported to .alog.
ALTER TABLE roast_profiles ADD COLUMN beans TEXT;
ALTER TABLE roast_profiles ADD COLUMN batch_size_g REAL;   -- green weight the profile was roasted with
ALTER TABLE roast_profiles ADD COLUMN roaster TEXT;        -- machine, free text (Artisan roastertype)
