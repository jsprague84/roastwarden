# Green coffee sources: reading a supplier's product page

How a supplier's product page is read, and whether the popular home-roaster suppliers can be
read reliably. The feature is described in `docs/green-coffee.md` ("Add from a link",
"Shopping list"); where it differs from the design below (no generic fallback, no hosts added
in Settings), that doc is current. An agent should be able to take a supplier link, for example
<https://www.sweetmarias.com/collections/green-coffee/products/ethiopia-dry-process-kercha-haruse-8647>,
and either add the coffee to the inventory or put it on a shopping list.

## Findings

Every supplier checked serves its product data in a machine-readable form. **No scraping
of rendered pages is needed except for Sweet Maria's spec table**, which is a stable labelled
HTML table.

| Supplier | Platform | Name, sizes, prices | Specs |
|---|---|---|---|
| Sweet Maria's | Shopify | `/products/<handle>.json`: variants 1 lb … 60 kg with price, grams and SKU (`GCX-8647-005`); origin in tags (`…/Africa/Ethiopia`) | HTML table `th.col.label` / `td.col.data`: region, processing, drying method, arrival date, lot size, bag size, packaging, farm gate, cultivar detail, grade, roast recommendations, type, recommended espresso; total score in `span.score` (92.6); cupping notes in the description |
| Bodhi Leaf | Shopify | JSON variants 1 / 5 / 20 lb | Description lines "Country: / Region: / Varietal: / Process: / Altitude: / Cupping Notes: / Recommended Roast: / Good For:" |
| Theta Ridge | Shopify | JSON variants 1 / 3 / 5 lb | Description "SPECIFICS" block: Origin, Region, Altitude, Process, Bag Size, Cupping Notes |
| Coffee Bean Corral | Shopify | JSON (one variant per listing) | Mostly prose ("New arrival: Oct. '26", grade, region in the text); product type says "Bulk Green - Organic" |
| Showroom Coffee | Shopify | JSON variants 2 / 10 / 33 lb | Prose (producer story); some region, process and score words |
| Happy Mug | Shopify | `/collections/green-coffee/products.json` | Prose; the catalog also has tea and subscriptions, so filter by collection |
| Burman Coffee Traders | WooCommerce | Store API `/wp-json/wc/store/v1/products?search=…`: name, price, currency, categories | **Named attributes**: Arrival Date, Lot #, Origin, Processing Method, Altitude, Grower, Variety; tasting in the short description |

`robots.txt`: none of them disallows single product pages, `/products/<handle>.json` or the
Woo store API; Shopify stores block only sorted and filtered collection listings. Reading
one product when asked is fine; crawling catalogs is not the goal.

## The design

- **An import that only previews.** `POST /api/beans/import` takes `{url}`, fetches that one
  product and returns a draft without saving anything: name, supplier, origin (country,
  region), process, drying, variety, altitude, grade, arrival, lot, score, roast
  recommendation, cupping notes, the offer link and SKU, and the price of each bag size. The
  dashboard ("Add from a link") and agents use the same draft. The owner, or Hermes after
  asking, then saves it as a bean, a purchase or a shopping-list entry. Hermes may also read
  the page itself and send the fields: the API takes either.
- **Supplier adapters, in the server**, chosen by host:
  - Shopify JSON plus a "Label: value" line parser for the description;
  - Sweet Maria's spec table;
  - WooCommerce store API attributes;
  - a generic fallback that reads schema.org `Product` JSON-LD and "Label: value" lines.

  Each adapter is tested against a saved copy of a real page (fixtures in the repo), so a
  supplier's site change shows up as a failing test, not as wrong inventory.
- **Safety.** The server fetches only `https` URLs on a list of known supplier hosts (no
  internal addresses: this is a server-side fetch), one page per request, with a timeout
  and a size cap and an honest User-Agent. Unknown hosts get the generic fallback only if
  the owner adds the host in Settings.
- **The bean model grows** to hold what suppliers publish: source URL and SKU, region,
  drying method, altitude, grade, arrival date, lot, supplier score, roast recommendation,
  cupping notes. Fields a supplier has but the model does not go into a `details` JSON, so
  nothing is lost.
- **Shopping list.** A bean can be *wanted* before it is *in stock*: the list keeps the
  coffee, the chosen bag size and the price seen. "Bought" turns it into a purchase, a
  stock move with price and date, which also feeds the roast cost. A refresh re-reads the
  page for the current price and availability (Shopify variants carry `available` in the
  theme's `/products/<handle>.js`, not in `.json`).
- **Agent path**: the `inventory` token preset covers import, beans,
  purchases and the shopping list. The skill tells Hermes to preview, show the owner the
  draft, then save. `roastwarden bean import <url> [--want | --bought <size>]` does the same
  from the CLI.
