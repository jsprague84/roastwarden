// Helpers for label designs (docs/labels.md, "Designs"). Import what you use:
//   #import "roastwarden.typ": fit, wrap, clamp, join, with, ascii
// A label's fields are text, or none when unknown or hidden.

// What a check reports (`roast labels design check`): text that was shrunk or cut
#let note(kind, body, ..more) = [#metadata((kind: kind, text: if type(body) == str { body } else { repr(body) }, ..more.named())) <rr-note>]

// `body` on one line, at the largest size from `max` down to `min` that fits `width`.
#let fit(body, width, max: 14pt, min: 7pt, ..style) = context {
  let size = calc.max(max, min)
  while size > min and measure(text(size: size, ..style, body)).width > width {
    size = calc.max(min, size - 0.5pt)
  }
  text(size: size, ..style, body)
  let over = measure(text(size: size, ..style, body)).width - width
  if over > 0pt { note("too_long", body, pt: calc.round(size / 1pt, digits: 1), over_mm: calc.round(over / 1mm, digits: 1)) }
  else if size < max { note("shrunk", body, pt: calc.round(size / 1pt, digits: 1), max: max / 1pt, min: min / 1pt) }
}

// `body` on one line at the largest size from `max` down to `floor` that fits `width`; when it
// does not fit at `floor`, on up to `lines` lines of about the same length, from `floor` down
// to `min`. For a name or a gift message that may be long. `round: true` keeps the wrapped
// lines narrower (0.9 of `width`): inside a circle they sit further from its middle.
// `floor: auto` is 0.7 of `max` (not under `min`).
#let wrap(body, width, max: 14pt, floor: auto, min: 7pt, lines: 2, round: false, ..style) = context {
  let floor = calc.max(min, if floor == auto { max * 0.7 } else { floor })
  let one(b, size) = measure(text(size: size, ..style, b)).width
  let size = calc.max(max, floor)
  while size > floor and one(body, size) > width {
    size = calc.max(floor, size - 0.5pt)
  }
  if one(body, size) <= width or lines < 2 {
    text(size: size, ..style, body)
    let over = one(body, size) - width
    if over > 0pt { note("too_long", body, pt: calc.round(size / 1pt, digits: 1), over_mm: calc.round(over / 1mm, digits: 1)) }
    else if size < max { note("shrunk", body, pt: calc.round(size / 1pt, digits: 1), max: max / 1pt, min: floor / 1pt) }
  } else {
    let narrow = if round { width * 0.9 } else { width }
    let words = if type(body) == str { body.split(" ") } else { () }
    let tall(size, w) = measure(block(width: w, text(size: size, ..style, body))).height
    // The height of n lines at a size, and the widest word (a word does not break)
    let room(size, n) = measure(block(text(size: size, ..style, range(n).map(_ => [Mg]).join(linebreak())))).height + 0.5pt
    let widest(size) = words.fold(0pt, (m, w) => calc.max(m, one(w, size)))
    let fits(size) = tall(size, narrow) <= room(size, lines) and widest(size) <= narrow
    let size = floor
    while size > min and not fits(size) {
      size = calc.max(min, size - 0.5pt)
    }
    if not fits(size) {
      block(width: narrow, text(size: size, ..style, body))
      note("too_long", body, pt: calc.round(size / 1pt, digits: 1), lines: lines,
        over_mm: calc.round(calc.max(tall(size, narrow) - room(size, lines), widest(size) - narrow) / 1mm, digits: 1))
    } else {
      // As few lines as it takes, then the narrowest width that keeps them: lines of about
      // the same length
      let n = range(1, lines + 1).find(k => tall(size, narrow) <= room(size, k))
      let (lo, hi) = (calc.max(narrow / n * 0.8, widest(size)), narrow)
      for _ in range(10) {
        let mid = (lo + hi) / 2
        if tall(size, mid) <= room(size, n) { hi = mid } else { lo = mid }
      }
      block(width: hi, text(size: size, ..style, body))
      note("wrapped", body, pt: calc.round(size / 1pt, digits: 1), lines: n)
    }
  }
}

// A note for the check (a soft issue, `hint`): something the design wants its user to know,
// e.g. "no brand set: the top of the label is empty". Prints nothing.
#let hint(body) = note("hint", body)

