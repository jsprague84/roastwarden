#!/usr/bin/env python3
"""Mock Phidget22 network server with one temperature board (docs/phidgets.md).

Speaks what Phidgets' own network server (phidget22networkserver, libphidget22) speaks on
TCP 5661, closely enough that Phidgets' official JavaScript client (`phidget22` on npm)
connects, opens the board's TemperatureSensor channels, sets them and gets readings
(checked with that client). Point a Roastwarden Phidget22
connection at it (host 127.0.0.1, port 5661, the password).

What it does, as the real server does:
  - the handshake (protocol 2.4) and the password proof:
    base64(sha256("phidgetclient" + password + nonceC + nonceS + salt)); a wrong proof is
    answered with error 7 and the connection closed;
  - announces the board (a VINT board's HUB0000 first) and, to a client whose type starts
    with "www", every channel: thermocouple, IC, RTD, voltage and resistance inputs, with
    channel ids laid out as libphidget22's (serial, flags, class, port, index), so most are
    above 2^53;
  - answers an open with the channel's state (a set-status bridge packet, reply flag on;
    unknown values are 1e+300 as libphidget22 sends them), a set with {"E":0} or
    {"E":21,"R":"Value must be in range: ..."} ({"E":20} for a packet the input does not
    take), a close with {"E":0}; an open of an unknown device {"E":2,"R":...};
  - sends each open temperature input's reading at its data interval (bridge packet 95,
    "f":1), an error event (17, code 4103) for an input with no thermocouple or out of
    range, at most once a second;
  - sends a keepalive right after the announcements, then --keepalive seconds after each
    answer, and drops a client that has not answered within --keepalive seconds.

Readings: a simulated roast on ch0 (BT, room temperature rising toward ~210 °C over about
10 minutes) and ch1 (ET, above it), on thermocouple and RTD inputs; the board's own (IC)
sensor reads about 28 °C; the other inputs room temperature.

Usage:
  python3 tools/mock-device/mock_phidget22.py                       # a 1048, no password
  python3 tools/mock-device/mock_phidget22.py --password s3cret --board tmp1101 --hub-port 2
  python3 tools/mock-device/mock_phidget22.py --fixed 201.5,230      # ch0 201.5, ch1 230 °C
  python3 tools/mock-device/mock_phidget22.py --open-circuit 1       # ch1 has no thermocouple
  python3 tools/mock-device/mock_phidget22.py --roast-minutes 2 --noise 0.2 -v

Boards: 1048 (USB, 4 thermocouples ch0-ch3, IC ch4), tmp1101 (VINT, same inputs),
tmp1100 (VINT, thermocouple ch0, IC ch1), tmp1200 (VINT, RTD ch0, PT1000 4-wire default).
Only the Python standard library.
"""

import argparse
import asyncio
import base64
import hashlib
import json
import math
import os
import random
import struct
import sys
import time

MAGIC = 0x50484930
HEADER = struct.Struct("<IIHHHBB")  # magic, length, flags, reqseq, repseq, type, subtype

NRF_REQUEST, NRF_REPLY, NRF_EVENT = 0x1, 0x2, 0x4

MSG_CONNECT, MSG_COMMAND, MSG_DEVICE = 10, 20, 30
HANDSHAKE_C0, HANDSHAKE_S0 = 10, 11
AUTH_C0, AUTH_S0, AUTH_C1 = 30, 31, 32
REPLY, KEEPALIVE = 40, 41
ATTACH, OPEN, CLOSE, BRIDGE, CHANNEL = 50, 60, 65, 70, 80

# Phidget22 return codes
E_OK, E_NOENT, E_ACCESS, E_UNSUPPORTED, E_INVALIDARG = 0, 2, 7, 20, 21
E_NOTATTACHED, E_INVALIDPACKET = 52, 53
# error event codes
EE_OUTOFRANGE = 4103

# bridge packets
BP_SETSTATUS, BP_ERROREVENT = 0, 17
BP_SETCHANGETRIGGER, BP_SETDATAINTERVAL = 46, 54
BP_SETRTDTYPE, BP_SETRTDWIRESETUP, BP_SETTHERMOCOUPLETYPE = 76, 77, 84
BP_TEMPERATURECHANGE = 95

