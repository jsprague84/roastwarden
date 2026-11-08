-- Label print runs (docs/labels.md, "Print runs"): what was made
-- when, on which sheet from which label, each label's coffee and design version, the data it
-- carried (a reprint matches), the printer's job and its states with times, and the PDF
-- itself. And the next free label on each part-used sheet.
CREATE TABLE label_runs (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    created_by TEXT NOT NULL,          -- owner, or the token's name
    sheet TEXT NOT NULL,               -- the sheet's id
    sheet_name TEXT NOT NULL,
    start INTEGER NOT NULL,            -- the first label used on the first page, from 1
    labels INTEGER NOT NULL,           -- how many labels
    pages INTEGER NOT NULL,
    items TEXT NOT NULL,               -- JSON: each item's roast, design@version and source,
                                       -- parameters, brand, copies and the label's data
    summary TEXT NOT NULL,             -- "Guji 1 (#7) ×2, Palmichal (#5)": for lists
    state TEXT NOT NULL,               -- made, sent, pending, processing, stopped, completed,
                                       -- aborted, canceled, failed, printed (by hand)
    states TEXT NOT NULL DEFAULT '[]', -- JSON: [{at, state, reasons, sheets}]
    printer TEXT,                      -- where it was sent
    job_id INTEGER,
    tray TEXT,
    pdf BLOB NOT NULL,                 -- what was made (and sent)
    reprint_of TEXT REFERENCES label_runs (id) ON DELETE SET NULL,
    note TEXT
);
CREATE INDEX label_runs_created ON label_runs (created_at);

CREATE TABLE label_sheet_use (
    sheet TEXT PRIMARY KEY,
    next_label INTEGER NOT NULL,       -- the next free label on the part-used sheet, from 1
    run_id TEXT,                       -- the run that left it so (none: set by hand)
    updated_at TEXT NOT NULL
);
