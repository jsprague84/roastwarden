# Hardware Integration Guide

This guide documents how to connect a coffee roaster microcontroller to the Roastwarden server.
**Start with [`connect-your-roaster.md`](connect-your-roaster.md)**: what Roastwarden expects on
one page, and a working example per transport (`examples/roasters/`, tested). This one is the
reference for every field and command. Five transports are supported: **MQTT**, **WebSocket**, **Modbus TCP**, **Modbus RTU** (serial, RS-485), and **Serial (TC4 / aArtisan)**. Choose the one that best fits your hardware and network environment.

## Choosing a Protocol

| Feature | MQTT | WebSocket | Modbus TCP | Modbus RTU | Serial TC4 |
|---|---|---|---|---|---|
| **Best for** | Unreliable networks, multiple devices | Direct connections, browser simulators | SCADA/HMI integration, Ethernet PID controllers | PID controllers and commercial roasters on RS-485 | USB-tethered roaster, Artisan compatibility, no Wi-Fi |
| **Complexity** | Medium (needs MQTT broker) | Low (direct to server) | Low (standard industrial) | Low (an RS-485 adapter and the device's serial settings) | Lowest (ASCII lines over USB) |
| **Bidirectional** | Yes (pub/sub) | Yes (full-duplex) | Yes (read/write registers) | Yes (read/write registers) | Yes (command/response) |
| **Firewall friendly** | Requires broker access (port 1883) | Single HTTP upgrade (port 8080) | Dedicated port (default 502) | No network at all | No network at all |
| **Libraries needed** | PubSubClient (Arduino), rumqttc (Rust) | WebSocketsClient (Arduino), tungstenite (Rust) | Arduino Modbus, tokio-modbus (Rust) | Built into most controllers | `Serial` only |
| **Auto-discovery** | Yes (server detects new device_ids) | No (device must be pre-registered) | No (server polls one device) | No (server opens a configured port) | No (server opens a configured port) |
| **Multiple devices** | Yes (topic-based routing) | Yes (per-device endpoint) | Yes (one poller per device) | One port per device | Yes (one port per device) |
| **Message format** | JSON text | JSON text | Binary registers | Binary registers (CRC-checked frames) | CSV / `CMD;arg` lines |
| **Latency** | Low (broker hop) | Very low (direct) | Very low (direct) | Low (9600 baud: about 10 ms per read) | Very low (direct) |

**Recommendations:**
- **MQTT** is the default and most flexible option. It handles intermittent connectivity gracefully through broker-based pub/sub, supports auto-discovery, and works well when the device and server are on different networks.
- **WebSocket** is simplest for direct connections where you don't want to run an MQTT broker. Good for ESP32 devices on the same LAN, browser-based simulators, or devices behind firewalls that only allow outbound HTTP.
- **Modbus TCP** is ideal for integrating with SCADA/HMI software (e.g., connecting Artisan roasting software alongside the Roastwarden dashboard). Data is encoded as binary registers rather than JSON.
- **Modbus RTU** reaches the PID controllers (Fuji, Omron, Delta, Autonics, REX-C100…) and commercial or Chinese electric drum roasters that sit on an RS-485 bus. The same register map and command names as Modbus TCP apply.
- **Serial TC4** is immune to Wi-Fi and broker outages and makes the same firmware usable by Artisan's built-in TC4 driver. Recommended as the wired fallback for an ESP32 that sits next to the server (or a Raspberry Pi running it).

## Adding a roaster

A roaster that already speaks something (Tasmota, ESPHome, a stock aArtisan TC4, Artisan's
WebSocket protocol, a Modbus controller) needs no new firmware: add it, map its fields, check
it ([`add-roaster.md`](add-roaster.md)). In the dashboard: Roasters → New, then the
connection's **Mapping** and the roaster's **Check** tab. From the command line (admin token):

```bash
roast roaster add tc4_popper --transport serial_tc4 --port /dev/ttyUSB0 --preset stock-aartisan --kind logger
roast roaster map tc4_popper --try          # the newest message it sent and what each field becomes, in °C
roast roaster check tc4_popper              # rate, BT and ET, units, outputs, channels, caps, control
```

A new roaster logs only (control off) until you raise it under Roasters → the roaster
→ Setup; `roast roaster check --write` on a cold roaster then sends heater 10 % and
emergency stop to prove it obeys. The rest of this guide is the wire format for firmware you
write yourself.

## The interface standard

Roastwarden has no per-model drivers. A roaster works with it when it meets one of three tiers,
over any transport above, in its own field names, units and topics (the mapping adapts to the
device, not the reverse). The tier a roaster meets is the most its **control level** may be set
to (Roasters → the roaster → Setup, [`roaster-setup.md`](roaster-setup.md)); every roaster
starts at control off.

| Tier | The device provides | Roastwarden gives | Control level |
|---|---|---|---|
| **Logger** | BT (ET optional) at 0.5–4 Hz; extra probes as declared channels | The scope, RoR, phases, events, alarms that notify, profiles as a background, history, guidance, reports, labels | `off` |
| **Manual** | Logger + the outputs it accepts (heater or burner, fan or air, drum, damper; % or steps), each echoed in its telemetry; emergency stop | Buttons and sliders, STOP, alarm actions, the roast sequence | `manual` |
| **Full** | Manual + its own safety (below), optionally a `lease` and an onboard PID | Profile drive, the server's PID, autotune, gain schedule, fan assist | `full` |

**A full-tier device's own safety** (it must hold whatever Roastwarden sends or fails to send;
see the device contract in [control](control.md)):

- over-temperature cutoffs: heater off at a BT and an ET limit of its own;
- sensor checks: heater off when a thermocouple reads nothing for a few samples;
- an interlock that keeps the heater off without airflow (or drum rotation);
- a comms watchdog: no controller for a while cuts the heater;
- presence: a retained `online` status with a last will `offline` (MQTT), or the equivalent;
- optional: `lease` (heater off when the server stops renewing it) and an onboard PID (`caps`).

**How a roaster is checked** ([`add-roaster.md`](add-roaster.md)): `roast roaster check <id>`
(or the Check tab) reads the rate, BT and ET, units, each declared output's echo, each declared
channel and the caps, and answers ok, warn or fail per line. The write check
(`--write`, a cold roaster, control manual or full) sends heater 10 %, waits for its echo, then
sends emergency stop and waits for the heater to read 0. The device's own safety cannot be
seen from the server: the firmware's author vouches for it before raising control to `full`.

## Telemetry JSON Format

Both MQTT and WebSocket protocols use the same JSON telemetry format. The field names use camelCase to match the ESP32 firmware convention.

```json
{
  "timestamp": 123456,
  "beanTemp": 185.5,
  "envTemp": 145.2,
  "rateOfRise": 12.5,
  "heater": 75.0,
  "heaterApplied": 75.0,
  "fan": 70.6,
  "setpoint": 200.0,
  "controlMode": 1,
  "heaterEnable": 1,
  "uptime": 1234,
  "Kp": 15.0,
  "Ki": 1.0,
  "Kd": 25.0,
  "freeHeap": 245760,
  "rssi": -45,
  "systemStatus": 0
}
```

### Field Reference