CLASS_HUB, CLASS_RESISTANCE, CLASS_TEMPERATURE, CLASS_VOLTAGE = 13, 23, 28, 29
CLASS_NAMES = {CLASS_HUB: "PhidgetHub", CLASS_RESISTANCE: "PhidgetResistanceInput",
               CLASS_TEMPERATURE: "PhidgetTemperatureSensor",
               CLASS_VOLTAGE: "PhidgetVoltageInput"}

UNKNOWN_DBL = 1e300       # libphidget22's "unknown" double
UNKNOWN_INT = 2147483647  # and enum

ROOM = 22.0
BOARD_IC = 28.0

# Each temperature input's limits, as Phidgets' libraries give them per channel.
TC_1048 = dict(min_di=32, max_di=60000, di=256, min_t=-270.0, max_t=1370.0, max_trig=1000.0,
               packets={46, 54, 84, 95}, tc=2)
IC_1048 = dict(min_di=32, max_di=60000, di=256, min_t=-55.0, max_t=150.0, max_trig=100.0,
               packets={46, 54, 95})
TC_VINT = dict(min_di=20, max_di=60000, di=250, min_t=-270.0, max_t=1370.0, max_trig=1640.0,
               packets={46, 54, 84, 95}, tc=2)
IC_VINT = dict(min_di=300, max_di=60000, di=500, min_t=-40.0, max_t=85.0, max_trig=125.0,
               packets={46, 54, 95})
RTD_VINT = dict(min_di=250, max_di=60000, di=250, min_t=-200.0, max_t=850.0, max_trig=1050.0,
                packets={46, 54, 76, 77, 95, 180}, rtd=2, wires=3)

# Channel groups: (name, class, count, first class index, limits or None)
BOARDS = {
    "1048": dict(type="USB", name="PhidgetTemperatureSensor 4-Input", sku="1048",
                 deviceID=26, productID=50, vintID=0, version=200, channels=[
                     ("Thermocouple Input", CLASS_TEMPERATURE, 4, 0, TC_1048),
                     ("Temperature Sensor (IC)", CLASS_TEMPERATURE, 1, 4, IC_1048),
                     ("Voltage Input", CLASS_VOLTAGE, 4, 0, None)]),
    "tmp1101": dict(type="VINT", name="4x Thermocouple Phidget", sku="TMP1101",
                    deviceID=89, productID=0, vintID=143, version=200, channels=[
                        ("Thermocouple Input", CLASS_TEMPERATURE, 4, 0, TC_VINT),
                        ("Temperature Sensor (IC)", CLASS_TEMPERATURE, 1, 4, IC_VINT),
                        ("Voltage Input", CLASS_VOLTAGE, 4, 0, None)]),
    "tmp1100": dict(type="VINT", name="Isolated Thermocouple Phidget", sku="TMP1100",
                    deviceID=88, productID=0, vintID=55, version=100, channels=[
                        ("Thermocouple Input", CLASS_TEMPERATURE, 1, 0, TC_VINT),
                        ("Temperature Sensor (IC)", CLASS_TEMPERATURE, 1, 1, IC_VINT),
                        ("Voltage Input", CLASS_VOLTAGE, 1, 0, None)]),
    "tmp1200": dict(type="VINT", name="RTD Phidget", sku="TMP1200",
                    deviceID=90, productID=0, vintID=16, version=100, channels=[
                        ("RTD input", CLASS_TEMPERATURE, 1, 0, RTD_VINT),
                        ("Resistance Input", CLASS_RESISTANCE, 1, 0, None)]),
}
HUB0000 = dict(type="USB", name="6-Port USB VINT Hub Phidget", sku="HUB0000", deviceID=64,
               productID=63, vintID=0, version=300,
               channels=[("VINT Hub", CLASS_HUB, 1, 0, None)])


def log(*a):
    print(time.strftime("%H:%M:%S"), *a, file=sys.stderr, flush=True)


