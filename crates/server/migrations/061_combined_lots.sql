-- Combined lots (docs/roasted.md, "Roasts in one bag"): roasts of one coffee whose coffee went
-- into one bag or jar. Each roast keeps its own moves; a move on the lot is split across its
-- roasts by what each has left, the rows sharing `combine_move` so they are taken back together.
CREATE TABLE roasted_combines (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT,
    note TEXT,
    created_at TEXT NOT NULL
);
CREATE TABLE roasted_combine_roasts (
    session_id TEXT PRIMARY KEY NOT NULL,
    combine_id TEXT NOT NULL,
    added_at TEXT NOT NULL
);
CREATE INDEX idx_roasted_combine_roasts_combine ON roasted_combine_roasts (combine_id);
ALTER TABLE roasted_moves ADD COLUMN combine_move TEXT;
CREATE INDEX idx_roasted_moves_combine ON roasted_moves (combine_move);

-- A deleted roast leaves its lot; a lot left with one roast is no lot.
CREATE TRIGGER roast_delete_combined AFTER DELETE ON roast_sessions
BEGIN
    DELETE FROM roasted_combine_roasts WHERE session_id = OLD.id;
    DELETE FROM roasted_combine_roasts WHERE combine_id IN
        (SELECT combine_id FROM roasted_combine_roasts GROUP BY combine_id HAVING COUNT(*) < 2);
    DELETE FROM roasted_combines WHERE id NOT IN (SELECT combine_id FROM roasted_combine_roasts);
END;
