# Roasted coffee: resting and ready

Green coffee has its stock ([batches and beans](batches.md)); this is the other end. Every
roast you weigh becomes a lot of roasted coffee: Roastwarden works out when it has rested
enough to drink, when it is past its best, what is left of it, and when to roast again so the
next lot is ready before this one runs out. "What's ready to drink?" has an answer on the
Beans page.

## Where it shows

- **Beans → Roasted coffee**, at the top of the Beans page: one line per purpose (espresso,
  filter, either), amber when running low, then each lot with what is left and its state.
  Ready lots come first, oldest first (drink those first), then resting lots by when they are
  ready, then those past their best.
- **History → the roast**: one line with what is left of it and its state.

A lot's state is **resting**, **ready**, **past its best** (some left after its best-until
date), **finished** or **not weighed**. A finished roast with no roasted weight shows as not
weighed for 14 days: record the weight on its page in History. On the simulated roaster an
unweighed roast counts as a test and is not listed.

## Rest and best-until

A lot is ready from its roast date plus a rest that depends on its purpose and level, the
same days a bag label prints as "best from". It is at its best until the roast date plus 35
days for a light roast, 28 for darker. Settings → Roasting → Rest, roasted coffee and bag
labels:

| Setting | Default |
|---|---|
| Espresso, light | 10 days |
| Espresso, medium or darker | 7 days |
| Filter, light | 7 days |
| Filter, medium or darker | 4 days |
| At its best for, light | 35 days |
| At its best for, darker | 28 days |
| Ground, at its best for | 14 days |
| Roasted coffee running low below | 200 g |

