# Touch panel

A small touch screen beside the roaster, for the roasts where you do not want a browser open.
It shows the bean temperature and the roast clock, and the buttons for the roast in front of
you: Start, Charge, First crack, Drop, Stop. The server still runs the roast, records it and
follows the profile; the panel is one more screen for it, like a phone.

The example is an ESP32 "Cheap Yellow Display", one of the cheapest boards with a touch
screen. Any small screen that can make two web requests works the same way.

## A roast from the panel

| Button | What it does |
|---|---|
| **◀ ▶** | Pick what to roast: your [planned roasts](batches.md) first, then the coffees this roaster roasted last, to roast again with the same profile and weight. |
| **START** | Starts recording the picked roast. With nothing planned and nothing to repeat, it starts a plain roast. |
| **CHARGE** | Marks the charge. On a roaster with a [roast sequence](roast-sequence.md) the fan and heater come on and [Profile drive](replay.md) starts. |
| **DRY**, **CRACK** | Mark dry end and first crack. One of each: a second tap marks nothing. |
| **DROP** | Marks the drop: the drive stops, the heater goes off and the roaster cools. |
| **DONE** | Completes the roast. Weigh it and add notes later, in the dashboard or by asking your agent. |
| **CANCEL** | Discards the roast. It asks "Sure?" first. |
| **STOP** | Heater off and fan to full, at once. |
| **− +** | Fan, in steps of 5 %; hold **−** for off or **+** for full. A tap on the label switches them to the heater when no profile drives it (hold **−** for heater off; the heater never jumps to full). |

The screen shows only the buttons that do something now. A refusal (the roaster's control is
off, say) appears on the status line in words.

## What you need

- An **ESP32-2432S028R** (the "Cheap Yellow Display": a 2.8" touch screen with the ESP32 on
  its back) and a USB power supply. Its touch works with gloves.
- Wi-Fi where the roaster stands, and the server reachable from there.
- A roaster already set up under Roasters. Its [control level](roaster-setup.md) decides what
  the buttons may send: with control off the panel still shows the roast and marks its
  events.

## Setting it up

1. **Make the panel's token.** Settings → Tokens → **Panel**, a name ("Roast room panel"),
   the roaster, and an expiry (never, for a panel that stays). The page shows the token once,
   as three lines for the sketch:

   ```cpp
   #define RW_SERVER  "http://192.168.1.50:8080"
   #define RW_ROASTER "my_roaster"
   #define RW_TOKEN   "rr_…"
   ```

   `RW_SERVER` is the server's address as the panel reaches it: plain `http://` on your own
   network, or `https://` behind a proxy.

2. **Get the sketch**:
   [roastwarden-panel.ino](https://roastwarden.com/examples/roastwarden-panel.ino). Paste the
   three lines over its own, and put in your Wi-Fi's name and password.

3. **Tell the screen library about the board.** The sketch draws with TFT_eSPI, which takes
   the board's pins as build settings. In PlatformIO:

   ```ini
   [env:panel]
   platform = espressif32
   board = esp32dev
   framework = arduino
   lib_deps =
       bblanchon/ArduinoJson@^7
       bodmer/TFT_eSPI@^2.5.43
       https://github.com/PaulStoffregen/XPT2046_Touchscreen.git
   build_flags =
       -DUSER_SETUP_LOADED=1 -DILI9341_2_DRIVER=1 -DTFT_WIDTH=240 -DTFT_HEIGHT=320
       -DTFT_MISO=12 -DTFT_MOSI=13 -DTFT_SCLK=14 -DTFT_CS=15 -DTFT_DC=2 -DTFT_RST=-1
       -DTFT_BL=21 -DTFT_BACKLIGHT_ON=HIGH -DUSE_HSPI_PORT=1
       -DLOAD_GLCD=1 -DLOAD_FONT2=1 -DLOAD_FONT4=1 -DLOAD_FONT6=1 -DLOAD_FONT7=1
       -DSPI_FREQUENCY=55000000 -DSPI_READ_FREQUENCY=20000000
   ```

   In the Arduino IDE, install the three libraries and put the same values into TFT_eSPI's
   `User_Setup.h`. A board sold with two USB sockets often has an ST7789 screen: use
   `ST7789_DRIVER` in place of `ILI9341_2_DRIVER`.

4. **Flash it.** The header shows the roaster's name and "online" within a few seconds.

If taps land beside the buttons, switch `CALIBRATE` on in the sketch: the status line then
shows what each tap reads. Tap the four corners and put those numbers in the `TOUCH_…`
settings.

When something does not work, the serial monitor (115200) says what the panel is doing: its
Wi-Fi and address, each answer of the server that is not a plain yes, and each tap with where
on the screen it landed. It never prints the token.

Try it on the [simulated roaster](virtual-roaster.md) first: make a panel token for
`virtual_roaster` and roast there.

## What a panel may do

A panel's token is made for one roaster and is held to it:

- It works **that roaster's panel and nothing else**. It reads no roasts, coffees, costs or
  settings, and another roaster's panel refuses it.
- It needs no "Tokens may control this roaster" switch: making the token for the roaster is
  your yes. The roaster's control level still decides what is sent.
- It **never asks first**. The person pressing Start stands at the roaster.
- Every press is in Settings → Audit under the panel's name. **Revoke** the token and the
  panel stops at once.

## Safety

**STOP on the panel is a convenience, not a safety device.** It is a button on Wi-Fi: with
the network down it does nothing. The roaster's own firmware has to keep it safe without the
server and without the panel ([control](control.md), "Emergency stop and safety"). A real
emergency stop is a switch wired to the heater.

When the panel loses the server its numbers turn grey and the header says "no server" within
five seconds, so a stale temperature is never mistaken for a live one.

## A panel of your own

Any screen that speaks HTTP can be a panel. It makes two calls, both with the panel token as
`Authorization: Bearer rr_…`:

- `GET /api/roaster/<roaster>/panel` answers what to show: `bt`, `et`, `ror`, `heater`, `fan`
  and `setpoint` (°C and %), `online`, the roast's `stage` (`idle`, `ready`, `roasting`,
  `dropped`) and `phase`, the open `roast` with its clock `time_s`, the `actions` that do
  something now, and the `queue` Start can start. Ask once a second.
