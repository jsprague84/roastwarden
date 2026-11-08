# Reports

Roastwarden makes PDF reports from its own data: a roast on a page to keep with the bag, two
roasts side by side after an experiment, a cupping form to score on, a stock-take sheet, a
roast-day plan to fill in at the roaster. They are pages laid out to read and print, not data
dumps, and the server makes them itself, so they read the same from any browser or a script.

| Report | What it is |
|---|---|
| **Roast report** | one roast: the coffee, eight figures (weight and loss, roast time, first crack, development, drop, colour, how closely it followed the profile, cost) each against the range it was judged by, the curve with RoR, heater and fan, the phases, events, notes and cupping |
| **Compare two roasts** | an A/B experiment: both curves and RoR on one chart, the phases on one time scale, about 25 numbers side by side with B − A |
| **Cupping sheet** | a blank form to score on, after the classic SCA protocol, with three-digit blind codes and the key on a page of its own; or a session's scores as a table |
| **Green inventory** | every coffee in stock with its age, cost and value, the recent pace and when it runs out; or a stock-take sheet to write the counts on |
| **Shopping list** | one short order per supplier, with the usual shipping, and the coffees running low |
| **Coffee report** | one green coffee: what is known of it, its roasts as a table and overlaid on one chart, its cuppings, its stock ledger |
| **Profile report** | a profile's roasts ranked, their curves overlaid on the profile's, and how the numbers spread. A profile never roasted prints its target curve to roast against by hand |
| **Period report** | a month, a year (the year in review) or a range: roasts, green roasted and bought, spend, stock at the end |
| **Purchases** | the green bought over a period, order by order with shipping and fees, and the landed cost per kg |
| **Roast-day plan** | the planned roasts in order: the green to weigh out, each profile's targets, last time's numbers, and boxes to write charge, first crack, drop and the weight in |
| **Traceability** | from a green lot to every bag, or a roast back to its lot: purchases, roasts, packed bags, label runs, gifts and what is left. It carries no prices |

## Using it

- **Dashboard**: **Reports** in the top bar. Pick the kind and what it is about (a roast,
  the roasts on a cupping sheet); a roast's, coffee's and profile's report start on the
  newest roast's, a cupping sheet with no roast ticked (a blank form, or the latest cupping
  session's scores). The preview is the server's own pages, the same as the PDF; while
  there is none (the report failed: the reason shows above it) **Download PDF** and **Open to
  print** wait. **Download PDF**, or **Open to
  print** (the PDF in a new tab). History → a roast → **Report** opens its report there.
  Reports is the `reports` module: if it is not in the top bar, switch it on under Settings →
  Modules.
- **From the command line**: `roast report roast latest` writes `roast-report-batch-N.pdf` here,
  `roast report compare` the newest two roasts side by side (`compare-12-13.pdf`),
  `roast report cupping latest --blank` a cupping form;
  `--format json` prints the data the page is made from, `--format png --page 1` is one
  page to look at ([`cli.md`](cli.md)). Any token may make a report: it changes nothing.
- **Settings → Roasting → Reports**: the paper (US Letter, the default, or A4) and the
  temperature unit (°C, the default, or °F) a report uses unless asked otherwise
  (`report_paper`, `report_units`). A report keeps its own units rather than the browser's,
  so the PDF reads the same wherever it is opened.
- **Writing results back**: what you write on a roast-day plan goes back in with `roast
  weigh` and `roast landmark`, or on the roast's page; stock-take counts on Beans → the coffee
  → **Counted (g)** → **Set stock**.

Every number is worked out by the server and a missing value prints "–", so nothing shifts on
the page. Charts use colours and dashes that still read in greyscale, and the PDF is tagged,
so headings and tables keep their structure for screen readers.

<!-- site: end -->

## Design notes

For developers: each report's contents in detail, how they are made and checked. The site
publishes only the part above. The old printable page, `/history/<id>/report`, redirects to
the roast's report.

PDF documents the server makes from its own data with Typst, the engine bag labels use
([`labels.md`](labels.md)): well-designed pages to keep, print or send, not data dumps.

| Kind | What it is |
|---|---|
| **Roast report** | one roast on a page: the coffee, the numbers against what was expected, the curve, phases and events, notes and cupping |
| **Compare two roasts** | an A/B experiment on a page: both curves and RoR on one chart, heater and fan, phases on one time scale, every number side by side with B − A |
| **Cupping sheet** | a blank form to score on (blind codes, the key apart) and a session's scores as a table |
| **Green inventory** | stock, value, age, run-out dates; a stock-take sheet to weigh against |
| **Shopping list** | a short purchase order per supplier, with what will run out |
| **Coffee report** | one coffee's roasts, cuppings and ledger |
| **Profile report** | a profile's roasts ranked, overlaid, and spread |
| **Period report** | a month or a range (a year is the year in review) |
| **Purchases** | orders over a range, per supplier and bean, with shipping and fees |
| **Roast-day plan** | the planned roasts in order with boxes to write in at the roaster |
| **Traceability** | the green coffee, each roast and where its coffee went: packed bags, label runs, gifts, use, what is left |


