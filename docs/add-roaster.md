# Adding a roaster: `roast roaster add | map | check`

Adding a roaster is configuration, not code: a connection (how Roastwarden reads it), a
mapping when the device uses its own field names, a kind, and its outputs and channels. Then
one check says whether it reads right. A roaster is always added at control **off**: it is
logged and sent nothing until you switch control on
([roaster kind and control level](roaster-setup.md)). What a roaster has to send to be logged or controlled is on
[connect your roaster](connect-your-roaster.md).

## In the dashboard

Roasters → **Add roaster** has four steps:

1. **Roaster**: Name, ID and Kind. **A known machine?** searches Artisan's machine catalogue,
   or opens an `.aset` file saved from Artisan, and fills in the rest
   ([roasters from Artisan's catalogue](artisan-machines.md)).
2. **Connection**: Wi-Fi (MQTT), USB thermocouple board (TC4), Phidgets temperature board,
   Controller on RS-485 (Modbus RTU), Controller on the network (Modbus TCP) or WebSocket. A
   Modbus roaster's registers are listed here, each with its role. **Save and check** saves the
   roaster at control off.
3. **Check** ("Does it read?"): the check, live every 3 s. If readings are missing or wrong,
   **Map its fields** opens the mapping: a row per field, the newest message the roaster sent,
   and what each field becomes. If nothing arrives, **Test the connection**.
4. **Control** ("What may Roastwarden send it?"): Off · logs only, Manual or Full. Leave it off
   until the check passes.

A roaster that is already sending on MQTT shows under Roasters → **Found, not added yet**.
Nothing is sent to it; **Set up** opens Add roaster with it filled in.

Later, on the roaster's own page: Overview → a connection's **Mapping** (or for Modbus,
Register Map → **Edit**), the **Check** tab, and Control → Setup for kind, outputs and
channels.

## MQTT passwords

When Roastwarden runs its own MQTT broker (the one-container install), every roaster that
sends on MQTT signs in with its id as the user name and a password of its own. Without one the
broker turns it away, and it never shows up under Roasters.

Roasters → **MQTT passwords** lists who may sign in, whether each is connected now, and
whether it is set up yet. Type the roaster's id and **Add**: Roastwarden makes a random
password and shows it once, with **Copy**. If the roaster's sketch already has a password, use
**Use the sketch's own** instead (8 to 128 characters, no spaces, quotes or backslashes). It
works at once, no restart. Building the roaster with an ESP32?
[ESP32 firmware](esp32-starter.md) fills the password into the sketch for you, and its
**Make its password** button adds one in the same step.

**New password** gives a roaster a new one. If the roaster is connected it is signed out at
once, and stays out until its sketch has the new password. **Remove** stops it signing in;
its roasts and setup stay. Neither works while the roaster is roasting, heating or cooling.

The roasters the installer set up are in `MQTT_DEVICES` in the server's `.env`. Those are
listed too, marked "In .env": change them there and restart. If Roastwarden uses a broker of
your own instead (Mosquitto, Home Assistant's), a roaster's user and password belong in that
broker and none can be added here ([with your own MQTT broker](install-docker.md#with-your-own-mqtt-broker)
lists the users and topics it needs).

From the command line: `roast roaster password list`, `add <id>`, `new <id>`, `remove <id>`
(admin; `--password-stdin` for a password of your own).

## From the command line

`add` and `map` need an admin token; `map --try` and `check` only read.

```
roast roaster add <id> --transport mqtt|websocket|modbus_tcp|modbus_rtu|serial_tc4|phidget22
    [--name …] [--kind fluid_bed_electric|drum_electric|drum_gas|logger|other]
    [--topic-prefix … --telemetry-topic …]           # mqtt
    [--device-url ws://…]                            # websocket
    [--host … --port … --unit-id …]                  # modbus_tcp
    [--port /dev/ttyUSB0 --baud … --parity …]        # modbus_rtu, serial_tc4
    [--preset stock-aartisan|tasmota|esphome|artisan-websocket]
roast roaster map <id> --field bt=data.BT --field et=data.ET [--units F] --try
roast roaster map <id> --field bt=data.BT --field et=data.ET [--units F]
roast roaster check <id>
```

The loop is `map --try` until the values look right, then `map` to save, then `check`. A
preset fills a common device's defaults:

| Preset | Fills in |
|---|---|
| `stock-aartisan` | a TC4 board running stock aArtisan: active channels only, ET on channel 1, BT on 2 |
| `tasmota` | the telemetry topic `tele/<id>/SENSOR`, BT from `MAX31855.Temperature` |
| `esphome` | BT and ET each on its own sensor topic (`<id>/sensor/bean_temperature/state`, `…/environment_temperature/state`) |
| `artisan-websocket` | Artisan's WebSocket protocol: the `getData` request every second, BT and ET from `data.BT`, `data.ET` |

`add` on a roaster found streaming on MQTT sets it up in place under its own id. `add` never
raises control: `roast roaster control <id> manual` asks the owner. The full flags are in the
[`roast` command reference](cli.md); the mapping is on
[mapping a device's own names](ingest-mapping.md).

## The check

Roasters → the roaster → Check, or `roast roaster check <id>` (exit 0 ok, 1 warn or fail). Each
line is ok, warn or fail; the verdict is the worst.

| Line | Passes when |
|---|---|
| rate | 0.5–4 samples a second over the last 30 s, evenly spaced |
| bt_et | BT present and within −10–300 °C; ET present (missing ET only warns) |
| units | at idle (heater off) both probes read room temperature, 5–45 °C; 50–115 suggests °F (`--units F`) |
| outputs | each declared output (heater, fan, drum, damper) is in the readings |
| channels | each declared channel is in the readings |
| caps | always ok: what the roaster says it can do, or that it said nothing |
| control | always ok: the control level and what it allows |

Saving a Modbus register map restarts its polling, so the rate can warn for the next 30 s:
check again.

## The write check

Once control is manual or full, run the write check once with the roaster cold: Check →
**Run the write check**, or `roast roaster check <id> --write`. It takes the owner key, a
heater declared, BT below 50 °C, no roast open and the roaster's own PID not in auto. It sends
heater 10 %, waits up to 5 s for the roaster to report it, sends emergency stop, and waits up
to 5 s for the heater to read 0. It always ends with emergency stop, also after a timeout or an
error.

To practise, the simulated gas drum is set up through this same path: switch it on under
Roasters → Simulator and open its Check tab ([simulated roasters](virtual-roaster.md)).

<!-- site: end -->

## Design notes

For developers: how adding a roaster, the mapping preview and the check work and why. The site
publishes only the part above.

Adding a roaster brings together the setup and gate ([`roaster-setup.md`](roaster-setup.md)),
ingest mapping ([`ingest-mapping.md`](ingest-mapping.md)) and declared channels
([`channels.md`](channels.md)). For builders, [`connect-your-roaster.md`](connect-your-roaster.md)
has the standard on one page and a tested example per transport.

## Why

Everything a stranger's roaster needs is configuration: a connection, a mapping, a kind,
outputs, channels. A non-technical owner will ask an agent to do it ("I have a TC4 on my
Behmor, set it up to log"); an agent that could only read (`roast roaster show`) would break
"agents are first class" for exactly the flow a non-technical owner needs help with. And the
owner needs a way to tell whether a roaster meets the interface standard: readings at a usable
rate, plausible values, the units right, what it can do.

Where it lives: `crates/server/src/roasters/raw.rs` (the newest raw messages),
`roasters/check.rs` (the check and the write check; the check's rules pure and tested), the
preview in `transports/ingest.rs` (`preview_json`, `preview_columns`), the routes in
`routes/contract.rs`; the CLI in `src/bin/roast/commands/adding.rs`; the simulated drum in
`sim/virtual_drum.rs` (`virtual-roaster.md`, "The simulated drum").

## `roast roaster add`

```
roast roaster add <device_id> --transport mqtt|websocket|modbus_tcp|modbus_rtu|serial_tc4|phidget22
    [--name …] [--kind fluid_bed_electric|drum_electric|drum_gas|logger|other]
    [--topic-prefix …] [--telemetry-topic …]          # mqtt
    [--device-url …]                                  # websocket
    [--host … --port … --unit-id …]                   # modbus_tcp
    [--port … --baud … --parity … --unit-id …]        # modbus_rtu, serial_tc4
    [--host … --port … --password … --serial-number … --hub-port … --thermocouple K]   # phidget22
    [--preset stock-aartisan|tasmota|esphome|artisan-websocket]
```

Creates the roaster (`POST /api/devices`), its connection (`POST …/connections`; the server's
config checks apply) and, with `--kind`, its setup at **control `off`**. It prints what it made
and the next step (`map`, then `check`). It never raises control: `roast roaster control <id>
manual` asks the owner (below).

- The WebSocket device's address is `--device-url`, not `--url`: `--url` is `roast`'s own global
  flag (the server's address) and would take the device's. `--port` is the TCP port for Modbus
  TCP and the serial port for Modbus RTU and a TC4.
- `add` also makes the roaster **active** (a roaster added through the API starts `pending`,
  and only an active one's connection is read).
- **Presets** fill the connection's defaults: stock aArtisan `reply_format: active`, ET on
  channel 1, BT on 2 (`CHAN;1200`); Tasmota `telemetry_topic tele/<id>/SENSOR` and `bt ←
  MAX31855.Temperature`; ESPHome `bt` and `et` on `<id>/sensor/bean_temperature/state` and
  `…/environment_temperature/state`; Artisan WebSocket the `getData` poll and
  `data.BT`/`data.ET`. Tasmota's and ESPHome's are a starting `ingest` to edit with `map`. A
  preset for another transport is a usage error (exit 2) before anything is made.
- **A roaster found streaming** (MQTT auto-discovery: pending, with the MQTT connection it was
  heard on) is set up in place by `roast roaster add <its id>`: its name, description and
  status change, its connection is changed when the protocol is the same and replaced when not,
  as the wizard's Set up does. A roaster added already exits 6 (`CONFLICT`). `POST
  /api/devices` with an existing id is a 409, not the database's unique-constraint 500.

**Tokens and the setup.** An agent setting up a logger needs to record its kind, outputs and
channels, none of which sends anything while control is `off`. So **a token with the admin
preset may `PUT` a setup that leaves control `off`** at once (also for a roaster with no setup
yet, and turning control off). A `PUT` that leaves control `manual` or `full` is held for the
owner's approval and made when they approve (`roaster-setup.md`, "Tokens and the setup";
`control.md`, "Tokens"); `roast roaster control <id> manual [--wait]` asks. The real-hardware
rule is otherwise unchanged.

### From Artisan's machine catalogue

```
roast roaster machines [QUERY] [--all]
roast roaster add <device_id> --artisan BRAND/MODEL | --aset FILE
    [--port /dev/ttyUSB0 | --host …] [--unit-id …] [--baud …] [--parity …] [--kind …] [--name …]
    [--dry-run]
```

A machine Artisan knows (its `.aset`, in the catalogue Roastwarden embeds, or a file the owner
has) is added the way Artisan has it: the server plans the connection, a register map with roles
and each register's slave, outputs from the sliders and channels from the extra curves
(`POST /api/artisan/machines/plan`, nothing saved), and `add` makes them, the setup at
**control off**. The transport comes from the file; `--port` (the serial port on the server) or
`--host` fills what the file cannot know (exit 2 naming it when missing). `--dry-run` prints the
plan and makes nothing. A Phidgets machine takes `--host` (the computer running Phidgets'
network server, [`phidgets.md`](phidgets.md)) and `--password`. A machine read through a device
Roastwarden has no transport for (S7, …) exits 1 with why. The details: [`artisan-machines.md`](artisan-machines.md).

## `roast roaster map`

```
roast roaster map <device_id> [--connection <id>]
    [--field bt=data.BT --field et=data.ET …] [--units C|F] [--topic bt=<topic> …]
    [--column ambient,et,bt,heater,fan]                # serial
    [--register 'input:0:float32:bt' …]                # modbus: kind:address[@slave]:type[*scale]:role[:unit]
    [--command heater='OT1;{value}' …] [--command-topic heater=<topic> …]
    [--try]
```

Writes the connection's `ingest` (and `commands`) through the connection `PUT` and the register
map's `role`s. It merges: the fields given replace theirs (`KEY=` takes one out), the rest of the
mapping stays; `--topic KEY=TOPIC` gives a field its own MQTT topic; templates are `--command
KEY=PAYLOAD` and, for MQTT, `--command-topic KEY=TOPIC`. `--register` replaces the register at
the same kind and address (writable when it is a holding register or coil with a write role);
the type takes a scale (`holding:0:int16*0.1:bt:°C`), as the register map editor's Scale column
does. The connection is `--connection`, or the roaster's only enabled one.

With `--try` it writes nothing: it shows the newest message the roaster sent and what the
mapping would make of it (below), and exits 1 when the mapping makes no sample; on a Modbus
connection it prints the newest poll. The agent's loop is `map --try` until the values look
right, then `map`, then `check`.

**The newest raw message.** The server keeps, in memory, the newest message as received before
any mapping, per roaster, connection **and topic** (at most 16 a roaster, so ESPHome's per-field
topics can be previewed; each at most 4 KB), and the topic it came on:

- MQTT keeps the topics in the route table and `roaster/<id>/telemetry` (with its saved MQTT
  connection's id, else none); the inbound device socket has no connection id.
- A TC4 keeps the last line that is not `#` chatter, parsed or not (a device printing its own
  CSV is mapped from it).
- A Modbus poll is kept as a JSON list `[{kind, address, name, type, value}]`, decoded by type
  and word order before scale, offset and role.
- For a roaster that is not mapped yet, MQTT messages on topics nobody claims are not kept (the
  server would keep every topic on the broker); `--topic-prefix`/`--telemetry-topic` claim them
  first (`add` does).

Reads:

- `GET /api/roaster/{id}/raw` → `[{connection_id, protocol, topic?, received_at, payload}]`;
- `POST /api/roaster/{id}/ingest/preview` `{connection_id, ingest}` (`ingest` null: the saved
  one) → `messages` (those it read), `sample`, `fields` (`key`, `field`, `source`, `raw`,
  `value` in °C, `problem`: path missing, not a number) and a `note`, or the checks' 400 for a
  bad mapping. Its MQTT telemetry topic is the connection's saved one. A TC4 with no `columns`
  lists the reply layout's fields. A Modbus connection is a 400 (registers have roles instead).
  Any token may call it (it saves and sends nothing: in `COMPUTE`, and exempt from the
  real-hardware switch); another roaster's connection is a 404.

## `roast roaster check`

Conformance against the interface standard, extending the contract view (`routes/contract.rs`):

`GET /api/roaster/{id}/check` (read-only, any token that reads) watches the ring and answers:

| Check | Passes when |
|---|---|
| Rate | samples arrive at 0.5–4 Hz over the last 30 s (the measured rate and its jitter); it needs two samples in the 30 s, and jitter (the spread of the gaps) warns above half the mean gap |
| BT, ET | present in the newest sample; BT within −10..300 °C. No BT fails, no ET warns |
| Units | judged only at idle (`heaterApplied` when reported, else the heater, at 0, or no heater declared): BT and ET within 5–45 °C; 50–115 warns "°F? set `--units F`", outside 5–45 otherwise warns "still hot" |
| Outputs | each declared output is echoed in telemetry (heater, fan, drum, damper); missing warns |
| Channels | each declared channel is present; missing warns |
| Caps | what the roaster said it can do (`caps`), or that it said nothing; always ok (it informs) |
| Control | the setup's control level, and what it permits; always ok (it informs) |

Each line is ok, warn or fail with a sentence; the overall answer is the worst. The answer adds
`rate_hz`, `samples` and `reading` (the newest sample, extras included: drum, damper, channels).
`roast roaster check <id>` prints it (exit 0 ok, 1 warn or fail).

**The write check** (`POST /api/roaster/{id}/check/write`, `roast roaster check <id> --write`):
the owner's flag on a cold roaster, never automatic.

- Owner key only (tokens 403). Refusals are 409, `CONTROL_OFF` for control off and
  `CHECK_REFUSED` otherwise: no heater declared, no sample in the last 10 s, BT at 50 °C or
  more, its PID in auto (`controlMode` 1: a heater % would not be what it runs), a roast open,
  or busy (a drive, an autotune, cooling).
- When the roaster reports heater power off (`heaterEnable` 0) it sends heater power on first
  (the fluid bed's firmware drops a heater % while power is off). Then it sends heater 10 %
  (shaped by the gate as always), waits up to 5 s for the heater's echo (any heater above 0 in a
  sample after the send), sends emergency stop, and waits up to 5 s for "off" (the heater, and
  `heaterApplied` when reported, at 0). It reports each step with its time.
- Always ends with emergency stop, also on a timeout or an error. It runs in a task of its own,
  so a caller that goes away still gets STOP sent.
- Its tests run on the virtual roasters and against a roaster that never answers (STOP after the
  timeout); development never runs it on real hardware.

## The simulated drum roaster

Next to the fitted fluid bed (`virtual_roaster`), a second simulated roaster, **`virtual_drum`**,
switched on beside it (`PUT /api/virtual-roaster` takes `drum: bool`; Roasters → Simulator has a
switch). It is a stranger's roaster on purpose: it speaks no Roastwarden JSON, so it is set up
through exactly the path a real one is.

- It publishes on `sim/drum/state` (not `roaster/…`), in **°F**, as
  `{"temps":{"bean":…,"exhaust":…,"inlet":…},"burner":…,"air":…,"drum":…}` once a second, and
  takes Tasmota-style commands: `sim/drum/cmnd/burner`, `…/air`, `…/drum` with a plain number,
  `…/stop`. It has no PID and sends no caps: Profile drive runs the server's loop on it.
- When it is first switched on, the server writes its roaster, MQTT connection (with that
  `ingest` and `commands`), and setup: kind `drum_gas`, control `full`, outputs heater
  "Burner" 0–100 step 5, fan "Air" 0–100, drum "Drum" 40–70 % and a declared channel `inlet`
  ("Inlet", C). If the owner changed them since, it leaves them.
- Its physics: a drum (metal mass with its own temperature) heated by the burner, air through it
  (more air, more convection and more heat carried out to the exhaust), beans heated by the
  drum and the air, a charge that drops the drum temperature. Not fitted to a real machine; it
  must only behave like one: turning point near 1:30, first crack near 9 min at about 60 %
  burner, the exhaust above BT. It obeys its outputs' limits and stops (burner 0, air 100) on
  STOP or when the server's presence topic reads offline for 10 s while the burner is on (a
  silence of commands would stop every hand-set roast).
- It is **not in the drive check**: `drive-check.mjs` leans on the fluid bed's CHARGE sequence
  starting Profile drive, and a drum's sequence only records. Tokens need the owner's switch for
  it, as for any roaster but `virtual_roaster`.

## Dashboard

- Add roaster's first step has **A known machine?** (a catalogue search or an
  `.aset` upload): the plan fills the wizard, its notes shown
  ([`artisan-machines.md`](artisan-machines.md)).
- The **Mapping** editor opens from a connection on the roaster's Overview
  (`MappingEditor.svelte`, `lib/roaster/mapping.ts`): one row per field (role or channel name,
  path or column, scale, unit), units, the templates; under it the newest raw message and the
  preview, refreshed every 2 s. It saves through the same `PUT`. The register map editor
  (`RegisterMapEditor.svelte`, the wizard's Connection step) also edits a Modbus roaster's saved
  map there (Register Map → Edit).
- Roasters → the roaster → **Check** (`/roasters/[id]/check`, `lib/roaster/check.ts`): the
  read-only check's lines, and for the owner key (or an open server) only, the write check
  behind a confirmation that names what it sends.
- Roasters → Simulator: the drum switch; CHARGE / DROP on `virtual_drum` load and dump its
  beans.

## MQTT passwords

`MQTT_DEVICES` is read at start, so a password kept only there means editing `.env` and
restarting for each new roaster, and the firmware download could fill in only those. The
dashboard's passwords are stored and checked live:

- **Store** (`mqtt_passwords.rs`, migration 060 `mqtt_passwords`): `MQTT_DEVICES`' (read at
  start, `source: env`, never changed from the API) plus the dashboard's (`source: dashboard`),
  kept as written because the firmware download fills them in, as `.env` keeps them. An id in
  both: `MQTT_DEVICES`' wins (a warning at start). Without the built-in broker the stored ones
  are not loaded and nothing can be added (409): the broker in use has its own users.
- **Sign-in** (`broker.rs`): the ACL plugin's rules hold only the server's and Home Assistant's
  users and the topic rules. A `ClientAuthenticate` hook of the server's own (priority 20,
  above rmqtt-acl's 10) answers for a roaster's user: right password `Allow`, wrong
  `BadUsernameOrPassword`, any other user on to the ACL. It reads the live list, so an added
  password works at the next connect. A new or removed password kicks every session with that
  user name (`Handle::sign_out`, rmqtt's `Entry::kick`), so the old password stops at once; the
  list's `signed_in` comes from the same session table.
- **Start order**: the database opens before the broker, so a stored password works from the
  first connect after a restart.
- **API** (`routes/mqtt_passwords.rs`, admin): `GET /api/mqtt/passwords` (never a password),
  `POST` (201, the password once, `Cache-Control: no-store`), `PUT /{id}` (a new one), `DELETE
  /{id}`. A change or removal is refused (409) while `busy.rs` lists the roaster: signing a
  roaster out mid-roast would cut its heater. Not gated by approvals: an admin token can
  already download a password in firmware, and nothing it does here reaches the heater.
- **Checks**: an id is a topic level (letters, digits, `_`, `-`, at most 48, not `roastwarden`
  or `homeassistant`); a password 8–128 printable ASCII without spaces, quotes or backslashes,
  so it goes into a sketch, `.env` and a shell unchanged; a random one is 128 bits as 32 hex.
- **Dashboard**: Roasters → MQTT passwords (`/roasters/passwords`); the firmware page's **Make
  its password** and `?id=`; About links it. `roast roaster password list | add | new |
  remove`.
- **Tests**: `mqtt_passwords::tests`, `broker::tests::a_password_from_the_dashboard_counts_at_
  once_and_a_new_one_signs_out` (a real rmqtt and rumqttc clients), `tests/roast_cli/roasters.rs`'s
  `an_agent_adds_a_roasters_mqtt_password`; checked by hand with `mosquitto_pub`/`_sub`.

## Agent path

- `roast roaster add | map | check`, `docs/cli.md`, the skill's "a new roaster" section (add →
  map --try → map → check; never raise control, tell the owner where to), the surface table:
  `POST /api/devices`, `PUT /api/devices/{id}` and `POST …/connections` are `roaster add`
  (admin), the connection `PUT` and the register map `roaster map`, the setup `PUT` `roaster
  control` (admin; `roaster add --kind` writes it too), `raw` and the preview `roaster map
  --try`, the check `roaster check`; the write check is the owner key's (`Via::None(OWNER)`).
- Task 39 (the drum): "Is the simulated drum roaster set up right? Check it, and tell me what
  it reads and whether it meets the standard. Don't change anything." Passes when `check`'s
  verdict, BT in °C and the drum speed are told and nothing changed. It switches the drum on,
  sets it to 55 % and waits 35 s first.

## Tests

- `add`'s requests per transport and preset (the CLI against the harness); the token rule for
  the setup `PUT` (off→off allowed for admin, control on held for approval, read-only token 403).
- Raw capture per transport and the preview (a JSON message, a TC4 line, per-field topics).
- The check's lines: rate from synthetic samples (1 Hz ok, 0.2 Hz fail), °F suspicion, a
  declared output not echoed; the write check's refusals (token, control off, hot, roast
  active) and its sequence against the virtual roaster, emergency stop on timeout.
- The virtual drum: physics (turning point, FC time at 60 % burner, exhaust above BT), its MQTT
  shape and the setup it writes once.
