#!/usr/bin/env python3
"""Third-party notices for roastwarden-server and roast (docs/DEVELOPMENT.md, "Licences").

Reads `cargo about generate --format json` (about.toml) and writes one text file: every licence
with the crates under it, the NOTICE files of the crates that ship one (Apache-2.0 asks for
them), the fonts the server embeds for labels and reports (SIL OFL 1.1), and Artisan's machine
setups it embeds for adding a roaster (AGPL v3, docs/artisan-machines.md). scripts/notices.sh
runs it; the image serves the file at /licenses/server.txt, linked from Settings → About.

    python3 scripts/notices.py about.json out/server.txt
"""

import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
FONTS = ROOT / "crates/server/assets/fonts"
MACHINES = ROOT / "crates/server/assets/artisan/machines.txt"
RULE = "-" * 96


def notice_files(manifest_path: str) -> list[pathlib.Path]:
    folder = pathlib.Path(manifest_path).parent
    return sorted(
        p for p in folder.iterdir() if p.is_file() and p.name.upper().startswith("NOTICE")
    )


def main(about_json: str, out: str) -> None:
    about = json.loads(pathlib.Path(about_json).read_text())
    lines = [
        "Third-party notices: roastwarden-server and roast",
        "",
        "Roastwarden is free software under the GNU Affero General Public License, version 3 or",
        "any later version (LICENSE.txt next to this file). The server and the roast CLI are",
        "built with the Rust crates below, each under its own licence, and embed the fonts in",
        "part 3 and Artisan's machine setups in part 4. The dashboard's npm packages are in",
        "dashboard.txt.",
        "",
        "1. Licences, and the crates under each",
        "2. NOTICE files",
        "3. Fonts",
        "4. Artisan's machine setups",
        "",
        "Crates by licence:",
    ]
    lines += [f"  {o['name']}: {o['count']}" for o in about["overview"]]

    lines += ["", "", "1. LICENCES", "=" * 11]
    for lic in about["licenses"]:
        lines += ["", RULE, f"{lic['name']} ({lic['id']}), used by:"]
        for used in lic["used_by"]:
            c = used["crate"]
            repo = f" ({c['repository']})" if c.get("repository") else ""
            lines.append(f"  {c['name']} {c['version']}{repo}")
        lines += ["", lic["text"].rstrip()]

    lines += ["", "", "2. NOTICE FILES", "=" * 15]
    seen = set()
    for crate in sorted(about["crates"], key=lambda c: c["package"]["name"]):
        pkg = crate["package"]
        key = (pkg["name"], pkg["version"])
        if key in seen:
            continue
        seen.add(key)
        for path in notice_files(pkg["manifest_path"]):
            lines += ["", RULE, f"{pkg['name']} {pkg['version']}: {path.name}", ""]
            lines.append(path.read_text(errors="replace").rstrip())

    lines += [
        "",
        "",
        "3. FONTS",
        "=" * 8,
        "",
        "Labels and reports are set in these fonts, embedded in roastwarden-server. Each is under",
        "the SIL Open Font License 1.1; its licence follows.",
    ]
    for ofl in sorted(FONTS.glob("OFL-*.txt")):
        family = ofl.stem.removeprefix("OFL-")
        files = sorted(p.name for p in FONTS.glob(f"{family}*.ttf"))
        lines += ["", RULE, f"{family}: {', '.join(files)}", ""]
        lines.append(ofl.read_text(errors="replace").rstrip())

    source = next(
        (l.removeprefix("; Source: ").rstrip(",") for l in MACHINES.read_text().splitlines()
         if l.startswith("; Source: ")),
        "https://github.com/artisan-roaster-scope/artisan",
    )
    lines += [
        "",
        "",
        "4. ARTISAN'S MACHINE SETUPS",
        "=" * 27,
        "",
        "Adding a roaster from Artisan's machine catalogue (docs/artisan-machines.md) reads",
        "Artisan's machine setups (src/includes/Machines/*.aset), trimmed and embedded in",
        f"roastwarden-server. Source: {source}.",
        "Artisan is Copyright the Artisan authors and licensed under the GNU Affero General",
        "Public License, version 3 (the same text as LICENSE.txt next to this file).",
    ]

    pathlib.Path(out).parent.mkdir(parents=True, exist_ok=True)
    pathlib.Path(out).write_text("\n".join(lines) + "\n")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2])
