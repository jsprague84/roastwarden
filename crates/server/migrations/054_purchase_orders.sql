-- Orders from receipts (docs/green-coffee.md, "Orders"): a supplier's order with its
-- own number, its lines and totals as the receipt has them, and the receipts themselves.
-- ordered → shipped → received; only received writes the purchase moves (their order_id is
-- this order's id). Purchases recorded before (a fresh UUID as order_id, no row here) stay.
CREATE TABLE purchase_orders (
    id TEXT PRIMARY KEY,
    supplier TEXT NOT NULL,
    supplier_key TEXT NOT NULL,          -- lower case, trimmed: one order number per supplier
    order_number TEXT NOT NULL,
    placed_at TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'ordered'
        CHECK (status IN ('ordered', 'shipped', 'received', 'cancelled')),
    currency TEXT,                       -- ISO 4217; absent: the home currency
    subtotal REAL,
    discount REAL NOT NULL DEFAULT 0,
    shipping REAL NOT NULL DEFAULT 0,
    tax REAL NOT NULL DEFAULT 0,
    total REAL NOT NULL,
    mismatch_reason TEXT,                -- why totals that do not add up were accepted
    carrier TEXT,
    tracking TEXT,
    shipped_at TEXT,
    received_at TEXT,
    note TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (supplier_key, order_number)
);
CREATE INDEX idx_purchase_orders_status ON purchase_orders (status);

CREATE TABLE purchase_order_lines (
    id TEXT PRIMARY KEY,
    order_id TEXT NOT NULL REFERENCES purchase_orders (id) ON DELETE CASCADE,
    position INTEGER NOT NULL,           -- 1, 2, … as on the receipt
    kind TEXT NOT NULL DEFAULT 'coffee' CHECK (kind IN ('coffee', 'other')),
    name TEXT NOT NULL,
    sku TEXT,
    variant_id TEXT,                     -- the shop's id for the bag (a cart link's)
    bag TEXT,                            -- "1 LB"
    quantity REAL NOT NULL,
    weight_g REAL,                       -- all of the line: bag × quantity
    price REAL NOT NULL,                 -- the line's price, all of it
    taxed INTEGER,                       -- the receipt says tax was charged on it
    url TEXT,
    bean_id TEXT REFERENCES beans (id) ON DELETE SET NULL,
    wanted_id TEXT,                      -- the shopping-list entry it fills
    move_id TEXT,                        -- the purchase written when received
    received_g REAL                      -- what arrived, when it differs
);
CREATE INDEX idx_purchase_order_lines_order ON purchase_order_lines (order_id);
CREATE INDEX idx_purchase_order_lines_bean ON purchase_order_lines (bean_id);

-- The receipts as received (email, HTML, PDF, image): private, they carry the owner's address,
-- phone and card digits. In the database so the nightly backups keep them.
CREATE TABLE purchase_receipts (
    id TEXT PRIMARY KEY,
    order_id TEXT NOT NULL REFERENCES purchase_orders (id) ON DELETE CASCADE,
    kind TEXT NOT NULL DEFAULT 'confirmation'
        CHECK (kind IN ('confirmation', 'shipping', 'invoice', 'other')),
    content_type TEXT NOT NULL,
    file_name TEXT,
    message_id TEXT,
    sha256 TEXT NOT NULL UNIQUE,
    size_bytes INTEGER NOT NULL,
    body BLOB NOT NULL,
    added_at TEXT NOT NULL
);
CREATE INDEX idx_purchase_receipts_order ON purchase_receipts (order_id);

-- The shopping list: an entry an order line fills is "ordered" until the order is received.
ALTER TABLE wanted ADD COLUMN order_line_id TEXT;
