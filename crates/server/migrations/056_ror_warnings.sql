-- The RoR watch's warnings (docs/prediction.md "RoR warnings"): per roast,
-- the first time each kind (crash, flick, stall) was warned once it held for two predictions,
-- what it said then, and the last time it still stood. A roast's score sets them against
-- what its RoR did.
CREATE TABLE roast_ror_warnings (
    session_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    -- s since CHARGE: when first warned, when it was expected then (or began, when seen)
    first_t REAL NOT NULL,
    expected_t REAL NOT NULL,
    -- 1: seen in the RoR; 0: expected by the model
    seen INTEGER NOT NULL,
    reason TEXT NOT NULL,
    last_t REAL NOT NULL,
    PRIMARY KEY (session_id, kind)
);

CREATE TRIGGER roast_ror_warnings_session_deleted AFTER DELETE ON roast_sessions BEGIN
    DELETE FROM roast_ror_warnings WHERE session_id = OLD.id;
END;
