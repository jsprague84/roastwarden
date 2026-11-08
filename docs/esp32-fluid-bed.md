# The ESP32 fluid-bed controller

This guide is for an electric fluid-bed roaster you build or convert yourself: a heating element
and a fan blowing hot air through the beans, such as a popcorn popper, a heat gun over a glass
chamber, or a home-made bed. An ESP32 reads the bean temperature (BT) and the air temperature
(ET), switches the heater through a solid state relay (SSR), and sets the fan's speed.
Roastwarden runs the roast: by hand from the dashboard, or following a profile.

The program, the **fluid-bed controller**, does what the controller on Roastwarden's own test
roaster does:

- a PID on the bean temperature, next to the heater, so a profile is followed smoothly even
  over a busy Wi-Fi network;
- the heater as a share of its power, adjusted every second;
- four thermocouple readings a second averaged, and a single stray reading ignored;
- its own safety, whatever happens to the network. It cuts the heater and runs the fan at full
  when a temperature passes its limit, a probe stops reading, the fan is too slow, or
  Roastwarden goes quiet.

New to the ESP32? Start with [your first roaster connection](esp32-starter.md), the starter.
It covers the parts, the thermocouple wiring and uploading, and this controller adds a heater
and a fan to the same wiring.

> **Mains electricity can start a fire or kill.** The heater side of this project works at
> mains voltage. Put it in an enclosure, with a fuse, earth and wire rated for the heater's
> current, or have someone who knows how do that part. A thermal fuse on the heater protects
> the house if everything else fails. Never leave a roast unattended.

## What you need

The starter's parts (an ESP32, two thermocouple boards and probes), and:

| Part | What to look for |
|---|---|
| A solid state relay for the heater | A **zero-crossing AC SSR** with a DC input (3–32 V), rated at least twice the heater's current (a 1500 W heater at 120 V draws 12.5 A: a 25 A or 40 A SSR), on a heat sink |
| A way to set the fan's speed | A DC fan: a **MOSFET board** with a PWM input. A brushless fan: its **ESC** (speed controller), driven like a model's servo |
| A power supply for the fan | What the fan needs (12 V, 24 V…), sharing its ground with the ESP32 |

Two probes are strongly recommended: the second (ET, in the air just below or above the beans)
gives the controller an air temperature limit, which protects the heater.

## 1. Wire it

