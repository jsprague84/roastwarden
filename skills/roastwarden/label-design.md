# Designing Roastwarden labels

Read this before writing or changing a label design. A design is one Typst file (Typst
**0.15**) that draws **one** label; Roastwarden places it on the sheet. Every snippet here
compiles on the server.

## The loop: text first, pixels once

1. Start from the template below, or a built-in design's idea (plain, gift, script, ascii,
   cherry, shop: `roast labels --list`).
2. `roast labels design check draft.typ --sheet 5163` (sample data; `--roast latest` for a
   real roast). It answers in one JSON line:
   - `errors`: Typst's, with `file`, `line`, `column`, `hints` (exit 6). Fix them first.
   - `report.issues` (exit 1): `overflow` (past the edge, by mm, which edge), `near_edge`
     (closer than 1.5 mm: printers land up to 1 mm off), `overlap` (text on text),
     `too_long` (a `fit` or `wrap` at its minimum and still too long), `cut` (a `clamp`
     dropped words; it says so when the label had room to spare: "while 21 mm of the
     label's height is free"),
     `tiny_text` (under 5.5 pt), `qr_small` / `qr_crowded`, `typst` (a warning, e.g. an
     unknown font that fell back), `param` (a parameter the design does not declare).
     Soft ones leave `"ok": true`: `dropped` (the name, roast date or best-from date did
     not print: a bag's label is for those) and `hint` (the design's own note, `hint()`).
   - `report.free`: the height nothing printed in, above the first line and below the last
     (mm): a third of a tall label empty is a design to centre or grow, not to ship.
   - `report.lines`: each line of text as printed, with its font, size (pt) and box (mm
     from the top left); `report.images`; `report.fields.shown` / `missing` (fields with a
     value that did not print: found by their words in any case or punctuation, so
     `upper(l.name)` or a slug `kenya-nyeri-gatugi-aa` counts, and a date in its usual
     forms, "29 Oct", "October 29", "2026-10-29"); `report.params` (what the design got);
     `report.notes` (what `fit`, `wrap` and `clamp` did: `shrunk`, `wrapped`, `cut`).
   Clean is exit 0 and `"ok": true`. `--bean "<coffee>"` checks it on that coffee's name,
   origin and notes (sample roast data), `--roast latest` on a real roast.
3. Fix from the report and check again. It is exact (the layout engine's own boxes); a
   picture is not.
4. Once it is clean, look at it **once** if you can see images: `--png label.png` (100 dpi,
   about 100 to 250 image tokens). Not every turn.
5. Check every sheet the owner uses: `--sheet 5163`, `6874`, `5294` (round, `round` is
   true: keep text inside the circle).
6. Use it: `roast labels latest --source draft.typ --set art=cup --count 2` writes the PDF
   (a one-off; nothing is saved).
7. To keep it (when the owner wants it again): `roast labels design add sam-gift draft.typ
   --note "…"` saves version 1 (it must compile; the answer has the check). Later changes:
   `roast labels design edit sam-gift draft.typ --note "…"` (version 2, …). To change a
   saved one, start from its source: `roast labels design show sam-gift -o draft.typ`. A
   coffee's own label: `roast labels design use sam-gift --bean "…" --set k=v`.

## The contract

```typst
#import "roastwarden.typ": fit, wrap, clamp, join, with, ascii, tall, hint, qr-width, qr-caption

// Optional manifest: a name, a description, the QR code's default, and parameters
#let design = (
  name: "Mine",
  description: "What it is for",
  qr: "share",                    // optional: app, share, supplier or none (a gift: share)
  params: (
    art: (default: "cup", choices: ("cup", "none"), help: "the drawing"),
    ink: "#000000",               // a plain default works too
  ),
)

#let label(l, w, h, round) = { ... }   // returns the label's content, w × h
```

- `w`, `h`: the label's size (lengths); `round`: true on a round label (`w == h`, draw
  inside the circle).
- `l`: the fields, each text or `none` (unknown or hidden): `l.name` (the bean's short
  name when it has one, else its name; `l.full_name` is always the whole name), `l.origin`
  ("Tarrazú, Costa Rica"), `l.variety`, `l.process`, `l.level` ("Light"), `l.purpose`
  ("Espresso", "Filter"), `l.roasted` ("25 Oct 2026"; "6–7 Nov 2026" for roasts in one bag), `l.batch` (a number, or text such as "16+17" for roasts in one bag: print it with `with("#", l.batch)` or `str(l.batch)`, never do sums with it), `l.best_from`, the dates
  as data for printing them your own way: `l.roasted_iso` and `l.best_from_iso`
  ("2026-10-25", `datetime(year: …)` from its parts) and `l.rest_days` (a number),
  `l.weight` ("250 g · 8.8 oz"), `l.net` ("Net wt 8.8 oz (250 g)", the way a bag that is
  sold states it), `l.weight_g` (a number), `l.notes` (the bean's label notes, a few
  words, "Chocolate · coconut · apple"; else the roast's cupping notes; else the
  supplier's prose cut at a sentence: may be long, so `clamp` it), `l.message` (a gift line),
  `l.roaster`, `l.brew` (how to brew it, the share page's recipe, or `none` for an omni
  roast: `method` "Espresso"/"Filter", `dose_g`, `out_g`, `out_is` "Yield"/"Water",
  `ratio` "1:2.5", `temp_c`, `time` "27–30 s", `grind`, `line` the recipe in a sentence;
  use it rather than a table of your own, so the label and the share page agree),
  `l.url` (what the QR code opens: the roast in the dashboard, or with
  `--qr share` its public share page, `https://…/R/` and 25 capitals). `l.qr` is true when
  the QR code is wanted: `image("qr.svg", width: qr-width(l))` (sized by its squares:
  `l.qr_modules`). `l.qr_to` is where it goes, `share`, `app` or `link`, and
  `qr-caption(l)` says it in words for a line beside the code ("Scan for this roast and
  how to brew it"). A design's `qr` in its manifest is the default when nobody asks.
- `l.brand`: the brand kit (`roast labels brand list`): `name`, `tagline`, `place`,
  `website`, `logo` (an image's name: `image(l.brand.logo, width: 0.5in)`), `ink`,
  `accent`, `paper` (`#rrggbb`), `font_display`, `font_body`, `font_script`. Every key is
  there and may be `none` (no brand): `if l.brand.name != none { … }`.
- Images the owner added (`roast labels asset list`): `image("logo.svg", width: 0.6in)`.
- `l.params`: the parameters, defaults filled in. Text given on the command line
  (`--set size=12`) takes the default's type (int, float, bool); colours are text: write
  `rgb(l.params.ink)`. `--set art=@cup.txt` reads a file (ASCII art).
- Always handle `none`: `if l.notes != none { … }`, or the helpers below.

## A complete label to start from

```typst
#import "roastwarden.typ": fit, wrap, clamp, join, with

#let label(l, w, h, round) = {
  set text(font: "Atkinson Hyperlegible Next", size: 8.5pt, top-edge: "ascender", bottom-edge: "descender")
  set par(leading: 0.15em)
  let pad = 0.15in
  let inner = if round { w * 0.7 } else { w - 2 * pad }
  box(width: w, height: h, inset: if round { 0pt } else { pad },
    align(if round { center + horizon } else { left + horizon }, stack(dir: ttb, spacing: 3pt, ..(
      wrap(l.name, inner, max: 16pt, floor: 11pt, min: 8pt, round: round, weight: "bold"),
      if join((l.origin, l.process)) != none { clamp(join((l.origin, l.process)), inner, lines: 1) },
      if join((l.level, l.purpose)) != none { text(weight: "bold", upper(join((l.level, l.purpose)))) },
      with("Roasted ", l.roasted),
      if l.message != none { wrap(l.message, inner, max: 10pt, floor: 8pt, min: 6pt, round: round, style: "italic") },
    ).filter(x => x != none))))
}
```

## Helpers (`roastwarden.typ`)

- `fit(text, width, max: 14pt, min: 7pt, ..style)`: one line, the largest size that fits.
- `wrap(text, width, max: 14pt, floor: auto, min: 7pt, lines: 2, round: false, ..style)`:
  for the name and a gift message, which can be long. One line, shrinking from `max` to
  `floor` (`auto`: 0.7 of `max`); longer, on up to `lines` lines of about the same length,
  from `floor` down to `min`. `round: round` keeps the wrapped lines narrower inside a
  circle. A 52-character message fits a 2½ in round label this way; `fit` cannot.
- `hint(text)`: a note for the check (a soft `hint` issue), printing nothing: say what
  the design needs ("no brand set: the top is empty").
- `qr-width(l, module: 0.4mm, min: 0.5in, max: 1.2in)` and `qr-caption(l)`: above.
- `clamp(text, width, lines: 2, height: none, ..style)`: at most N lines, cut after the
  last whole sentence when that keeps at least half, else after a word
  with "…". With `height` (the room left, from `measure` in a `context`), as many lines as
  fit in it, so notes fill a tall label instead of stopping at two lines.
- `join((a, b, c), sep: " · ")`: the parts that are there, or `none`.
- `with("Roasted ", l.roasted)`: the prefix and the value, or `none`.
- `ascii(art, size: 7pt, font: "JetBrains Mono NL")`: monospaced lines exactly as written.
- `tall(text, height, caps: auto, min: 6pt, ..style)`: the size at which its letters (the
  lowercase "o", or the capitals in an all-capitals line, or `caps: true`) reach `height`.

`fit`, `wrap` and `clamp` tell the check what they did (`too_long`, `cut`, and `notes` for
text they shrank or wrapped).

Notes in the height that is left (the built-in `plain` does this):

```typst
#import "roastwarden.typ": fit, wrap, clamp

#let label(l, w, h, round) = {
  set text(font: "Atkinson Hyperlegible Next", size: 8.5pt, top-edge: "ascender", bottom-edge: "descender")
  let pad = 0.15in
  let inner = if round { w * 0.7 } else { w - 2 * pad }
  let head = (
    wrap(l.name, inner, max: 16pt, floor: 11pt, min: 8pt, round: round, weight: "bold"),
    if l.roasted != none { fit("Roasted " + l.roasted, inner, max: 9pt) },
  ).filter(x => x != none)
  box(width: w, height: h, inset: if round { (x: 0pt, y: 0.5in) } else { pad },
    align(if round { center + top } else { left + top }, context {
      let left = h - 2 * (if round { 0.5in } else { pad }) - measure(block(width: inner, stack(dir: ttb, spacing: 3pt, ..head))).height - 3pt
      let notes = if l.notes != none and left > 10pt { clamp(l.notes, inner, height: left, style: "italic") }
      stack(dir: ttb, spacing: 3pt, ..(..head, notes).filter(x => x != none))
    }))
}
```

## Fonts (all SIL Open Font License)

| Family | For |
|---|---|
| Atkinson Hyperlegible Next | clean sans, small sizes (regular, bold, italic) |
| DM Serif Display | a display serif for names (regular, italic) |
| Cormorant Garamond | a book serif, small caps lines (regular, italic, semibold) |
| Pinyon Script | calligraphy: names, gift lines (regular only) |
| JetBrains Mono NL | monospace for ASCII art and ticket layouts (regular, bold; no ligatures) |

A family not in this list falls back to another font: the check reports it as `typst`.
`roast labels --list` shows the fonts there are, the owner's included.

## Typst 0.15: what trips models up

- **Modes**: a file starts in markup; `#` switches to code for one expression. Inside
  `{ … }` (code) do not write `#`. `[ … ]` is content (markup); `"…"` is a string. Join
  content with `+` or put it in `[…]`; `text("a") + text("b")`.
- **`if` / `else` at the top of the file** ends at the line's end: `#let f(x) = if a { 1 }`
  then `else …` on the next line is an error ("unclosed delimiter" or "expected
  expression"). Wrap it: `#let f(x) = { if a { 1 } else { 2 } }`.
- **Lengths** have units: `2mm`, `8pt`, `0.15in`, `1em`, `50%`, `1fr`. `3 * 1pt` is a
  length; a bare `3` is not. Divide a length by a length for a number: `w / 1mm`.
- **`set` rules** last to the end of the block they are in. `set page(…)` is the driver's:
  never write it.
- **Measuring** needs `context`: `context { let m = measure(body); … }`. `locate(…)` with a
  function, `style(…)`, `path(…)`, `pattern(…)` and `.decode` are gone; use `context`,
  `curve(…)`, `tiling(…)`, `json(bytes(…))`.
- **Shapes**: `rect(width:, height:, radius:, stroke:, fill:)`, `circle(radius:)`,
  `ellipse(width:, height:)`, `line(start: (x, y), end: (x, y), stroke: 0.5pt + black)`,
  `polygon((x, y), …)`, and `curve(stroke:, curve.move((x, y)), curve.line((x, y)),
  curve.quad(ctrl, end), curve.cubic(c1, c2, end), curve.close())`. Points are arrays of
  lengths.
- **Placing**: `place(top + left, dx: 2mm, dy: 3mm, body)` takes no room; `box(width:,
  height:, inset:, body)` is inline; `stack(dir: ttb, spacing: 3pt, a, b)`;
  `grid(columns: (1in, 1fr), column-gutter: 0.1in, a, b)`; `align(center + horizon, body)`;
  `rotate(-20deg, body)`. Spread an array into arguments with `..list`, and drop the
  `none`s first: `stack(..items.filter(x => x != none))`.
- **Text** wraps at the width it gets: one line that must not wrap goes through `fit`.
  Raw text (`raw(…)`) ignores `text(font:)`; use `ascii` (it sets the raw font).
- **Colours**: `black`, `luma(90)` (grey), `rgb("#5c5048")`.
- **Loops**: `for x in arr { … }`; Typst stops `while` after 10 000 turns; a render that runs
  past 10 s or 1 GB is stopped (the error says so).
- No packages (`@preview/…`) and no network: draw with the shapes above.

## Patterns

**ASCII art** (the built-in `ascii` design): keep the drawing in a parameter so each coffee
can have its own, and print it with `ascii`:

```typst
#let design = (params: (art: (default: "   ( (\n    ) )\n ........\n |      |]\n \\      /\n  `----'", help: "ASCII art")))
#let label(l, w, h, round) = box(width: w, height: h, inset: 0.15in,
  grid(columns: (1.05in, 1fr), rows: (h - 0.3in,), align: horizon,
    ascii(l.params.art, size: 8.5pt), text(font: "JetBrains Mono NL", upper(l.name))))
```

In a Typst string a backslash is `\\` and a quote `\"`. With `--set art=@file.txt` the file
is used as it is. 6 to 10 lines of 8 to 14 characters fit beside the text on a 4 × 2 label.

**Calligraphy** (the built-in `script` design): Pinyon Script for the name or the gift line
only, large (`fit(…, max: 30pt, min: 14pt, font: "Pinyon Script")`), never in capitals,
with a book serif (Cormorant Garamond) in small capitals or italic for the rest. A drawn
flourish under it: two mirrored `curve.cubic` swashes and a small `circle`.

**Drawings** (the built-in `cherry` design): line art from `circle`, `ellipse`, `line` and
`curve`, 0.5 to 0.9 pt strokes. Compute points from a base, an angle and a length
(`calc.cos`, `calc.sin`) rather than nudging numbers. A coffee cherry is a circle with a
small shine curve and a tiny ring at its end; a roasted bean is an `ellipse` with an
S-shaped `curve.cubic` down its middle.

**QR code**: only when `l.qr`; at least 0.33 mm per square (13 mil), about 13 mm wide for a roast's
link (the check reports `qr_small`); keep two squares of white around it (`qr_crowded`).

**Round labels**: centre everything (`align(center + horizon, …)`); the top and bottom lines
are narrower (about 0.6 of the width); the check measures text corners against the circle.
Long names and messages: `wrap(…, round: true)`, never `fit` alone (a 2½ in circle holds
about 25 characters of 9 pt on one line).

## Printing on a laser

- Most of the owner's prints are black toner on white or kraft stock: black and greys
  (`luma(60)` or darker). Very light greys and strokes under 0.3 pt vanish.
- Keep text 1.5 mm from every edge; at least 5.5 pt (6 pt or more on kraft).
- The labels are 4 × 2 in (5163), 3¾ × 3 in (6874) and 2½ in round (5294).

**For sale** (only if the owner sells coffee, not for gifts): check with `--retail <bag>`
(`pouch-12oz`, `pouch-8oz`, `sample-2oz`, … in `roast labels --list`; ask the owner which
bag). The type height follows the **bag's front**, not the label: a 12 oz pouch needs the net
weight's letters at 3/16 in (4.8 mm). The check reads `retail_*` issues with the size to use;
fix them until it is clean, and tell the owner the "no check can know" list. What it needs:
"Whole bean coffee" in bold, its capitals at least half the largest print's; `l.net`
("Net wt 11.9 oz (340 g)", never rounded up) alone in the bottom 30 %, bold, with its letter
height of space above it; the brand's name and place with city, state and ZIP
(`l.brand.name`, `l.brand.place`), at least 1/16 in. With `--retail` the label gets
`l.retail` (`min_net_mm`, `min_mm`): size lines with `tall(…)`:

```typst
#import "roastwarden.typ": fit, tall

#let label(l, w, h, round) = {
  set text(font: "Atkinson Hyperlegible Next", top-edge: "ascender", bottom-edge: "descender")
  let r = l.at("retail", default: none)
  let need = if r != none { r.min_net_mm * 1mm } else { 2mm }
  let net = if l.net != none { l.net } else { "Net wt 12 oz (340 g)" }
  box(width: w, height: h, inset: 0.14in, {
    place(top + left, stack(dir: ttb, spacing: 4pt,
      fit(l.name, w - 0.3in, max: 14pt, weight: "bold"),
      tall("Whole bean coffee", need / 2, caps: true, weight: "bold"),
    ))
    place(bottom + left, tall(upper(net), need, weight: "bold"))
  })
}
```

The built-in `shop` design does all of it with the brand on top: start from it
(`roast labels design show shop -o mine.typ`); on a 4 × 2 label its brand header gives way.
`--diff mine@2` says what moved against an earlier version.
