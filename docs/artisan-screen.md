# Artisan as a screen for Roastwarden

If you roast with Artisan, you can keep its screen while you try Roastwarden. Roastwarden's
server speaks Modbus TCP as a roaster would, and Artisan connects to it like any Modbus
roaster: Artisan draws and records the roast it reads, and Roastwarden records the same roast
with its own alarms, control and history. Nothing in Artisan is lost: your profiles, alarms
and palettes stay as they were. It is off unless you switch it on.

## Switching it on

The server opens the slave when `ROASTWARDEN_MODBUS_ADDR` is set:

| Variable | Default | What it does |
|---|---|---|
| `ROASTWARDEN_MODBUS_ADDR` | unset (off) | Where the slave listens: `0.0.0.0:502` (502 is Modbus's port and Artisan's default) |
| `ROASTWARDEN_MODBUS_DEVICE_ID` | the one real roaster | The roaster it serves. Unset: the one real roaster heard in the last minute; the virtual roasters never count, so with none or two real roasters it serves nothing and says why |
| `ROASTWARDEN_MODBUS_WRITES` | off | `on`: Artisan's sliders become commands (below). Off: every write is refused |

With the compose file, uncomment the two lines it has for it (the environment line and the
port) and restart:

```yaml
    ports:
      - "8080:8080"
      - "502:502"            # Artisan as a screen (docs/artisan-screen.md)
    environment:
      - ROASTWARDEN_MODBUS_ADDR=0.0.0.0:502
```

The container runs as an ordinary user; Docker lets a container's own network open port 502
anyway. If the log says the port could not be opened (another engine, or the host's network),
use a high port on both sides (`"1502:1502"` and `0.0.0.0:1502`); the setup file follows it.

**Modbus has no password.** Anyone who reaches the port reads the roaster, and with writes on
can command it, through the same gate as the dashboard (the roaster's control level, its outputs'
ranges, its fan floor; see [roaster setup](roaster-setup.md)). Keep the port on the roaster's LAN, never forward
it. Every write is in the audit log either way (below).

## Connecting Artisan

1. Get the setup file: **Settings → About → Artisan screen → Setup for Artisan (.aset)**, or
   `roast artisan-screen --aset Roastwarden.aset --host <the server's LAN address>`. The server
   fills in its own address (the one the browser or `roast` used; `--host` when Artisan reaches
   it by another), the slave's port and the unit (the dashboard's °C/°F; `--fahrenheit`).
   Without the server at hand, the compose bundle has the same file as `artisan/Roastwarden.aset`
   with `host=SERVER_ADDRESS`: put the address in that line.
2. In Artisan: **Help → Load Settings…**, pick the file. It sets the device (Modbus), the
   connection, the four inputs, the extra curves and the unit, and leaves the rest of Artisan as
   it was (profiles, alarms, palettes). Help → Save Settings first keeps a copy of the old ones.
3. Press **ON**. BT and ET are drawn at once, with Heater and Fan as extra curves (their LCDs on).
   Press **START** with Roastwarden's CHARGE to record the roast in Artisan too; the two records
   are independent.

`roast artisan-screen` (and `roast status`, and About) say whether the slave is on, the roaster
it serves (or why none), read-only or with writes, and when Artisan last read and from where.

## What Artisan reads

The slave serves the "Roastwarden Standard" map (the Modbus TCP section of the
[hardware integration guide](hardware-integration-guide.md)): input registers 0–7, function code 4, float32 with the high word first. The setup
reads them in one block a second:

| Artisan | Register | Type |
|---|---|---|
| BT (input 1) | 0–1 | float32, °C (Artisan converts for °F) |
| ET (input 2) | 2–3 | float32, °C |
| Heater (extra device MODBUS_34, input 3) | 6 | whole %, no conversion |
| Fan (MODBUS_34, input 4) | 7 | whole %, no conversion |

The holding registers read back what the roaster reports now (setpoint, fan, heater, mode,
heater enable). While the roaster is silent (nothing for 10 s), or there is no roaster to serve,
a read is answered "gateway target failed to respond": Artisan draws a gap (-1) and notes
"Modbus Communication Error" rather than a line at 0 °C, and picks up again by itself.

## Control from Artisan (writes on)

With `ROASTWARDEN_MODBUS_WRITES=on` the setup file also has two sliders: **Air** writes the fan
(holding register 2) and **Burner** switches the heater on and writes its % (registers 5 and 3,
`writeSingle(1,5,1);writeSingle(1,3,{})`: on the firmware a heater % does nothing while the
heater is off). Each write becomes the same command the dashboard sends, through the roaster's
control gate: refused when control is off for the roaster or the output is not declared, shaped
to its range and fan floor otherwise. A refusal is answered "server device failure", which
Artisan does not show (its writes do not wait for an answer): the curve not moving is the sign,
and the audit log has the reason.

