# Green coffee: purchases, costs, pick lists

What you paid for your green coffee, what each roast cost, and what to buy next. Roastwarden
records purchases with their shipping and fees, keeps each bean's cost per kg, and can read a
supplier's product page into a new bean or onto a shopping list. The bean, its stock and
planned roasts are in [batches and beans](batches.md).

Prices, the shopping list, orders and spending are part of the costs module: switch it on
under Settings → Modules (the Roastery preset includes it). Set your currency under Settings →
Roasting → Coffee costs (USD unless you change it).

## Recording a purchase

- **One bean**: Beans → the bean → **Purchase**: weight, price, shipping, supplier and date;
  **More** adds fees and taxes, a currency, crop year and arrival. The date is when you bought
  it.
- **An order of several coffees**: Beans → **Record an order**: one line per bean with its
  weight and price, and the order's shipping and fees, which are split over the lines by
  weight. The beans must exist first.
- **Fix one**: Edit on its row in the bean's ledger. A roast's own line belongs to the roast:
  change the roast's green weight or bean instead.

## What it cost

- **A bean's cost per kg** is the weighted average of what is left: each priced purchase, with
  its shipping and fees, mixes into the stock held then, and a roast takes beans out at that
  average. The bean's page shows it and what the stock is worth.
- **A roast's cost**: History → the roast → Coffee cost: its green weight at the bean's average
  when it started, and per kg roasted, per espresso dose, per filter dose and per bag (sizes
  under Settings → Roasting → Coffee costs: 18 g, 15 g and 250 g unless you change them).
- **Spending**, on the Beans page, by month: what you spent, kg bought, roasts, green and
  roasted kg, and what the green coffee of those roasts cost.
- **One currency.** A purchase in another currency is kept and shown in amber, but left out
  of the average rather than converted at a made-up rate.

## Adding a coffee from a link

Beans → **Add from a link**, then **Preview**: Roastwarden reads the product page and shows the
coffee with its details and every bag size, with prices and what is sold out. Nothing is saved
until you choose:

- **Only add the bean**, with every detail the supplier publishes (region, producer,
  altitude, process, grade, score, cupping notes, the link).
- **Put on the shopping list**, with a bag size.
- **Bought it**: the bean and its purchase at the price shown.

**Buy for** an existing bean to buy it again rather than make a new one. Supported suppliers:
Sweet Maria's, Bodhi Leaf, Theta Ridge, Coffee Bean Corral, Showroom, Happy Mug and Burman
Coffee Traders. A link to another shop is refused with the list. The server only reads that
one page, over https. When the store is busy or down it says so, and names the wait the store
asked for when it gave one. A page read in the last ten minutes is answered from memory, so
asking again costs the store nothing.

The bean form suggests values you used before and common ones (countries, processes,
varieties), and keeps a **Label name** and **Label notes** for bag labels ([labels](labels.md)).

## The shopping list

What you want before it is in stock: the coffee, its supplier and link, the bag size and how
many bags, and the price you saw. Beans → Shopping list → **Bought** records it when it comes:
the weight that arrived, price, shipping and date. That makes the bean (from the page you read)
and its purchase. To see today's price, preview the link again. With reports on, **As a PDF**
prints the list ([reports](reports.md)).

## Cart links

Beans → Shopping list → **Carts** makes one link per supplier that opens that supplier's cart
with your bags in it. You check out there yourself: Roastwarden never logs in, holds a cart or
pays. Each coffee is read again first, a few seconds apart, so a sold-out bag is left out and a
changed price is noted. A Shopify store's link replaces whatever was in that browser's cart.
An entry with no link, or at a store it cannot fill, is listed with the reason.

## Orders

The receipt is the record of what you paid: the confirmation email has every price, the
shipping and the tax. You hand it to your agent, which reads it and sends it to the server
through the `roast` command; the server checks it before anything is recorded:

- the same order number from the same supplier, or the same receipt file, is refused as a
  duplicate;
