# Batches and beans

Roastwarden keeps your green coffee as beans with a stock, a queue of planned roasts, and a
batch number for every roast. Plan a morning's roasts once, start each with one tap on the
Roast page, and each roast takes its green weight off the bean's stock, so you can see when a
bag is running low.

## Beans and their stock

Beans → **Add bean** (or **Add from a link** for a supplier's page, [green coffee](green-coffee.md)):
a name, and as much as you know of origin, variety, process, lot, supplier, moisture and
density. **In stock** is what is in the bag now; **Low below** marks the bean as running low
(500 g unless you set it).

- **Stock is a ledger.** A purchase adds to it, a roast takes its green weight off, and a
  correction writes the difference. A bean's page (Beans → the bean) lists every move.
- **A roast takes its beans when it starts.** Cancelling the roast puts them back; changing
  its green weight or bean afterwards moves the stock with it.
- **Purchase** on the bean's page records beans bought (with the price, if you track costs:
  [green coffee](green-coffee.md)). **Set stock** after weighing what is left: type the
  **Counted** weight, and the difference goes in as a correction.
- A bean with roasts can only be archived, which hides it from the pickers but keeps it for
  the roasts that used it. A bean without roasts can be deleted.

## Planned roasts

Beans → **Plan a roast** queues a bean, a green weight, a profile and a roaster (or any
roaster). The Planned roasts list keeps them in order: move them up or down, edit or remove
them. Each row says how much of the bean is left after it, or in red that the bean will not
cover it.

On the Roast page, before a roast, the set-up step offers the first planned roast for that
roaster: "Planned 1 of 3", the bean, weight and profile, with **Skip** (offer the next) and
**Unplanned** (roast something else). It loads the plan's profile, and the button reads
**Start planned roast**. The roast takes the plan's bean, weight, profile and name, and the
bean's origin, variety, moisture and density.

Without a plan, open **Batch details (optional)** on the set-up step to pick a bean from stock:
it shows the stock left after this roast and warns when it runs low.

With reports on, **Roast-day sheet as PDF** prints the queue ([reports](reports.md)).

## Batch numbers

Each roaster numbers its roasts: a new roast gets the next number on its roaster, shown as
`#12` on the Roast page during the roast, in History's **#** column and on the roast's
report. You can change it on the roast's page in History. A cancelled roast frees its number;
roasts imported from Artisan get none. The Artisan export writes the number as Artisan's batch
number.

## Limits

No blends, and one stock per bean (no separate stores or locations). Roasted coffee has its
own stock: [roasted coffee](roasted.md).

## From the command line

`roast beans`, `roast bean add | show | edit`, `roast bean stock add | set`, and `roast plan`,
`plan add | edit | move | rm` ([the roast command](cli.md)). A roast can be named by its batch
number: `roast show '#12'`.

<!-- site: end -->

## Design notes

For developers: how batches and beans work and why. The site publishes only the part above.

Server: migration `018_batches_beans.sql`,
`crates/server/src/coffee/inventory/`, routes `/api/beans…` and `/api/plan…`. Dashboard: the
**Beans** section (`/beans`, `/beans/[id]`), the Roast page's set-up step, and History.

## Why

Artisan keeps a batch counter and leaves stock to artisan.plus. Roastwarden had neither: a
roast's bean was free text, nothing said how much of a lot was left, and the next roast was
set up from scratch every time. This is the lite version: enough to plan a morning's
roasts, number them, and know when a bag runs low. No blends or multi-store;
roasted stock and its rest are in `docs/roasted.md`; prices, orders and the cost of a roast are in
`docs/green-coffee.md`.

## Data model (migration 018)

