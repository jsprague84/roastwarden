-- The shopping list (docs/green-coffee.md, "Shopping list"): coffees wanted, often from a
-- supplier's link (the import's draft kept as JSON). "Bought" turns one into a purchase: the
-- bean (made from the draft when it is new) and a stock move with the price.
CREATE TABLE wanted (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    supplier TEXT,
    url TEXT,
    bean_id TEXT,              -- an existing bean to buy again
    size_label TEXT,           -- "5 LB"
    weight_g REAL,
    price REAL,                -- for that size, when it was seen
    currency TEXT,
    notes TEXT,
    draft TEXT,                -- the import's draft (JSON), to make the bean from
    status TEXT NOT NULL DEFAULT 'wanted',   -- wanted | bought
    move_id TEXT,              -- the purchase it became
    bought_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX idx_wanted_status ON wanted (status);

CREATE TRIGGER bean_delete_wanted AFTER DELETE ON beans
BEGIN
    UPDATE wanted SET bean_id = NULL WHERE bean_id = OLD.id;
END;