The thermocouple boards are wired as in the [starter guide](esp32-starter.md#1-wire-it). Then:

| From | To |
|---|---|
| ESP32 **26** | the SSR's input **+** (often marked 3 or DC+) |
| ESP32 **GND** | the SSR's input **−** (4 or DC−) |
| ESP32 **27** | the fan board's PWM or signal input, or the ESC's signal wire (white or yellow) |
| ESP32 **GND** | the fan board's or the ESC's ground |
| The fan's power supply | the fan board or the ESC, as their instructions say |

The SSR's other side (the load) switches one of the heater's mains wires. With an AC fan
(most popcorn poppers), the fan and the heater are often wired together: separate them, and run
the fan from its own supply, so the controller can always keep air moving.

Leave the heating element **disconnected** for now.

## 2. Get the program

The easiest way: in Roastwarden, **Roasters → ESP32 firmware**. Choose **Fluid-bed controller**,
check the roaster's id, the server's address and your Wi-Fi, and **Download** (a new id needs a
password first: **Make its password**). The zip has the program with those settings and the
roaster's password filled in, ready for VS Code with PlatformIO or for the Arduino IDE:
its README.txt has the steps for both.

Or download the sketch by itself,
[roastwarden-fluid-bed.ino](https://roastwarden.com/examples/roastwarden-fluid-bed.ino), and
fill in **YOUR SETTINGS 1 to 3** as in the [starter guide](esp32-starter.md#3-open-the-sketch-and-fill-in-your-settings).
It needs one more library than the starter: **ArduinoJson** by Benoit Blanchon, version 7.

## 3. Your settings

The rest of the settings are at the top of the sketch, under **YOUR SETTINGS 4 to 7**:

| Setting | What it is |
|---|---|
| `PIN_HEATER`, `HEATER_ON_HIGH` | The SSR's pin; `false` for a relay board that switches on when its input is low |
| `HEATER_WINDOW_MS` | The heater runs this share of each window: 30 % is on for 300 ms of each second. 1000 suits most heaters |
| `FAN_PWM` or `FAN_ESC` | Keep one: a DC fan through a MOSFET board, or a brushless fan's ESC |
| `PIN_FAN`, `FAN_PWM_HZ` | The fan's pin; 5000 Hz suits most MOSFET boards (try 25000 if the fan whines) |
| `FAN_ESC_MIN_US`, `FAN_ESC_MAX_US` | An ESC's pulse for stopped and for full speed (1000 and 2000 µs for most) |
| `MAX_BT`, `MAX_ET` | Over either, the heater goes off and the fan runs at full (240 °C and 300 °C) |
| `MIN_FAN_FOR_HEAT` | No heat at or below this fan speed (40 %): too little air burns beans and the element |
| `SENSOR_FAIL_SECONDS` | A probe that reads nothing this long cuts the heater (3 s) |
| `NO_CONTROL_MS` | Roastwarden gone this long while heating cuts the heater (60 s); a profile in progress gets 10 s |
| `START_KP`, `START_KI`, `START_KD` | The PID's starting gains, until Roastwarden sends tuned ones |
| `BT_OFFSET`, `ET_OFFSET` | A correction in °C if a probe reads high or low against a reference |
| `UPDATE_PASSWORD` | A password of your own switches on updates over Wi-Fi (empty: USB only) |

Set `MAX_BT` and `MAX_ET` for your roaster: a little above the hottest roast you plan, and
below what the chamber and the element can take.

## 4. The first test, without heat

With the heating element still disconnected, upload the program and open the serial monitor
at 115200 baud:

```
Roastwarden fluid-bed controller fluid-bed-1
Wi-Fi: connected. This ESP32's address is 192.168.1.71
Roastwarden: connecting to 192.168.1.50 port 1883 as "my_roaster"... connected.
Roastwarden is online
BT 21.5 °C   ET 22.0 °C   heater 0% (off)   fan 0%   manual
```

In Roastwarden, **Roasters**: the roaster is under **Found, not added yet**. **Set up**: kind
**Fluid bed, electric**, the connection as found, **Save and check** (every line should read
ok), and in the last step **Control: Manual**. Then, on the Roast page's **Control** tab:

1. **Fan** to 60 %: the fan runs.
2. Switch the **heater** on and set it to 30 %: the SSR's light blinks, on for about a third
   of each second, and the serial monitor shows `heater 30%`.
3. **Fan** to 30 %: the heater shows 0 % (too little air). Back to 60 %.
4. **STOP**: the heater goes off and the fan runs at full.
5. Unplug the bean probe from its board: within 3 seconds the serial monitor says
   `SAFETY: heater off, fan to full: bean probe reads nothing`. Plug it back in.

Finally, run Roastwarden's own check with the roaster cold: **Roasters → the roaster → Check →
Run the write check**. It sends heater 10 %, waits for the roaster to report it, then sends STOP
and waits for the heater to read 0.

If any step does not do what it says, find out why before the heater is connected.

## 5. Heat, then tuning

Connect the heating element. A first heat-up without beans: fan at 70 %, heater on at 30 %,
and watch BT and ET rise on the Roast page; STOP at 150 °C.

The PID needs gains that suit your roaster. Raise the roaster's control to **Full** (Roasters →
the roaster → Control → Setup), load beans as for a roast (old or cheap beans do), and run
**Autotune** (Roasters → the roaster → Autotune). It heats up, swings the heater around a
temperature, works out how your roaster responds, and gives gains to apply. Roastwarden keeps
them and sends them to the controller each time it starts.

Then roast: by hand, or with a profile and **Profile drive**, which sends the controller the
temperature to follow, second by second ([the user guide](USER-GUIDE.md)).

## Making it yours

- **A heat gun or a hot-air element with its own fan**: the same wiring, as long as the
  controller can set the fan's speed and the heater separately.
- **Other pins**: change the `PIN_` numbers (the [starter guide](esp32-starter.md#making-it-yours)
  lists the safe ones).
- **A relay board instead of an SSR**: a mechanical relay wears out when it switches every
  second. Use an SSR for the heater.
- **A drum roaster**: the same program runs any electric roaster with one heater and a fan
  (Roastwarden's drum kind adds a drum and a damper, which this program does not drive).
- **Updates over Wi-Fi**: set `UPDATE_PASSWORD`, upload once over USB, then with PlatformIO
  `UPDATE_PASSWORD=… pio run -e esp32-wifi -t upload`, or in the Arduino IDE pick the roaster
  under Tools → Port (network). The heater goes off while an update runs.

What Roastwarden expects of a controlled roaster, field by field, is in
[connect your roaster](connect-your-roaster.md#what-roastwarden-wants).

<!-- site: end -->

## Design notes

For developers: how the controller and its download are built. The site publishes only the
part above.

- The sketch is `examples/roasters/roastwarden-fluid-bed/roastwarden-fluid-bed.ino`, one file so
  the site can serve it at `/examples/roastwarden-fluid-bed.ino` and the Arduino IDE opens it
  alone. Settings 1–3 carry the starter's names, so the installer's printed lines and the
  firmware download fill both the same way.
- Modelled on the fluid-bed firmware 3.7.0 (the `Esp32_FluidBedCoffeeRoaster` repo), and kept
  to its device contract (`control.md`, "Device contract"): `caps` `pid ff lease tpc
  oversample`; the PID in output units 0–255 with the firmware's gains semantics (P on error,
  D on measurement, conditional integration, bumpless start), so Autotune 2's gains (× 2.55)
  and the gain schedule apply unchanged; `uptimeMs` in telemetry so the server resends gains
  after a reboot; the stop codes' numbers; heater enable off at power-up. Left out against
  3.7.0: the serial (Artisan) protocol, NTP, the debug topic, the RoR in telemetry.
- PubSubClient rather than that firmware's ESP32MQTTClient (an ESP-IDF task): one library the
  Arduino IDE's library manager has, its callbacks on `loop()`'s task (no atomics). Its connect
  blocks, so the heater's SSR is switched by an `esp_timer` every 10 ms from the duty `loop()`
  last wrote; a duty older than 2 s switches it off (a long connect, a stuck loop), and the
  10 s task watchdog restarts the chip. The connect is held to 2 s (`setConnectionTimeout`) and
  3 s for CONNACK. The fluid-bed firmware switches the pin from `loop()` instead (a 1 Hz LEDC
  trips the interrupt watchdog).
- Heat continues while the broker is gone, up to `NO_CONTROL_MS` (a broker blip does not spoil
  a roast); a profile in progress holds a 10 s lease. Unlike 3.7.0, a lost broker with the
  heater already off does not run the fan at full.
- The stop codes are constants, not an enum: the Arduino builder puts function prototypes
  above any type defined after the first function.
- The download (`firmware.rs`, `routes/firmware.rs`; `GET`/`POST /api/firmware`, admin) builds
  the sketch into a zip: `<sketch>-<id>/platformio.ini` (`src_dir` the sketch's folder,
  pioarduino's platform for Arduino core 3, the board `esp32dev`, the libraries; for this one an
  `esp32-wifi` env for updates) and `README.txt`. Settings are replaced line by line and C
  escaped; input is checked first. The roaster's password comes from the request or the
  server's (`MQTT_DEVICES` or `mqtt_passwords.rs`; **Make its password** adds one first);
  nothing typed is stored, and the audit log masks both passwords.
- Checked: `examples/build.sh` compiles it (MAX31855 with a PWM fan, MAX6675 with an
  ESC); `connect_examples.rs`'s `the_fluid_bed_example` adds its `roaster.json` and checks a
  twin speaking its messages ok, with manual control's commands; a downloaded zip builds with
  `pio run` as it is. Not yet run on a physical ESP32 and roaster.
