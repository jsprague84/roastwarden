-- The classic SCA form's defects (docs/cupping.md): cups with a taint take 2 points each off the
-- total, cups with a fault 4 each.
ALTER TABLE cupping_scores ADD COLUMN taint_cups INTEGER NOT NULL DEFAULT 0;
ALTER TABLE cupping_scores ADD COLUMN fault_cups INTEGER NOT NULL DEFAULT 0;

-- overall_score held the mean of the attributes. It is the final score now: the ten
-- attributes' sum less defects (none were recorded before), and null until all ten are scored.
UPDATE cupping_scores
SET overall_score = CASE
    WHEN (SELECT COUNT(*) FROM cupping_attributes a WHERE a.cupping_id = cupping_scores.id) = 10
    THEN (SELECT SUM(score) FROM cupping_attributes a WHERE a.cupping_id = cupping_scores.id)
    ELSE NULL
END;
