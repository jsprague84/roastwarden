// A minimal roaster for Roastwarden over Modbus TCP (docs/connect-your-roaster.md).
//
// An ESP32 on Wi-Fi as a Modbus TCP slave on port 502, unit 1: two MAX31855 thermocouple
// boards (BT and ET), a solid state relay for the heater and a PWM fan. Roastwarden polls it
// (the register map in roaster.json gives each register its role); Artisan can poll the same
// registers (Modbus, function 3, divider 1/10 for BT and ET).
//
//   holding  0  BT              int16, °C × 10   read
//            1  ET              int16, °C × 10   read
//            2  heater          %                read, write
//            3  fan             %                read, write
//            4  heater applied  %                read (after the interlocks)
//            5  heater enable   0 / 1            read, write
//            6  stop            write 1          emergency stop
//
// Its own safety: over-temperature cutoffs, a failed probe, no heat below FAN_MIN_PCT of fan,
// and no heat when nobody has read it for WATCHDOG_MS.
//
// Libraries: eModbus, Adafruit MAX31855. Board: any ESP32 (Arduino core 3).

#include <WiFi.h>
#include <ModbusServerWiFi.h>
#include <Adafruit_MAX31855.h>

// ---- Your settings ----
const char* WIFI_SSID = "your-wifi";
const char* WIFI_PASSWORD = "your-password";
const uint8_t UNIT_ID = 1;

// ---- Wiring ----
const int PIN_SCK = 18, PIN_MISO = 19, PIN_CS_BT = 5, PIN_CS_ET = 17;
const int PIN_HEATER = 26;  // SSR input
const int PIN_FAN = 27;     // PWM to the fan's MOSFET

// ---- Safety limits ----
const float BT_MAX = 250.0, ET_MAX = 300.0;  // °C: the heater trips above either
const float FAN_MIN_PCT = 30.0;              // no heat with less air than this
const unsigned long WATCHDOG_MS = 5000;      // no poll for this long: no heat

enum { R_BT, R_ET, R_HEATER, R_FAN, R_APPLIED, R_ENABLE, R_STOP, R_COUNT };

Adafruit_MAX31855 btProbe(PIN_SCK, PIN_CS_BT, PIN_MISO);
Adafruit_MAX31855 etProbe(PIN_SCK, PIN_CS_ET, PIN_MISO);
ModbusServerWiFi modbus;

float bt = NAN, et = NAN;
uint16_t heater = 0, fan = 0;  // what the master asked for, %
bool heaterEnable = true;
bool tripped = false;          // an over-temperature trip holds until heater enable 1, cold
volatile unsigned long lastPoll = 0;

uint16_t heaterApplied() {
  if (!heaterEnable || tripped) return 0;
  if (isnan(bt) || isnan(et)) return 0;          // a probe failed
  if (fan < FAN_MIN_PCT) return 0;               // no heat without air
  if (millis() - lastPoll > WATCHDOG_MS) return 0;  // nobody is watching
  return heater;
}

// A temperature as int16 × 10; a failed probe reads -3276.8, which the check flags.
uint16_t tenths(float c) {
  return (uint16_t)(int16_t)(isnan(c) ? -32768 : lroundf(c * 10));
}

uint16_t readRegister(uint16_t r) {
  switch (r) {
    case R_BT: return tenths(bt);
    case R_ET: return tenths(et);
    case R_HEATER: return heater;
    case R_FAN: return fan;
    case R_APPLIED: return heaterApplied();
    case R_ENABLE: return heaterEnable ? 1 : 0;
    default: return 0;  // stop reads 0
  }
}

bool writeRegister(uint16_t r, uint16_t v) {
  switch (r) {
    case R_HEATER: heater = min<uint16_t>(v, 100); return true;
    case R_FAN: fan = min<uint16_t>(v, 100); return true;
    case R_ENABLE:
      heaterEnable = v != 0;
      if (!heaterEnable) heater = 0;
      if (heaterEnable && bt < BT_MAX - 20 && et < ET_MAX - 20) tripped = false;
      return true;
    case R_STOP:
      if (v) { heater = 0; fan = 100; }
      return true;
    default: return false;  // BT, ET and heater applied are read only
  }
}

ModbusMessage readHolding(ModbusMessage request) {
  uint16_t addr = 0, words = 0;
  request.get(2, addr);
  request.get(4, words);
  ModbusMessage response;
  if (words == 0 || addr + words > R_COUNT) {
    response.setError(request.getServerID(), request.getFunctionCode(), ILLEGAL_DATA_ADDRESS);
    return response;
  }
  lastPoll = millis();
  response.add(request.getServerID(), request.getFunctionCode(), (uint8_t)(words * 2));
  for (uint16_t i = 0; i < words; i++) response.add(readRegister(addr + i));
  return response;
}

ModbusMessage writeSingle(ModbusMessage request) {
  uint16_t addr = 0, value = 0;
  request.get(2, addr);
  request.get(4, value);
  if (!writeRegister(addr, value)) {
    ModbusMessage response;
    response.setError(request.getServerID(), request.getFunctionCode(), ILLEGAL_DATA_ADDRESS);
    return response;
  }
  return ECHO_RESPONSE;
}

ModbusMessage writeMultiple(ModbusMessage request) {
  uint16_t addr = 0, words = 0, value = 0;
  request.get(2, addr);
  request.get(4, words);
  for (uint16_t i = 0; i < words; i++) {
    request.get(7 + 2 * i, value);
    if (!writeRegister(addr + i, value)) {
      ModbusMessage response;
      response.setError(request.getServerID(), request.getFunctionCode(), ILLEGAL_DATA_ADDRESS);
      return response;
    }
  }
  ModbusMessage response;
  response.add(request.getServerID(), request.getFunctionCode(), addr, words);
  return response;
}

void setup() {
  Serial.begin(115200);
  pinMode(PIN_HEATER, OUTPUT);
  digitalWrite(PIN_HEATER, LOW);
  analogWrite(PIN_FAN, 0);

  WiFi.begin(WIFI_SSID, WIFI_PASSWORD);
  while (WiFi.status() != WL_CONNECTED) delay(500);
  Serial.printf("Modbus TCP on %s:502, unit %u\n", WiFi.localIP().toString().c_str(), UNIT_ID);

  modbus.registerWorker(UNIT_ID, READ_HOLD_REGISTER, &readHolding);
  modbus.registerWorker(UNIT_ID, WRITE_HOLD_REGISTER, &writeSingle);
  modbus.registerWorker(UNIT_ID, WRITE_MULT_REGISTERS, &writeMultiple);
  modbus.start(502, 2, 20000);  // port, clients, idle timeout (ms)
}

void loop() {
  static unsigned long lastRead = 0;
  if (millis() - lastRead >= 250) {
    lastRead = millis();
    bt = btProbe.readCelsius();  // NAN when the probe is open or shorted
    et = etProbe.readCelsius();
    if (bt > BT_MAX || et > ET_MAX) {
      tripped = true;
      heater = 0;
      fan = 100;
    }
  }
  digitalWrite(PIN_HEATER, (millis() % 1000) < heaterApplied() * 10UL ? HIGH : LOW);
  analogWrite(PIN_FAN, fan * 255 / 100);
}
