# Artisan machine setups as roaster setups

If Artisan knows your roaster, Roastwarden can set it up the way Artisan has it, with no
manual to read and no register numbers to type in. Artisan ships a setup file (`.aset`) for
each machine it supports, 264 machines from 94 brands. Each names the device Artisan reads the
machine through and, for Modbus machines, every input (register, controller, scale, °C or
°F), the sliders that drive the burner, air, drum and damper, and the extra curves (inlet,
exhaust, gas pressure). Roastwarden carries that catalogue and turns a machine into a roaster:
its connection, its register map or field mapping, outputs from the sliders and declared
channels from the extra curves. 189 of the 264 machines plan a roaster.

A roaster from the catalogue is added at control **off**, like any other: check what it reads
first, then switch control on ([roaster kind and control level](roaster-setup.md)).

## Using it

**Dashboard**: Roasters → Add roaster → **A known machine?** Type a brand or model (**show all**
lists the machines it cannot read too, with why), or **open an .aset file** saved from Artisan.
Picking one fills in the wizard and lists notes: what was read, guessed or left out. When the
file cannot know something, it says "Next step: give the …": the serial port on the server, the
roaster's address, or an MQTT topic. Then Connection, Check and Control, as for any roaster
([add a roaster](add-roaster.md)).

**Command line**:

```
roast roaster machines coffed                      # search the catalogue
roast roaster machines --all                       # every machine, with why not
roast roaster add sr5 --artisan Coffed/SR5_manual --port /dev/ttyUSB1 --dry-run
roast roaster add sr5 --artisan Coffed/SR5_manual --port /dev/ttyUSB1
roast roaster add r1 --aset my-roaster.aset --host 192.168.1.70
```

`--dry-run` prints the plan and makes nothing. Without the port, address or topic the file
cannot know, `add` stops and names it. Then `roast roaster check <id>`.

## What it can read

| Artisan reads it through | Roastwarden sets up |
|---|---|
| Modbus RTU, ASCII or TCP (most machines) | the register map with roles; each register on its own controller when BT and ET sit on different ones on one RS-485 line; BCD values (the Loring Smart Roast) |
| Fuji PIDs: PXG, PXR, PXF (Sedona Elite, Phoenix ORO, Santoker 2xPXF/2xPXR, Wintop, Bideli) | Modbus RTU registers for each controller's temperature and its setpoint (an ET target) |
| Arduino TC4 (Hottop TC4, Kaleido Legacy) | a TC4 connection as stock aArtisan answers, the sliders as command templates |
| Artisan's WebSocket (Probat P Series III, G/UG and Sample Roaster, Prisma PLC) | the WebSocket client with Artisan's request, the field mapping and the sliders' commands |
| MQTT (ROEST 100, 200, P3000) | a field mapping; ROEST's broker is its own cloud, so bridge the roaster's data to your server's broker and give that topic (`--telemetry-topic`) |
| Phidgets boards (1048, TMP1101, TMP1200, 1051; Diedrich, Mill City and others) | a Phidgets connection: give the computer running Phidgets' network server ([Phidgets boards](phidgets.md)) |

The kind comes from Artisan's heating type (gas or electric). Without one it is a guess, and
noted: a gas drum when the machine has sliders, else a logger.

## What it cannot read, and what is left out

- **Not supported**: machines Artisan reads through Siemens S7 (Giesen and others), and those
  on Santoker's, Kaleido's, IKAWA's, Aillio's or Hottop's own protocols, HB, ARC and a few
  more; Modbus binary or UDP. `roast roaster machines --all` lists each with the reason.
- **Left out, and noted**: Artisan's custom buttons (Roastwarden has none), a slider in °C (an
  inlet setpoint), formulas other than a plain scale, a slider's second command, Artisan's
  CHARGE and DROP messages to a WebSocket roaster, and a Phidgets 1018's voltages.

<!-- site: end -->

## Design notes

For developers: how the catalogue is embedded and an `.aset` becomes a roaster, and why. The
site publishes only the part above.

## Why

Artisan ships its machine catalogue as data: one setup file per machine
(`src/includes/Machines/<brand>/<model>.aset`, 264 machines from 94 brands at the pinned
commit). Each names the device Artisan reads the machine through and, for Modbus machines, every
input (slave, register, function, divider, float or integer, °C or °F), the sliders that drive
the burner, air, drum and damper (the command each writes) and the extra curves (inlet, exhaust,
gas pressure). An owner moving from Artisan should not have to read a manual to type those
numbers in again: they name their machine and Roastwarden sets it up the way Artisan has it.

## What it is

