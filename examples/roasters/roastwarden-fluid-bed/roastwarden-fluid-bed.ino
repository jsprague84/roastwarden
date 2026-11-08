// Roastwarden fluid-bed controller: an ESP32 that runs an electric fluid-bed roaster.
//
// It reads the bean temperature (BT) and the air temperature (ET), switches the heater through
// a solid state relay (SSR) and sets the fan's speed. Roastwarden drives it: by hand from the
// dashboard, or following a profile. The roaster keeps itself safe whatever happens to the
// network: it cuts the heater when a temperature passes its limit, a probe fails, the fan is
// too slow, or nobody is in control any more.
//
// It does what Roastwarden's own fluid-bed firmware does, in one file: a PID on the bean
// temperature next to the heater (Roastwarden sends the target and the gains), the heater as
// time-proportioned power, four thermocouple reads a second averaged, and a control lease.
//
// The guide (parts, wiring, the first test without heat, tuning):
//   https://roastwarden.com/docs/esp32-fluid-bed/
//
// 1. Fill in the parts marked YOUR SETTINGS below. Settings 1 to 3 are the starter's.
// 2. Upload, then open the serial monitor at 115200 baud: it says what it is doing.
// 3. Test it the first time with the heating element disconnected.
//
// WARNING: a heater on mains power can start a fire or kill. Wire the mains side safely (an
// enclosure, a fuse, earth, wire rated for the current), or have someone who knows how do it.
// The limits below protect the roast; a thermal fuse on the heater protects the house.
//
// Board: "ESP32 Dev Module" (esp32 by Espressif Systems, version 3).
// Libraries: "PubSubClient" by Nick O'Leary, "ArduinoJson" by Benoit Blanchon (version 7), and
// for your thermocouple board "Adafruit MAX31855 library" or "MAX6675 library" by Adafruit.

#include <WiFi.h>
#include <ESPmDNS.h>
#include <ArduinoOTA.h>
#include <PubSubClient.h>
#include <ArduinoJson.h>
#include "esp_task_wdt.h"
#include "esp_timer.h"

// ============================== YOUR SETTINGS 1: Wi-Fi ==============================
// The ESP32 joins 2.4 GHz networks only. Both are case-sensitive.
const char* WIFI_NAME = "your-wifi-name";
const char* WIFI_PASSWORD = "your-wifi-password";

// ============================ YOUR SETTINGS 2: Roastwarden ===========================
// Roastwarden's installer printed these three when it finished. To see them again, run it
// again on the server: sh install.sh
const char* SERVER_ADDRESS = "192.168.1.50";
const char* ROASTER_ID = "my_roaster";
const char* ROASTER_PASSWORD = "paste-the-password-here";
const int SERVER_PORT = 1883;  // change it only if you moved Roastwarden's MQTT port

// ===================== YOUR SETTINGS 3: your thermocouple boards =====================
// Which board you have. Keep one of these two lines; put // in front of the other.
#define BOARD_MAX31855
// #define BOARD_MAX6675

// The ESP32 pins each board is wired to. The boards share the clock and data pins; each has a
// select pin (CS) of its own. A controller wants both probes: without the second one
// (PIN_ET_SELECT -1) there is no air temperature limit.
const int PIN_CLOCK = 18;      // the boards' CLK (also called SCK)
const int PIN_DATA = 19;       // the boards' DO (also called SO or MISO)
const int PIN_BT_SELECT = 5;   // CS of the board with the bean probe
const int PIN_ET_SELECT = 17;  // CS of the board with the air probe, or -1

// Added to each reading, in °C, if a probe reads high or low against a reference.
const float BT_OFFSET = 0.0;
const float ET_OFFSET = 0.0;

// ============================ YOUR SETTINGS 4: the heater ============================
// The pin to the SSR's input (+); the SSR's input (-) goes to GND.
const int PIN_HEATER = 26;
// true: the pin high switches the heater on (most SSRs). false: low switches it on (relay
// boards marked "low level trigger").
const bool HEATER_ON_HIGH = true;
// Heater % is the share of each window the SSR is on. With a zero-crossing AC SSR (the usual
// kind) that is the share of the heater's power. 1000 ms suits most heaters.
const int HEATER_WINDOW_MS = 1000;

