# Control architecture: where the BT loop runs

How Roastwarden controls a roaster: what you can set by hand, how the bean temperature is held,
what stops the heater when something goes wrong, and what scripts and agents may do. The
server runs the roast (profiles, [Profile drive](replay.md), alarms, [fan assist](fan-assist.md),
the [roast sequence](roast-sequence.md)); the roaster measures, switches its heater and fan,
and keeps itself safe when the server is gone.

## How much control a roaster gets

Each roaster has a control level, set under Roasters → the roaster → Control → Control
([roaster setup](roaster-setup.md)):

| Level | The server may send |
|---|---|
| Off · logs only | nothing: it only records. A newly added roaster starts here. |
| Manual | heater %, fan %, heater power on and off, emergency stop |
| Full | everything: setpoint, mode, PID gains, Profile drive, autotune |

A command the level does not allow is refused. Features that need more than the level quietly
skip the roaster (the roast sequence, fan assist) or refuse to start (Profile drive, autotune).
A heater or fan % is kept to the output's range, and while the heater is on, a fan below the
roaster's fan floor is raised to it.

## The Control tab

On the Roast page, the Control tab holds the roaster's controls:

- **Manual / Auto (PID).** In Manual you set the heater % and the setpoint is not used. In
  Auto the roaster's PID moves the heater to hold BT at the setpoint; the heater slider shows
  what the PID does. Switching to Auto starts the PID from the current heater, so there is no
  jump.
- **Setpoint**: the BT the PID holds in Auto.
- **Fan** and **Heater** sliders, in %.
- **Power**: heater power, the master switch. Off cuts the heater in any mode. Drop and
  completing the roast switch it off for you.
- **Advanced: PID gains**: Kp, Ki and Kd, and **Apply gains now**. With the
  [gain schedule](autotune-2.md) on, the next fan change replaces them.

While a profile drives, touching the setpoint, heater, fan or mode pauses that channel only.

## Where the PID runs

During Profile drive, the loop that holds BT runs in one place, set by **PID runs**:

- **On the roaster**: the roaster runs its own PID and the server sends the setpoint once a
  second. The loop runs next to the heater, at the sensor's rate, and is not bothered by
  Wi-Fi delays. Best when the roaster has a PID.
- **On the server**: the server runs the PID once a second and sends heater %. It works with
  any roaster that takes a heater %: a TC4, Modbus I/O.
- **Auto** (the default) picks the roaster when it can.

The server keeps the PID gains. If the roaster reboots and comes up with other gains, the
server sends its own back.

## Emergency stop and safety

The red **STOP** button in the header shows while the selected roaster has something to
stop. It switches heater power off, sets the heater to 0 % and sends the roaster's emergency
stop: heater off, fan to full. Once the roaster has cooled, the button becomes **Fan off**.

A roaster must keep itself safe without the server. Its own firmware has to cut the heater on
an over-temperature, a failed thermocouple, too little airflow, or a lost connection
([hardware integration guide](hardware-integration-guide.md)). On a roaster with a control
lease, Profile drive renews the lease every 3 s; if the server goes quiet, the roaster cuts
its heater after 10 s and goes to its safe state (heater off, fan to full). The drive also
stops when the roaster's readings stop coming.

Roasters → the roaster → Overview → **Device contract** shows what the roaster reports: its
firmware, whether its readings have BT, ET, heater and fan, what it supports, and whether its
PID gains match the ones the server holds. `roast roaster contract <roaster>` gives the same.

## Scripts and agents

The owner key, and a browser signed in with it, may control any roaster its level allows.
API tokens (Settings → Tokens) have a preset (Read-only, Roast, Inventory, Admin) and an
expiry, and are held back from real hardware:

- **Tokens may control this roaster** (Roasters → the roaster → Control) is off for every
  physical roaster until you switch it on; only the owner key can. While it is off, a token
  may read the roaster but not command it, start or change its roasts, or change its tuning.
  It may still fill in a finished roast's record: weights, landmarks, cupping. The virtual
  roaster is always open to tokens, for practice.
- **Ask first** (on by default for a new token) holds some calls for your approval: starting a
  roast on a physical roaster, and deletes. A label print and switching a roaster's control on
  always wait for you, whatever the token. The request goes out through your
  [notification channels](notifications.md) and waits an hour under Settings → Approvals,
  where you **Approve** or **Deny** it. An agent can never approve its own request.