- **The catalogue, embedded.** `crates/server/assets/artisan/machines.txt` holds every machine's
  setup, trimmed to the sections the importer reads (the button palettes and curve styles, most
  of each file, are dropped; the lines kept are verbatim). `scripts/artisan-machines.py` makes it
  from a pinned Artisan commit (`--commit SHA` moves the pin; `--from DIR` reads a checkout).
  Artisan is AGPL v3 like Roastwarden; the file's header names the source and the commit, and the
  image's notices name it too (`DEVELOPMENT.md`, "Licences").
- **`GET /api/artisan/machines`**: each machine (`key` `Brand/Model`, `name`, the Artisan device
  and its name, `supported`, `why` not). Any token that reads.
- **`POST /api/artisan/machines/plan`** `{machine}` or `{aset, file_name}` (an `.aset` the owner
  has, up to 1 MB): the **plan**, nothing saved. Any token (it computes only, like the mapping
  preview):
  - `protocol` and `config`: `modbus_rtu` (with `framing: ascii` for a Modbus ASCII line) or
    `modbus_tcp` (baud, parity, data and stop bits, the BT controller's slave as the
    connection's `unit_id`, Artisan's sampling interval as the poll, its timeout);
    `serial_tc4`, `websocket` or `mqtt` with the connection's ingest mapping and command
    templates (below, "Machines on other transports"); `phidget22` with the board's inputs
    mapped ([`phidgets.md`](phidgets.md));
  - `registers`: for Modbus, the register map (as `PUT /api/devices/{id}/register-map` takes
    it), with roles and each register's own slave; empty for the others;
  - `setup`: kind, **control `off`**, outputs from the sliders, channels from the extra curves (as
    `PUT /api/roaster/{id}/setup` takes it);
  - `needs`: what the file cannot know: `port` (the serial port on the server), `host` (the
    roaster's address, when the file has only `127.0.0.1`: Modbus TCP, a WebSocket; a Phidget's
    network server) or
    `telemetry_topic` (MQTT: the topic the roaster's data reaches the server's broker on);
  - `notes`: one sentence per thing read, guessed or left out.
- **`roast roaster machines [QUERY] [--all]`** lists them; **`roast roaster add ID --artisan
  BRAND/MODEL | --aset FILE [--port … | --host … | --device-url … | --telemetry-topic …]
  [--dry-run]`** applies the plan: the roaster (active), its register map (Modbus), its
  connection, its setup at control off. `--dry-run` prints the plan (registers, or the mapped
  fields, the poll and the templates) and makes nothing. The dashboard's **Add roaster** starts
  from a machine or a file in its first step and fills the wizard from the plan; what the form
  does not show (a mapping, a poll, templates) goes with the connection it saves.

Control is always off: the owner reads the values (`roast roaster check`, the Roast page), then
switches control on under Roasters → the roaster → Setup, as for any roaster
(`roaster-setup.md`).

## A register's own slave (migration 053)

