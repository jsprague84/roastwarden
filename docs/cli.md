# The `roast` command line

`roast` uses and administers Roastwarden from a shell: for you, for scripts, and for AI
agents. It is a thin client over the server's API with an API token, so it works on any machine
that reaches your server. Nearly everything the dashboard does with your records has a command:
roasts and their records, green coffee, orders and the shopping list, roasted coffee, the plan,
profiles, labels, reports, roasters, notifications, settings and backups. What it leaves out is
live control of a roaster (setpoint, heater, fan, mode, a profile drive) and PID tuning: you do
those at the roaster, and `roast api` reaches the API for anything without a command.

An AI agent with a shell uses `roast` through Roastwarden's Agent Skill, which teaches it the
commands, the exit codes and the rules: see [Agents](agents.md).

## Setup

Each release on the project's releases page has `roast` for Linux x86_64 and aarch64 with
its SHA-256 sums (`roast-linux-<arch>`, `roast-SHA256SUMS`); the server's image has it too
(`/usr/local/bin/roast`). Use the one of the server's
version: `roast status` shows the server's, `roast --version` its own.

```sh
export ROASTWARDEN_URL=https://roast.example.com        # default http://localhost:8080
export ROASTWARDEN_TOKEN=rr_…                           # Settings → Tokens
roast status
```

