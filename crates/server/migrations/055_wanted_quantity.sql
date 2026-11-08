-- Cart links (docs/green-coffee.md, "Cart links"): how many bags of the chosen size.
ALTER TABLE wanted ADD COLUMN quantity INTEGER NOT NULL DEFAULT 1;
