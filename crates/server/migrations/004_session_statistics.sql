-- Migration: 004_session_statistics.sql
-- Add enhanced session completion statistics columns to roast_sessions.
-- Each ALTER TABLE is a separate statement so it silently fails if the column
-- already exists (SQLite does not support IF NOT EXISTS for ALTER TABLE).

ALTER TABLE roast_sessions ADD COLUMN weight_loss_pct REAL;
ALTER TABLE roast_sessions ADD COLUMN max_ror REAL;
ALTER TABLE roast_sessions ADD COLUMN avg_ror_drying REAL;
ALTER TABLE roast_sessions ADD COLUMN avg_ror_maillard REAL;
ALTER TABLE roast_sessions ADD COLUMN avg_ror_development REAL;
ALTER TABLE roast_sessions ADD COLUMN drying_end_time INTEGER;
ALTER TABLE roast_sessions ADD COLUMN drying_end_temp REAL;