def g(x):
    """A double as libphidget22 prints one (%.17g; nan for not a number)."""
    if isinstance(x, float) and math.isnan(x):
        return "nan"
    return format(float(x), ".17g")


def nonce():
    # libphidget22 keeps 15 base64 characters of 16 random bytes
    return base64.b64encode(os.urandom(16)).decode()[:15]


def proof(password, nonce_c, nonce_s, salt):
    text = "phidgetclient" + password + nonce_c + nonce_s + salt
    return base64.b64encode(hashlib.sha256(text.encode()).digest()).decode()


def channel_id(unique, cls, serial, vint, port):
    # serial (32 bits), flags, class, port, index: libphidget22's layout, little-endian
    flags = 0x02 if vint else 0
    return serial | flags << 32 | cls << 40 | (port if vint else 0) << 48 | unique << 56


class Channel:
    def __init__(self, dev, unique, index, name, cls, limits):
        self.dev, self.unique, self.index, self.name, self.cls = dev, unique, index, name, cls
        self.limits = limits
        self.chid = channel_id(unique, cls, dev.serial, dev.kind == "VINT", dev.hub_port)
        # bridge packets carry the channel's own handle as "I", not the device's phid
        self.handle = dev.phid + 0x7F3000 + unique * 0x2C0

    def is_temp(self):
        return self.cls == CLASS_TEMPERATURE

    def is_sensor_ic(self):
        return "(IC)" in self.name


class Device:
    def __init__(self, spec, serial, hub_port, phid, parent):
        self.spec, self.serial, self.hub_port = spec, serial, hub_port
        self.kind, self.phid, self.parent = spec["type"], phid, parent
        self.channels = []
        unique = 0
        for name, cls, count, first, limits in spec["channels"]:
            for k in range(count):
                self.channels.append(Channel(self, unique, first + k, name, cls, limits))
                unique += 1

    def attach_json(self):
        s = self.spec
        d = {"type": self.kind, "phid": self.phid, "parent": self.parent.phid if self.parent else 0,
             "vendorID": 1730 if self.kind == "USB" else 0, "productID": s["productID"],
             "interfaceNum": 0, "version": s["version"], "serialNumber": self.serial,
             "label": "", "index": self.hub_port if self.kind == "VINT" else 0,
             "deviceID": s["deviceID"], "vintID": s["vintID"],
             "hubPort": self.hub_port if self.kind == "VINT" else 0, "isHubPort": 0,
             "name": s["name"], "desc": s["name"], "fwstr": s["sku"]}
        if s is HUB0000:
            d["hubPortsInfo"] = {"portProto": [1] * 6, "portSuppSetSpeed": [0] * 6,
                                 "portMaxSpeed": [100000] * 6}
        if self.kind == "VINT":
            d.update(vintProto=1, suppSetSpeed=0, maxSpeed=100000, commSpeed=100000)
        return d


class Readings:
    """What each temperature input reads at a moment."""

    def __init__(self, args):
        self.start = time.monotonic()
        self.fixed = None
        if args.fixed:
            bt, et = (float(v) for v in args.fixed.split(","))
            self.fixed = {0: bt, 1: et}
        self.minutes = args.roast_minutes
        self.noise = args.noise

    def value(self, ch):
        if ch.is_sensor_ic():
            return BOARD_IC + random.gauss(0, 0.05)
        if self.fixed is not None and ch.index in self.fixed:
            return self.fixed[ch.index]
        t = (time.monotonic() - self.start) * 10.0 / self.minutes  # in "roast minutes"
        if ch.index == 0:
            v = ROOM + (212.0 - ROOM) * (1 - math.exp(-t / 3.3))
        elif ch.index == 1:
            v = ROOM + (262.0 - ROOM) * (1 - math.exp(-t / 1.5))
        else:
            v = ROOM
        return v + (random.gauss(0, self.noise) if self.noise > 0 else 0.0)


