-- Roasted coffee (docs/roasted.md): what is left of a roast is its roasted weight plus the sum
-- of its moves. kind: used | given | discarded | adjust (a count of what is left).
CREATE TABLE roasted_moves (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    at TEXT NOT NULL,
    delta_g REAL NOT NULL,
    kind TEXT NOT NULL,
    recipient TEXT,
    note TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX idx_roasted_moves_session ON roasted_moves (session_id);

-- A deleted roast takes its roasted coffee with it.
CREATE TRIGGER roast_delete_roasted AFTER DELETE ON roast_sessions
BEGIN
    DELETE FROM roasted_moves WHERE session_id = OLD.id;
END;
