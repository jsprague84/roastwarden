-- The rename to Roastwarden (0.25.0): the label helpers a design imports are
-- `roastwarden.typ` now, so every saved design version that imported `rustroast.typ` imports
-- the new name (the same helpers; a reprint of an old version still renders).
UPDATE label_design_versions
SET source = replace(source, '"rustroast.typ"', '"roastwarden.typ"')
WHERE source LIKE '%"rustroast.typ"%';