- `POST /api/roaster/<roaster>/panel/<button>` presses `start`, `charge`, `dry_end`,
  `first_crack`, `drop`, `complete`, `cancel`, `stop`, `fan` or `heater`. `start` takes
  `{"plan": "<id>"}` or `{"again": "<id>"}` from the queue (neither: the first), `fan` and
  `heater` take `{"percent": 85}`. The answer is the new screen, so the panel need not ask
  again.

From the command line, `roast roaster panel <roaster>` prints what a panel at that roaster
shows now. It only reads: the buttons are for the person at the roaster.

<!-- site: end -->

## Design notes

For developers: how the panel works and why. The site publishes only the part above.

Server: `crates/server/src/routes/panel.rs` (the two routes), the "panel" preset in
`crates/server/src/auth/tokens.rs`, a token's roaster in migration 062. Sketch:
`examples/panels/roastwarden-panel/`, compiled by `examples/build.sh roastwarden-panel`.
Dashboard: Settings → Tokens.

## Why a client of the server, not a screen on the roaster

The server already runs the roast: the sequence at CHARGE and DROP, Profile drive, auto DROP,
cooling, the record. A panel that sends "charge" gets all of it. A screen wired to the
roaster's own controller would share the processor that times the heater and would still need
the server for the profile and the record; and the roaster cuts its heater when the server
goes away, so it would not roast alone either.

## Why its own two routes

The Roast page makes a roast from several calls and works out the stage, the clock and which
buttons apply in the browser. A microcontroller doing the same would fetch five endpoints a
second and parse kilobytes of JSON to draw four numbers. So the server answers one small
object and decides the `actions` itself; the panel draws what it is told and sends a button's
name. A second kind of panel (another board, a page on an old phone) needs no logic of its own.

Each button goes through the handler the Roast page's button uses (`roast_events::mark`,
`sessions::complete`, `control::set_fan`, …), so the roast sequence, a drive's manual
override, fan assist's ceiling and the control gate behave the same from either screen. What
the Roast page does beside its calls is done here too: heater power off with DROP and
Complete, heater power off and the heater at 0 % with STOP, and the simulated roasters' beans
loaded at CHARGE and dumped at DROP.

The marks take the server's clock (seconds since the roast started) and the newest BT. A
second DRY or CRACK is refused (`ALREADY_MARKED`), as CHARGE and DROP are everywhere: a
resistive screen double-taps.

## The queue

Start offers the roaster's planned roasts in their order, then its last finished roasts by
coffee and profile, newest first, each pair once (`kind` `plan` or `again`), six in all. A
roast repeated takes the finished roast's bean, profile and green weight; a planned one is
started as from the Roast page, and cancelling it puts the plan back. With an empty queue
Start records a plain roast named by the date: on a roaster with a roast sequence CHARGE
still switches fan and heater power on, and the heater is the person's.

## The panel preset

The four other presets share every read. A panel's token sits in the flash of a device on a
wall, so "panel" shares none of them: it may `GET /healthz`, `/readyz`, `/version`,
`/api/auth/status` and `/api/roaster/*/panel`, and `POST /api/roaster/*/panel/*`.

- **One roaster.** The token's `roaster` (a column on the token) is set when it is made and
  never changes. A request under another roaster's path is refused, `TOKEN_SCOPE` (403). The
  path is compared as it was sent, so a roaster whose id a URL would write differently (a
  `%`, a space) cannot have a panel: its encoded form could name another roaster.
- **No switch.** The real-hardware rule keeps every other token off a physical roaster until
  the owner switches "Tokens may control this roaster" on, for all tokens at once. A panel
  token under its own roaster's path passes without it: only the owner key makes tokens, and
  naming the roaster there is the same yes, given to one token.
- **The buttons are the panel preset's alone**, beside the owner. Start on a panel asks no
  one, so the route is not in "roast" or "admin": a token that asks first would get round its
  approval by pressing Start here. What a panel shows is in every preset's reads.
- **Never asks first.** `require_approval` is false on a panel token and cannot be set.
- The control gate is below all of it: a roaster at control off refuses a command from a
  panel as from anyone (`CONTROL_OFF`).

## Not yet

- A page in the dashboard with the same buttons, for an old phone kept at the roaster.
- The sketch with the roaster, the token and Wi-Fi filled in, as the roaster firmware
  download does.
- The profile's target beside BT, and the auto DROP countdown.
