# Ingest mapping and command templates

A device keeps its own field names, units, topics and command syntax; its connection maps
them. A Tasmota board that publishes `{"MAX31855":{"Temperature":415.2}}` in °F on
`tele/<name>/SENSOR`, an ESPHome node with one number per sensor topic, a serial board that
prints its own CSV, a Modbus controller with its own register layout, a roaster that speaks
Artisan's WebSocket protocol: each is read without reflashing. A connection with no mapping
expects Roastwarden's own names ([connect your roaster](connect-your-roaster.md)).

## Using it

- **Dashboard**: Roasters → the roaster → Overview → a connection's **Mapping** (in Add
  roaster, the Check step's **Map its fields**). Choose what the device sends (°C or °F), then a
  row per field: Role or name, Path, Scale, Offset, Unit and, on MQTT, Own topic. **Command
  templates** below: Command, Topic, Payload, Scale, Decimals. Under it the newest message the
  device sent and what the mapping makes of it, before anything is saved. A Modbus roaster has
  no mapping: give each register its role in the register map's Role column.
- **Command line**: `roast roaster map <roaster>` with `--field`, `--units`, `--topic`,
  `--column`, `--register`, `--command` and `--command-topic`. Add `--try` to see what the
  mapping makes of the newest message without saving it. Fields you give replace theirs and
  the rest stay; `--field KEY=` takes one out.

## Roles and units

A role says what a value is. The mapping and the Modbus register map use the same words:

| Role | What | Unit |
|---|---|---|
| `bt`, `et` | bean and environment temperature | temperature |
| `ambient` | room temperature | temperature |
| `ror` | the device's own rate of rise (Roastwarden still computes its own from BT) | temperature per minute |
| `heater`, `fan` | heater or burner, fan or air | % |
| `heater_applied` | the heater % after the device's interlocks | % |
| `drum`, `damper` | drum speed, damper | % |
| `setpoint`, `mode` | an onboard PID's target, and 0 manual / 1 auto | temperature, 0 / 1 |
| `heater_enable` | heater power | 0 / 1 |

Any other name (letters, digits and `_`, up to 32 characters) is an **extra channel**: `t3`,
`exhaust`, `burnerGas`. It is shown live; declare it to keep it with each roast
([extra channels](channels.md)).

**Units**: the device sends °C (default) or °F. With °F, the temperature roles are converted
to °C, and a setpoint is sent back in °F. An extra channel is converted only when its own unit
says °F.

## JSON fields (MQTT and WebSocket)

```json
"ingest": {
  "units": "F",
  "fields": {
    "bt": "MAX31855.Temperature",
    "et": { "path": "probes[1].value" },
    "heater": { "path": "out.burner", "scale": 0.392157 },
    "exhaust": { "path": "probes[2].value", "unit": "F" }
  }
}
```

- A path is dot-separated keys with `[n]` indexes (`data.BT`, `probes[0].value`; a leading `$.`
  is allowed); `$` is the whole message, for a plain number.
- A value may be a number, a numeric string, a boolean, or `on` / `off` / `true` / `false`;
  anything else leaves the field out of that sample.
- The value is `raw × scale + offset` (defaults 1 and 0), then converted to °C.
- Only mapped fields pass; a message that gives neither BT nor ET is not a sample. At most 32
  fields, and at least one of `bt` and `et`.

## MQTT topics

| Setting | Default | What it does |
|---|---|---|
| Topic prefix | `roaster/<id>` | commands go to `<prefix>/control/<command>` |
| Telemetry topic | `<prefix>/telemetry` | the topic whose messages are samples |
| Own topic (per field) | none | read this field from its own topic (ESPHome, Home Assistant style) |

With per-field topics, Roastwarden keeps each field's newest value and makes one sample when
BT's message arrives (ET's when there is no BT), from values no older than 10 s. Topics take no
wildcards, and a topic belongs to one roaster. Presence and caps stay on `roaster/<id>/status`.

## WebSocket requests

