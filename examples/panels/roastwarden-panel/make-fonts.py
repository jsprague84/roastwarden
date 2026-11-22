#!/usr/bin/env python3
"""Make fonts.h for the panel sketch: Roastwarden's own face, Atkinson Hyperlegible Next
(crates/server/assets/fonts, SIL Open Font License), as the anti-aliased fonts TFT_eSPI
loads from flash, and the Warden mark as a small picture.

    python3 make-fonts.py            # needs Pillow; rsvg-convert on PATH for the mark

A font is TFT_eSPI's "smooth font" (.vlw) layout: six 32-bit big-endian words (glyphs,
version, size, 0, ascent, descent), 28 bytes a glyph (code, height, width, advance, top
above the baseline, left, 0), then each glyph's pixels as one alpha byte each.
"""
import pathlib, struct, subprocess, sys
from PIL import Image, ImageDraw, ImageFont

HERE = pathlib.Path(__file__).resolve().parent
FONTS = HERE.parents[2] / "crates/server/assets/fonts"
BRAND = HERE.parents[2] / "crates/server/assets/brand"
TEXT = "".join(chr(c) for c in range(33, 127))
DIGITS = "0123456789.:-+"
CAPS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ<>?+-"
# name, file, pixel size, glyphs, what it is for
FACES = [
    ("FONT_TINY", "AtkinsonHyperlegibleNext-Regular.ttf", 11, DIGITS + "DFCX", "the graph's axes and landmarks"),
    ("FONT_SMALL", "AtkinsonHyperlegibleNext-Regular.ttf", 14, TEXT, "names, the status line, the small readings"),
    ("FONT_TEXT", "AtkinsonHyperlegibleNext-Bold.ttf", 20, TEXT, "the second readings, fan and heater"),
    ("FONT_BUTTON", "AtkinsonHyperlegibleNext-Bold.ttf", 24, CAPS, "the buttons' words"),
    ("FONT_NUMBER", "AtkinsonHyperlegibleNext-Bold.ttf", 30, DIGITS + "%", "the graph's readings"),
    ("FONT_CLOCK", "AtkinsonHyperlegibleNext-Bold.ttf", 40, DIGITS, "the roast clock"),
    ("FONT_BIG", "AtkinsonHyperlegibleNext-Bold.ttf", 58, DIGITS, "BT"),
]


def vlw(path: pathlib.Path, size: int, glyphs: str) -> bytes:
    font = ImageFont.truetype(str(path), size)
    ascent, descent = font.getmetrics()
    table, pixels = b"", b""
    for ch in glyphs:
        x0, y0, x1, y1 = font.getbbox(ch, anchor="ls")
        w, h = max(x1 - x0, 0), max(y1 - y0, 0)
        image = Image.new("L", (max(w, 1), max(h, 1)), 0)
        ImageDraw.Draw(image).text((-x0, -y0), ch, font=font, fill=255, anchor="ls")
        table += struct.pack(">7i", ord(ch), h, w, round(font.getlength(ch)), -y0, x0, 0)
        pixels += image.tobytes()[: w * h] if w and h else b""
    return struct.pack(">6i", len(glyphs), 11, size, 0, ascent, descent) + table + pixels


def array(name: str, data: bytes, note: str) -> str:
    rows = [", ".join(f"0x{b:02X}" for b in data[i : i + 20]) for i in range(0, len(data), 20)]
    return f"// {note}\nconst uint8_t {name}[] PROGMEM = {{\n  " + ",\n  ".join(rows) + "\n};\n"


def mark(size: int) -> bytes:
    """The Warden mark's coverage, `size` pixels square: one alpha byte a pixel."""
    png = subprocess.run(
        ["rsvg-convert", "-w", str(size), "-h", str(size), str(BRAND / "mark-mono.svg")],
        check=True, capture_output=True,
    ).stdout
    import io
    return Image.open(io.BytesIO(png)).convert("RGBA").getchannel("A").tobytes()


out = [
    "// Made by make-fonts.py: Roastwarden's face, Atkinson Hyperlegible Next (Braille Institute of",
    "// America, SIL Open Font License 1.1), as TFT_eSPI smooth fonts, and the Warden mark.",
    "// Change the sizes there, not here.",
    "#pragma once",
    "#include <Arduino.h>",
    "",
]
for name, file, size, glyphs, note in FACES:
    data = vlw(FONTS / file, size, glyphs)
    out.append(array(name, data, f"{file.split('-')[1].split('.')[0]}, {size} px, {len(glyphs)} glyphs: {note} ({len(data)} bytes)"))
    print(f"{name}: {len(data)} bytes", file=sys.stderr)
MARK = 20
out.append(f"const int MARK_SIZE = {MARK};")
out.append(array("MARK", mark(MARK), f"The Warden mark, {MARK} x {MARK}, one alpha byte a pixel"))
(HERE / "fonts.h").write_text("\n".join(out))
