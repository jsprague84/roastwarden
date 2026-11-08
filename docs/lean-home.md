# Lean home, deep roastery

Roastwarden does more than a home roaster needs: PDF reports, cupping, autotune, bag labels,
costs, tokens for agents. Modules switch the optional parts off. A home install shows the
roast, its history, profiles, beans and roasters, and nothing else in the way. Turn a module
back on and everything it held is still there.

## The first run

A new install (nothing set up yet, no roast and no bean) opens a **Welcome** page once, in
place of the Roast page. It asks four things:

- **What kind of roasting?** Home, Enthusiast or Roastery: a preset of modules (below).
- **Temperatures in** °C or °F.
- **Weights in** g, kg or oz, lb.
- **Time zone**: this browser's zone is filled in. It sets the day a roast falls on, for labels
  and reports.

**Save and add a roaster** saves them and opens Roasters → Add (or the simulator's page when
the simulated roaster is the only one). **Not now** saves Home and opens the Roast page. All of
it can be changed later in Settings.

## Presets and modules

Settings → Modules: pick a preset, or switch modules one by one ("Your own mix").

| Preset | What it shows |
|---|---|
| Home | no modules: Roast, History, Profiles, Beans (green stock, roasted coffee and planned roasts, no prices), Roasters, Settings |
| Enthusiast | Home, plus Reports, Cupping, Tuning, and Agents & access |
| Roastery | everything: adds Costs and Labels, for selling coffee |

| Module | What it adds |
|---|---|
| Reports | PDF reports: the Reports section and a roast's Report button |
| Cupping | the cupping form and its scores on a roast |
| Tuning | autotune, the gain schedule and fan assist on a roaster's pages. The PID and Profile drive stay: they follow the roaster's control level |
| Agents & access | Settings → Tokens, Approvals and Audit, for scripts and agents |
| Costs | prices on purchases, cost per kg and per roast, spending and the shopping list |
| Labels | the Labels section (Label Studio), a roast's labels and the share pages their QR codes open |

Some parts are never modules: the Roast page, alarms, guidance, notifications, history,
profiles, green stock and roasted coffee, and the roasters with their check.

What switching a module off does:

- It leaves the menu bar, Settings' menu and the buttons that lead to it. A link to one of its
  pages shows one line instead, such as "Labels are off: Settings → Modules".
- Nothing is deleted. Turn it back on and its data is there.
- It is not a lock. The API stays open to scripts and the `roast` command line, and tokens
  still decide what they may do. An agent with the Roastwarden skill reads the modules and
  does not offer what is off; ask it again and it goes ahead.

An install that has never saved the setting has every module on.

From the command line: `roast modules` shows what is on and the preset it matches;
`roast modules --preset home`, `--on labels` or `--off cupping` change it (an admin token).

## Weight units

Settings → This browser → **Weight unit**, per browser like °C and °F:

- **Metric**: grams, then kilograms from 1 kg.
- **US**: ounces, then pounds from 1 lb. Prices per kg show per lb.

A weight field takes a number with a unit ("250 g", "8 oz", "2 lb", "1.5kg"); a bare number
is grams or ounces, and the field says which. The server stores grams whatever you pick.
Labels print both ("250 g · 8.8 oz"), and reports have their own unit setting under Settings
→ Roasting.

## Packed bags and ground coffee

On a roasted lot, Beans → Roasted coffee → **Packed…** records bags packed from it: how many
and their size ("3x250g"), whole bean or ground with a grind (espresso, filter, moka, French
press, or your own word), and the date. The grams come off the lot, but are not counted as
used. A label print for the roast can do the same: its dialog offers "Take 3 × 250 g off this
roast" when that much is left, off until you tick it.

A ground bag's label reads "Ground · espresso" where a whole-bean one reads "Whole bean".
Ground coffee goes stale sooner: its "best until" is the earlier of the lot's and the pack date
plus **Ground, at its best for** (Settings → Roasting, 14 days unless you change it). More on
lots and their moves in [roasted coffee](roasted.md).

From the command line: `roast roasted pack <roast> 3x250g --ground espresso`.

## Traceability

A bag's batch number leads back to its green coffee. Reports → **Traceability** (or
`roast trace <roast|coffee>`) shows the green coffee (supplier, lot, origin, process,
purchases), each roast of it (batch number, date, roaster, profile, green and roasted weight,
loss) and where the coffee went: packed bags with form and grind, label runs, gifts with the
recipient, used, discarded, and what is left. It holds no prices.