| Field | Type | Required | Description |
|---|---|---|---|
| `beanTemp` | float | Yes | Bean temperature in °C |
| `envTemp` | float | Strongly (the check warns without it) | Environment/exhaust temperature in °C |
| `rateOfRise` | float | No | Bean temperature rate of rise in °C/min |
| `heater` | float | When a heater is declared | Heater command, % (0-100), echoed |
| `heaterApplied` | float | No | Heater % that reached the output after the device's interlocks |
| `fan` | float | When a fan is declared | Fan, % (0-100), echoed |
| `setpoint` | float | With an onboard PID | Current target bean temperature in °C |
| `controlMode` | int | With an onboard PID | 0 = manual, 1 = auto (PID) |
| `heaterEnable` | int | No | 0 = heater disabled, 1 = heater enabled |
| `timestamp` | int | No | Device uptime in milliseconds |
| `uptime` | int | No | Device uptime in seconds |
| `Kp` | float | No | Current PID proportional gain |
| `Ki` | float | No | Current PID integral gain |
| `Kd` | float | No | Current PID derivative gain |
| `freeHeap` | int | No | Free heap memory in bytes |
| `rssi` | int | No | WiFi signal strength in dBm |
| `systemStatus` | int | No | System status code (0 = normal); refreshed every 5 s, so a safety stop's code may not show |
| `stopCode` | int | No | Why the device last cut its heater, latched until the next stop: 0 none, 1 emergency stop (MQTT), 2 serial STOP, 3/4 BT/ET over-temperature, 5/6 BT/ET thermocouple failed, 7 control lease expired, 8 no controller, 9 server offline, 10 OTA update, 11 onboard autotune ended |
| `stopUptimeMs` | int | No | Device uptime (ms) of that stop; 0 = none since boot |

## MQTT Connection

### Overview

The ESP32 publishes telemetry and subscribes to control commands via an MQTT broker. The Roastwarden server connects to the same broker, consuming telemetry and publishing control commands on behalf of the dashboard.

```
ESP32 <--MQTT--> Broker <--MQTT--> Roastwarden Server <--HTTP/WS--> Dashboard
```

### Server Configuration

| Environment Variable | Default | Description |
|---|---|---|
| `MQTT_BROKER_HOST` | `localhost` | MQTT broker hostname |
| `MQTT_BROKER_PORT` | `1883` | MQTT broker port |
| `MQTT_CLIENT_ID` | auto-generated | Server's MQTT client ID |
| `MQTT_USERNAME` | (none) | MQTT authentication username (`roastwarden` with the shipped broker) |
| `MQTT_PASSWORD` | (none) | MQTT authentication password |

### Credentials

