// A minimal roaster for Roastwarden over Wi-Fi and MQTT (docs/connect-your-roaster.md).
//
// An ESP32, two MAX31855 thermocouple boards (BT and ET), a solid state relay for the heater
// and a PWM fan. It speaks Roastwarden's own topics and field names, so nothing needs mapping:
//
//   publishes  roaster/<id>/telemetry  {"beanTemp":..,"envTemp":..,"heater":..,"heaterApplied":..,"fan":..,"heaterEnable":..}  1 Hz
//              roaster/<id>/status     {"status":"online",...} retained, last will {"status":"offline"}
//   listens    roaster/<id>/control/heater | fan | heater_enable | emergency_stop
//              roastwarden/server/status (the server's presence)
//
// Its own safety (the interface standard's full tier): over-temperature cutoffs, a failed
// probe, no heat below FAN_MIN_PCT of fan, and no heat while the broker or the server is gone.
//
// Libraries: PubSubClient, ArduinoJson 7, Adafruit MAX31855. Board: any ESP32 (Arduino core 3).

#include <WiFi.h>
#include <PubSubClient.h>
#include <ArduinoJson.h>
#include <Adafruit_MAX31855.h>

// ---- Your settings (the same names as the starter's, docs/esp32-starter.md) ----
const char* WIFI_NAME = "your-wifi-name";
const char* WIFI_PASSWORD = "your-wifi-password";
const char* SERVER_ADDRESS = "192.168.1.50";  // Roastwarden's (or the broker it uses)
const int SERVER_PORT = 1883;
const char* ROASTER_ID = "mqtt_roaster";      // the roaster's id in Roastwarden, and its broker user
const char* ROASTER_PASSWORD = "its-password";  // Roasters -> MQTT passwords (or MQTT_DEVICES in .env)

// ---- Wiring ----
const int PIN_SCK = 18, PIN_MISO = 19, PIN_CS_BT = 5, PIN_CS_ET = 17;
const int PIN_HEATER = 26;  // SSR input
const int PIN_FAN = 27;     // PWM to the fan's MOSFET

// ---- Safety limits ----
const float BT_MAX = 250.0, ET_MAX = 300.0;  // °C: the heater trips above either
const float FAN_MIN_PCT = 30.0;              // no heat with less air than this

Adafruit_MAX31855 btProbe(PIN_SCK, PIN_CS_BT, PIN_MISO);
Adafruit_MAX31855 etProbe(PIN_SCK, PIN_CS_ET, PIN_MISO);
WiFiClient net;
PubSubClient mqtt(net);
String prefix, telemetryTopic, statusTopic;

float bt = NAN, et = NAN;
float heater = 0, fan = 0;   // what Roastwarden asked for, %
bool heaterEnable = true;
bool tripped = false;        // an over-temperature trip holds until heater_enable 1, cold
bool serverOnline = false;   // roastwarden/server/status

// The heater % that reaches the relay after every interlock.
float heaterApplied() {
  if (!heaterEnable || tripped) return 0;
  if (isnan(bt) || isnan(et)) return 0;          // a probe failed
  if (fan < FAN_MIN_PCT) return 0;               // no heat without air
  if (!mqtt.connected() || !serverOnline) return 0;  // nobody is watching
  return heater;
}

void stopHeat() {
  heater = 0;
  fan = 100;  // cool what is in the chamber
}

void onMessage(char* topic, byte* payload, unsigned int length) {
  String t(topic), v;
  v.reserve(length);
  for (unsigned int i = 0; i < length; i++) v += (char)payload[i];

  if (t == "roastwarden/server/status") {
    serverOnline = v.indexOf("\"online\"") >= 0;
    return;
  }
  String cmd = t.substring(t.lastIndexOf('/') + 1);
  if (cmd == "heater") {
    heater = constrain(v.toFloat(), 0, 100);
  } else if (cmd == "fan") {
    fan = constrain(v.toFloat(), 0, 100);
  } else if (cmd == "heater_enable") {
    heaterEnable = v == "1";
    if (!heaterEnable) heater = 0;
    if (heaterEnable && bt < BT_MAX - 20 && et < ET_MAX - 20) tripped = false;
  } else if (cmd == "emergency_stop") {
    stopHeat();
  }
  // setpoint, mode and pid are for a roaster with its own PID: this one has none, so
  // Roastwarden runs its own loop and sends heater %.
}

void connectMqtt() {
  while (!mqtt.connected()) {
    if (mqtt.connect(ROASTER_ID, ROASTER_ID, ROASTER_PASSWORD,
                     statusTopic.c_str(), 1, true, "{\"status\":\"offline\"}")) {
      // Retained, so the server knows it at once; "caps": [] says it has no PID of its own.
      mqtt.publish(statusTopic.c_str(),
                   "{\"status\":\"online\",\"version\":\"example-1\",\"caps\":[]}", true);
      mqtt.subscribe((prefix + "/control/#").c_str());
      mqtt.subscribe("roastwarden/server/status");
    } else {
      delay(2000);
    }
  }
}

void publishTelemetry() {
  JsonDocument doc;
  if (!isnan(bt)) doc["beanTemp"] = bt;
  if (!isnan(et)) doc["envTemp"] = et;
  doc["heater"] = heater;
  doc["heaterApplied"] = heaterApplied();
  doc["fan"] = fan;
  doc["heaterEnable"] = heaterEnable ? 1 : 0;
  char out[192];
  size_t n = serializeJson(doc, out, sizeof(out));
  mqtt.publish(telemetryTopic.c_str(), (const uint8_t*)out, n, false);
}

void setup() {
  Serial.begin(115200);
  pinMode(PIN_HEATER, OUTPUT);
  digitalWrite(PIN_HEATER, LOW);
  analogWrite(PIN_FAN, 0);

  prefix = String("roaster/") + ROASTER_ID;
  telemetryTopic = prefix + "/telemetry";
  statusTopic = prefix + "/status";

  WiFi.begin(WIFI_NAME, WIFI_PASSWORD);
  while (WiFi.status() != WL_CONNECTED) delay(500);
  mqtt.setServer(SERVER_ADDRESS, SERVER_PORT);
  mqtt.setCallback(onMessage);
}

void loop() {
  if (!mqtt.connected()) {
    serverOnline = false;
    digitalWrite(PIN_HEATER, LOW);
    connectMqtt();
  }
  mqtt.loop();

  static unsigned long lastRead = 0;
  if (millis() - lastRead >= 1000) {
    lastRead = millis();
    bt = btProbe.readCelsius();  // NAN when the probe is open or shorted
    et = etProbe.readCelsius();
    if (bt > BT_MAX || et > ET_MAX) {
      tripped = true;
      stopHeat();
    }
    publishTelemetry();
  }

  // The heater as a slow PWM on the relay: on for heaterApplied % of each second.
  digitalWrite(PIN_HEATER, (millis() % 1000) < heaterApplied() * 10 ? HIGH : LOW);
  analogWrite(PIN_FAN, (int)(fan * 255 / 100));
}
