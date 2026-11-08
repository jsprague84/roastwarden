# Roast guidance

After a roast you have numbers: weight loss, development time, colour, first crack. Roast
guidance reads them for you: against published references for the numbers that carry over
between machines, and against your own roasts of the same profile for the ones that don't.
It adds no page and no alarm. It puts a thin band beside the numbers you already see, and at
most three short lines naming what is off.

## Set a roast level on the profile

Guidance starts from the profile. Profiles → the profile → **Edit**:

- **Roast level**: Extremely light to Dark, each with its ground Agtron range. This sets what
  the roast's numbers are judged against.
- **Purpose**: espresso, filter, omni or other. It only suggests a level; the level is what
  counts.
- **Expected** (collapsed): weight loss, development time, DTR and ground colour. Empty fields
  use the level's defaults, shown as placeholders; fill one in to set this profile's own range.

With no level, Settings → Roasting → **Development target (%)** still sets the scope's DROP
time, and the roast's page asks you to set a level.

## What is judged, and against what

| Number | Judged against |
|---|---|
| Weight loss % | the level's published range, corrected about 1 point per point of green moisture away from 10.5 % |
| Development time (first crack to DROP) | the level's range, in seconds |
| DTR % | shown with development, judged loosely: on an air roaster roasting light it runs well below the 20–25 % often quoted |
| Ground colour | the level's SCA colour tile on the ground Agtron (Gourmet) scale |
| Total time, first crack and drop BT, dry-end BT | your own roasts of this profile on this roaster only |
| Green moisture | 8–12.5 % (ICO); density gets a word (low, average, dense), never a warning |

The level defaults, by ground Agtron:

| Level | Agtron | Weight loss | Development |
|---|---|---|---|
| Extremely light | 90 + | 11–13 % | 0:30–1:15 |
| Very light | 80–90 | 12–14 % | 0:45–1:30 |
| Light | 70–80 | 14–16 % | 1:00–2:00 |
| Medium light | 60–70 | 15–17 % | 1:30–2:30 |
| Medium | 50–60 | 15–18 % | 2:00–3:30 |
| Medium dark | 40–50 | 18–20 % | none |
| Dark | below 40 | 20–22 % | none |

Some ranges are Roastwarden defaults built from the cited points, and their hint says so.

**Temperatures are never judged against published numbers.** Thermocouples read differently
from one roaster to the next, so first crack at 195 °C on yours says nothing about someone
else's. Your own history needs at least 3 other roasts of the profile on this roaster; until
then a muted line says so, and temperatures and times are not judged.

## Reading it

- **The band** is a thin bar beside a number: the range, and a marker for this roast. Within
  the range is plain text. Outside is amber, on the number only.
- **The "?"** beside it says who expects what ("Light expects 14–16 %. Above."), the basis,
  and links to the sources. Weight loss also says what level the loss suggests, and that chaff
  blown off in a fluid bed adds about 0.5–1 point.
- **The summary lines** name only what is off; nothing off, no lines. For example: "Development 4:06 (DTR 34 %): Light expects 1:00–2:00. Shorten the
  profile after first crack."
- **RoR notes** name the signal, not a defect: "RoR rose after first crack", not "baked". Their
  thresholds are Roastwarden's own: no source gives numbers.

Colour is entered on the roast's page with its meter: **Agtron (gourmet)** as is, **Tonino**
converted with the maker's table (approximate; it can't tell above about Agtron 90), **Lebrew
RoastSee** read as Agtron-like with no published conversion, **Other** against your own
readings only. Read colour 30 minutes to 4 hours after the roast: it drifts darker by up to 3
points in a day.

## Where it shows

| Place | What |
|---|---|
| History → the roast | the summary under the title; bands beside duration, colour, weight loss, development and first crack and drop BT |
| History list | an amber dot after a loss, colour or DTR that is outside |
| Roast page | while developing, the expected window as a bracket at the top of the scope and "aim 1:00–2:00" in the readout; after the roast, once you enter the weights, the summary |
| Profiles → the profile | where the level sits on the colour tiles, what it should give, and its roasts on a roaster with at most three lines on the newest five ("First crack came 1:12 earlier … Set the profile's FC mark to 195 °C") |
| Profile designer | development, DTR and the phase split judged as you drag the curve (the phase split is information, never amber) |
| Beans → the bean | green moisture and density |
| Reports | the summary, and ranges in brackets: "21.7 % (light 14–16)" |

