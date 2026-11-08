#!/usr/bin/env python3
"""Mock TC4 / aArtisan roaster on a pseudo-terminal.

Creates a pty pair and prints the device path. Point a Roastwarden `serial_tc4`
connection (or Artisan's TC4 device) at that path. Speaks the aArtisan line
protocol:

  master -> device: READ, CHAN;ijkl, UNITS;C|F, OT1;n, OT2;n, IO3;n, DCFAN;n,
                    PID;ON, PID;OFF, PID;SV;t, PID;T;p;i;d
  device -> master: READ answer "ambient,T1,T2,T3,T4,heater,fan,sv,pid_on,heater_en"
                    (reply_format "fixed"), or with TC4_STOCK=1 stock aArtisan's
                    "ambient,<active channels>,heater,fan[,sv]" (reply_format "active")
                    acknowledgements as "# ..." lines

A small first-order thermal model makes the bean temperature respond to the
heater duty, so PID;SV with PID;ON actually regulates.

Usage:
  python mock_tc4_device.py            # prints e.g. /dev/pts/7
  TC4_LINK=/tmp/tc4 python mock_tc4_device.py   # also symlinks the pty there
  TC4_STOCK=1 python mock_tc4_device.py          # stock aArtisan replies
"""

import os
import select
import sys
import time

AMBIENT = 22.0
LINK = os.environ.get("TC4_LINK")
# TC4_STOCK=1: answer READ as stock aArtisan does (reply_format "active").
STOCK = os.environ.get("TC4_STOCK", "") not in ("", "0")


class Roaster:
    def __init__(self) -> None:
        self.bt = AMBIENT
        self.et = AMBIENT
        self.heater = 0.0  # OT1 %
        self.fan = 50.0    # IO3 %
        self.pid_on = False
        self.sv = 0.0
        self.kp, self.ki, self.kd = 8.0, 0.2, 10.0
        self._integral = 0.0
        self._last_err = 0.0
        self.units = "C"
        self.chan = "1200"

    def step(self, dt: float) -> None:
        if self.pid_on:
            err = self.sv - self.bt
            self._integral = max(-50.0, min(50.0, self._integral + err * dt))
            deriv = (err - self._last_err) / dt if dt > 0 else 0.0
            self._last_err = err
            self.heater = max(0.0, min(100.0, self.kp * err + self.ki * self._integral + self.kd * deriv))
        # Heater drives ET; ET drives BT; fan cools a little.
        et_target = AMBIENT + 2.6 * self.heater - 0.2 * self.fan
        self.et += (et_target - self.et) * min(1.0, dt / 8.0)
        self.bt += (self.et - self.bt) * min(1.0, dt / 25.0)

    def temps(self) -> tuple[float, float]:
        if self.units == "F":
            return self.et * 9 / 5 + 32, self.bt * 9 / 5 + 32
        return self.et, self.bt

    def stock_reply(self, et: float, bt: float) -> str:
        """aArtisan 3.11's logger(): ambient, the active logical channels in CHAN order,
        heater (OT1), fan (IO3), and SV only while the PID runs."""
        # Physical inputs: TC1 = ET probe, TC2 = BT probe, TC3/TC4 two slow extra probes.
        physical = {"1": et, "2": bt, "3": (et + AMBIENT) / 2, "4": AMBIENT + 5.0}
        cols = [f"{AMBIENT:.2f}"]
        cols += [f"{physical[c]:.2f}" for c in self.chan if c in physical]
        cols += [f"{self.heater:.0f}", f"{self.fan:.0f}"]
        if self.pid_on:
            cols.append(f"{self.sv:.0f}")
        return ",".join(cols)

    def handle(self, line: str) -> list[str]:
        parts = line.strip().split(";")
        cmd = parts[0].upper()
        if cmd == "READ":
            et, bt = self.temps()
            if STOCK:
                return [self.stock_reply(et, bt)]
            # logical channel order follows CHAN; default 1200 => T1=ET, T2=BT
            if self.chan.startswith("21"):
                t1, t2 = bt, et
            else:
                t1, t2 = et, bt
            return [f"{AMBIENT:.1f},{t1:.2f},{t2:.2f},-1.00,-1.00,{self.heater:.0f},{self.fan:.0f},{self.sv:.1f},"
                    f"{1 if self.pid_on else 0},{1 if (self.pid_on or self.heater > 0) else 0}"]
        if cmd == "CHAN" and len(parts) > 1:
            self.chan = parts[1]
            return [f"# Active channels set to {self.chan}"]
        if cmd == "UNITS" and len(parts) > 1:
            self.units = parts[1].upper()[:1]
            return [f"# Physical units set to {self.units}"]
        if cmd == "OT1" and len(parts) > 1:
            self.pid_on = False
            self.heater = max(0.0, min(100.0, float(parts[1])))
            return [f"# OT1 = {self.heater:.0f}"]
        if cmd == "OT2" and len(parts) > 1:
            return [f"# OT2 = {float(parts[1]):.0f}"]
        if cmd in ("IO3", "DCFAN") and len(parts) > 1:
            self.fan = max(0.0, min(100.0, float(parts[1])))
            return [f"# IO3 = {self.fan:.0f}"]
        if cmd == "PID" and len(parts) > 1:
            sub = parts[1].upper()
            if sub == "ON":
                self.pid_on = True
                self._integral = 0.0
                return ["# PID turned ON"]
            if sub == "OFF":
                self.pid_on = False
                return ["# PID turned OFF"]
            if sub == "SV" and len(parts) > 2:
                self.sv = float(parts[2])
                return [f"# SV = {self.sv:.1f}"]
            if sub == "T" and len(parts) > 4:
                self.kp, self.ki, self.kd = float(parts[2]), float(parts[3]), float(parts[4])
                return [f"# PID tunings {self.kp},{self.ki},{self.kd}"]
        return [f"# Unknown command: {line.strip()}"]


def main() -> None:
    master, slave = os.openpty()
    path = os.ttyname(slave)
    if LINK:
        try:
            os.unlink(LINK)
        except FileNotFoundError:
            pass
        os.symlink(path, LINK)
    print(f"[tc4-mock] device at {path}" + (f" (link {LINK})" if LINK else ""), flush=True)

    roaster = Roaster()
    buf = b""
    last = time.time()
    while True:
        now = time.time()
        roaster.step(now - last)
        last = now
        r, _, _ = select.select([master], [], [], 0.25)
        if not r:
            continue
        try:
            data = os.read(master, 1024)
        except OSError:
            data = b""
        if not data:
            time.sleep(0.1)
            continue
        buf += data
        while b"\n" in buf:
            line, buf = buf.split(b"\n", 1)
            text = line.decode(errors="replace").strip("\r ")
            if not text:
                continue
            for reply in roaster.handle(text):
                os.write(master, (reply + "\n").encode())
            if text.upper() != "READ":
                print(f"[tc4-mock] {text} -> {roaster.handle.__name__} bt={roaster.bt:.1f} heater={roaster.heater:.0f} fan={roaster.fan:.0f} pid={'on' if roaster.pid_on else 'off'} sv={roaster.sv:.1f}", flush=True)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        sys.exit(0)