// ============================== YOUR SETTINGS 5: the fan =============================
// How the fan's speed is set. Keep one of these two lines; put // in front of the other.
#define FAN_PWM  // a DC fan through a MOSFET board, or a fan speed board with a PWM input
// #define FAN_ESC  // a brushless fan through an ESC (a servo signal, 1000 to 2000 µs)

const int PIN_FAN = 27;
const int FAN_PWM_HZ = 5000;      // FAN_PWM: 5000 suits most MOSFET boards; try 25000 if it whines
const int FAN_ESC_MIN_US = 1000;  // FAN_ESC: the pulse for stopped
const int FAN_ESC_MAX_US = 2000;  // FAN_ESC: the pulse for full speed

// ============================ YOUR SETTINGS 6: safety limits ==========================
// The heater goes off, and the fan to full, when any of these is passed, whatever Roastwarden
// says. Light to dark roasts end between 200 and 230 °C bean temperature.
const float MAX_BT = 240.0;                   // °C
const float MAX_ET = 300.0;                   // °C, air into or out of the beans
const float MIN_FAN_FOR_HEAT = 40.0;          // %: no heat at or below this fan speed
const int SENSOR_FAIL_SECONDS = 3;            // a probe that reads nothing this long
const unsigned long NO_CONTROL_MS = 60000;    // Roastwarden or its broker gone this long

// ============================ YOUR SETTINGS 7: the PID and updates ====================
// The gains the PID starts with. Roastwarden sends its own when the roaster comes online, once
// you have tuned it (Roasters > your roaster > Tuning): these only matter before that. Output
// units 0 to 255, as Roastwarden's own firmware.
const double START_KP = 5.5;
const double START_KI = 0.085;
const double START_KD = 0.0;

// Over-the-air updates from PlatformIO or the Arduino IDE once it is on Wi-Fi. Empty: off (upload
// over USB). Set a password of your own to switch them on; never leave a known one.
const char* UPDATE_PASSWORD = "";

// ==================== Nothing below needs changing to get started ====================

#if defined(BOARD_MAX31855) && defined(BOARD_MAX6675)
#error "Keep only one of BOARD_MAX31855 and BOARD_MAX6675 (put // in front of the other)"
#elif defined(BOARD_MAX31855)
#include <Adafruit_MAX31855.h>
typedef Adafruit_MAX31855 Probe;
#elif defined(BOARD_MAX6675)
#include <max6675.h>
typedef MAX6675 Probe;
#else
#error "Say which thermocouple board you have: #define BOARD_MAX31855 or #define BOARD_MAX6675"
#endif

#if defined(FAN_PWM) && defined(FAN_ESC)
#error "Keep only one of FAN_PWM and FAN_ESC (put // in front of the other)"
#elif !defined(FAN_PWM) && !defined(FAN_ESC)
#error "Say how the fan is driven: #define FAN_PWM or #define FAN_ESC"
#endif

const char* VERSION = "fluid-bed-1";

// ---------------------------------------------------------------------------------------
// The control arithmetic: the same as Roastwarden's own fluid-bed firmware, so the gains its
// autotune finds mean the same here.
namespace ctl {

double clampd(double v, double lo, double hi) { return v < lo ? lo : (v > hi ? hi : v); }

// The PID works in output units 0..255 (the gains keep those units); the wire speaks percent.
double pctTo255(double pct) { return clampd(pct, 0.0, 100.0) * 255.0 / 100.0; }
double out255ToPct(double out) { return clampd(out, 0.0, 255.0) * 100.0 / 255.0; }
double pct1(double pct) { return round(clampd(pct, 0.0, 100.0) * 10.0) / 10.0; }

// A percent payload ("62", "62.5"): the whole text a number from 0 to 100.
bool parsePct(const char* s, double* out) {
  while (*s == ' ') s++;
  if (!*s) return false;
  char* end = nullptr;
  const double v = strtod(s, &end);
  if (end == s) return false;
  while (*end == ' ' || *end == '\r' || *end == '\n') end++;
  if (*end != '\0' || !isfinite(v) || v < 0.0 || v > 100.0) return false;
  *out = v;
  return true;
}

// A thermocouple board now and then reads one sample far off. A step bigger than maxStep is
// held back once; if the next read is far off too, the change is real and passes.
struct SpikeFilter {
  double maxStep = 5.0;
  bool have = false, pending = false;
  double value = 0.0;
  uint32_t rejected = 0;