- **Settings → Audit** lists every change made through the API, who made it and whether it
  was refused, kept for a year.

Live control (heater, fan, setpoint, mode, stop) has no `roast` command on purpose: it is for
the person at the roaster. `roast audit` reads the audit log and `roast approval <id> --wait`
waits for an answer ([the roast CLI](cli.md)).

<!-- site: end -->

## Design notes

For developers: how control works and why. The site publishes only the part above.

Server, dashboard and firmware 3.1.0 carry the design below, checked on the roaster (replay on
the device loop, feedforward, lease renewal, lease expiry into the safe state). It builds on the
replay engine (`docs/replay.md`), with the ESP32 firmware changing too, and with other roasters
in mind.

## Principle

The server owns the **strategy**: profile curves, events, alignment, ghost
runs, overrides, logging. The **bean-temperature loop** (BT error → heater)
runs in exactly one place per replay run, chosen from what the device can do:

- **Device loop**: the device runs its own PID. The server streams the
  setpoint (and a feedforward heater %) once a second. The loop runs at
  sensor rate next to the heater and ignores network jitter. This is the
  default for the Roastwarden firmware.
- **Server loop**: the server runs the PID at 1 Hz and sends heater %. This
  works with any roaster that accepts a heater output: TC4/aArtisan,
  Modbus I/O, a future third-party roaster. It is what Artisan's software PID
  does.

Profiles stay device-agnostic: heater % means **% of heater power**, BT is °C,
fan is %.

## The control gate

Every command, from every feature, passes one check in `TransportRegistry::send` first
([`roaster-setup.md`](roaster-setup.md)): the roaster's control level. `off` (a roaster with no
setup, so any roaster added or discovered) gets nothing; `manual` gets heater %, fan %, drum %
and damper % when declared, heater on/off and emergency stop; `full` gets everything. A
refused command answers 409 `CONTROL_OFF`. The roast sequence, fan assist and the gain schedule
skip a roaster below their level quietly; Profile drive (except a ghost) and the autotune
refuse to start. The roasters that existed before the gate (the ESP32, the virtual roaster)
were set to `full`. The same check also holds the roaster's **outputs**: a command for an
output the roaster has not declared (a logger has none) answers 409 `OUTPUT_NOT_DECLARED`; a %
is clamped to the output's range and step; and while the heater is on a fan below the
roaster's fan floor (45 % on the fluid bed, none on a drum) is raised to it
([`roaster-setup.md`](roaster-setup.md), "Outputs", "The interlock" and "Kinds' defaults").

## Device contract

Written with firmware 3.5.0: what a roaster must do to work with Roastwarden, and what it may offer. The server runs the roast (profiles, Profile drive,
the gain schedule, Autotune 2, alarms, fan assist, the roast sequence); the device measures,
switches the outputs, and keeps the roaster safe when the server is gone. A device that does
more than this (its own autotune, a RoR limiter, stored gains) ends up as a second controller
fighting the first: firmware 3.4.0's RoR limiter cut the heater 19 times in one recorded roast
while Profile drive caught up, which is why 3.4.1 and 3.5.0 took those out.

**Required**

| What | How (MQTT; the other transports in `hardware-integration-guide.md`) |
|---|---|
| BT and ET in °C, about once a second | telemetry `beanTemp`, `envTemp` |
| heater % and fan % in | `control/heater`, `control/fan` (0–100, decimals allowed); `heater_enable`, `mode` |
| the heater and fan it applies | telemetry `heater` (command), `heaterApplied`, `fan` |
| emergency stop | `control/emergency_stop`: heater off, fan to full |

