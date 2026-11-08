// Roastwarden starter: an ESP32 that sends your roaster's temperatures to Roastwarden.
//
// Once a second it reads the bean temperature (BT) and, with a second probe, the environment
// temperature (ET), and sends them over Wi-Fi. It controls nothing: you roast with the
// roaster's own controls while Roastwarden draws the curve and the rate of rise, marks the
// phases and keeps every roast.
//
// The guide, step by step (parts, wiring, Arduino IDE, the first roast):
//   https://roastwarden.com/docs/esp32-starter/
//
// 1. Fill in the parts marked YOUR SETTINGS below.
// 2. Upload, then open Tools > Serial Monitor at 115200 baud. It says what it is doing and,
//    when something is wrong, what to check.
//
// Board: "ESP32 Dev Module" (esp32 by Espressif Systems, version 3).
// Libraries (Tools > Manage Libraries): "PubSubClient" by Nick O'Leary, and for your
// thermocouple board either "Adafruit MAX31855 library" or "MAX6675 library" by Adafruit.

#include <WiFi.h>
#include <PubSubClient.h>

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

// The ESP32 pins each board is wired to (the guide's wiring table). The boards share the
// clock and data pins; each has a select pin (CS) of its own.
// Only one probe? Set PIN_ET_SELECT to -1.
const int PIN_CLOCK = 18;      // the boards' CLK (also called SCK)
const int PIN_DATA = 19;       // the boards' DO (also called SO or MISO)
const int PIN_BT_SELECT = 5;   // CS of the board with the bean probe
const int PIN_ET_SELECT = 17;  // CS of the board with the second probe, or -1

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

Probe* btProbe = nullptr;
Probe* etProbe = nullptr;  // stays empty with PIN_ET_SELECT -1

WiFiClient network;
PubSubClient mqtt(network);
String telemetryTopic, statusTopic;

// One probe's temperature in °C, or NAN when it has no reading. Roastwarden wants °C; a
// missing probe is left out of the message, never sent as 0.
// A different sensor (a MAX31856, a PT100 board, an analog amplifier)? Change this function
// and the two lines in setup() that create the probes: the rest stays as it is.
float readProbe(Probe* probe) {
  if (probe == nullptr) return NAN;
  float c = probe->readCelsius();  // NAN when the thermocouple is unplugged
  if (c == 0.0) return NAN;        // exactly 0: the board is not wired or not powered
  return c;
}

void connectWifi() {
  Serial.printf("Wi-Fi: joining \"%s\" ", WIFI_NAME);
  WiFi.mode(WIFI_STA);
  WiFi.begin(WIFI_NAME, WIFI_PASSWORD);
  unsigned long started = millis();
  while (WiFi.status() != WL_CONNECTED) {
    delay(500);
    Serial.print(".");
    if (millis() - started > 20000) {
      Serial.println();
      Serial.println("Wi-Fi: not connected after 20 seconds. Check WIFI_NAME and WIFI_PASSWORD");
      Serial.println("  (capitals count), and that the network is 2.4 GHz: an ESP32 cannot join");
      Serial.println("  a 5 GHz network. Trying again.");
      WiFi.disconnect();
      WiFi.begin(WIFI_NAME, WIFI_PASSWORD);
      started = millis();
    }
  }
  Serial.printf("\nWi-Fi: connected. This ESP32's address is %s\n",
                WiFi.localIP().toString().c_str());
}

