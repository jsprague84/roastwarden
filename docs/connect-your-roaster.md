# Connect your roaster

Roastwarden has no driver per model. It works with any roaster that sends its temperatures
over one of its transports, and it can control a roaster that also takes commands for its
heater and fan (or burner and air).

- **Logged** (monitoring only): the roaster sends bean temperature (BT), and best also
  environment temperature (ET), about once a second. That is all Roastwarden needs for the live
  chart, rate of rise, phases, events, alarms that notify you, a profile as a background
  curve, the roast history, reports and labels. Roastwarden sends it nothing: you roast with
  the roaster's own controls.
- **Controlled**: the roaster also takes commands for its outputs (heater and fan on an
  electric fluid bed; burner, air, drum speed or damper on a drum), reports each output back in
  its readings, cuts the heat on an emergency stop, and has safety cutoffs of its own. Then the
  sliders, STOP, alarm actions and the roast sequence work, and at full control Profile drive,
  the server's PID loop, autotune and fan assist.

Every roaster starts at control **off**, logged only, until you raise its control level
([roaster kind and control level](roaster-setup.md)).

**The transports**: MQTT (an ESP32 or anything on Wi-Fi), Modbus TCP or RTU (PID controllers,
PLCs and many commercial roasters), a TC4-style board on a USB port of the server, a
WebSocket, and Phidgets thermocouple boards (reading only). A connection maps a device's own
field names, units, topics and command syntax ([mapping a device's own names](ingest-mapping.md)),
so a board that already speaks something (Tasmota, ESPHome, a stock TC4) needs no new firmware.

## Start here

| Your roaster | The way to connect it |
|---|---|
| None yet, or just looking | **Roasters → Simulator** switches on a simulated electric fluid bed or gas drum: a whole roast, no hardware ([simulated roasters](virtual-roaster.md)) |
| One you built, or any roaster you can fit a probe into | An ESP32 board and the **starter sketch**: step by step, no programming, about an hour ([your first roaster connection](esp32-starter.md)) |
| An electric fluid bed you built, controlled by Roastwarden | An ESP32, an SSR for the heater and the **fluid-bed controller**: its own PID and safety ([the ESP32 fluid-bed controller](esp32-fluid-bed.md)) |
| A machine Artisan knows | **Roasters → Add roaster → A known machine?** fills in its connection, registers and outputs ([roasters from Artisan's catalogue](artisan-machines.md)) |
| A TC4 board, or an Arduino that talks like one, on a USB port | [Example 3](#example-3-the-tc4-serial-lines) below |
| A PID controller or PLC that speaks Modbus | [Example 2](#example-2-a-modbus-tcp-register-layout) below |
| A Phidgets thermocouple board | Phidgets' own network server, no firmware ([Phidgets boards](phidgets.md)) |
| A board that already sends something (Tasmota, ESPHome) | Map its own names, no new firmware ([mapping a device's own names](ingest-mapping.md)) |

Whatever the way, the steps in Roastwarden are the same: **Roasters → Add roaster** (or
`roast roaster add`), the connection, then the **Check**, which reads the roaster live and says
whether it is right ([add a roaster](add-roaster.md)).

The rest of this page is for whoever writes the roaster's firmware: what Roastwarden expects,
and four small tested examples, one per transport. Every field and command is in the
[hardware integration guide](hardware-integration-guide.md).

## What Roastwarden wants

Roastwarden judges a roaster with one check (`roast roaster check`, or the Check step of Add
roaster). What the roaster provides decides how far its **control level** may go:

| Tier | The roaster provides | Control level |
|---|---|---|
| Logger | BT (and ET), steadily, 0.5 to 4 times a second | off |
| Manual | + the outputs it accepts, each echoed in its readings; emergency stop | manual |
| Full | + its own safety (below) | full |

### Readings

| Role | What | Needed |
|---|---|---|
| `bt` | bean temperature, °C | always |
| `et` | environment, exhaust or inlet temperature, °C | strongly (the check warns without it) |
| `heater`, `fan` | the % each output runs at now: the command echoed | for each output you declare |
| `heater_applied` | the heater % that reaches the element after your interlocks | optional; the write check uses it |
| `heater_enable` | heater power on (1) or off (0) | optional |
| `drum`, `damper` | % | when declared |
| `ambient`, `setpoint`, `mode` | room temperature; the onboard PID's target and mode | optional |
| a channel's name | any other probe or number (`inlet`, `exhaust`) | when declared ([`channels.md`](channels.md)) |

- **Rate**: 1 Hz is right. The check wants 0.5 to 4 samples a second over the last 30 s, and
  warns when the gaps are uneven (more than half a gap of jitter): RoR is computed by the server
  from BT, so send BT on a steady clock and do not compute RoR yourself.
- **Units**: °C. A device in °F says so (`units: F` in its mapping, `--units F`) and the
  server converts. The check judges units at idle: with the heater off both probes should read
  room temperature (5 to 45 °C); 50 to 115 looks like °F.
- **A failed probe**: leave the field out (JSON), print `-1` (TC4), or send a value the check
  rejects (Modbus has no "none"). Never send 0 for a missing probe.

### Commands

What Roastwarden sends depends on the control level and the outputs declared in the roaster's
setup (Roasters → the roaster → Control → Setup):

| Control | Sends |
|---|---|
| off | nothing at all, not even STOP |
| manual | heater %, fan %, drum %, damper %, heater on/off, emergency stop |
| full | + setpoint, mode and PID gains (for an onboard PID), feedforward, lease, RoR limit |

Every % is 0 to 100, shaped to the output's range and step before it is sent. **A roaster with
no PID of its own** still runs Profile drive at full control: the server's loop sends heater %.
Ignore what you do not implement; never fail on it.

**Emergency stop** means heater off now, fan up to cool, from any state, and it must work
without the server's next message. The write check sends it after heater 10 % and expects the
heater to read 0 within 5 s.

### Its own safety (full tier)

The server cannot see these, so whoever writes the firmware vouches for them before control
goes to full ([control](control.md)):

- heater off above a BT and an ET limit of its own;
- heater off when a probe fails;
- no heat without air (a fan floor) or, on a drum, without rotation;
- a watchdog: no heat when the server has gone quiet (no poll, no connection, or the
  server's `offline` on MQTT);
- presence (MQTT): a retained `online` status with a last will `offline`.

Every example below has all five.

### What the check reads

`roast roaster check <id>` (read only, any token) answers ok, warn or fail per line; the
verdict is the worst. With the owner key, `roast roaster check <id> --write` (a cold roaster,
control manual or full) sends heater 10 %, waits for its echo, sends emergency stop and waits for the
heater to read 0.

| Line | Passes when |
|---|---|
| rate | 0.5–4 samples a second over 30 s, evenly |
| bt_et | BT present and within −10–300 °C; ET present |
| units | at idle, the probes read 5–45 °C |
| outputs | each declared output is in the readings |
| channels | each declared channel is in the readings |
| caps | always ok: what the roaster says it can do (`pid`, `lease`, …) |
| control | always ok: the control level and what it allows |

## Choosing a transport

| Your roaster | Use | Why |
|---|---|---|
| An ESP32 (or anything) on Wi-Fi | **MQTT** | Roastwarden's own; found by itself when it starts sending; survives Wi-Fi drops |
| A board on a USB port of the server | **TC4 serial** | no network at all; Artisan's TC4 device reads the same board |
| A PLC, a PID controller, an industrial gateway | **Modbus TCP** or **RTU** | the industry's; Artisan reads the same registers |
| A device that serves a WebSocket | **WebSocket** | the server dials it; your own JSON, mapped |
| A Phidgets thermocouple board (1048, TMP1101) | **Phidgets** | no firmware to write: Phidgets' network server on the computer it is plugged into ([`phidgets.md`](phidgets.md)); it only reads, so controls need a second connection |

If you write the firmware, MQTT with Roastwarden's own topics and names is the least work: nothing
needs mapping. Each example's sketch downloads from its heading below; in the source repository
it is in `examples/roasters/<name>/` with its `roaster.json`: the roaster, connection and
register map as Add roaster saves them. New to the ESP32 and the Arduino IDE? The
[starter guide](esp32-starter.md) sets them up step by step, and the examples build the same way.
For an electric fluid bed, the [fluid-bed controller](esp32-fluid-bed.md) is the complete
version of Example 1: its own PID, time-proportioned heat and the lease; **Roasters → ESP32
firmware** fills its settings in.

All four sketches are for an ESP32 with the Arduino core and the same parts: two MAX31855
thermocouple boards (BT on GPIO 5, ET on 17, clock 18, data 19), a solid state relay for the
heater on GPIO 26, run as a one-second slow PWM, and a fan on GPIO 27 through a MOSFET. Change
the pins and the safety limits at the top of each sketch. They compile with PlatformIO
(ESP32 core 3.3).

## The steps, whatever the transport

1. **Flash it** and power it with the heater element disconnected the first time.
2. **Add it**: Roasters → Add roaster (Roaster → Connection → Check → Control), or
   `roast roaster add` (an admin token). It is saved at control off after the Connection step.
3. **Check it**: the Check step reads it live, with the mapping inline. Fix the mapping
   until the verdict is ok.
4. **Control**: pick manual or full in the last step (it takes the owner key; or later under
   Roasters → the roaster → Control → Setup), then run the write check once, cold: Roasters →
   the roaster → Check → **Run the write check**, or `roast roaster check <id> --write`.

## Example 1: an ESP32 on MQTT

[mqtt-esp32.ino](https://roastwarden.com/examples/mqtt-esp32.ino) (PubSubClient, ArduinoJson 7,
Adafruit MAX31855): the [starter](esp32-starter.md) with heater and fan control and the safety
that needs. Its settings at the top carry the same names as the starter's.

It speaks Roastwarden's own topics and names, so there is **no mapping**:

```
publishes  roaster/mqtt_roaster/telemetry   once a second
           {"beanTemp":182.3,"envTemp":215.0,"heater":45,"heaterApplied":45,"fan":70,"heaterEnable":1}
           roaster/mqtt_roaster/status      retained; last will {"status":"offline"}
           {"status":"online","version":"example-1","caps":[]}
listens    roaster/mqtt_roaster/control/heater           45      (%, may have decimals)
           roaster/mqtt_roaster/control/fan              70
           roaster/mqtt_roaster/control/heater_enable    1 | 0
           roaster/mqtt_roaster/control/emergency_stop   1
           roastwarden/server/status                       the server's presence
```

`"caps": []` says it has no PID of its own (`pid` would mean it takes `setpoint`, `mode` and
`pid`). Its watchdog: no heat while the broker connection or the server is gone.

**The broker**: Roastwarden's built-in broker takes no anonymous clients, and nor does the
compose bundle's Mosquitto. On the built-in broker, add the roaster's password under Roasters →
[MQTT passwords](add-roaster.md#mqtt-passwords) (the device id is its user name; it works at
once); with the compose bundle's Mosquitto, put it in `.env`
(`MQTT_DEVICES=mqtt_roaster:its-password`) and restart Roastwarden (`docker compose up -d` in
its folder; [install](INSTALL.md)). It may then use `roaster/mqtt_roaster/#` only. Fill the same id and password, and the server's address, into
the sketch's settings at the top.

**Add roaster**: the roaster appears by itself under Roasters → **Found, not added yet** a
second after it starts sending; **Set up** opens Add roaster with it filled in.

- Roaster: Name, ID `mqtt_roaster` (what it sends; kept), Kind (Fluid bed, electric here).
- Connection: **Wi-Fi (MQTT)**, Topic prefix `roaster/mqtt_roaster`. Save and check.
- Check: the lines below, live.
- Control: Manual or Full.

From the command line (it sets up the found roaster in place):

```
$ roast roaster add mqtt_roaster --transport mqtt --name "DIY roaster (MQTT)" --kind fluid_bed_electric
Set up mqtt_roaster (it had streamed already) with a mqtt connection (936dbd6b-…), kind fluid_bed_electric.
Control is off: Roastwarden only records it; the owner switches control on under Roasters → mqtt_roaster → Setup.
$ roast roaster check mqtt_roaster
mqtt_roaster: ok
  ok   rate     1.00 Hz over the last 30 s, jitter ±0.00 s
  ok   bt_et    BT 20.6 °C, ET 21.0 °C
  ok   units    °C: the probes read room temperature with the heater off
  ok   outputs  Heater, Fan declared and reported
  ok   channels no channels declared
  ok   caps     it says it can do: nothing optional
  ok   control  control off: Roastwarden only records it; the owner switches control on under Roasters → mqtt_roaster → Setup
  reading: bt 20.65, et 21.0, fan 0.0, heater 0.0, heaterApplied 0.0, ror 0.0
$ roast roaster check mqtt_roaster --write          # the owner key, cold, at control manual
mqtt_roaster write check: ok
  2 ms ok   heater 10 %: sent via mqtt (shaped by the roaster's outputs)
  711 ms ok   heater echo: it reports heater 10 %
  714 ms ok   emergency stop: sent via mqtt
  1727 ms ok   heater off: it reports the heater at 0
```

Your own names and topics instead (a board you cannot reflash): a mapping on the connection,
[`ingest-mapping.md`](ingest-mapping.md).

## Example 2: a Modbus TCP register layout

[modbus-tcp.ino](https://roastwarden.com/examples/modbus-tcp.ino) (eModbus, Adafruit MAX31855): a Modbus TCP slave
on port 502, unit 1, holding registers only (function 3 to read, 6 and 16 to write):

| Register | Holds | Type | Read / write | Role |
|---|---|---|---|---|
| 0 | BT, °C × 10 | int16 | read | `bt` (scale 0.1) |
| 1 | ET, °C × 10 | int16 | read | `et` (scale 0.1) |
| 2 | heater % | uint16 | read, write | `heater` |
| 3 | fan % | uint16 | read, write | `fan` |
| 4 | heater % after the interlocks | uint16 | read | `heater_applied` |
| 5 | heater power, 0 / 1 | uint16 | read, write | `heater_enable` |
| 6 | stop: write 1 | uint16 | write | `emergency_stop` |

Tenths in an int16 are what most PID controllers use, and Artisan reads them too (Modbus,
function 3, divider 1/10), so the same roaster works in both. Its watchdog: no heat when nobody
has read it for 5 s (keep Roastwarden's poll at 1 s). **Answer a write with an echo of the
request**, as the Modbus spec says, also for a register that clears itself (the stop register
reads 0): a master may check the echo.

**The register map**: each register gets a **role**, so its name is free
([mapping a device's own names](ingest-mapping.md)). In `roaster.json` it is the
`register_map` list.

**Add roaster**:

- Roaster: Name, ID `modbus_roaster`, Kind.
- Connection: **Controller on the network (Modbus TCP)**, Address (the ESP32's IP), Port 502;
  Advanced: Unit ID 1, Poll 1000 ms. Registers: **+ Add Register** per row of the table: Address,
  Name, Type `holding`, Data Type, **Scale** (0.1 for BT and ET), Unit, RW (on for 2, 3, 5, 6),
  Role. Save and check.
- Check, Control as above.

From the command line (`map --register KIND:ADDRESS:TYPE[*SCALE]:ROLE[:UNIT]`; writable follows
from a holding register with a command's role):

```
$ roast roaster add modbus_roaster --transport modbus_tcp --host 192.168.1.60 --port 502 --unit-id 1 \
    --name "DIY roaster (Modbus TCP)" --kind fluid_bed_electric
$ roast roaster map modbus_roaster \
    --register 'holding:0:int16*0.1:bt:°C' --register 'holding:1:int16*0.1:et:°C' \
    --register 'holding:2:uint16:heater:%' --register 'holding:3:uint16:fan:%' \
    --register 'holding:4:uint16:heater_applied:%' --register holding:5:uint16:heater_enable \
    --register holding:6:uint16:emergency_stop
modbus_roaster: 7 registers saved. Next: roast roaster check modbus_roaster
$ roast roaster map modbus_roaster --try
modbus_roaster: the newest Modbus poll (decoded by type, before scale and role)
  [{"address":0,"kind":"holding","name":"bt","type":"int16","value":207.0},{"address":1,…,"value":210.0},…]
$ roast roaster check modbus_roaster
modbus_roaster: ok
  ok   rate     1.00 Hz over the last 30 s, jitter ±0.00 s
  ok   bt_et    BT 20.7 °C, ET 21.0 °C
  ok   units    °C: the probes read room temperature with the heater off
  ok   outputs  Heater, Fan declared and reported
  ok   channels no channels declared
  ok   caps     it says nothing about what it can do: Profile drive runs the server's own PID loop
  ok   control  control off: Roastwarden only records it; the owner switches control on under Roasters → modbus_roaster → Setup
  reading: bt 20.7, emergencyStop 0.0, et 21.0, fan 0.0, heater 0.0, heaterApplied 0.0, ror 0.0
```

Saving the map restarts the poller, so a check in the first 30 s after it can warn about the
rate (the window holds the polls from before): check again. Modbus RTU on RS-485 is the same
register map with a serial port, baud and parity instead of an address
([hardware integration guide](hardware-integration-guide.md)).

## Example 3: the TC4 serial lines

[tc4-serial.ino](https://roastwarden.com/examples/tc4-serial.ino) (Adafruit MAX31855): any Arduino-compatible board
on a USB port of the server, answering as stock aArtisan does, so Roastwarden's "stock aArtisan"
reply format and Artisan's own TC4 device both read it:

```
server → board   UNITS;C     CHAN;1200     (once, at open)
                 READ        (every second)
                 OT1;45      heater %
                 IO3;70      fan %
                 PID;OFF  OT2;0   (emergency stop sends PID;OFF, OT1;0, OT2;0, IO3;100)
board → server   READ:   ambient,ET,BT,heater,fan     e.g. 21.00,180.50,165.20,45,70
                 others: a "# …" line, which the server ignores
```

`CHAN;1200` puts ET on logical channel 1 and BT on 2, Artisan's convention: keep the
connection's channels at ET 1, BT 2, as this board always prints them. Its watchdog: no heat
when no `READ` came for 5 s.

**Add roaster**:

- Roaster: Name, ID `tc4_roaster`, Kind.
- Connection: **USB thermocouple board (TC4)**, the serial port (`/dev/ttyUSB0`,
  `/dev/ttyACM0`; in Docker pass it through and add the `dialout` group), Reply format **Stock
  aArtisan (active channels only)**; Advanced: Baud 115200, ET 1, BT 2, Fan output IO3. Save
  and check.
- Check, Control as above.

```
$ roast roaster add tc4_roaster --transport serial_tc4 --port /dev/ttyUSB0 --preset stock-aartisan \
    --name "DIY roaster (TC4)" --kind fluid_bed_electric
$ roast roaster map tc4_roaster --try
tc4_roaster serial_tc4 356ead0c-…
  newest: 21.00,21.00,20.65,0,0
  ambientTemp ← active reply layout → ambientTemp 21
  beanTemp ← active reply layout → beanTemp 20.7
  envTemp ← active reply layout → envTemp 21
  fan ← active reply layout → fan 0
  heater ← active reply layout → heater 0
$ roast roaster check tc4_roaster
tc4_roaster: ok
  ok   rate     1.00 Hz over the last 30 s, jitter ±0.00 s
  ok   bt_et    BT 20.7 °C, ET 21.0 °C
  …
```

A board that prints its own columns or °F takes `--column` and `--units F`
([mapping a device's own names](ingest-mapping.md)).

## Example 4: a WebSocket server of its own

[websocket.ino](https://roastwarden.com/examples/websocket.ino) (WebSockets by Markus Sattler, ArduinoJson 7,
Adafruit MAX31855): a WebSocket on port 81 that pushes its own JSON once a second; Roastwarden
dials it and maps the fields, and its commands go out through the connection's templates:

```
board → server   {"BT":165.2,"ET":180.5,"heater":45,"fan":70}
server → board   {"set":"heater","value":45}   {"set":"fan","value":70}
                 {"set":"enable","value":1}    {"set":"stop"}
```

The mapping (`roaster.json`'s `ingest` and `commands`):

```json
"ingest": { "fields": { "bt": "BT", "et": "ET", "heater": "heater", "fan": "fan" } },
"commands": {
  "heater": { "payload": "{\"set\":\"heater\",\"value\":{value}}" },
  "fan": { "payload": "{\"set\":\"fan\",\"value\":{value}}" },
  "heater_enable": { "payload": "{\"set\":\"enable\",\"value\":{value}}" },
  "emergency_stop": { "payload": "{\"set\":\"stop\"}" }
}
```

Its watchdog: no heat while no client is connected.

**Add roaster**:

- Roaster: Name, ID `ws_roaster`, Kind.
- Connection: **WebSocket**, WebSocket address `ws://192.168.1.61:81/`. Save and check.
- Check: the Mapping editor under the check: a row per field (`bt` ← `BT`, `et` ← `ET`,
  `heater` ← `heater`, `fan` ← `fan`) and the four templates; it shows the newest frame and
  what each field becomes. Save, and the check turns ok.
- Control as above.

```
$ roast roaster add ws_roaster --transport websocket --device-url ws://192.168.1.61:81/ \
    --name "DIY roaster (WebSocket)" --kind fluid_bed_electric
$ roast roaster map ws_roaster --field bt=BT --field et=ET --field heater=heater --field fan=fan --try
ws_roaster websocket 55caf888-…
  newest: {"BT": 20.65, "ET": 21.0, "heater": 0.0, "fan": 0.0}
  bt ← BT = 20.65 → beanTemp 20.65
  et ← ET = 21.0 → envTemp 21
  fan ← fan = 0.0 → fan 0
  heater ← heater = 0.0 → heater 0
  sample {"beanTemp":20.65,"envTemp":21.0,"fan":0.0,"heater":0.0}
Looks right? Run it again without --try to save it.
$ roast roaster map ws_roaster --field bt=BT --field et=ET --field heater=heater --field fan=fan \
    --command 'heater={"set":"heater","value":{value}}' --command 'fan={"set":"fan","value":{value}}' \
    --command 'heater_enable={"set":"enable","value":{value}}' --command 'emergency_stop={"set":"stop"}'
$ roast roaster check ws_roaster
ws_roaster: ok
  ok   rate     1.07 Hz over the last 30 s, jitter ±0.25 s
  …
```

A device that answers requests instead of pushing (Artisan's WebSocket protocol) takes a
`poll` (`--preset artisan-websocket`); one that dials the server instead uses its inbound
socket, `/ws/device/<id>/telemetry`, with Roastwarden's own names
([hardware integration guide](hardware-integration-guide.md)).

## Try them without the hardware

The source repository's `tools/mock-device/example_roaster.py` speaks what each sketch
speaks, with a small thermal model behind it (the heater warms, air cools, no heat below 30 %
fan):

```bash
pip install -r tools/mock-device/requirements.txt
python tools/mock-device/example_roaster.py modbus        # Modbus TCP on :5020
python tools/mock-device/example_roaster.py websocket     # ws://…:8081/
TC4_LINK=/tmp/tc4 python tools/mock-device/example_roaster.py tc4   # a pseudo-terminal at /tmp/tc4
MQTT_BROKER_HOST=… MQTT_PASSWORD=… python tools/mock-device/example_roaster.py mqtt
```

Add each with its example's steps (the twin's address, `/tmp/tc4` as the port), on a server
you run for testing (MQTT needs a broker). The outputs above came from these twins.

<!-- site: end -->

## Design notes

For developers: how the example roasters are kept working. The site publishes only the part
above.

Each example's `roaster.json` is in the API's shapes, and the tests
read these files. A local server for the twins: `cargo run -p roastwarden-server`
(`docs/DEVELOPMENT.md`, "Run locally").

## How the examples are kept working

- `crates/server/src/connect_examples.rs`: one test per example adds its `roaster.json` as Add
  roaster does, runs a device that speaks what the sketch speaks (the MQTT loopback, a Modbus TCP
  slave, a pseudo-terminal, a WebSocket server), expects the check's verdict ok at control off
  with nothing sent, then fan 60 %, heater 40 % and emergency stop in the device's own syntax at
  control manual, and the readings echoing them.
- `examples/build.sh` compiles the four sketches with PlatformIO (not in CI: run it
  when a sketch changes).
