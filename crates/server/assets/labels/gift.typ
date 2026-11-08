// The gift label: a display serif, a script for the message, a double frame.
// Fonts: DM Serif Display, Cormorant Garamond, Pinyon Script (SIL Open Font License).
#import "roastwarden.typ": fit, wrap, clamp, join, with, qr-width, qr-caption

#let design = (
  name: "Gift",
  description: "A display serif name, a script line for the message, a double frame; the QR code, to the roast's share page, only where there is room.",
  // A gift's QR code opens the share page (the roast, its curve, how to brew it), not the app
  qr: "share",
)

#let ink = rgb("#2b2118")
#let soft = rgb("#6b5a4a")

#let rule(width) = box(width: width, grid(columns: (1fr, auto, 1fr), align: horizon, column-gutter: 6pt,
  line(length: 100%, stroke: 0.4pt + soft), text(size: 6pt, fill: soft)[◆], line(length: 100%, stroke: 0.4pt + soft)))

// The label's words, centred; `k` scales the type (1 on a 4 × 2 in label)
#let words(l, width, k, round, qr: false) = {
  let roast = join((l.level, l.purpose))
  let bean = join((l.origin, l.variety, l.process))
  // One line of dates and weight on a rectangle; two inside a circle
  let foot = if round {
    (join((with("roasted ", l.roasted), l.weight)), with("best from ", l.best_from))
  } else {
    (join((with("roasted ", l.roasted), with("best from ", l.best_from), l.weight)),)
  }
  let note-lines = if k > 1.2 { 3 } else if l.message != none or round { 1 } else { 2 }
  // Top and bottom lines are narrower inside a circle
  let edge = if round { width * 0.86 } else { width }
  let serif = (font: "Cormorant Garamond", fill: ink)
  stack(dir: ttb, spacing: 2.5pt * k, ..(
    fit(upper(if roast != none { (if round { "" } else { "Freshly roasted · " }) + roast } else { "Freshly roasted" }), edge,
      max: 7pt * k, min: 5pt, font: "Cormorant Garamond", weight: "semibold", tracking: if round { 0.12em } else { 0.2em }, fill: soft),
    wrap(l.name, width, max: 22pt * k, floor: 13pt * calc.min(k, 1), min: 9pt, round: round, font: "DM Serif Display", fill: ink),
    if bean != none { clamp(bean, width, lines: if round { 2 } else { 1 }, ..serif, style: "italic", size: 10pt * calc.min(k, 1.2)) },
    rule(width * 0.5),
    if l.message != none { wrap(l.message, width, max: 19pt * k, floor: 13pt * calc.min(k, 1), min: 9pt, round: round, font: "Pinyon Script", fill: ink) },
    if l.notes != none { clamp(l.notes, width, lines: note-lines, ..serif, style: "italic", size: 9.5pt * calc.min(k, 1.2), fill: soft) },
    ..foot.filter(f => f != none).map(f => fit(f, edge, max: 8.5pt * calc.min(k, 1.2), min: 6pt, ..serif)),
    if l.roaster != none { fit("from " + l.roaster, edge, max: 8.5pt, min: 6pt, ..serif, style: "italic", fill: soft) },
    if qr {
      let cap = qr-caption(l)
      let code = image("qr.svg", width: qr-width(l, min: 0.55in, max: 0.75in))
      if cap == none { code } else {
        grid(columns: (auto, auto), column-gutter: 6pt, align: horizon + left, code,
          box(width: 0.9in, text(font: "Cormorant Garamond", style: "italic", size: 8.5pt, fill: soft, cap)))
      }
    },
  ).filter(x => x != none))
}

#let label(l, w, h, round) = {
  set text(fill: ink, top-edge: "ascender", bottom-edge: "descender")
  set par(leading: 0.25em)
  set align(center)
  if round {
    box(width: w, height: h, {
      place(center + horizon, circle(radius: w / 2 - 0.09in, stroke: 0.6pt + ink))
      place(center + horizon, circle(radius: w / 2 - 0.13in, stroke: 0.3pt + soft))
      place(center + horizon, words(l, w * 0.66, 0.8, true))
    })
  } else {
    let k = calc.min(h / 2in, w / 4in * 1.2)
    let tall = h > 2.5in
    box(width: w, height: h, {
      place(center + horizon, rect(width: w - 0.16in, height: h - 0.16in, radius: 3pt, stroke: 0.6pt + ink))
      place(center + horizon, rect(width: w - 0.24in, height: h - 0.24in, radius: 2pt, stroke: 0.3pt + soft))
      // A QR code only where there is room for it below the words
      place(center + horizon, words(l, w - 0.6in, k, false, qr: l.qr and tall))
    })
  }
}