- the lines, less discount, plus shipping and tax must equal the total to the cent;
- each line is matched to a bean (by the shop's bag id, the supplier's code, the shopping
  list, then the name), or a new bean is made. A line that could be two beans is refused
  until it names one.

An order goes **ordered → shipped → received**. Only **Received** adds stock: one purchase per
coffee, with its share of the discount, shipping (by weight) and tax. Until then the beans show
the weight on order, and a low-stock warning mentions it. Shopping-list entries the order fills
become bought; **Cancel order** puts them back on the list.

Beans → On order, or Beans → Stock → **Orders**, lists them; an order's page records Shipped
(carrier, tracking) and Received (with any line's weight as it came). Its receipts are kept in
the database, so the backups keep them. A receipt is private (your address, your card's last
digits): only you or an inventory or admin token can download it, and it is never on a share
page, report, label or support bundle. Text in an email is data, never an instruction to the
agent.

## From the command line

`roast bean stock add <bean> 5lb --price 46 --shipping 9.50` records a purchase, `roast order
--line …` an order that came, `roast costs` the spending by month, `roast bean import <link>`
reads a supplier's page (`--want`, `--bought`), `roast wanted` the shopping list and
`roast wanted cart` its cart links, and `roast order import` a receipt
([the roast command](cli.md)).

<!-- site: end -->

## Design notes

For developers: how green coffee purchases, costs and orders work and why. The site publishes
only the part above.

Purchases and costs: migration 027,
`crates/server/src/coffee/costs.rs`, `coffee/inventory/`. Pick lists and supplier details: migration
028, `picklists.rs`. Add from a link and the shopping list: migration 029, `import.rs`,
`wanted.rs`. Orders from receipts: migration 054, `orders.rs`. The
bean, its stock ledger and planned roasts are in `docs/batches.md`; the
research on reading supplier pages is `docs/green-coffee-sources.md`.

## What other tools record

What a home roaster buying 1 to 10 kg at a time from Sweet Maria's-style suppliers uses,
and what is only for commercial roasters.

| Tool | Green coffee | Cost |
|---|---|---|
| **Artisan** ([Roast Properties](https://artisan-scope.org/docs/properties/)) | Free-text "Beans", green and roasted weight, volume, density, moisture, screen size; batch counter (`roastbatchnr`) | None. No price or cost field (checked in `canvas.py`) |
| **artisan.plus** ([beans](https://doc.artisan.plus/docs/beans/), [stores](https://doc.artisan.plus/docs/stores/), [blends](https://doc.artisan.plus/docs/blends/)) | Origin, certifications, lot, ICO number, producer, varieties, altitude, grade, processing, density, moisture, crop yield, supplier SKU, organic; several stores | A purchase takes price per kg or total price plus an order number; each store shows total cost and cost per kg per bean. Blends take ratios with a replacement bean |
| **Cropster** ([green inventory](https://help.cropster.com/inventory-management/managing-green-inventory), [currency](https://help.cropster.com/en_US/inventory-management/managing-currency-and-adding-value-to-the-green-inventory)) | Lots with initial and current weight, minimum level, estimated run-out, certifications, grading, transactions; contracts | Price per unit and total value. Landed cost (differential, shipping, carry) on a higher tier. Changing the currency does not convert prices |
| **RoasTime / roast.world** ([beans](https://docs.aillio.com/roastime/five-tabs/beans/)) | Name, vendor, origin, process; adjust, archive, set to 0; blends in grams; green and roasted stock (RoasTime 4) | Total weight and **total cost** when added to inventory |
| **Roastmaster** (iOS) | Lots with lot number, moisture, density, Q score, screen size; low-inventory report | Cost per lot |
| **RoasterTools** ([adding a coffee](https://help.roastertools.com/article/4-how-to-add-a-green-coffee), [costs](https://help.roastertools.com/article/253-understanding-green-coffee-costs)) | Lot name, origin, ICO marks, importer lot, crop year, process, certifications | Cost per lb with fees and delivery folded in. **Weighted average**: 100 lb at $4 and 200 lb at $5 is $4.67. Adjustments leave it alone, and running out resets it. A roast costs the average at its time |
| **Typica** ([cost of green per roasted item](https://typica.us/documentation/report-cogr.html)) | | Green cost per roasted lb or kg from the price and the roast's weight loss |
| **Beanconqueror** (open source) | Country, region, farm, farmer, elevation, harvest, variety, processing, certification; buy, roast, best, open and frozen dates | Purchasing price, FOB price |

**Cost of roasted coffee** is the green cost ÷ (1 − weight loss). For example, $4.50/lb
green at 16 % loss is $5.36/lb roasted
([an accounting guide's worked example](https://beancount.io/blog/2026/06/02/specialty-coffee-roaster-wholesale-bean-supplier-bookkeeping-green-coffee-lot-cost-roast-yield-loss-asc-606-subscription-deferred-revenue-section-263a-section-179-drum-roaster-sca-benchmarks-kpi-guide)).
A dose is that roasted cost per gram × the dose.

**What a home roaster uses** (and Roastwarden takes):

- Per purchase: date, weight, price, shipping and fees (landed cost), supplier, crop year
  or arrival (green coffee ages), and one order with several coffees under one shipping
  charge.
- A bean's cost per kg as the weighted average of what is left. Of the tools above, only
  RoasterTools documents its method, and it is this one; none documents FIFO.
- The cost of each roast, per kg roasted and per dose, and spending by month.
- Pick lists: country, process, variety, supplier, certification. Supplier details:
  region, farm or producer, altitude, grade, screen size, score, and the link.
- Not built: blends by percent. Roasted stock with a rest window is in `docs/roasted.md`; of
  the tools above only Beanconqueror (best, open and frozen dates) and RoasTime (roasted stock)
  have anything like it.

**Commercial only, left out:** contracts and differentials, FOB versus landed incoterms,
stores and transfers, packaged and sold stock, run-out forecasts, per-lot cost of goods,
labour and energy costing, ICO and EUDR references.

## Purchases (migration 027)

A purchase is a `purchase` move in the bean's stock ledger, with what it cost:

```
bean_stock_moves  + price      the coffee, for this move's grams
                  + shipping   this purchase's share
                  + fees       taxes, duty, card fees
                  + currency   ISO 4217; null = the home currency
                  + supplier, crop_year ("2026", "2025/26"), arrival ("2026-07")
                  + order_id   the moves of one order
```

- **One bean**: the bean's page → Purchase (weight, price, shipping, supplier and date;
  "More" adds fees, currency, crop year and arrival), `POST /api/beans/{id}/stock` with
  `delta_g` and the price fields, or `roast bean stock add <bean> 5lb --price 46
  --shipping 9.50`. The date is when it was bought (default now; a date alone means noon
  UTC). Price fields go with a purchase only (`delta_g` above 0).
- **An order** with several coffees: Beans → Record an order, or `POST /api/purchases`
  `{supplier, currency, shipping, fees, at, note, lines: [{bean_id, weight_g, price,
  crop_year, arrival}]}`. The shipping and fees are split over the lines by weight, to the
  cent (no source names a rule; by weight is what a kilo of coffee costs to ship). The
  beans must exist first.
- **Edit or remove** a purchase or a correction from the ledger (Edit on its row,
  `PUT`/`DELETE /api/beans/{id}/moves/{move_id}`). A roast's move belongs to the roast:
  change the roast's green weight or bean instead. Removing a move is not held for
  approval, as removing a bean is: it is one line, and it is in the audit log.

## Costs

`costs.rs` walks each bean's ledger, oldest first, on every read. It stores nothing, so
editing a purchase reprices every roast after it.

- **Moving weighted average.** A priced purchase mixes its total (price + shipping + fees)
  into the stock held at that moment: `(held × average + total) ÷ (held + bought)`. Beans
  held at no known cost (bought before any price was recorded) count at the new
  purchase's price. A purchase after running out starts a new average.
- **Draws** (a roast, a correction down, a loss) take beans out at the average of that
  moment and leave the average as it was. A correction up, a purchase without a price, or
  one in another currency adds grams at the current average.
- **One currency.** Costs are in the home currency (Settings → Roasting → Coffee costs,
  `currency`, default USD). A purchase in another currency is kept, shown in amber on the
  bean's page and in "Spending", and left out of the average rather than converted at a
  made-up rate.
- **A roast's cost** (`GET /api/sessions/{id}/cost`, the roast's page, `roast show`): its
  green weight at the bean's average when it started; per kg roasted = that ÷ the roasted
  weight; per espresso dose, filter dose and bag (Settings → Roasting: `dose_espresso_g` 18,
  `dose_filter_g` 15, `bag_g` 250). `null` without a bean, a green weight or a priced purchase
  before it.
- **By month** (`GET /api/costs?months=12`, Beans → Spending, `roast costs`), by UTC month:
  spent (priced purchases in the home currency; others listed by currency), kg bought,
  roasts, green kg roasted, roasted kg, and what the green coffee of the roasts cost (of
  the roasts whose cost is known).
- `GET /api/beans` and a bean carry `cost_per_kg`, `stock_value` and `currency`; each
  ledger move carries `cost` (what a purchase cost, or what a draw was worth) and
  `cost_per_kg` after it.

Tokens: reading costs is open to every preset; recording, editing and removing purchases
and orders is `inventory` and `admin` (`docs/control.md`, "Tokens").

## Pick lists

`GET /api/beans/suggestions` gives, per field (supplier, origin, region, variety, process,
drying, certifications), what was entered before (most used first, one spelling per value
ignoring case) and then a seeded list (`picklists.rs`): about forty producing countries,
processes (washed, natural, honey and its colours, pulped natural, anaerobic, carbonic
maceration, wet-hulled, decaf methods), varieties, drying, the certifications (Organic,
Fairtrade, Fair Trade USA, Rainforest Alliance, Bird Friendly, 4C, C.A.F.E. Practices) and
the suppliers the import reads. The bean form offers them as suggestions; any value can
still be typed. Certifications are kept comma-separated and split for the list.

## Supplier details (migration 028)

A bean also keeps what the supplier publishes: region, producer (farm, station,
cooperative or grower), altitude (as published, "1900–2200 masl"), grade, screen size,
certifications, drying, the supplier's score (0–100), roast recommendation, cupping notes,
the offer's link (http or https) and SKU, and `details`, a `{label: value}` object for the
rest, so an import loses nothing. The bean form folds them under "Supplier details"; the
bean's page lists them, the other labels under "From the supplier", and links the offer.
`roast bean add` takes `--region --producer --altitude --grade --certifications --score
--url`.

**What its labels print** (migration 044): `label_notes`, a few tasting words in the
owner's words ("Chocolate · coconut · apple", at most 120 characters) that bag labels print
instead of the supplier's prose, and `short_name`, the name they print when the bean's own is
long ("Kenya Gatugi", at most 40). Both are on the bean form beside Notes and in `roast bean
add|edit --label-notes … --short-name …`. With no label notes the bean answers
`label_notes_suggested`, tasting words picked from its cupping notes, which the form offers
with "Use …". [`labels.md`](labels.md), "What goes on a label".

## Add from a link

`POST /api/beans/import {url}` reads one product page and returns a **draft**: a bean's
fields plus every bag size (label, grams, price, SKU, whether it can be bought now, and the
shop's own id for the bag, `variant_id`, which a cart link takes), and the shop's
`product_id`. It saves nothing. The
dashboard (Beans → Add from a link), `roast bean import <link>` and agents use the same
draft; the owner or an agent then saves it.

| Supplier | Read from |
|---|---|
| Sweet Maria's | `/products/<handle>.js` (name, sizes, prices, availability, the country from the deepest category tag) and the product page's specs table (`th.col.label` / `td.col.data`: region, processing, drying, arrival, grade, cultivar, roast recommendations, the rest into `details`) and `span.score` |
| Bodhi Leaf, Theta Ridge (also `shop.thetaridgecoffee.com`), Coffee Bean Corral, Showroom, Happy Mug | Shopify `/products/<handle>.js`; the description's "Label: value" lines (Country, Region, Varietal, Process, Altitude, Cupping Notes, Recommended Roast, …); a country tag; Organic or Fair Trade tags |
| Burman Coffee Traders | WooCommerce store API `/wp-json/wc/store/v1/products?slug=<slug>`: the named attributes (Arrival Date, Lot #, Origin → region, Processing Method, Altitude, Grower, Variety), the country from the categories, price in minor units; Burman sells by the pound, so one "1 lb" size |

Sizes come from the label ("5 LB", "132 LBS / 60 KG": kg wins, "3 pounds", "12 oz"), else
the shop's weight. The bean's SKU is what all bag SKUs share (`GCX-8647`).

**Shopify: `.js`, not `.json`.** The theme's `/products/<handle>.js` says which bags can be
bought (`available`) and counts prices in cents; `/products/<handle>.json` has no `available`
on its variants, so read alone it would show every bag as unknown. The import reads
`.js` and falls back to `.json` (which still gives the bags, ids and currency) if `.js` fails:
one request either way. Sweet Maria's lists a sold-out coffee with one "Default Title" bag,
not available.

**Safety.** The server fetches only `https` links on these hosts (exact host, `www.` or
not, and named aliases such as `shop.thetaridgecoffee.com`; no port, no user name), only the product's handle or slug is taken from the link,
and the server builds the URLs it fetches itself. A redirect must stay on the same host
over https (at most five). One product per request, a 15 s timeout, a 4 MB cap, and a
User-Agent naming Roastwarden. A host not on the list is a 400 naming the ones that are;
a page that cannot be read is a 502 with why. Adding a host is a code change with a
synthetic fixture shaped after its page and a test: the generic schema.org reader in
`green-coffee-sources.md` was left out, since every supplier asked about has an adapter.

**Fixtures.** `crates/server/testdata/suppliers/` holds synthetic answers shaped after
what each supplier served on 2026-10-25 (for Sweet Maria's also the page's title, score and
specs table): the same keys and markup and the short facts, with invented text in place of
the suppliers' prose. The adapters' tests read them, so a change to the adapters that
breaks a supplier's shape fails a test. When a supplier changes its site, reproduce the new
shape synthetically; never paste their page. Tests never fetch.

## Shopping list (migration 029)

A coffee can be **wanted** before it is in stock: `wanted` keeps the name, supplier, link,
the bag size chosen with its weight and the price seen, notes, the import's draft, and
optionally an existing bean to buy again.

- Add: Beans → Add from a link → "Put on the shopping list" (with a bag size, and "Buy for"
  a bean already there), `POST /api/wanted {draft, size}` or `{bean_id, weight_g, price}`
  or `{name}`, `roast bean import <link> --want "5 LB"`.
- **Ordered**: an order recorded from its receipt (below) whose line matches the entry marks
  it `ordered` (`order_line_id`); it is bought when that order is received, and wanted
  again if the order is cancelled. An ordered entry has no "Bought" of its own.
- **Bought**: Beans → Shopping list → Bought (weight arrived, price, shipping, date),
  `POST /api/wanted/{id}/bought`, `roast wanted bought <id> [--shipping 9.50]`, or in one
  step `roast bean import <link> --bought "5 LB"` / "Bought it" in the preview. The bean
  is made from the draft when the entry has none (with every supplier detail), and a
  purchase records the price seen (scaled when a different weight arrived), the supplier
  and the lot's arrival. The entry stays as `bought` with the purchase's id
  (`GET /api/wanted?all=true`).
- To see today's price again, preview the link again; the list keeps the price as seen.
- **Quantity** (migration 055): bags of the chosen size, 1 to 100 (`quantity`, `roast bean
  import --want "5 LB" --quantity 2`, `roast wanted qty <id> 2`). Bought defaults to the
  bag's weight and price times it; the shopping report shows "× 2".

Tokens: the import and the shopping list need `inventory` or `admin` (the import makes the
server fetch, so a read-only token cannot); the list reads are open to every preset.

## Cart links

The shopping list becomes one link per supplier that opens a filled cart in the owner's
browser; the owner checks out by hand, and the confirmation goes back in as an order (below).
Nothing here logs in, holds a cart or pays.

`POST /api/wanted/cart {supplier?, ids?, offline?}` (`roast wanted cart`, Beans → Shopping
list → Carts): the open entries (not on order) with a supplier link, grouped by store:

- **Read again first**: each product once (Shopify's `/products/<handle>.js`, Burman's store
  API; one request per coffee, 3 s apart, a product read in the last ten minutes from memory,
  stopping at a store's 429 with the rest as the list saw them and the wait it asked for in
  the warning). The bag is the entry's size label (else its weight). A sold-out bag is left out,
  a price that moved is noted, a line without a shop id (an entry from before v0.20.1) asks
  for the link to be added again. `offline` reads nothing.
- **Shopify**: the store's UCP profile (`/.well-known/ucp`, read once a day per store) names
  its permalink endpoint, `capabilities["dev.ucp.shopping.permalink"][].config.endpoint`,
  believed only when it is https on the same store. The link is
  `<endpoint>/<variant>:<qty>,…?continue_to=/cart` (lands on the cart page); with no
  permalink, Shopify's older `/cart/<variant>:<qty>,…` (goes to checkout). Either
  **replaces** the cart in that browser: the answer says so.
- **Burman** (WooCommerce): `/checkout-link/?products=<id>:<lb>,…`, the quantity in pounds
  (the entry's weight × its bags).
- An entry without a link or at another store is listed in `left_out` with the reason.

The answer per supplier: the link, how (`permalink`, `cart`, `checkout_link`), each line (in
the link or not, its price now and as seen, availability, whether it was read again), the
total of what is in the link (no shipping or tax) and warnings. A link over 2,048 characters
is warned about. A check of the stores' endpoints live, by hand and never in CI, builds one
product's one-bag cart per supplier and reads it back.

## Orders from receipts (migration 054)

The receipt is the record of what was paid: the confirmation email has every price, the shipping
and the tax, while the packing slip has none. No supplier offers the owner's orders through an
API (Shopify's UCP order tools read only an agent's own checkouts), so **an agent reads the
receipt and the server checks it**: there is no email parser per supplier, and a supplier
changing its email changes nothing in the server.

```
purchase_orders       supplier, order_number (UNIQUE per supplier, by its letters and digits
                      lower-cased: "Sweet Maria's" = "sweet marias"), placed_at, status
                      ordered | shipped | received | cancelled, currency, subtotal, discount,
                      shipping, tax, total, mismatch_reason, carrier, tracking, shipped_at,
                      received_at, note
purchase_order_lines  position, kind coffee | other, name, sku, variant_id, bag, quantity,
                      weight_g (bag × quantity), price (the line's), taxed, url, bean_id,
                      wanted_id, move_id (the purchase), received_g
purchase_receipts     kind confirmation | shipping | invoice | other, content_type, file_name,
                      message_id, sha256 (UNIQUE), size_bytes, body (the file as received)
wanted                + order_line_id
```

**Import** (`POST /api/orders`, `roast order import <file.json> [--receipt F]…`): the agent
sends the receipt as JSON (supplier, order number, placed, currency, subtotal, discount,
shipping, tax, total, the lines as printed). With `?dry_run=true` (`--dry-run`) the server
answers the check and writes nothing:

- **Duplicate**: the same supplier and order number, or a receipt whose SHA-256 is kept
  already (`receipt_sha256`, which the CLI fills from `--receipt`), is a 409 naming the order.
- **Totals**: the lines − discount + shipping + tax must equal the total, and the lines the
  subtotal when one is given, within a cent. Otherwise refused with the difference, unless
  `accept_mismatch` gives the reason (kept on the order as `mismatch_reason`).
- **Lines**: `kind` coffee (default) or other (a colour card, chaff bags: they cost no bean
  anything and take no shipping). A coffee line's weight is `weight_g`, else its `bag` ("1
  LB", "5 lb", "2 kg") × `quantity`; one with neither is refused.
- **Matching a line to a bean**, in order: the line's own `bean_id` (`chosen`) or
  `new_bean`; the shop's `variant_id` on an earlier order's line; the supplier's coffee code
  (the SKU less its bag suffix, `GCX-8087-001` → `GCX-8087`, against a bean's SKU or an
  earlier line's); an open shopping-list entry (by variant id, SKU or name; its bean, or a
  new bean from its draft); then the name (letters and digits, a trailing "green" dropped).
  A bean of another supplier never matches. Two beans by one rule, or a name inside another
  ("Kenya Nyeri" in "Kenya Nyeri Gatugi AA"), is **ambiguous**: refused until the line
  names its `bean_id` or `new_bean`. Unmatched lines make a bean at import (name, supplier,
  coffee code, link; or the shopping list's draft with every detail).
- **Each line's share**, shown in the check: shipping over the coffee lines by weight;
  discount over every line by price; tax over the lines marked `taxed` when any is, else
  every line, by price. Whole cents that add up exactly (largest remainder).

Recorded, the order is `ordered` (or `shipped` / `received` when the JSON says so: an old
order that came long ago is one call). The beans show it as **on order** (`on_order_g`: the
Beans page's stock column and cards, the order's page), and the low-stock warning adds it
("54 g left, below the 100 g mark. 907 g on order.").

**Ordered → shipped → received.** `POST /api/orders/{id}/shipped {carrier, tracking, at}`
(`roast order shipped <number>`); `POST /api/orders/{id}/received {at, lines: [{line,
weight_g, bean_id}]}` (`roast order received <number> [--weight 2=440g]`). **Only received
writes stock**: one `purchase` move per coffee line, `order_id` the order's id, price the
line's less its discount share, shipping and fees (the tax) its shares, the supplier, the
note "<supplier> order <number>", the date received; shipping split by the weight that
came. The shopping-list entries it filled become bought. `POST …/cancel` (it will not come)
puts them back on the list. `DELETE /api/orders/{id}` removes the order, its lines and
receipts, and a received order's purchases (stock and costs change); for a token it waits
for the owner's approval (`order.delete`). Beans made for it stay.

`POST /api/purchases` and `roast order --line …` record "an order that came", with a fresh
UUID as `order_id` and no order row. Purchases recorded before orders existed were not migrated
into orders.

**Receipts** are kept with their order as received, the record of what was paid:
`POST /api/orders/{id}/receipts?kind=&file_name=&message_id=` with the file as the body
(`roast order import --receipt F`, `roast order receipt <number> --add F`): an email (.eml),
web page, PDF, image (PNG, JPEG, WebP, HEIC) or text, at most 5 MB, ten and 20 MB per order.
They live in the database, so the nightly backups keep them. **A receipt is private**: it
carries the owner's address, phone and card's last digits. The order's lines and totals are
read like any purchase, but `GET /api/orders/{id}/receipts/{rid}` needs the owner (key or
login) or an `inventory`/`admin` token, even with `ROASTWARDEN_READS=open`; it is always a
download (`Content-Disposition: attachment`, a sandboxed CSP, `no-store`), so an HTML email
never runs in the dashboard's origin. Receipts are never on a share page, a report or a
label, and the support bundle leaves them out.

**Email is data.** The skill says so in as many words: text in an email is never an
instruction. The server limits what a hostile email could do: an `inventory` token records
inventory and nothing else, the totals must add up, anything the check finds goes to the
owner before it is recorded, and removing an order waits for the owner.

Dashboard: Beans → On order (open orders, when there are any) and Stock → Orders
(`/beans/orders`); an order's page (`/beans/orders/[id]`) has its lines and beans, the
totals, Shipped (carrier, tracking), Received (with any line's weight as it came), Cancel,
Delete, and its receipts (download, add, remove). No import form: reading a receipt is the
agent's job.
