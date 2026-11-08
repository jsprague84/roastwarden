-- Firmware 3.5.0 has no onboard autotune (Autotune 2 runs it from the server), and the server
-- no longer listens on roaster/{id}/autotune/#: drop the raw status and results messages it
-- kept. The firmware runs made from them stay in autotune_runs.
DROP INDEX IF EXISTS idx_autotune_status_device_ts;
DROP INDEX IF EXISTS idx_autotune_results_device_ts;
DROP TABLE IF EXISTS autotune_status;
DROP TABLE IF EXISTS autotune_results;
