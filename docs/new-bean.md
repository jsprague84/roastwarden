# A new bean's first roast

The first roast of a new coffee is the one you know least about. Roastwarden helps on both
sides of it: before it, what to expect (first crack's temperature and time on your profile,
how wide the window is, what the bean's process means for marking it); after it, what to
change for the second roast, in seconds or °C you can act on.

## How it reads a new bean

Your roaster's own history counts more than the bean's description. No study predicts first
crack from process, density or moisture well, and one roast of a coffee tells more than any of
them. So the expected first crack comes from your roasts here (this profile's, else every
roast on the roaster), and the bean's process only widens the window. The guide never changes
a profile or a plan by itself, and never reads your tasting notes: it gives lines, and you
decide.

What it uses is already on the bean: its process, whether it is a peaberry, the supplier's
roast recommendation and espresso advice, and density and moisture when you entered them.

## Before the first roast

The bean's page (Beans → the bean) shows **First roast** while the bean has no finished roast
on the roaster, with a profile picker. When the next planned roast is a new bean, the Roast
page's set-up step shows its first line, the rest behind the "?". The lines:

| When | What it says |
|---|---|
| always | start from your usual profile for the roast's purpose: the process doesn't change the start |
| always | first crack expected near a BT, about a time on the profile, ± 3 °C for a washed coffee, ± 4 °C for a natural, honey, wet-hulled or unknown process |
| natural, honey, wet-hulled | naturals often crack quietly, with stray pops first: mark first crack at the first run of pops |
| the profile drops by time | a first crack off by that much changes development as much: drop by development for a first roast? The **Drop by development…** button opens the profile, where you set Auto DROP to development ([replay](replay.md)) |
| wet-hulled, density below 650 g/L or moisture above 12 % | expect slower drying, about 30 s more to first crack and 1–2 points more weight loss at the same colour |
| the supplier gives advice | the supplier's roast level and espresso advice, quoted |

## After a roast, for the next one

History → the roast shows **Next roast of this bean** while the bean has fewer than three
finished roasts on the profile; from the third, the profile's own history judges it. The
lines:

- **First crack**: where it came against the profile's mark; the next roast of the bean
  expects it there.
- **Development**: by time or by hand, how far to move the drop to keep the profile's
  development; by development, nothing to move.
- **Out of range**: development or weight loss outside the expected range, with a change in
  steps of 15 s ([roast guidance](roast-guidance.md)). One change at a time: when the drop is
  to move, the development line gives no second number.
- **Late first crack**: after 80 % of the roast, a hint that development is likely short.
- **Wet-hulled**: a reminder that these lose 1–2 points more weight at the same colour.
- **If it tastes…**, under the lines: sour or grassy, add 15–30 s of development or a
  higher drop; flat or bready, shorten development; dark tips, a gentler start.

## Limits

It does not predict which way a process moves first crack (the sources disagree), does not
map a supplier's level names (City, Full City) to roast levels, and does not use the green
coffee's age or suggest rest days (the roasted coffee's rest window covers that:
[roasted coffee](roasted.md)).

## From the command line

`roast bean guide <bean> [--profile P] [--roaster R]` gives the lines before a first roast, or
with `--after <roast>` those after it; `roast review` prints "Next roast of this bean" too
([roast analysis](analysis.md)).

<!-- site: end -->

## Design notes

For developers: how a new bean's guide works and why. The site publishes only the part above.
The Source columns below cite the graded research behind these rules by its finding (§) or rule
number, with the grade (a, b, derived).

## What it is for

A new bean (often a natural, sometimes wet-hulled) is the coffee known least about. Before its
first roast, say what to expect; after it, say what to change for the second, in numbers.

## Principles

1. **This roaster's history beats the bean's attributes.** No measured study predicts first
   crack from process, density or moisture; across coffees on one hot-air roaster first crack
   spread by 16–31 s (SD), and one roast of the coffee cut the error to 18 s (research § 2).
   So expectations come from the roasts here, widened for what the bean is, never from a
   published temperature (`roast-guidance.md` principle 2).
2. **Say why, quote sources, never a verdict.** Each line names its source and grade, as the
   roast guidance does. Sweet Maria's level and espresso advice are quoted, not judged against
   the roasts here (research § 7).
3. **Development is what to keep.** When first crack moves, the drop moves with it (research § 5).
   With auto DROP by development this is automatic; by time, the app says how far to move.