Roastwarden's built-in broker (the one-container install) and the Mosquitto in the
repository's `docker-compose.yml` take **no anonymous clients** ([security](../SECURITY.md)).
The server signs in as `roastwarden`; each device signs in with its **device id as the user
name** and its own password: on the built-in broker added under Roasters → MQTT passwords
(at once; [MQTT passwords](add-roaster.md#mqtt-passwords)) or listed in `.env` as
`MQTT_DEVICES=<id>:<password>,…` (read at start; Mosquitto reads only this). A device may publish and subscribe only under
`roaster/<its id>/#` and read `roastwarden/server/status`; anything else it sends is dropped. A
device with topics of its own (below, "Your own field names") must keep them under
`roaster/<its id>/` on the built-in broker; on the Mosquitto, give it its own lines in the ACL
(`docker/mosquitto/config/acl.in`). With your own broker, give the server and each device
credentials the same way.

### Topic Layout

All topics are namespaced under `roaster/{device_id}/` where `{device_id}` is a unique identifier for the device (e.g., `my_roaster`).

#### Published by Device (consumed by server)

| Topic | Payload | QoS | Retained | Frequency |
|---|---|---|---|---|
| `roaster/{device_id}/telemetry` | Telemetry JSON (see above) | 0 | No | 1 Hz |
| `roaster/{device_id}/status` | Status JSON (see below) | 0 | Yes | On connect/disconnect |

**Status JSON:**
```json
{
  "status": "online",
  "id": "my_roaster-ABC123",
  "ip": "192.168.1.100",
  "rssi": -45,
  "freeHeap": 245760,
  "version": "1.0.0"
}
```

#### Subscribed by Device (published by server)

| Topic | Payload | Description |
|---|---|---|
| `roaster/{device_id}/control/setpoint` | `200.0` | Target bean temperature (°C, float) |
| `roaster/{device_id}/control/fan` | `70.5` | Fan, % (0-100, decimals allowed) |
| `roaster/{device_id}/control/heater` | `75` | Manual heater power, % (0-100, decimals allowed) |
| `roaster/{device_id}/control/mode` | `"auto"` or `"manual"` | Control mode selection |
| `roaster/{device_id}/control/heater_enable` | `"1"` or `"0"` | Enable/disable heater |
| `roaster/{device_id}/control/pid` | `{"kp":15.0,"ki":1.0,"kd":25.0}` | PID tuning parameters |
| `roaster/{device_id}/control/emergency_stop` | `"1"` | Trigger emergency stop |
| `roaster/{device_id}/control/drum` | `55` | Drum speed, % (only when the roaster's setup declares a drum output) |
| `roaster/{device_id}/control/damper` | `30` | Damper, % (only when the setup declares a damper output) |

### Your own field names (MQTT)

A device that cannot speak the topics and JSON above (a Tasmota board, an ESPHome node, a
commercial controller) does not need reflashing: the MQTT connection's config says how to read
it and how to write to it ([ingest mapping](ingest-mapping.md)). A Tasmota board with a MAX31855 in °F and a
dimmer for the heater:

```json
{
  "topic_prefix": "cmnd/roaster",
  "telemetry_topic": "tele/roaster/SENSOR",
  "ingest": {
    "units": "F",
    "fields": {
      "bt": "MAX31855.Temperature",
      "et": { "path": "DS18B20.Temperature" },
      "heater": { "path": "Dimmer", "scale": 1 }
    }
  },
  "commands": { "heater": { "topic": "cmnd/roaster/Dimmer", "payload": "{value}", "decimals": 0 } }
}
```

- `ingest.fields` maps a role (`bt`, `et`, `ambient`, `ror`, `heater`, `heater_applied`, `fan`,
  `drum`, `damper`, `setpoint`, `mode`, `heater_enable`) or an extra channel's name (`exhaust`) to
  a JSON path (`data.BT`, `probes[1].value`, `$` for a plain number), optionally with `scale`,
  `offset`, an extra's own `unit` and its own `topic`. Only mapped fields pass. `units: "F"`
  converts the temperatures (and the RoR) to °C.
- A field with its own `topic` (ESPHome's `<node>/sensor/<name>/state`) is read from that topic;
  a sample is made when BT's message arrives (ET's without BT), from each field's newest value
  no older than 10 s.
- `commands` templates write `setpoint`, `heater`, `fan`, `drum`, `damper`, `mode`,
  `heater_enable` and `emergency_stop` in the device's syntax: `topic` (default
  `<topic_prefix>/control/<command>`), `payload` with `{value}`, `scale`, `decimals`. With `units:
  "F"` a setpoint goes out in °F. A command with no template goes out as above under the prefix.
- A topic is one roaster's: a topic another roaster's connection reads is refused. Status and
  capabilities stay on `roaster/{device_id}/status`.

### Discovery on the LAN (mDNS)

The server browses for `_roastwarden._tcp` and `_modbus._tcp` services and lists them on the Roasters page (`/roasters`) under "Found, not added yet" with **Set up**, which prefills the Add roaster wizard (an MQTT roaster already streaming also has **Add**, which keeps it as it is). Advertise from firmware with the ESP32 mDNS responder:

```cpp
MDNS.addService("roastwarden", "tcp", 1883);          // port is informational for MQTT devices
MDNS.addServiceTxt("roastwarden", "tcp", "device_id", DEVICE_ID);
MDNS.addServiceTxt("roastwarden", "tcp", "proto", "mqtt");   // mqtt | websocket | modbus_tcp
MDNS.addServiceTxt("roastwarden", "tcp", "version", FIRMWARE_VERSION);
```

Multicast does not cross Docker's default bridge: run the server container with `network_mode: host` for discovery, or disable it with `ROASTWARDEN_MDNS=false`.

### Controller heartbeat

The server publishes a retained presence message on `roastwarden/server/status` (`{"status":"online",...}`) when it connects and leaves a last will of `{"status":"offline"}`. Firmware that follows profiles or alarms driven by the server should subscribe to it and fall back to a safe state (heater off, fan on) when it reads `offline` for a while, in addition to the broker-connection timeout it already has. The topic is deliberately outside `roaster/` so it is never mistaken for a device.

### Telemetry enrichment

Whatever the transport, the server computes one canonical `rateOfRise` (°C/min) from `beanTemp` using the window and smoothing algorithm from Settings (moving average, weighted moving average, or Savitzky-Golay) and stores that value with the session. A device-supplied `rateOfRise` is kept as `deviceRateOfRise` for comparison, so firmware need not compute it at all.

### Auto-discovery

When the Roastwarden server receives telemetry from an unknown `device_id`, it automatically creates a device entry with status `pending`. It streams at once and can be picked as the roaster; Roasters lists it under "Found, not added yet", where **Add** keeps it as it is and **Set up** walks it through Add roaster's steps (its kind, connection, check and control level).

### ESP32 Example (Arduino/PlatformIO)

```cpp
#include <WiFi.h>
#include <PubSubClient.h>
#include <ArduinoJson.h>

// --- Configuration ---
const char* WIFI_SSID     = "your-wifi";
const char* WIFI_PASSWORD = "your-password";
const char* MQTT_HOST     = "192.168.1.50";   // your broker
const int   MQTT_PORT     = 1883;
const char* DEVICE_ID     = "my_roaster";
const char* MQTT_PASSWORD = "its-password";    // Roasters → MQTT passwords, or MQTT_DEVICES

WiFiClient wifiClient;
PubSubClient mqtt(wifiClient);

// Topic strings
String telemetryTopic;
String controlPrefix;

void setup() {
    Serial.begin(115200);
    WiFi.begin(WIFI_SSID, WIFI_PASSWORD);
    while (WiFi.status() != WL_CONNECTED) delay(500);

    telemetryTopic = String("roaster/") + DEVICE_ID + "/telemetry";
    controlPrefix  = String("roaster/") + DEVICE_ID + "/control/";

    mqtt.setServer(MQTT_HOST, MQTT_PORT);
    mqtt.setCallback(onMqttMessage);
}

void loop() {
    if (!mqtt.connected()) reconnectMqtt();
    mqtt.loop();

    // Publish telemetry at 1 Hz
    static unsigned long lastPublish = 0;
    if (millis() - lastPublish >= 1000) {
        lastPublish = millis();
        publishTelemetry();
    }
}

void publishTelemetry() {
    DynamicJsonDocument doc(512);
    doc["timestamp"]     = millis();
    doc["beanTemp"]      = readBeanTemp();
    doc["envTemp"]       = readEnvTemp();
    doc["rateOfRise"]    = calculateRoR();
    doc["heater"]        = getHeaterPercent();  // 0-100
    doc["fan"]           = getFanPercent();     // 0-100
    doc["setpoint"]      = getSetpoint();
    doc["controlMode"]   = getControlMode();   // 0=manual, 1=auto
    doc["heaterEnable"]  = isHeaterEnabled();   // 0 or 1
    doc["uptime"]        = millis() / 1000;
    doc["freeHeap"]      = ESP.getFreeHeap();
    doc["rssi"]          = WiFi.RSSI();

    String payload;
    serializeJson(doc, payload);
    mqtt.publish(telemetryTopic.c_str(), payload.c_str());
}

void onMqttMessage(char* topic, byte* payload, unsigned int length) {
    String topicStr(topic);
    String payloadStr((char*)payload, length);

    if (topicStr == controlPrefix + "setpoint") {
        setTargetTemp(payloadStr.toFloat());
    } else if (topicStr == controlPrefix + "fan") {
        setFanPercent(payloadStr.toFloat());     // 0-100
    } else if (topicStr == controlPrefix + "heater") {
        setHeaterPercent(payloadStr.toFloat());  // 0-100
    } else if (topicStr == controlPrefix + "mode") {
        setControlMode(payloadStr == "auto" ? 1 : 0);
    } else if (topicStr == controlPrefix + "heater_enable") {
        setHeaterEnabled(payloadStr == "1");
    } else if (topicStr == controlPrefix + "emergency_stop") {
        emergencyStop();
    } else if (topicStr == controlPrefix + "pid") {
        DynamicJsonDocument doc(256);
        if (!deserializeJson(doc, payloadStr)) {
            setPIDTunings(doc["kp"], doc["ki"], doc["kd"]);
        }
    }
}

void reconnectMqtt() {
    while (!mqtt.connected()) {
        // The device id is its user name on the broker
        if (mqtt.connect(DEVICE_ID, DEVICE_ID, MQTT_PASSWORD)) {
            // Subscribe to all control topics
            String wildcard = controlPrefix + "#";
            mqtt.subscribe(wildcard.c_str());
        } else {
            delay(5000);
        }
    }
}
```

## WebSocket Connection

### Overview

Devices can connect directly to the Roastwarden server via WebSocket, bypassing the need for an MQTT broker. The device pushes telemetry JSON over the WebSocket and receives control commands on the same connection.

```
ESP32 <--WebSocket--> Roastwarden Server <--HTTP/WS--> Dashboard
```

### Requirements

- The device must be **pre-registered** in the Roastwarden device configuration with status `active`. Unlike MQTT, there is no auto-discovery for WebSocket devices.
- The server validates that the `device_id` exists and is active. Unless authentication is off (below), the device must also present the owner key, as `?token=<key>` on the URL or an `Authorization: Bearer <key>` header, otherwise the upgrade is refused with HTTP 401.

### Endpoint

```
WS /ws/device/{device_id}/telemetry
```

Connect to: `ws://<server-host>:8080/ws/device/{device_id}/telemetry?token=<owner key>`

### Sending Telemetry

Send JSON text messages using the same telemetry format described above. Messages are processed identically to MQTT telemetry: cached, persisted to the database, and recorded to any active roast session.

### Receiving Control Commands

The server sends control commands as JSON messages on the same WebSocket connection:

```json
{
  "type": "control",
  "topic": "roaster/{device_id}/control/setpoint",
  "payload": "200.0"
}
```

The `topic` field uses the same MQTT topic format, so devices can parse it the same way regardless of transport. The `payload` field contains the control value as a string.

### Connection Lifecycle

1. **Connect** — HTTP upgrade to WebSocket at `/ws/device/{device_id}/telemetry`
2. **Validate** — Server checks device exists and has status `active`; returns HTTP 404 if not found, HTTP 403 if not active
3. **Stream** — Device sends telemetry JSON; server sends control commands JSON
4. **Disconnect** — Either side closes the connection; server cleans up and logs the event

### Your own field names (the WebSocket client)

The inbound socket above is Roastwarden's own protocol and takes no mapping. A device with its
own WebSocket server is reached by a **WebSocket connection** (the server dials `url`), and that
connection takes the same `ingest` and `commands` as MQTT ([ingest mapping](ingest-mapping.md)), plus a
`poll` for a device that answers requests rather than pushes, as Artisan's WebSocket roasters do:

```json
{
  "url": "ws://192.168.1.40/ws",
  "poll": { "message": "{\"command\":\"getData\",\"id\":{id},\"roasterID\":0}", "interval_ms": 1000 },
  "ingest": { "fields": { "bt": "data.BT", "et": "data.ET" } },
  "commands": { "heater": { "payload": "{\"command\":\"setBurner\",\"value\":{value}}", "decimals": 0 } }
}
```

`{id}` counts 1, 2, … ; `interval_ms` is at least 200. Each text frame received goes through the
mapping; a frame with neither BT nor ET (an answer to a command) is no sample. A template's
`payload` is the whole frame sent; a command with no template is the control frame above.

### ESP32 Example (Arduino)

```cpp
#include <WiFi.h>
#include <WebSocketsClient.h>
#include <ArduinoJson.h>

const char* WIFI_SSID       = "your-wifi";
const char* WIFI_PASSWORD   = "your-password";
const char* SERVER_HOST     = "192.168.1.100";
const int   SERVER_PORT     = 8080;
const char* DEVICE_ID       = "my_roaster";

WebSocketsClient ws;

void setup() {
    Serial.begin(115200);
    WiFi.begin(WIFI_SSID, WIFI_PASSWORD);
    while (WiFi.status() != WL_CONNECTED) delay(500);

    String path = String("/ws/device/") + DEVICE_ID + "/telemetry";
    ws.begin(SERVER_HOST, SERVER_PORT, path);
    ws.onEvent(onWebSocketEvent);
    ws.setReconnectInterval(5000);
}

void loop() {
    ws.loop();

    static unsigned long lastPublish = 0;
    if (millis() - lastPublish >= 1000) {
        lastPublish = millis();
        sendTelemetry();
    }
}

void sendTelemetry() {
    if (!ws.isConnected()) return;

    DynamicJsonDocument doc(512);
    doc["timestamp"]     = millis();
    doc["beanTemp"]      = readBeanTemp();
    doc["envTemp"]       = readEnvTemp();
    doc["rateOfRise"]    = calculateRoR();
    doc["heater"]        = getHeaterPercent();  // 0-100
    doc["fan"]           = getFanPercent();     // 0-100
    doc["setpoint"]      = getSetpoint();
    doc["controlMode"]   = getControlMode();
    doc["heaterEnable"]  = isHeaterEnabled();
    doc["uptime"]        = millis() / 1000;
    doc["freeHeap"]      = ESP.getFreeHeap();
    doc["rssi"]          = WiFi.RSSI();

    String payload;
    serializeJson(doc, payload);
    ws.sendTXT(payload);
}

void onWebSocketEvent(WStype_t type, uint8_t* payload, size_t length) {
    switch (type) {
        case WStype_CONNECTED:
            Serial.println("WebSocket connected");
            break;
        case WStype_DISCONNECTED:
            Serial.println("WebSocket disconnected");
            break;
        case WStype_TEXT: {
            // Parse control command from server
            DynamicJsonDocument doc(512);
            if (deserializeJson(doc, payload, length)) break;

            const char* msgType = doc["type"];
            if (msgType && strcmp(msgType, "control") == 0) {
                String topic = doc["topic"].as<String>();
                String value = doc["payload"].as<String>();
                handleControlCommand(topic, value);
            }
            break;
        }
        default:
            break;
    }
}

void handleControlCommand(const String& topic, const String& value) {
    // Extract the command suffix from the MQTT-style topic
    // e.g., "roaster/my_roaster/control/setpoint" -> "setpoint"
    int lastSlash = topic.lastIndexOf('/');
    if (lastSlash < 0) return;
    String command = topic.substring(lastSlash + 1);

    if (command == "setpoint") {
        setTargetTemp(value.toFloat());
    } else if (command == "fan") {
        setFanPercent(value.toFloat());     // 0-100
    } else if (command == "heater") {
        setHeaterPercent(value.toFloat());  // 0-100
    } else if (command == "mode") {
        setControlMode(value == "auto" ? 1 : 0);
    } else if (command == "heater_enable") {
        setHeaterEnabled(value == "1");
    } else if (command == "emergency_stop") {
        emergencyStop();
    } else if (command == "pid") {
        DynamicJsonDocument pidDoc(256);
        if (!deserializeJson(pidDoc, value)) {
            setPIDTunings(pidDoc["kp"], pidDoc["ki"], pidDoc["kd"]);
        }
    }
}
```

## Modbus TCP Connection

### Overview

The Roastwarden server can expose a Modbus TCP server, allowing SCADA/HMI software (such as Artisan roasting software) to read telemetry and write control values using the standard industrial Modbus protocol.

```
Artisan/HMI <--Modbus TCP--> Roastwarden Server <--MQTT/WS--> ESP32
```

The Modbus server acts as a **bridge**: it answers reads from the server's newest telemetry of one roaster (from any transport) and, when writes are on, turns register writes into the same commands the dashboard sends. Artisan as a screen, with the setup file the server writes for it and the check against Artisan 4.2.0, is [`artisan-screen.md`](artisan-screen.md).

### Server Configuration

The Modbus TCP server is **disabled by default**. Set the following environment variables to enable it:

| Environment Variable | Default | Description |
|---|---|---|
| `ROASTWARDEN_MODBUS_ADDR` | (disabled) | Bind address, e.g., `0.0.0.0:502` |
| `ROASTWARDEN_MODBUS_DEVICE_ID` | the one real roaster | The roaster it serves; unset, the one real roaster heard in the last minute (the virtual roasters never count; with none or several it serves nothing) |
| `ROASTWARDEN_MODBUS_WRITES` | off | `on`: writes become commands through the roaster's control gate. Off: every write is refused (illegal function) |

A read while the roaster is silent (no telemetry for 10 s), or with no roaster to serve, is answered with exception 0x0B (gateway target failed to respond); a read past the map with 0x02. A write that the gate refuses (control off, an output not declared) or that cannot be delivered is answered with 0x04, a half setpoint or a % over 100 with 0x03, a reserved register with 0x02. Every write is in the audit log (`MODBUS /modbus/holding/<n>`, actor `modbus`). Modbus has no authentication: keep the port on the LAN.

### Register Map: Roastwarden Standard

#### Input Registers (Function Code 0x04 — Read Only)

These registers expose live telemetry data. Float32 values are encoded as IEEE 754 big-endian across two consecutive 16-bit registers (high word at lower address).

| Address | Name | Data Type | Unit | Description |
|---|---|---|---|---|
| 0x0000-0x0001 | bean_temp | float32 | °C | Bean temperature |
| 0x0002-0x0003 | env_temp | float32 | °C | Environment temperature |
| 0x0004-0x0005 | rate_of_rise | float32 | °C/min | Bean temperature rate of rise |
| 0x0006 | heater | uint16 | % | Heater command, whole % (0-100) |
| 0x0007 | fan | uint16 | % | Fan, whole % (0-100) |

#### Holding Registers (Function Code 0x03 — Read/Write)

Reading them gives the roaster's current values from its telemetry (heater enable is the heater above 0 when the roaster does not report it). With writes on, writing one sends the command to the roaster through its control gate; the setpoint needs both of its registers in one write (function code 16).

| Address | Name | Data Type | Unit | Description |
|---|---|---|---|---|
| 0x0000-0x0001 | setpoint | float32 | °C | Target bean temperature |
| 0x0002 | fan_cmd | uint16 | % | Fan command, whole % (0-100) |
| 0x0003 | heater_cmd | uint16 | % | Heater command, whole % (0-100) |
| 0x0004 | control_mode | uint16 | — | 0 = manual, 1 = auto |
| 0x0005 | heater_enable | uint16 | — | 0 = off, 1 = on |
| 0x0006-0x000B | (reserved) | — | — | Gap registers (reads return 0, a write is refused) |
| 0x000C | emergency_stop | uint16 | — | Write 1 to trigger emergency stop |

### Float32 Encoding

Float32 values occupy two consecutive 16-bit registers using IEEE 754 big-endian byte order (ABCD):

```
Value: 185.5 (0x43398000)

Register N:     0x4339  (high word, bytes A and B)
Register N+1:   0x8000  (low word, bytes C and D)
```

To decode: `float_value = (reg[N] << 16) | reg[N+1]`, interpreted as IEEE 754 single-precision.

### Register Map: Artisan Compatible

For integration with [Artisan](https://artisan-scope.org/) roasting software, an alternative register layout is available that uses int16 values scaled by x10 (the standard convention for industrial PID controllers):

#### Input Registers (Function Code 0x04)

| Address | Name | Data Type | Scale | Unit | Description |
|---|---|---|---|---|---|
| 0x0000 | BT | int16 | x0.1 | °C | Bean Temperature (value / 10 = °C) |
| 0x0001 | ET | int16 | x0.1 | °C | Environment Temperature (value / 10 = °C) |
| 0x0002 | BT_RoR | int16 | x0.1 | °C/min | BT Rate of Rise (value / 10 = °C/min) |
| 0x0003 | heater_duty | uint16 | 1 | % | Heater duty cycle (0-100) |
| 0x0004 | fan_duty | uint16 | 1 | % | Fan duty cycle (0-100) |

#### Holding Registers (Function Code 0x03)

| Address | Name | Data Type | Scale | Unit | Description |
|---|---|---|---|---|---|
| 0x0000 | SV | int16 | x0.1 | °C | Set Value / target temperature (value / 10 = °C) |
| 0x0001 | heater_cmd | uint16 | 1 | % | Heater command (0-100) |
| 0x0002 | fan_cmd | uint16 | 1 | % | Fan command (0-100) |
| 0x0003 | pid_mode | uint16 | 1 | — | 0 = manual, 1 = auto |

#### Coil Registers (Function Code 0x01/0x05)

| Address | Name | Description |
|---|---|---|
| 0x0000 | heater_on | Heater enable (on/off) |
| 0x0001 | fan_on | Fan enable (on/off) |
| 0x0002 | e_stop | Emergency stop |

> **Note:** The Artisan compatible layout is a dashboard-side preset for configuring custom Modbus devices. The built-in Roastwarden Modbus server uses the Roastwarden Standard layout, which Artisan reads as it is (float32 inputs with word order big; [`artisan-screen.md`](artisan-screen.md)).

### Roastwarden as Modbus Master (polling your device)

When a device is configured with a `modbus_tcp` or `modbus_rtu` connection (Modbus RTU below), the server connects to it, reads the registers in its register map at `poll_interval_ms`, and publishes the result as telemetry. The same connection carries control: dashboard commands are translated into holding-register or coil writes using the register **names** in the map, so the layout is up to you. Recognised names (case-insensitive):

| Command | Register names |
|---|---|
| Setpoint | `setpoint`, `SV`, `target_temp`, `bean_setpoint` |
| Fan | `fan_cmd`, `fan_setpoint`, `fan`, `IO3`, `air` |
| Heater | `heater_cmd`, `heater_setpoint`, `heater`, `OT1`, `burner` |
| Mode | `control_mode`, `pid_mode`, `mode` (0 = manual, 1 = auto) |
| Heater enable | `heater_enable`, `heater_on` |
| Emergency stop | `emergency_stop`, `e_stop`, `estop` |
| PID gains | `kp` / `ki` / `kd` (all three required) |

Heater and fan commands are in %. A register on another scale converts with
its `scale_factor` (engineering value = raw × scale + offset): a 0-255 PWM
fan register uses `scale_factor` 0.3922, and the same factor turns its
readings into %. Telemetry registers named `heater`/`heater_duty` and
`fan`/`fan_duty` are reported as `heater` and `fan`.

Only holding registers and coils marked `writable` are written: a read-back register named `heater` with `writable: false` gets no command. When the map has a role only as read-only registers, the command fails with a message naming the register (`the register map has 'heater' (holding register 6) for heater, but it is read-only; nothing was written`, HTTP 502) rather than writing anything else; a map with no register for the role cannot carry the command (501).

Reads go in contiguous blocks, each from its first address: input and holding registers at most 125 per read, coils and discrete inputs (function codes 1 and 2, read as 0/1) at most 2000. So a map may sit anywhere in the device's address space (a PID controller's PV and SV at `0x1000`, say); a gap in the map starts a new read rather than asking for addresses the map does not name. A read the device answers with an exception leaves its registers out of that sample and the rest still go through (the log names the refused reads once); when every read is refused, or the device does not answer within `timeout_ms`, the connection is reopened with a backoff of 1 to 30 s.

**Data types**: `uint16`, `int16`, `float32`, `uint32`, `int32`, `bool` (a coil or discrete input), and binary-coded decimal: `bcd16` (one register, four digits: `0x1234` is 1234) and `bcd32` (two registers in the map's word order, eight digits), as a Loring's PLC sends them. A BCD digit above 9 is a bad read and leaves the value out; a write goes out as the same digits.

**The register map is checked when it is saved** (`PUT /api/devices/{id}/register-map`); a bad one is refused with a 400 naming the register and what is wrong: a missing or repeated name (case-insensitive), an address outside 0–65535 (a 32-bit value, `bcd32` too, takes two), a coil or discrete input that is not `bool`, an input register or discrete input marked `writable`, a word order other than `AB`, `BA`, `ABCD`, `CDAB`, `BADC`, `DCBA`, a scale factor of 0.

**Changes apply at once.** Adding, editing, enabling, disabling or deleting a connection, saving a register map, or setting a device active or not starts, restarts or stops its pollers there and then; no server restart. A connection's config is checked for its protocol when it is saved (a 400 says what is wrong), and the connection test (`POST /api/devices/test-connection`) opens the link and reads the device's saved register map once (the Roastwarden Standard layout when there is none yet), answering with the values it read.

| `modbus_tcp` field | Default | Description |
|---|---|---|
| `host` | | IP address or host name (resolved at each connect) |
| `port` | `502` | |
| `unit_id` | `1` | |
| `poll_interval_ms` | `1000` | At least 100 |
| `timeout_ms` | `1000` | How long one request waits for its answer (50–10000) |

Values are encoded with the register's data type, scale and offset in reverse (`raw = (value - offset) / scale`), so an Artisan-style `SV` int16 ×0.1 register receives `2004` for 200.4 °C. Feedforward and the lease are not available over Modbus. If no register map is configured, the Roastwarden Standard layout (input and holding registers above) is assumed.

### Modbus RTU (serial, RS-485)

Protocol `modbus_rtu` is the same master over a serial line: the same register map, read plan, command names and checks, framed as RTU (binary, CRC-checked) by tokio-modbus over the serial port. Most PID controllers and commercial drum roasters with a "Modbus" or "RS-485" port speak it; an RS-485 to USB adapter on the server (or the Raspberry Pi running it) is the usual wiring (A/B to the controller's A/B or D+/D−, and ground).

| Field | Default | Description |
|---|---|---|
| `port` | | Serial device, e.g. `/dev/ttyUSB0` |
| `baud` | `9600` | The controller's setting (9600 and 19200 are common) |
| `parity` | `none` | `none`, `even` or `odd`, as the controller is set |
| `framing` | `rtu` | `rtu`, or `ascii` for Modbus ASCII (below) |
| `data_bits` | `8` | Modbus RTU frames are 8 data bits; anything else is refused (ASCII: 7 or 8) |
| `stop_bits` | `1` | `1` or `2` |
| `unit_id` | `1` | The controller's slave address, 1–247 |
| `poll_interval_ms` | `1000` | At least 100; at 9600 baud a read of a few registers takes about 10 ms |
| `timeout_ms` | `1000` | How long one request waits; a silent device reopens the port |

**Modbus ASCII** (`framing: "ascii"`): some controllers (Delta DTA and DTK, several in Artisan's catalogue) frame Modbus as text lines, `:`, the slave and the message in hex, an LRC, CRLF, usually at 7 data bits and even parity (`7E1`). The same connection takes them with `framing: "ascii"` and `data_bits: 7`; the register map, slaves and writes are as for RTU (`roast roaster add ID --transport modbus_rtu --port /dev/ttyUSB0 --framing ascii --parity even`).

One connection owns its port. **Several controllers on one RS-485 bus** (BT on one PID, ET on another, the burner's output on a third, as most of Artisan's machine setups have) are one roaster with one connection: a register in the map names its own slave (`unit_id`, 1–247; empty: the connection's), and the poller reads and writes each at its own address ([`artisan-machines.md`](artisan-machines.md)). A roaster in Artisan's catalogue is set up with all of this from its machine setup: `roast roaster add ID --artisan BRAND/MODEL --port /dev/ttyUSB0`, or Add roaster → "Start from an Artisan machine". In Docker, pass the adapter through and give the container user the `dialout` group, as for Serial TC4 below. Feedforward and the lease are not available, as over TCP.

### How commands are routed

Every control command goes through one transport registry:

1. A live direct connection for the device wins: an inbound device WebSocket, an outbound WebSocket the server dialled, a connected Modbus poller (TCP or RTU) or serial TC4 port. The `priority` field on the device connection breaks ties when several are live.
2. Otherwise the command is published on MQTT if the device has an MQTT connection configured, has no connection rows at all, or is unknown to the device registry.
3. Otherwise the API answers `503` with a message instead of publishing into the void. `501` means the live transport cannot carry that command (the lease over Modbus), `502` a delivery failure, `504` an acknowledgement timeout.

`GET /api/roaster/{device_id}/transports` lists the live transports for a device.

### Your own field names (register roles)

A register's name decides what it is only through a list of known names (`beanTemp`, `BT`, `SV`,
`heater_cmd`, …). Give a register a **role** instead (`bt`, `et`, `ambient`, `ror`, `heater`,
`heater_applied`, `fan`, `drum`, `damper`, `setpoint`, `mode`, `heater_enable`, or
`emergency_stop` for a write; the register map editor's Role column, `role` in the API) and its
name is free: a role's register is read as the role's field and a command writes the writable
holding register or coil with its role, before any name. A temperature role in a register whose
unit is `°F` is converted, and a setpoint register in `°F` is written in °F. A role is on one
readable and one writable register at most (a heater's output read-back and its command
register, say); the read-only one is the reading.

### Modbus TCP Slave Example (Arduino/ESP32)

If your device acts as a Modbus TCP slave (the Roastwarden server or Artisan polls it directly), use a Modbus library:

```cpp
#include <WiFi.h>
#include <ModbusServerTCPasync.h>  // eModbus library

const char* WIFI_SSID     = "your-wifi";
const char* WIFI_PASSWORD = "your-password";

ModbusServerTCPasync modbusServer;

// Input register storage (8 registers)
uint16_t inputRegisters[8] = {0};
// Holding register storage (13 registers)
uint16_t holdingRegisters[13] = {0};

void setup() {
    Serial.begin(115200);
    WiFi.begin(WIFI_SSID, WIFI_PASSWORD);
    while (WiFi.status() != WL_CONNECTED) delay(500);

    // Register callbacks for Modbus function codes
    modbusServer.registerWorker(1, READ_INPUT_REGISTER, &handleReadInput);
    modbusServer.registerWorker(1, READ_HOLD_REGISTER, &handleReadHolding);
    modbusServer.registerWorker(1, WRITE_HOLD_REGISTER, &handleWriteHolding);
    modbusServer.registerWorker(1, WRITE_MULT_REGISTERS, &handleWriteMultiple);
    modbusServer.start(502, 4, 2000);  // port, max clients, timeout

    Serial.printf("Modbus TCP server on %s:502\n", WiFi.localIP().toString().c_str());
}

void loop() {
    // Update input registers with current sensor data
    updateInputRegisters();
    delay(100);
}

void updateInputRegisters() {
    // Encode bean_temp as float32 across registers 0-1
    float beanTemp = readBeanTemp();
    uint32_t bits = *(uint32_t*)&beanTemp;
    inputRegisters[0] = (bits >> 16) & 0xFFFF;  // High word
    inputRegisters[1] = bits & 0xFFFF;           // Low word

    // Encode env_temp as float32 across registers 2-3
    float envTemp = readEnvTemp();
    bits = *(uint32_t*)&envTemp;
    inputRegisters[2] = (bits >> 16) & 0xFFFF;
    inputRegisters[3] = bits & 0xFFFF;

    // Encode rate_of_rise as float32 across registers 4-5
    float ror = calculateRoR();
    bits = *(uint32_t*)&ror;
    inputRegisters[4] = (bits >> 16) & 0xFFFF;
    inputRegisters[5] = bits & 0xFFFF;

    // Single uint16 registers
    inputRegisters[6] = getHeaterPercent();  // whole %, 0-100
    inputRegisters[7] = getFanPercent();     // whole %, 0-100
}

ModbusMessage handleReadInput(ModbusMessage request) {
    uint16_t addr = 0, count = 0;
    request.get(2, addr);   // byte 0 is the server id, 1 the function code
    request.get(4, count);

    ModbusMessage response;
    if (addr + count > 8) {
        response.setError(request.getServerID(), request.getFunctionCode(),
                          ILLEGAL_DATA_ADDRESS);
        return response;
    }

    response.add(request.getServerID(), request.getFunctionCode(),
                 (uint8_t)(count * 2));
    for (uint16_t i = 0; i < count; i++) {
        response.add(inputRegisters[addr + i]);
    }
    return response;
}

// handleReadHolding, handleWriteHolding, handleWriteMultiple
// follow the same pattern for holding registers.
// On write: apply control changes (setpoint, fan %, heater %, mode, etc.) and answer with an
// echo of the request (return ECHO_RESPONSE), as the Modbus spec says.
// A complete, compiled sketch: examples/roasters/modbus-tcp/modbus-tcp.ino
```

## Serial Connection (TC4 / aArtisan protocol)

### Overview

The TC4 shield's `aArtisan` firmware defined the serial protocol most DIY roasters and Artisan's "TC4" device type speak. Roastwarden implements the master side, so any device that answers `READ` with a CSV line can be polled and controlled over a USB or UART link:

```
ESP32 <--USB CDC / UART--> Roastwarden Server <--HTTP/WS--> Dashboard
```

Configure a device connection with protocol `serial_tc4` (the dashboard: Roasters → Add roaster, or a roaster's Connections → Add):

| Field | Default | Description |
|---|---|---|
| `port` | | Serial device, e.g. `/dev/ttyUSB0`, `/dev/ttyACM0` (Linux) or `COM5` |
| `baud` | `115200` | Line speed |
| `poll_interval_ms` | `1000` | How often the server sends `READ` |
| `et_channel` | `1` | Physical TC4 input (1–4) behind logical channel 1, read as ET; `0` = no ET probe |
| `bt_channel` | `2` | Physical input behind logical channel 2, read as BT; `0` = no BT probe |
| `t3_channel` | `0` | Physical input behind logical channel 3, reported as the extra field `t3`; `0` = off |
| `t4_channel` | `0` | Physical input behind logical channel 4, reported as `t4`; `0` = off |
| `reply_format` | `fixed` | How the device lays out its answer to `READ`: `active` for stock aArtisan, `fixed` for Roastwarden's own firmware (below) |
| `fan_output` | `io3` | The output a fan command drives: `io3`, `ot2`, `dcfan` or `none` (below) |

The dashboard's form starts a new connection on `active` (stock aArtisan), shows the `CHAN` line it will send and the reply it expects, and **Test connection** prints the raw line the device sent next to how it was read (`answered READ with "21.50,182.25,165.10,45,60": BT 165.1, ET 182.3, heater 45.0, fan 60.0`), so a wrong format shows at once: a heater that reads as the setpoint, or a fan that reads as a temperature.

The server checks the config when it is saved, as the form does (a 400 says what is wrong): a port, a baud above 0, each channel 0 to 4, ET or BT on a channel, and each physical input behind one channel only. Saving, enabling, disabling or deleting the connection opens, reopens or closes the port at once.

When running in Docker, pass the device through (`devices: ["/dev/ttyUSB0:/dev/ttyUSB0"]`) and make sure the container user can open it (`group_add: [dialout]`).

### Protocol

Master → device, one command per line terminated with `\n`:

| Line | Meaning |
|---|---|
| `UNITS;C` | report temperatures in Celsius (sent once at open) |
| `CHAN;ijkl` | map logical channels 1–4 to physical inputs, `0` turns a channel off; built from `et_channel`, `bt_channel`, `t3_channel`, `t4_channel` (default `CHAN;1200`) and sent once at open |
| `READ` | request one sample |
| `OT1;<0-100>` | heater duty % |
| `OT2;<0-100>` | second output duty %; the fan when `fan_output` is `ot2` |
| `IO3;<0-100>` | fan % when `fan_output` is `io3` (the default) |
| `DCFAN;<0-100>` | fan % on IO3, rising at most 25 points a second, when `fan_output` is `dcfan` |
| `PID;ON` / `PID;OFF` | auto / manual mode |
| `PID;SV;<temp>` | setpoint |
| `PID;T;<kp>;<ki>;<kd>` | PID tunings |

Device → master: the answer to `READ` is one CSV line. Logical channel 1 is ET and channel 2 is BT, Artisan's convention (Artisan sends `CHAN;` + its ET channel + its BT channel); channels 3 and 4 are carried as the extra fields `t3` and `t4`, which reach the live readings (the telemetry WebSocket, `roast live --json`) and are kept with a roast when the roaster declares them as channels ([channels](channels.md)). Two layouts, set by `reply_format`:

**`active`: stock aArtisan.** aArtisan prints the ambient temperature, then **only the logical channels `CHAN` turned on**, in channel order, then the heater and fan duty, then the setpoint while its PID runs:

```
ambient,<active channels…>,heater,fan[,sv]
21.50,182.25,165.10,45,60              CHAN;1200, PID off
21.50,182.25,165.10,73,60,200.00       CHAN;1200, PID on
22.00,190.00,170.50,120.25,50,70       CHAN;1230: ET, BT, T3
21.00,150.50,30,100                    CHAN;0100: BT only
```

aArtisanQ_PID prints the setpoint always, `0` while its PID is off; Roastwarden reads a setpoint of 0 or below as none. The heater and fan columns are whatever the firmware calls `HEATER_DUTY` and `FAN_DUTY` (aArtisan: OT1 and IO3; aArtisanQ_PID's default build: OT1 and OT2), so they show what the board is doing whichever output the fan is on. A line with fewer columns than active channels is not a sample.

**`fixed`: Roastwarden's firmware.** All four logical channels are always printed, `-1` for none, then the outputs; this is what the example below and the mock device print, and how a connection with no `reply_format` is read:

```
ambient,T1,T2,T3,T4[,heater,fan,sv[,pid_on,heater_enable]]
23.5,180.2,165.8,-1,-1,42,60,170.0,1,1
```

Read the old way (fixed), a stock aArtisan reply puts its heater duty in T3, its fan in T4 and the setpoint in the heater column: that is the bug `active` exists for. In both layouts `pid_on` and `heater_enable` (0/1), after the setpoint, are a Roastwarden extension that lets the dashboard show the mode and heater state; Artisan ignores them. A temperature of `-1` or one that does not parse means no probe, and the field is left out. Everything the device prints that starts with `#` (command acknowledgements such as `# Active channels set to 1200`, boot banners) is ignored.

**Your own field names (serial).** A device that prints its own CSV, or °F, takes an `ingest`
on its connection ([ingest mapping](ingest-mapping.md)): `"ingest": {"units": "F"}` sends `UNITS;F` at start
instead of `UNITS;C` and converts the probes and setpoint (for a device that ignores `UNITS` and
prints °F); `"ingest": {"columns": ["ambient", "et", "bt", "heater", "fan"]}` reads the reply by
position (roles or extra names, `""` to skip a column) instead of `reply_format`'s layout, `-1`
still meaning no probe (an extra column is kept as printed). `commands` templates replace a
command's line (several lines split by `\n`), e.g. `"commands": {"drum": {"payload":
"OT2;{value}", "decimals": 0}}`: a TC4 has no standard drum or damper line, so without a
template those commands fail (501) naming the connection.

Command mapping: heater enable off becomes `PID;OFF` + `OT1;0`. Emergency stop becomes `PID;OFF`, `OT1;0` and the fan to full on its output: `OT2;0`, `IO3;100` (`io3`, as it always was); `OT2;100` (`ot2`, OT2 is never zeroed then); `OT2;0`, `DCFAN;100` (`dcfan`); `OT2;0` alone (`none`). With `fan_output` `none` a fan command is refused (the transport cannot carry it). Feedforward and the lease are not available over serial.


### ESP32 Example (Arduino)

This responder prints the `fixed` layout, so its connection takes `reply_format: fixed`.

```cpp
// Minimal aArtisan-compatible responder on the USB serial port.
void handleLine(String line) {
    line.trim();
    if (line == "READ") {
        Serial.printf("%.1f,%.2f,%.2f,-1,-1,%d,%d,%.1f,%d,%d\n",
                      ambientTemp(), readEnvTemp(), readBeanTemp(),
                      getHeaterDuty(), getFanDutyPercent(), getSetpoint(),
                      getControlMode(), isHeaterEnabled() ? 1 : 0);
    } else if (line.startsWith("OT1;")) {
        setHeaterDuty(line.substring(4).toInt());      Serial.println("# OT1 set");
    } else if (line.startsWith("IO3;")) {
        setFanDutyPercent(line.substring(4).toInt());  Serial.println("# IO3 set");
    } else if (line == "PID;ON") {
        setControlMode(1);                             Serial.println("# PID ON");
    } else if (line == "PID;OFF") {
        setControlMode(0);                             Serial.println("# PID OFF");
    } else if (line.startsWith("PID;SV;")) {
        setTargetTemp(line.substring(7).toFloat());    Serial.println("# SV set");
    } else if (line.startsWith("PID;T;")) {
        int a = line.indexOf(';', 6), b = line.indexOf(';', a + 1);   // PID;T;kp;ki;kd
        setPIDTunings(line.substring(6, a).toFloat(),
                      line.substring(a + 1, b).toFloat(),
                      line.substring(b + 1).toFloat());
        Serial.println("# tunings set");
    } else if (line.startsWith("CHAN;") || line.startsWith("UNITS;")) {
        Serial.println("# ok");                        // fixed mapping, Celsius only
    }
}

void loop() {
    static String buf;
    while (Serial.available()) {
        char c = Serial.read();
        if (c == '\n') { handleLine(buf); buf = ""; } else if (c != '\r') buf += c;
    }
    // ... sensor reads, control loop, MQTT as before
}
```

Because this is the protocol Artisan speaks, the same device also works in Artisan directly: Config → Device → TC4, the serial port, 115200 baud. Artisan reads ET and BT from columns 1 and 2 in either layout; its "+ArduinoTC4_56" extra device (heater and fan) expects the active layout, so with a fixed-layout device it would read T3 and T4 there.

A simulator is included for development: `python tools/mock-device/mock_tc4_device.py` creates a pseudo-terminal, prints its path, and answers the protocol with a simple thermal model in the fixed layout; `TC4_STOCK=1` makes it answer as stock aArtisan does (the active layout, SV only while its PID is on, TC3 and TC4 two slow extra probes).

## Authentication

The server has an owner key: `ROASTWARDEN_API_KEY`, or one it makes at its first start and keeps
in `owner.key` beside the database. Every control command and other change needs the owner key
or an API token; reads need one too unless the server runs with `ROASTWARDEN_READS=open`.
Authentication is off only on a server that listens on loopback alone, or with
`ROASTWARDEN_AUTH=off`. MQTT devices are unaffected: secure the broker itself with credentials
and ACLs (above). Devices using the inbound WebSocket endpoint pass the owner key as described
above. Details: [security](../SECURITY.md).

## Minimal Implementation Checklist

The full list, required and optional, is the device contract in [control](control.md). Regardless of which protocol you choose, your device firmware must:

1. **Read sensors** — Sample bean and environment thermocouples at >= 1 Hz
2. **Report telemetry** — Send all required fields at 1 Hz (see Field Reference above); the server computes RoR itself
3. **Accept control commands** — Handle fan %, heater %, heater enable and emergency stop; with an onboard PID also setpoint, mode and PID gains (kept in RAM: the server sends them again after a reboot)
4. **Implement safety** — over-temperature cutoffs, sensor checks, no heat without airflow, emergency stop, a comms watchdog
5. **Leave the roast to the server** — no onboard autotune, RoR limiter or stored gains to fight its control

### Protocol-specific steps:

**MQTT:**
- Connect to broker and publish to `roaster/{device_id}/telemetry`
- Subscribe to `roaster/{device_id}/control/#` for control commands
- Publish status to `roaster/{device_id}/status` with retained flag

**WebSocket:**
- Register device in Roastwarden dashboard first (status must be `active`)
- Connect to `ws://<server>:8080/ws/device/{device_id}/telemetry`
- Send telemetry as JSON text frames
- Parse incoming `{"type":"control","topic":"...","payload":"..."}` messages

**Modbus TCP:**
- Run a Modbus TCP slave on port 502 (or configure alternate port in Roastwarden)
- Populate input registers with sensor data using the register map above
- Handle holding register writes for control commands
- Configure the device connection in the Roastwarden dashboard with host, port, and unit ID

**Modbus RTU:**
- Set the controller's slave address, baud, parity and stop bits, and note them
- Wire it to an RS-485 adapter on the server
- Add a Modbus RTU connection with the same settings and the controller's register map (PV, SV, output; its manual lists the addresses)

<!-- site: end -->

## Design notes

For developers: Roastwarden's own fluid-bed firmware, the history behind some fields, and the
sources for the TC4 protocol. The site publishes only the part above.

- **The fluid-bed firmware** (`Esp32_FluidBedCoffeeRoaster`, a separate repository) follows the
  controller heartbeat (`SAFETY_MQTT_TIMEOUT_MS`, default 60 s, unless a serial master is
  active), prints the TC4 `fixed` layout in `src/serial_handler.cpp` alongside MQTT, and mutes
  its debug chatter while a serial master is talking. `stopCode` came with its firmware 3.3.2;
  code 11 (onboard autotune ended) is from before 3.5.0. The old example device id was
  `esp32_roaster_01`, its status `version` `2.0.0-mqtt-only`.
- **The heartbeat's old topic.** Before 0.25.0 (as rustRoast) the topic was
  `rustroast/server/status`; servers 0.25.x announced both, and from 0.26.0 only the new one
  (`DEPLOY.md`, "Upgrading from rustRoast").
- **TC4 connections saved before** `t3_channel`, `t4_channel`, `reply_format` and `fan_output`
  existed have none of them and work exactly as before: `CHAN;<et><bt>00`, fixed replies, the
  fan on `IO3`.
- **TC4 channels 3 and 4** are live only unless declared as channels, which keep them with a
  roast ([`channels.md`](channels.md)).
- **Authentication**, as this guide had it before the owner key was always made: set
  `ROASTWARDEN_API_KEY` to require a key for every change; reads stayed open, and the
  dashboard stored the key in the browser. Today a browser signs in for a cookie
  (`SECURITY.md`, "Logins").
- Sources for the stock behaviour: aArtisan 3.11 (`applications/Artisan/aArtisan/trunk/src/aArtisan/`: `aArtisan.ino`'s `logger()`, `user.h`'s `HEATER_DUTY levelOT1` and `FAN_DUTY levelIO3`, `commands.txt` for `CHAN`, `READ`, `OT1`, `OT2`, `IO3` and `DCFAN`, `cmndreader.cpp`'s `CHAN` handler: a digit above 4 turns the channel off) and aArtisanQ_PID 6.8 (`applications/Artisan/aArtisan_PID/trunk/src/aArtisanQ_PID/`: `logger()` always appends SV, `user.h`'s `CONFIG_PAC2` fan on OT2), both in github.com/greencardigan/TC4-shield; Artisan's `ARDUINOTC4temperature` in `src/artisanlib/comm.py` (github.com/artisan-roaster-scope/artisan), which sends `CHAN;<ET><BT>00` and reads heater and fan at columns 3 and 4 with two channels, 5 and 6 with four.
