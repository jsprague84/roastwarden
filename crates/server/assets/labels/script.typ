// Script: the coffee's name in calligraphy over a drawn flourish, the details in small caps.
// Fonts: Pinyon Script, Cormorant Garamond (SIL Open Font License).
#import "roastwarden.typ": fit, wrap, clamp, join, with

#let design = (
  name: "Script",
  description: "The name in a copperplate script, a drawn flourish, small-caps details. For gifts and small batches.",
  params: (
    title: (default: "", help: "script line instead of the coffee's name (e.g. Sam's Blend); empty: the name"),
    top: (default: "small batch · hand roasted", help: "the small line above the name; empty: none"),
    flourish: (default: true, help: "draw the flourish under the name"),
  ),
)

#let ink = rgb("#1f1a17")
#let soft = rgb("#5c5048")

// A copperplate flourish: two mirrored swashes meeting at a small loop
#let flourish(width) = box(width: width, height: 0.16in, {
  let half = width / 2
  let s = (stroke: 0.6pt + ink)
  place(top + left, curve(..s,
    curve.move((0pt, 0.11in)),
    curve.cubic((half * 0.35, 0.02in), (half * 0.7, 0.15in), (half - 0.05in, 0.07in)),
  ))
  place(top + left, curve(..s,
    curve.move((width, 0.11in)),
    curve.cubic((width - half * 0.35, 0.02in), (width - half * 0.7, 0.15in), (half + 0.05in, 0.07in)),
  ))
  place(top + left, dx: half - 0.035in, dy: 0.035in, circle(radius: 0.035in, stroke: 0.5pt + ink))
})

#let small-caps(t, size) = text(font: "Cormorant Garamond", weight: "semibold", size: size,
  tracking: 0.14em, fill: soft, upper(t))

#let label(l, w, h, round) = {
  set text(font: "Cormorant Garamond", fill: ink, top-edge: "ascender", bottom-edge: "descender")
  set par(leading: 0.2em)
  set align(center)
  let title = if l.params.title != "" { l.params.title } else { l.name }
  let k = if round { 0.82 } else { calc.min(h / 2in, w / 4in * 1.15) }
  let inner = if round { w * 0.7 } else { w - 0.5in }
  let edge = if round { w * 0.6 } else { inner }
  let roast = join((l.level, l.purpose))
  let bean = join((l.origin, l.process))
  let dates = if round {
    (with("roasted ", l.roasted), with("best from ", l.best_from))
  } else {
    (join((with("roasted ", l.roasted), with("best from ", l.best_from), l.weight)),)
  }
  box(width: w, height: h, align(center + horizon, stack(dir: ttb, spacing: 3pt * k, ..(
    if l.params.top != "" { fit(upper(l.params.top), edge, max: calc.max(6.5pt * k, 6pt), min: 5.5pt,
      font: "Cormorant Garamond", weight: "semibold", tracking: 0.14em, fill: soft) },
    // The script line: smaller, then on two lines; on a round label down to 11 pt
    wrap(title, inner, max: 30pt * k, floor: 18pt * k, min: if round { 11pt } else { 14pt }, round: round, font: "Pinyon Script"),
    if l.params.flourish { flourish(inner * 0.55) },
    if title != l.name { wrap(l.name, inner, max: 10pt * k, min: 7pt, round: round, style: "italic") },
    if l.message != none { wrap(l.message, inner, max: 12pt * k, floor: 9pt, min: 7pt, round: round, style: "italic") },
    if bean != none {
      if round { clamp(bean, inner, lines: 1, size: 7pt, style: "italic", fill: soft) }
      else { fit(bean, inner, max: 9.5pt * k, min: 6.5pt, style: "italic", fill: soft) }
    },
    if roast != none { fit(upper(roast), edge, max: calc.max(7pt * k, 6pt), min: 5.5pt, weight: "semibold", tracking: 0.14em) },
    ..dates.filter(d => d != none).map(d => fit(d, edge, max: 8pt * k, min: 6pt, fill: soft)),
    if l.notes != none and h > 2.5in { clamp(l.notes, inner, lines: 2, size: 9pt, style: "italic") },
  ).filter(x => x != none))))
}