71 of the 163 Modbus machines put BT and ET on different controllers on one RS-485 line (BT on
slave 1, ET on slave 2, the burner's output on slave 3). A register map entry has an optional
**`unit_id`** (1–247; null: the connection's): the poller reads each slave's registers in their
own requests and writes a command to its register's slave (`ingest-mapping.md`, "Modbus TCP and
RTU"). `roast roaster map --register KIND:ADDRESS@SLAVE:TYPE:ROLE` writes one; the register map
editor has a Slave column.

## How a setup is read

| Artisan | Roastwarden |
|---|---|
| `[Device] id=29` (Modbus) | supported; other devices are listed with why not (below) |
| `[Modbus] type` 0 serial RTU, 1 ASCII, 3 TCP | `modbus_rtu`, the same with `framing: ascii`, `modbus_tcp`; 2 binary, 4 UDP: not supported |
| `baudrate`, `parity` N/E/O, `bytesize`, `stopbits`, `comport` | baud, parity, data bits (8; ASCII 7 or 8), stop bits; the port is the owner's (`--port`) |
| `host`, `port` | host and port; `127.0.0.1` (Artisan's default) is not kept: `--host` |
| `[General] Delay` (ms) | `poll_interval_ms` (100–10 000) |
| `timeout`, `IP_timeout` (s) | `timeout_ms` (200–10 000) |
| input 1 / input 2 | BT / ET (`bt`, `et` roles) |
| `inputNdeviceId` | the register's slave; the BT controller's (input 1's) is the connection's `unit_id` |
| `inputNcode` 3 / 4 / 1 / 2 | holding / input register / coil / discrete input |
| `inputNdiv` 1 / 2 | scale 0.1 / 0.01 |
| `inputNfloat`, `FloatsAsInt` (+ `Signed`) | `float32`, `int32` / `uint32`; else `int16` / `uint16` |
| `wordorderLittle` (default true) | byte order `CDAB`, else `ABCD` (32-bit values) |
| `inputNmode` F | a role's register in `F` (the poller converts); a channel's register converts in its scale and offset, `(raw × div − 32) × 5/9` |
| `inputNbcd` / `inputNBCDsAsInt` | `bcd32` (two registers, the word order) / `bcd16`, then the divider |
| extra devices 33, 55, 109, 150, 207 (`MODBUS_34` … `_1112`) | inputs 3–12: the first curve (`extraname1`) is the lower input |
| a curve `{3}` (an event type's read-back), or `Burner %` | the output's read-back role (heater, fan, drum, damper) |
| a curve `IT`, `DT`, `Exhaust`, `Gas Pressure Pa`, `Drum RPM` | a declared channel (°C, or its unit from the name), at most 8 |
| a curve named `RoR` | a channel in `C/min` |
| a formula that is a scale (`x*1000`, `x/10`) on a curve not a temperature | the register's scale (a field's `scale`) |
| any other formula (`extramathexpression`) | the raw value; the formula is noted |
| an input drawn by no curve | left out, noted |
| `[Sliders]` action 2 (Modbus) `writeSingle(s,r,{})`, `write`, `writem` | a writable holding register (`uint16`, `int16` when the range is negative) with the slider's role, on slave `s` |
| `writeWord` / `writeLong` | `float32` / `int32` in the setup's word order |
| `sliderfactors`, `slideroffsets` | the register's inverse scale and offset: Artisan's `value × factor + offset` is what goes out |
| `slidermin`, `slidermax` in % (or no unit) | the output's min and max |
| a slider in Hz, rpm or Pa | an output in % of its top (`Drum (% of 60 Hz)`), min as its share |
| a slider in °C (an inlet setpoint) | left out, noted |
| the event type names (`[events] etypes`, default Air, Drum, Damper, Burner) | the output's label and role: burner/heater/gas/power → heater, air/fan/blower → fan, drum → drum, damper/flap → damper |
| a slider's second command (`;mwrite(…)`) | the value is written alone; the rest is noted |
| `PID_device_ID`, `PID_SV_register`, `SVmultiplier`, `SVwriteFloat`/`Long` | a writable `setpoint` register |
| `[ExtraEventButtons]` Modbus buttons | not imported (Roastwarden has no custom buttons): listed in the notes |
| `[General] roasterheating_setup` 1 LPG, 2 NG / 3 electric | kind `drum_gas` / `drum_electric`; not given: `drum_gas` with an output, else `logger` (noted as a guess) |

**Not supported** (listed by `roast roaster machines --all` with the reason): machines Artisan
reads through Siemens S7 (37, Giesen and others), Santoker, Kaleido's
own protocol, IKAWA, Aillio, Hottop's own, HB, ARC and others Roastwarden has no transport for,
Modbus binary or UDP. At the pinned commit, 189 of the 264 machines plan a roaster.

## Machines on other transports

- **BCD inputs** (the Loring Smart Roast's BT, ET and burner): the register map's data types
  `bcd16` (one register, four digits, 0–9999) and `bcd32` (two registers, eight digits, in the
  map's word order), read and written by the poller (a digit above 9 is a bad read: no value).
- **Modbus ASCII** (11: Delta DTA and DTK controllers, Daetgen, KapoK, Nordic and others): a
  Modbus RTU connection with `framing: "ascii"` and 7 or 8 data bits. The poller talks
  `:`-framed hex lines with an LRC (`modbus_ascii.rs`, a tokio-modbus client of its own);
  everything else (the register map, each register's slave, writes) is as for RTU.
  `roast roaster add ID --transport modbus_rtu --framing ascii` for one by hand.
- **Fuji PIDs** (9: Sedona Elite, Phoenix ORO, Santoker 2xPXF/2xPXR, Wintop, Bideli). Artisan's
  "Fuji PID" device is Modbus RTU underneath, so the plan is a register map from Artisan's own
  tables (`pid_control.py`): `readBTpid` and `controlETpid` name each controller's type (0 PXG,
  1 PXR, 4 PXF) and slave; its PV (input register 1000, a PXF 2000, × 0.1) is BT or ET, the ET
  controller's output MV1 (× 0.01) the heater's read-back, its SV in use a channel `et_sv`, and
  the SV Artisan writes (a PXR's SV at 1002, a PXG's or PXF's SV1 at 1240 / 2240) the
  `setpoint` role: an ET target, not BT's. Serial settings from `[SerialPort]`. No outputs, so
  the kind is a guess (logger) until the owner sets it.
- **Arduino TC4** (2: Hottop TC4, Kaleido Legacy): a `serial_tc4` connection as stock aArtisan
  answers (`reply_format: active`, ET and BT on `arduinoETChannel` / `arduinoBTChannel`, default
  1 and 2), the baud from `[SerialPort]`; channels 3 and 4 (`TC4 34`) as `t3`/`t4` when Artisan
  names them; each serial slider (`OT1,{}`, `DCFAN,{}`, `IO3,{}`) a command template
  (`decimals: 0`) and the fan slider's output the connection's `fan_output`.
- **Artisan's WebSocket** (4: Probat P Series III, G/UG and Sample Roaster, Prisma PLC): the
  client's `url` (`ws://host:port/path`; the host the owner's when the file has `127.0.0.1`), the
  poll Artisan sends (`{"command":"getData","id":{id},"roasterID":0}` with the setup's node
  names and request), an ingest mapping (channel 1 BT, 2 ET, 3–10 the extra curves, each
  `data.<node>`; a channel in °F says so), and each `send({…})` slider as a template whose frame
  carries `{id}` (the sink's own count) and the machine node. Artisan's CHARGE and DROP messages
  and the roaster's own events are noted, not sent or read.
- **Artisan's MQTT** (3: ROEST 100, 200 and P3000): an ingest mapping from the channels' paths
  (`data.bt`, …); a channel with its own topic is read from it. ROEST's broker is its cloud
  (`coap.roestcoffee.com:8883`, TLS, the owner's login) and Roastwarden reads only the server's
  broker, so the plan needs the topic the owner bridges the roaster's data to
  (`--telemetry-topic`). Artisan's JMESPath (`avg(…)`) and negative indexes (`events[-1]`) are
  left out, noted.

- **Phidgets** (13: Phidget's 1048 and TMP1101 databridges and TMP1200, Diedrich's 4
  and 6 sensor logging, Mill City, VNT, Titanium TGX, Tostabar Genius, Hive's Data Dome): a
  `phidget22` connection ([`phidgets.md`](phidgets.md)) whose host is the computer running
  Phidgets' network server, the board plugged in (`needs: host`, unless the file names a remote
  server: `phidgetRemoteFlag`, `phidgetServerID`, `phidgetPort`). Device 34 (1048) and 58
  (TMP1101): BT input 0, ET 1; extra devices 35/59 (`_34`) inputs 2 and 3, 36/60 (`_AT`) the
  board's own sensor (input 4). 52 (1051) and 61 (TMP1100): BT input 0, and the board's sensor,
  which Artisan draws as ET, the channel `board`. 68 (TMP1200): BT its RTD,
  `phidget1200_formula` and `_wire` its element and wires. `phidget1048_types` (1 K, 2 J, 3 E,
  4 T) gives the thermocouple types, `phidget1048_dataRate` / `phidget1200_dataRate` the data
  interval. Left out and noted: a 1018's voltages (Diedrich's air and power formulas), Modbus
  extras (Mill City's Delta and Shihlin controls) and sliders, a second TMP1200 board, Artisan's
  change triggers and async mode. Phidgets as other devices (1018 IO, DAQ1400, the hubs' own
  inputs, VCP1000s) as a machine's main device are listed as not supported.

A curve whose read-back name is an output (`{1}` Drum) but whose path names another unit
(`data.rpm`) is a channel in that unit, not the output's %.

## Tests

- The Qt INI reader (lists, quotes, escapes, comments, `@Variant` one-element lists), the
  write commands, and a hand-made bus setup planned end to end (`roasters/aset/tests.rs`).
- **Every catalogue machine**: each plan passes the checks the real calls apply (connection
  config with its mapping and templates, register map, outputs, channels), reads BT or ET,
  every register or field without a role is a declared channel and every channel has a source,
  and every declared output has a writable register or a template.
- One machine of each new kind planned in detail: a Sedona Elite PXF, the Kaleido and Hottop
  TC4s, a Probat WebSocket, a ROEST, a Loring (BCD) and a Delta DTA line (ASCII).
- BCD read and written (`transports/pollers/tests.rs`); the ASCII client against a fake slave
  (`modbus_ascii.rs`); `{id}` in a WebSocket template (`transport.rs`).
- A bus of three slaves read and written at each one's address (`transports/pollers/tests.rs`).
- `roast roaster machines`, `add --artisan --dry-run`, the missing `--port`, the roaster made,
  and an S7 machine refused; a Probat with `--host`, a ROEST needing `--telemetry-topic`, a
  Kaleido TC4 and a Delta DTA line made (`tests/roast_cli/adding.rs`).
- Every Phidgets machine plans; a 1048, a Diedrich, a 1051, a TMP1200, a Mill City and an
  owner's remote file in detail (`roasters/aset/tests.rs`); a Diedrich added with `--host` and `--password`
  (`tests/roast_cli/adding.rs`).

## Not built

- Modbus binary and UDP (no catalogue machine needs them).
- A second MQTT broker per connection (a ROEST is read through a bridge).
- Artisan's CHARGE / DROP messages to a WebSocket roaster and its pushed events.
- Buttons (Roastwarden has no custom buttons).
- Siemens S7 (37 machines): a transport of its own.