  double update(double raw) {
    if (isnan(raw)) return raw;
    if (!have || fabs(raw - value) <= maxStep || pending) {
      have = true;
      pending = false;
      value = raw;
      return value;
    }
    pending = true;
    rejected++;
    return value;
  }
};

// The mean of a second's reads: finer than one read's 0.25 °C step, with half the noise.
struct Averager {
  double sum = 0.0;
  uint16_t n = 0;
  void add(double v) {
    if (isnan(v)) return;
    sum += v;
    n++;
  }
  double take() {
    const double m = n ? sum / n : NAN;
    sum = 0.0;
    n = 0;
    return m;
  }
};

// A lease runs out when it is set and its last renewal is older than it. Wrap-safe.
bool leaseExpired(uint32_t nowMs, uint32_t renewedMs, uint32_t leaseMs) {
  return leaseMs > 0 && (int32_t)(nowMs - renewedMs) > (int32_t)leaseMs;
}

// PID on bean temperature with feedforward: P on error, D on measurement, Ki per second, Kd in
// seconds; the integral stops growing while the output is saturated in the error's direction
// or the heater is held off by an interlock. Output = ff + pid, 0..255.
struct Pid {
  double kp = 0, ki = 0, kd = 0;
  double ff = 0, integ = 0, lastInput = NAN;

  void setGains(double p, double i, double d) { kp = p; ki = i; kd = d; }
  void setFeedforward(double ff255) { ff = clampd(ff255, 0.0, 255.0); }

  // Bumpless start: the first output equals the heater's output now.
  void reset(double input, double setpoint, double currentOut) {
    lastInput = input;
    integ = clampd(currentOut - ff - kp * (setpoint - input), -255.0, 255.0);
  }

  double compute(double setpoint, double input, double dt, bool hold) {
    if (isnan(input)) return clampd(ff + integ, 0.0, 255.0);
    const double e = setpoint - input;
    const double dIn = isnan(lastInput) ? 0.0 : input - lastInput;
    lastInput = input;
    const double p = kp * e;
    const double d = dt > 0 ? -kd * dIn / dt : 0.0;
    const double candidate = integ + ki * e * (dt > 0 ? dt : 0.0);
    const double raw = ff + p + candidate + d;
    const bool pushingHigh = raw > 255.0 && e > 0;
    const bool pushingLow = raw < 0.0 && e < 0;
    if (!hold && !pushingHigh && !pushingLow) integ = clampd(candidate, -255.0 - ff, 255.0 - ff);
    return clampd(ff + p + integ + d, 0.0, 255.0);
  }
};

}  // namespace ctl

// ---------------------------------------------------------------------------------------
// Why the heater was last cut, sent as stopCode. The numbers are Roastwarden's: never renumber.
// (Constants, not an enum: the Arduino IDE declares the functions above any type defined here.)
const uint8_t STOP_NONE = 0;
const uint8_t STOP_EMERGENCY = 1;
const uint8_t STOP_BT_OVER = 3;
const uint8_t STOP_ET_OVER = 4;
const uint8_t STOP_BT_PROBE = 5;
const uint8_t STOP_ET_PROBE = 6;
const uint8_t STOP_LEASE = 7;
const uint8_t STOP_NO_CONTROL = 8;
const uint8_t STOP_SERVER_OFFLINE = 9;
const uint8_t STOP_UPDATE = 10;