It won't look anything up online, give an AI verdict or raise an alarm.

From the command line, `roast review <roast>` judges a roast's numbers against what was
expected, and `roast profile edit <profile> --level --purpose --expect KEY=LOW-HIGH` sets them
([command line](cli.md)).

<!-- site: end -->

## Design notes

For developers: how roast guidance works and why. The site publishes only the part above.

Code: `apps/dashboard/src/lib/guidance/` (pure, with tests), `RangeBand.svelte`,
`GuidanceSummary.svelte`, `ProfileGuidanceFields.svelte`, `RoastAfterSummary.svelte`; server:
migration 019, `ProfileHistoryRoast` in `profiles/history.rs`. The sources below are graded
(a, b, c) in the research behind this guidance.

## What it is for

After a roast the numbers are recorded (the green and roasted weight, development, colour,
first crack), but not where they fall against what the profile or the roast wanted: is this
loss in moisture what a light roast gives? Finding out takes manual research. So: **the app
reads the numbers for you**, against cited references and against this roaster's own history,
in the places those numbers already appear. It adds no page, no panel and no alarm: guidance
must not turn into creep, bloat or clutter. It is tuned for light roasts for espresso and light
or medium ones for filter, ~100 g batches, on a home fluid-bed roaster; the drive follows the
profile and roasts are shaped by editing profiles, so guidance points at **profile edits**.

## Principles (each from a graded source)