Writes are your choice: they let a second program command the roaster, and Artisan's
sliders know nothing of a profile drive Roastwarden is running. Read-only is the default, and
the right one for a screen.

## The audit log

Every write, refused or not, is a row: actor `modbus`, method `MODBUS`, path
`/modbus/holding/<register>`, the client's address, the roaster and what it asked for (kind
`control`; Settings → Audit, `GET /api/audit?kind=control`). A write while writes are off is
logged with status 403 and "read-only".

## Not yet

- One roaster per slave. With two real roasters, set `ROASTWARDEN_MODBUS_DEVICE_ID` to the
  one Artisan should show.
- Artisan's START/CHARGE/DROP do not reach Roastwarden, nor Roastwarden's events Artisan: each app
  keeps its own record. Artisan's event buttons could write a "mark" register later.
- A setpoint slider (Artisan's `writeWord(1,0,{})` writes the float the slave takes) is left out:
  a fluid bed's setpoint belongs to Roastwarden's profile drive.
- The setup in Artisan's Machines menu (Artisan asks for the host then): it would be a pull
  request to Artisan.

<!-- site: end -->

## Design notes

For developers: why Artisan can be a screen, how the setup file is made and how it is checked
against Artisan. The site publishes only the part above.

The slave lets an Artisan user try Roastwarden without giving anything up. It was checked
against Artisan 4.2.0 itself on the virtual roaster (below, "Checking it"). One roaster per
slave: a second roaster could be a second unit id, not built.

## Why

Someone who roasts with Artisan has years of habit in its screen: the curves, the LCDs, the
event buttons, their saved profiles. Roastwarden runs the roast in a server and is a better place
for the roaster to live (a roast carries on with no laptop, any phone is a full view, control
and alarms run where the roaster is), but a switch that asks for all of that at once is a hard
sell. So the server speaks Modbus TCP as a roaster would, and Artisan connects to it as to any
Modbus roaster: Artisan draws and records the roast it reads, Roastwarden records the same roast,
and the owner moves over when they are ready.

Nothing changes for someone who never had Artisan: the slave is off unless it is switched on,
and the only place it shows is one line on Settings → About.

## The setup file

`crates/server/assets/artisan/screen.aset` is the read-only file the server fills in;
`screen-sliders.aset` replaces its last section, `[Sliders]`, when writes are on. Each key was
checked against Artisan 4.2.0's code and by loading it in Artisan 4.2.0 itself.
The ones that are not obvious:

- `Mode` is required: Help → Load Settings refuses a file without it, and it sets Artisan's
  unit (so the server writes `F` for a °F user).
- `wordorderLittle=false`: Artisan's default is little, which reads BT 150.5 as nonsense.
- `inputNdeviceId=1`: a device id of 0 switches an input off in Artisan; any unit id reaches the
  slave. `input3mode=` and `input4mode=` empty: with `C` a °F Artisan would convert 40 % to 104.
- `extradevices` in Qt's `@Variant` form: a one-element list written plainly is read as a string
  and split into two devices. `extraname1=Heater,` (a trailing comma makes a list): the extra
  device's pair comes back swapped, so name 1 is input 3.
- All six `[ExtraComm]` keys with one element each: Artisan checks that the extra devices and
  their serial settings have the same length, and without them Heater and Fan read -1.
- `fetch_max_blocks=true`: one read of registers 0–7 a sample. `input5…10deviceId=0`, empty
  sliders and `[DefaultButtons]` clear what an Artisan set up for another roaster would still do.
- No `[System]` section: an `artisan_version` of 4.2 or later there makes Artisan ask for a
  signature.

## Checking it

The setup was checked in the real Artisan 4.2.0 (built from its git tag, run headless in
Docker): it loads a setup file the way Help → Load Settings does, presses ON, and reports what
Artisan itself read each sample. Against the virtual roaster: BT, ET, Heater and Fan read on
every sample, in °C and °F; the Air slider to 70 % and the Burner to 55 % reached the virtual
roaster and came back in Artisan's curves; read-only, the write was refused, logged and the heater stayed off; with no roaster to
serve, Artisan drew gaps. That check runs by hand, not in CI; the slave itself is tested in
`modbus.rs` (a Modbus client against the whole server: reads, refusals, the gate, the audit).