## The roast report

Page 1, top to bottom:

- **The coffee**: its name (the bean's, else the roast's), origin, region, producer,
  variety and process; the date and time, the roaster, the profile, purpose and level.
- **Eight figures**: weight in → out and the loss; roast time; first crack (time and BT);
  development (time and DTR); drop BT; colour; how closely BT followed the profile (RMS and
  max, TP to drop); what the coffee cost per kg roasted and per dose. Under each, the range
  it was judged against, as roast guidance judges it on the dashboard
  ([`roast-guidance.md`](roast-guidance.md)): the profile's own range, else its roast level's
  (loss shifted about one point per point of green moisture from 10.5 %), else this
  roaster's usual for the profile (mean ± max(2σ, a floor), from three other roasts; never
  below zero: a band that would be starts at the smallest value seen). Outside a range it
  reads "above" or "below" before the range, in the accent colour. Times and temperatures
  are judged against the roaster's own history only, as on the dashboard. The coffee's cost
  is per kg roasted (its label says so), then per dose and the green it took.
- **The curve**: BT, ET, the profile's BT and the rate of rise (a ±15 s least-squares
  slope, from 45 s after charge to 10 s before drop) on °C (or °F) and °C/min axes, minutes
  along the bottom. The rate-of-rise axis is drawn and scaled from the same start, its top on
  a robust maximum (the 95th percentile, at least 20 °C/min), so the steep first minute or a
  spike does not squeeze the rest into the bottom third; the line starts where it first comes
  inside the axis. Drying and development tinted; dry end, first crack and drop as dotted
  lines named at the top (names of close events pushed apart); each line named at its end
  instead of a legend, the names pushed apart. Under it heater (a 15 s mean: the PID's output
  is jittery) and fan, 0–100 %, named the same way.
- **The phases** as one bar (each phase's share of the roast), then two tables: phases
  (time, %, ΔBT, RoR; drying's ΔBT and RoR from the turning point when one is marked, as
  Artisan does) and events (time, BT, ET to one decimal, RoR, heater).
- **Notes and cupping**: the roast's notes; its cupping on the classic SCA form
  ([`cupping.md`](cupping.md)): the final score, its band, total − defects, the ten scores
  and the tasting notes.

A section never splits from its heading; a roast with long notes runs onto page 2, whose
header names the roast and the report. A missing value prints "–", so nothing shifts.

## Compare two roasts

An A/B experiment on one page, from the numbers `roast compare` reads
([`analysis.md`](analysis.md)). **A** is the roast B is measured against, **B** the change:

- **The two roasts**: each with its letter in its colour (A blue, B vermilion), batch and
  name, coffee, date, roaster, profile, green weight and BT at charge.
- **Four figures**: first crack, drop, development and weight loss as "A → B", with B − A
  and first crack's and drop's BT, development's DTR.
- **One chart**: both roasts' BT (A solid, B dashed) and rate of rise (thinner, on the
  right axis, as on the roast report: a ±15 s slope from 0:45, the axis on a robust
  maximum), the profile once when both followed it (else each one's), first crack and drop
  of each as dotted lines in its colour; under it heater and fan, A and B on each strip. Then
  how closely B's BT followed A's: "B's BT stayed within 0.5 °C of A's from 1:30 on (RMS
  0.6 °C, the largest +4.0 °C at 0:10)" (`within`, default 0.5 °C).
- **The phases**, one bar per roast on the same time scale, so the longer roast is the longer
  bar.
- **Every number side by side**: about 25 rows (BT at charge, heater in the first 30 s, BT −
  profile at 0:30, the RoR peak, RoR at 1:00 and 2:00, dry end, first crack and its BT, drop
  and its BT, development, DTR, the lowest late RoR, the FC → drop slope, heater and fan over
  the last minute, time at 90 % heater, profile RMS and bias, loss) in two tables, the start
  and the rest, with A, B and B − A ("same" when it rounds to nothing). These are the
  server's own RoR, as `roast compare` reads them; the chart's is the report's smoothed one.
- **Notes**: what each roast's notes say it was for.