Roastwarden dials the device. A device that pushes needs only the fields. One that answers
requests (Artisan's WebSocket protocol) takes a poll: a message sent every interval (at least
200 ms), with `{id}` replaced by a counter. The preset `artisan-websocket` sets
`{"command":"getData","id":{id},"roasterID":0}` every second. A device that dials the server
instead speaks Roastwarden's own names and takes no mapping.

## TC4 columns

- **Units**: with °F the server sends `UNITS;F` at start and converts.
- **Columns**: the reply's layout by position, a role or an extra name per column, empty to skip
  one: `--column ambient,et,bt,heater,fan`. `-1` still means no probe.
- An extra column's unit: a column named after a declared °C channel follows the device's units;
  another unit goes after a colon (`exhaust:F`, `exhaust:C`, `gas:none` for as printed).

## Phidgets inputs

A Phidgets board ([Phidgets boards](phidgets.md)) maps its inputs `ch0` to `ch7`, with scale and
offset; it always reports °C. With no mapping BT is `ch0` and ET `ch1`, as in Artisan;
`roast roaster map <roaster> --field exhaust=ch2` adds a third. It only reads: no templates.

## Modbus registers

Each register in the map gets a role, so its name is free: `--register
KIND:ADDRESS[@SLAVE]:TYPE[*SCALE]:ROLE[:UNIT]`, for example `holding:0:int16*0.1:bt:°C` (BT in
tenths). A temperature register in °F is converted. A command is written to the writable
holding register or coil with its role; a role is on at most one readable and one writable
register, and `emergency_stop` is write only. `@SLAVE` puts a register on another controller of
the same bus.

## Command templates

```json
"commands": {
  "heater": { "topic": "cmnd/roaster/Dimmer", "payload": "{value}", "decimals": 0 },
  "drum":   { "payload": "OT2;{value}" },
  "emergency_stop": { "payload": "STOP" }
}
```

- Commands: `setpoint`, `heater`, `fan`, `drum`, `damper`, `mode`, `heater_enable`,
  `emergency_stop`. PID gains, feedforward, the lease and the RoR limit take no template.
- `{value}` is the value (outputs in %, a setpoint in the device's unit, mode 0 / 1, heater
  enable 1 / 0, emergency stop 1), times `scale` (default 1), rounded to `decimals` (0–3,
  default 1, a trailing `.0` dropped). No payload writes the value alone; mode writes `auto` or
  `manual`.
- MQTT publishes `payload` on `topic` (default `<prefix>/control/<command>`). WebSocket sends
  the payload as the frame; `{id}` in it is a request number. Serial sends it as the line;
  `\n` splits several lines.
- A command with no template goes out in Roastwarden's own syntax. A TC4 has no standard drum or
  damper line, so those need a template.
- A template only says how a command is written, never whether it is sent: that is the control
  level and the outputs ([roaster kind and control level](roaster-setup.md)).

<!-- site: end -->

## Design notes

For developers: how ingest mapping and command templates work and why. The site publishes only
the part above.

The mapping lets a device speak **its own** field names, units, topics and command syntax:
configuration adapts the server to the device rather than the reverse.

## Why

Without a mapping, an MQTT or WebSocket device must send Roastwarden's exact JSON (`beanTemp`,
`envTemp`, `heater`, `fan`, … in °C) on `roaster/<id>/telemetry`, and take commands on
`roaster/<id>/control/<command>`. A Tasmota board publishes `tele/<name>/SENSOR` with
`{"MAX31855":{"Temperature":415.2}}` in °F; an ESPHome node publishes one plain number per
sensor on `<node>/sensor/<name>/state`; Artisan's WebSocket roasters answer
`{"command":"getData"}` with `{"data":{"BT":…,"ET":…}}`. None of them could be used without
reflashing. A serial device that prints its own CSV would be read only if it looked like a TC4,
and a Modbus register would be a bean temperature only if its name were one of a list. A drum
roaster's drum speed or damper needs commands of its own, which the contract's `DeviceCommand`
carries for that reason (below, "Drum and damper").

