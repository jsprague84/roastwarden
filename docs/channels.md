# Declared channels

A roaster can send more than bean and environment temperature: an exhaust or inlet probe, a
TC4's third and fourth thermocouple, a gas pressure. Those values are shown live but not kept.
Declare one as a **channel** and Roastwarden stores it with each roast, draws it on the scope,
the history chart and the PDF roast report, lets an alarm watch it, and exports it to Artisan.
On a gas drum the exhaust temperature is often the second most useful curve.

## Declaring a channel

Roasters → the roaster → Control → Setup → **Channels** → Edit, then **Add a channel**:

| Field | What it is |
|---|---|
| Key | the name the value has in the roaster's readings (`t3`, `exhaust`). The editor offers the extra keys the roaster sends right now, or type one |
| Label | what the chart, alarms and Artisan call it, up to 24 characters ("Exhaust") |
| Unit | °C (drawn on the temperature axis, in your display unit), % (on the output axis), or **other…**: a unit of your own, up to 8 characters (`kPa`), drawn on a scale of its own |
| Unit, top | for a unit of your own, the top of its scale; left empty (auto), the highest value seen |

**Save channels**. Up to 8 per roaster, each key once. Nothing is declared by default. A
machine added from Artisan's catalogue arrives with its extra curves declared
([roasters from Artisan's catalogue](artisan-machines.md)). Changing the roaster's kind keeps
its channels. Temperatures are kept in °C: a probe that reads °F is converted by the mapping,
not declared in °F.

**Getting the value into the readings.** A key must be one the roaster sends and not one of
Roastwarden's own fields (BT, ET, heater, fan, setpoint, drum, damper…). A device on
Roastwarden's own names just adds the field to its JSON; any other device names it in its
mapping, as an extra name for a JSON field, a TC4 column or a Phidgets input, or a Modbus
register with no role, by its name ([mapping a device's own names](ingest-mapping.md)). For
example `roast roaster map <roaster> --column ambient,et,bt,heater,fan,exhaust` reads a TC4's
sixth column as `exhaust`.

## Where a channel shows

- **Scope and history**: a dashed line per channel, with a legend entry that hides and shows
  it. The hover readout does not list channel values.
- **PDF roast report**: a °C channel is a line on the chart; a % channel or one of your own
  unit joins the heater and fan strip.
- **Alarms**: Settings → Alarms, condition **Channel above** or **Channel below**, the
  threshold in the channel's unit. A rule naming a channel no roaster declares never fires, and
  the editor says so.
- **Artisan export**: the `.alog` gets one Artisan extra device per pair of channels. With no
  channel declared it has no extra devices, so loading it leaves your Artisan setup alone.
  Importing an `.alog` does not read its extra devices.
- **A roast keeps the channels it started with**: declaring or removing one later does not
  change how earlier roasts are drawn or exported.

From the command line, `roast roaster show <roaster>` lists the declared channels and
`roast review <roast>` prints each channel's value at CHARGE, first crack and DROP. Declaring
channels is in the dashboard only.

<!-- site: end -->

## Design notes

For developers: how declared channels are stored, drawn and exported, and why. The site
publishes only the part above.

Channels build on the roaster setup ([`roaster-setup.md`](roaster-setup.md)) and ingest mapping
([`ingest-mapping.md`](ingest-mapping.md)), which names a device's extra values.

## Why

A roaster's extra values reach the server: a TC4's third and fourth probes (`t3`, `t4`), an
ambient probe (`ambientTemp`), an exhaust probe or burner pressure named by an ingest mapping.
They go to the ring and the live WebSocket as extras (`Sample.extras`). Undeclared, they are lost
when the roast ends: `session_telemetry` keeps BT, ET, RoR, heater, fan and setpoint in columns
and an allowlist of nine control fields in `extras` (`telemetry.rs`, `control_extras`). The scope
draws BT, ET, RoR, heater and fan; alarms know BT, ET, RoR and the profile delta; the `.alog`
export writes no `extra*` keys unless channels are declared. An exhaust temperature is the second
most useful curve on a gas drum roaster; a declared channel keeps it.

## What it is

Each roaster's setup has **channels**: the extra values the owner wants kept, each declared
once.

```json
"channels": [
  { "key": "t3", "label": "Exhaust", "unit": "C" },
  { "key": "burner_kpa", "label": "Gas", "unit": "kPa", "max": 4 }
]
```

| Field | Meaning |
|---|---|
| `key` | The sample's field name (`^[A-Za-z][A-Za-z0-9_]{0,31}$`); not a typed field (`beanTemp`, `envTemp`, `heater`, `fan`, `setpoint`, `rateOfRise`, …) nor `drum` / `damper` (outputs) |
| `label` | What the dashboard, alarms and Artisan call it, 1–24 characters |
| `unit` | `C` (a temperature: drawn on the temperature axis, shown in the owner's unit; `°C` and `c` read as `C`), `%` (drawn on the output axis), or any other short text (1–8 characters: drawn on its own scale, `max` sets it). `F` / `°F` is refused: temperatures are kept in °C, and the ingest mapping converts |
| `max` | Optional top of the scale for a non-°C, non-% channel, above 0; default the highest value seen. Dropped on a `C` or `%` channel (they use the shared axes) |

- At most 8 channels; keys unique. Stored in `roaster_setup.channels` (JSON, migration 048,
  nullable: NULL is none). The setup `PUT` takes `channels` (as the rest of the setup,
  [`roaster-setup.md`](roaster-setup.md), "Tokens and the setup"); a `PUT` without it keeps the
  saved list, `[]` clears it (stored as NULL).
- Nothing is declared by default, on any roaster. A kind change keeps the channels: they are
  the roaster's, not the kind's.
- The control fields (`heaterApplied`, …) may be declared: they are stored anyway, and declaring
  one draws it.
- Drum and damper are not channels: as outputs they are stored and drawn with the heater and fan
  when declared.
- **The virtual roaster** keeps its fixed setup: it declares no channels and its `PUT` is
  refused, so channels are tried on a roaster with a setup row (the tests use `tc4`).

**Where it lives.** `src/roasters/channels.rs`: the `Channel` shape (`key`, `label`, `unit`,
`max`), `validate` (the 400s), and the readers (`value_in` a payload, `stored` a sample's
`extras`, `for_device` from the table, `for_roast` a roast's own list). The list is in the setup:
`RoasterSetup.channels` (always a list, `[]` for none), `SetupChange.channels` (left out keeps the
saved list), `Setups::channels(device)` in memory.

## Each roast keeps its channel list

`roast_sessions.channels` (JSON, nullable, migration 049) holds the roaster's declared channels
as they were when the roast started recording (`start_session`, which is CHARGE by the clock):
`[]` when none were declared, so a channel declared later does not reach back, and changing a
roaster's channels does not change how its earlier roasts are drawn or exported.

`channels::for_roast(db, roast, device)` reads the roast's own list; NULL (a roast recorded
before the column existed, or one not started yet) falls back to the roaster's list now.
`GET /api/sessions/{id}` returns it as `channels`, and the history page, the PDF roast report,
`roast review` (`reports/roast.rs`'s data) and the Artisan export all read that list. The live
scope and alarms read the setup in memory (a roast in progress records what is declared now).
Tested in `channels.rs` (`a_roast_keeps_the_channels_it_started_with`).

## Where a declared channel goes

1. **Stored with the roast.** `TelemetryService` holds the `Setups` (built before it in
   `app/build.rs`) and adds each declared key's numeric value to `control_extras`, so
   `process_telemetry` writes it into `session_telemetry.extras` beside the control fields,
   under its key (from the in-memory setup map, as the gate's, so a sample does no SQL lookup).
   An undeclared extra stays live only. A roast's telemetry API returns `extras`, so the
   history and reports read it back.
2. **On the scope.** Each declared channel is a dashed line, live and in a past roast: `C` on
   the temperature axis (converted to °F for an owner in °F), `%` on the output axis, other units
   on a right-hand scale of their own. Series come after the fixed ones (`CH0 = 17`), colours
   from `lib/roaster/channels.ts` (`CHANNEL_COLORS`, apart from BT, ET, RoR, heater, fan, SP); the
   chart is rebuilt when the channel set changes. A legend entry hides and shows each; the
   toggles are kept per browser by key (`roastwarden.scope.channels`). The hover readout has no
   channel values.
3. **History chart**: °C channels on the temperature axis, % on the output axis, another unit on
   a hidden axis of its own (0 to its top; the tooltip shows the raw value).
4. **PDF roast report** (`reports/roast.rs`): a `C` channel is a line on the chart (`ch0`…`ch7`
   in `report.typ`'s palette, each its own dash); a `%` channel joins the heater and fan strip;
   another unit joins the strip scaled 0..top, tagged `Gas (0–4 kPa)`.
5. **Alarm conditions.** `channel_above` and `channel_below` with a `channel` key
   (`AlarmRule.channel`) and the threshold in the channel's unit; a `C` channel's threshold
   converts like BT's. A channel rule without the key is skipped at load (logged). The engine
   reads the device's declared channels from the in-memory setups only when an enabled rule is a
   channel rule; `Ctx.channels` holds the declared ones present in the sample, so an undeclared
   key never fires. Rules are global, so the check is in the Settings → Alarms editor: the
   channel picker offers every roaster's declared channels (and says which roasters), and a rule
   naming a channel no roaster declares shows "this alarm never fires". Artisan `.alrm` export
   skips channel rules with a note.
6. **Artisan export.** When channels are declared, the `.alog` gets one Artisan extra device per
   pair of channels, written as Artisan writes it: device `25` (`+Virtual`, what Artisan's own
   Cropster import uses for curves from a file), `extradevices`, `extraname1/2`, `extratimex`
   (the roast's `timex`), `extratemp1/2` (-1 where a sample has none, one decimal),
   `extramathexpression1/2` (`""`), `extradevicecolor1/2`, `extraLCDvisibility1/2`,
   `extraCurveVisibility1/2`, `extraDelta1/2` (false), `extraFill1/2` (0),
   `extraNoneTempHint1/2` (false for a `C` channel). An odd channel's partner is "Extra 2", all
   -1, hidden (as the Cropster import pads). Artisan (`main.py`, `setProfile`) backs up the
   user's extra-device settings and restores them on RESET, so loading such a file does not lose
   them. Line styles, markers and widths are left to Artisan's defaults. With no channel
   declared the export has no `extra*` keys at all (`profiles.md`, "Compatibility notes": an
   empty list would replace the user's extra-device setup). Import ignores them. Profile exports
   (`.alog` of a profile) carry no channels.
7. **The agent path.** `roast roaster show` lists the channels; `roast review` prints
   `channels` (key, label, unit, `charge`, `first_crack`, `drop`: the nearest stored value within
   5 s) and a line per channel (`analysis.rs`). `roast show` has no channel lines (its
   `--telemetry` has the raw `extras`). Declaring them is the setup `PUT`.

## Dashboard

- The Setup panel has a **Channels** block (list, add, edit, remove), as Outputs: key from the
  live sample's extras (a picker of the keys the roaster sends now, or typed), label, unit, max.
- The scope's legend, the history page's chart and the PDF roast report draw them. The Roast
  page still fits a laptop window with no scroll (`ui-check.mjs`, `drive-check.mjs`): channels
  add legend entries, not panels.

## Tests

- The setup: channels validate (keys, labels, units, 8 at most, no typed field), stored and
  read back; a `PUT` without `channels` keeps them.
- Storage: a declared extra is in the stored sample's `extras`; an undeclared one is not; the
  control fields are kept as before.
- Alarms: `channel_above`/`channel_below` hit and miss, an undeclared channel never fires.
- Artisan: with two channels the export has one extra device whose curves match the samples;
  with none it has no `extra*` key (the existing tests).
- Scope data (vitest): declared channels become series with the right axis and unit conversion.
- A roast keeps the channels it started with.