class OpenChannel:
    def __init__(self, ch):
        self.ch = ch
        lim = ch.limits
        self.di = lim["di"]
        self.trigger = 0.0
        self.tc = lim.get("tc", UNKNOWN_INT)
        self.rtd = lim.get("rtd", UNKNOWN_INT)
        self.wires = lim.get("wires", UNKNOWN_INT)
        self.temperature = UNKNOWN_DBL
        self.next_at = time.monotonic() + self.di / 1000.0
        self.last_sent = None
        self.last_error = 0.0


class Session:
    def __init__(self, server, reader, writer):
        self.srv, self.r, self.w = server, reader, writer
        self.peer = writer.get_extra_info("peername")
        self.reqseq = 0
        self.open = {}  # unique index -> OpenChannel
        self.keepalive_due = None  # when the unanswered keepalive expires
        self.keepalive_seq = None
        self.answered = 0

    def vlog(self, *a):
        if self.srv.args.verbose:
            log(self.peer, *a)

    async def send(self, flags, repseq, kind, sub, body=""):
        if isinstance(body, dict):
            body = json.dumps(body, separators=(",", ":"))
        data = body.encode()
        self.reqseq = (self.reqseq + 1) & 0xFFFF
        self.w.write(HEADER.pack(MAGIC, len(data), flags, self.reqseq, repseq, kind, sub) + data)
        await self.w.drain()
        self.vlog(f"=> {kind}/{sub} f{flags} seq{self.reqseq} rep{repseq} {body[:300]!r}")
        return self.reqseq

    async def recv(self):
        head = await self.r.readexactly(HEADER.size)
        magic, length, flags, reqseq, repseq, kind, sub = HEADER.unpack(head)
        if magic != MAGIC:
            raise ConnectionError(f"bad magic {magic:#x}")
        data = await self.r.readexactly(length) if length else b""
        text = data.decode("utf-8", "replace")
        self.vlog(f"<= {kind}/{sub} f{flags} seq{reqseq} rep{repseq} {text[:300]!r}")
        body = json.loads(text) if text.strip() else None
        return flags, reqseq, repseq, kind, sub, body

    async def reply(self, repseq, code, text=None):
        body = {"E": code} if text is None else {"E": code, "R": text}
        await self.send(NRF_REPLY, repseq, MSG_COMMAND, REPLY, body)

    def bridge_json(self, flags, packet, ch, entries):
        # rendered as libphidget22 renders a bridge packet (newlines and all)
        parts = []
        for name, (t, v) in entries.items():
            val = json.dumps(v) if t == "s" else (g(v) if t == "g" else str(int(v)))
            parts.append(f'"{name}":{{"t":"{t}","v":{val}}}')
        return (f'{{"v":0,"s":1,"f":{flags},"p":{packet},"I":{ch.handle},"O":{ch.chid},'
                f'"X":{ch.unique},"c":{len(entries)},"e":{{\n' + ",\n".join(parts) + "\n}}")

    async def handshake(self):
        args = self.srv.args
        _, seq, _, kind, sub, body = await self.recv()
        if (kind, sub) != (MSG_CONNECT, HANDSHAKE_C0):
            raise ConnectionError(f"expected a handshake, got {kind}/{sub}")
        ctype = str(body.get("type", ""))
        if body.get("pmajor") != 2:
            await self.send(NRF_REQUEST, 0, MSG_CONNECT, HANDSHAKE_S0,
                            {"type": "phid22device", "pmajor": 2, "pminor": 4,
                             "result": E_UNSUPPORTED})
            raise ConnectionError(f"protocol {body.get('pmajor')}.{body.get('pminor')}")
        self.channels_wanted = ctype.startswith("www")
        await self.send(NRF_REQUEST, 0, MSG_CONNECT, HANDSHAKE_S0,
                        {"type": "phid22device", "pmajor": 2, "pminor": 4, "result": 0})
        _, seq, _, kind, sub, body = await self.recv()
        if (kind, sub) != (MSG_CONNECT, AUTH_C0) or body.get("ident") != "phidgetclient":
            raise ConnectionError("expected an auth C0 with ident phidgetclient")
        nonce_c = str(body.get("nonceC", ""))[:15]
        nonce_s, salt = nonce(), nonce()
        await self.send(NRF_REQUEST, 0, MSG_CONNECT, AUTH_S0,
                        {"srvname": "mock_phidget22", "result": 0, "nonceC": nonce_c,
                         "nonceS": nonce_s, "salt": salt, "count": 1})
        _, seq, _, kind, sub, body = await self.recv()
        if (kind, sub) != (MSG_CONNECT, AUTH_C1):
            raise ConnectionError(f"expected an auth C1, got {kind}/{sub}")
        if body.get("nonceC") != nonce_c or body.get("nonceS") != nonce_s:
            raise ConnectionError("the nonces came back changed")
        ok = body.get("proof") == proof(args.password, nonce_c, nonce_s, salt)
        await self.reply(seq, E_OK if ok else E_ACCESS)
        if not ok:
            raise PermissionError("wrong password")
        log(self.peer, f"logged in (type {ctype!r})")

    async def announce(self):
        for dev in self.srv.devices:
            await self.send(NRF_EVENT, 0, MSG_DEVICE, ATTACH, dev.attach_json())
            if not self.channels_wanted:
                continue
            for ch in dev.channels:
                await self.send(NRF_EVENT, 0, MSG_DEVICE, CHANNEL, {
                    "parent": dev.phid, "chid": ch.chid, "class": ch.cls,
                    "uniqueIndex": ch.unique, "index": ch.index, "version": 1,
                    "name": ch.name, "channelname": CLASS_NAMES[ch.cls]})

    def status(self, oc):
        lim = oc.ch.limits
        e = {"_class_version_": ("u", 1), "dataInterval": ("u", round(oc.di)),
             "minDataInterval": ("u", lim["min_di"]), "maxDataInterval": ("u", lim["max_di"]),
             "dataIntervalDbl": ("g", oc.di), "minDataRate": ("g", 1000.0 / lim["max_di"]),
             "maxDataRate": ("g", 1000.0 / lim["min_di"]), "RTDType": ("d", oc.rtd),
             "RTDWireSetup": ("d", oc.wires), "temperature": ("g", oc.temperature),
             "minTemperature": ("g", lim["min_t"]), "maxTemperature": ("g", lim["max_t"]),
             "temperatureChangeTrigger": ("g", oc.trigger),
             "minTemperatureChangeTrigger": ("g", 0.0),
             "maxTemperatureChangeTrigger": ("g", lim["max_trig"]),
             "thermocoupleType": ("d", oc.tc)}
        return self.bridge_json(0x04, BP_SETSTATUS, oc.ch, e)

    def find(self, phid, unique):
        for dev in self.srv.devices:
            if dev.phid == phid:
                for ch in dev.channels:
                    if ch.unique == unique:
                        return ch
        return None

    async def on_open(self, seq, body):
        ch = self.find(int(body.get("phid", -1)), int(body.get("index", -1)))
        if ch is None:
            return await self.reply(seq, E_NOENT, "Specified device ID was not found.")
        if int(body.get("class", -1)) != ch.cls:
            return await self.reply(seq, E_INVALIDARG, "Wrong channel class.")
        if not ch.is_temp():
            return await self.reply(seq, E_UNSUPPORTED, "The mock serves temperature inputs only.")
        if body.get("channel") not in (None, ch.chid):
            log(self.peer, f"open: channel {body.get('channel')} is not {ch.chid}")
        oc = self.open.get(ch.unique) or OpenChannel(ch)
        self.open[ch.unique] = oc
        log(self.peer, f"opened {ch.dev.spec['sku']} ch{ch.index} ({ch.name})")
        await self.send(NRF_REPLY, seq, MSG_DEVICE, BRIDGE, self.status(oc))

    async def on_close(self, seq, body):
        self.open.pop(int(body.get("index", -1)), None)
        await self.reply(seq, E_OK)

    async def on_set(self, seq, body):
        ch = self.find(int(body.get("I", -1)), int(body.get("X", -1)))
        oc = self.open.get(ch.unique) if ch else None
        if oc is None:
            return await self.reply(seq, E_NOTATTACHED, "The channel is not open.")
        p = body.get("p")
        entries = body.get("e") or {}
        first = entries.get("0", {}).get("v")
        lim = ch.limits
        if p not in lim["packets"]:
            return await self.reply(seq, E_UNSUPPORTED)
        if first is None or not isinstance(first, (int, float)):
            return await self.reply(seq, E_INVALIDPACKET, "Missing value.")
        if p == BP_SETDATAINTERVAL:
            if not lim["min_di"] <= first <= lim["max_di"]:
                return await self.reply(
                    seq, E_INVALIDARG,
                    f"Value must be in range: {lim['min_di']} - {lim['max_di']}.")
            oc.di = float(first)
        elif p == BP_SETCHANGETRIGGER:
            if not 0 <= first <= lim["max_trig"]:
                return await self.reply(
                    seq, E_INVALIDARG,
                    f"Value must be in range: {g(0.0)} - {g(lim['max_trig'])}.")
            oc.trigger = float(first)
        elif p == BP_SETTHERMOCOUPLETYPE:
            if first not in (1, 2, 3, 4):  # J K E T
                return await self.reply(seq, E_INVALIDARG, "Invalid thermocouple type.")
            oc.tc = int(first)
        elif p == BP_SETRTDTYPE:
            if first not in (1, 2, 3, 4):
                return await self.reply(seq, E_INVALIDARG, "Invalid RTD type.")
            oc.rtd = int(first)
        elif p == BP_SETRTDWIRESETUP:
            if first not in (1, 2, 3):
                return await self.reply(seq, E_INVALIDARG, "Invalid RTD wire setup.")
            oc.wires = int(first)
        log(self.peer, f"ch{ch.index}: set {p} = {first}")
        await self.reply(seq, E_OK)

    async def events(self):
        """Readings, error events and keepalives, until the client goes."""
        args = self.srv.args
        next_keepalive = time.monotonic()  # the first right after the login, as the real one
        while True:
            now = time.monotonic()
            # the next keepalive goes one interval after the last was answered
            if self.keepalive_due is None and self.answered_at is not None:
                next_keepalive = self.answered_at + args.keepalive
                self.answered_at = None
            if self.keepalive_due is None and now >= next_keepalive:
                self.keepalive_seq = await self.send(NRF_EVENT, 0, MSG_COMMAND, KEEPALIVE)
                self.keepalive_due = now + args.keepalive
            elif self.keepalive_due is not None and now > self.keepalive_due:
                log(self.peer, f"no answer to the keepalive in {args.keepalive} s: closing")
                self.w.close()
                return
            for oc in list(self.open.values()):
                if now < oc.next_at:
                    continue
                oc.next_at = now + oc.di / 1000.0
                await self.reading(oc, now)
            await asyncio.sleep(0.005)

    async def reading(self, oc, now):
        ch, lim = oc.ch, oc.ch.limits
        problem = None
        if ch.index in self.srv.args.open_circuit and not ch.is_sensor_ic():
            problem = ("Input reported an undefined temperature. Probably there is no "
                       "thermocouple plugged in.")
        else:
            v = self.srv.readings.value(ch)
            if v > lim["max_t"]:
                problem = (f"Input reported a temperature of {v:.4f}C which is above the "
                           f"maximum of {lim['max_t']:.4f}C.")
            elif v < lim["min_t"]:
                problem = (f"Input reported a temperature of {v:.4f}C which is below the "
                           f"minimum of {lim['min_t']:.4f}C.")
        if problem:
            oc.temperature = UNKNOWN_DBL
            if now - oc.last_error >= 1.0:
                oc.last_error = now
                await self.send(NRF_EVENT, 0, MSG_DEVICE, BRIDGE, self.bridge_json(
                    0x01, BP_ERROREVENT, ch, {"0": ("d", EE_OUTOFRANGE), "1": ("s", problem)}))
            return
        if oc.trigger > 0 and oc.last_sent is not None and abs(v - oc.last_sent) < oc.trigger:
            return
        oc.temperature = oc.last_sent = v
        await self.send(NRF_EVENT, 0, MSG_DEVICE, BRIDGE, self.bridge_json(
            0x01, BP_TEMPERATURECHANGE, ch, {"0": ("g", round(v, 4))}))

    async def run(self):
        self.answered_at = None
        try:
            await self.handshake()
            await self.announce()
            ev = asyncio.create_task(self.events())
            try:
                while True:
                    flags, seq, repseq, kind, sub, body = await self.recv()
                    if (kind, sub) == (MSG_COMMAND, KEEPALIVE) and flags & NRF_REPLY:
                        if repseq != self.keepalive_seq:
                            log(self.peer, f"keepalive answer for {repseq}, sent {self.keepalive_seq}")
                        self.keepalive_due = None
                        self.answered_at = time.monotonic()
                        self.answered += 1
                    elif (kind, sub) == (MSG_DEVICE, OPEN):
                        await self.on_open(seq, body or {})
                    elif (kind, sub) == (MSG_DEVICE, CLOSE):
                        await self.on_close(seq, body or {})
                    elif (kind, sub) == (MSG_DEVICE, BRIDGE):
                        await self.on_set(seq, body or {})
                    elif flags & NRF_REPLY:
                        pass
                    else:
                        log(self.peer, f"unexpected message {kind}/{sub}")
                        await self.reply(seq, E_UNSUPPORTED, f"unexpected {kind}/{sub}")
            finally:
                ev.cancel()
        except PermissionError as e:
            log(self.peer, f"refused: {e}")
        except (asyncio.IncompleteReadError, ConnectionError, OSError) as e:
            log(self.peer, f"gone ({type(e).__name__}: {e}); keepalives answered: {self.answered}")
        finally:
            self.w.close()


