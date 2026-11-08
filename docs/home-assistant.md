# Home Assistant

Each roaster can show up in Home Assistant as a device: its temperatures, rate of rise,
heater, fan, the roast's phase and time, and the RoR warning, on your Home Assistant
dashboards and in its automations (a lamp that turns red at first crack, a message when the
roast ends). It is read only: no entity sends the roaster anything, so an automation cannot
raise a heater. Control stays with the dashboard and the `roast` command. It is off until
you switch it on.

## Setting it up in short

1. In Home Assistant, set up the MQTT integration (Settings → Devices & services → MQTT),
   pointed at the broker you pick below.
2. In Roastwarden, Settings → Home Assistant: tick **On** under Discovery, pick the
   **Broker**, **Save**.
3. Within a few seconds the roasters appear among the MQTT integration's devices. The page
   then lists them under "In Home Assistant", each with whether it is available and its
   entities, and says what is wrong when nothing is published ("not connected to … yet").

Needs Home Assistant 2024.11 or newer (it uses device discovery).

### The broker

- **The server's broker, the one the roasters use** (the default). Home Assistant needs a
  user there that may only read. With Roastwarden's built-in broker, set
  `MQTT_HOME_ASSISTANT_PASSWORD` in `.env` and restart: Home Assistant then signs in as
  `homeassistant`, may read only `homeassistant/#`, `roastwarden/ha/#` and
  `roastwarden/server/status`, and cannot see a roaster's own topics or send it a command.
  The same variable makes the same user on the Mosquitto in the compose file. On a broker of
  your own, make such a user yourself.
- **Home Assistant's own broker**: enter its Address (`mqtt://host[:port]`), User and
  Password. The server opens a second connection and publishes there only; the roasters stay
  on their broker. Plain MQTT only, no `mqtts://`.

### Options

| Option | Default | What it does |
|---|---|---|
| Discovery prefix | `homeassistant` | Change it only if you changed it in Home Assistant |
| Every … s | 5 | How often each roaster's values are sent (1–60 s) |
| The simulated roasters too | off | Announce the simulated roasters as well |

## What it shows

One device per roaster, named as the roaster (Roasters), with the roaster's firmware as its
software version and, when `public_url` is set, a link to its page on the dashboard.

| Entity | Kind | From |
|---|---|---|
| Bean temperature | sensor, °C (Home Assistant converts to its own unit) | BT |
| Rate of rise | sensor, °C/min | the server's RoR (the RoR setting's window) |
| Environment temperature | sensor, °C | ET, when the roaster sends it |
| Setpoint | sensor, °C | when the roaster sends it |
| Heater, Fan | sensor, % | when the roaster sends them |
| Phase | enum: idle, drying, maillard, development, cooling | the open roast's marks: DRY end, first crack, DROP; cooling while the roast sequence cools |
| Roast time | duration, s | since CHARGE, while a roast is open |
| Roasting | binary sensor (running) | a roast is open |
| Roast | text | the open roast's name |
| RoR warning | enum: none, crash, flick, stall; attribute `reason` | the live prediction's RoR watch ([`prediction.md`](prediction.md), "RoR warnings") |

An entity is announced only for what the roaster reports (a thermocouple board with BT alone
gets no heater or fan); once seen it stays. A value missing in a message reads unknown.
A roaster heard but waiting to be set up (Roasters) is not announced until it is; the simulated
roasters only with "The simulated roasters too" (`ha_virtual`).

## What it does not do

- No control from Home Assistant: there is no fan or setpoint entity, on purpose. An
  automation could act with no one at the roaster.
- No `mqtts://` to Home Assistant's broker.
- The predicted first crack and DROP are not entities (they are in `roast live`).

