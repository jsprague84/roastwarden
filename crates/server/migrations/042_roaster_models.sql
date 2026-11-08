-- Prediction from the roaster's own model (docs/prediction.md).
-- Each fit of a roaster's thermal model, newest last: its parameters, its leave-one-roast-out
-- score and the roasts it was fitted on.
CREATE TABLE roaster_models (
    id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL,
    fitted_at TEXT NOT NULL,
    model TEXT NOT NULL,
    quality TEXT NOT NULL,
    roasts INTEGER NOT NULL,
    samples INTEGER NOT NULL,
    session_ids TEXT NOT NULL
);
CREATE INDEX idx_roaster_models_device ON roaster_models(device_id, fitted_at);

-- What the live prediction said during a roast, every 15 s from CHARGE + 30 s to DROP, and
-- BT when the time came: the prediction's error, kept with the roast.
CREATE TABLE roast_predictions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    model_id TEXT,
    t REAL NOT NULL,
    plan TEXT NOT NULL,
    bt REAL NOT NULL,
    pred_60 REAL NOT NULL,
    pred_120 REAL NOT NULL,
    proj_60 REAL,
    proj_120 REAL,
    actual_60 REAL,
    actual_120 REAL,
    fc_t REAL,
    drop_t REAL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (session_id, t)
);
CREATE INDEX idx_roast_predictions_session ON roast_predictions(session_id, t);

CREATE TRIGGER roast_delete_predictions AFTER DELETE ON roast_sessions
BEGIN
    DELETE FROM roast_predictions WHERE session_id = OLD.id;
END;