## Profiles for your kind of roaster

A profile may say which kind of roaster it was written for. The seed profiles include the
fluid-bed ones and two for a gas drum (a light filter and a medium omni roast). The profile
library lists those for the selected roaster's kind first and marks the others ("for a drum",
"for a fluid bed"); nothing is hidden. A profile saved from a roast takes its roaster's kind;
one with no kind fits any roaster. See [profiles](profiles.md).

<!-- site: end -->

## Design notes

For developers: how modules, the first run and the rest of this page work and why. The site
publishes only the part above.

A home install shows only what a home roaster uses; a roastery turns on the rest with one
switch. Each part below removes clutter or closes a gap someone hits (some roasters grind
before packing, for one); nothing here adds a page a home roaster must pass through, and an
existing install looks and works as before unless its owner changes a switch. The risk this
guards against is bloat: clutter and complication that help no one.

## Modules

One server setting, `modules`: the list of optional parts that are on. Six, no more
(`src/auth/modules.rs` holds the six and the presets):

| Module | What it shows when on |
|---|---|
| `reports` | the Reports section, a roast's Report button |
| `cupping` | the cupping form and scores on a roast |
| `tuning` | Roasters → Control's autotune, gain schedule and fan assist (Profile drive and the PID stay: they follow the roaster's control level) |
| `agents` | Settings → Tokens, Approvals and Audit, as one group "Agents & access" |
| `costs` | prices on purchases, cost per kg and per roast, the shopping list |
| `labels` | the Labels section, a roast's Labels, share pages, packed bags and the traceability report |

Presets, each a list, picked in the first-run page and in Settings → Modules:

- **Home**: none of them. Sections: Roast, History, Profiles, Beans (stock, roasted coffee and
  plans, no prices), Roasters, Settings.
- **Enthusiast**: `reports`, `cupping`, `tuning`, `agents`.
- **Roastery**: all six.

Rules:

- Not set (or empty) = every module on, so an install from before modules existed is
  unchanged. `GET /api/about` and `roast status` report `modules`, the `preset` they match
  (`home`, `enthusiast`, `roastery` or `custom`) and `first_run` (true when the setting was
  never saved and the database has no roast and no bean).
- An off module leaves the nav, the Settings menu and the buttons and cards that lead to it.
  Its API routes stay (tokens still gate them; the parity check stays simple). A link to a page
  of an off module shows one line: "Labels are off: Settings → Modules", not a 404. The
  dashboard's menus and page-to-module map are in `lib/modules/modules.ts`; the root layout
  shows the "off" line.
- Turning `tuning` off keeps the PID and Profile drive (and fan assist's status line while it
  runs); `agents` off also hides "Tokens may control this roaster".
- Core is never a module: the Roast page, alarms, guidance, notifications, history, profiles,
  green stock and roasted coffee, roasters and their check.
- The skill reads `modules` from `roast status`. It does not offer what is off; asked for it,
  it says it is off and where to turn it on, and goes ahead only if the owner asks again.
  Hermes task 40.
- `roast modules [--preset home|enthusiast|roastery] [--on <m>…] [--off <m>…]`; the
  `admin` token preset may change it.
- The browser checks save the modules on a fresh database so they open the Roast page.

### First run

When `first_run` is true the dashboard opens `/welcome` instead of the Roast page, once: one
page, no steps. It asks what kind of roasting (the three presets, each with one line of what
it shows), the temperature unit, the weight unit and the time zone (the browser's, prefilled),
then Save, which writes the settings and goes to Roasters → Add (or the simulator's page when
`ROASTWARDEN_VIRTUAL_ROASTER` is on and no other roaster exists). "Not now" saves Home. Save
writes `modules`, `report_units` and `time_zone` in one call, and the browser's temperature and
weight units. Settings → Modules changes it later.

## Weight units

Per browser, like °C/°F (`lib/stores/units.svelte.ts`: `weightState`, `setWeightUnit`, key
`roastwarden.weight_unit`): **Metric** (g, then kg from 1 kg) or **US** (oz, then lb from
1 lb). Storage, the wire and the API stay grams.

- `fmtGrams` (`lib/beans/stock.ts`) and every weight the dashboard shows go through it.
- Weight inputs take a number with a unit ("250 g", "8 oz", "2 lb", "1.5kg", as the CLI does;
  `lib/beans/weight.ts` formats and parses those forms); a bare number is the browser's small
  unit (g or oz), and the field says so. Every weight field is a `WeightInput` that binds grams
  and keeps a stored weight exact until edited.
- Doses stay in grams (weighed in grams in the US too); prices per kg show per lb in US.
- Set in Settings → This browser and on the first-run page; no header toggle (the header has
  enough). Reports keep their own `report_units` setting.
- Labels keep printing "250 g · 8.8 oz" (both, a legal net weight).

## Control from a token, a roast's channels, TC4 units, the register map

- **An `admin` token may switch a roaster's control on** (`manual` or `full`). A setup `PUT`
  that leaves control on is held as `roaster.control` for every token, as a label print is
  (stricter than "tokens that ask first"), and waits for the owner's approval (`control.md`,
  "Approvals"), as starting a roast on a physical roaster does; turning control off or any
  setup change at control off goes straight through. `roast roaster control <id> manual
  [--wait]`.
