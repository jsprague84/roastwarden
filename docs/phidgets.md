# Phidgets temperature boards

Phidgets boards are the usual thermocouple add-on for gas roasters, and the most common
device in Artisan after Modbus. If your roaster has one, Roastwarden logs its temperatures:
bean and environment temperature, and any other input as an extra channel. A Phidgets board
only reads: it gives you logging, alarms and analysis, not control.

## What it reads

A Phidgets temperature board through **Phidgets' network server** (`phidget22networkserver`,
TCP 5661) running on the computer the board is plugged into: a Raspberry Pi beside the roaster,
or your Roastwarden server itself. Roastwarden is a network client of it, as Artisan is with "remote"
on; no Phidgets library is linked into the server or the image.

| Board | Inputs (`chN`) |
|---|---|
| 1048 (USB, PhidgetTemperatureSensor 4-Input) | ch0–ch3 thermocouples, ch4 the board's own sensor |
| TMP1101 (VINT, on a hub port) | ch0–ch3 thermocouples, ch4 the board's own sensor |
| 1051 (USB), TMP1100 (VINT) | ch0 a thermocouple, ch1 the board's own sensor |
| TMP1200 (VINT) | ch0 an RTD (PT100 or PT1000) |

`chN` is Phidgets' own channel number among the board's temperature inputs (what the Phidget
Control Panel shows and Artisan's `setChannel` takes). Readings are °C. A thermocouple's type is
set on the board (J, K, E or T; K by default); an RTD's element and wiring likewise.

Not read: Phidgets' voltage, current and digital inputs (a 1018, a DAQ1400), outputs (relays,
PWM, voltage outputs for a burner), and a board on the server's own USB without the network
server. A Phidget connection only reads, so a Phidget roaster logs at control off; a roaster that
also takes commands needs a second connection that carries them.

## Adding one

- **Roasters → Add roaster**: "Phidgets temperature board" in the Connection step: the computer
  it is plugged into, BT and ET inputs, the thermocouple type; port, password, board, intervals and
  RTD under Advanced. Test reads the board. A network server found on the LAN (mDNS
  `_phidget22server._tcp`) is offered in "Found, not added yet".
- **`roast roaster add ID --transport phidget22 --host H [--port 5661] [--password P]
  [--serial-number N] [--hub-port N] [--thermocouple K]`**; another input:
  `roast roaster map ID --field exhaust=ch2` (BT and ET stay).
- **From Artisan**: the 13 Phidgets machines of Artisan's catalogue (Phidget 1048 and TMP1101
  databridges, TMP1200, Diedrich's 4 and 6 sensor logging, Mill City, VNT, Titanium TGX, Tostabar
  Genius, Hive's Data Dome) plan a `phidget22` connection ([Artisan machines](artisan-machines.md)):
  `roast roaster add ID --artisan "Diedrich/6_Sensor_Data_Logging" --host 192.168.1.60
  --password …`. The thermocouple types (`phidget1048_types`), data rate and RTD settings come
  from the file; Artisan's extra devices `1048_34` / `TMP1101_34` are inputs ch2 and ch3,
  `_AT` the board's sensor (ch4); its 1018 voltages, Modbus extras and sliders are noted and left
  out. A 1051's or TMP1100's board sensor, which Artisan draws as ET, is the channel `board`.
  A file with Artisan's remote server set (`phidgetRemoteFlag`, `phidgetServerID`) keeps that
  host.

On the computer with the board: install Phidgets' drivers and network server
(<https://www.phidgets.com/docs/OS_-_Linux>; Windows and macOS: the Phidget Control Panel's
Network Server tab), switch the server on, and check the board shows in the Control Panel there.

## The connection

The settings, as the API and `roast` name them (protocol `phidget22`, `Phidget22ConnectionConfig`):

