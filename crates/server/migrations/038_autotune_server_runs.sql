-- Autotune 2 (docs/autotune-2.md): the server runs the relay experiment, so
-- a run keeps where it came from, the experiment's summary (its switches and settings), the
-- whole record (time, heater as applied, BT, fan) and the analysis (models, designs, quality).
-- Firmware runs keep their results message in `payload` as before.
ALTER TABLE autotune_runs ADD COLUMN source TEXT NOT NULL DEFAULT 'firmware';
ALTER TABLE autotune_runs ADD COLUMN samples TEXT;
ALTER TABLE autotune_runs ADD COLUMN analysis TEXT;