Which two: `roasts=A,B`; one roast is B, against the finished roast before it on its
roaster; none, the newest two finished roasts (`roaster` narrows it), the older as A. Two
the same is refused (400); none before it, 404. The data's `comparison` is `roast compare`'s
answer (°C, seconds) for an agent.

```
GET /api/reports/compare?roasts=<A>,<B>&within=0.5&format=…
GET /api/reports/compare?roaster=esp32_roaster_01        # the newest two there
```

On the dashboard: Reports → **Compare two roasts** (A and B start on the newest roast and the
one before it on its roaster), or History → pick two → Compare → **Compare report** (the
reference as A).

## The cupping sheet

After the classic SCA protocol, as Roastwarden records it ([`cupping.md`](cupping.md)). Our own
form: the official CVA forms may be shared only unchanged, so they are linked, not redrawn.

- **A form to score on** (`blank`): for the roasts picked and any samples of the owner's own
  (two when none is named). Each sample gets a **three-digit blind code**, the same all day for
  the same roast, never two alike; the form shows the code, the roast date and how long it has
  rested, never the coffee. **Two a page**: the seven quality attributes as 6–10 scales in
  quarter points to mark, with dry and break marks for fragrance and intensity marks for
  acidity (high, low) and body (heavy, thin); uniformity, clean cup and sweetness as five cups
  each; taint (×2) and fault (×4) cups; every score box in one column, then defects (the minus
  by its box), total and final; two lines for notes. **Six a page** (`per_page=6`): a box per
  score, total, defects and final, for cuppers who write numbers. The **key** (which code is
  which roast) prints on a page of its own, to keep apart until the cups are scored, with the
  protocol in brief (8.25 g per 150 ml, about 93 °C, five cups, roasted 8–24 h before).
- **Scores**: the roasts picked, else the latest day's cuppings (or `date`). A table in the
  form's order (the cup attributes as `10 8 10`), total, defects and final with its band;
  the best named at the top; a **dot strip** per quality attribute on one 6–10 axis, each
  roast its batch number in a ring (rings that would touch stack upward, and the row grows
  to hold them), so the roasts compare attribute by attribute in greyscale; then the tasting
  notes.

```
GET /api/reports/cupping?blank=true&roasts=<id>,<id>&samples=1&per_page=2|6&format=…
GET /api/reports/cupping?roasts=<id>,<id> | ?date=2026-10-26
```

At most 24 samples on a blank sheet and 30 roasts in a table.

## Green inventory

Every coffee in stock, soonest to run out first: supplier and lot; when it was last bought and
how old that is ("ageing" from 6 months and "old" from 12, in the accent colour: green keeps
its best for about six months, up to a year when cool and dry); crop year; kg in stock and
planned (roasts in the plan not yet roasted); cost per kg (the weighted average of what is
left, shipping and fees in, `green-coffee.md`) and value; the **pace**, green roasted in the
last 90 days (or since it came in, a week at least) per week; and when it **runs out** at that
pace after the planned roasts. Above the table: kg in stock, what it is worth (and how many
coffees have no price), which runs out first and when, and which are below their low mark. A
totals row closes the table.

**Stock-take** (`count=true`): the date, who counted, and a row per coffee with the grams
in the app and boxes to write the count and the difference, 10 mm tall for handwriting.
Counts go back in on Beans → the coffee → **Counted (g)** → **Set stock** (or `roast bean
stock set`): a correction moves the stock, not the cost per kg.

## Shopping list

The shopping list as **one short order per supplier**: each bag with its link, when its
price was seen and the price (in another currency it is shown and left out of the total);
subtotal; **shipping** as that supplier's earlier orders carried it (the mean per order of
the purchases recorded with shipping, "not known yet" before any); total. Then **Running
low**: coffees below their low mark or out within 30 days at the recent pace, with what is
left, when it runs out, where it came from and what it cost, and whether it is on the list.

## Coffee report

One green coffee: its name with origin, region and process; four figures (in stock against
its low mark, cost per kg and the stock's worth, or "none in stock" when it is all roasted,
what was bought in how many purchases and for how much, how many roasts and how much green
they took; grams below a kilogram); what is known of it (producer,
variety, altitude, grade, crop, arrival, supplier, lot, their score, moisture, density) and
the supplier's and your notes. **Its roasts** as a table (date, the roast's name over its
profile, green, loss, first crack, drop, development, DTR, cupping) and the newest twelve
**overlaid** on one chart from charge: each roast's BT and rate of rise (right axis) thin and
grey, the newest's in colour and named, with its first crack and drop marked (one chart, not
small multiples: the roasts compare where they differ, and no page is left half empty). Then its cuppings
and **the ledger**: every purchase, roast and correction with the stock after it and the cost
per kg after it. The closing note stays with the last section, never on a page alone.