| Key | Default | Meaning |
|---|---|---|
| `host` | (required) | the computer running the network server |
| `port` | 5661 | the network server's port |
| `password` | none | the server's password; answers show `********`, and sending that back keeps the saved one |
| `serial_number` | the first temperature board | the board's serial number (a VINT board's is its hub's) |
| `hub_port` | any | a VINT board's hub port, 0–5 |
| `poll_interval_ms` | 1000 | how often a sample is made |
| `data_interval_ms` | 256 | how often the board reports each input (a 1048 takes 32–60000) |
| `thermocouple_type` | `K` | every thermocouple input's type: `J`, `K`, `E`, `T` |
| `thermocouple_types` | none | an input's own type: `{"ch1": "J"}` |
| `rtd_type`, `rtd_wires` | `pt100_3850`, 2 | a TMP1200's element (`pt100_3850`, `pt1000_3850`, `pt100_3920`, `pt1000_3920`) and wires (2, 3, 4) |
| `ingest` | `{"fields": {"bt": "ch0", "et": "ch1"}}` | which input is BT, ET or a channel ([ingest mapping](ingest-mapping.md)) |

Each sample is the **mean of the readings since the last one** (Artisan does the same when a
channel is not async); an input with no new reading keeps its newest for three data intervals
(at least 3 s), then has no value. An error event from the board (a thermocouple open, out of
range) leaves the input without a value until it reads again. `commands` is refused.

<!-- site: end -->

## Design notes

For developers: how the server reads a board and speaks Phidgets' protocol without their
library. The site publishes only the part above.

Phidgets are the standard thermocouple add-on for gas roasters and Artisan's most common device
after Modbus, which is why Roastwarden reads them.

## The connection, inside

The poller logs in, waits for the server's announcements, picks the board (`serial_number`,
`hub_port`, else the first board with every mapped input), opens the mapped inputs, sets each
(data interval, change trigger 0, thermocouple type or RTD), and makes a sample every poll. A lost
connection or the board unplugged reconnects with a backoff (1 s, doubling to 30 s). Each sample's
raw values (`{"ch0": 201.3, "ch1": 230.1}`) are the roaster's newest raw message, so
`roast roaster map --try` and the dashboard's mapping editor show them.

## How it talks (the protocol)

No Phidgets library: `crates/server/src/transports/phidget.rs` speaks the network server's protocol as
Phidgets' own JavaScript client does (`phidget22` on npm, ISC), read from it and from
libphidget22's server (BSD):

- each message is a 16-byte header (magic `0x50484930`, the body's length, flags, request and
  reply numbers, type and subtype, little-endian) and a JSON body; libphidget22 prints a double
  that is not a number as `nan`, which is read as null;
- the client says hello (type `www,roastwarden`, protocol 2.4; a type starting with `www` makes the
  server announce each channel), then proves the password:
  `base64(sha256("phidgetclient" + password + nonceC + nonceS + salt))`;
- the server announces devices (`30/50`) and channels (`30/80`, class 28 = temperature sensor);
  the client opens a channel (`30/60`, answered with its state) and sets it with bridge packets
  (`30/70`: 54 data interval, 46 change trigger, 84 thermocouple type, 76/77 RTD); readings come
  as bridge packet events (95 temperature change, 17 an error event); the server's keepalive
  (`20/41`) is answered.

## Tests

- The codec (header, `nan`), the proof, the board's choice and its messages, the sample's mean
  and staleness, error events (`phidget.rs`).
- A mock server (`phidget::mock`, the same protocol from the server's side) for the client: login
  and a wrong or missing password, open and set, readings, keepalives answered, the probe.
- End to end in the harness: a roaster on a TMP1101 read by the poller, its check, the mapping
  preview, the password masked and kept through an edit, the connection test using the saved one.
- Every Phidgets machine of Artisan's catalogue plans a connection the server takes; a 1048, a
  Diedrich, a 1051, a TMP1200, a Mill City and an owner's remote file in detail (`roasters/aset/tests.rs`).
- `roast roaster add --transport phidget22`, `roaster show`, `roaster map --field`, and a
  Diedrich from Artisan with `--host` and `--password` (`tests/roast_cli/adding.rs`).
- By hand, not in CI: Phidgets' official JavaScript client against
  `tools/mock-device/mock_phidget22.py` (the mock is faithful), and Phidgets' real
  `phidget22networkserver` in Docker against Roastwarden's client
  (`cargo test … real_phidget22_server -- --ignored`).
