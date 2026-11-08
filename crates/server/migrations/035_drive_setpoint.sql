-- Profile drive (docs/replay.md, "Lookahead"): the setpoint goes out in
-- 0.2 °C steps where it went out in 1 °C steps (a kick of kp·1 °C every 7–12 s late in a
-- roast), unless the setting was changed from the old default; and the lookahead adapts to
-- the loop's lag (on unless switched off).
UPDATE settings SET value = '0.2' WHERE key = 'follow_hysteresis_c' AND value IN ('1', '1.0');
INSERT OR IGNORE INTO settings (key, value) VALUES ('profile_lookahead_adapt', 'true');