## What it is

Two optional keys on a connection's config, the same shape for every protocol:

- **`ingest`**: how to read the device's telemetry into Roastwarden's own field names (below);
- **`commands`**: how to write each command in the device's syntax (below).

A connection without them expects Roastwarden's own names and syntax. Nothing downstream
changes: the mapping produces the same JSON (`beanTemp`, `envTemp`, `heater`, `fan`, … in °C)
that `process_telemetry` gets from a device that speaks them, so RoR, the ring, storage, alarms,
replay and the dashboard are untouched.

Plus two commands for drum roasters: **drum speed** and **damper**, declared as outputs in the
roaster's setup ([`roaster-setup.md`](roaster-setup.md), "Outputs"), carried by every
transport, gated like the fan.

Where it lives: `crates/server/src/transports/ingest.rs` holds the pure parts (roles, paths,
units, the per-field topic assembler, TC4 columns, templates, the MQTT route table
`MqttRouter`); `transports/pollers/mod.rs`'s `Pollers` keep the route table in step with the
database (`sync_device`, `sync_all`) beside the pollers; `app/consumer.rs`'s MQTT consumer asks
the table first; `transport.rs`'s sinks render commands.

### Roles

A role is what a value means. The mapping and the Modbus register map use the same words:

| Role | Field it becomes | Kind | Notes |
|---|---|---|---|
| `bt` | `beanTemp` | temperature | |
| `et` | `envTemp` | temperature | |
| `ambient` | `ambientTemp` | temperature | an extra, as the TC4 reports it |
| `ror` | `rateOfRise` | temperature rate | the device's own RoR; the server's stays canonical (`deviceRateOfRise`) |
| `heater` | `heater` | % | heater or burner |
| `heater_applied` | `heaterApplied` | % | |
| `fan` | `fan` | % | fan or air |
| `drum` | `drum` | % | drum speed |
| `damper` | `damper` | % | |
| `setpoint` | `setpoint` | temperature | |
| `mode` | `controlMode` | 0 manual / 1 auto | |
| `heater_enable` | `heaterEnable` | 0 / 1 | |

Any other key (`^[A-Za-z][A-Za-z0-9_]{0,31}$`, not a role and not one of the fields above) is an
**extra channel** and keeps its own name (`t3`, `exhaust`, `burnerGas`): live on the ring and the
WebSocket as other extras; a declared one is stored with each roast ([`channels.md`](channels.md)).
An extra may not take the name of a field Roastwarden fills: the roles' fields and
`rateOfRiseEt`, `deviceRateOfRise`, `timestamp`, `uptime`, `uptimeMs`, `emergencyStop`.

### Units

`ingest.units` is the device's temperature unit, `C` (default) or `F`. It converts the
temperature roles (`bt`, `et`, `ambient`, `setpoint`: `(v − 32) × 5/9`; `ror`: `v × 5/9`). An
extra channel is converted only when its field says `"unit": "F"` (or `"C"`: no change); `unit`
is for extras only (400 on a role). The command side follows: with `F`, a setpoint goes out in
°F (`v × 9/5 + 32`), and so does the RoR limit's temperature (its rate `× 9/5`).

## `ingest` for JSON (MQTT, the WebSocket client)

```json
"ingest": {
  "units": "F",
  "fields": {
    "bt": "MAX31855.Temperature",
    "et": { "path": "probes[1].value" },
    "heater": { "path": "out.burner", "scale": 0.392157 },
    "fan": { "path": "Fan", "scale": 1, "offset": 0 },
    "exhaust": { "path": "probes[2].value", "unit": "F" }
  }
}
```

- A field is a **path** string or an object `{ path, scale, offset, unit, topic }`. `topic` (on
  a field or a template) is MQTT's only: 400 elsewhere.
- A path is dot-separated keys with `[n]` indexes: `data.BT`, `probes[0].value`; a leading `$.`
  is allowed (`$.data.BT`); `$` or an empty path is the whole payload (a plain number). Keys
  that contain a dot are not reachable (rare).
