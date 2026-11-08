-- Batches and beans (docs/batches.md): green coffee lots with a stock ledger, a queue of
-- planned roasts, and a batch number per roaster.
CREATE TABLE beans (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    origin TEXT,
    variety TEXT,
    process TEXT,
    lot TEXT,
    supplier TEXT,
    notes TEXT,
    moisture_pct REAL,
    density_g_l REAL,
    low_stock_g REAL NOT NULL DEFAULT 500,
    archived INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Stock = SUM(delta_g). kind: purchase | adjust | roast (one per roast, kept by triggers).
CREATE TABLE bean_stock_moves (
    id TEXT PRIMARY KEY,
    bean_id TEXT NOT NULL,
    at TEXT NOT NULL,
    delta_g REAL NOT NULL,
    kind TEXT NOT NULL,
    session_id TEXT,
    note TEXT
);
CREATE INDEX idx_bean_stock_moves_bean ON bean_stock_moves (bean_id);
CREATE INDEX idx_bean_stock_moves_session ON bean_stock_moves (session_id);

-- The queue: session_id NULL = still planned.
CREATE TABLE planned_roasts (
    id TEXT PRIMARY KEY,
    position REAL NOT NULL,
    device_id TEXT,
    bean_id TEXT,
    profile_id TEXT,
    green_weight_g REAL,
    name TEXT,
    notes TEXT,
    session_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX idx_planned_roasts_session ON planned_roasts (session_id);

ALTER TABLE roast_sessions ADD COLUMN bean_id TEXT;
ALTER TABLE roast_sessions ADD COLUMN batch_number INTEGER;
ALTER TABLE roast_sessions ADD COLUMN planned_roast_id TEXT;

-- Number the roasts so far per roaster, oldest first.
UPDATE roast_sessions SET batch_number = (
    SELECT n FROM (
        SELECT id, ROW_NUMBER() OVER (PARTITION BY device_id ORDER BY created_at, id) AS n
        FROM roast_sessions
    ) r WHERE r.id = roast_sessions.id
);

-- A roast with a bean and a green weight has exactly one 'roast' move of minus that weight.
CREATE TRIGGER roast_draws_stock_insert AFTER INSERT ON roast_sessions
WHEN NEW.bean_id IS NOT NULL AND NEW.green_weight > 0
BEGIN
    INSERT INTO bean_stock_moves (id, bean_id, at, delta_g, kind, session_id)
    VALUES (lower(hex(randomblob(16))), NEW.bean_id, NEW.created_at, -NEW.green_weight, 'roast', NEW.id);
END;

CREATE TRIGGER roast_draws_stock_update AFTER UPDATE OF bean_id, green_weight ON roast_sessions
BEGIN
    DELETE FROM bean_stock_moves WHERE session_id = NEW.id AND kind = 'roast';
    INSERT INTO bean_stock_moves (id, bean_id, at, delta_g, kind, session_id)
    SELECT lower(hex(randomblob(16))), NEW.bean_id, NEW.created_at, -NEW.green_weight, 'roast', NEW.id
    WHERE NEW.bean_id IS NOT NULL AND NEW.green_weight > 0;
END;

-- A deleted (cancelled) roast gives its beans back and its plan returns to the queue.
CREATE TRIGGER roast_delete_inventory AFTER DELETE ON roast_sessions
BEGIN
    DELETE FROM bean_stock_moves WHERE session_id = OLD.id;
    UPDATE planned_roasts SET session_id = NULL WHERE session_id = OLD.id;
END;

CREATE TRIGGER bean_delete_inventory AFTER DELETE ON beans
BEGIN
    DELETE FROM bean_stock_moves WHERE bean_id = OLD.id;
    UPDATE planned_roasts SET bean_id = NULL WHERE bean_id = OLD.id;
END;

CREATE TRIGGER profile_delete_plans AFTER DELETE ON roast_profiles
BEGIN
    UPDATE planned_roasts SET profile_id = NULL WHERE profile_id = OLD.id;
END;