## Profile report

A profile's targets (level, purpose, first crack, drop, charge, batch, roaster) and
description; four figures (roasts, mean first crack, mean loss, how closely BT followed the
profile); **its roasts ranked** after Artisan's Ranking report (each roast's name over its
coffee), with an **average** row; the
newest twenty **overlaid**, BT from charge, each roast thin and light grey, the newest in the
BT colour, the profile dark grey and dashed, the newest and the profile named at the plot's
right edge; and **how the numbers spread**: a dot strip each for first crack, drop temperature,
DTR and loss, every roast a ring with its batch number, the newest filled; rings that would
touch stack upward (a beeswarm), clear of the axis numbers. `roaster` limits it to one
roaster's roasts. A profile with no finished roast prints **its target curve** (the chart, and
BT minute by minute from charge to its drop time, to roast against by hand) and **what to
expect** (weight loss, first crack, development, roast time, drop and colour, each the
profile's own range or its level's) instead of an empty page.

## Period report

A month (`month=2026-10`), a year (`year=2026`: **the year in review**) or any range
(`from`, `to`; default this month so far). A month or year not over yet ends today and says
"so far" (its last figure is the stock now). Four figures: roasts and on how many roasters;
green roasted with what came out and the mean loss; green bought, what it cost with
shipping and fees, and the landed cost per kg; the stock and its worth at the end of the
period, from each coffee's ledger up to then. Then the coffee roasted most, the profile used
most, the best cupping, the first and last roast; for two months or more, **month by month**
as bars and a table (roasts, green, bought, spent; a run of months with nothing roasted or
bought is one line, "Jan – Aug"); **each coffee's** roasts, green used and
bought, spend, landed cost, stock and worth at the end; and every roast (its name over its
coffee) with a **sum row**
(totals in g below a kilogram, else kg).

## Purchases

The green bought over a period (taken as the period report takes it): **order by order**
(the purchases of one order together, else each alone) with its coffees, kg, goods,
shipping, fees, total and **landed cost per kg**, under a totals row; each supplier's share
when there is more than one (orders with no supplier recorded are counted as orders, not
as a supplier); and the landed cost per kg as **a dot per purchase over time**.
A purchase in another currency is listed in it and left out of the sums.

## Roast-day plan

For the roaster: the planned roasts in queue order (`roaster` for one roaster's). First
**the green to weigh out** per coffee against what is in stock, marked "short by 50 g" when
it is not enough; then each roast as a block: its number, name, coffee and profile, the green
to weigh in large type, the profile's targets (charge, first crack time and the profile's BT
then, drop time and temperature, level), the output to expect from the level's weight-loss
range ("about 84–86 g out"), **last time**: the coffee's last finished roast on that profile
(its batch and day, first crack and drop with their BT, the loss, and what its notes came to:
the "Result:" paragraph, else the last, its end when long) or "First roast of this coffee on
this profile", and **boxes to write** charge, first crack and its BT, drop and its BT, and the
roasted weight, each with its unit faint in its corner (°C, m:ss, g), with a line for notes.
What is written goes back in with
`roast weigh` and `roast landmark` (or the roast's page).

## Traceability

From the green lot to every bag, or a bag's batch number back to the green lot
(`lean-home.md`). The same data three ways:

- `GET /api/trace/roast/{id}` (a roast back to its green coffee, and where its coffee went)
  and `GET /api/trace/bean/{id}` (a coffee to every finished roast of it, oldest first): the
  coffee (name, supplier, lot, origin, region, producer, variety, process, its offer, and its
  purchases with grams, supplier, crop year and arrival), and each roast (batch number, date,
  roaster, profile, green → roasted, loss, what is left, its best-until), its moves (used,
  given with the recipient, packed with bags, size, form, grind, the bags' best-until and the
  label run, discarded, counted) and the label runs with its labels (date, how many, state,
  note). Any token reads it.
- `roast trace <#12 | latest | roast id | coffee name or id>` ([`cli.md`](cli.md)).
- The report, `GET /api/reports/trace?roast=<id>` or `?bean=<id>` (`roast report trace`,
  Reports → **Traceability**: a roast back to its green lot, or a coffee to every bag): four
  figures (roasts and the green they took, bags packed, given and used, left), the green coffee
  with its facts and a purchases table, then a section per roast with its date, roaster and
  profile, green → roasted (loss), what is left, and a table of where it went by date: each
  move and label run.

A trace carries **no prices**: it is for whoever holds the bag (a market, a shop, a buyer);
the coffee's cost stays in the coffee report. A switch to add them (with the `costs` module
on) is not built.