- A value is a number, a string that parses as one, a boolean (1 / 0), or one of the strings
  `on` / `off` / `true` / `false` (1 / 0: ESPHome's switch states); anything else (null,
  missing, other text) leaves the field out of that sample.
- The value is `raw × scale + offset` (defaults 1 and 0), then converted to °C, then rounded to
  0.001.
- With a mapping the sample has **only the mapped fields**; nothing else of the payload passes.
  A message, frame or TC4 line whose mapping gives neither BT nor ET is no sample (Tasmota's
  other messages, an Artisan answer to a command).
- At most 32 fields; at least one of `bt` and `et`.

## MQTT

The MQTT connection's config:

| Key | Default | Meaning |
|---|---|---|
| `topic_prefix` | `roaster/<device_id>` | Commands go to `<prefix>/control/<command>` |
| `telemetry_topic` | `<prefix>/telemetry` | The topic whose messages are samples |
| `ingest` | none | The mapping above; a field may also name its own `topic` |
| `commands` | none | Command templates (below) |

- **Per-field topics** (ESPHome, Home Assistant style): a field with a `topic` is read from that
  topic's messages (with no `path`, the plain payload, `$`). The server keeps the newest value
  of each field and makes **one sample when the message carrying `bt` arrives** (`et` when
  there is no `bt`), from every field's newest value no older than 10 s. When all fields come
  from the telemetry topic, one message is one sample.
- **Subscriptions follow the database.** A route table (device → its topics, mapping and
  templates) is built at start and rebuilt for a device whenever its connections change (the
  same calls that sync the pollers); the server subscribes to each topic it lists outside
  `roaster/` (the server's `roaster/#` covers the rest, and an overlapping subscription makes
  the broker deliver twice). A topic in the table is handled by its device's mapping alone.
  Everything else goes the default way (`roaster/+/telemetry`, auto-discovery included). The
  MQTT client has no unsubscribe: a topic dropped from a mapping stays subscribed until a
  restart, and its messages are ignored (they are in no route and not under `roaster/`).
- A connection is in the table when it has an `ingest`, a `commands`, or a prefix or telemetry
  topic other than the defaults. With only the topics moved (no `ingest`) the messages are
  Roastwarden's own JSON, passed through. A device with several enabled MQTT connections uses
  the highest-priority one.
- Checks (400): topics have no wildcards (`+`, `#`), are 1–256 characters; a topic another
  device's connection already claims is refused naming that device, and so is a topic
  `roaster/<other roaster>/…` of a roaster that exists.
