#!/usr/bin/env python3
"""Mock Modbus TCP device that serves coffee roaster telemetry registers.

Simulates a coffee roaster exposing telemetry via Modbus TCP input registers
and accepting control commands via holding registers. Register layout matches
the Roastwarden Standard preset.

Input Registers (FC 04, read-only):
  0x0000-0x0001  bean_temp      float32  (IEEE 754 big-endian)
  0x0002-0x0003  env_temp       float32
  0x0004-0x0005  rate_of_rise   float32
  0x0006         heater         uint16   (0-100 %)
  0x0007         fan            uint16   (0-100 %)

Holding Registers (FC 03/06/16, read-write):
  0x0000-0x0001  setpoint       float32
  0x0002         fan_cmd        uint16   (0-100 %)
  0x0003         heater_cmd     uint16   (0-100 %)
  0x0004         control_mode   uint16   (0=manual, 1=auto)
  0x0005         heater_enable  uint16   (0=off, 1=on)
  0x000C         emergency_stop uint16   (write 1 to trigger)

Usage:
  python mock_modbus_device.py
  MODBUS_HOST=0.0.0.0 MODBUS_PORT=5020 python mock_modbus_device.py
"""

import math
import os
import random
import struct
import threading
import time

from pymodbus.server import StartTcpServer
from pymodbus.datastore import (
    ModbusSequentialDataBlock,
    ModbusDeviceContext,
    ModbusServerContext,
)
from pymodbus.server.server import ModbusDeviceIdentification

HOST = os.environ.get("MODBUS_HOST", "0.0.0.0")
PORT = int(os.environ.get("MODBUS_PORT", "5020"))
UNIT_ID = int(os.environ.get("MODBUS_UNIT_ID", "1"))
UPDATE_HZ = float(os.environ.get("UPDATE_HZ", "1"))

# --- Roast simulation ---

ROAST_PROFILES = [
    {"name": "Light", "duration": 600, "fc_temp": 196, "drop_temp": 205},
    {"name": "Medium", "duration": 720, "fc_temp": 200, "drop_temp": 215},
    {"name": "Dark", "duration": 900, "fc_temp": 200, "drop_temp": 230},
]


def float_to_regs(value: float) -> tuple[int, int]:
    """Convert a float to two 16-bit registers (IEEE 754, big-endian ABCD)."""
    packed = struct.pack(">f", value)
    hi = (packed[0] << 8) | packed[1]
    lo = (packed[2] << 8) | packed[3]
    return hi, lo


class RoastSimulator:
    """Cycles through roast profiles generating realistic temperature curves."""

    def __init__(self):
        self.profile_idx = 0
        self.roast_start = time.time()
        self.prev_bean_temp = 22.0
        self.bean_temp = 22.0
        self.env_temp = 37.0
        self.ror = 0.0
        self.heater = 0
        self.fan = 70
        self.setpoint = 200.0
        self.control_mode = 1  # auto
        self.heater_enable = 1

    def current_profile(self):
        return ROAST_PROFILES[self.profile_idx % len(ROAST_PROFILES)]

    def tick(self, hold_fan: bool = False):
        profile = self.current_profile()
        elapsed = time.time() - self.roast_start
        fraction = min(elapsed / profile["duration"], 1.0)

        # Ambient -> drop temp sigmoid curve
        ambient = 22.0
        target = profile["drop_temp"]
        sigmoid = 1.0 / (1.0 + math.exp(-12 * (fraction - 0.35)))
        ideal_temp = ambient + (target - ambient) * sigmoid

        # Add realistic noise
        noise = random.gauss(0, 0.3)
        self.prev_bean_temp = self.bean_temp
        self.bean_temp = ideal_temp + noise
        self.env_temp = self.bean_temp + 15 + random.gauss(0, 0.5)

        # Rate of rise (degrees per minute)
        dt = 1.0 / max(UPDATE_HZ, 0.1)
        self.ror = (self.bean_temp - self.prev_bean_temp) / dt * 60.0

        # Heater follows a bell curve peaking mid-roast
        self.heater = max(0, min(100, int(100 * math.exp(-((fraction - 0.4) ** 2) / 0.08))))

        # Fan ramps up during roast unless a master has commanded it
        if not hold_fan:
            self.fan = max(0, min(100, int(47 + 53 * fraction)))

        # Cycle to next roast after completion + cooldown
        if elapsed > profile["duration"] + 30:
            self.profile_idx += 1
            self.roast_start = time.time()
            self.bean_temp = 22.0
            self.prev_bean_temp = 22.0
            print(f"[modbus-mock] Starting roast: {self.current_profile()['name']}")