const char* stopName(uint8_t code) {
  switch (code) {
    case STOP_EMERGENCY: return "emergency stop";
    case STOP_BT_OVER: return "bean temperature over MAX_BT";
    case STOP_ET_OVER: return "air temperature over MAX_ET";
    case STOP_BT_PROBE: return "bean probe reads nothing";
    case STOP_ET_PROBE: return "air probe reads nothing";
    case STOP_LEASE: return "Roastwarden stopped renewing its control lease";
    case STOP_NO_CONTROL: return "no connection to Roastwarden";
    case STOP_SERVER_OFFLINE: return "Roastwarden is offline";
    case STOP_UPDATE: return "update started";
    default: return "none";
  }
}

// ---------------------------------------------------------------------------------------
// The roaster's state. MQTT messages arrive inside mqtt.loop(), on this same task.
Probe* btProbe = nullptr;
Probe* etProbe = nullptr;
ctl::SpikeFilter btFilter, etFilter;
ctl::Averager btMean, etMean;
ctl::Pid pid;

float bt = NAN, et = NAN;
double heaterOut = 0;     // the heater command, 0..255: Roastwarden's (manual) or the PID's (auto)
double appliedOut = 0;    // what reaches the SSR after the interlocks, 0..255
double fanPct = 0;
double setpoint = 0;
bool autoMode = false;
bool heaterEnabled = false;  // off at power-up: Roastwarden switches it on for a roast
bool ffOn = false;
double ffPct = 0;
double kp = START_KP, ki = START_KI, kd = START_KD;
int btFails = 0, etFails = 0;

uint8_t stopCode = STOP_NONE;
uint32_t stopAtMs = 0;

uint32_t leaseMs = 0, leaseRenewedMs = 0;
bool serverSeen = false, serverOnline = false;
unsigned long serverOfflineSince = 0;
unsigned long lastConnectedMs = 0;  // MQTT connected, last seen

WiFiClient network;
PubSubClient mqtt(network);
String controlPrefix, telemetryTopic, statusTopic;

// ---------------------------------------------------------------------------------------
// The heater: a timer switches the SSR every 10 ms, on for heaterOnMs of each window, so the
// heater keeps its power while loop() waits on the network for a moment. If loop() stops
// renewing it for 2 s, the heater goes off; after 10 s the watchdog restarts the ESP32.
volatile uint16_t heaterOnMs = 0;
volatile uint32_t heaterRenewedMs = 0;
esp_timer_handle_t heaterTimer;

void setHeaterPin(bool on) { digitalWrite(PIN_HEATER, on == HEATER_ON_HIGH ? HIGH : LOW); }

void heaterTick(void*) {
  const uint32_t now = millis();
  const bool fresh = (uint32_t)(now - heaterRenewedMs) < 2000;
  setHeaterPin(fresh && heaterOnMs > 0 && (now % HEATER_WINDOW_MS) < heaterOnMs);
}

// No heat without air.
bool interlocked() { return fanPct <= MIN_FAN_FOR_HEAT; }

void writeHeater() {
  appliedOut = (!heaterEnabled || interlocked()) ? 0.0 : ctl::clampd(heaterOut, 0.0, 255.0);
  heaterOnMs = (uint16_t)(appliedOut / 255.0 * HEATER_WINDOW_MS + 0.5);
  heaterRenewedMs = millis();
}

// ---------------------------------------------------------------------------------------
// The fan
void fanBegin() {
#if defined(FAN_PWM)
  ledcAttach(PIN_FAN, FAN_PWM_HZ, 8);
#else
  ledcAttach(PIN_FAN, 50, 16);  // a servo signal: one pulse every 20 ms
#endif
}

void writeFan() {
  static int32_t last = -1;
#if defined(FAN_PWM)
  const int32_t duty = (int32_t)(ctl::clampd(fanPct, 0, 100) * 255.0 / 100.0 + 0.5);
#else
  const double us = FAN_ESC_MIN_US + ctl::clampd(fanPct, 0, 100) / 100.0 * (FAN_ESC_MAX_US - FAN_ESC_MIN_US);
  const int32_t duty = (int32_t)(us / 20000.0 * 65535.0 + 0.5);
#endif
  if (duty != last) {
    ledcWrite(PIN_FAN, duty);
    last = duty;
  }
}