- `status` and caps stay on `roaster/<id>/status` (the contract's own devices).

## The WebSocket client

The outbound client's config (`url`, `reconnect_interval_ms`) takes `ingest`, `commands` and:

```json
"poll": { "message": "{\"command\":\"getData\",\"id\":{id},\"roasterID\":0}", "interval_ms": 1000 }
```

for devices that answer requests rather than push (Artisan's WebSocket protocol): the client
sends `message` (1–1024 characters) every `interval_ms` (≥ 200), `{id}` replaced by a counter
that runs for the task's life (it does not restart at 1 on a reconnect). Each text frame
received goes through the mapping. The inbound device socket (`/ws/device/{id}/telemetry`) is
Roastwarden's own protocol and takes no mapping.

## Serial TC4

`ingest` on a TC4 connection (`fields` is not taken; `columns` is the serial form):

- `units`: `F` sends `UNITS;F` at start (instead of `UNITS;C`) and converts; for a device that
  ignores `UNITS` and prints °F. Without `columns` it converts the layout's ambient, ET, BT,
  `t3`, `t4` and SV; with `columns`, the temperature roles (and the RoR rate).
- `columns`: the reply's layout by position, roles or extra names, `""` to skip a column, e.g.
  `["ambient", "et", "bt", "heater", "fan"]`. It replaces `reply_format`'s layout, for a device
  that prints its own CSV. `-1` still means "no probe".
- **An extra column's unit**: a column naming one of the roaster's declared channels whose unit
  is `C` ([`channels.md`](channels.md)) is a temperature: it follows `units` (with `F` it is
  converted to °C like BT and ET) and `-1` is no probe. Any other extra column passes as
  printed. A column may say its own unit after a colon, which overrides that: `exhaust:F` (°F,
  converted), `exhaust:C` (°C), `gas:none` (as printed). A unit on a role (`bt:F`) is a 400:
  roles follow `units`. The poller reads the declared °C channels from the setup in memory
  before each `READ` (`Setups::temp_channels`), so declaring one takes effect at once; the
  mapping preview (`roast roaster map --try`) reads them too.

## Phidgets

A Phidgets board's connection ([`phidgets.md`](phidgets.md)) takes `ingest.fields` only: each
path is one of the board's temperature inputs, `ch0` to `ch7`; a field may take `scale` and
`offset`, never `unit`, `topic` or `units` (a Phidget reports °C). With no `ingest` it reads BT
`ch0` and ET `ch1`, as Artisan reads a 1048 or a TMP1101; `roast roaster map --field
exhaust=ch2` adds to that. The inputs the mapping names are the ones opened. `commands` is
refused: the connection only reads.

```json
"ingest": { "fields": { "bt": "ch0", "et": "ch1", "exhaust": "ch2" } }
```

## Modbus TCP and RTU

`ingest` and `commands` on a Modbus connection are refused: each register in the map gets an
optional **`role`** instead (migration 047, the nullable column `modbus_register_maps.role`):

- Reading: a register with a role is reported as the role's field (`bt` → `beanTemp`); one
  without is read by its name, through the names table. A temperature role in a register whose
  `unit` is `°F` or `F` is converted. A role's read-only register is the reading when it also
  has a writable one.
- Writing: a command's register is the writable holding register or coil with the command's role
  (`heater`, `fan`, `drum`, `damper`, `setpoint`, `mode`, `heater_enable`, `emergency_stop`),
  then the name list. Names still work for registers **with no role**; a register with a role
  answers only to it. Drum and damper by name: `drum_cmd`, `drum_speed`, `drum`; `damper_cmd`,
  `air_damper`, `damper`. A setpoint register in °F gets the value in °F.
- A role is on at most one readable register and at most one writable one (400 otherwise);
  `emergency_stop` is a write role only, and on a register that is not a writable holding
  register or coil it is a 400. A read role on a writable register is allowed (it is read).
- The default register map (no map saved) has no roles.
- **A register's own slave** (migration 053, [`artisan-machines.md`](artisan-machines.md)):
  `unit_id` 1–247 puts it on another controller of the bus than the connection's `unit_id`
  (null: the connection's). Reads are planned per slave (the connection's first), a command is
  written to its register's slave and the link goes back to the connection's after it. The
  newest raw poll shows `unit_id` on such a register. `roast roaster map --register
  holding:4096@2:int16:et`.
- The register map check (`check_register_map`) knows the roles; the dashboard's map editor has
  a Role column.

## `commands`: command templates

```json
"commands": {
  "heater": { "topic": "cmnd/roaster/Dimmer", "payload": "{value}", "decimals": 0 },
  "drum":   { "payload": "OT2;{value}" },
  "emergency_stop": { "payload": "STOP" }
}
```

- Keys: `setpoint`, `heater`, `fan`, `drum`, `damper`, `mode`, `heater_enable`,
  `emergency_stop`. PID gains, feedforward, the lease and the RoR limit are the contract's and
  take no template.
- `{value}` is the command's value: heater, fan, drum, damper in %; setpoint in the device's
  unit; mode 0 manual / 1 auto; heater enable 1 / 0; emergency stop 1. It is `× scale`
  (default 1) and rounded half away from zero to `decimals` (default 1, a trailing `.0`
  dropped, as Roastwarden's own %; 62.5 with 0 decimals → `63`).
- A template with no `payload` writes `{value}` alone (scaled, rounded); mode writes its word
  (`auto` / `manual`).
- MQTT: published on `topic` (default `<prefix>/control/<command>`) with `payload`. WebSocket:
  `payload` is the whole frame sent (default Roastwarden's control frame); `{id}` in it is a
  request number (from 1000000, one count per connection, apart from the poll's), as Artisan's
  WebSocket protocol numbers its requests. Serial: `payload` is the line, several lines split by
  `\n`, blank ones dropped (default the aArtisan line).
- A command with no template goes out in Roastwarden's own syntax, in the device's unit. **Drum
  and damper** default to `<prefix>/control/drum|damper` on MQTT and WebSocket and to registers
  named or with the role `drum`, `damper` on Modbus; a TC4 has no standard drum or damper line,
  so there they need a template (else a 501 naming the port, `TransportError::NeedsTemplate`).
- Checks (400): known keys only; `payload` 1–256 characters; `scale` a finite non-zero number;
  `decimals` 0–3; `topic` as above.

The gate ([`roaster-setup.md`](roaster-setup.md)) runs before any template: a template is how a
command is written, never whether it is sent.

## Drum and damper

- `DeviceCommand::Drum { percent }` and `DeviceCommand::Damper { percent }` (names `drum`,
  `damper`), MQTT `control/drum`, `control/damper`, payload as the fan's.
- Outputs (`roaster_setup.outputs`) include `drum` and `damper` (label, min, max, step), declared
  by the owner; no kind declares them by default (a drum's speed is often fixed). The gate:
  `manual` and `full` allow them; undeclared is 409 `OUTPUT_NOT_DECLARED`; the value is shaped
  to the range (0 is off). No floor applies.
- Routes: `POST /api/roaster/{id}/control/drum` and `/control/damper` take `{ "percent": n }`,
  as the fan's; in the CLI's surface as the other live controls (the person at the roaster; an
  agent only on `virtual_roaster`).
- Telemetry: `drum` and `damper` are fields (the roles above); the roast keeps them with each
  sample beside the control fields (`session_telemetry.extras`).
- Dashboard: the Roast page's Control tab shows a Drum and a Damper slider only when declared,
  as the heater and fan; the Setup panel's Outputs editor declares them.
- Not built: drum and damper in Profile drive, alarm actions and the roast sequence.

## Agent path and dashboard

- `roast roaster show` prints each connection's mapping in one line, read from the connection's
  config (the CLI only reads the API): `mapping °F; bt ← MAX31855.Temperature, …; telemetry …;
  commands under …/control; polls … every … ms; templates …`, topics only where they differ
  from `roaster/<id>/…`; and the drum and damper outputs.
- Setting a mapping is the device routes' `PUT` (owner key); `roast roaster map` is the agent's
  way in.
- `docs/cli.md`, the skill's roaster section and `docs/hardware-integration-guide.md` (a section
  per transport: "Your own field names") describe it.
- Dashboard: the Role column is in the register map editor (`RegisterMapEditor.svelte`: the Add
  roaster wizard's Connection step, and Register Map → Edit on the roaster's Overview);
  `lib/roaster/registerRoles.ts` mirrors the server's rule. The Control tab's debounce is one
  timer per control, so a quick fan change then heater change does not drop the fan command.

## Tests

- The mapping (pure, `src/transports/ingest.rs`): paths, indexes, `$`, numeric strings and booleans,
  scale and offset, °F for each temperature role and the RoR, extras with and without a unit,
  only mapped fields pass, the checks' 400s.
- Per-field topics: a sample is cut on BT's message, stale fields (over 10 s) left out.
- MQTT in the harness: a device with a custom telemetry topic and °F mapping records °C samples;
  a heater command goes to its template's topic with its payload; the ESP32-style default
  device is unchanged (the existing tests are the no-regression check).
- The WebSocket client's poll message and `{id}`; TC4 `columns` and `UNITS;F`; Modbus roles
  for reading and writing, °F registers, the one-readable-one-writable rule.
- Drum and damper: the gate (undeclared 409, range, manual allows), each transport's rendering,
  the routes.