| over-temperature cutoffs | heater off at a BT and an ET limit of its own, whatever it was told |
| sensor checks | a thermocouple that reads nothing for a few samples cuts the heater |
| fan interlock | no heat without airflow (Roastwarden firmware: fan above 40 %) |
| comms watchdog | no controller for a while (no MQTT and no serial master, or Roastwarden's retained presence `offline`) cuts the heater |
| presence | a retained `status` message, `online` with a last will `offline` |

These have to work with the server, the network or the broker gone: they are the only control
that does not depend on Roastwarden.

A drum roaster may also take `control/drum` and `control/damper` (%) when its setup declares
those outputs; a device with its own field names, units, topics or command syntax is adapted by
its connection's mapping rather than by this contract (`ingest-mapping.md`).

**Optional capabilities** (listed in the status message's `caps`, below)

| cap | What the device offers | Without it |
|---|---|---|
| `pid` | an onboard PID on BT: `setpoint` in auto mode, gains by `control/pid` | the server runs the PID and sends heater % (any roaster) |
| `ff` | feedforward heater % added to the onboard PID | `feedforward_pid` runs on the server loop |
| `lease` | a control lease: heater off when the server stops renewing it | the comms watchdog is the only fail-safe (slower) |
| `tpc` | heater % is time-proportioned power, linear in % | heater % is whatever the output stage does with it |
| `oversample` | thermocouples read several times per sample and averaged (information only) | — |
| `ror` | an onboard RoR limiter (`control/ror_limit`); Roastwarden firmware 3.2 to 3.4.0 only | the API answers 409 for a roaster that lists its caps without it |

The onboard PID is worth keeping on a Wi-Fi roaster: the loop runs at sensor rate next to the
heater and does not see network jitter.

**Gains belong to the server.** Firmware 3.5.0 keeps PID gains in RAM only and boots on the
build's defaults. The server holds them: the newest stored change (`pid_changes`, what the
dashboard's Apply and the CLI store) or, with the gain schedule on, the schedule's gains for the
fan. On the first sample from a roaster after the server starts, and on a sample whose uptime
went back (the roaster rebooted), the server sends the gains it holds if the roaster reports
others (`schedule.rs`, `GainScheduler::on_sample`; any transport). `control/pid`'s `persist`
flag only mattered to 3.2–3.4, which wrote gains to NVS unless it was `false`.

**Checking a roaster against it**: `GET /api/roaster/{id}/contract` (`routes/contract.rs`;
`roast roaster contract <roaster>`; Roasters → the roaster → Overview → Device contract) gives
its `firmware`, `reads` (BT, ET, heater and fan present in its latest sample), `caps` (as
reported, or inferred from its telemetry: `reported: false`), and `gains`: `reported` (its
onboard PID's), `held` (what the server would send now), `matches`, and `last_boot`, the last
check after a boot since the server started (`why`: `server_start` or `reboot`; `reported`,
`held`, and `sent`, null when it already had them or the server holds none). It reads only;
404 for a roaster not heard since the server started. The safety parts (cutoffs, interlock,
watchdog) are the firmware's and cannot be seen from here: the hardware guide's checklist
covers them.

**Not part of the contract** (the server does these): autotune (Autotune 2 runs the relay from
the server, `autotune-2.md`; the firmware's own relay test went in 3.5.0, its stored runs stay
readable), RoR limiting, profile timing, events and alarms.

## Device capabilities

The firmware reports what it supports in its retained status message:

```json
{"status":"online", "version":"3.5.0", "caps":["pid","ff","lease","tpc","oversample"], ...}
```

The caps are those of the contract's table above.

Devices that send no `caps` are inferred: `pid` when their telemetry carries a
setpoint (old firmware, TC4 with aArtisan PID, a Modbus map with a setpoint
register), nothing else. A run's `loop` option is `auto` (default), `device`
or `server`; `auto` picks the device loop when the mode's needs are met
(`pid`, plus `ff` for feedforward+PID), else the server loop. The replay state
reports `loop`, `loop_reason` and `caps`.

The server subscribes to `roaster/#` only after its MQTT consumer is
listening, so retained status messages (and the capabilities in them) are not
lost at startup.

## Modes × loop location

| Mode | Device loop | Server loop |
|---|---|---|
| `setpoint` | auto mode; `setpoint` = profile BT at t + lookahead | manual; server PID on the same target → `heater` % |
| `feedforward` | manual; `heater` = profile heater % | same |
| `feedforward_pid` | auto; `setpoint` = profile BT, `feedforward` = profile heater % | manual; server PID + feedforward → `heater` % |

Lookahead defaults to the `profile_lookahead_seconds` setting (20 s) in
`setpoint` mode and to 0 in `feedforward_pid`, where the feedforward already
anticipates. Setting `lookahead_seconds` explicitly overrides both.

The server PID is a full PI(D) in heater %/°C with
derivative on measurement, conditional-integration anti-windup and
feedforward, gains from the setting `server_pid` (JSON `{kp, ki, kd}`),
overridable per device as `server_pid:<device_id>`. Defaults kp 3, ki 0.03,
kd 0. The MAX6675's 0.25 °C steps at 1 Hz make D mostly noise. On a stale reading it holds
its last output (dropping P and D stepped the heater by kp·e); a gain change mid-run (the
gain schedule) moves the integral by the change in the P term, so the output does not jump;
engaging a ghost run starts it again from the heater the roaster has.

## Control lease

While a live run drives a lease-capable device, the server sends
`control/lease` = 10000 (ms) when the run starts and every 3 s after that.
If the lease runs out (the server process hangs, the replay task dies, the
network partitions while MQTT still looks connected), the firmware enters its
safe state: heater off, fan to maximum, status `COMMS_TIMEOUT`. That is the
comms watchdog's fail-safe, reached in 10 s instead of 60 s and without depending
on the broker's last-will. If the device's heater-enable flag drops during a
live run (safe state, or the operator cut the heater), the replay stops
itself. Stopping a run releases the lease
(`control/lease` = 0); a manual operator never holds one, so dashboard control
is unaffected. The lease is renewed only while the roaster's telemetry is fresh
(under 8 s old): a roaster the server can still reach but no longer hears is left to fall
into its safe state, and the drive stops at 20 s (`replay.md`, "Lost telemetry").

Firmware 3.4.0 and older could cut a lease that had just been renewed: the safety check read
the clock, the MQTT task stored a renewal stamped a moment later, and the unsigned age
wrapped to 49 days (a recorded roast showed a cut 0.5 s after a renewal, 4:08 into the roast).
3.4.1 signs the age and reads the lease before the clock. When a roaster reports stop code 7
within 10 s of a renewal, the drive's stop reason says the server renewed it and the roaster
missed it, rather than "the server went quiet".

## Firmware changes (3.1.0)

1. **Heater output as time-proportioned power.** The roaster's SSR is DC-in /
   AC-out zero-crossing (3-32 VDC in, 24-380 VAC out); driven with 5 kHz PWM
   it does not deliver power proportional to duty. The heater now switches in
   a 1 s window (`HEATER_WINDOW_MS`; ~1 % resolution at 50/60 Hz mains),
   from `loop()`: a 1 Hz LEDC is not usable on the ESP32 because each duty
   update busy-waits with interrupts off until the period boundary, which
   tripped the interrupt watchdog. `HEATER_FAST_PWM` keeps the old 5 kHz LEDC
   output for DC SSRs.
2. **PID on the sensor clock.** Compute once per new BT sample with the sample
   time set to the read interval (was every 100 ms on a 1 Hz input, so the
   derivative spiked on each new reading).
3. **Bumpless transfer.** Switching to auto starts the PID from the current
   output instead of a stale integral.
4. **Feedforward.** `control/feedforward` (heater %, or `off`): in auto,
   output = feedforward + PID, with the PID's limits shifted so the sum stays
   in 0–100 % and the integral does not wind up.
5. **Lease** as above.
6. **Capabilities** in the status message, and `ff` in telemetry.

Gains keep their current units (PID output 0–255), so stored and autotuned
gains stay valid. The pure arithmetic lives in `include/control_math.h`,
unit-tested on the host (`pio test -e native`).

## Server changes

- `roastwarden-core`: `DeviceCommand::Feedforward { value: Option<f64> }` and
  `DeviceCommand::Lease { ms: u32 }` (MQTT/WebSocket topics `feedforward`,
  `lease`; TC4 and Modbus report them unsupported).
- Capabilities cache fed by status messages, with the inference fallback.
- Replay engine: a `LoopLocation` per run, the server PID, feedforward sent as
  its own command on the device loop, lease heartbeat in the service.
- Replay state and panel show where the loop runs and why.

## Heater and fan in percent (firmware 3.3)

Heater and fan are percent end to end: API, dashboard, profiles, replay,
storage and the device wire. Before 3.3 the MQTT interface took the heater
in % but the fan as 0–255 PWM, and reported both the same way; the serial
(TC4) interface was already % for both.

| where | heater | fan |
|---|---|---|
| REST | `POST control/heater {"percent"}` | `POST control/fan {"percent"}` |
| MQTT command | `control/heater` `62.5` | `control/fan` `75` |
| telemetry JSON | `heater`, `heaterApplied` | `fan` |
| v2 sample | `h` | `f` |
| `session_telemetry` | `heater` REAL | `fan` REAL |
| autotune results | — | `fan` |
| TC4 serial | `OT1;<whole %>` | `IO3;<whole %>` (or `OT2` / `DCFAN`, the connection's `fan_output`) |
| Modbus | register value × `scale_factor` = % | same |

`control/heater_pwm`, `control/fan_pwm`, `heaterPWM` and `fanPWM` are gone,
with no fallback: server and firmware switch together. Migration 013 moves
stored data: session telemetry and device-profile fan values are divided
by 2.55, and `autotune_runs.fan_native` is dropped. Inside the firmware the
heater stays 0–255 because the PID gains are in those units; the fan becomes
an 8-bit LEDC duty only when written. The heater interlock is
`SAFETY_MIN_FAN_PCT` 40 % (was PWM 100, about 39 %).

## Watchdog and OTA

The Arduino core starts the task watchdog itself (5 s), so the firmware's
`esp_task_wdt_init(30 s)` had always been rejected. ArduinoOTA uploads inside
`loop()`, so a 1.2 MB OTA over the roaster's Wi-Fi reset the chip ~12 s in;
3.0.0 could not be updated over the air. 3.1.0 reconfigures the watchdog
(10 s), feeds it from OTA progress and turns the heater off when an OTA
starts. From 3.1.0 on, OTA updates work again.

## Firmware before 3.1.0

The server works with firmware before 3.1.0: no `caps` → `pid`
inferred → `setpoint` runs on the device, `feedforward_pid` falls
back to the server loop. Flashing 3.1.0 moves `feedforward_pid` onto the
device and adds the lease. Flashing is OTA from a host on the roaster's LAN
(see the firmware's DEVELOPMENT.md) and is the owner's call.

## Audit log

Every state-changing API request (any non-GET under `/api/`) gets a row in `audit_log`
(migration 022, `crates/server/src/auth/audit.rs`), refused ones included: control commands,
roast actions, profiles, settings, backups. A row has:

- **who**: the credential (`open` when the server has no owner key, `owner key`, `owner
  login` for a browser signed in with it (SECURITY.md, "Logins"), `token` with the token's id
  and name, whether sent or signed in with, `none` when refused, with the token's name when a revoked or
  expired one was sent), the edge proxy's signed-in user
  (`Remote-User`, `X-Forwarded-User` or the email variants, if the proxy passes one), the
  User-Agent and the client address (`X-Forwarded-For`, first hop);
- **what**: method, path, the roaster (from `/api/roaster/{device_id}/…`, the session's
  roaster for `/api/sessions/{id}/…`, or a `device_id` in the body), and the JSON body with
  any field named like a password, secret, token or API key masked, and the secret settings
  (`notify_webhook_url`, the notification tokens) masked by name, in a batch or as the
  `value` of `PUT /api/settings/{key}`; cut at 4 KB. At most 1 MB of a body is read for
  the log before auth has seen the request; a larger one goes on to its handler untouched
  and the row says only "(more than 1024 KB, its type)";
- **result**: the HTTP status.

Reads are not logged. Commands the server sends by itself (the roast sequence, alarms,
Profile drive, fan assist) are not API requests and are not in it; the request that started
them is. Rows are kept a year. Settings → Audit shows them in words.

Each row has a **kind**, from its path (`audit::KINDS`, the first matching rule wins):
`control` (control commands, autotune, the simulator), `roast` (sessions, events, Profile
drive), `profile`, `beans` (beans, stock, purchases, the shopping list, roasted coffee and the plan),
`labels` (label checks and one-off renders; saved designs, assets and brands; share links),
`roaster` (roaster setup: devices, connections, fan assist, gain schedule, sequence),
`settings` (settings, notifications, Web Push), `access` (tokens, approvals, the per-roaster
token switch, signing in and out, the signed-in browsers), `backup`, and `other`. A sign-in's
key or token is masked like any field named `credential`. A test fails when a write route lands in `other`
(`every_write_route_has_a_kind`), so a new route needs a rule.

`GET /api/audit` lists them newest first and filters on the server, so paging (`before`)
walks the matches. It is the owner's and admin tokens' alone, whatever
`ROASTWARDEN_READS` says: the bodies hold every change made.

| Parameter | Keeps |
|---|---|
| `limit` | 1–1000 rows (100) |
| `before` | rows older than this id |
| `device_id` | that roaster's |
| `kind` | these kinds, comma-separated; an unknown one is a 400 |
| `q` | text anywhere in the path, body, token name, signed-in user, roaster, address or client (`%` and `_` are plain text; ASCII case ignored) |
| `token` | requests sent with the token of that name |
| `failed` | `true`: only refused or failed ones (status 400 and up) |

Settings → Audit has the kinds as chips (any of the chosen), a search box, the roaster and
token pickers and "Refused or failed only"; `roast audit --kind --find --by --failed` the
same (`docs/cli.md`).

## Tokens

API tokens let scripts and agents use the API with less than the owner key can do
(`crates/server/src/auth/tokens.rs`, migration 023). The owner creates them under Settings →
Tokens with a name, a preset and an expiry (30 days, 90 days, a year or never). A token is
`rr_` and 40 random letters and digits; the server keeps only its SHA-256 and the first six
characters, and shows the secret once. It is sent like the key: `Authorization: Bearer rr_…`
or `X-API-Key`, never in a query string.

| Preset | May |
|---|---|
| read-only | every read: roasts, profiles, telemetry and its WebSockets, settings; not the audit log |
| roast | reads, and what the Roast page does during a roast: create, start, pause, complete and cancel a roast, edit its details, mark events, Profile drive, setpoint, fan, heater, mode, heater power, emergency stop, cupping notes, the roast plan, the simulator's charge and dump |
| inventory | reads, and the green coffee: add, edit and delete beans, record stock (purchases, orders and corrections, and edit or remove them), import a coffee from a supplier's link, the shopping list, roasted coffee used, given, thrown away or counted (`docs/roasted.md`), the roast plan; bag labels: saved designs and their versions, assets, brand kits, a coffee's own label (`docs/labels.md`). Nothing on a roaster or a roast: for an agent that records a purchase or designs labels (`docs/cli.md`) |
| admin | everything the others may, and: the audit log, PID and RoR limit, autotune, gain schedule, fan assist, the roast sequence, profiles, beans, devices, settings, backups (not restoring one), deleting and importing roasts, the test and MQTT endpoints |

Access is decided by method and path before any handler runs, deny by default: a route no
rule names is refused to every token, and a test (`every_route_is_classified`) fails when a
route is added without a rule. A token that is unknown, revoked or expired gets 401 even for
a read. A browser may sign in with a token (Settings → This browser): it then acts as that
token, until the token is revoked or expires.

**The owner key's alone** (or a browser signed in with it), reads included: listing,
creating, revoking and removing tokens, listing and signing out the signed-in browsers,
the per-roaster switch below, the write check (`POST /api/roaster/{d}/check/write`: it sends
heater 10 % to the roaster; [`add-roaster.md`](add-roaster.md)), and restoring a backup (which
would bring back the tokens and switches of the day it was taken).

**A roaster's setup** (`PUT /api/roaster/{d}/setup`): an **admin** token's
`PUT` that leaves control `off` runs at once: kind, outputs and channels, none of which sends
anything, and turning control off. One that leaves control `manual` or `full` (switching it on,
or any change while it stays on) waits for the owner's approval ("Approvals" below), whatever
the token's `require_approval`. The real-hardware rule below lets that `PUT` through, as it
does the ingest preview (`POST /api/roaster/{d}/ingest/preview`, any token: nothing saved or
sent). This is what lets an agent add a logger (`roast roaster add`, `docs/add-roaster.md`)
and ask for control (`roast roaster control`) without ever giving itself control. Without `ROASTWARDEN_API_KEY` the API is open and tokens
restrict nothing, so the server refuses to create one.

**The real-hardware rule.** Whatever its preset, a token may not change anything under
`/api/roaster/{d}/…`, or `/api/sessions/{id}/…` for a roast on roaster `d`, unless `d` is
`virtual_roaster` or the owner switched on **Tokens may control this roaster** under
Roasters → the roaster → Control. The switch lives in its own table
(`roaster_token_access`), not in `/api/settings`, which an admin token may write. The
refusal is a 403 that says where to allow it. Reads are never affected.

The record of a **finished** roast is the exception: its details and weights
(`PUT /api/sessions/{id}`), landmarks (`POST …/events`, `PUT`/`DELETE …/events/{eid}`),
cupping and Save as Profile reach no roaster (the roast sequence and Profile drive act only
on an active roast), so a token with the preset for them may make them on any roaster: an
agent can enter the roasted weight or move first crack after the owner roasted
(`tokens::finished_roast_record`). Starting, resuming or driving a roast, and anything on a
roast that is not finished, stay behind the switch.

**Approvals** (`crates/server/src/auth/approvals.rs`, migration 026). A token
that asks first (`require_approval`; Settings → Tokens, on by default for new tokens, "Stop
asking" / "Ask first" per token, `PUT /api/tokens/{id}`) cannot make these calls on its own:

| Call | Held when |
|---|---|
| `POST /api/sessions/{id}/start` | the roast is on a roaster other than `virtual_roaster` |
| `DELETE /api/sessions/{id}`, `/api/beans/{id}`, `/api/profiles/{id}`, `/api/devices/{id}` | always (a missing one is a plain 404) |
| `DELETE /api/label-designs/{id}`, `/api/label-assets/{name}`, `/api/label-brands/{id}` | always (`docs/labels.md`, "Label studio") |
| `POST /api/sessions/{id}/labels/print` | always, for **every** token, also one that does not ask first: a print uses a sheet someone must load (`docs/labels.md`, "Printing") |
| `PUT /api/roaster/{d}/setup` | the body leaves control `manual` or `full`, for **every** token: switching control on is the owner's yes ([`roaster-setup.md`](roaster-setup.md), "Tokens and the setup"); action `roaster.control` |

The preset and the real-hardware switch are checked first: an approval never lets a token do
what they refuse. A held call answers **202** with `{approval, message, wait_url}`; the same
call again while it waits, with the same body, gives the same request, and a different body is
a request of its own (another print of the same roast is not the first one's). The request goes out through every
notification channel (`docs/notifications.md`) with a link to Settings → Approvals
(`public_url` sets the address links start with; else the address the call came to) and
waits an hour. The caller may wait with `GET /api/approvals/{id}?wait=25` (any token, or
none: the id is 22 random characters). Only the owner key lists (`GET /api/approvals`),
approves and denies (`POST /api/approvals/{id}/approve|deny`), so an agent can never
approve itself. On approval the server makes the call there and then, as the token, through
every layer (preset, switch, handler, audit row naming the token); a token revoked or
expired meanwhile does nothing (`410`, `TOKEN_GONE`). A request's status: `pending`,
`approved` (running), `done`, `failed` (the call answered an error; `result_status` and
`result` say which), `denied` or `expired`. Live control during a roast is never held: the
per-roaster switch is its gate.

**Errors.** Every error under `/api/` is one JSON body (`crates/server/src/http/errors.rs`):
`{"success": false, "error": {"code": "…", "message": "…"}}`. The code
follows the status (`BAD_REQUEST`, `UNAUTHORIZED`, `FORBIDDEN`, `NOT_FOUND`,
`METHOD_NOT_ALLOWED`, `CONFLICT`, `UNPROCESSABLE` for a body of the wrong shape,
`INTERNAL_ERROR`, `UNAVAILABLE`), except for refusals by the credential check, which say why:
`TOKEN_INVALID`, `TOKEN_REVOKED`, `TOKEN_EXPIRED`, `OWNER_ONLY`, `TOKEN_SCOPE` (the preset
does not allow it), `REAL_HARDWARE` (the switch above is off), `TOO_MANY_FAILURES` (429: ten
refused credentials from one client within a minute; `Retry-After` says when to try again;
`SECURITY.md`), and the control gate's
`CONTROL_OFF` (409: the roaster's control level does not allow the command, for every
credential; [`roaster-setup.md`](roaster-setup.md)) and `OUTPUT_NOT_DECLARED` (409: the roaster
has no such output). An `/api/` path no route knows
is a `NOT_FOUND`; the message is for people, the code for scripts and agents.

`GET /api/auth/status` says what the request's credential is (`open`, `owner key`, `token`
with its name and preset, or `none`), so the dashboard can tell the owner key from a token.