// ---------------------------------------------------------------------------------------
// Safety
void safeStop(uint8_t why) {
  const bool wasHeating = heaterEnabled || heaterOut > 0;
  if (wasHeating || stopCode != why) {
    stopCode = why;
    stopAtMs = millis() ? millis() : 1;
    Serial.printf("SAFETY: heater off, fan to full: %s\n", stopName(why));
  }
  heaterEnabled = false;
  heaterOut = 0;
  ffOn = false;
  ffPct = 0;
  pid.setFeedforward(0);
  leaseMs = 0;
  fanPct = 100;  // cool what is in the chamber
  writeHeater();
  writeFan();
}

// Once a sample: the limits, the probes, the lease and whether anyone is in control.
void checkSafety() {
  if (!isnan(bt) && bt >= MAX_BT) return safeStop(STOP_BT_OVER);
  if (!isnan(et) && et >= MAX_ET) return safeStop(STOP_ET_OVER);
  btFails = isnan(bt) ? btFails + 1 : 0;
  etFails = (etProbe != nullptr && isnan(et)) ? etFails + 1 : 0;
  if (btFails >= SENSOR_FAIL_SECONDS) return safeStop(STOP_BT_PROBE);
  if (etFails >= SENSOR_FAIL_SECONDS) return safeStop(STOP_ET_PROBE);
  if (ctl::leaseExpired(millis(), leaseRenewedMs, leaseMs)) return safeStop(STOP_LEASE);

  const bool heating = heaterEnabled || heaterOut > 0;
  if (!heating) return;
  if (!mqtt.connected() && millis() - lastConnectedMs > NO_CONTROL_MS) return safeStop(STOP_NO_CONTROL);
  if (serverSeen && !serverOnline && millis() - serverOfflineSince > NO_CONTROL_MS) {
    return safeStop(STOP_SERVER_OFFLINE);
  }
}

// ---------------------------------------------------------------------------------------
// Temperatures: four reads a second, each through the spike filter, averaged per second.
float readProbe(Probe* probe) {
  if (probe == nullptr) return NAN;
  float c = probe->readCelsius();  // NAN when the thermocouple is unplugged
  if (c == 0.0) return NAN;        // exactly 0: the board is not wired or not powered
  return c;
}

// True once a second, when a new sample is in bt and et.
bool readTemperatures() {
  static unsigned long lastRead = 0, lastSample = 0;
  const unsigned long now = millis();
  if (now - lastRead >= 250) {  // the MAX6675 needs 220 ms per conversion
    lastRead = now;
    btMean.add(btFilter.update(readProbe(btProbe)));
    etMean.add(etFilter.update(readProbe(etProbe)));
  }
  if (now - lastSample < 1000) return false;
  lastSample = now;
  bt = btMean.take();
  et = etMean.take();
  if (!isnan(bt)) bt += BT_OFFSET;
  if (!isnan(et)) et += ET_OFFSET;
  return true;
}

// ---------------------------------------------------------------------------------------
// The heater command: in auto, the PID's, once a sample with the measured time between them.
void runPid(bool newSample) {
  static bool wasAuto = false;
  static unsigned long lastMs = 0;
  if (!heaterEnabled || !autoMode) {
    wasAuto = false;
    return;
  }
  if (isnan(bt)) {  // no bean temperature, nothing to follow: no heat until it reads again
    heaterOut = 0;
    wasAuto = false;
    return;
  }
  if (!wasAuto) {  // just switched to auto: start from the heater's output now
    pid.reset(bt, setpoint, heaterOut);
    wasAuto = true;
    lastMs = millis();
    return;
  }
  if (!newSample) return;
  const double dt = (millis() - lastMs) / 1000.0;
  lastMs = millis();
  heaterOut = pid.compute(setpoint, bt, dt, interlocked());
}