bool connectRoastwarden() {
  Serial.printf("Roastwarden: connecting to %s port %d as \"%s\"... ", SERVER_ADDRESS,
                SERVER_PORT, ROASTER_ID);
  // The last will: if this ESP32 goes quiet, the server tells Roastwarden it is offline.
  if (mqtt.connect(ROASTER_ID, ROASTER_ID, ROASTER_PASSWORD, statusTopic.c_str(), 1, true,
                   "{\"status\":\"offline\"}")) {
    // "caps": [] tells Roastwarden this roaster takes no commands of its own.
    mqtt.publish(statusTopic.c_str(),
                 "{\"status\":\"online\",\"version\":\"starter-1\",\"caps\":[]}", true);
    Serial.println("connected.");
    Serial.println("  In Roastwarden it appears under Roasters > Found, not added yet.");
    return true;
  }
  int why = mqtt.state();
  Serial.printf("failed (code %d).\n", why);
  if (why == MQTT_CONNECT_BAD_CREDENTIALS || why == MQTT_CONNECT_UNAUTHORIZED) {
    Serial.println("  Roastwarden turned down the id or the password. ROASTER_ID and");
    Serial.println("  ROASTER_PASSWORD must match a roaster under Roasters -> MQTT passwords in");
    Serial.println("  Roastwarden (or the MQTT_DEVICES line in its .env file; changed .env?");
    Serial.println("  Restart Roastwarden in its folder: docker compose up -d)");
  } else {
    Serial.println("  Nothing answered. Check SERVER_ADDRESS (on the server, hostname -I prints");
    Serial.println("  it), that Roastwarden runs (docker ps lists roastwarden), and that this");
    Serial.println("  ESP32 and the server are on the same network.");
  }
  return false;
}

// Roastwarden's own message, so nothing needs mapping: {"beanTemp":182.25,"envTemp":215.5}
void sendReadings(float bt, float et) {
  char message[64];
  if (!isnan(bt) && !isnan(et)) {
    snprintf(message, sizeof(message), "{\"beanTemp\":%.2f,\"envTemp\":%.2f}", bt, et);
  } else if (!isnan(bt)) {
    snprintf(message, sizeof(message), "{\"beanTemp\":%.2f}", bt);
  } else if (!isnan(et)) {
    snprintf(message, sizeof(message), "{\"envTemp\":%.2f}", et);
  } else {
    return;  // no reading at all: nothing to send
  }
  mqtt.publish(telemetryTopic.c_str(), message);
}

void printReadings(float bt, float et) {
  if (isnan(bt)) {
    Serial.print("BT: no reading   ");
  } else {
    Serial.printf("BT %.1f °C   ", bt);
  }
  if (etProbe == nullptr) {
    Serial.print("(no ET probe)");
  } else if (isnan(et)) {
    Serial.print("ET: no reading");
  } else {
    Serial.printf("ET %.1f °C", et);
  }
  Serial.println(mqtt.connected() ? "   sent" : "   not sent: not connected");
  if (isnan(bt) || (etProbe != nullptr && isnan(et))) {
    Serial.println("  No reading: check the thermocouple is screwed into its board, and the");
    Serial.println("  board's five wires (VIN or VCC to 3.3V, GND, CLK, DO, CS).");
  }
}

void setup() {
  Serial.begin(115200);
  delay(500);
  Serial.println();
  Serial.println("Roastwarden starter");

  btProbe = new Probe(PIN_CLOCK, PIN_BT_SELECT, PIN_DATA);
  if (PIN_ET_SELECT >= 0) etProbe = new Probe(PIN_CLOCK, PIN_ET_SELECT, PIN_DATA);
  delay(500);  // the boards' first conversion

  telemetryTopic = String("roaster/") + ROASTER_ID + "/telemetry";
  statusTopic = String("roaster/") + ROASTER_ID + "/status";
  mqtt.setServer(SERVER_ADDRESS, SERVER_PORT);

  connectWifi();
}

void loop() {
  if (WiFi.status() != WL_CONNECTED) {
    Serial.println("Wi-Fi: lost the connection.");
    connectWifi();
  }

  static unsigned long lastTry = 0;
  if (!mqtt.connected() && (lastTry == 0 || millis() - lastTry > 5000)) {
    lastTry = millis();
    connectRoastwarden();
  }
  mqtt.loop();

  // Once a second: read both probes and send them. Roastwarden works out the rate of rise
  // itself, so a steady second matters more than anything else here.
  static unsigned long lastRead = 0;
  static int count = 0;
  if (millis() - lastRead >= 1000) {
    lastRead = millis();
    float bt = readProbe(btProbe);
    float et = readProbe(etProbe);
    if (mqtt.connected()) sendReadings(bt, et);
    if (count++ % 5 == 0) printReadings(bt, et);  // every 5 seconds, to keep it readable
  }
}
