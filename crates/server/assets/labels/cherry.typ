// Cherry: a coffee branch drawn in line, two cherries and two leaves, beside the details.
// Drawn with Typst's own shapes: no image file. Fonts: DM Serif Display, Atkinson Hyperlegible
// Next (SIL Open Font License).
#import "roastwarden.typ": fit, wrap, clamp, join, with

#let design = (
  name: "Cherry",
  description: "Line art of a coffee branch with two cherries, a serif name, the details in a clean sans. art: cherry, leaf, bean or none; ink: a colour for colour printers.",
  params: (
    art: (default: "cherry", choices: ("cherry", "leaf", "bean", "none"), help: "the drawing"),
    ink: (default: "#000000", help: "the colour of the drawing and the name (black for a mono laser)"),
  ),
)

// A leaf from `base` along `angle` (0deg points right, -90deg up): two curves to the tip, a vein
#let leaf-at(base, angle, len, ink) = {
  let (x, y) = base
  let (dx, dy) = (calc.cos(angle), calc.sin(angle))
  let (nx, ny) = (-dy, dx)
  let at(along, side) = (x + len * (dx * along + nx * side), y + len * (dy * along + ny * side))
  place(top + left, curve(stroke: 0.7pt + ink,
    curve.move(base),
    curve.cubic(at(0.3, 0.24), at(0.75, 0.2), at(1, 0)),
    curve.cubic(at(0.75, -0.2), at(0.3, -0.24), base),
  ))
  place(top + left, line(start: base, end: at(0.85, 0), stroke: 0.4pt + ink))
  // side veins on a big leaf
  if len > 1in {
    for t in (0.3, 0.5, 0.7) {
      for side in (1, -1) {
        place(top + left, line(start: at(t, 0), end: at(t + 0.12, side * 0.13), stroke: 0.3pt + ink))
      }
    }
  }
}

// A coffee cherry centred at `c`: the fruit, a shine, and the little crown at its end
#let fruit-at(c, r, ink) = {
  let (x, y) = c
  place(top + left, dx: x - r, dy: y - r, circle(radius: r, stroke: 0.8pt + ink))
  place(top + left, curve(stroke: 0.5pt + ink,
    curve.move((x - r * 0.55, y - r * 0.05)), curve.quad((x - r * 0.55, y - r * 0.55), (x - r * 0.05, y - r * 0.6))))
  place(top + left, dx: x + r * 0.42 - r * 0.13, dy: y + r * 0.62 - r * 0.13, circle(radius: r * 0.13, stroke: 0.5pt + ink))
}

// A branch: a stem across the top, two leaves, two cherries hanging on short stalks
#let branch(size, ink) = box(width: size, height: size, {
  let p(x, y) = (size * x, size * y)
  place(top + left, curve(stroke: 0.9pt + ink,
    curve.move(p(0.04, 0.34)), curve.cubic(p(0.3, 0.2), p(0.62, 0.36), p(0.96, 0.22))))
  leaf-at(p(0.2, 0.3), -55deg, size * 0.36, ink)
  leaf-at(p(0.68, 0.31), -20deg, size * 0.32, ink)
  leaf-at(p(0.8, 0.26), 30deg, size * 0.2, ink)
  place(top + left, line(start: p(0.45, 0.31), end: p(0.36, 0.5), stroke: 0.6pt + ink))
  place(top + left, line(start: p(0.47, 0.31), end: p(0.62, 0.53), stroke: 0.6pt + ink))
  fruit-at(p(0.33, 0.66), size * 0.16, ink)
  fruit-at(p(0.64, 0.68), size * 0.15, ink)
})

// A roasted bean: an oval with its S-shaped centre cut
#let bean(size, ink) = box(width: size, height: size, align(center + horizon, rotate(-25deg, box(width: size * 0.56, height: size * 0.8, {
  let (bw, bh) = (size * 0.56, size * 0.8)
  place(top + left, ellipse(width: bw, height: bh, stroke: 0.9pt + ink))
  place(top + left, curve(stroke: 0.8pt + ink,
    curve.move((bw * 0.5, bh * 0.06)),
    curve.cubic((bw * 0.25, bh * 0.35), (bw * 0.75, bh * 0.65), (bw * 0.5, bh * 0.94)),
  ))
}))))

#let art-of(name, size, ink) = {
  if name == "cherry" { branch(size, ink) }
  else if name == "leaf" { box(width: size, height: size, leaf-at((size * 0.15, size * 0.85), -45deg, size * 0.95, ink)) }
  else if name == "bean" { bean(size, ink) }
}

#let label(l, w, h, round) = {
  let ink = rgb(l.params.ink)
  set text(font: "Atkinson Hyperlegible Next", size: 8pt, fill: black, top-edge: "ascender", bottom-edge: "descender")
  set par(leading: 0.15em)
  let roast = join((l.level, l.purpose))
  let bean-line = join((l.origin, l.variety, l.process))
  let when = join((with("Roasted ", l.roasted), with("best from ", l.best_from)))
  let has-art = l.params.art != "none"
  if round {
    let inner = w * 0.68
    box(width: w, height: h, align(center + horizon, stack(dir: ttb, spacing: 3pt, ..(
      // A smaller drawing leaves room for a message
      if has-art { art-of(l.params.art, w * if l.message != none { 0.21 } else { 0.3 }, ink) },
      wrap(l.name, inner, max: 15pt, floor: 11pt, min: 8pt, round: true, font: "DM Serif Display", fill: ink),
      if bean-line != none { clamp(bean-line, inner * 1.05, lines: 2, size: 7pt) },
      if roast != none { text(size: 8pt, weight: "bold", upper(roast)) },
      if when != none { fit(when, inner, max: 7pt, min: 6pt) },
      if l.message != none { wrap(l.message, inner, max: 8.5pt, floor: 7pt, min: 6pt, round: true, style: "italic") },
    ).filter(x => x != none))))
  } else {
    let pad = 0.15in
    let tall = h > 2.5in
    let art-w = if has-art { calc.min(h - 2 * pad, if tall { 1.5in } else { 1.25in }) } else { 0pt }
    let gap = if has-art { 0.12in } else { 0pt }
    let text-w = w - 2 * pad - art-w - gap
    box(width: w, height: h, inset: pad, grid(
      columns: (art-w, text-w), rows: (h - 2 * pad,), column-gutter: gap, align: horizon,
      if has-art { art-of(l.params.art, art-w, ink) },
      stack(dir: ttb, spacing: 3pt, ..(
        wrap(l.name, text-w, max: if tall { 22pt } else { 18pt }, floor: 13pt, min: 9pt, font: "DM Serif Display", fill: ink),
        if bean-line != none { clamp(bean-line, text-w, lines: if tall { 2 } else { 1 }, size: 8pt) },
        line(length: text-w * 0.4, stroke: 0.5pt + ink),
        if roast != none { text(size: 9pt, weight: "bold", upper(roast)) },
        if when != none { fit(when, text-w, max: 8pt, min: 6pt) },
        if l.weight != none { text(size: 8pt, l.weight) },
        if l.message != none { wrap(l.message, text-w, max: 10pt, floor: 8.5pt, min: 7pt, style: "italic") },
        if l.notes != none { clamp(l.notes, text-w, lines: if tall { 3 } else { 1 }, size: 8pt, style: "italic") },
        if l.roaster != none { text(size: 7pt, fill: luma(80), l.roaster) },
      ).filter(x => x != none)),
    ))
  }
}