1. **Judge against the roaster's own reference first, published ranges second.** Every tool
   that guides well does this (Cropster "meet the reference" goals, Artisan background CM,
   Typica specs, RoastPATH re-anchoring community data to the user's own FC temperature).
   Published ranges are for numbers that carry over between machines.
2. **Never judge absolute temperatures against published values.** Thermocouple readings are
   not consistent between machines (Cooper & Hoos 2019; Rao 2016, probe size). FC BT, drop BT,
   dry-end BT: own history only.
3. **Development time in seconds over DTR %.** Rao, who coined DTR, says its 20–25 % applies to
   drum roasters at > 60 % load and medium or light-medium roasts; on air roasters roasting
   light, "most roasts end up with much lower DTR" (2023). FC marking error of 20–40 s moves DTR
   by ~4 points. Show DTR, but judge development by seconds.
4. **Three states, judged only when judgeable** (Cropster smart goals): *waiting* (can't be
   judged yet), *within*, *outside* (below/above). Weights and colour are judged after the roast;
   development live.
5. **Name the signal, not the defect.** "Baked" has three competing definitions (Rao: RoR crash;
   old SCAA: BT dip after caramelisation; SCA/Münchow: long development). Say "RoR rose after
   FC", not "baked".
6. **Say "not enough data" rather than guess** (RoastPATH withholds thin results). Own-history
   bands need ≥ 3 roasts.
7. **No live web lookups, no LLM verdicts.** Users distrust generic AI roast judgements
   ("garbage results that sound convincing"; ChatGPT mixing BT and ET for a machine). The
   references are a reviewed, dated, cited table in the repo.
8. **Amber is the only colour for "outside"**, and only on the number itself; *within* is
   unmarked text. The band is a thin bar, not a box ("Chart paper", `ui-structure.md`).

## What gets judged, and against what

Roast level vocabulary: the **SCA colour tiles on ground Agtron Gourmet** (Agtron M-Basic II
manual 2018, type a), each tile ±5. Ground, because the manual says the ground reading is the
roast classification and whole bean only shows evenness. Do not mix in Coffee Review's names
(they use the average of whole and ground and sit about one tile darker).

| Level key | Label | Ground Agtron (Gourmet) |
|---|---|---|
| `extremely_light` | Extremely light | 90 + |
| `very_light` | Very light | 80–90 |
| `light` | Light | 70–80 |
| `medium_light` | Medium light | 60–70 |
| `medium` | Medium | 50–60 |
| `medium_dark` | Medium dark | 40–50 |
| `dark` | Dark | below 40 |

Purpose → suggested level (typical practice, **not a standard**; label it so): light filter →
very light (Coffee Review's lightest ≈ 95 ground; one roaster 97); light espresso → light
(shop figures 75–83, low confidence); medium filter → medium light / medium (SCA cupping
reference 63 ground, Agtron "nominal medium" 60). The profile stores the level; the purpose
only suggests it.

### Reference table (ships in `lib/guidance/references.ts`)

Every row names its sources by id (registry below). "Derived" rows are Roastwarden defaults built
from the cited points; the hint must say "Roastwarden default from …".

| Metric | By level | Range | Basis | Sources |
|---|---|---|---|---|
| **Weight loss %** | extremely light | 11–13 | Mondry: Agtron 95 → 12 %; Sweet Maria's FC 10.3, City− 11.5 | mondry2004, sm2023 |
| | very light | 12–14 | Mondry 85 → 13 %; SM City 12.7, City+ 13.3; Rao light ≈ 12 % | mondry2004, sm2023, rao2023 |
| | light | 14–16 | Mondry 75 → 15 % (interp. 80 → 14, 70 → 15.5) | mondry2004 (derived) |
| | medium light | 15–17 | Mondry 65 → 16 % | mondry2004 |
| | medium | 15–18 | Mondry 55 → 17 %; Schenker 100 g fluid bed 15.3–15.8 % at L* 24 | mondry2004, schenker2000 |
| | medium dark / dark | 18–20 / 20–22 | Mondry 45 → 19, 35 → 20, 25 → 22 | mondry2004 |
| Weight loss correction | all | ± 1 pt per ± 1 pt green moisture from 10.5 %, both ways | Schenker: final moisture and organic loss converge whatever the start (derived slope ≈ 0.97); the argument is symmetric | schenker2000 (derived), sm2023 |
| Chaff in measured loss | fluid bed | ≈ 0.5–1 pt | Schwartzberg assumes 1 %; home measurement ≈ 0.5 %; silverskin 1–2 % of bean is the upper bound | schwartzberg2011, nolasco2022 |
| Coffee-to-coffee spread | all | ± 2 pt at equal colour | Schenker's "medium" at 13 % and 15 % in two coffees | schenker2000 |
| **Development time s** (FC → DROP) | extremely light | 30–75 | Wendelboe 45–65 s light on a ROEST (≤ 6.5 min roasts) | wendelboe2022 (derived) |
| | very light | 45–90 | same; Alstrup "fast" 90 s = fruity, sweet, acidic | wendelboe2022, alstrup2020 (derived) |
| | light | 60–120 | Alstrup fast 90 / medium 143 s | alstrup2020, munchow2020 (derived) |
| | medium light | 90–150 | Alstrup medium 143 s | alstrup2020 (derived) |
| | medium | 120–210 | Alstrup medium 143 / slow 266 s | alstrup2020 (derived) |
| Development, measured effect | all | at equal colour, longer development → less acidity and sweetness, more bitterness; body unaffected | Münchow et al. 2020 (80–405 s studied) | munchow2020 |
| **DTR %** (shown, judged loosely) | very / extremely light | 10–17 | Rao 2023 (air roaster, light: "much lower" than 20); Wendelboe implied 12–17 | rao2023, wendelboe2022 (derived) |
| | light | 12–20 | Rao 2016: sample roasters "much lower DTR"; 15 % on high power-to-batch | rao2016 (derived) |
| | medium light / medium | 18–25 | Rao 2014: 20–25 % drum, light-medium/medium | rao2014 |
| **Ground colour** | per level | the tile band above | SCA tiles via Agtron manual | agtron2018 |
| Colour on-target tolerance | own target | ± 2 (Rao) to ± 3 (Hoos) points | batch QC practice | rao2024, hoos2021 |
| Colour drift | — | up to − 3 points in 24 h; measure within 30 min–4 h, at room temperature, record the grind | Agtron manual; SCA cupping standard | agtron2018, sca2015 |
| Whole − ground gap | — | **shown, not judged**: no fluid-bed data; grind moves ground readings 63 → 85 | Coffee Review 2026 data (20–25 at medium light, drum); Hoos saw a wider gap on an IKAWA | coffeereview2026, hoos2023, luther2023 |
| **Total time** | — | **own history only**; context in the hint: air roasters 6–7 min common (Rao 2023), SCA sample roast 8–12 min | rao2023b, sca2015 |
| Too fast / too slow | own normal | reaching the reference colour in ≈ 0.5× / ≈ 2× the normal time | SCA sample-roasting guidance (secondary) | scaexam (type c) |
| Phase split % | — | **hint only, never amber**: drying 30–45, Maillard 35–45, development 15–25 (community); Rao 2022: Maillard-length advice unsupported | torque, rao2022 |
| **Green moisture %** | — | 8–12.5 (ICO Resolution 420, ISO 6673); typical specialty ≈ 10.5 | ico2004, sm2023 |
| Green density g/L (free-settled) | — | ≈ 640 low, 670 average, > 690 dense (one importer's classes, not a standard; − 0.04 g/mL per + 1 % moisture) | kornman2016 |
| **FC BT, drop BT, dry-end BT, ΔT FC → DROP** | — | **own history only** | cooper2019, rao2016b |
| **Tracking RMSE** | — | own history; Artisan's author: ≈ 1 °C "not too bad" | luther2019 |

### Meter scales

`color_scale` is `agtron | tonino | lebrew | other`. Bands are in Agtron Gourmet ground.
- `agtron`: as is.
- `tonino`: convert with the maker's table (Tonino 125–130 ≈ 80–90, 115–125 ≈ 70–80, 100–115 ≈
  60–70, 85–100 ≈ 50–60, 75–85 ≈ 40–50, 60–75 ≈ 30–40; cupping 105 ≈ 63), piecewise linear,
  hint "approximate (maker's 13-sample fit); Tonino can't resolve above ≈ Agtron 90".
- `lebrew` (RoastSee) and DiFluid-type meters: read as Gourmet-like (0–150), hint "no published
  conversion to an Agtron M-Basic; compare against your own readings first".
- `other`: no published band; own history only.

### Source registry

`SOURCES` in `references.ts`: `{ id, title, url, type: 'a'|'b'|'c', date }`. The set:

| id | Source | Type |
|---|---|---|
| agtron2018 | Agtron M-Basic II manual (tiles, Gourmet ↔ Commercial, drift) | a |
| sca2015 | SCAA standard "Roast Level for Cupping" (63 ± 1 ground; 8–12 min) | a |
| mondry2004 | Mondry, SCAA 2004, via Schwartzberg (UMass) "Coffee Roasting" PDF, Fig. 2 | a |
| schwartzberg2011 | Schwartzberg roast-loss mass balance (chaff 1 %) | a |
| schenker2000 | Schenker, ETH Diss. 13620, 100 g fluid bed | a |
| munchow2020 | Münchow, Alstrup, Steen, Giacalone, *Beverages* 6:29 (2020) | a |
| alstrup2020 | Alstrup et al., *Beverages* 6:70 (2020): 90 / 143 / 266 / 390 s | a |
| ico2004 | ICO Resolution 420 (moisture 8–12.5 %, ISO 6673) | a |
| nolasco2022 | Nolasco et al., *Foods* 11:2834 (silverskin 1–2 %) | a |
| rao2014 | Rao, Daily Coffee News 2014-08-12 (DTR 20–25 %) | b |
| rao2016 | Rao blog 2016-08-25 "Development Time Ratio" | b |
| rao2016b | Rao blog 2016-09-07 "Be careful when interpreting bean data" | b |
| rao2022 | Rao blog 2022-07-13 "Maillardmania" | b |
| rao2023 | Rao blog 2023-10-06 "What changed in coffee over the past 30 years" | b |
| rao2023b | Rao blog 2023-05-18 "IMF vs Loring" (air roasters 6–7 min) | b |
| rao2024 | Rao blog 2024-05-07 "Prodigal QC protocols" (± 2 points) | b |
| wendelboe2022 | ROEST blog 2022-01-25 "Tim's roesting tips" (45–65 s) | b |
| hoos2021 | Hoos, "A tale of two color meters" (± 3) | b |
| hoos2023 | Hoos on the IKAWA blog 2023-09-15 | b |
| cooper2019 | Cooper & Hoos, Daily Coffee News 2019-05-24 | b |
| luther2019 | Artisan blog 2019-05 "How close" (CM) | b |
| luther2023 | Artisan blog 2023-03-05 "Understanding roast color" | b |
| sm2023 | Sweet Maria's "How to calculate weight loss" 2023-08-17 | b |
| kornman2016 | Royal Coffee, green analytics part IV: density | b |
| coffeereview2026 | Coffee Review roast definitions + 2026 review data | b |
| torque | Torque Coffees phases guide | c |
| scaexam | SCA sample-roasting problems (study-guide paraphrase) | c |

## Data model

Migration **019** (`019_roast_guidance.sql`, registered in `db.rs`; no backfill beyond the
defaults):

- `roast_profiles` gains `roast_level TEXT` (a level key), `roast_purpose TEXT`
  (espresso | filter | omni | other) and `expectations TEXT` (JSON, overrides only:
  `{"weight_loss_pct":[14,16],"development_s":[60,120],"dtr_pct":[12,20],"color_ground":[72,78],…}`).
  Missing keys fall back to the level's defaults; the editor shows the defaults as placeholders.
- `create_session` copies the profile's `roast_level` into `target_roast_level` and its purpose
  into `roast_purpose` when the request leaves them out (same rule as the plan and bean fill).
- The JSON export carries all three fields; a roast saved as a profile keeps its level and
  purpose. Artisan import/export: `roast_level` ↔ nothing (Artisan has no field); the `.alog`
  is left as is.

Own history, server side (temperatures are not in the session list): `GET
/api/profiles/{id}/history?device_id=` → per completed roast of that profile on that roaster
(newest first, at most 20): `session_id, batch_number, fc_s, fc_bt, drop_s, drop_bt, dry_s,
dry_bt, development_s, delta_t_fc_drop, total_s, weight_loss_pct, color_ground, rmse`, from
`roast_sessions` joined with `roast_events`. Pure TS turns it into bands: mean ± max(2σ,
floor) with floors temperature 2 °C, time 15 s, loss 0.5 pt, colour 2 points; null when fewer
than 3 roasts (the roast being judged excluded). A band never starts below zero: when one odd
roast makes σ so wide it would, it starts at the smallest value seen (the reports'
`expect::history_band` is the same).

## Dashboard

Pure modules (vitest next to each), under `lib/guidance/`:
- `references.ts`: `SOURCES`, `LEVELS`, `REFERENCES`, `defaultsFor(level)`, `toAgtron(value, scale)`,
  `levelForAgtron(agtron)`, `levelForLoss(lossPct, moisture?)` (inverse of the Mondry curve, for
  "loss suggests about Agtron 30").
- `expect.ts`: `expectationsFor(profile, session)` → merged ranges with their source
  (`profile` | `default` | `history`).
- `judge.ts`: `judge(value, range)` → `{ state: 'waiting'|'within'|'below'|'above', range, source }`.
- `summary.ts`: `guidanceSummary(...)` (`roastSummary` is taken, by `lib/roast/report.ts`) → at
  most three short lines naming only what is off, ordered colour, weight loss, development, FC
  vs profile, total time, RoR events, temperatures vs history; an empty list when nothing is off
  (show nothing then). `readRoast()` wires a session, its events, telemetry and the profile's
  history into expectations, judgements, the lines and a note.
- `ror.ts`: Roastwarden's own RoR event heuristics, **labelled as Roastwarden's** in the hint (no
  source gives numeric thresholds): after FC, a *rise* of ≥ 2 °C/min in the 30 s-smoothed RoR
  held ≥ 20 s ("RoR rose after FC"); a *fall* of ≥ 8 °C/min within 60 s around FC ("RoR
  dropped sharply at FC"); RoR ≤ 0.5 °C/min for ≥ 30 s after FC ("BT stalled"). Thresholds are
  constants to tune on real roasts; they feed the summary only, never the scope.
- `profile.ts`: `reviewProfile`, a profile's roasts read against it (below).

Components: `RangeBand.svelte` (a thin bar ~56 px wide beside a number: range, marker, amber
marker when outside, `Hint` with the range, its basis and its source titles and links) and
`GuidanceSummary.svelte` (the lines, plain text, amber bullet).

**Hints.** Every band's "?" says who expects what ("Light expects 14–16 %. Above."), the basis,
"a Roastwarden default from the cited points" for derived rows, the extra lines (loss suggests
a level, chaff, DTR caveat, colour drift) and links to the sources.

**Thin history** shows one muted line under the summary: "No other roasts of this profile on
this roaster yet; temperatures and times are judged from 3." A profile with no level and no
expectations gets "Set a roast level on the profile to read weight loss, colour and development
against references."

Where it appears (and nowhere else):

| Place | What |
|---|---|
| History → roast page | `GuidanceSummary` under the title; `RangeBand` beside Duration and colour (the metadata line), weight loss (with the moisture correction and a "loss suggests ≈ <level>" line in its hint), the Development card (development time, DTR in its hint) and FC / drop temperature (`KeyTemperatures`, own history). The phase table's DTR is never amber (amber means outside). The chart draws BT RoR (±15 s slope, as in the report) on its own axis, from 30 s after CHARGE to 15 s before DROP; an Artisan `.alog` carries temperatures only and Artisan computes its own RoR, so this changes nothing in imports or exports |
| History → report | the summary, and ranges printed as text in brackets ("21.7 % (light 14–16)") |
| History table | an amber dot after Loss %, Colour G or DTR % when outside; development is judged in seconds there too (Time × DTR), against the level only (no per-row history) |
| Roast page | while developing, the scope draws the expected window as a bracket under its top edge from FC + low to FC + high, the Dev readout adds "aim 1:00–2:00", and the development time turns amber once past the window. After Complete, "Saved: …" takes the roasted weight (and the green weight if the roast had none), then shows the summary. The page must still fit (`ui-check`; `drive-check` enters the weights and checks it) |
| Profiles → profile | the editor's "Roast level" and "Purpose" selects and a collapsed "Expected" group (open when the profile overrides something) with the level defaults as placeholders and their sources in `Hint`s. The profile page lists the level and purpose with what it was made for, shows where the level sits on a strip of the colour tiles and what results it should give ("Light: weight loss 14–16 %, ground Agtron 70–80, development 1:00–2:00"), bands the planned development, and has a "Roasts on <roaster>" section: the profile's roasts newest first (first crack against the profile, development, loss and colour, amber when outside the level) and at most three lines averaging the newest five, e.g. "First crack came 1:12 earlier … at 194.5 °C, 5.5 °C below its mark (1 roast). Set the profile's FC mark to 195 °C …" (`reviewProfile`; first crack is compared with the profile's own mark, never a published temperature; tolerances 30 s and 3 °C) |
| Profile designer | the curve judged as you drag it: development with a band, DTR in its hint, the phase split as information (never amber), and the results to expect. The phase table's Share column has the same hint |
| Beans → bean | green moisture against ICO 420 (8–12.5 %) with a band; density a class word (low / average / dense, one importer's classes) with a hint, never amber |
| Settings → Roasting | the existing "Development target (%)" stays the fallback when a profile has no level |

Compare stays as it is (it already has mean ± σ).

## Worked example (the acceptance fixture)

A recorded roast: Light espresso profile, purpose espresso, 100 g Brazil, FC 7:48 @ 194.5 °C,
FC end 10:48, DROP 11:54 @ 212.3 °C (stored events, CHARGE at 5.4 s), 78.3 g → 21.7 % loss,
development 246 s, DTR 34.5 %, no colour recorded. With the profile's level set to `light`, the
summary must read (wording may differ, content may not):

1. "Weight loss 21.7 %: Light expects 14–16 %. Loss like this usually means a dark roast (about
   Agtron 25–35); a colour reading would confirm it."
2. "Development 4:06 (DTR 34 %): Light expects 1:00–2:00. Shorten the profile after first crack."
3. "First crack 1:12 earlier than the profile (7:48 vs 9:00)."

and nothing about temperatures (the thin-history line instead). This is a unit test of
`summary.ts` and a browser check of the History page.

## Tests and checks

- `references.test.ts`: every reference names ≥ 1 known source; every source is used; derived
  rows say so; bands per level are ordered and don't invert; Tonino conversion hits the maker's
  points; `levelForLoss(21.7)` → dark.
- `judge`, `expect`, `summary` (the fixture above, a within-everything roast → empty summary, a
  roast with no weights → no loss line), `ror` on synthetic curves (rise, crash, stall, smooth).
- Server: migration 019 on a copy of a real database; the create-session fill; the history
  endpoint (cancelled roasts excluded, other roasters excluded).
- `ui-check.mjs` and `drive-check.mjs` stay green at all three laptop sizes; screenshots of the
  roast page, report and profile editor in light and dark.

## Not doing

Live web lookups; LLM verdicts; auto-drop from guidance; alarms from guidance; judging
temperatures against published values; a separate guidance page; whole/ground gap targets for
a fluid bed; water-activity fields (no meter here).

## Keeping it current

Each `SOURCES` row carries its source's date. Review the references yearly, or when one looks
wrong in use.
