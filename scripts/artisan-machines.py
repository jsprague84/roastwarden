#!/usr/bin/env python3
"""Artisan's machine setups, trimmed to what Roastwarden reads (docs/artisan-machines.md).

Artisan (https://github.com/artisan-roaster-scope/artisan, AGPL-3.0) ships a setup per
machine in src/includes/Machines/<brand>/<model>.aset (Qt INI). This writes one file,
crates/server/assets/artisan/machines.txt, that the server embeds: each setup's lines kept
verbatim, only the sections and keys the importer reads (the button palettes and curve
styles are most of each file and are dropped).

    scripts/artisan-machines.py                 # the pinned commit, fetched with git
    scripts/artisan-machines.py --from DIR      # an Artisan checkout already on disk
    scripts/artisan-machines.py --commit SHA    # move the pin (then update PINNED below)
"""

import argparse
import pathlib
import subprocess
import sys
import tempfile

PINNED = "5be39f640bced5e1e555d645f864daeae66ac11f"  # 2026-10-24
REPO = "https://github.com/artisan-roaster-scope/artisan.git"
OUT = pathlib.Path(__file__).resolve().parent.parent / "crates/server/assets/artisan/machines.txt"

# section -> the keys kept (None: every key)
KEEP = {
    "General": {"roastertype_setup", "roasterheating_setup", "roastersize_setup", "Delay"},
    "Device": {
        "id", "controlETpid", "readBTpid", "arduinoETChannel", "arduinoBTChannel",
        "arduinoATChannel",
        # Phidgets (docs/phidgets.md): thermocouple types, data rate, RTD, a remote server
        "phidget1048_types", "phidget1048_dataRate", "phidget1200_formula", "phidget1200_wire",
        "phidget1200_dataRate", "phidgetRemoteFlag", "phidgetServerID", "phidgetPort",
    },
    "Modbus": None,
    "SerialPort": {"baudrate", "bytesize", "comport", "parity", "stopbits", "timeout"},
    "WebSocket": None,
    "MQTT": None,
    "Sliders": {
        "slideractions", "slidercommands", "sliderfactors", "slideroffsets", "slidermin",
        "slidermax", "eventsliderunits", "slidervisibilities",
    },
    "events": {"etypes"},
    "ExtraDev": {
        "extradevices", "extraname1", "extraname2", "extramathexpression1",
        "extramathexpression2",
    },
    "ExtraEventButtons": {"extraeventsactions", "extraeventsactionstrings", "extraeventslabels"},
}


def trim(text: str) -> list[str]:
    out, section = [], None
    for line in text.replace("\r\n", "\n").replace("\r", "\n").split("\n"):
        s = line.strip()
        if s.startswith("[") and s.endswith("]"):
            section = s[1:-1]
            if section in KEEP:
                out.append(s)
            continue
        if section not in KEEP or "=" not in s or s.startswith(";"):
            continue
        keys = KEEP[section]
        if keys is None or s.split("=", 1)[0].strip() in keys:
            out.append(s)
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--from", dest="src", help="an Artisan checkout")
    ap.add_argument("--commit", default=PINNED)
    a = ap.parse_args()
    with tempfile.TemporaryDirectory() as tmp:
        if a.src:
            root = pathlib.Path(a.src)
            commit = subprocess.run(
                ["git", "-C", str(root), "rev-parse", "HEAD"], capture_output=True, text=True
            ).stdout.strip() or a.commit
        else:
            root = pathlib.Path(tmp) / "artisan"
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            git = ["git", "-C", str(root)]
            subprocess.run(git + ["remote", "add", "origin", REPO], check=True)
            subprocess.run(git + ["sparse-checkout", "set", "src/includes/Machines"], check=True)
            subprocess.run(
                git + ["fetch", "-q", "--depth", "1", "--filter=blob:none", "origin", a.commit],
                check=True,
            )
            subprocess.run(git + ["checkout", "-q", "FETCH_HEAD"], check=True)
            commit = a.commit
        machines = root / "src/includes/Machines"
        files = sorted(machines.rglob("*.aset"), key=lambda p: str(p.relative_to(machines)).lower())
        lines = [
            "; Artisan's machine setups (src/includes/Machines), trimmed to the sections Roastwarden",
            "; reads (docs/artisan-machines.md). Made by scripts/artisan-machines.py: do not edit.",
            f"; Source: {REPO.removesuffix('.git')} at commit {commit},",
            "; licensed under the GNU AGPL v3 (Artisan's LICENSE); Roastwarden is AGPL-3.0-or-later.",
        ]
        for f in files:
            key = str(f.relative_to(machines).with_suffix(""))
            lines.append(f"@@ {key}")
            lines.extend(trim(f.read_text(encoding="utf-8", errors="replace")))
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"{len(files)} machines, {OUT.stat().st_size // 1024} KB -> {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
