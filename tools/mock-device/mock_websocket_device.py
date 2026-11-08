#!/usr/bin/env python3
"""Mock WebSocket device that streams coffee roaster telemetry.

Simulates a coffee roaster sending real-time telemetry over WebSocket.
Each connected client receives JSON telemetry messages at 1 Hz, matching
the same payload format as the MQTT mock device.

The Roastwarden connection test handler expects:
  - WebSocket handshake succeeds
  - At least one message is received after connection

Usage:
  python mock_websocket_device.py
  WS_HOST=0.0.0.0 WS_PORT=8765 python mock_websocket_device.py
"""

import asyncio
import json
import math
import os
import random
import time

import websockets

HOST = os.environ.get("WS_HOST", "0.0.0.0")
PORT = int(os.environ.get("WS_PORT", "8765"))
DEVICE_ID = os.environ.get("DEVICE_ID", "mock-ws-device-01")
PUBLISH_HZ = float(os.environ.get("PUBLISH_HZ", "1"))

# --- Roast simulation ---

ROAST_PROFILES = [
    {"name": "Light", "duration": 600, "fc_temp": 196, "drop_temp": 205},
    {"name": "Medium", "duration": 720, "fc_temp": 200, "drop_temp": 215},
    {"name": "Dark", "duration": 900, "fc_temp": 200, "drop_temp": 230},
]


class RoastSimulator:
    """Cycles through roast profiles generating realistic temperature curves."""

    def __init__(self):
        self.profile_idx = 0
        self.roast_start = time.time()
        self.prev_bean_temp = 22.0
        self.bean_temp = 22.0
        self.uptime_start = time.time()
        self.setpoint = 200.0
        self.fan = 70.0
        self.heater = 0.0
        self.control_mode = 1
        self.heater_enable = 1

    def current_profile(self):
        return ROAST_PROFILES[self.profile_idx % len(ROAST_PROFILES)]

    def tick(self) -> dict:
        profile = self.current_profile()
        elapsed = time.time() - self.roast_start
        fraction = min(elapsed / profile["duration"], 1.0)

        # Temperature sigmoid curve
        ambient = 22.0
        target = profile["drop_temp"]
        sigmoid = 1.0 / (1.0 + math.exp(-12 * (fraction - 0.35)))
        ideal_temp = ambient + (target - ambient) * sigmoid

        noise = random.gauss(0, 0.3)
        self.prev_bean_temp = self.bean_temp
        self.bean_temp = ideal_temp + noise
        env_temp = self.bean_temp + 15 + random.gauss(0, 0.5)

        dt = 1.0 / max(PUBLISH_HZ, 0.1)
        ror = (self.bean_temp - self.prev_bean_temp) / dt * 60.0

        # Heater bell curve
        self.heater = max(0.0, min(100.0, 100 * math.exp(-((fraction - 0.4) ** 2) / 0.08)))

        # Fan ramps up
        self.fan = max(0.0, min(100.0, 47 + 53 * fraction))

        # Cycle to next roast
        if elapsed > profile["duration"] + 30:
            self.profile_idx += 1
            self.roast_start = time.time()
            self.bean_temp = 22.0
            self.prev_bean_temp = 22.0
            print(f"[ws-mock] Starting roast: {self.current_profile()['name']}")

        return {
            "timestamp": int(time.time()),
            "beanTemp": round(self.bean_temp, 2),
            "envTemp": round(env_temp, 2),
            "rateOfRise": round(ror, 2),
            "heater": round(self.heater, 1),
            "fan": round(self.fan, 1),
            "setpoint": self.setpoint,
            "controlMode": self.control_mode,
            "heaterEnable": self.heater_enable,
            "uptime": int(time.time() - self.uptime_start),
            "Kp": 15.0,
            "Ki": 1.0,
            "Kd": 25.0,
            "freeHeap": random.randint(180000, 220000),
            "rssi": random.randint(-70, -40),
            "systemStatus": 0,
            "deviceId": DEVICE_ID,
        }


# Shared simulator so all clients see the same roast state
sim = RoastSimulator()
connected_clients: set = set()


async def telemetry_handler(websocket):
    """Handle a single WebSocket client connection."""
    remote = websocket.remote_address
    print(f"[ws-mock] Client connected: {remote[0]}:{remote[1]}")
    connected_clients.add(websocket)

    try:
        # Send initial message immediately (required for connection test)
        payload = sim.tick()
        await websocket.send(json.dumps(payload))

        # Then stream at configured rate
        while True:
            await asyncio.sleep(1.0 / PUBLISH_HZ)
            payload = sim.tick()
            await websocket.send(json.dumps(payload))
    except websockets.exceptions.ConnectionClosed:
        pass
    finally:
        connected_clients.discard(websocket)
        print(f"[ws-mock] Client disconnected: {remote[0]}:{remote[1]}")


async def main():
    print(f"[ws-mock] Coffee roaster WebSocket simulator")
    print(f"[ws-mock] Device ID: {DEVICE_ID}")
    print(f"[ws-mock] Listening on ws://{HOST}:{PORT}")
    print(f"[ws-mock] Publishing at {PUBLISH_HZ} Hz")
    print(f"[ws-mock] Starting roast: {sim.current_profile()['name']}")

    async with websockets.serve(telemetry_handler, HOST, PORT):
        await asyncio.Future()  # run forever


if __name__ == "__main__":
    asyncio.run(main())
