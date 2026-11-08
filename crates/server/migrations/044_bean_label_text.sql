-- What a bean's labels say (docs/labels.md, "What goes on a label"): a few tasting words in
-- the owner's words ("chocolate · coconut · apple") instead of the supplier's prose, and a
-- short name for the label when the bean's own name is long ("Kenya Gatugi").
ALTER TABLE beans ADD COLUMN label_notes TEXT;
ALTER TABLE beans ADD COLUMN short_name TEXT;
