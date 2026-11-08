-- What suppliers publish about a coffee (docs/green-coffee.md, "Supplier details"). Fields a
-- supplier has but this table does not go into `details` (a JSON object of label: value), so
-- an import loses nothing.
ALTER TABLE beans ADD COLUMN region TEXT;
ALTER TABLE beans ADD COLUMN producer TEXT;             -- farm, station, cooperative or grower
ALTER TABLE beans ADD COLUMN altitude TEXT;             -- as published: "1900–2200 masl"
ALTER TABLE beans ADD COLUMN grade TEXT;                -- "Grade 1", "AA", "SHB EP"
ALTER TABLE beans ADD COLUMN screen_size TEXT;          -- "16-18"
ALTER TABLE beans ADD COLUMN certifications TEXT;       -- comma-separated: "Organic, Fairtrade"
ALTER TABLE beans ADD COLUMN drying TEXT;               -- "Raised beds"
ALTER TABLE beans ADD COLUMN score REAL;                -- the supplier's cupping score
ALTER TABLE beans ADD COLUMN roast_recommendation TEXT;
ALTER TABLE beans ADD COLUMN cupping_notes TEXT;
ALTER TABLE beans ADD COLUMN url TEXT;                  -- the offer
ALTER TABLE beans ADD COLUMN sku TEXT;
ALTER TABLE beans ADD COLUMN details TEXT;              -- JSON {label: value}