// `body` (text) in at most `lines` lines of `width`, cut if longer: after the last whole
// sentence when that keeps at least half of what fits, else after a word with "…".
// With `height`, as many lines as fit in it (`lines` then caps them; `auto`: no cap).
#let clamp(body, width, lines: auto, height: none, ..style) = context {
  let shown(t) = block(width: width, text(..style, t))
  let room(n) = measure(shown(range(n).map(_ => [Mg]).join(linebreak()))).height + 0.5pt
  let cap = if lines == auto { if height == none { 2 } else { 40 } } else { lines }
  let lines = if height == none { cap } else {
    let n = 1
    while n < cap and room(n + 1) <= height { n += 1 }
    n
  }
  let limit = room(lines)
  if measure(shown(body)).height <= limit {
    shown(body)
  } else {
    let words = body.split(" ")
    let k = words.len() - 1
    while k > 1 and measure(shown(words.slice(0, k).join(" ") + "…")).height > limit {
      k -= 1
    }
    // A whole sentence reads better than one cut off mid-way
    let ends(w) = w.ends-with(".") or w.ends-with("!") or w.ends-with("?")
    let whole = range(k, 0, step: -1).find(j => j * 2 >= k and ends(words.at(j - 1)))
    if whole != none {
      shown(words.slice(0, whole).join(" "))
      note("cut", body, lines: lines, kept: whole, words: words.len(), at: "sentence")
    } else {
      shown(words.slice(0, k).join(" ") + "…")
      note("cut", body, lines: lines, kept: k, words: words.len())
    }
  }
}

// The parts that are there (not none, not empty) joined by `sep`, or none.
#let join(parts, sep: " · ") = {
  let there = parts.filter(p => p != none and p != "").map(p => if type(p) == str { p } else { str(p) })
  if there.len() == 0 { none } else { there.join(sep) }
}

// Prefix `word` to a value that is there: `with("Roasted ", l.roasted)`.
#let with(word, value) = if value == none { none } else { word + if type(value) == str { value } else { str(value) } }

// The QR code's width for its squares to be `module` each (0.4 mm reads well from a phone),
// from `min` to `max`: `image("qr.svg", width: qr-width(l))`. A share link's code has fewer
// squares than a long address, so it can be smaller (`l.qr_modules`, its width in squares).
#let qr-width(l, module: 0.4mm, min: 0.5in, max: 1.2in) = {
  let m = l.at("qr_modules", default: none)
  if m == none { min } else { calc.min(max, calc.max(min, m * module)) }
}

// What the QR code opens, in a few words for a line beside it, or none (`l.qr_to`: `share`,
// `app` or `link`)
#let qr-caption(l) = {
  let to = l.at("qr_to", default: none)
  if not l.qr { none }
  else if to == "share" { "Scan for this roast and how to brew it" }
  else if to == "app" { "Scan for this roast" }
  else if to == "link" { "Scan for the coffee" }
  else { none }
}

// ASCII art (or any monospaced text) exactly as written: spaces kept, a line per line.
// `raw` keeps the spacing; the show rule gives it the label's monospace font and size.
#let ascii(art, size: 7pt, font: "JetBrains Mono NL", leading: 0.3em, ..style) = {
  show raw: set text(font: font, size: size, ..style)
  set par(leading: leading)
  raw(art, block: true)
}

// `body` with its letters at least `height` tall as US labelling rules measure them: the
// lowercase "o" (the x-height), or the capitals when the text is all capitals. For the net
// weight on a bag that is sold (`l.retail.min_net_mm`, 21 CFR 101.7) and for what must be
// at least 1/16 in (`l.retail.min_mm`). The size grows from `min` by half points until it is.
// `caps: true` measures the capitals whatever the text (to match another line's capitals).
#let tall(body, height, min: 6pt, max: 72pt, caps: auto, ..style) = context {
  let t = if type(body) == str { body } else { repr(body) }
  let letters = t.clusters().filter(c => lower(c) != upper(c))
  let all-caps = letters.len() > 0 and letters.all(c => c == upper(c))
  let edge = if caps == true or (caps == auto and all-caps) { "cap-height" } else { "x-height" }
  let h(size) = measure(text(size: size, top-edge: edge, bottom-edge: "baseline", ..style, body)).height
  let size = min
  while h(size) < height and size < max { size += 0.5pt }
  text(size: size, ..style, body)
}
