// A minimal roaster for Roastwarden over a WebSocket of its own (docs/connect-your-roaster.md).
//
// An ESP32 on Wi-Fi serving a WebSocket on port 81: two MAX31855 thermocouple boards (BT and
// ET), a solid state relay for the heater and a PWM fan. Roastwarden dials ws://<its address>:81/
// and reads its own JSON through a mapping (roaster.json); commands come back as its own JSON
// through the connection's command templates:
//
//   device -> server   {"BT":165.2,"ET":180.5,"heater":45,"fan":60}   once a second
//   server -> device   {"set":"heater","value":45}   {"set":"fan","value":60}
//                      {"set":"enable","value":1}    {"set":"stop"}
//
// Its own safety: over-temperature cutoffs, a failed probe, no heat below FAN_MIN_PCT of fan,
// and no heat while no client is connected.
//
// Libraries: WebSockets (Markus Sattler), ArduinoJson 7, Adafruit MAX31855. Board: any ESP32.

#include <WiFi.h>
#include <WebSocketsServer.h>
#include <ArduinoJson.h>
#include <Adafruit_MAX31855.h>

// ---- Your settings ----
const char* WIFI_SSID = "your-wifi";
const char* WIFI_PASSWORD = "your-password";

// ---- Wiring ----
const int PIN_SCK = 18, PIN_MISO = 19, PIN_CS_BT = 5, PIN_CS_ET = 17;
const int PIN_HEATER = 26;  // SSR input
const int PIN_FAN = 27;     // PWM to the fan's MOSFET

// ---- Safety limits ----
const float BT_MAX = 250.0, ET_MAX = 300.0;  // °C: the heater trips above either
const float FAN_MIN_PCT = 30.0;              // no heat with less air than this

Adafruit_MAX31855 btProbe(PIN_SCK, PIN_CS_BT, PIN_MISO);
Adafruit_MAX31855 etProbe(PIN_SCK, PIN_CS_ET, PIN_MISO);
WebSocketsServer ws(81);

float bt = NAN, et = NAN;
float heater = 0, fan = 0;  // what Roastwarden asked for, %
bool heaterEnable = true;
bool tripped = false;       // an over-temperature trip holds until enable 1, cold

float heaterApplied() {
  if (!heaterEnable || tripped) return 0;
  if (isnan(bt) || isnan(et)) return 0;      // a probe failed
  if (fan < FAN_MIN_PCT) return 0;           // no heat without air
  if (ws.connectedClients() == 0) return 0;  // nobody is watching
  return heater;
}

void onCommand(const JsonDocument& doc) {
  String set = doc["set"] | "";
  float value = doc["value"] | 0.0f;
  if (set == "heater") {
    heater = constrain(value, 0, 100);
  } else if (set == "fan") {
    fan = constrain(value, 0, 100);
  } else if (set == "enable") {
    heaterEnable = value != 0;
    if (!heaterEnable) heater = 0;
    if (heaterEnable && bt < BT_MAX - 20 && et < ET_MAX - 20) tripped = false;
  } else if (set == "stop") {
    heater = 0;
    fan = 100;
  }
}

void onEvent(uint8_t client, WStype_t type, uint8_t* payload, size_t length) {
  if (type != WStype_TEXT) return;
  JsonDocument doc;
  if (deserializeJson(doc, payload, length) == DeserializationError::Ok) onCommand(doc);
}

void sendReading() {
  JsonDocument doc;
  if (!isnan(bt)) doc["BT"] = bt;
  if (!isnan(et)) doc["ET"] = et;
  doc["heater"] = heater;
  doc["fan"] = fan;
  String out;
  serializeJson(doc, out);
  ws.broadcastTXT(out);
}

void setup() {
  Serial.begin(115200);
  pinMode(PIN_HEATER, OUTPUT);
  digitalWrite(PIN_HEATER, LOW);
  analogWrite(PIN_FAN, 0);

  WiFi.begin(WIFI_SSID, WIFI_PASSWORD);
  while (WiFi.status() != WL_CONNECTED) delay(500);
  Serial.printf("WebSocket on ws://%s:81/\n", WiFi.localIP().toString().c_str());
  ws.begin();
  ws.onEvent(onEvent);
}

void loop() {
  ws.loop();

  static unsigned long lastRead = 0;
  if (millis() - lastRead >= 1000) {
    lastRead = millis();
    bt = btProbe.readCelsius();  // NAN when the probe is open or shorted
    et = etProbe.readCelsius();
    if (bt > BT_MAX || et > ET_MAX) {
      tripped = true;
      heater = 0;
      fan = 100;
    }
    sendReading();
  }
  digitalWrite(PIN_HEATER, (millis() % 1000) < heaterApplied() * 10 ? HIGH : LOW);
  analogWrite(PIN_FAN, (int)(fan * 255 / 100));
}