- **Each roast keeps its channel list** (migration 049, `roast_sessions.channels`): the
  roaster's declared channels at CHARGE (or at the start of recording) are saved on the roast
  (`'[]'` for none). History, the PDF report, `roast review` and the Artisan export read the
  roast's own list; a roast recorded before the list was kept has NULL and reads the roaster's
  current list (`channels::for_roast`).
- **TC4 extra columns get a unit**: a column mapped to a declared channel whose unit is a
  temperature follows the device's unit (`UNITS;F` converts it to °C like BT and ET); other
  columns pass through. A suffix on the column overrides it (`exhaust:F|C|none`).
- **The Modbus register map is editable on the roaster's page**, not only while adding the
  roaster: `RegisterMapEditor.svelte` serves the Add wizard and the Overview.

## Packed bags, ground coffee and traceability

- **Packed** is a move on a roasted lot (`roasted.md`): a number of bags of a size, **whole
  bean** or **ground** with a grind ("espresso", "filter", "moka", "French press", or the
  owner's own word), and the date. It takes the grams off like *used*, and is not use.
  Migration 050 (`bags`, `bag_g`, `form`, `grind`, `label_run` on `roasted_moves`).
- It is recorded from the lot (Beans → Roasted coffee → Packed…, `roast roasted pack <roast>
  3x250g [--ground espresso]`), or by a label print run for a roast: the print dialog offers
  "Take 3 × 250 g off this roast" when that much is left, off until ticked. A print run item
  with `pack` packs once (a reused run does not).
- A ground bag's label reads "Ground · espresso" where a whole-bean one reads "Whole bean"
  (the designs that print a form line: plain prints it, on round labels only when ground; shop
  says "Ground coffee"). Ground coffee goes stale sooner: its "best until" is the earlier of the
  lot's and the pack date plus `best_until_days_ground` (Settings → Roasting), default 14 days
  after Workshop Coffee's "within two weeks of the 'Ground On' date"
  (https://workshopcoffee.com/pages/more-on-our-pre-ground-coffee). "Best from" is unchanged.
- **Traceability** (module `labels`): `GET /api/trace/roast/{id}` and `/api/trace/bean/{id}`
  (`routes/trace.rs`), `roast trace <roast|bean>`, and a report kind `trace`
  (`reports/trace.rs`, `roast report trace`): the green coffee (bean, supplier, lot, origin,
  process, purchases), each roast (batch number, date, roaster, profile, green → roasted, loss)
  and where it went (packed bags with form and grind, label runs, gifts with the recipient,
  used, discarded, what is left). A bag's printed batch number leads back to the green lot. No
  prices (a costs switch on the report is not built). Hermes task 41.

## Seed profiles by roaster kind

- A profile may name the roaster `kind` it was written for (migration 051,
  `roast_profiles.roaster_kind`); the fluid bed seeds say `fluid_bed_electric`. A profile saved
  from a roast takes its roaster's kind; the `.alog` is unchanged.
- Two `drum_gas` seeds (light filter, medium omni) from `scripts/gen_drum_profiles.py`, written
  from published drum roasting practice (sources in `profiles.md`) and checked open loop on
  `virtual_drum`'s model: within 7.4 °C of the curve, first crack within 21 s.
- The profile library lists the selected roaster's kind first and marks the others ("for a
  drum" / "for a fluid bed"); nothing is hidden. Electric and gas drums are one family; a
  profile with no kind fits any roaster. Profile drive of a drum's burner and air is not built.

## Generic skill wording

The skill's examples say `<roaster>`, with one line on finding it (`roast roasters`), rather
than naming one roaster; only the installed copy's "This deployment" paragraph, on the agent
host, names that deployment's roasters. The skill reads the modules (above).