class Server:
    def __init__(self, args):
        self.args = args
        self.readings = Readings(args)
        spec = BOARDS[args.board]
        base = 0x55D0C3A10000  # ids look like the real server's (its pointers)
        self.devices = []
        if spec["type"] == "VINT":
            hub = Device(HUB0000, args.serial, 0, base, None)
            self.devices.append(hub)
            self.devices.append(Device(spec, args.serial, args.hub_port, base + 0x1A40, hub))
        else:
            self.devices.append(Device(spec, args.serial, 0, base, None))

    async def client(self, reader, writer):
        await Session(self, reader, writer).run()


def parse_args(argv=None):
    p = argparse.ArgumentParser(description="Mock Phidget22 network server (one temperature board).")
    p.add_argument("--host", default="0.0.0.0")
    p.add_argument("--port", type=int, default=5661)
    p.add_argument("--password", default="")
    p.add_argument("--board", choices=sorted(BOARDS), default="1048")
    p.add_argument("--serial", type=int, default=370001)
    p.add_argument("--hub-port", type=int, default=0, help="the VINT hub port (0-5)")
    p.add_argument("--keepalive", type=float, default=4.0, help="seconds (the real server: 4)")
    p.add_argument("--fixed", metavar="BT,ET", help="fixed ch0 and ch1 values, °C")
    p.add_argument("--open-circuit", metavar="N", type=int, action="append", default=[],
                   help="input N sends error events (no thermocouple); repeatable")
    p.add_argument("--roast-minutes", type=float, default=10.0,
                   help="how long the simulated roast takes to near 210 °C")
    p.add_argument("--noise", type=float, default=0.0, help="reading noise, °C (std dev)")
    p.add_argument("-v", "--verbose", action="store_true", help="log every message")
    a = p.parse_args(argv)
    if a.board != "1048" and a.board in BOARDS and not 0 <= a.hub_port <= 5:
        p.error("--hub-port is 0 to 5")
    return a


async def main(argv=None):
    args = parse_args(argv)
    srv = Server(args)
    server = await asyncio.start_server(srv.client, args.host, args.port)
    d = srv.devices[-1]
    where = f" on hub port {d.hub_port}" if d.kind == "VINT" else ""
    log(f"Phidget22 mock: {d.spec['sku']} #{d.serial}{where} on {args.host}:{args.port}, "
        f"password {'set' if args.password else 'none'}, keepalive {args.keepalive} s")
    async with server:
        await server.serve_forever()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        pass