// ---------------------------------------------------------------------------------------
// Roastwarden's commands: roaster/<id>/control/<command>
void onCommand(const String& command, const char* value) {
  double pct;
  if (command == "heater") {
    if (ctl::parsePct(value, &pct)) heaterOut = ctl::pctTo255(pct);
  } else if (command == "fan") {
    if (ctl::parsePct(value, &pct)) fanPct = pct;
  } else if (command == "setpoint") {
    char* end = nullptr;
    const double v = strtod(value, &end);
    if (end != value && v >= 0 && v <= MAX_BT) setpoint = v;
  } else if (command == "mode") {
    if (!strcmp(value, "auto") || !strcmp(value, "1")) autoMode = true;
    if (!strcmp(value, "manual") || !strcmp(value, "0")) autoMode = false;
  } else if (command == "heater_enable") {
    heaterEnabled = !strcmp(value, "1") || !strcmp(value, "true");
    if (!heaterEnabled) heaterOut = 0;
  } else if (command == "emergency_stop") {
    safeStop(STOP_EMERGENCY);
  } else if (command == "pid") {
    JsonDocument doc;
    if (deserializeJson(doc, value)) return;
    if (doc["kp"].is<double>()) kp = doc["kp"];
    if (doc["ki"].is<double>()) ki = doc["ki"];
    if (doc["kd"].is<double>()) kd = doc["kd"];
    pid.setGains(kp, ki, kd);
    Serial.printf("PID gains: Kp %.3f Ki %.4f Kd %.3f\n", kp, ki, kd);
  } else if (command == "feedforward") {
    if (!strcmp(value, "off") || !strcmp(value, "none") || !*value) {
      ffOn = false;
      ffPct = 0;
    } else if (ctl::parsePct(value, &pct)) {
      ffOn = true;
      ffPct = pct;
    }
    pid.setFeedforward(ctl::pctTo255(ffPct));
  } else if (command == "lease") {
    char* end = nullptr;
    const long ms = strtol(value, &end, 10);
    if (end != value && ms >= 0 && ms <= 600000) {
      leaseRenewedMs = millis();
      leaseMs = (uint32_t)ms;
    }
  } else {
    Serial.printf("Roastwarden sent \"%s\", which this roaster does not know\n", command.c_str());
  }
}

void onMessage(char* topic, byte* payload, unsigned int length) {
  char value[256];
  const unsigned int n = length < sizeof(value) - 1 ? length : sizeof(value) - 1;
  memcpy(value, payload, n);
  value[n] = '\0';
  String t(topic);

  if (t == "roastwarden/server/status") {  // Roastwarden's own presence (retained)
    JsonDocument doc;
    if (deserializeJson(doc, value)) return;
    const bool online = doc["status"] == "online";
    if (!online && (serverOnline || !serverSeen)) serverOfflineSince = millis();
    if (!serverSeen || online != serverOnline) {
      Serial.printf("Roastwarden is %s\n", online ? "online" : "offline");
    }
    serverSeen = true;
    serverOnline = online;
    return;
  }
  if (t.startsWith(controlPrefix)) onCommand(t.substring(controlPrefix.length()), value);
}

// ---------------------------------------------------------------------------------------
// The network
void connectWifi() {
  Serial.printf("Wi-Fi: joining \"%s\" ", WIFI_NAME);
  WiFi.mode(WIFI_STA);
  WiFi.setAutoReconnect(true);
  WiFi.begin(WIFI_NAME, WIFI_PASSWORD);
  const unsigned long started = millis();
  while (WiFi.status() != WL_CONNECTED && millis() - started < 20000) {
    delay(500);
    Serial.print(".");
  }
  if (WiFi.status() == WL_CONNECTED) {
    Serial.printf("\nWi-Fi: connected. This ESP32's address is %s\n", WiFi.localIP().toString().c_str());
  } else {
    Serial.println("\nWi-Fi: not connected after 20 seconds. Check WIFI_NAME and WIFI_PASSWORD");
    Serial.println("  (capitals count), and that the network is 2.4 GHz. It keeps trying.");
  }
}

