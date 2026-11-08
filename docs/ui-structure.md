# Dashboard structure

The dashboard's information architecture: which section holds what, and why it is shaped so.
Every capability sits where it is used and explains itself in place, so a roast never means
bouncing between pages, and every per-roaster setting says which roaster it belongs to. The
history of how it got here (the audit of the first dashboard, the overhaul's phases) is a
development note, not this document.

## Structure

Eight sections, named for what you do there:

| Nav | URL | Purpose |
|---|---|---|
| **Roast** | `/` | The cockpit: everything needed before, during and right after a roast, and nothing else |
| **History** | `/history`, `/history/[id]`, `/history/[id]/labels`, `/history/compare` | Finished roasts: ranking table, detail, bag labels, comparator (a roast's **Report** opens it on Reports; `/history/[id]/report` redirects there) |
| **Profiles** | `/profiles`, `/profiles/[id]`, `/profiles/new`, `/profiles/[id]/edit` | Roast profiles: library (the selected roaster's kind first, others marked "for a drum"), profile page (curves, phases, events), designer, transposer, Artisan import and export, Share (a public link) and Import from a link |
| **Beans** | `/beans`, `/beans/[id]`, `/beans/orders`, `/beans/orders/[id]` | Roasted coffee (resting and ready, `roasted.md`; roasts in one bag as one row; a blend of coffees with its shares), what is on order (when anything is), the planned roasts queue and green stock (the header's four buttons open their form in one place under them); one bean's ledger and roasts (`batches.md`); supplier orders from their receipts and one order's lines, totals, shipped and received, receipts (`green-coffee.md`, "Orders") |
| **Labels** | `/labels`, `/labels/designs/[id]` (`new`), `/labels/print`, `/labels/history`, `/labels/printers`, `/labels/coffees`, `/labels/assets`, `/labels/brands` | Label Studio: the design gallery, a design's editor with the live check and versions, print runs and their history, printers and their calibration, each coffee's label, artwork and fonts, brand kits (`labels.md`) |
| **Reports** | `/reports` (`?kind=roast&roast=<id>`) | PDF reports the server makes with Typst: the kinds as underlined tabs, what each is about, paper and units, the server's own pages as the preview, Download PDF and Open to print (`reports.md`) |
| **Roasters** | `/roasters`, `/roasters/[id]`, `/roasters/[id]/check`, `/roasters/[id]/control`, `/roasters/[id]/autotune`, `/roasters/[id]/simulator`, `/roasters/new`, `/roasters/templates`, `/roasters/firmware`, `/roasters/passwords` | Each roaster as a card (reading, kind, connection, control, check) and the ones found but not added; each roaster's page with its connection (and its mapping), its check against the interface standard *and* its control tuning; the simulators (the fluid bed and the drum); connection templates; ESP32 firmware (`/roasters/firmware`, linked beside Add roaster and from its first step): the starter or the fluid-bed controller with this server's settings filled in, a zip for PlatformIO or the Arduino IDE (`esp32-fluid-bed.md`); MQTT passwords (`/roasters/passwords`, linked beside ESP32 firmware, from the firmware page and from Settings → About): who may sign in to the built-in broker, a new roaster's password added (random, shown once) or changed or removed at once, those in `MQTT_DEVICES` read only (`add-roaster.md`, "MQTT passwords"). Add roaster (`/roasters/new`) is four steps, Roaster → Connection → Check → Control (below); its first can start from an Artisan machine (a catalogue search or an `.aset` file: `artisan-machines.md`) that fills its steps (Modbus, TC4, WebSocket, MQTT or a Phidgets board, a mapping and templates riding along with the connection); the RTU forms offer Modbus ASCII framing; the register map editor has a Slave column for a bus of controllers, BCD data types and a Scale column (0.1 for tenths) |
| **Settings** | `/settings` (this browser), `/settings/roasting`, `/settings/alarms`, `/settings/notifications`, `/settings/home-assistant`, `/settings/backups`, `/settings/modules`, `/settings/tokens`, `/settings/approvals`, `/settings/audit`, `/settings/about` | App-wide only, one page per group (`/settings/labels` redirects to `/labels`) |
| (first run) | `/welcome` | Shown once, instead of the Roast page, on a new install: a module preset, the temperature and weight units and the time zone, then Roasters → Add (`lean-home.md`, "First run") |

**Modules** (`lean-home.md`): Labels and Reports are optional sections, and
cupping, tuning, costs and agents optional parts of other pages. The list that is on comes
from `GET /api/about` (`lib/stores/modules.svelte.ts`; the nav and Settings menus are
`NAV` and `SETTINGS` in `lib/modules/modules.ts`, each entry with its module). An off module
leaves the nav, the Settings menu and the buttons, cards and fields that lead to it (a roast's
Report, Labels…, cupping and share page, its coffee cost; Beans' prices, cost per kg,
spending, "Record an order" and the shopping list; a roaster's Autotune tab, gain schedule,
fan assist switch and panel, and token access with agents). A URL of an off module's page
(`moduleOfPath`) shows one line, "Labels are off: Settings → Modules", not a 404. Home (none
on): Roast, History, Profiles, Beans, Roasters, Settings.

`/sessions/...` and `/devices/...` (including `?tab=`), the sections' former URLs, redirect to
their new places (`lib/routes.ts`), so old bookmarks work.


### Global header (every page)

- Brand, the five sections, connection dot.
- **Roaster picker**: the selected roaster is global (stored per browser), not
  a dashboard widget. Discovered (pending) roasters are listed too, so a
  freshly enabled virtual roaster is pickable.
- **Live roast chip** when a roast is running and you are elsewhere:
  "Roasting 6:12 · Light espresso", one tap back to the cockpit.
- **Demo** beside the mark on a public demo only (`ROASTWARDEN_DEMO=1`,
  `lib/components/app/DemoChip.svelte`): a small popover says what the demo is (the visitor's own
  simulated roaster, shared records, a reset every few hours) with "Install your own", "Send
  feedback" (a mail naming the page, `lib/demo/demo.ts`), the discussions and the running
  release's source (About's `source_url`). It sits in the bar, not in a banner,
  so the Roast page keeps its height. On a demo the visitor's own simulated roaster is selected
  at start (`lib/stores/demo.svelte.ts`), and a demo that counts visits loads Umami's tracker
  there; no other install does.
- **STOP** (emergency stop) on every page while there is something to stop
  (`lib/roaster/stop.ts`): a roast open, the selected roaster's heater on or drum turning, the
  server cooling after DROP, or BT or ET at or above the roast sequence's `cool_end_bt_c` (50 °C
  when it has none). Once the heater is off, nothing is open and both are below it but the fan
  still runs (after an emergency stop it runs at 100 %), the same spot shows **Fan off** (plain,
  not red; fan 0 %). Nothing on a cold idle roaster (a cleaner bar), when the
  roaster has been silent for 15 s, or when its control is off. The tooltip says why.
- Unit (°C/°F) and theme toggles.
- Phone: brand, STOP and a menu button; the menu holds sections and toggles.

### Roast cockpit (`/`)

Three zones, in the order your eyes need them:

1. **Readouts**: BT, ET, RoR (live / profile) and Δ profile in large type;
   a second row with time since CHARGE, setpoint, heater, fan and
   development (DTR). Phone: 2 × 2 big tiles, the second row scrolls.
2. **Scope**: unchanged features (projections, phase bands, development
   guide, tracking RMS), now with the full profile drawn. A roaster's declared channels
   (`channels.md`) are dashed lines with a legend toggle each: °C on the temperature axis,
   % on the output axis, another unit on a right-hand axis of its own. Legend entries only,
   never a panel, so the page still fits.
3. **Action rail** (right column on a laptop, below the scope on a phone):
   - **Next step** card, one primary button per stage:

     | Stage | Shown when | Primary | Secondary |
     |---|---|---|---|
     | Set up | no session | **Start roast** (profile picker, batch details folded) | — |
     | Waiting for charge | session, no CHARGE | **Charge** (virtual roaster: also loads the simulated beans) | Pause, Complete |
     | Roasting | CHARGE, no DROP | the next landmark (DRY → FC → …), **Drop** always one tap | every landmark, Pause |
     | Dropped | DROP marked | **Complete roast** | Cool end |
     | Done | just completed | **Review roast** (session page) | Start another |

     Landmark keys 1-8 keep working. Drop on the virtual roaster also dumps
     the beans. With a profile's auto DROP (`replay.md`) one line under the primary button
     says when it drops, then counts down in violet with Cancel (or "cancelled", Turn back
     on); the line keeps one height whatever its state, and below 820 px tall the landmark
     buttons shorten and their turning-point note hides while it shows, so the rail does not
     grow.
   - **Profile drive** (replay), folded to one status line when running.
   - **Roaster control**: mode, setpoint, fan, heater, heater enable;
     PID gains and their source under *Advanced*.
   - **Simulator** (only for `virtual_roaster` and `virtual_drum`): the selected
     simulator's chamber or drum, Charge/Dump beans, on/off (the same size as before:
     only the selected one is shown).
   - **Phases** once events exist.

   On a phone the primary action is also pinned in a bottom bar next to
   STOP, so it is reachable with one thumb mid-roast.

Autotune leaves the cockpit (it cannot run during a roast). A finished
autotune still shows its "apply" banner on the cockpit, linking to the
roaster's Tuning tab.

### Roasters and Add roaster

Made for three users at once, a home roaster, a small business and a power user building a
roaster of their own, with no clutter: the common path first, the rest folded in
place where it is needed, no global basic/advanced mode. A home roaster follows four steps and
never opens a fold; a shop reads the list; a builder opens the folds (open stays open per
browser) and the roaster's own page; agents and power users have `roast roaster add | map |
check`.

- **The list** (`/roasters`): a card per roaster (`RoasterCard.svelte`): reading or silent (the
  check's samples in 30 s), kind, connection, control level, the check's verdict (a link to the
  Check tab), refreshed every 10 s. **Found, not added yet** is one plain list of the roasters
  streaming on MQTT (pending: **Add** activates as it is, **Set up** opens the wizard) and
  those seen with mDNS (**Set up**). The simulators' panel, then a small link to connection
  templates (device profiles: stored defaults, linked by `profile_id`, which nothing applies yet).
- **Add roaster** (`/roasters/new`): **Roaster** (name; the id follows it as `home_roaster`
  until "change"; kind as a row of choices; **A known machine?**, Artisan's catalogue or an
  `.aset`, one line; description and location under More) → **Connection**
  (`ConnectionFields.svelte`, `lib/roaster/connectionForm.ts`: the protocols as plain-word
  choices, under them one line linking `connect-your-roaster.md` at the running revision for
  someone programming their own roaster (its path when the server has no repository URL), only the fields every roaster of that kind needs, an **Advanced** fold whose closed
  line says what it holds; a Modbus roaster's register map in the same step; **Save and check**
  saves the roaster, its connection, map and kind at control off, and going back changes them in
  place, a new protocol replacing the connection) → **Check** (`CheckView.svelte`, every 3 s;
  nothing arrived: a hint and **Test the connection**; a mapping protocol: **Map its fields**
  opens `MappingEditor` there, open by itself when readings arrive but fail) → **Control**
  (Off · Manual · Full, raising confirms; "Roast with it in this browser", on when none or a
  simulator is selected). The MQTT prefix defaults to `roaster/<id>` (it was a bare `roaster/`,
  which heard nothing). `ui-check.mjs` walks this path at 1366×768 and 390×844 (first shard).
- **One connection form**: the roaster's Overview → Add Connection is the same `ConnectionFields`.
- **Phidgets** ([`phidgets.md`](phidgets.md)): "Phidgets temperature board" in the
  Connection step (`PhidgetFields.svelte`, `lib/roaster/phidget.ts`): the computer it is plugged
  into, BT and ET inputs (ch0–ch7) and the thermocouple type; port, password (shown as saved,
  "Forget it"), board serial and hub port, intervals and RTD under Advanced. Its mapping uses
  `MappingEditor`'s JSON mode with an input select instead of a path (°C, no unit, no templates).

What it looks like: [1 Roaster](images/roasters-after-1-roaster.png),
[2 Connection](images/roasters-after-2-connection.png), [3 Check](images/roasters-after-3-check.png),
[4 Control](images/roasters-after-4-control.png), [the list](images/roasters-after-list.png),
[phone](images/roasters-after-phone.png).

### Roaster pages (`/roasters/[id]/...`)

The header: name, id, kind and control level (a link to Control), last seen; **Use this
roaster**, **Edit** (name, status, description, location, and **Delete this roaster** at its
foot: roasts stay in History). Tabs, each its own URL, so a roaster's settings are all under the roaster and nothing else:

| Tab | Contents |
|---|---|
| Overview | the device contract (firmware, readings, caps, gains against the held ones, the last boot check; `DeviceContract.svelte`), its model (`RoasterModel.svelte`, `prediction.md`), connections (Add Connection: the wizard's form), register map. Each MQTT, WebSocket, serial TC4 and Phidgets connection has a **Mapping** button (`MappingEditor.svelte`, `lib/roaster/mapping.ts`; `ingest-mapping.md`, `add-roaster.md`): units, one row per field (role or a name of its own, path, scale, offset, an extra's unit, MQTT's own topic) or the TC4's columns, the MQTT telemetry topic and command prefix, command templates; under it the newest raw message and what the mapping makes of it, refreshed every 2 s; it saves through the connection's PUT. A Modbus connection says to give registers their role in the map |
| Check | the roaster against the interface standard (`add-roaster.md`, `lib/roaster/check.ts`, `CheckView.svelte`, shared with the wizard): the verdict, a line per check (rate, BT and ET, units, outputs, channels, can do, control) with ok, warn or fail in the colour tokens, the reading now; refreshed every 5 s. Only reads. For the owner the **write check** behind a confirmation that names what it sends (heater 10 %, then emergency stop), and its steps |
| Control | **Setup** first (kind, control: Off · logs only, Manual, Full, and Outputs → Edit: each output's label, range and step, the fan floor; `roaster-setup.md`; Channels → Edit: the extra values kept with a roast, key from what the roaster sends now, label, unit, top; `channels.md`), then what that level and the outputs allow (fan assist needs both outputs): stored PID gains, gain schedule by fan and fan assist (full), the roast sequence and token access (manual or full), each with what it does; a line for each panel the level hides |
| Autotune | relay autotune (full control; below it a line instead), its live chart, results, run history and undo |
| Simulator | (the simulators only) on/off, chamber or drum, what the model is. Roasters (the list) has both switches: the fluid bed's and the **simulated drum**'s (`virtual_drum`, `add-roaster.md`), whose first switch-on sets up its roaster |

The per-roaster panels take the roaster explicitly instead of reading
"whatever the dashboard picked".

The setup also shapes the Roast page (`lib/roaster/setup.ts`, the level from
`roast.svelte.ts`): with control off the Control tab is one line and a link to the setup and
the header has no STOP; with manual it shows the heater and fan only;
below full Profile drive offers a ghost run only. Full looks as it always did. A declared drum
or damper output (`ingest-mapping.md`) adds a one-row slider each to the Control tab at manual
or full; the fluid bed declares neither, so its tab is unchanged. Roasters → Add asks the kind
first and saves at Off, then checks, then offers the level (below, "Add roaster"); its register map table has a Role column (`lib/roaster/registerRoles.ts`),
which the roaster's Overview shows read-only.

### Settings (`/settings/...`)

One page per group, with a section menu:

- **This browser** (`/settings`): temperature unit, weight unit (Metric: g, then kg; US: oz,
  then lb; `lean-home.md`), theme, alarm notifications for this device and the list of
  devices that get them (`push.md`), Sign-in (this browser signed in with the owner key or a
  token, or out; the owner also sees the signed-in browsers and signs one or all out;
  SECURITY.md, "Logins"). Every weight the dashboard shows goes through
  `fmtGrams`; every weight field is a `WeightInput` (grams bound, "250 g", "8 oz", "2 lb"
  read, a bare number is g or oz and its label says which). Doses stay grams; labels print
  both; reports keep `report_units`.
- **Roasting** (`/settings/roasting`): development target, profile
  lookahead, RoR smoothing (server-side, for every roaster), the owner's time zone (dates
  for the CLI and agents), the public address (where share pages, label QR codes and
  approval links point).
- **Alarms** (`/settings/alarms`): alarms and auto events.
- **Notifications** (`/settings/notifications`): where alarms and system events go: Web
  Push, Gotify, ntfy, Matrix, webhook, a card each with Save and Test; which system events
  are sent; the problems not cleared yet (`notifications.md`).
- **Home Assistant** (`/settings/home-assistant`): discovery on or off, the broker (the
  server's own or Home Assistant's, with its user and password), the prefix, interval and the
  simulated roasters; what is published, each roaster's entities and why nothing is
  (`home-assistant.md`). Read only in Home Assistant.
- **Backups** (`/settings/backups`): the nightly copy, Back up now, a checked restore.
- **Modules** (`/settings/modules`): the three presets (Home, Enthusiast, Roastery) and the
  six switches, each with one line; saved at once (`lean-home.md`).
- Under the heading **Agents & access**, only with the agents module:
- **Tokens** (`/settings/tokens`): API tokens for scripts and agents; the owner key only
  (`control.md`, "Tokens"). The per-roaster "Tokens may control this roaster" switch is on
  Roasters → the roaster → Control.
- **Approvals** (`/settings/approvals`): a token's calls that wait for the owner (starting a
  roast on a physical roaster, deletes), with Approve and Deny, and the decided ones; the
  link in an approval notification opens it with `?id=` (`control.md`, "Approvals").
- **Audit** (`/settings/audit`): every request that changed something, with who sent it
  (`control.md`, "Audit log").
- **About** (`/settings/about`): versions, revision, health, last backup, open problems, a
  link to the changelog at the running revision, and Download the support bundle (one JSON
  file for a bug report, secrets removed; `support.md`). With the built-in MQTT broker
  (`ROASTWARDEN_BROKER_ADDR`), the MQTT line adds "built in, on <address>" and the roasters that
  may sign in (`DEPLOY.md`, "One container"). Only while the Modbus slave is on, an
  "Artisan screen" line: its port, the roaster it serves, read-only or with writes, Artisan's
  last read, and the setup file for Artisan in the dashboard's unit
  ([`artisan-screen.md`](artisan-screen.md)). It lives here, not under Roasters, so nobody who
  never had Artisan meets it.
- A pointer, "Roaster control": PID, gain schedule, fan assist and autotune are per roaster,
  Roasters → *name* → Control.

### Label Studio (`/labels`)

Label Studio is a section of its own, not a settings page: labels are a maker's daily tool, and
the layout follows the label tools a small maker uses: a gallery to pick from, and a design page that is source
on one side and the label on the other, as typst.app is.

- **Designs** (`/labels`): every design as the server lays it out on sample data, at the
  true shape of the picked sheet (4 × 2, 3¾ × 3, 2½ round), with `design@N`, its settings
  and the coffees that use it; filters by kind and a search; **New design**.
- **A design** (`/labels/designs/[id]`, the whole width): the Typst source on the left
  (CodeMirror 6, only on this page) and on the right the live check: the label (the last
  good one dimmed while the source does not compile), what to fix, the whole sheet, and the
  report as an agent reads it; the data (sample, a roast, a coffee), brand, message and the
  design's settings above it; versions with Compare (side by side), Open and Back to this
  below. Ctrl+S saves (a new version), Ctrl+Enter checks now. A built-in or folder design is
  saved under a new id.
- **Print** (`/labels/print`, `?roast=` to start with roasts): the roasts on the sheet in
  order, each with a roast's Labels page editor (`LabelItemFields`: design, copies, net
  weight and message, with its settings, brand, notes and fields under **More**); the
  whole first sheet as it prints beside them, used labels grey, each label numbered by its
  roast, a click to start elsewhere; the start is the sheet's next free label. The actions
  (`RunActions`: Download PDF, Print, then **Printed it**) are the same as on a roast's
  Labels page and stay in view: sticky above the preview on a wide window, fixed at the
  window's foot below `lg`. The page stays put; the run is in History.
- **History** (`/labels/history`, `?run=` opens one): the runs, newest first, with their state
  in words (PDF made, printed, waiting at the printer, aborted, printed by hand) and the
  part-used sheets; a run opens to its items, the printer's job steps with times, and a
  reprint of all or the ticked labels from a chosen label.
- **Printers** (`/labels/printers`): each printer with its state in words, how labels go to
  it (pdf, or pwg/urf at a resolution), its tray, colour and format, and its calibration per
  sheet (the calibration page for that printer; first and last label measured); add one by
  address or from the ones found on the network; an `ipps` printer by its certificate's
  fingerprint, read and then ticked as trusted. A printer picker (and a folded tray and colour
  choice) shows on Print and on a roast's Labels page when there is more than one printer.
- **Coffees**, **Assets**, **Brands**: each coffee's own label with a preview on that
  coffee; uploads with font samples and the Typst line to copy; brand kits with a preview on
  the shop design.

History → a roast → Labels… stays the quick way to print one roast's labels, and links to
its design here. Its **QR code opens** choice (the roast in Roastwarden, its share page, none)
is on Print too; the roast's page in History has a folded **Share page** section under
Cupping Notes (the link, opened how often, the line for the person, another link, revoke).
The share page itself (`/R/<code>`) is not the dashboard: the server makes it, phone first,
with no script ([`labels.md`](labels.md), "The share page").

### What a page keeps while you move around

Leaving a page for a moment (a setting, a look at the scope) and coming back finds it as it
was, for the browser tab (sessionStorage, `lib/stores/kept.ts`; a new tab starts fresh):

- **Section links** (`lib/nav/sections.ts`): each top-bar link goes back to the page last open
  in that section, e.g. Beans to the bean you had open, Settings to Roasting. The link of the
  section you are in goes to its main page, as a phone's tab bar does. Forms (`/new`,
  `/edit`) and the printable report are not returned to.
- **The scope's view** (Roast, 10 min, 30 min, All).
- **History's table**: search, "For", "Completed only" and the sort.

Kept per browser already (localStorage): temperature and weight units, theme, the heater line (actual or a 15 s
average, Settings → This browser), the roaster, the Roast page's rail tab,
series shown on the scope (declared channels too, by key), panel folds, Profile drive options.
Zoom is not kept.


## Explaining modes in place

Each mode gets a one-line description under the control and a "?" that
expands the full explanation inline (no modal, no separate help page):

| Mode | One line |
|---|---|
| Manual | You set the heater %. The setpoint is ignored. |
| Auto (PID) | The roaster's PID moves the heater to hold BT at the setpoint. |
| Profile drive: Ghost | Computes what it would send and shows it; sends nothing. Engage when it looks right. |
| Profile drive: Live | Sends the profile to the roaster every second. Touching a control pauses that channel. |
| Replay mode Setpoint | Follows the profile's BT curve with the PID. |
| Feedforward | Plays the profile's heater % open loop. |
| Feedforward + PID | The profile's heater % plus PID correction on BT. |
| Loop: device / server | Where the BT PID runs: on the roaster (fast, default) or on the server (any roaster that takes heater %). |
| Events: by time / by BT | When the profile's step changes fire. |
| Align on | Which landmark lines the profile up with the roast; until it is marked, CHARGE. |
| Fan channel | Replays the profile's fan; with fan assist the profile fan is the ceiling. |
| Fan assist | Lowers the fan when the heater is pinned, never below the floor; the fan slider becomes the ceiling. |
| Gain schedule vs stored gains | The schedule picks gains by fan speed when on; otherwise the stored gains are used. |
| Autotune | The server's relay test (Autotune 2) measures the roaster and designs gains; empty chamber or beans, no session. |

## Visual direction

- Top bar instead of a sidebar: the scope gets the width.
- Readouts in tabular numerals, the series colours the scope uses (BT amber,
  ET blue, RoR green), so a glance links number and line.
- Cards with a one-line header summary when folded; fold state is kept per
  browser.
- Ink (`action`) is the accent; amber marks BT and warnings; red is reserved for
  STOP and errors; green for "go" (start, charge, engage).
- Light and dark from the same tokens.
- Colour jobs have their own tokens (`src/app.css`): `action` (primary buttons,
  the selected tab underline), `focus` (rings), `go` (start, charge, engage), `selected` (the
  chosen option of a segmented control), `highlight` (where you are: current step, unit).
  Use them, not palette classes, for those jobs; amber stays for BT and warnings.
- Roast guidance (`roast-guidance.md`) uses amber only for a value outside its expected
  range, on the number and its band marker; within is plain text. Bands are thin bars beside
  the number, never boxes, and the summary is at most three lines; no page or panel of its own.
- **The look is "Chart paper"**, picked from three directions shown side by side: a strip-chart recorder's paper and ink. Dark is carbon
  blue with a periwinkle ink, light is recorder-green paper with ink blue and a chart-paper
  grid on the scope; Atkinson Hyperlegible Next a size above Tailwind's default. The ink rule
  under the top bar (the text colour, so near-white in dark) read as a glitch, so the divider is
  plain; the active section's underline carries the ink. Tokens in `src/app.css`, chart colours in `lib/constants/chartTheme.ts`.
  The other two directions (a grey ISA-101 control room, green enamel and brass) were dropped.
- **The mark is "Warden"**, picked from fourteen marks over four rounds shown on the real app: a shield in the text colour keeping the amber flame,
  drawn on Lucide's 24 px grid with stroke 2 so it sits beside the app's icons. It is the
  top bar's mark (`lib/components/ui/Mark.svelte`), the favicon (`static/favicon.svg`, the
  shield follows the browser's colour scheme) and the installed app's icons (ink-blue tile,
  rendered from `crates/server/assets/brand/`). In one
  colour it heads the reports' footer line and the share pages' "Made with Roastwarden".
  The wordmark is the name in Atkinson Hyperlegible Next Bold beside the mark; README shows
  it (`docs/images/brand-{light,dark}.png`). Amber stays the flame's colour and BT's. Not
  chosen: a curve on chart paper (a tick at 16 px), a bean, the flame redrawn (a drop from
  afar), the flame in a ring or on a paper tile, softer shields (scooped, rounded, amber,
  open), a bean or curve inside the shield: the plain shield read best at a glance.


## Choices that hold

- **The Roast page fits the window** on a laptop (1280×720 up to 1920×1080, idle and
  mid-roast): no page scroll and no scrolling panel; the browser checks fail on either, idle
  and during a driven roast. The readouts are one strip (time, BT, ET, RoR, Δ profile
  large; setpoint, heater, fan, DTR small). The panels under the step card are tabs (Control ·
  Profile drive · Simulator · Phases, kept per browser) with a dot on Profile drive while it
  runs; the tab body may scroll only on very short windows. The Profile drive tab's label
  carries the drive's state and profile time ("Drive Live 4:12", Ghost, Paused), so it shows
  from any tab; Stop sits on the drive's status line; below 820 px tall the tab drops the log
  tail, the PID line (it is the status line's tooltip) and events after the next two, so it
  never scrolls mid-drive. Scope landmark labels stack in up to three rows, losing the
  temperature and then the label when there is no room; event values along the bottom skip
  rather than overlap.
- **The Next step card stays pinned** at the top of the right rail on a laptop; only the panels
  under it scroll, so the landmark buttons never leave the screen mid-roast. The set-up step
  offers the next planned roast for the roaster in one line ("Planned 1 of 3: bean · weight",
  Skip, Unplanned; `batches.md`), loads its profile, and the primary button reads "Start planned
  roast"; the bean picker sits in the folded batch details with the stock the roast leaves.
  `#12` precedes the roast's name. Charge and Drop act on the roaster (heater power, fan,
  Profile drive, cooling) as set per roaster under Control (`roast-sequence.md`); the step
  card's hint says what Charge will do. Cancel sits with Pause and Complete and confirms on the
  card before it discards the roast.
