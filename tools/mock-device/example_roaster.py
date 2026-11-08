#!/usr/bin/env python3
"""The example roasters of docs/connect-your-roaster.md, without the hardware.

Each mode speaks exactly what its sketch in examples/roasters/ speaks, over the same
transport, with a small thermal model behind it (the heater warms BT and ET, air cools them,
no heat below 30 % fan), so a builder can add it to a local Roastwarden with that example's
roaster.json, watch the check pass and drive it from the dashboard:

  mqtt       roaster/<id>/telemetry with Roastwarden's own names; commands on roaster/<id>/control/#
             (needs a broker: MQTT_BROKER_HOST, MQTT_BROKER_PORT, MQTT_USERNAME, MQTT_PASSWORD)
  modbus     a Modbus TCP slave, holding registers 0-6 as modbus-tcp.ino has them (MODBUS_PORT, 5020)
  tc4        stock aArtisan lines on a pseudo-terminal; prints its path (TC4_LINK: a symlink to it)
  websocket  a WebSocket server pushing {"BT","ET","heater","fan"} and taking {"set": ...} (WS_PORT, 8081)

Usage:
  python example_roaster.py modbus
  DEVICE_ID=mqtt_roaster python example_roaster.py mqtt

Needs: paho-mqtt (mqtt), pymodbus 3.10 (modbus), websockets (websocket); requirements.txt.
"""

import json
import os
import sys
import threading
import time

AMBIENT = 21.0
FAN_MIN_PCT = 30.0
BT_MAX, ET_MAX = 250.0, 300.0


class Roaster:
    """The sketches' state and safety, and a first-order thermal model behind them."""

    def __init__(self) -> None:
        self.bt = AMBIENT + 0.5
        self.et = AMBIENT + 1.0
        self.heater = 0.0
        self.fan = 0.0
        self.enabled = True
        self.tripped = False
        self.lock = threading.Lock()
        self.last = time.time()

    def applied(self) -> float:
        if not self.enabled or self.tripped or self.fan < FAN_MIN_PCT:
            return 0.0
        return self.heater

    def step(self) -> None:
        with self.lock:
            now = time.time()
            dt, self.last = now - self.last, now
            # ET follows the heater and the air; BT follows ET.
            et_target = AMBIENT + 1.0 + 2.6 * self.applied() * (1.3 - 0.5 * self.fan / 100)
            self.et += (et_target - self.et) * min(1.0, dt / 20)
            bt_target = AMBIENT + 0.5 + 0.85 * (self.et - AMBIENT - 1.0)
            self.bt += (bt_target - self.bt) * min(1.0, dt / 60)
            if self.bt > BT_MAX or self.et > ET_MAX:
                self.tripped = True
                self.stop()

    def stop(self) -> None:
        self.heater = 0.0
        self.fan = 100.0

    def set(self, what: str, value: float) -> None:
        with self.lock:
            value = max(0.0, min(100.0, value))
            if what == "heater":
                self.heater = value
            elif what == "fan":
                self.fan = value
            elif what == "enable":
                self.enabled = value != 0
                if not self.enabled:
                    self.heater = 0.0
                elif self.bt < BT_MAX - 20 and self.et < ET_MAX - 20:
                    self.tripped = False
            elif what == "stop" and value:
                self.stop()
        print(f"[example] {what} {value:g} -> heater {self.heater:g} fan {self.fan:g} (bt {self.bt:.1f})", flush=True)


def ticking(r: Roaster) -> None:
    def run() -> None:
        while True:
            r.step()
            time.sleep(0.25)

    threading.Thread(target=run, daemon=True).start()


def mqtt(r: Roaster) -> None:
    import paho.mqtt.client as paho

    device = os.environ.get("DEVICE_ID", "mqtt_roaster")
    prefix = f"roaster/{device}"
    server_online = {"v": False}
    client = paho.Client(paho.CallbackAPIVersion.VERSION2, client_id=device)
    user = os.environ.get("MQTT_USERNAME", device)
    if os.environ.get("MQTT_PASSWORD"):
        client.username_pw_set(user, os.environ["MQTT_PASSWORD"])
    client.will_set(f"{prefix}/status", '{"status":"offline"}', qos=1, retain=True)

    def on_connect(c, _u, _f, _rc, _p=None):
        c.publish(f"{prefix}/status", json.dumps({"status": "online", "version": "example-1", "caps": []}), qos=1, retain=True)
        c.subscribe(f"{prefix}/control/#")
        c.subscribe("roastwarden/server/status")

    def on_message(_c, _u, msg):
        v = msg.payload.decode(errors="replace")
        if msg.topic == "roastwarden/server/status":
            server_online["v"] = '"online"' in v
            return
        cmd = msg.topic.rsplit("/", 1)[-1]
        if cmd in ("heater", "fan"):
            r.set(cmd, float(v or 0))
        elif cmd == "heater_enable":
            r.set("enable", 1.0 if v == "1" else 0.0)
        elif cmd == "emergency_stop":
            r.set("stop", 1.0)

    client.on_connect = on_connect
    client.on_message = on_message
    client.connect(os.environ.get("MQTT_BROKER_HOST", "localhost"), int(os.environ.get("MQTT_BROKER_PORT", "1883")))
    client.loop_start()
    print(f"[example] MQTT roaster {device} on {prefix}/telemetry", flush=True)
    while True:
        with r.lock:
            # Like the sketch: no heat while the server is gone.
            applied = r.applied() if server_online["v"] else 0.0
            body = {"beanTemp": round(r.bt, 2), "envTemp": round(r.et, 2), "heater": r.heater,
                    "heaterApplied": applied, "fan": r.fan, "heaterEnable": int(r.enabled)}
        client.publish(f"{prefix}/telemetry", json.dumps(body))
        time.sleep(1)


