-- Purchases with what they cost (docs/green-coffee.md): a purchase move carries its price,
-- shipping and fees; the bean's cost per kg is the moving weighted average of what is left
-- (costs.rs). Moves of one order share an order_id (its shipping split by weight).
ALTER TABLE bean_stock_moves ADD COLUMN price REAL;      -- the coffee, for this move's grams
ALTER TABLE bean_stock_moves ADD COLUMN shipping REAL;   -- this move's share of the shipping
ALTER TABLE bean_stock_moves ADD COLUMN fees REAL;       -- its share of taxes, duty, fees
ALTER TABLE bean_stock_moves ADD COLUMN currency TEXT;   -- ISO 4217, upper case
ALTER TABLE bean_stock_moves ADD COLUMN supplier TEXT;
ALTER TABLE bean_stock_moves ADD COLUMN crop_year TEXT;  -- "2026" or "2025/26"
ALTER TABLE bean_stock_moves ADD COLUMN arrival TEXT;    -- when the lot landed at the supplier ("2026-07")
ALTER TABLE bean_stock_moves ADD COLUMN order_id TEXT;
CREATE INDEX idx_bean_stock_moves_order ON bean_stock_moves (order_id);
