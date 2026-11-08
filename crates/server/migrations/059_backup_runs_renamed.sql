-- The rename to Roastwarden (0.25.0): legacy.rs renames every backup `rustroast-<time>.db` to
-- `roastwarden-<time>.db` at start, so the backup runs that named the old files name the new
-- ones (Settings → Backups and `roast status` showed a last copy that no longer existed).
UPDATE backup_runs
SET file_name = 'roastwarden-' || substr(file_name, length('rustroast-') + 1)
WHERE file_name LIKE 'rustroast-%';