void publishStatus() {
  JsonDocument doc;
  doc["status"] = "online";
  doc["id"] = ROASTER_ID;
  doc["version"] = VERSION;
  doc["ip"] = WiFi.localIP().toString();
  // What Roastwarden may use (its device contract): an onboard PID with feedforward, a
  // control lease, heater % as time-proportioned power, averaged thermocouple reads.
  JsonArray caps = doc["caps"].to<JsonArray>();
  for (const char* c : {"pid", "ff", "lease", "tpc", "oversample"}) caps.add(c);
  char out[256];
  const size_t n = serializeJson(doc, out, sizeof(out));
  mqtt.publish(statusTopic.c_str(), (const uint8_t*)out, n, true);
}

void connectRoastwarden() {
  Serial.printf("Roastwarden: connecting to %s port %d as \"%s\"... ", SERVER_ADDRESS, SERVER_PORT, ROASTER_ID);
  esp_task_wdt_reset();
  if (mqtt.connect(ROASTER_ID, ROASTER_ID, ROASTER_PASSWORD, statusTopic.c_str(), 1, true,
                   "{\"status\":\"offline\"}")) {
    Serial.println("connected.");
    lastConnectedMs = millis();
    mqtt.subscribe((controlPrefix + "#").c_str());
    mqtt.subscribe("roastwarden/server/status");
    publishStatus();
    return;
  }
  const int why = mqtt.state();
  Serial.printf("failed (code %d).\n", why);
  if (why == MQTT_CONNECT_BAD_CREDENTIALS || why == MQTT_CONNECT_UNAUTHORIZED) {
    Serial.println("  Roastwarden turned down the id or the password: ROASTER_ID and ROASTER_PASSWORD");
    Serial.println("  must match a roaster under Roasters -> MQTT passwords in Roastwarden (or the");
    Serial.println("  MQTT_DEVICES line in its .env file).");
  } else {
    Serial.println("  Nothing answered. Check SERVER_ADDRESS, that Roastwarden runs, and that this");
    Serial.println("  ESP32 and the server are on the same network.");
  }
}

void publishTelemetry() {
  JsonDocument doc;
  if (!isnan(bt)) doc["beanTemp"] = round(bt * 100) / 100.0;
  if (!isnan(et)) doc["envTemp"] = round(et * 100) / 100.0;
  doc["heater"] = ctl::pct1(ctl::out255ToPct(heaterOut));
  doc["heaterApplied"] = ctl::pct1(ctl::out255ToPct(appliedOut));
  doc["fan"] = ctl::pct1(fanPct);
  doc["setpoint"] = round(setpoint * 10) / 10.0;
  doc["controlMode"] = autoMode ? 1 : 0;
  doc["heaterEnable"] = heaterEnabled ? 1 : 0;
  if (ffOn) doc["ff"] = round(ffPct * 10) / 10.0;
  doc["Kp"] = kp;
  doc["Ki"] = ki;
  doc["Kd"] = kd;
  doc["uptimeMs"] = (uint64_t)millis();  // Roastwarden sends its gains again after a restart
  doc["stopCode"] = stopCode;
  doc["stopUptimeMs"] = stopAtMs;
  doc["btSpikes"] = btFilter.rejected;
  doc["etSpikes"] = etFilter.rejected;
  doc["rssi"] = WiFi.RSSI();
  doc["freeHeap"] = ESP.getFreeHeap();
  char out[512];
  const size_t n = serializeJson(doc, out, sizeof(out));
  mqtt.publish(telemetryTopic.c_str(), (const uint8_t*)out, n, false);
}

void printState() {
  Serial.print(isnan(bt) ? String("BT: no reading   ") : String("BT ") + String(bt, 1) + " °C   ");
  if (etProbe == nullptr) {
    Serial.print("(no ET probe)   ");
  } else {
    Serial.print(isnan(et) ? String("ET: no reading   ") : String("ET ") + String(et, 1) + " °C   ");
  }
  Serial.printf("heater %.0f%%%s   fan %.0f%%   %s%s\n", ctl::out255ToPct(appliedOut),
                heaterEnabled ? "" : " (off)", fanPct,
                autoMode ? (String("PID to ") + String(setpoint, 0) + " °C").c_str() : "manual",
                mqtt.connected() ? "" : "   not connected");
}