def modbus(r: Roaster) -> None:
    from pymodbus.datastore import ModbusDeviceContext, ModbusSequentialDataBlock, ModbusServerContext
    from pymodbus.server import StartTcpServer

    def tenths(c: float) -> int:
        return int(round(c * 10)) & 0xFFFF

    class Holding(ModbusSequentialDataBlock):
        """Holding registers 0-6 of modbus-tcp.ino, read from and written to the roaster.
        pymodbus 3.10 hands the block address + 1, and answers a write with what it reads
        back: the stop register (which reads 0) echoes the 1 written to it once, since a
        write's answer must echo the request (tokio-modbus checks it in debug builds)."""

        echo = None

        def getValues(self, address, count=1):
            with r.lock:
                regs = [tenths(r.bt), tenths(r.et), int(r.heater), int(r.fan), int(r.applied()), int(r.enabled), 0]
            if self.echo is not None:
                regs[6], self.echo = self.echo, None
            return [regs[a] if 0 <= a < len(regs) else 0 for a in range(address - 1, address - 1 + count)]

        def setValues(self, address, values):
            for i, v in enumerate(values if isinstance(values, list) else [values]):
                what = {2: "heater", 3: "fan", 5: "enable", 6: "stop"}.get(address - 1 + i)
                if what == "stop":
                    self.echo = int(v)
                if what:
                    r.set(what, float(v))

    port = int(os.environ.get("MODBUS_PORT", "5020"))
    device = ModbusDeviceContext(hr=Holding(0, [0] * 7))
    print(f"[example] Modbus TCP roaster on 0.0.0.0:{port}, unit 1, holding registers 0-6", flush=True)
    StartTcpServer(context=ModbusServerContext(devices={1: device}, single=False), address=("0.0.0.0", port))


def tc4(r: Roaster) -> None:
    import select

    master, slave = os.openpty()
    path = os.ttyname(slave)
    link = os.environ.get("TC4_LINK")
    if link:
        if os.path.lexists(link):
            os.unlink(link)
        os.symlink(path, link)
    print(f"[example] TC4 roaster at {path}" + (f" (link {link})" if link else ""), flush=True)
    buf = b""
    while True:
        if not select.select([master], [], [], 0.5)[0]:
            continue
        buf += os.read(master, 1024)
        while b"\n" in buf:
            raw, buf = buf.split(b"\n", 1)
            line = raw.decode(errors="replace").strip().upper()
            if not line:
                continue
            if line == "READ":
                with r.lock:
                    reply = f"{AMBIENT:.2f},{r.et:.2f},{r.bt:.2f},{int(r.heater)},{int(r.fan)}"
            elif line.startswith("OT1;"):
                r.set("heater", float(line[4:] or 0))
                reply = "# OT1"
            elif line.startswith("IO3;"):
                r.set("fan", float(line[4:] or 0))
                reply = "# IO3"
            elif line.startswith("CHAN;"):
                reply = "# Active channels set to 1200"
            elif line.startswith("UNITS;"):
                reply = "# Units C"
            else:
                reply = "# ignored"
            os.write(master, (reply + "\n").encode())


def websocket(r: Roaster) -> None:
    import asyncio
    import websockets

    port = int(os.environ.get("WS_PORT", "8081"))

    async def client(ws):
        async def push():
            while True:
                with r.lock:
                    frame = {"BT": round(r.bt, 2), "ET": round(r.et, 2), "heater": r.heater, "fan": r.fan}
                await ws.send(json.dumps(frame))
                await asyncio.sleep(1)

        pusher = asyncio.create_task(push())
        try:
            async for text in ws:
                try:
                    msg = json.loads(text)
                except ValueError:
                    continue
                what = msg.get("set")
                if what in ("heater", "fan", "enable"):
                    r.set(what, float(msg.get("value", 0)))
                elif what == "stop":
                    r.set("stop", 1.0)
        finally:
            pusher.cancel()

    async def main():
        async with websockets.serve(client, "0.0.0.0", port):
            print(f"[example] WebSocket roaster on ws://0.0.0.0:{port}/", flush=True)
            await asyncio.Future()

    asyncio.run(main())


MODES = {"mqtt": mqtt, "modbus": modbus, "tc4": tc4, "websocket": websocket}

if __name__ == "__main__":
    if len(sys.argv) != 2 or sys.argv[1] not in MODES:
        print(__doc__)
        sys.exit(2)
    roaster = Roaster()
    ticking(roaster)
    try:
        MODES[sys.argv[1]](roaster)
    except KeyboardInterrupt:
        sys.exit(0)