A roast for omni, another purpose or none rests as filter; a roast with no level rests as
light. Dates are counted in your time zone: Settings → Roasting → Time zone (the dashboard
uses your browser's).

## Recording what happened to it

On each lot: **Used…**, **Gave…** (to someone, or sold), **Packed…** and **Finished**. An
amount can be grams (`36 g`), doses (`2 doses`, by the roast's purpose), `1 bag` or `all`;
the dose and bag sizes are under Settings → Roasting → Coffee costs, with the costs module on
(18 g espresso, 15 g filter, 250 g bag). Taking more than is left is refused, with how much is left. A count of
what is left after weighing it, coffee thrown away and taking a move back are on the command
line.

## How long it lasts

For each purpose the summary gives the grams ready, resting and past their best, and when the
next resting lot is ready. Once you record coffee as used, it also works out:

- **per day**: the grams used over the last four weeks, spread over at least a week;
- **lasts**: how long what is left lasts at that rate;
- **roast by**: the day to roast so a new lot has rested by the time this runs out.

Gifts, packed bags, discards and counts are not use, so they do not change the rate.

## Packing bags

Packed bags, to sell or give at a market or shop, come off a lot like use but do not count as
it. **Packed…** takes bags as `3x250g` (or `3`, of the bag size), **Whole bean** or
**Ground** with a grind (espresso, filter, moka, French press, or your own word). A ground bag
is at its best for less: until the lot's best-until date or the pack date plus 14 days,
whichever comes first.

Printing labels can do it for you: tick "Take 3 × 250 g off this roast, sold or given" on a
roast's Labels page ([labels](labels.md)). It is off unless you tick it: labelling the bags you
keep, right after roasting, changes nothing. The traceability report lists every
packed bag, from the green lot to the bag ([reports](reports.md)).

## Roasts in one bag

Small batches often go into one bag or jar: two 115 g roasts of the same coffee in a 250 g
bag. Put them together with **One bag with…** on a lot: tick the other roasts that went in,
and they show as one row, the bag, with each roast's batch number under it. **Add a roast…**
puts another batch in the same bag, and **Split** makes them separate again.

- **What you take from the bag** (Used…, Gave…, Packed…, Finished) comes off each roast in it
  by its share of what is left, so each roast's cost and history stay right. **Undo** takes the
  whole move back, from every roast.
- **Its rest**: the bag is ready when its newest roast is, and at its best until its oldest
  roast's best-until date.
- **Its label**: a label for any roast in the bag is the bag's. It prints every batch number
  ("batch #16+17"), the days they were roasted ("6–7 Nov 2026"), "best from" when the newest
  is ready, and their weight together. The share page behind its QR code says the same.
- **Where it came from**: the trace of each roast shows its share of the bag's moves.

Only roasts for the same use (espresso, filter or either), weighed, with coffee left and their
coffee recorded go into one bag.

## Blends

Batches of different coffees in one bag make a **blend**: a Guji and a Kenya mixed for
espresso, say. **One bag with…** lists the other coffees after the roasts of the same one.

- **Its shares**: each coffee's share of what is in the bag, shown on its row ("60 % Guji ·
  40 % Kenya Nyeri"). What you take comes off each roast by its share, so the shares stay.
- **Its name**: the coffees' names and the batches, unless you give it one ("House espresso").
- **Its label** names each origin with its share ("60 % Ethiopia · 40 % Kenya"), and the
  variety and process of each. Write its tasting notes on the label: one coffee's notes would
  not describe the blend. The share page behind the QR code lists every coffee in it.
- **Where it came from**: each roast's trace leads back to its own green coffee.

## Limits

No frozen coffee (the rest does not pause), no blend recipes (a blend is made from roasts
already roasted, not planned by percent before roasting), and no notification when a lot is
ready or the roast-by day comes.

## From the command line

`roast roasted [--for espresso|filter] [--ready|--resting]` lists it; `roasted show`, `use`,
`give`, `pack`, `toss`, `count` and `undo` record moves, `roasted combine '#16' '#17'` puts
roasts in one bag, a blend when they are different coffees (`uncombine` splits it), and
`roast weigh` the roasted weight
([the roast command](cli.md)).

<!-- site: end -->

## Design notes

For developers: how roasted coffee works and why. The site publishes only the part above.

Green coffee has a stock ledger (`batches.md`,
`green-coffee.md`); this is the other end: what came out of the roaster, how long it rests, when
it is at its best, what is left of it, and when to roast again. "What's ready to drink?" has an
answer in the Beans page and in `roast roasted`.

## What counts as roasted coffee

Every finished roast with a roasted weight is a **lot**. What is left is that weight plus its
moves (`roasted_moves`, migration 043): coffee **used**, **given** (to someone, or sold),
**packed** into bags (migration 050, below), **discarded**, or a **count** of what is left
after weighing it (`adjust`). Nothing derived is
stored; the state is worked out on every read for the owner's today: the API takes `tz`
(minutes east of UTC; the dashboard sends the browser's, the CLI its machine's unless that is
UTC), and a request without it gets the `time_zone` setting (Settings → Roasting, an IANA name
such as `America/Chicago`, DST right for each date), else UTC.

- A finished roast with no roasted weight is listed as **not weighed** for 14 days (record the
  weight: History → the roast, or `roast weigh`), except on `virtual_roaster`, where an
  unweighed roast is a test.
- A deleted roast takes its moves with it.
- A lot untouched for two months past its best counts as gone (an old Artisan import, a
  forgotten jar); a move on it brings it back.

## The rest window

- **Ready from**: the roast date (the DROP, in the owner's time zone) plus the rest by purpose
  and level, the same days the bag label prints as "best from" (`labels.md`, "Best from"):
  espresso 10 days light, 7 darker; filter 7 light, 4 darker (omni, other and no purpose rest
  as filter; no level rests as light). Settings → Roasting, `rest_days_*`.
- **At its best until**: the roast date plus 35 days for a light roast, 28 darker
  (`best_until_days_light`, `best_until_days`), where roasters' advice ends ("within a month
  to five weeks", in the research behind the rest windows).
- **States**: resting (before ready), ready, past its best (some left after best-until),
  finished (nothing left), not weighed.

Lots are listed ready first, oldest first (drink those first), then resting by when they are
ready, then past their best.

## For espresso, filter or either

Each lot is for **espresso**, **filter** or **either** (an omni roast, another purpose, or none
set). Per purpose the summary gives:

- ready, resting and past-best grams, and when the next resting lot is ready and how much;
- **per day**: the grams *used* in the last four weeks over the days since the first use in
  them (a week at least, so one morning is not a day's use). Gifts, packed bags, discards and
  counts are not use. No rate until something is recorded as used;
- **lasts**: everything left at that rate, and **roast by**: the day to roast so a new roast
  has rested (the newest lot's rest) when this runs out; today at the latest;
- **low**: less than `roasted_low_g` left (200 g; Settings → Roasting), or a roast is due.

`roast roasted --for espresso` shows espresso and either.

## Recording what happened to it

`POST /api/roasted/{roast}/moves` with `kind` used, given or discarded and one amount:
`grams`, `doses` (the espresso or filter dose, `dose_espresso_g` / `dose_filter_g`, by the
roast's purpose; refused for "either"), `bags` (`bag_g`), or `all`; or `kind: adjust` with
`set_g`, the grams left now. A gift can name its `recipient`; any move takes a `note` and a
date (`at`). Taking more than is left is refused with how much is left. `kind: packed` takes `bags` of `bag_g`, whole or ground ("Packed bags" below). `DELETE
/api/roasted/{roast}/moves/{move}` takes one back. The inventory and admin token presets may
record moves; every preset reads.

## Packed bags

Bags packed from a lot, to sell, for a market or a shop (`lean-home.md`):
`kind: packed` with `bags` (a whole number) and `bag_g` (grams a bag; default the bag size,
`bag_g` in Settings → Roasting), and the form: **whole bean** (`form: whole`, the default) or
**ground** (`form: ground`) with a `grind` ("espresso", "filter", "moka", "French press", or
the owner's own word; a grind alone means ground). The grams come off like *used*, refused
when more than is left; they are not use, so the per-day rate does not count them. Migration
050 adds `bags`, `bag_g`, `form`, `grind` and `label_run` to `roasted_moves`.

- **Ground keeps its best for less**: a ground bag's best-until is the earlier of the lot's and
  the pack date plus `best_until_days_ground` (Settings → Roasting, 1–60, default 14: Workshop
  Coffee, who sell pre-ground bags, say to finish one "within two weeks of the 'Ground On'
  date"). Each packed move answers its own `best_until`. "Best from" (the rest) is the lot's.
- **From a label print run**: an item of `POST /api/label-runs` with `pack: true` takes its
  copies × the label's weight (`weight_g`, else the roasted weight) off the roast when the run
  is made, as a packed move that names the run (`label_run`; the run answers its `packed`
  moves). The item's `form` and `grind` print on the labels too (`labels.md`, "Whole bean or
  ground"). More than is left refuses the run before it is made; the same labels again (a
  reused run) take nothing more; a token whose preset may not record moves (`roast`) cannot
  pack. On the dashboard: a roast's Labels page has "Take 3 × 250 g off this roast, sold or
  given", off until ticked: bagging and labelling every roast right after roasting is how the
  coffee is stored, not sold, and on by default the print would take such a bag off as packed.
- **From the lot**: Beans → Roasted coffee → **Packed…** (bags as `3x250g`, or `3` of the bag
  size; Whole bean or Ground with a grind), or `roast roasted pack <roast> 3x250g [--ground
  espresso]`.
- **Where it went**: the trace (`reports.md`, "Traceability") lists every packed bag with its
  form, grind and label run, from the green lot to the bag and back.

## Roasts in one bag

Small batches are often smaller than the bags they go into (two 115 g roasts mixed into one
bag). A **combined lot** (migration 061, `roasted_combines` and `roasted_combine_roasts`) holds
two or more roasts, of one coffee or a blend. Each roast keeps its own ledger, so costs per
roast, the use rate and the trace stay per roast; the lot is a layer over them.

- **Making one**: `POST /api/roasted/combined` `{roasts, name?, note?}`. Every roast finished,
  weighed, with coffee left, with its bean recorded (a blend's label must name every coffee),
  for the same use (`for`). When one of them is in a lot
  already the others join it; roasts in two different lots are refused. A roast is in one lot
  at most (`session_id` is the membership's key). The name defaults to the bean's with the
  batches ("Ethiopia … #16+17"); a blend's, its beans' joined by " + ", the largest share first.
- **Coffees and shares**: `CombinedRef.coffees` gives each bean once with `share_pct`, its
  roasts' share of what is left together (of their roasted weights once nothing is), whole
  percents adding up to 100 (largest remainder, `roasted::percents`); `blend` is true for more
  than one bean. Moves split by share, so the shares hold until a roast joins. A blend's
  `CombinedLot` has no `bean_id` or `bean_name`.
- **Moves**: a move on any of its roasts (`POST /api/roasted/{roast}/moves`, so the CLI,
  the dashboard and label runs need nothing new) is planned against what the roasts have left
  together, then split by each one's share of what is left (by roasted weight when nothing is,
  a count up from empty), to a tenth of a gram, the last roast taking the rounding, none more
  than it has. The rows share a `combine_move` id; deleting any of them takes the whole move
  back. Moves from before the roast joined stay its own.
- **Its rest**: ready from its newest roast's ready date, at its best until its oldest roast's
  best-until; its state is worked out like a lot's from what is left together. In
  `GET /api/roasted` each roast in a lot carries `combined` (the lot's summary), counts once in
  its purpose's `lots`, and takes the lot's state for the ready, resting and past-best sums and
  the list's order (its roasts side by side, oldest first).
- **Reading it**: `GET /api/roasted/combined/{id}` (the lot's id or any of its roasts') answers
  its roasts and its moves, each with how it was split.
- **Splitting it**: `DELETE /api/roasted/combined/{id}`; each roast keeps its share of the
  moves. A deleted roast leaves its lot, and a lot left with one roast goes (a trigger).
- **A blend's label**: `label_lot` also answers the bag's own name and its beans with their
  shares; `labels::facts` reads each bean into a `BlendPart` (short name, origin, region,
  variety, process, share) and makes the facts the bag's, not the printed roast's bean: the
  name the bag's own, else the parts' short names joined by " + "; no producer; no notes
  unless the owner types them (the bean's label or supplier notes, or one roast's cupping,
  would describe one coffee). `label_data` prints `origin` as `blend_origin` ("60 % Ethiopia ·
  40 % Kenya": one country's coffees together, a coffee with no origin by its name), the
  distinct varieties and processes joined by " / " (the line's fields are already split by
  " · "), `blend: true` and `components` (hidden
  with `origin`). Every design, saved ones too, prints a blend with no change. The share page
  adds a list, one line per coffee ("60 % Guji (Guji, Ethiopia), natural"). The design and
  brand stay the printed roast's bean's.
- **Labels and the share page**: `labels::facts` reads the lot (`roasted::label_lot`): `batch`
  is "16+17" (a number for one roast; the built-in designs print it with `with`), `roasted` the
  range ("6–7 Nov 2026", "30 Oct – 2 Nov 2026"), `roasted_iso` the oldest day and
  `roasted_last_iso` the newest, "best from" from the newest, best-until from the oldest, the
  default weight the roasts' roasted weight together. A label run's `pack` checks the lot's
  coffee, and two items on roasts of one lot draw on the same grams.
- **The trace**: each roast carries `combined`; its share of a lot's move reads "its share,
  from the bag #16+17".
- **Agents**: `roast roasted combine <roast>… [--name] [--note]`, `roast roasted uncombine
  <roast>`; `roasted show` names the bag (a blend's coffees and shares); the inventory and admin
  presets may combine and split.

## Where it shows

- **Beans page**: "Roasted coffee" first, one line per purpose (running low in amber), each
  lot with what is left and its state, and Used…, Gave…, Packed… and Finished; a bag of several
  roasts as one row with its batches (a blend's coffees and shares), One bag with… (the same
  coffee's roasts first, then other coffees), Add a roast… and Split.
- **History → a roast**: one line with what is left of it and its state.
- **Settings → Roasting**: the rest days, the best-until days (light, darker, ground) and the
  low mark.
- **CLI** (`cli.md`): `roast roasted [--for] [--ready|--resting] [--all]`, `roasted show`,
  `use`, `give`, `pack`, `toss`, `count`, `undo`, `combine`, `uncombine`; `roast trace`; the
  skill's "Roasted coffee" section.

## Not built

- Frozen coffee (Beanconqueror keeps frozen and thawed dates: the rest pauses in the freezer).
- Blend recipes (green coffee drawn by percent from several beans before one roast:
  `green-coffee.md`), and a blend's own tasting notes kept with the bag rather than typed per
  label run.
- A notification when a lot becomes ready or the roast-by day comes; roasted coffee in the
  inventory report's stock-take.
