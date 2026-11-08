#!/usr/bin/env python3
"""Mock ESP32 coffee roaster that publishes realistic telemetry via MQTT.

Simulates three roast profiles cycling continuously:
  - Light roast (~10 min, FC at 196°C, drop at 205°C)
  - Medium roast (~12 min, FC at 200°C, drop at 215°C)
  - Dark roast (~15 min, FC at 200°C, SC at 224°C, drop at 230°C)
"""

import json
import math
import os
import random
import sys
import time

import paho.mqtt.client as mqtt

MQTT_HOST = os.environ.get("MQTT_BROKER_HOST", "localhost")
MQTT_PORT = int(os.environ.get("MQTT_BROKER_PORT", "1883"))
DEVICE_ID = os.environ.get("DEVICE_ID", "mock-device-01")
PUBLISH_HZ = float(os.environ.get("PUBLISH_HZ", "1"))
COOLDOWN_SECS = int(os.environ.get("COOLDOWN_SECS", "60"))

TELEMETRY_TOPIC = f"roaster/{DEVICE_ID}/telemetry"
STATUS_TOPIC = f"roaster/{DEVICE_ID}/status"

# Roast profiles: list of (time_secs, bean_temp_target)
PROFILES = {
    "light": {
        "name": "Light Roast",
        "charge_temp": 200,
        "points": [
            (0, 200), (30, 110), (60, 105),  # charge drop + turning point
            (120, 130), (180, 150), (240, 165),
            (300, 178), (360, 188), (420, 193),
            (480, 196), (540, 200), (600, 205),  # FC ~480s, drop ~600s
        ],
        "fc_time": 480,
        "drop_time": 600,
    },
    "medium": {
        "name": "Medium Roast",
        "charge_temp": 205,
        "points": [
            (0, 205), (30, 115), (60, 108),
            (120, 132), (180, 152), (240, 168),
            (300, 180), (360, 190), (420, 196),
            (480, 200), (540, 205), (600, 210),
            (660, 213), (720, 215),  # drop ~720s
        ],
        "fc_time": 480,
        "drop_time": 720,
    },
    "dark": {
        "name": "Dark Roast",
        "charge_temp": 210,
        "points": [
            (0, 210), (30, 118), (60, 110),
            (120, 135), (180, 155), (240, 170),
            (300, 182), (360, 192), (420, 198),
            (480, 200), (540, 208), (600, 215),
            (660, 220), (720, 224), (780, 227),
            (840, 229), (900, 230),  # SC ~720s, drop ~900s
        ],
        "fc_time": 480,
        "sc_time": 720,
        "drop_time": 900,
    },
}


def interpolate_temp(points: list, elapsed: float) -> float:
    """Linear interpolation between profile points."""
    if elapsed <= points[0][0]:
        return points[0][1]
    if elapsed >= points[-1][0]:
        return points[-1][1]
    for i in range(len(points) - 1):
        t0, temp0 = points[i]
        t1, temp1 = points[i + 1]
        if t0 <= elapsed <= t1:
            frac = (elapsed - t0) / (t1 - t0)
            return temp0 + frac * (temp1 - temp0)
    return points[-1][1]


def add_noise(value: float, amplitude: float = 0.5) -> float:
    """Add realistic sensor noise."""
    return value + random.gauss(0, amplitude)


def on_connect(client, userdata, flags, rc, properties=None):
    print(f"Connected to MQTT broker (rc={rc})")
    client.subscribe(f"roaster/{DEVICE_ID}/control/#")
    # Publish status
    status = {
        "status": "online",
        "id": DEVICE_ID,
        "ip": "192.168.1.100",
        "version": "mock-1.0.0",
        "rssi": -45,
        "freeHeap": 200000,
    }
    client.publish(STATUS_TOPIC, json.dumps(status), qos=0, retain=True)


def on_message(client, userdata, msg):
    print(f"Received control: {msg.topic} = {msg.payload.decode()}")