## How it is made

```
GET /api/reports                         the kinds, their data's schema and address
GET /api/reports/roast/{id}?format=pdf|svg|png|json&page=&dpi=&paper=letter|a4&units=C|F&tz=
```

- `pdf` (the default); `svg`, every page in one SVG, one under the other (the dashboard's
  preview); `png`, one page (`page`, from 1; `dpi`, 36–200, default 96); `json`, the data.
  `X-Report-Pages` says how many pages. `tz` is minutes east of UTC for the dates; without
  it the `time_zone` setting (an IANA name, Settings → Roasting; DST right for each date),
  else UTC.
- **Data first** (`crates/server/src/reports/`): each kind gathers what it needs, does every
  sum, mean, range and conversion in Rust and formats every number (`reports/mod.rs`: units,
  money, `9:51`, dates; grams in a table always to one decimal so the column lines up, a weight
  to read in g below a kilogram), then hands the design one JSON value with a versioned
  `schema` (`roastwarden.report.roast/1`), the same one `format=json` answers. Charts are
  prepared too (`reports/chart.rs`): round ticks ("nice numbers"), series thinned with
  Largest-Triangle-Three-Buckets to at most 900 points, the smoothed RoR. The level ranges are
  the dashboard's (`reports/expect.rs`, a test checks them against `references.ts`).
- **Designs** are Typst files (`crates/server/assets/reports/<kind>.typ`) with a `report(data)`
  function. They import `report.typ`, the helpers: `page-frame` (paper, margins 18–20 mm,
  Atkinson Hyperlegible Next at 9.5 pt with tabular figures, a header from page 2, a footer
  "Roastwarden <version> · made <date>" and "Page n of N"), `section`, `figures`, `rtable`
  (booktabs rules, no vertical rules, stripes or fills), `roast-chart` (events of one roast
  of several in its colour, `of`), `strip-chart` (a `title`, each line's `tag`),
  `overlay-chart` (several roasts' BT, and their RoR on a right axis with `y2`),
  `phase-bar`, `phase-bars` (several roasts' phases on one time scale), `facts`,
  `small-multiples`, and `spread` (labels pushed apart) and `swarm` (a beeswarm's levels)
  for charts. A `section` keeps together unless it is `breakable: true`; its title always stays
  with what follows. Charts are drawn with Typst's own `curve`, `line` and `place` in a clipped
  box (about 50 ms for a whole roast; a charting package costs 0.7–1.7 s), in Okabe-Ito colours
  a step darker than the dashboard's, each series with its own dash so the page reads in
  greyscale; they sit in a `figure` with `alt` text, marked `pdf.artifact`.
- **The worker**: a report renders in the label sandbox (`labels/sandbox.rs`): a child
  process with 10 s, 1 GB and no files, network or clock. The time the report was made
  comes in `meta.made_iso` and becomes the PDF's date; the PDF is tagged (headings and
  tables keep their structure for screen readers and agents), its creator "Roastwarden
  <version> (Typst)".
- **Tests**: `reports::roast::tests` builds the report from a real roast (a recorded roast on the
  fluid bed, `testdata/reports/roast-batch5.json`), checks the figures, and lays it
  out on one page in °C on Letter and °F on A4 with a cupping and events close together; the
  other kinds have fixtures for the hard cases too (thirteen close roasts on the profile's
  strips, a coffee at 0 g with twelve roasts overlaid on A4, a year not over with empty
  months, a profile never roasted, two roasts compared in °C and °F) and each runs the
  report check below;
  `RR_REPORTS_OUT=/tmp/out cargo test -p roastwarden-server --lib reports` writes the PDFs and
  a PNG of each page to look at. `tests/roast_cli/reports.rs` runs `roast report` against the server.

## The report check

Every kind's tests lay its fixtures out and read the pages back as text
(`reports/check.rs`, through the engine's `page_text`: each run of text Typst set, with its
size, baseline and ink box), and fail on what a reader would call a layout fault:

- **an orphan last page**: two pages or more, the last holding less than 20 mm of content
  between the header and the footer (a closing note, a table's last rows): it belongs on
  the page before;
- **colliding text**: two runs whose ink overlaps by more than 0.2 mm both ways (event names,
  end labels, rings on a strip, tick labels), as the label check measures it; runs that abut
  on one baseline are one line;
- **text past the paper**.

A test (`every_kind_is_checked`) fails when a kind's file never runs the check, so a new kind
comes with it.

## Later

- The owner's own report designs (saved and versioned like label designs), and the check run
  on them (and a greyscale render) as the label check runs on label designs.