`--url` and `--token` override the environment; `--json` and `--text` choose the output
([Output](#output)). `roast` never picks up the owner key by itself, not even inside the
container where `ROASTWARDEN_API_KEY` is set: you pass it on purpose,
`docker exec roastwarden sh -c 'roast -t "$ROASTWARDEN_API_KEY" …'`. Reads need a credential
too (any token), unless the server runs with `ROASTWARDEN_READS=open`
([Security](../SECURITY.md)); a token that is sent must be valid even for a read. The audit log
is the owner's and admin tokens' only.

On a **finished** roast `weigh` and `landmark` work on any roaster; on one still running, a
token needs **Tokens may control this roaster** switched on under Roasters → the roaster →
Control ([`control.md`](control.md), "The real-hardware rule").

Which preset a token needs ([`control.md`](control.md), "Tokens"): `read_only` for every read but the
audit log (admin);
`inventory` for beans, stock and the plan (`roast` may change the plan too); `roast` for a
roast's record (weights, landmarks, notes, cupping); `admin` for profiles, settings,
imports, deletes of roasts and `backups --now`. A token never controls
a physical roaster unless you allowed it for that roaster.

## Commands

| Command | Does | Needs |
|---|---|---|
| `health [--ready]` | `/healthz` and the version; `--ready` also MQTT and the database (`/readyz`) | nothing |
| `busy [--wait MINUTES]` | is a roaster in the middle of something a restart would cut: a profile drive, the server's autotune, cooling after DROP, its heater on, a roast with beans at 50 °C or more (`/api/busy`). Exit 1 while a real roaster is; the virtual roaster is listed but never counts. `--wait` checks every 30 s up to that long. Deploys wait on it (DEPLOY.md) | nothing |
| `status` | version and how many commits past its release (`build`), revision, uptime, problems (Settings → About), roasters heard and online, planned roasts, low beans, last backup, whether a roaster is busy, which token (and preset) this is, the modules that are on (`modules`, `preset`, `first_run`), and the licence and the source of the running revision (`licence`, `source_url`); when the Modbus slave is on, the Artisan screen line (`artisan_screen`); when the server runs its own MQTT broker, where it listens and the roasters that may sign in (`mqtt_broker`, [`DEPLOY.md`](DEPLOY.md), "One container") | nothing |
| `login [ADDRESS]` | check an API token against the server, then keep the address and token in `~/.config/roastwarden/roast.json` (mode 600; `ROASTWARDEN_CONFIG` names another file): `roast` and an agent's shell then need no environment. The token is asked for without showing it, or read from stdin (`roast login https://… < token.txt`), or taken from `--token` / `ROASTWARDEN_TOKEN`; the owner key is refused | the token being saved |
| `logout` | forget the saved address and token | nothing |
| `skill install [claude-code \| hermes \| agents] [--dir DIR]` | write the Agent Skill built into this `roast` (SKILL.md, label-design.md) where the agent reads skills: `~/.claude/skills/roastwarden`, `~/.hermes/skills/devops/roastwarden`, `~/.agents/skills/roastwarden` (Codex and other Agent Skills readers), or `--dir`; it names the saved server; an older SKILL.md is kept as `.bak`. [Agents](agents.md) | nothing |
| `skill show [--label-design]` | print the skill (or its label design guide) for an agent with no skills folder: paste it into its instructions | nothing |
| `artisan-screen [--aset FILE [--host H] [--fahrenheit]]` | Artisan as a screen ([`artisan-screen.md`](artisan-screen.md)): whether the server's Modbus slave is on (`ROASTWARDEN_MODBUS_ADDR`), its address, the roaster it serves (or why none), read-only or with writes, when Artisan last read and from where; exit 1 while it is off. `--aset FILE` saves the Artisan setup that reads it (Help → Load Settings in Artisan; `-` for stdout): BT, ET, heater and fan, and the Air and Burner sliders only when writes are on; `--host` the server's address as Artisan reaches it (default: the host in `ROASTWARDEN_URL`), `--fahrenheit` for an Artisan in °F (loading the file sets Artisan's unit) | nothing |
| `roasts [--roaster R] [--limit N]` | roasts, newest first, with their numbers | nothing |
| `review <roast> [--vs <roast>] [--within C] [--roaster R]` | a roast read ([`analysis.md`](analysis.md)): marks with BT, phases, development and DTR, loss; RoR (the early peak, each minute, the late low, FC→drop slope); BT against its profile (at 0:30–2:00, the overshoot after CHARGE, RMS/bias/max from TP, FC and drop against the profile's); the heater and fan (first 30 s, time at 90 % or more, the last minute); its numbers judged against what was expected, and notes; and how the live prediction did on it (`prediction`: the model's and the projection's error 2 min ahead, first crack and DROP as predicted about 2 min before, and the RoR warnings given against what the RoR did: `ror_warnings` with `came_t` and `lead_s`, `ror_events`; [`prediction.md`](prediction.md)); each of the roaster's declared channels at CHARGE, first crack and DROP (`channels`: an exhaust probe, burner pressure; [`channels.md`](channels.md)). `--vs` lines it up against another roast as `compare` does. For a bean with fewer than three roasts on the profile it also prints "Next roast of this bean" (`next_roast`, `bean guide --after`). A roast: its id, `#12` (batch number) or `latest` | nothing |
| `compare <roast> <roast> … [--ref <roast>] [--within C] [--curve] [--roaster R]` | 2–8 roasts against the first (or `--ref`): each number with its difference from the reference, and from when each roast's BT stays within `--within` °C (default 0.5) of the reference's; `--curve` adds BT, RoR, heater and fan every 30 s ([`analysis.md`](analysis.md)) | nothing |
| `show <roast> [--telemetry] [--roaster R]` | one roast with its events, cupping and, when known, its coffee cost; the samples only with `--telemetry` (otherwise their count). `<roast>`: an id or `latest` | nothing |
| `weigh <roast> <roasted> [--green G] [--roaster R]` | a roast's roasted weight (as typed, `85.75`), and green weight: weight loss and cost per dose follow | roast |
| `landmark <roast> <kind> --at M:SS [--temp C] [--note …] [--roaster R]` | move a landmark (`fc`, `drop`, `dry`, `tp`, `charge`, `fc-end`, `sc`, `sc-end`, `cool`) or add it; the time is from CHARGE as the app shows it, the temperature the roast's own BT then unless given; `--rm` removes it. A finished roast's DTR, development RoR and AUC follow | roast |
| `edit <roast> [--name --notes \| --add-note --rating 1-5 --level --purpose --bean --green --roasted --color-whole --color-ground --color-scale --moisture-green --moisture-roasted --density-green --ambient --humidity --flags] [--roaster R]` | a roast's record; absent fields are kept | roast |
| `cup <roast> [NAME=SCORE …] [--taint N] [--fault N] [--notes …] [--rm] [--roaster R]` | the roast's cupping on the classic SCA form ([`cupping.md`](cupping.md)): `aroma`, `flavor`, `aftertaste`, `acidity`, `body`, `balance`, `overall` 0–10 in quarter points; `uniformity`, `clean_cup`, `sweetness` 2 a cup over five cups (0, 2 … 10); `--taint`/`--fault` the cups with a taint (2 off each) or a fault (4 off each). Without scores it shows it; scores not named keep theirs (on a first cupping 6.00, and 10 for the cup attributes); the answer has `total_score`, `defects`, `final_score`, `band`; `--rm` removes it | roast |
| `export <roast> [--csv] [-o FILE] [--roaster R]` | the roast as an Artisan `.alog` (or CSV) file; `-o -` writes to stdout | nothing |
| `labels <roast> [--sheet 5163\|6874\|5294\|… --design plain\|gift\|script\|ascii\|cherry\|ID[@N]\|… \| --source FILE.typ] [--set K=V …] [--brand B] [--count N --start N --weight G --message … --notes … --hide F,…] [--qr app\|share\|supplier\|none\|URL] [--svg] [-o FILE \| --print [--wait]] [--roaster R]` | bag labels for a roast ([`labels.md`](labels.md)): a PDF to print at "Actual size", from label N of a part-used sheet; `--qr` what the QR code opens: `app` (the roast in the dashboard), `share` (its public share page, made when it has none by a caller who may share, and only by labels that are made: `--svg` and `labels design check` show a placeholder code), `supplier` (the coffee's offer, its bean's link), `none`, or an address (default: the design's own, `share` for gift, else the `label_qr` setting, else `app`); `--svg` the first page with outlines (a preview); `--source` a design's file not saved (a one-off), `--set` its parameters (`KEY=@FILE` reads a file), `--brand` a brand kit; with none of them the roast's bean's own label (`labels design use`); `--print` sends them to the label printer (the default, or `--printer P`, with `--tray`, `--color`) at actual size on label stock, and from a token it always waits for the owner's approval (exit 3; the approval says which sheet to load and from which label); `labels --calibration --sheet S` the sheet's calibration page; `labels --list` the sheets (with offsets), designs, fields, rest days and fonts. Dates are this machine's local dates, or on a machine in UTC the server's `time_zone` setting | nothing (a token for `--source`, `--set`; `roast`, `inventory` or `admin` for `--print`) |
| `labels design check <FILE.typ \| design> [--sheet S] [--roast R [--roaster R] \| --bean B] [--set K=V …] [--message … --notes … --weight G --hide F,… --qr Q] [--retail [BAG]] [--diff DESIGN] [--png [FILE]] [--dpi N]` | lay one label of a design out on the server's sandbox and report: Typst's errors with file and line (exit 6), or each line of text with its font, size and box in mm and the issues (overflow, near_edge, overlap, too_long, cut, tiny_text, qr_small, qr_crowded, typst warnings, unknown params; exit 1; a `cut` says how much of the label was free; soft, still exit 0: `dropped` when the name, roast date or best-from did not print, `hint` a design's own note), which fields printed (`fields.shown` / `missing`, found by their words in any case or slug, a date in its usual forms); `--png` the label as a picture (36–200 dpi). Sample data unless `--roast`; `--bean` puts a coffee's name, origin, process and notes on the sample roast. `--retail [BAG]` reads it as a label for coffee that is sold, on that bag (`pouch-12oz`, `pouch-8oz`, `sample-2oz`, … from `labels --list`; `label` when the label is the bag's whole front; alone: the `label_bag` setting): the statement of identity, the net weight (both units, never rounded up, bottom 30 %, its letters as tall as the bag's front needs), the packer's name, city, state and ZIP, with what no check can know (`retail_*` issues; [`labels.md`](labels.md), "The retail check"). `--diff sam-gift@2` also lays out another design or version on the same data and says what changed (lines added, gone, moved or resized, issues new and gone). Exit 0 when clean ([`labels.md`](labels.md), "Designing a label"; the skill's `label-design.md`) | any token (`read_only`) |
| `labels design list` | every design: saved (`id@version`, its parameters), the owner's folder, built in | nothing |
| `labels design show <design[@N]> [--source] [-o FILE]` | a design's details, every version (who, when, note) and its source; `--source` prints only the source, `-o` writes it (start a new design from a built-in) | nothing |
| `labels design add <id> <FILE.typ> [--name --description --note --sheet S]` | save a new design (version 1); it must compile (exit 6 with Typst's errors, nothing saved), and the answer has the check's report on sample data; the id may not be a built-in's or a folder design's | inventory |
| `labels design edit <id> [FILE.typ] [--name --description --note --sheet S]` | a new version from the file (checked first), or a new name or description | inventory |
| `labels design rollback <id> <version> [--note]` | back to an earlier version: its source becomes the next version | inventory |
| `labels design use <design[@N]> --bean B [--set K=V …] [--brand B] \| --bean B --clear` | a coffee's own label: its roasts' labels use this design, parameters and brand unless told otherwise (`id@N` pins a version) | inventory |
| `labels design rm <id> [--wait]` | delete a saved design and its versions; refused while a bean uses it; may be held for approval | inventory |
| `labels asset list` | the images and fonts designs can use, with a font's family | nothing |
| `labels asset add <FILE> [--name N] [--note …]` | add or replace an image (`.svg`, `.png`, `.jpg`, 2 MB) or a font (`.ttf`, `.otf`, 8 MB); a design names an image by its name (`image("logo.svg")`), a font by its family | inventory |
| `labels asset get <name> [-o FILE]` | save an asset's file | nothing |
| `labels asset rm <name> [--wait]` | delete an asset; may be held for approval | inventory |
| `labels brand list` | the brand kits | nothing |
| `labels brand show <id>` | one brand kit | nothing |
| `labels brand set <id> [--name --tagline --place --website --logo ASSET --ink --accent --paper '#rrggbb' --font-display --font-body --font-script]` | add a brand kit or change the flags given (an empty value clears one); a design reads it as `l.brand`; `label_brand` (setting) is the default brand | inventory |
| `labels brand rm <id> [--wait]` | delete a brand kit; may be held for approval | inventory |
| `labels printer [ID]` | a label printer asked now (default the default printer): make and model, state and reasons (a door open, no paper), what it takes (PDF, PWG Raster, URF), trays; exit 1 when it cannot print now. Prints nothing | nothing |
| `labels printer list` | every label printer, the default first: how labels go to it (`pdf`, `pwg`/`urf` with the resolution), its calibrations per sheet, and what it said when last asked ([`labels.md`](labels.md), "Printers") | nothing |
| `labels printer add <id> <address> [--name … --tray T --color monochrome\|color --send auto\|pdf\|pwg\|urf --trust SHA256 --default]` | add a printer by its address (`ipp://host:631/ipp/print`; `ipps://…` with `--trust` and the fingerprint from `printer certificate`); it is asked what it can do. The first printer is the default | admin |
| `labels printer edit <id> [--address … and the flags of add]` | change a printer (an empty value clears one) | admin |
| `labels printer rm <id> [--wait]` | delete a printer and its calibrations; may be held for approval | admin |
| `labels printer default <id>` | labels go to this printer unless another is named | admin |
| `labels printer find` | IPP printers on the server's network (mDNS, 4 s): only on the server's own network segment (in Docker with host networking) | nothing |
| `labels printer certificate <address>` | the SHA-256 of the certificate an `ipps://` printer shows, to check on the printer and then `--trust` | admin |
| `labels printer calibrate <id> --sheet S (--right MM --down MM \| --first R,D [--last R,D]) [--tray T] \| --clear` | a printer's offset for a sheet; `--first`/`--last` are the first and last labels' marks on the calibration page (more than 0.5 mm apart means the page scales: refused) | admin |
| `labels run (--roast R … [--count N --design D --set K=V … --brand B --message … --weight G --ground GRIND --notes … --hide F,… --qr Q --retail [BAG]] \| --items JSON\|@FILE) [--roaster R] [--sheet S] [--start N] [--note …] [--new] [--pack] [-o FILE \| --print [--wait]]` | a **print run**: one sheet of labels for one roast or several at once (a gift run), each roast with its design (default its coffee's own), settings, brand and copies, kept together, from the sheet's next free label (`labels sheets`) unless `--start`. `--ground espresso` prints "Ground · espresso" (else "Whole bean") and a ground bag's earlier best-until; `--pack` takes the bags off each roast as packed (`--count` bags of the label's weight; refused when that much is not left; a reused run does not take them twice). The run is kept with its PDF (`labels runs`); without `--print` the PDF is written here, to print at Actual size, then `labels runs done`; the same labels as a run made in the last 24 hours and not yet printed answer that run (`"reused": true`) unless `--new`; `--print` sends it to the label printer (a token's print waits for the owner's approval: exit 3). `--items` gives each roast its own design, message and copies: `[{"roast":"latest","count":2,"design":"gift","message":"for Sam"}]` ([`labels.md`](labels.md), "Print runs") | inventory |
| `labels runs [--limit N]` | the print runs, newest first: what (the roasts and copies), the sheet and first label, who, and how the printer's job went (made, completed, stopped, aborted, printed by hand) | nothing |
| `labels runs show <run>` | one run (`latest`: the newest): each roast's labels with their `design@version` and what they carried, the printer's job states with times | nothing |
| `labels runs pdf <run> [-o FILE]` | a run's PDF as it was made | nothing |
| `labels runs print <run> [--wait]` | print a run that was made (a token's print waits for the owner's approval, which says the sheet to load and the first label) | inventory |
| `labels runs done <run>` | a run printed by hand from its PDF: mark it printed, so the sheet's next free label moves on | inventory |
| `labels reprint <run> [--start N] [--labels 3,4] [--note …] [-o FILE \| --print [--wait]]` | a run's labels again as they were (each label's data, design version and QR code), all or some (`--labels`, from 1 in the run's order), from the sheet's next free label unless `--start`: a new run | inventory |
| `labels sheets [--set SHEET=N]` | each sheet's next free label (where a run starts; it moves on when a print completes); `--set 5163=1` after loading a new sheet | nothing (inventory for `--set`) |
| `share <roast> [--message …] [--new] [--roaster R]` | a roast's public **share page**, what a bag's QR code opens (`labels --qr share`; [`labels.md`](labels.md), "The share page"): its link (`https://…/R/CODE`), made when it has none; `--message` sets the line for the person it is for (`""` clears it), `--new` makes another link (someone else, their own message). Anyone with the link sees the coffee, origin, roast date and when it is at its best, the notes, the curve and a brew suggestion; never the roast's id, cost or stock | `roast`, `inventory` or `admin` |
| `share list [--all] [--roast R]` | the share links, newest first: the coffee, the address, how often it was opened, the message; `--all` also the revoked ones | nothing |
| `report list` | the kinds of report the server makes ([`reports.md`](reports.md)), each with its data's schema | nothing |
| `report roast <roast> [-o FILE] [--format pdf\|svg\|png\|json] [--page N] [--dpi N] [--paper letter\|a4] [--units C\|F] [--roaster R]` | a roast's report as a PDF written here (`roast-report-batch-5.pdf`, or `-o`): the coffee, eight figures judged against what its level, profile and earlier roasts on that roaster lead to expect (weight and loss, roast time, first crack, development and DTR, drop, colour, profile tracking, cost), the curve with phases and events, heater and fan, phases and events tables, notes and cupping; one page unless it has more. `--format png` one page to look at (`--page`, `--dpi`, 36–200), `svg` every page in one, `json` the data the page is made from (printed; the numbers are also under `numbers`). Paper and units default to the `report_paper` and `report_units` settings (letter, °C); dates are this machine's local dates, or on a machine in UTC the server's `time_zone` setting | nothing |
| `report compare [A] [B] [--roaster R] [--within C] [-o FILE] [--format …] [--paper …] [--units …]` | two roasts compared as an A/B experiment ([`reports.md`](reports.md), "Compare two roasts"), a PDF written here (`compare-12-13.pdf`): both BT and RoR on one chart with heater and fan, the phases on one time scale, first crack, drop, development and loss as A → B, every number `roast compare` reads with A, B and B − A, how closely B's BT followed A's (`--within`, °C, default 0.5), each roast's notes. A then B (ids, `#12`, `latest`); one roast: it is B, against the finished roast before it on its roaster; none: the newest two finished roasts (on `--roaster`), the older as A. `--format json` prints the data (the comparison itself under `comparison`, °C and s) | nothing |
| `report cupping [ROAST …] [--blank [--samples N] [--per-page 2\|6] \| --date YYYY-MM-DD] [-o FILE] [--format …] [--paper …] [--roaster R]` | the cupping sheet ([`reports.md`](reports.md), [`cupping.md`](cupping.md)). `--blank`: a form to score on after the classic SCA protocol for these roasts (ids or `latest`) and `--samples` more of your own (2 when none is named): each sample a three-digit blind code (the same all day), its roast date and rest, the ten attributes (scales to mark, two a page; `--per-page 6`: boxes to write in), defects, total and final, notes; the key (which code is which roast) on a page of its own. Without `--blank`: the roasts' scores as a table, a dot strip per attribute and the notes, the best named; no roast named: the latest cupping session (or `--date`'s) | nothing |
| `report inventory [--count] [-o FILE] [--format …] [--paper …]` | green inventory ([`reports.md`](reports.md)): each coffee in stock with supplier and lot, when bought and how old ("ageing" from 6 months, "old" from 12), crop year, kg, planned, cost per kg, value, the recent pace (g a week over 90 days) and when it runs out at that pace after the planned roasts, soonest first, with totals; `--count` a stock-take sheet (grams in the app, boxes for the count and the difference; enter counts with `bean stock set`) | nothing |
| `report shopping [-o FILE] [--format …] [--paper …]` | the shopping list as one short order per supplier: each bag, when seen, its price, subtotal, the shipping that supplier's earlier orders carried, total; then the coffees running low (below the low mark or out within 30 days), on the list or not | nothing |
| `report coffee <coffee> [-o FILE] [--format …] [--paper …] [--units …]` | a green coffee's report (its name or id): what is known of it, stock and cost, what was bought and roasted, its roasts as a table (each roast's name over its profile) and overlaid on one chart with their RoR (the newest in colour), its cuppings, its ledger with a running balance and the cost per kg after each move | nothing |
| `report profile <profile> [--roaster R] [-o FILE] [--format …] [--paper …] [--units …]` | a profile's report (its name or id): its targets, its roasts in a ranking table with an average row, their BT overlaid on the profile's (the newest in colour), and a dot strip each for first crack, drop, DTR and loss; `--roaster` only that roaster's roasts. A profile never roasted prints its target curve (BT minute by minute, `curve`) and what to expect of a roast of it (`expected`) | nothing |
| `report period [--year Y \| --month YYYY-MM \| --from D --to D] [-o FILE] [--format …] [--paper …]` | a period's report (default this month so far; `--year` is the year in review): roasts, green roasted and out, bought and spent with the landed cost per kg, the stock and its worth at the end; most roasted, profile, best cupped; month by month (bars and a table) for two months or more; each coffee's part; every roast with a sum row | nothing |
| `report purchases [--year Y \| --month YYYY-MM \| --from D --to D] [-o FILE] [--format …] [--paper …]` | the green bought over a period: order by order with its coffees, goods, shipping, fees, total and landed cost per kg, a totals row; each supplier's share; landed cost per kg as a dot per purchase over time | nothing |
| `report trace <roast\|coffee> [--roaster R] [-o FILE] [--format …] [--paper …]` | the traceability report (`roast trace` on paper): the green coffee with its supplier, lot and purchases, each roast (batch, date, roaster, profile, green → roasted, loss) and every move and label run of it by date, what is left | nothing |
| `report plan [--roaster R] [-o FILE] [--format …] [--paper …] [--units …]` | the roast-day plan sheet: the green each coffee needs against stock (short by how much), then each planned roast in order with the green to weigh, the output its level's loss range leads to expect, the profile's charge, first crack and drop targets, how the coffee went last time on that profile (FC, drop, loss, what its notes came to; `last` in the data, `first` when it never was), and boxes to write charge, first crack, drop and the roasted weight in, each with its unit | nothing |
| `share revoke <code>` | stop a link (its code or address): the page is a 404 from now on, like a code that never was | `roast`, `inventory` or `admin` |
| `import <file.alog> [--roaster R] [--name …]` | a roast Artisan recorded, as a finished roast | admin |
| `rm <roast-id> [--wait]` | delete a roast (by id, never `latest`); may be held for approval | admin |
| `beans [--archived] [--low]` | green beans and stock; `--low` only those below their mark | nothing |
| `bean show <bean>` | a bean with its stock ledger (move ids) and roasts; its label name and label notes, or with none a suggestion of tasting words from the supplier's notes | nothing |
| `bean guide <bean> [--profile P] [--roaster R] [--after <roast>]` | a new bean ([`new-bean.md`](new-bean.md)): before its first roast on the roaster, first crack's expected BT and time on the profile with its window, a natural's quiet crack, auto DROP by time, a low-density or wet bean, the supplier's level and espresso advice quoted; after a roast (`--after`, default its newest), while it has fewer than three on the profile, where first crack came, how far to move the drop to keep the profile's development, development or loss outside the level's range in 15 s steps, and "if it tastes…". Defaults: the bean's next planned roast's profile and roaster | nothing |
| `bean add --name … [--origin --variety --process --lot --supplier --notes --moisture --density --stock --low-stock --region --producer --altitude --grade --certifications --score --link --label-notes --short-name]` | a new bean (`--link`: the offer); `--stock` is the first purchase; `--label-notes "chocolate · coconut · apple"` the few tasting words its bag labels print instead of the supplier's notes, `--short-name` the name they print when its own is long ([`labels.md`](labels.md)) | inventory |
| `bean edit <bean> [--name … and the fields of bean add] [--archive \| --unarchive] [--clear lot,notes,label_notes,…]` | edit a bean; absent fields are kept (`--label-notes`, `--short-name`: what its labels print) | inventory |
| `bean import <link> [--add \| --want SIZE [--quantity N] \| --bought SIZE [--shipping]] [--bean B]` | read a supplier's product page ([`green-coffee.md`](green-coffee.md)): a preview with every bag size; `--add` saves the bean, `--want` puts it on the shopping list (`--quantity`: bags of that size), `--bought` records the purchase (`--bean`: buy again for a bean) | inventory |
| `bean stock add <bean> <weight> [--note --price --shipping --fees --currency --supplier --date --crop-year --arrival]` | a purchase (negative: a loss, without the price flags); money in `--currency` (default the server's) | inventory |
| `bean stock set <bean> <weight> [--note]` | a correction: the stock becomes the weight | inventory |
| `bean stock edit <bean> <move-id> [--weight --note --price --shipping --fees --currency --supplier --date --crop-year --arrival]` | fix a purchase or correction in the ledger (`bean show` lists the ids) | inventory |
| `bean stock rm <bean> <move-id> [--wait]` | take a purchase or correction out of the ledger; may be held for approval | inventory |
| `bean rm <bean> [--wait]` | delete a bean (archived if roasts used it); may be held for approval | inventory |
| `order --line BEAN=WEIGHT[@PRICE] … [--supplier --shipping --fees --currency --date --note]` | one order of several coffees: a purchase per line, the shipping and fees split by weight; answers each bean's stock and cost per kg after | inventory |
| `order import <file.json\|-> [--receipt [KIND=]FILE]… [--dry-run]` | a receipt read into an order ([`green-coffee.md`](green-coffee.md), "Orders"; JSON: `supplier`, `order_number`, `placed_at`, `currency`, `subtotal`, `discount`, `shipping`, `tax`, `total`, `status`, `lines[]` with `name`, `sku`, `variant_id`, `bag`, `quantity`, `price` (the line's), `kind` coffee\|other, `taxed`, `bean_id` or `new_bean`): checked (the number once per supplier, lines − discount + shipping + tax = total to the cent unless `accept_mismatch` gives the reason, each line's bean by variant id, coffee code, shopping list, then name), then recorded as ordered with each `--receipt` kept (`shipping=FILE` for a shipping notice). `--dry-run`: the check only, exit 6 when it would be refused. Only received adds stock | inventory |
| `order show <order> [--supplier S]` | one order (by its number, or id): lines, beans, totals, status, receipts | nothing |
| `order shipped <order> [--supplier S] [--carrier --tracking --date]` | it shipped (the shipping notice) | inventory |
| `order received <order> [--supplier S] [--date] [--weight LINE=WEIGHT]… [--bean LINE=BEAN]…` | it came: a purchase per coffee line with its share of discount, shipping (by weight) and tax; `--weight 2=440g` a line that came different | inventory |
| `order cancel <order> [--supplier S]` | it will not come; its shopping-list entries are wanted again | inventory |
| `order receipt <order> [--supplier S] [<n> [--out FILE]] [--add FILE [--kind K]] [--rm <n>]` | an order's receipts: list; save one (private: inventory or admin, never read-only); keep another; remove one | inventory |
| `order rm <order> [--supplier S] [--wait]` | remove an order and its receipts (a received one's purchases too); may be held for approval | inventory |
| `orders [--all \| --status S]` | supplier orders: open (ordered, shipped) or all, newest first | nothing |
| `wanted [--all]` | the shopping list, on order marked (`--all`: also what was bought) | nothing |
| `wanted bought <id> [--weight --price --shipping --date]` | it arrived: the bean (new from the link, or the entry's) and its purchase | inventory |
| `wanted rm <id>` | off the list | inventory |
| `wanted qty <id> <N>` | how many bags of the entry's size (1 to 100) | inventory |
| `wanted cart [--supplier S] [--id ID]… [--offline]` | per supplier one link that opens a filled cart in your browser ([`green-coffee.md`](green-coffee.md), "Cart links"): each bag's price and availability read again first, 3 s apart (`--offline`: as the list saw them, no request); the store's UCP `/buy/<variant>:<qty>` link, its `/cart/` link, or Burman's `/checkout-link/` in pounds; sold-out bags left out; a Shopify link replaces the cart in that browser. Never a checkout | inventory |
| `costs [--months N]` | by month: spent, bought, roasts, green and roasted kg, what the roasts' coffee cost ([`green-coffee.md`](green-coffee.md)) | nothing |
| `roasted [--for espresso\|filter] [--ready \| --resting] [--all]` | roasted coffee ([`roasted.md`](roasted.md)): per purpose what is ready, resting (next ready when), past its best, how fast it goes and when to roast again; each roast's coffee left and its rest window, ready first | nothing |
| `roasted show <roast>` | one roast's coffee: left, rest window, every move with its id | nothing |
| `roasted use <roast> <amount> [--note --at --roaster]` | coffee used: `36g`, `2 shots` (doses by the roast's purpose), `1 bag`, `all` (finished). A roast is an id, `#N`, `latest` or its name | inventory |
| `roasted give <roast> <amount> [--to WHO --note --at]` | coffee given away or sold | inventory |
| `roasted toss <roast> [amount] [--note]` | coffee thrown away (default all) | inventory |
| `roasted count <roast> <grams> [--note]` | weighed what is left: it becomes the stock | inventory |
| `roasted undo <roast> [--move ID]` | take back the last move (or one by id) | inventory |
| `roasted combine <roast> <roast>… [--name N --note T --roaster R]` | roasts that went into one bag or jar: one combined lot, of one coffee or, from different coffees, a blend (`blend`, and `coffees` with each one's `share_pct`). From then on a move on any of them (use, give, pack, toss, count, undo) is split across them by what each has left; the bag is ready when its newest roast is; a label for any of them prints the bag (`#16+17`, the days roasted; a blend's origins with their shares, "60 % Ethiopia · 40 % Kenya"). A roast already in a bag: the others join it; with only that roast it shows the bag | inventory |
| `roasted uncombine <roast>` | split the bag a roast is in apart: each keeps its share of what was taken | inventory |
| `roasted pack <roast> <bags> [--ground GRIND --note --at --roaster]` | bags packed from it: `3x250g` (or `3`, the bag size); whole bean, or `--ground espresso` (filter, moka, "French press", your own word). The grams come off like use but are not counted as use; a ground bag's best-until is the earlier of the lot's and the pack date plus `best_until_days_ground` (14) | inventory |
| `trace <roast\|coffee> [--roaster R]` | traceability: a roast (`#12` off a bag, `latest`, its id) back to its green coffee (supplier, lot, origin, process, purchases) and where its coffee went (packed bags with form and grind, label runs, gifts with the recipient, used, discarded, left); or a coffee (name or id) to every roast of it. No prices | nothing |
| `plan [--roaster R] [--done]` | the queue of planned roasts | nothing |
| `plan add [--bean --weight --profile --roaster --name --notes]` | a planned roast at the end of the queue | inventory |
| `plan edit <id> [--bean --weight --profile --roaster R\|any --name --notes]` | change a planned roast | inventory |
| `plan move <id> --to N` | move it in the queue (`--to 1`: next) | inventory |
| `plan rm <id> [--wait]` | take it off the plan | inventory |
| `profiles` | roast profiles, seeds included (no descriptions), each with the roaster kind it is for (`roaster_kind`; none fits any) | nothing |
| `profile show <profile>` | one profile: description, targets, expected ranges, curve | nothing |
| `profile history <profile> [--limit N]` | its finished roasts, newest first: FC, drop, development, weight loss, colour, fit to the curve | nothing |
| `profile edit <profile> [--name --description --level --purpose --target-fc M:SS --target-total M:SS --target-drop-temp C --charge-temp --preheat-temp --batch --beans --auto-drop time\|bt\|development\|off --expect KEY=LOW-HIGH --kind KIND]` | change its details and targets; the curve and events stay. `--kind`: the roaster kind it is written for, a setup kind (`fluid-bed-electric`, `drum-electric`, `drum-gas`, `logger`, `other`) or `any` (none); the library lists the selected roaster's kind first and marks the others ([`profiles.md`](profiles.md), "Roaster kinds"). `--expect` (repeat it) sets the profile's own expected range over its level's default ([`roast-guidance.md`](roast-guidance.md)): `development=1:45-2:45` (m:ss or s), `loss=14-16`, `dtr=18-24`, `color=70-80`; `KEY=off` goes back to the level's; the analysis, `roast review`, the bean guide and reports judge by it. `--auto-drop`: Profile drive marks DROP by itself at the profile's drop time, its drop temperature (from first crack on) or its development time after first crack is marked, after a countdown the Roast page can cancel ([`replay.md`](replay.md), "Auto DROP"); refused when the profile lacks that target. `profile show` says which | admin |
| `profile save <roast> [--name --description] [--roaster R]` | a finished roast as a new profile | admin |
| `profile import <file> [--name]` | an Artisan `.alog` (or a Roastwarden profile export) as a profile | admin |
| `profile export <profile> [--artisan] [-o FILE]` | the profile as Roastwarden JSON, or `.alog` with `--artisan` | nothing |
| `profile share <profile> [--new]` | a public link for the profile (`https://…/P/CODE`, [`profiles.md`](profiles.md), "Sharing a profile"): its page shows the curve, level and purpose, targets and the events it replays, and offers it as `profile.json` (Roastwarden) and `profile.alog` (Artisan); never its id, who made it or its roasts. The profile's newest link, made when it has none; `--new` makes another | admin |
| `profile shares [--profile P] [--all]` | the profiles' share links, newest first: the address, how often it was opened and downloaded; `--all` also the revoked ones | nothing |
| `profile unshare <code>` | stop a profile's link (its code or address): its page and files are a 404 from now on | admin |
| `profile import-link <link> [--name] [--preview]` | import a profile from its share link: another server's `https://host/P/CODE` (the server reads it: public addresses only, no redirects, 10 s, 2 MB) or a code of this server's; its description ends with where it came from. `--preview` reads it and saves nothing | admin |
| `profile rm <profile> [--wait]` | delete a profile; may be held for approval | admin |
| `roasters` | the roasters: set up, heard on MQTT, online, firmware, kind and control level (`off` logs only, `manual`, `full`; [`roaster-setup.md`](roaster-setup.md)); any waiting to be set up | nothing |
| `roaster show <roaster>` | its connections and transports (a serial TC4 one with its `CHAN` mapping, reply format and fan output; under a connection with its own field names, units, topics or command templates a `mapping …` line: `°F; bt ← MAX31855.Temperature, …; telemetry tele/roaster/SENSOR; templates heater drum`, [`ingest-mapping.md`](ingest-mapping.md)), its setup (kind, control level, outputs with drum and damper when declared, and fan floor, declared channels: [`channels.md`](channels.md)), what CHARGE and DROP do, fan assist, whether tokens may control it | nothing |
| `roaster contract <roaster>` | the roaster against the device contract (`control.md`): its firmware, whether its latest sample has BT, ET, heater and fan, its caps (reported or inferred), and its PID gains against the ones the server holds (`matches`), with the last check after a boot (`last_boot`: what it came up with, what the server sent); only reads | nothing |
| `roaster add <id> --transport mqtt\|websocket\|modbus_tcp\|modbus_rtu\|serial_tc4\|phidget22 [--name …] [--kind fluid_bed_electric\|drum_electric\|drum_gas\|logger\|other] [--topic-prefix … --telemetry-topic …] [--device-url ws://…] [--host … --port … --unit-id …] [--port … --baud … --parity … --framing rtu\|ascii --data-bits 7\|8] [--host … --port … --password … --serial-number … --hub-port … --thermocouple J\|K\|E\|T] [--preset stock-aartisan\|tasmota\|esphome\|artisan-websocket]` | add a roaster ([`add-roaster.md`](add-roaster.md)): the roaster (active), its connection (the server's config checks apply) and, with `--kind`, its setup at **control off**; a preset fills the connection's defaults (stock aArtisan: active replies, `CHAN;1200`; Tasmota: `tele/<id>/SENSOR` and a starting mapping; ESPHome: per-sensor topics; Artisan WebSocket: the `getData` poll, `data.BT`/`data.ET`). `--framing ascii` makes a Modbus RTU line Modbus ASCII (7 data bits unless `--data-bits 8`: Delta DTA/DTK controllers). `phidget22`: a Phidgets temperature board through Phidgets' network server ([`phidgets.md`](phidgets.md)): `--host` the computer running it, the board plugged in (port 5661), `--password` its password (saved, shown as `********`), `--serial-number` and `--hub-port` to pick a board, `--thermocouple` the type (K); BT input ch0, ET ch1 until `roaster map --field bt=ch2`. It only reads. A roaster the server found streaming on MQTT (pending, `roasters`: waiting to be set up) is set up in place under its own id, its connection changed rather than doubled; one added already exits 6 (`CONFLICT`). It never raises control (`roaster control` asks the owner). Next: `roaster map --try`, `roaster map`, `roaster check`. Programming your own roaster: [`connect-your-roaster.md`](connect-your-roaster.md). A machine in Artisan's catalogue: `--artisan BRAND/MODEL` or `--aset FILE` instead of `--transport` (the row after `roaster machines`) | admin |
| `roaster control <roaster> off\|manual\|full [--wait]` | its control level ([`roaster-setup.md`](roaster-setup.md)), the rest of the setup kept. `off` runs at once; `manual` or `full` waits for the owner's approval (exit 3 with the request; `--wait` waits for the answer), for every token: switching control on is the owner's yes | admin |
| `roaster map <roaster> [--connection ID] [--field KEY=PATH …] [--units C\|F] [--topic KEY=TOPIC …] [--column a,b,…] [--register KIND:ADDRESS[@SLAVE]:TYPE[*SCALE]:ROLE[:UNIT] …] [--command KEY=PAYLOAD …] [--command-topic KEY=TOPIC …] [--try]` | the connection's mapping ([`ingest-mapping.md`](ingest-mapping.md)): fields given replace theirs (`KEY=` takes one out), the rest stay; `--column` a TC4's reply by position (an extra column's own unit after a colon, `exhaust:F`, `:C` or `:none`; without one a column naming a declared °C channel follows `--units`); `--register` a Modbus register with a role, `*SCALE` when the value is the raw times it (`holding:0:int16*0.1:bt:°C`, tenths), `@SLAVE` when it is on another controller of the bus than the connection's (a register at the same kind, address and slave is replaced). `--try` writes nothing: the newest message the roaster sent (`/raw`) and what the mapping makes of each field, in °C, or why not (exit 1 when it makes no sample); without it the mapping is saved | admin (`--try`: nothing) |
| `roaster firmware [<id> [--kind starter\|fluid-bed] [--address …] [--port …] [--wifi NAME [--wifi-password-stdin]] [--password-stdin] [--board max31855\|max6675] [--no-et] [-o FILE]]` | firmware for an ESP32 roaster the owner builds ([`esp32-starter.md`](esp32-starter.md), [`esp32-fluid-bed.md`](esp32-fluid-bed.md)). With no id: the kinds, the MQTT port (the built-in broker's, else 1883) and the ids the server has a password for (`MQTT_DEVICES` and `roaster password`; the download fills theirs in; never the passwords). With an id: the sketch (`starter` reads BT and ET; `fluid-bed` runs an electric fluid bed: heater SSR, fan, its own PID and safety) with the id, its MQTT password (the server's, or `--password-stdin`), the server's address (`--address`, default the host `roast` calls; localhost is refused) and the Wi-Fi filled in, as a PlatformIO project in a zip that the Arduino IDE opens too; answers `file`, `bytes` and `missing` (`password`, `wifi`: left for the owner to fill in). Nothing is stored; the passwords are masked in the audit log | admin |
| `roaster password list` | who may sign in to the built-in MQTT broker as a roaster ([`add-roaster.md`](add-roaster.md), "MQTT passwords"): each id with its `source` (`env`: `MQTT_DEVICES` in `.env`; `dashboard`: added here), `created_at`, `signed_in` (connected now) and `added` (set up under Roasters); `built_in_broker` false when the server uses a broker of its own (none can be added then). Never a password | admin |
| `roaster password add <id> [--password-stdin]` | a new roaster's MQTT password: a random one (32 characters) or one from stdin (8 to 128, no spaces, quotes or backslashes); it may sign in at once, no restart. Answers `roaster_id` and `password`, the only time the password is shown (`roaster firmware <id>` fills it in later). An id with one already, or in `MQTT_DEVICES`, exits 6 (`CONFLICT`), as does a server on a broker of its own | admin |
| `roaster password new <id> [--password-stdin]` | a new password for one added here; connected with the old one, it is signed out until its sketch has the new one. Refused (6, `CONFLICT`) while the roaster is busy (roasting, driving, tuning, heating or cooling) and for one in `MQTT_DEVICES` (changed in `.env`, with a restart) | admin |
| `roaster password remove <id>` | one added here can no longer sign in; it is signed out now. Its roasts and setup stay. Refused while it is busy; 4 when it has no password here | admin |
| `roaster machines [QUERY] [--all]` | Artisan's machine catalogue ([`artisan-machines.md`](artisan-machines.md)): the machines Roastwarden can set up from Artisan's own setup files, `Brand/Model`, name and the Artisan device; `--all` every one, with why not (S7, Hottop, …). QUERY's words all match brand, model or device. Exit 4 when none match | nothing |
| `roaster add <id> --artisan BRAND/MODEL \| --aset FILE [--port … \| --host … \| --device-url … \| --telemetry-topic …] [--unit-id …] [--baud …] [--parity …] [--framing …] [--kind …] [--name …] [--dry-run]` | a roaster planned from an Artisan machine setup (`POST /api/artisan/machines/plan`, [`artisan-machines.md`](artisan-machines.md)): the roaster (active, named as the machine), its connection and its setup at **control off** with the sliders' outputs and the extra curves as channels. A Modbus machine (RTU, ASCII, TCP, Fuji PIDs) gets its register map (roles, each register's own slave, BCD); a TC4, WebSocket or MQTT machine its ingest mapping, poll and command templates; a Phidgets machine (a 1048, TMP1101, 1051, TMP1200) a `phidget22` connection with its inputs mapped, `--host` the computer running Phidgets' network server and `--password` its password. The plan's notes say what was guessed or left out. `--port` (the serial port), `--host` (the roaster's address, Modbus TCP or a WebSocket; `--device-url` the whole URL) or `--telemetry-topic` (MQTT: the topic its data reaches this server's broker on) when the file cannot know it (exit 2 naming it); a machine on a device Roastwarden cannot read exits 1 with why; `--dry-run` prints the plan and makes nothing | admin (`--dry-run`: nothing) |
| `roaster check <roaster> [--write]` | the roaster against the interface standard ([`add-roaster.md`](add-roaster.md)): rate (0.5–4 Hz over 30 s, with jitter), BT and ET, units (room temperature at idle; 50–115 with the heater off: °F?), each declared output and channel in its telemetry, caps, control level; each line ok, warn or fail, and its newest `reading`; exit 0 ok, 1 warn or fail. `--write` (the owner key's alone; a cold roaster with control manual or full, no roast open): heater 10 %, its echo, then emergency stop, always, and the heater reading 0 | nothing (`--write`: the owner key) |
| `roaster model <roaster> [--fit]` | its thermal model, fitted from its own roasts ([`prediction.md`](prediction.md)): when and on how many roasts, BT two minutes ahead with each roast left out of the fit (RMS and 90 % bound: heater and fan as they were, the PID following the plan, heater and fan held, and the scope's old projection), where BT settles at a few heater and fan values (the headroom), and how the live prediction did on the recent roasts; `--fit` refits it now (the server refits by itself after a roast) | roast for `--fit` |
| `roaster tuning <roaster> [--run ID [--tau-c N] [--rule R] [--samples]]` | its PID gains over time, the gain schedule with its `advice` (`autotune.md`, "Checking a schedule"), autotune runs (the server's, and older firmware runs) and the server's test running (`test`); `--run` one run (a server run in words: model, checks, every PI design, `autotune-2.md`); `--tau-c`/`--rule` preview a design on its model (SIMC τc 1 tight … 3 smooth; `amigo_pi`, `zn_pi`, `tyreus_luyben_pi`); `--samples` its record | nothing |
| `live [--roaster R]` | what each roaster reads now (BT, ET, RoR, setpoint, heater, fan), its running roast, the prediction from its own model while a roast runs (BT in 1 and 2 min with the band, first crack and DROP with their ranges, and `reason`: the plan it predicts under; the RoR watch: RoR now and its trend, the model's RoR in 1 and 2 min, and a warning before a crash, a flick after first crack or a stall, each with its reason, from DRY end; [`prediction.md`](prediction.md), "RoR warnings"), and Profile drive (its profile time; a drive the roaster's heater cut or a server restart stopped, with `stopped_reason` and the `resume` call: `roast api POST /api/sessions/<id>/drive/resume`; the drive's `auto_drop`: what it goes by, `state` armed / counting / due / cancelled, `eta_seconds`, what it waits for, and the `cancel` call, for virtual_roaster only); only reads | nothing |
| `notifications [--test CHANNEL]` | the channels and how each one's last send went, the phones signed up, open problems; `--test` sends one (`push`, `gotify`, `ntfy`, `matrix`, `webhook`) | admin for `--test` |
| `home-assistant [--on \| --off] [--broker mqtt://host[:port] \| server] [--username U] [--password-stdin] [--prefix P] [--virtual true\|false] [--interval S]` | Home Assistant discovery ([`home-assistant.md`](home-assistant.md)): on or off, the broker (`server`: the server's own, the one the roasters use; or Home Assistant's own), whether it is connected, the roasters announced and their entities, any heard but waiting to be set up (`roaster add`), and why nothing is published. The options change it; the password comes from stdin only. Read-only in Home Assistant: no entity sends a command | admin to change |
| `support-bundle [-o FILE]` | a support bundle for a bug report ([`support.md`](support.md)): versions, the environment's names, the roasters' setups, contracts and newest readings, the newest roast with its telemetry, settings, problems and the recent log, in one JSON file, every password, key and token removed, no green coffee, orders or receipts; written to the server's file name here (`-o -`: stdout); the answer names the file and its size | admin |
| `modules [--preset P] [--on M…] [--off M…]` | the dashboard's optional parts that are on (`reports`, `cupping`, `tuning`, `agents`, `costs`, `labels`), the preset they match (`home` none, `enthusiast` reports + cupping + tuning + agents, `roastery` all six, else `custom`) and `first_run`; `--preset` starts from a preset, `--on`/`--off` change only the modules named. An off module leaves the dashboard, not the API ([`lean-home.md`](lean-home.md)) | admin to change |
| `settings [KEY]` | every setting, or one (a secret shows only whether it is set; a setting the server reads that has no value is `unset`, `is_set: false`; a key it does not read with nothing stored exits 4, so a typo shows; JSON values such as `roast_alarms` as JSON) | nothing |
| `settings set KEY=VALUE …` | write settings, all or none; `KEY=-` reads the value from stdin. `time_zone=America/Chicago` (an IANA name; empty is UTC) dates labels, share pages, roasted coffee and reports for a `roast` on a machine in UTC (an agent's box, a server), which sends no offset of its own; `public_url=https://roast.example.com` is where share links, label QR codes and approval links start (unset: the address the call came to, `http://localhost:8080` for a `roast` on the server) | admin |
| `audit [--limit N] [--roaster R] [--kind K,…] [--find TEXT] [--by TOKEN_NAME] [--failed]` | the audit log, newest first, filtered on the server (kinds and search: [`control.md`](control.md), "Audit log"); a request's body as JSON | admin |
| `backups [--now \| --check NAME]` | the copies, the last attempt, the next run; `--now` writes one, `--check` checks one without touching anything | admin for `--now`, `--check` |
| `approval <id> [--wait]` | one approval request; `--wait` waits for the owner's answer | nothing |
| `openapi [--offline] [--find <word>]` | the server's OpenAPI document; `--find` lists only the matching operations (method, path, id, summary, body type, and the `roast` command that does it) | nothing |
| `api <METHOD> <PATH> [--body JSON\|-] [--wait]` | any API call with the token and the error handling | depends |
| `backup [--db PATH]` | **local**: the nightly copy, now, next to the database | the file |
| `restore <file> [--check] [--db PATH]` | **local**: check a copy, and without `--check` stage it for the next start | the file |

Every API operation has its way from the command line: one of these commands, or `roast api`
when there is no command (live control, tuning and parts of roaster setup, which you do at the
roaster), or none (the owner's alone, or a browser's). `roast openapi --find <word>` shows it per
operation, with the preset each command needs.

A bean, profile or roaster is named by its id or by its name (ignoring case). A name that
matches nothing is `NOT_FOUND` with the names there are in `error.details.known`; two beans
with the same name are `AMBIGUOUS` (use the id).

**Units.** Temperatures are °C, weights grams. A weight argument also reads `5kg`, `2500g`,
`2,5kg`, `2lb` and `12oz`, and is stored in grams to 0.1 g.

**Local commands** run where the database is, without the server or a token:
`docker exec roastwarden roast backup` writes the same copy as the nightly run (recorded as
`cli` in the run history, so Settings → Backups shows it), and
`docker exec roastwarden roast restore <name>` checks a copy from the backup folder and stages
it; `docker restart roastwarden` then puts it in place (`docs/DEPLOY.md`, "Backups").
`--db` defaults to `ROASTWARDEN_DB_PATH`, which the image sets.

## Approvals

A token that asks first (the default for new tokens) has some calls held for the owner:
starting a roast on a physical roaster and deletes ([`control.md`](control.md), "Approvals"). `roast`
then prints the held request (`{"approval": {id, status, summary, url, …}, "message": …,
"wait_url": …}`) and exits **3**: a human must act. With `--wait` (`bean rm`, `api`) it waits
for the answer instead (up to `--wait-timeout`, default an hour) and prints the decided
request: exit 0 when it was approved and done, 1 when it was denied, expired or the call
failed. `roast approval <id> --wait` waits on one later. An agent tells the owner what it
asked for and where to approve it; it never looks for another way.

## Output

Lists are for finding: `roasts`, `beans`, `plan`, `profiles` and `audit`
leave out timestamps, descriptions and the browser's user agent; the `show` of one item has
them. Filter with the flags (`--low`, `--roaster`, `--limit`), not with jq.


On a terminal: short text. On a pipe (an agent, a script), or with `--json`: one compact JSON
line, the server's own answer with every `null` left out (an absent key means `null`).
`--text` forces text on a pipe.

Errors are one JSON document on stdout (on a pipe) or two lines on stderr (on a terminal):

```json
{"error":{"code":"TOKEN_SCOPE","message":"…","hint":"This token's preset does not cover that; …","status":403}}
```

`code` is the API's ([`control.md`](control.md), "Errors") or roast's own: `UNREACHABLE`,
`AMBIGUOUS`, `BAD_RESPONSE` (the server answered something this `roast` does not read: two
different versions), `NOT_READY`, `NO_FILE`, `FAILED`. `hint` says what to do next.

## Exit codes

| Code | Meaning | Error codes |
|---|---|---|
| 0 | done | |
| 1 | it ran, and the answer is a failure | `FAILED`, `NOT_READY` |
| 2 | bad arguments | `USAGE` (and clap's own) |
| 3 | a human must act: an approval is waiting | `APPROVAL_PENDING` |
| 4 | not found | `NOT_FOUND`, `NO_FILE`, `LINK_NOT_ACTIVE` (a share link revoked or never made) |
| 5 | not allowed | `UNAUTHORIZED`, `TOKEN_*`, `TOO_MANY_FAILURES`, `OWNER_ONLY`, `REAL_HARDWARE`, `CONTROL_OFF`, `OUTPUT_NOT_DECLARED` |
| 6 | invalid, or conflicts with the current state | `BAD_REQUEST`, `CONFLICT`, `UNPROCESSABLE`, `ALREADY_MARKED` (a second CHARGE or DROP), a copy that cannot be restored |
| 7 | Roastwarden unreachable, or failed itself | `UNREACHABLE`, `INTERNAL_ERROR`, `UNAVAILABLE`, `BAD_RESPONSE` |

`TOO_MANY_FAILURES` (429) means ten wrong keys or tokens came from this address within a
minute: wait the seconds the message names and fix `ROASTWARDEN_TOKEN`; retrying in a loop
keeps it blocked. `REAL_HARDWARE` is a refusal on purpose: the roaster has a heater, and only the owner allows
tokens to control it (`SECURITY.md`). An agent asks the owner; it does not look for another
way. `CONTROL_OFF` is the same kind of answer for everyone, the owner key included: the
roaster's control level does not allow the command (`off` logs only; `manual` takes heater,
fan, heater on/off and STOP; [`roaster-setup.md`](roaster-setup.md)). `OUTPUT_NOT_DECLARED`
likewise: the roaster has no such output (a logger has none). The level goes up only with the
owner's yes: `roaster control` asks for it, and the owner approves or denies. Declared
channels ([`channels.md`](channels.md)) are in the setup; `roaster show` lists them, and each
roast keeps the list it started with (`roast review` reads that list).

<!-- site: end -->

## Design notes

For developers: how `roast` is built, where its pieces live, and how agents on a host of their
own use it. The site publishes only the part above.

### Where it comes from

`roast` uses and administers Roastwarden from a shell, for people and for agents (Hermes,
Claude Code), with an API token instead of SSH. Its output rules and exit codes are made for
agents: one JSON line on a pipe, text on a terminal, exit codes that say what to do next.

It is a second binary of the server package (`crates/server/src/bin/roast/`), built with the
server and shipped in the image (`/usr/local/bin/roast`), where it is also the healthcheck.
`main.rs` is `main()` and a dispatch of one line per command; `cli.rs` the commands as clap
reads them (its doc comments are `roast --help`); `commands/` one file per command family,
each family's own arguments beside its code (`labels/` a folder of its subcommands); `text.rs`
the weights and times read and written and the text views; `api.rs` the client and the lookups
by name; `out.rs` the JSON or text answer and the exit codes. The server is a library too
(`crates/server/src/lib.rs`): `roast` uses its backup and OpenAPI code, and the integration
tests (`crates/server/tests/roast_cli/`, one file per family, `common.rs` running the binary
against a served server) run the real binary against the whole server on a temporary database.

For agents there is a skill, `skills/roastwarden/SKILL.md` (Hermes, Claude Code): setup, the
exit codes, recording a purchase, and the rules (never control the real roaster; report an
approval, never work around it).

### Notes on the sections above

- Setup, on the owner key: `roast` never reads `ROASTWARDEN_API_KEY` by itself, so an agent
  inside the container cannot act as the owner by accident (an agent under test once did,
  through `docker exec`, to get past its own token's limits).
- Commands: Every API operation has its way for agents in `crates/server/src/bin/roast/surface.rs`: one of
  these commands, or `roast api` with the reason there is no command (live control, tuning and
  roaster setup, which a person does at the roaster), or none (the owner's, a browser's, a test
  hook). `roast openapi --find <word>` shows it per operation. Tests fail when an operation has
  no entry, an entry names a command that is gone, a command is missing here or in the skill, or
  a token with the preset named here could not make the call (`docs/agents.md`, "The check").
- Output: lists are for finding; `show` is for reading.
- Commands, `roaster control`: the row said "exit 3 `NEEDS_HUMAN` with the request";
  `NEEDS_HUMAN` is exit 3's name in `out.rs`, the error code on the wire is `APPROVAL_PENDING`.

## Agents

On an agent's machine: `roast login <server>` once, or the token in a file only that user reads
(mode 600) and the binary at a known path, named in a "This deployment" paragraph beside the
installed skill; the agent exports `ROASTWARDEN_URL` and `ROASTWARDEN_TOKEN` in its subprocess
(Hermes's terminal does not pass the caller's environment). Behind a proxy that signs people
in, a request carrying an `rr_` token must reach the API without the browser login.

`roast skill install claude-code` writes the skill under `~/.claude/skills/`, `hermes` under
`~/.hermes/skills/`, `agents` under `~/.agents/skills/` (or `--dir`); run it again after an
upgrade so the copy matches the binary.