4. **One change at a time,** in seconds or °C the owner can act on.
5. **Nothing changes a roast by itself.** Lines only; the owner edits the profile or the plan.

## What a bean is, for these rules

From the bean's record, nothing new to enter:

| Read as | From | Values |
|---|---|---|
| process | `process` (free text from the supplier) | washed, natural (dry, "dry process"), honey (pulped natural), wet-hulled (giling basah), unknown |
| peaberry | `details["Bean form"]`, the name | yes / no |
| supplier's level | `roast_recommendation` | quoted ("City to Full City+") |
| supplier's espresso advice | `details["Recommended espresso"]` / `["Reference Recommended espresso"]`, `cupping_notes` | yes / no / not said |
| density, moisture | `density_g_l`, `moisture_pct` | used only when entered |

Sweet Maria's ".6 d/300gr, 15-17 Screen" is a defect count per 300 g and the screen size, not
density (research § 1): the screen-size field gets that as its hint, and nothing reads a density
from it.

The supplier's espresso advice is the first sentence of the roast recommendation or the cupping
notes that names espresso, quoted; a `Recommended espresso: No` detail outranks it ("not
recommended"). The supplier's level (B6) is the recommendation's first sentence.

## Before the first roast

Shown when the bean has no finished roast on this roaster. Where: the bean's page ("First roast"
under its details), the Roast page's step card when the planned roast's bean is new (one line,
the rest behind a `Hint`), and `roast bean guide` (agent path below).

| # | When | Line | Source |
|---|---|---|---|
| B1 | always | "Start from your *<purpose>* profile; the bean's process doesn't change the start." | research rule 1 (b) |
| B2 | always | "First crack expected near *<BT>* °C (*<basis>*), about *<mm:ss>* on *<profile>*; ± *<window>*." BT from `fc_history` (the prediction's: this profile's roasts, else every roast here); the window ±3 °C for a washed coffee, ±4 °C for natural, honey, wet-hulled and an unknown process (≈ ±30 s), the time from the profile's curve at that BT | research § 2, § 4, rule 2 (a, derived) |
| B3 | natural, honey, wet-hulled | "Naturals often crack quietly, with stray pops first: mark first crack at the first run of pops." | research § 3 (b) |
| B4 | the profile's auto DROP is by time | "This profile drops at *<time>* whatever first crack does; a first crack *<window>* off changes development as much. Drop by development for a first roast?" with a button | research § 5 (a) |
| B5 | wet-hulled, or density < 650 g/L, or moisture > 12 % | "Low density or wet: expect slower drying, about +30 s to first crack and more weight loss (+1–2 points at the same colour). A gentler start is an option, not tested here." | research § 6 (a, b) |
| B6 | supplier advice known | "Sweet Maria's: *<level>*; for espresso *<yes / at Full City / not recommended>*." Quoted, and only when the plan's purpose is espresso or the supplier names a level | research § 7 (b) |

**The window is ± 3 °C (washed) and ± 4 °C (the rest)** because ± 2 / ± 3 failed on a recorded
roast: a light espresso profile's eight roasts of two other coffees gave 195.9 °C, and a washed
Kenya's first roast cracked at 193.0 °C, 2.9 °C under. ± 4 °C ≈ ± 30 s at this roaster's rise,
the top of Yergenson's SD 16–31 s. An unknown process gets the wide window: nothing says it is
washed.

**B4's button** is "Drop by development…" on the bean page: it opens the profile's editor, where
the owner sets Auto DROP to development (a profile update replaces its points, so the page does
not rewrite a profile itself). The CLI prints the `roast profile edit … --auto-drop development`
line.

