# Roast statistics

Three numbers that help you roast the same coffee the same way again: the area under the bean
temperature curve (how much heat the beans saw), the slope of the rate of rise through
development (whether RoR keeps falling or flicks up), and how far a set of roasts spread
around their mean (how repeatable you are).

## AUC (area under the curve)

BT summed over time, in °C·min. It tracks roast degree better than the drop temperature alone.

- It runs from a landmark to DROP and counts only BT above a base. Set both under Settings →
  Roasting → Area under the curve (AUC): **From** CHARGE, turning point, dry end or first
  crack, and **Base** (0 counts everything). Many Artisan users start at the turning point.
- **Per phase**: drying (CHARGE to dry end), Maillard (dry end to first crack) and
  development (first crack to DROP). With the start at CHARGE they add up to the total.
- **Live**: on the Roast page's Phases tab, the roast's AUC so far, the profile's at the same
  roast time (amber when they differ by more than 5 % or 30 °C·min) and the profile's at its
  end as the target.
- **After the roast**: History → the roast shows it with its phases; Compare roasts shows it
  side by side, with each roast's difference from the reference.

Each roast stores its AUC with the setting it was completed under. History, the roast report
and Compare work it out again from the curve with the current setting.

## RoR slope, first crack to drop

How fast the rate of rise falls through development, in °C/min per minute: a straight line
fitted through RoR from first crack to 15 s before DROP (so the drop itself does not count).
It needs first crack and DROP at least 45 s apart. Negative means RoR is falling, the usual
aim; near zero or positive means a flick. It shows on the roast's page in History (RoR slope
FC → drop), in its report, and in Compare roasts.

## Consistency (Compare roasts)

In History tick the roasts to compare and press **Compare**:

- **Consistency band**: draws the mean ± σ (standard deviation) of BT and of RoR across the
  roasts shown, every 5 s where two or more have data. Each roast counts from its CHARGE to
  its DROP, aligned as the page's **Align on** says.
- **BT spread**: the band's σ averaged over the roast, and its largest value: one number for
  how repeatable the set is.
- **Mean ± σ** row under the table: time, first crack, DTR, the RoR slope, drop temperature,
  AUC and weight loss.

From the command line, `roast compare` reads the same roasts against a reference
([roast analysis](analysis.md)).

<!-- site: end -->

## Design notes

For developers: how the roast statistics work and why. The site publishes only the part above.

Pure logic in
`apps/dashboard/src/lib/roast/stats.ts` (tested); the server's stored AUC in
`services.rs` (`auc_between`, `compute_auc`) follows the same rule.

## AUC (area under the curve)

BT summed over time, in °C·min: one number for how much heat the beans saw, which
tracks roast degree better than the end temperature alone.

- From the **start landmark** (Settings → Roasting → AUC: CHARGE, turning point, dry end or
  first crack; `auc_start_event`, CHARGE when unset or not marked) **to DROP** (else the last
  sample). Only BT above the **base** counts (`auc_base_temp`, °C, default 0); a segment that
  crosses the base counts the part above it.
- **Per phase**: drying (CHARGE → DRY), Maillard (DRY → FC), development (FC → DROP), each
  clipped to the start; with the start at CHARGE they add up to the total.
- Before 0.3.0 the server integrated from the start of recording and through the
  cooling: on a real roast about 100 °C·min too much. Roasts completed before keep their
  stored value; History, the report and Compare compute it from the curve with the current
  settings.
- **Live** on the Roast page (Phases tab, `LiveAuc.svelte`): the roast's AUC so far, the
  profile's at the same roast time (amber when they differ by more than 5 % or 30 °C·min) and
  the profile's at its end as the target. With a start other than CHARGE the profile's start
  is its own landmark (its first-crack target, or its landmark events).

## RoR slope, first crack to drop

How fast the rate of rise falls through development, °C/min per minute: a least-squares
line through the report's RoR (±15 s slope) from FC to 15 s before DROP (so the dump does
not count), when FC and DROP are 45 s apart or more. Negative is falling, the usual aim; near
zero or rising means a flick. On a roast's page, its report and in Compare.

## Consistency (Compare)

- **Consistency band**: a switch on the Compare page draws mean ± σ (sample standard
  deviation) of BT and of RoR across the roasts shown, every 5 s where two or more have
  data, as a shaded band with the mean dotted. Each roast counts from its CHARGE to its
  DROP only, aligned as the page aligns them.
- **BT spread**: the band's σ averaged over the roast and its largest value, next to the
  switch: one number for how repeatable the selection is.
- **Mean ± σ row** under the table: time, first crack, DTR, the RoR slope, drop
  temperature, AUC and weight loss.
- Columns added: RoR slope FC → drop, AUC (difference to the reference in brackets) and AUC
  by phase.
