// A minimal roaster for Roastwarden over USB serial, in the TC4 / aArtisan protocol
// (docs/connect-your-roaster.md).
//
// Any Arduino-compatible board on a USB port of the server (an ESP32 here; an Uno or a Nano
// with other pins), two MAX31855 thermocouple boards, a solid state relay for the heater
// and a PWM fan. It answers as stock aArtisan does, so Roastwarden's "stock aArtisan" preset
// reads it and Artisan's own TC4 device does too:
//
//   master -> device   UNITS;C   CHAN;1200   READ   OT1;<heater %>   IO3;<fan %>
//                      OT2;<n>   PID;ON | PID;OFF  (no PID here: acknowledged, ignored)
//   device -> master   READ: "ambient,ET,BT,heater,fan"   e.g. 22.1,180.5,165.2,45,60
//                      anything else: a "# ..." line, which the master ignores
//
// Its own safety: over-temperature cutoffs, a failed probe, no heat below FAN_MIN_PCT of fan,
// and no heat when no READ came for WATCHDOG_MS.
//
// Libraries: Adafruit MAX31855.

#include <Adafruit_MAX31855.h>

// ---- Wiring ----
const int PIN_SCK = 18, PIN_MISO = 19, PIN_CS_BT = 5, PIN_CS_ET = 17;
const int PIN_HEATER = 26;  // SSR input
const int PIN_FAN = 27;     // PWM to the fan's MOSFET

// ---- Safety limits ----
const float BT_MAX = 250.0, ET_MAX = 300.0;  // °C: the heater trips above either
const float FAN_MIN_PCT = 30.0;              // no heat with less air than this
const unsigned long WATCHDOG_MS = 5000;      // no READ for this long: no heat

Adafruit_MAX31855 btProbe(PIN_SCK, PIN_CS_BT, PIN_MISO);
Adafruit_MAX31855 etProbe(PIN_SCK, PIN_CS_ET, PIN_MISO);

float bt = NAN, et = NAN, ambient = NAN;
int heater = 0, fan = 0;  // OT1 and IO3, %
bool tripped = false;     // an over-temperature trip holds until the next OT1, cold
unsigned long lastRead = 0;

int heaterApplied() {
  if (tripped) return 0;
  if (isnan(bt) || isnan(et)) return 0;           // a probe failed
  if (fan < FAN_MIN_PCT) return 0;                // no heat without air
  if (millis() - lastRead > WATCHDOG_MS) return 0;   // nobody is watching
  return heater;
}

// aArtisan prints -1 for a channel with no reading.
void printTemp(float c) {
  if (isnan(c)) Serial.print(-1);
  else Serial.print(c, 2);
}

void handleLine(String line) {
  line.trim();
  line.toUpperCase();
  if (line == "READ") {
    lastRead = millis();
    printTemp(ambient); Serial.print(',');
    printTemp(et); Serial.print(',');  // logical channel 1
    printTemp(bt); Serial.print(',');  // logical channel 2
    Serial.print(heater); Serial.print(',');
    Serial.println(fan);
  } else if (line.startsWith("OT1;")) {
    heater = constrain(line.substring(4).toInt(), 0, 100);
    if (bt < BT_MAX - 20 && et < ET_MAX - 20) tripped = false;
    Serial.println("# OT1");
  } else if (line.startsWith("IO3;")) {
    fan = constrain(line.substring(4).toInt(), 0, 100);
    Serial.println("# IO3");
  } else if (line.startsWith("CHAN;")) {
    // Two probes, always ET on 1 and BT on 2: keep the connection's channels at ET 1, BT 2.
    Serial.println("# Active channels set to 1200");
  } else if (line.startsWith("UNITS;")) {
    Serial.println("# Units C");  // this board reads °C only
  } else {
    Serial.println("# ignored");  // OT2, PID;...: no second output, no PID
  }
}

void setup() {
  Serial.begin(115200);
  pinMode(PIN_HEATER, OUTPUT);
  digitalWrite(PIN_HEATER, LOW);
  analogWrite(PIN_FAN, 0);
}

void loop() {
  static String buf;
  while (Serial.available()) {
    char c = Serial.read();
    if (c == '\n') { handleLine(buf); buf = ""; }
    else if (c != '\r' && buf.length() < 64) buf += c;
  }

  static unsigned long lastProbe = 0;
  if (millis() - lastProbe >= 250) {
    lastProbe = millis();
    bt = btProbe.readCelsius();  // NAN when the probe is open or shorted
    et = etProbe.readCelsius();
    ambient = btProbe.readInternal();
    if (bt > BT_MAX || et > ET_MAX) {
      tripped = true;
      heater = 0;
      fan = 100;
    }
  }
  digitalWrite(PIN_HEATER, (millis() % 1000) < heaterApplied() * 10UL ? HIGH : LOW);
  analogWrite(PIN_FAN, fan * 255 / 100);
}
