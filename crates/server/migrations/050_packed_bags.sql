-- Packed bags (docs/roasted.md, "Packed bags"): a packed move is a number of
-- bags of a size, whole bean or ground with a grind. One made by a label print run names it.
ALTER TABLE roasted_moves ADD COLUMN bags INTEGER;
ALTER TABLE roasted_moves ADD COLUMN bag_g REAL;
ALTER TABLE roasted_moves ADD COLUMN form TEXT;
ALTER TABLE roasted_moves ADD COLUMN grind TEXT;
ALTER TABLE roasted_moves ADD COLUMN label_run TEXT;
CREATE INDEX idx_roasted_moves_run ON roasted_moves (label_run);