From the command line, `roast home-assistant` shows the state, the roasters announced and
their entities; `roast home-assistant --on` switches it on with the server's broker
(`--broker`, `--username`, `--password-stdin` for Home Assistant's own).

<!-- site: end -->

## Design notes

For developers: how the Home Assistant discovery works and why. The site publishes only the
part above.

Each roaster shows up in Home Assistant as a device, over MQTT discovery
(`crates/server/src/notify/home_assistant.rs`). Off until switched on (Settings → Home Assistant, or
`roast home-assistant --on`). **Read only**: no entity sends a command, and Roastwarden subscribes
to nothing for it. Control stays with the dashboard, the CLI and the control gate
([`control.md`](control.md)); a Home Assistant automation cannot raise a heater.

Checked against Home Assistant 2026.10.4 (Docker, its MQTT integration) on both brokers below:
the entities appear, follow a roast, turn unavailable when the server stops and disappear when
discovery is switched off.

## Topics

| Topic | Retained | What |
|---|---|---|
| `<prefix>/device/roastwarden_<id>/config` | yes | the device discovery message (Home Assistant 2024.11 or newer): every entity at once |
| `roastwarden/ha/<id>/state` | no | JSON: `bt`, `et`, `ror`, `heater`, `fan`, `setpoint`, `phase`, `roast_time`, `roasting`, `roast`, `batch`, `ror_warning`, `ror_warning_reason`; every 5 s (`ha_interval_seconds`, 1–60) while the roaster reports |
| `roastwarden/ha/<id>/availability` | yes | `online` while the roaster reported in the last 30 s, else `offline` |
| `roastwarden/server/status` | yes | the server's presence and its last will, on the server's own broker |
| `roastwarden/ha/status` | yes | the same on Home Assistant's own broker (its client's last will) |

Each entity is available when both its roaster's availability and the server's presence say so.
The discovery is sent again when it changes, after a reconnect and every five minutes (a broker
without persistence forgets retained messages). Switched off, a prefix or broker changed, or a
roaster gone: the old discovery is cleared (an empty retained message), and Home Assistant
removes the device.

`<id>` is the device id in lower case with anything but letters, digits and `-` as `_`.

## Which broker

**The server's own** (default; `ha_broker` empty): the broker the roasters use. Home Assistant's
MQTT integration connects to it with a user of its own that may only read:

- The built-in broker (`ROASTWARDEN_BROKER_ADDR`, [`DEPLOY.md`](DEPLOY.md) "One container"): set
  `MQTT_HOME_ASSISTANT_PASSWORD` and Home Assistant signs in as `homeassistant`. Its rules let
  it read `homeassistant/#`, `roastwarden/ha/#` and `roastwarden/server/status`, and send only its
  own birth message (`homeassistant/status`); a roaster's topics are closed to it, so it can
  neither read their raw messages nor send a command. With this user the prefix stays
  `homeassistant`.
- The compose file's Mosquitto: the same variable makes the same user (`docker/mosquitto/users.sh`,
  `config/acl.in`). Restart the stack for the user to be written.
- A broker of your own: give Home Assistant a user that reads those topics.

**Home Assistant's own** (`ha_broker` = `mqtt://host[:port]`, with `ha_username` and
`ha_password`): the server opens a second client to it and publishes there only; the
roasters stay on their broker. Its presence and last will are `roastwarden/ha/status`, never
`roastwarden/server/status`, so pointing it at the roasters' broker by mistake cannot tell a
roaster the server went away. Plain MQTT only (no `mqtts://`): Home Assistant's broker is on
the LAN.

## Setting it up

1. Home Assistant: Settings → Devices & services → MQTT, pointed at the broker above.
2. Roastwarden: Settings → Home Assistant → On, the broker, Save. Or
   `roast home-assistant --on` (the server's broker), or
   `printf %s "$PW" | roast home-assistant --on --broker mqtt://homeassistant.local:1883 --username roastwarden --password-stdin`.
3. The roasters appear under the MQTT integration's devices within a few seconds.
   `roast home-assistant` (or the page) lists them, their entities and any problem
   ("not connected to … yet", "the server is not connected to its MQTT broker").

## Settings

| Key | Value |
|---|---|
| `ha_enabled` | `true` / `false` (off) |
| `ha_broker` | empty (the server's broker) or `mqtt://host[:port]` |
| `ha_username`, `ha_password` | Home Assistant's broker's user; the password is a secret (never read back) |
| `ha_prefix` | the discovery prefix, `homeassistant` |
| `ha_interval_seconds` | 1–60, 5 |
| `ha_virtual` | `true`: the simulated roasters too |

`GET /api/home-assistant` (any token) answers what is published; the settings go through
`PUT /api/settings` (admin).

## Not yet

- Control from Home Assistant (a fan or setpoint entity): deliberately left out. It would pass
  the control gate, but outside the owner's approvals and with no one at the roaster.
- `mqtts://` to Home Assistant's broker.
- The predicted first crack and DROP as entities (they are in `roast live`).
