# Roast analysis

After a roast you want to know how it went: when first crack came, how long development ran,
whether the rate of rise crashed or flicked, how closely the beans followed the profile and
how hard the heater worked. The roast analysis reads all of that from the recorded roast in
one go, and judges the numbers against what the profile, the roast level or your own earlier
roasts of the profile lead you to expect. Comparing roasts puts two or more side by side, each
number with its difference from a reference roast.

## In the dashboard

- **One roast**: History → the roast. The page shows the marks, the phases, development,
  weight loss, AUC, the RoR slope from first crack to drop ([statistics](statistics.md)) and
  how its numbers sit against the expected ranges.
- **Several roasts**: in History tick two or more roasts and press **Compare**. The Compare
  roasts page draws their curves on one chart (Align on CHARGE, turning point, dry end, first
  crack or drop), with a **Reference** to measure against and a **Consistency band**. It shows
  the first ten roasts you picked.
- **On paper**: **Compare report** on the Compare page, or Reports → Compare two roasts: the
  reference as A, the other roast as B, both curves on one chart and every number as A, B and
  B − A ([reports](reports.md)).

## What a roast read gives

All times are from CHARGE, temperatures in °C, RoR in °C/min.

| Part | What it says |
|---|---|
| Marks | CHARGE, turning point, dry end, first crack and its end, second crack, DROP: the time, BT, ET, RoR, heater and fan at each |
| Totals | BT at CHARGE, roast time, development time and DTR, weight loss, the highest BT, AUC |
| Phases | drying, Maillard and development: how long, their share of the roast, the BT gained, the mean RoR |
| RoR | the early peak, RoR at each minute, the lowest RoR late in the roast, RoR at DROP, and the slope from first crack to DROP |
| Against the profile | BT − profile at 0:30, 1:00, 1:30 and 2:00, the largest overshoot in the first two minutes, how closely BT tracked from the turning point to DROP (RMS, bias, largest gap), first crack and DROP against the profile's targets |
| Heater and fan | the heater in the first 30 s, its mean and highest, time at 90 % or more and at 100 %, how busy it was, heater and fan over the last minute, BT against the setpoint |
| Expected | weight loss, roast time, first crack, development, drop BT and ground colour, each within, below or above its range ([roast guidance](roast-guidance.md)) |
| Notes | a plain line for each number outside its range; "Little heater left late" when the heater averaged 90 % or more over the last minute; BT 5 °C or more above the profile in the first two minutes |

A roast compared with others adds about 25 of these numbers in rows, each with its difference
from the reference, and from when each roast's BT stays within a set distance of the
reference's (0.5 °C unless you change it): how closely it followed.

## From the command line

```bash
roast review latest
roast review '#12' --vs '#11'
roast compare '#10' '#11' '#12' --ref '#11' --within 1 --curve
roast report compare '#12' '#13'
```

A roast is its id, `latest`, or `#N`, its batch number (add `--roaster` when two roasters use
the same number). `roast compare` takes 2 to 8 roasts; `--curve` adds BT, RoR, heater and fan
every 30 s. Add `--json` for the whole answer. For a bean with fewer than three roasts on the
profile, `roast review` also says what to change next time ([a new bean](new-bean.md)).

## Limits

The analysis reads; it changes nothing. The dashboard still works out its own phases, AUC and
compare figures, so it can differ from `roast review` in small ways: its AUC follows Settings →
Roasting → Area under the curve.

<!-- site: end -->

## Design notes

For developers: how the roast analysis works and why. The site publishes only the part above.

The code: `crates/server/src/analysis/mod.rs` (the numbers),
`routes/analysis.rs` (the API), `bin/roast/commands/analysis.rs` (`roast review`, `roast compare`).

One reading of a roast for the API, the CLI and agents: what the roast report already judged
(`reports/roast.rs`, `expect.rs`), plus what was worked out by hand from the samples when
comparing four recorded roasts, #9–#12 (the overshoot after CHARGE, the RoR peak, the late RoR
dip with the heater near its top). An agent asked "how did that go?" or "compare it with the
last one" reads this instead of computing from `roast show --telemetry`.

All times are seconds from CHARGE, temperatures °C, RoR °C/min. RoR is the server's own (the
`rate_of_rise` stored with each sample, as the scope draws it); a roast without it (an import,
hand-posted samples) gets RoR computed from BT as the report draws it (a ±15 s slope).

## One roast: `GET /api/sessions/{id}/analysis`

