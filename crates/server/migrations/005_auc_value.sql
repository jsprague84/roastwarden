-- Migration: 005_auc_value.sql
-- Add AUC (Area Under the Curve) column to roast_sessions.

ALTER TABLE roast_sessions ADD COLUMN auc_value REAL;