def regs_to_float(hi: int, lo: int) -> float:
    """Inverse of float_to_regs."""
    return struct.unpack(">f", struct.pack(">HH", hi, lo))[0]


def update_registers(context: ModbusServerContext, sim: RoastSimulator):
    """Background thread that updates Modbus registers from the simulator."""
    print(f"[modbus-mock] Register updater running at {UPDATE_HZ} Hz")
    print(f"[modbus-mock] Starting roast: {sim.current_profile()['name']}")

    last_written: list[int] | None = None
    fan_commanded = False

    while True:
        slave = context[UNIT_ID]

        # Honour holding-register writes from a master (Roastwarden poller or
        # Artisan): anything that differs from what we last wrote is a command.
        current = list(slave.getValues(3, 0, 13))
        if last_written is not None and current[:6] != last_written[:6]:
            sim.setpoint = regs_to_float(current[0], current[1])
            if current[2] != last_written[2]:
                sim.fan = max(0, min(100, current[2]))
                fan_commanded = True
            if current[3] != last_written[3]:
                sim.heater = max(0, min(100, current[3]))
            sim.control_mode = 1 if current[4] else 0
            sim.heater_enable = 1 if current[5] else 0
            print(f"[modbus-mock] command: setpoint={sim.setpoint:.1f} fan={sim.fan} "
                  f"heater={sim.heater} mode={sim.control_mode} enable={sim.heater_enable}")
        if current[12] == 1:
            print("[modbus-mock] EMERGENCY STOP received")
            sim.heater_enable = 0
            sim.heater = 0
            slave.setValues(3, 12, [0])

        sim.tick(hold_fan=fan_commanded)

        # Update input registers (FC 04, address block ir)
        bt_hi, bt_lo = float_to_regs(sim.bean_temp)
        et_hi, et_lo = float_to_regs(sim.env_temp)
        ror_hi, ror_lo = float_to_regs(sim.ror)

        # pymodbus uses 1-based addressing internally for setValues
        slave.setValues(4, 0, [bt_hi, bt_lo, et_hi, et_lo, ror_hi, ror_lo,
                                sim.heater, sim.fan])

        # Update holding registers with current control state
        sp_hi, sp_lo = float_to_regs(sim.setpoint)
        last_written = [sp_hi, sp_lo, sim.fan, sim.heater,
                        sim.control_mode, sim.heater_enable]
        slave.setValues(3, 0, last_written)

        time.sleep(1.0 / UPDATE_HZ)


def main():
    print(f"[modbus-mock] Coffee roaster Modbus TCP simulator")
    print(f"[modbus-mock] Listening on {HOST}:{PORT} (unit ID {UNIT_ID})")

    # Create data blocks: input registers and holding registers
    # 20 registers each is plenty for the Roastwarden register map
    ir_block = ModbusSequentialDataBlock(0, [0] * 20)
    hr_block = ModbusSequentialDataBlock(0, [0] * 20)

    slave = ModbusDeviceContext(
        di=ModbusSequentialDataBlock(0, [0] * 10),  # discrete inputs (unused)
        co=ModbusSequentialDataBlock(0, [0] * 10),  # coils (unused)
        ir=ir_block,
        hr=hr_block,
    )

    context = ModbusServerContext(devices={UNIT_ID: slave}, single=False)

    # Device identification
    identity = ModbusDeviceIdentification()
    identity.VendorName = "Roastwarden"
    identity.ProductCode = "MOCK-MODBUS"
    identity.ProductName = "Mock Coffee Roaster (Modbus TCP)"
    identity.ModelName = "MockRoaster-v1"

    # Start simulator in background
    sim = RoastSimulator()
    updater = threading.Thread(target=update_registers, args=(context, sim), daemon=True)
    updater.start()

    # Start Modbus TCP server (blocks)
    StartTcpServer(
        context=context,
        identity=identity,
        address=(HOST, PORT),
    )


if __name__ == "__main__":
    main()