Not shown: the process's direction for first crack (sources disagree, research § 3), rest days
(unmeasured, research § 9; the roasted coffee's rest window already applies).

## After a roast, for the next one

Shown on History → the roast ("Next roast of this bean", the guide with `after`) and in
`roast review` while the bean has fewer than three finished roasts on this profile; from the
third the guide answers `stage: "settled"` with one line pointing at the profile's own history.
The count is taken as of the roast read: History → an old roast shows what was to change after
it then, counting the bean's roasts on the profile up to and with that one.

| # | When | Line | Source |
|---|---|---|---|
| A1 | first crack marked | "First crack came at *<BT>* °C, *<±s>* against the profile's *<mark>*; next roast of this bean expects it there." (the prediction already uses this bean's own first crack from its second roast) | research § 2, rule 8 (a, derived) |
| A2 | auto DROP by time or a manual drop | "Development *<m:ss>* (*<±s>* against the profile's *<m:ss>*): move the drop *<±s>* to keep it." | rule 7 (a, derived) |
| A3 | auto DROP by development | "Dropped *<m:ss>* after first crack as planned: nothing to move." | rule 7 |
| A4 | development or loss outside the level's band | the guidance line already made (`roast-guidance.md`), with "change development by *<±s>*" in steps of 15 s | rule 9 (b) |
| A5 | first crack after 80 % of the roast | "First crack at *<P>* % of the roast: late first crack usually means short development." (a hint, not amber) | rule 10 (b) |
| A6 | wet-hulled | the weight-loss line adds "wet-hulled beans lose 1–2 points more at the same colour" | rule 11 (derived) |
| A7 | always (a `Hint`, static) | "If it tastes…": sour, grassy → +15–30 s development or a higher drop; flat, bready → shorter development; tipping (dark tips) → a gentler start. One change at a time. | rule 12 (a, b) |

- **One change at a time between A2 and A4.** When A2 asks to move the drop (to keep the
  profile's development), A4's development line keeps its range but drops its "change
  development by" number; the loss line gives its one 15 s step only when development has no
  line of its own and A2 asks for nothing. A profile whose own development sits outside its
  level's range (a light espresso profile at 2:15 against the light level's 1:00–2:00) would
  show A4 on every roast; it gets a range of its own instead (`roast profile edit …
  --expect development=1:45-2:45`), and the level's stays 1:00–2:00.
- **How a roast dropped is read from its DROP mark's note** ("marked by auto DROP (…)"), not
  from the profile now: a profile switched to development later still reads its roasts by
  time as by time.
- **A5 fires often on a profile whose first crack is planned late:** at 77.7 % of the roast, a
  first crack 15 s late passes 80 %. That is why it is a hint (muted).
- **Lines carry °C as text** (the server writes them, as the analysis notes do); the step card's
  one line is built from the numbers in the display unit.

No line reads the roast's notes or cupping text (no LLM verdicts, `roast-guidance.md`).

## Where it lives

- **Server, one source of truth** (as `analysis.rs`): `crates/server/src/analysis/bean_guide.rs`,
  the rules as pure functions `before` and `next` over the bean, the profile, `predict::fc_history`
  (shared with the live prediction; the guide leaves no roast out) and, after a roast, its
  `Analysed` (`reports::roast::gather` + `analysis::analyse`). One endpoint in
  `routes/inventory.rs`: `GET /api/beans/{id}/guide?profile=&roaster=&after=<session>`: `before`
  lines when the bean has no finished roast on the roaster, else `next` lines from `after`
  (default: the bean's newest roast). The analysis response is unchanged. No migration: nothing
  new is stored.
- **Dashboard** reads those: `lib/components/beans/BeanGuidePanel.svelte` (the bean page's "First
  roast" / "Next roast" section, History → a roast's "Next roast of this bean"),
  `lib/beans/guide.ts` and `RoastStepCard.svelte` (the Roast page step card's one line).
- **Agent path:** `roast bean guide <bean> [--profile P] [--roaster R]`; `roast review` prints the
  next-roast lines (`next_roast`); the skill's "Reading a roast" gets "a new bean";
  `docs/cli.md`.

## Tests

- Rule functions: each line's condition and numbers (unit tests).
- Fixtures from recorded roasts: a washed Kenya's first roast (profile history from two other
  coffees' roasts) gives B2 within its actual first crack ± window; a roast dropped by auto DROP
  by time and one dropped by hand give A2. No recorded roast dropped by development, so A3 is a
  unit test.
- `api_shapes.json` for the guide (refusals and a bean with no profile); openapi.
- Browser: the bean page section and the step card line fit (`ui-check`, the Roast page never
  scrolls at 1280×720).

## Not doing

- Predicting first crack's direction from process, density or moisture (no evidence).
- Mapping Sweet Maria's City / City+ / Full City to this app's levels or Agtron (no published
  mapping, as the roast guidance's research on colour found).
- Changing a profile or a plan by itself; per-bean profile copies (the profile edit and
  development-based auto DROP cover it).