// Updates over Wi-Fi (UPDATE_PASSWORD), and the roaster announced on the network.
void startNetworkServices() {
  if (strlen(UPDATE_PASSWORD) > 0) {
    ArduinoOTA.setHostname(ROASTER_ID);
    ArduinoOTA.setPassword(UPDATE_PASSWORD);
    ArduinoOTA.onStart([]() { safeStop(STOP_UPDATE); });  // nothing else runs while it uploads
    ArduinoOTA.onProgress([](unsigned int, unsigned int) { esp_task_wdt_reset(); });
    ArduinoOTA.begin();  // starts mDNS too
    Serial.println("Updates over Wi-Fi: on");
  } else {
    MDNS.begin(ROASTER_ID);
  }
  MDNS.addService("roastwarden", "tcp", SERVER_PORT);
  MDNS.addServiceTxt("roastwarden", "tcp", "device_id", ROASTER_ID);
  MDNS.addServiceTxt("roastwarden", "tcp", "proto", "mqtt");
  MDNS.addServiceTxt("roastwarden", "tcp", "version", VERSION);
}

// ---------------------------------------------------------------------------------------
void setup() {
  pinMode(PIN_HEATER, OUTPUT);
  setHeaterPin(false);
  Serial.begin(115200);
  delay(500);
  Serial.println();
  Serial.printf("Roastwarden fluid-bed controller %s\n", VERSION);

  fanBegin();
  writeFan();  // stopped (an ESC arms on this)
  const esp_timer_create_args_t timer = {.callback = heaterTick, .name = "heater"};
  esp_timer_create(&timer, &heaterTimer);
  esp_timer_start_periodic(heaterTimer, 10000);  // every 10 ms

  btProbe = new Probe(PIN_CLOCK, PIN_BT_SELECT, PIN_DATA);
  if (PIN_ET_SELECT >= 0) {
    etProbe = new Probe(PIN_CLOCK, PIN_ET_SELECT, PIN_DATA);
  } else {
    Serial.println("No air probe (PIN_ET_SELECT -1): there is no MAX_ET limit.");
  }
  pid.setGains(kp, ki, kd);

  controlPrefix = String("roaster/") + ROASTER_ID + "/control/";
  telemetryTopic = String("roaster/") + ROASTER_ID + "/telemetry";
  statusTopic = String("roaster/") + ROASTER_ID + "/status";
  network.setConnectionTimeout(2000);  // a broker that does not answer holds loop() 2 s at most
  mqtt.setServer(SERVER_ADDRESS, SERVER_PORT);
  mqtt.setCallback(onMessage);
  mqtt.setBufferSize(1024);
  mqtt.setSocketTimeout(3);
  mqtt.setKeepAlive(15);

  connectWifi();
  startNetworkServices();

  // The watchdog: loop() stuck for 10 s restarts the ESP32, and the heater pin goes low.
  esp_task_wdt_config_t wdt = {.timeout_ms = 10000, .idle_core_mask = 1 << 0, .trigger_panic = true};
  if (esp_task_wdt_reconfigure(&wdt) != ESP_OK) esp_task_wdt_init(&wdt);
  esp_task_wdt_add(NULL);
}

void loop() {
  esp_task_wdt_reset();
  ArduinoOTA.handle();

  static unsigned long lastTry = 0;
  if (mqtt.connected()) {
    lastConnectedMs = millis();
  } else if (WiFi.status() == WL_CONNECTED && (lastTry == 0 || millis() - lastTry > 5000)) {
    lastTry = millis();
    connectRoastwarden();
  }
  mqtt.loop();

  const bool newSample = readTemperatures();
  if (newSample) checkSafety();
  runPid(newSample);
  writeHeater();
  writeFan();

  if (newSample) {
    if (mqtt.connected()) publishTelemetry();
    static int count = 0;
    if (count++ % 5 == 0) printState();  // every 5 seconds, to keep it readable
  }
}