| Field | What |
|---|---|
| `marks` | CHARGE, TP, DRY, FC, FCe, SC, DROP as marked: time, BT (the mark's own reading, else the nearest sample), ET, RoR (from 0:15), heater, fan |
| `charge_bt`, `total_s`, `dropped`, `development_s`, `dtr_pct`, `loss_pct`, `max_bt`, `auc` | `total_s` runs to DROP, else the last sample; `auc` as Complete stored it |
| `phases` | drying, Maillard, development: from, to, duration, % of the roast, BT gained (drying from TP, as Artisan), mean RoR |
| `ror.peak` | the highest RoR from TP (else 0:15) to 5:00 or FC: the early peak |
| `ror.by_minute` | RoR at 1:00, 2:00, … |
| `ror.late_min`, `ror.at_drop` | the lowest RoR in the last 2:30 before DROP (from FC on), and RoR at DROP |
| `ror.fc_drop_slope` | least-squares slope of RoR from FC to DROP, °C/min per minute |
| `profile` | with a profile: BT − profile at 0:30, 1:00, 1:30, 2:00 (`early`); the largest BT − profile in the first 2:00 (`early_peak`, the overshoot after CHARGE); RMS, bias and max from TP to DROP (as Complete stores them: `services::tracking_stats`); FC and DROP against the profile's targets |
| `control` | `heater_opening` (mean heater in the first 30 s), mean and max, `heater_high_s` (time at 90 % or more), `heater_saturated_s` (99.5 % or more), `heater_step_per_s` (mean \|change\| per second: how busy the loop was), `late_heater` / `late_fan` (means over the last minute), fan min and max, BT − setpoint after TP (mean ± sd) |
| `expected` | weight loss, roast time, FC, development, drop BT and ground colour judged `within` / `below` / `above` a range from the profile, the roast level or this roaster's own roasts of the profile (`roast-guidance.md`) |
| `notes` | plain lines: each judged number outside its range; "little heater left late" when the heater averaged 90 % or more over the last minute; an overshoot of 5 °C or more above the profile in the first two minutes |

## Several roasts: `GET /api/analysis/compare?roasts=a,b,c&reference=b&within=0.5`

2 to 8 roasts, each read as above (`roasts`), measured against the reference (the first unless
`reference` names another):

- `rows`: about 25 numbers (BT at CHARGE, heater in the first 30 s, BT − profile at 0:30, the
  early overshoot, TP, the RoR peak and when, RoR at 1:00 and 2:00, DRY, FC and its BT, DROP
  and its BT, development, DTR, the lowest late RoR, the FC → drop slope, heater and fan over
  the last minute, time at 90 % heater, profile RMS and bias, loss), each with its values and
  its difference from the reference's; rows no roast has are left out.
- `curve`: BT, RoR, heater and fan of every roast every 30 s from CHARGE to the longest DROP.
- `overlay`: per roast, from when its BT stays within `within` °C of the reference's (to the
  end of the shorter roast; null when it never settles), the RMS of the difference and the
  largest one.

## The CLI

```bash
roast review latest --roaster esp32_roaster_01
roast review '#12' --vs '#11'                      # and against #11
roast compare '#10' '#11' '#12' --ref '#11' --within 1 --curve
```

A roast is its id, `latest`, or `#N` (its batch number; `--roaster` when two roasters share
one). Text by default on a terminal, `--json` for the whole answer.

Two roasts on paper: `roast report compare '#12' '#13'` (or no roast: the newest two) is the
compare report ([`reports.md`](reports.md), "Compare two roasts"): these rows with A, B and
B − A, both curves and RoR on one chart, the phases on one time scale, the overlay's sentence.

## Checked against recorded roasts

On a copy of a production database the analysis gives what was found by hand:
#12 against #11 BT − profile +4.0 / +7.8 °C at 0:30, RoR peak 65.0 / 71.1, RoR at 1:00
35.5 / 29.9, profile RMS 0.39; #9 early peak +8.5 °C at 0:29, RMS 0.48, max 1.3, FC 8:03.
It also shows what the late RoR dip comes from: on #10–#12 the heater sits at 90 % or more
from about 4:00 to DROP (5:54–6:12 of a 10:05 roast) while fan assist takes the fan from 100
to 79 %.

## Not yet

The dashboard still computes its own phases, AUC and comparator statistics in TypeScript
(`lib/roast/*.ts`, `lib/guidance/*.ts`). It moves to this endpoint once a fixture test shows
both give the same numbers on recorded roasts; until then the two can differ in small ways
(the dashboard's AUC base and start come from Settings → Roasting).
