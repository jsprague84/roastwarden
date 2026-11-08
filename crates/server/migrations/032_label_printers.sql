-- Label printers (docs/labels.md, "Printers"): more than one, each
-- with what it said it can do when last asked, the tray and colour to ask for, how to send
-- (PDF, or raster for a printer without PDF), the certificate the owner trusted for ipps,
-- and its own calibration per sheet (a printer feeds paper its own way).
CREATE TABLE label_printers (
    id TEXT PRIMARY KEY,                -- letters, digits, - and _
    name TEXT NOT NULL,
    url TEXT NOT NULL,                  -- ipp://host:631/ipp/print, ipps://…
    is_default INTEGER NOT NULL DEFAULT 0,
    tray TEXT,                          -- media-source to ask for (none: the tray holding the sheet's size)
    color TEXT,                         -- monochrome, color (none: monochrome unless the design has colour)
    send TEXT NOT NULL DEFAULT 'auto',  -- auto (PDF, else PWG Raster, else URF), pdf, pwg, urf
    fingerprint TEXT,                   -- ipps: SHA-256 of the certificate the owner trusted, hex
    info TEXT,                          -- JSON: what Get-Printer-Attributes answered last
    info_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE label_calibrations (
    printer_id TEXT NOT NULL REFERENCES label_printers (id) ON DELETE CASCADE,
    sheet TEXT NOT NULL,
    x_mm REAL NOT NULL,                 -- right
    y_mm REAL NOT NULL,                 -- down
    tray TEXT,                          -- the tray it was measured from
    updated_at TEXT NOT NULL,
    PRIMARY KEY (printer_id, sheet)
);

-- The printer set so far (the label_printer_url setting) becomes the default one
INSERT INTO label_printers (id, name, url, is_default, send, created_at, updated_at)
SELECT 'label-printer', 'Label printer', trim(value), 1, 'auto',
       strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
FROM settings WHERE key = 'label_printer_url' AND trim(value) != '';

-- Which printer a run was laid out for (its calibration) and sent to
ALTER TABLE label_runs ADD COLUMN printer_id TEXT;