```
beans                 one green coffee lot
  id, name            "Brazil Cerrado natural" (required, what lists show)
  origin, variety, process, lot, supplier, notes
  moisture_pct, density_g_l     copied to a roast that uses the bean, if given
  low_stock_g         a bean below this is "low" (default 500 g)
  archived            hidden from pickers; kept for the roasts that used it

bean_stock_moves      the stock ledger; stock = SUM(delta_g)
  id, bean_id, at, delta_g, kind, session_id, note
  kind: purchase (+g) | adjust (±g, "set stock to") | roast (−green weight)

planned_roasts        the queue
  id, position        order (lower first)
  device_id           roaster, or NULL for any roaster
  bean_id, profile_id, green_weight_g, name, notes
  session_id          the roast that ran it; NULL = still planned

roast_sessions        + bean_id, batch_number, planned_roast_id
```

- **Stock is a ledger**, not a number to overwrite: a purchase adds, "set stock to 820 g"
  adds the difference, and every roast with a bean and a green weight has exactly one
  `roast` move of minus its green weight. Triggers keep that move in step with the
  roast (on insert, and when `bean_id` or `green_weight` change), so editing a weight
  after the roast moves the stock with it.
- **A roast draws down when it is created** (Start roast): the beans are in the roaster.
  Cancel deletes the roast and a trigger deletes its move, so the beans come back. The
  same trigger puts a planned roast whose session was deleted back in the queue.
- **Batch numbers** are per roaster: a new roast gets `MAX(batch_number) + 1` on its
  roaster, in the same `INSERT`. Existing roasts were numbered by creation time. A
  cancelled roast frees its number. Imported Artisan logs get none (they were roasted
  elsewhere). The number is editable on the roast's page; the Artisan export writes it as
  `roastbatchnr`.
- **Starting a planned roast** passes `planned_roast_id` with the new session; the server
  fills what the request leaves out from the plan (profile, bean, green weight, name) and
  links the plan to the roast in the same transaction. A roast from a bean also takes the
  bean's origin and variety (so History, search and the Artisan export keep working) and its
  moisture and density.
- A bean with roasts is archived, not deleted; a bean without any can be deleted.

## Where it shows

| Place | What |
|---|---|
| **Beans** (`/beans`), new section between Profiles and Roasters | Planned roasts (add, reorder, edit, delete; each row warns when its bean will not cover it) above the stock table (name, origin, lot, supplier, stock, roasts, low-stock note). Add a bean; archived beans folded away. |
| **A bean** (`/beans/[id]`) | Details form, stock with "Add purchase" and "Set stock to", the ledger, and the roasts that used it. |
| **Roast page**, set-up step | When something is planned for this roaster (or any roaster): one line, "Planned 1 of 3: Brazil · 100 g · Light espresso", with Skip and "Unplanned". It loads the plan's profile into the preview, and the primary button reads **Start planned roast**. Batch details gain a bean picker (beans in stock), which replaces origin and variety, with the stock left after this roast and a low-stock note. The roast is named after the plan, else the bean. |
| Roast page, during the roast | `#12` before the roast's name on the step card. |
| A plan's bean or profile | must exist (400 otherwise); deleting a bean or profile clears it from plans. |
| **History** | A `#` column (sortable) and the bean; the roast's page edits the bean and the batch number and links to the bean; the report prints the batch number. |

The set-up step is the idle state `ui-check.mjs` measures, so the planned line is one row
and the bean picker lives in the folded batch details: the Roast page keeps fitting the
window.

## API

| Method | Path | Does |
|---|---|---|
| GET | `/api/beans?archived=true` | beans with `stock_g`, `roast_count`, `low` |
| POST | `/api/beans` | create (`stock_g` optional: the first purchase) |
| GET / PUT / DELETE | `/api/beans/{id}` | one bean with its moves; edit; delete (archives if used) |
| POST | `/api/beans/{id}/stock` | `{delta_g, note}` (purchase) or `{set_g, note}` (adjust) |
| GET | `/api/plan?device_id=&done=true` | planned roasts in order (`done` adds the ones that ran) |
| POST | `/api/plan` | add to the end of the queue |
| PUT / DELETE | `/api/plan/{id}` | edit (also `position`); remove |
| POST | `/api/plan/reorder` | `{ids: [...]}` the queue's new order |

`POST /api/sessions` takes `bean_id` and `planned_roast_id`; `PUT /api/sessions/{id}` takes
`bean_id` (null clears) and `batch_number`.