def run_roast(client: mqtt.Client, profile_name: str, profile: dict):
    """Simulate a single roast."""
    print(f"\n{'='*50}")
    print(f"Starting {profile['name']} roast")
    print(f"{'='*50}")

    points = profile["points"]
    drop_time = profile["drop_time"]
    start = time.time()
    prev_bean = profile["charge_temp"]
    heater = 80.0
    fan = 70.0
    setpoint = profile["charge_temp"]
    mode = 1  # auto

    elapsed = 0
    while elapsed < drop_time:
        elapsed = time.time() - start
        target_bean = interpolate_temp(points, elapsed)
        # Simulate actual bean temp with lag and noise
        bean_temp = add_noise(target_bean, 0.3)
        env_temp = add_noise(bean_temp + 15 + random.gauss(0, 2), 0.5)

        # RoR (rate of rise in °C/min)
        ror = (bean_temp - prev_bean) * 60 * PUBLISH_HZ
        prev_bean = bean_temp

        # Simulate heater following PID-like behavior
        error = setpoint - bean_temp
        heater = max(0.0, min(100.0, 50 + error * 2))
        fan = 70.0 if elapsed < drop_time * 0.8 else 78.0

        # FC exotherm: reduce heater
        if elapsed > profile.get("fc_time", 9999):
            heater = max(0.0, heater - 15)

        telemetry = {
            "timestamp": int(time.time()),
            "beanTemp": round(bean_temp, 1),
            "envTemp": round(env_temp, 1),
            "rateOfRise": round(ror, 1),
            "heater": round(heater, 1),
            "fan": round(fan, 1),
            "setpoint": round(setpoint, 1),
            "controlMode": mode,
            "heaterEnable": 1,
            "uptime": int(elapsed),
            "Kp": 15.0,
            "Ki": 1.0,
            "Kd": 25.0,
            "freeHeap": 180000 + random.randint(-5000, 5000),
            "rssi": -45 + random.randint(-5, 5),
            "systemStatus": 0,
        }

        client.publish(TELEMETRY_TOPIC, json.dumps(telemetry), qos=0)

        if int(elapsed) % 60 == 0:
            print(
                f"  {int(elapsed)}s: BT={bean_temp:.1f}°C "
                f"ET={env_temp:.1f}°C RoR={ror:.1f} "
                f"Heater={heater:.0f}% Fan={fan:.0f}%"
            )

        time.sleep(1.0 / PUBLISH_HZ)

    print(f"Roast complete: {profile['name']} ({int(elapsed)}s)")


def cooldown(client: mqtt.Client, duration: int):
    """Simulate cooldown between roasts."""
    print(f"\nCooling down for {duration}s...")
    start = time.time()
    temp = 180.0
    while time.time() - start < duration:
        elapsed = time.time() - start
        temp = max(30, 180 * math.exp(-elapsed / 60))
        telemetry = {
            "timestamp": int(time.time()),
            "beanTemp": round(add_noise(temp, 0.5), 1),
            "envTemp": round(add_noise(temp + 5, 0.5), 1),
            "rateOfRise": round(-temp * 0.02, 1),
            "heater": 0,
            "fan": 100,
            "setpoint": 0,
            "controlMode": 0,
            "heaterEnable": 0,
            "uptime": int(elapsed),
            "Kp": 15.0,
            "Ki": 1.0,
            "Kd": 25.0,
            "freeHeap": 180000,
            "rssi": -45,
            "systemStatus": 0,
        }
        client.publish(TELEMETRY_TOPIC, json.dumps(telemetry), qos=0)
        time.sleep(1.0 / PUBLISH_HZ)


def main():
    client = mqtt.Client(
        client_id=f"mock-{DEVICE_ID}",
        callback_api_version=mqtt.CallbackAPIVersion.VERSION2,
    )
    client.on_connect = on_connect
    client.on_message = on_message

    print(f"Connecting to MQTT at {MQTT_HOST}:{MQTT_PORT} as {DEVICE_ID}")
    client.connect(MQTT_HOST, MQTT_PORT, keepalive=60)
    client.loop_start()

    time.sleep(1)  # Wait for connection

    profile_cycle = ["light", "medium", "dark"]
    idx = 0

    try:
        while True:
            name = profile_cycle[idx % len(profile_cycle)]
            run_roast(client, name, PROFILES[name])
            cooldown(client, COOLDOWN_SECS)
            idx += 1
    except KeyboardInterrupt:
        print("\nShutting down mock device")
    finally:
        client.loop_stop()
        client.disconnect()


if __name__ == "__main__":
    main()
