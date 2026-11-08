-- The label studio (docs/labels.md, "Label studio"): saved label designs
-- and every version of each (a reprint matches its version), assets a design draws on
-- (images and fonts), brand kits, and the label a coffee (bean) gets by default. In the
-- database so the nightly backup has them.
CREATE TABLE label_designs (
    id TEXT PRIMARY KEY,         -- letters, digits, - and _: what `design: "…"` names
    name TEXT NOT NULL,
    description TEXT,
    version INTEGER NOT NULL,    -- the current one
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE label_design_versions (
    design_id TEXT NOT NULL REFERENCES label_designs (id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    source TEXT NOT NULL,        -- the Typst file
    manifest TEXT,               -- JSON: its `design` dictionary when it was saved
    note TEXT,
    created_by TEXT NOT NULL,    -- owner, or the token's name
    created_at TEXT NOT NULL,
    PRIMARY KEY (design_id, version)
);

CREATE TABLE label_assets (
    name TEXT PRIMARY KEY,       -- what a design names: image("logo.svg")
    kind TEXT NOT NULL,          -- image | font
    content_type TEXT NOT NULL,
    data BLOB NOT NULL,
    size INTEGER NOT NULL,
    sha256 TEXT NOT NULL,
    family TEXT,                 -- a font's family, as a design names it
    note TEXT,                   -- where it came from, its licence
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE label_brands (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,          -- the roaster's name on a label
    tagline TEXT,
    place TEXT,                  -- city and state: for a label on coffee that is sold
    website TEXT,
    logo TEXT,                   -- an asset's name
    ink TEXT,                    -- colours, #rrggbb
    accent TEXT,
    paper TEXT,
    font_display TEXT,           -- font families
    font_body TEXT,
    font_script TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- A coffee's own label: a design ("sam-gift", or "sam-gift@3" for one version), its
-- parameters (JSON) and a brand
ALTER TABLE beans ADD COLUMN label_design TEXT;
ALTER TABLE beans ADD COLUMN label_params TEXT;
ALTER TABLE beans ADD COLUMN label_brand TEXT;