- **Explanations sit behind "?"** next to each control's label (a popover with the line and the
  full text), except the line for the current mode, which stays visible (`Hint`). Profile
  drive's three mode cards are one segmented control.
- **The roaster's tuning tabs act on the selected roaster** (their live values come from its
  telemetry). On another roaster's page they offer "Use this roaster" rather than tune it
  blind. Roaster PID gains are editable in two places on purpose: quick edits under Roast →
  Roaster control → Advanced, the schedule and history on the roaster's page.
- **Profiles → "Use for next roast"** picks the profile and opens the Roast page with the
  preview on the scope.
- **Profiles read like Artisan backgrounds** (`/profiles/[id]`): BT, ET, ΔBT, ΔET, fan and
  heater, landmark lines with time and BT, phase bands, control events as markers; key numbers
  (charge, TP, dry end, FC, drop, development); phase, landmark and event tables; beans, batch
  size and roaster. Curves run through the points on a monotone spline (as the Artisan export
  does), so ΔBT has no steps at the points; the scope keeps straight segments, which is what the
  replay follows. `lib/profile/view.ts` computes all of it (tested). The list is a table linking
  to each profile; import is one button (the file's own title names the profile); the page
  exports `.alog` and `.json`. The designer has beans, batch size and roaster fields and returns
  to the profile's page on save.
- **History lists roasts as cards on a phone**; the full ranking table from `sm` up. A roast's
  chart in History takes the Roast page's compact axes below 560 px (`lib/scope/axes.ts`);
  every roast chart's time axis steps in whole clock minutes (2:00, not 1:40).
- **The scope's internals, the profile designer and transposer, the comparator and the report**
  are self-contained pages and stay so.
- **A plainer look**: smaller corner radii, no pill-shaped nav (underlined text tabs), fewer
  tinted boxes; nothing that reads as generated.
