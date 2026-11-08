// The plain label: everything at a glance, in the dashboard's typeface.
#import "roastwarden.typ": fit, wrap, clamp, join, with

#let design = (
  name: "Plain",
  description: "Everything at a glance in the dashboard's typeface, the QR code on the right; a centred stack on round labels.",
)

#let label(l, w, h, round) = {
  // Line boxes from ascender to descender, so stacked lines keep their space
  set text(font: "Atkinson Hyperlegible Next", size: 9.5pt, fill: black,
    top-edge: "ascender", bottom-edge: "descender")
  set par(leading: 0.1em)
  let roast = join((l.level, l.purpose))
  let bean = join((l.origin, l.variety, l.process))
  let when = join((with("Roasted ", l.roasted), with("batch #", l.batch)))
  let best = join((with("Best from ", l.best_from), l.weight))
  // Whole bean or ground ("Ground · espresso"); a ground bag says how long it keeps
  let ground = l.at("ground", default: false)
  let form = join((l.at("form", default: none), if ground { with("best until ", l.at("best_until", default: none)) }))
  let tall = h > 2.5in
  if round {
    let inner = w * 0.7
    box(width: w, height: h, align(center + horizon, stack(dir: ttb, spacing: 2pt,
      ..(
        wrap(l.name, inner, max: 13pt, floor: 10pt, min: 7pt, round: true, weight: "bold"),
        // With a message there is room for one line: the variety gives way; a ground bag's
        // form line takes the other (a round label prints the form only when ground)
        if l.message != none and ground { none }
        else if (l.message != none or ground) and join((l.origin, l.process)) != none { clamp(join((l.origin, l.process)), inner * 1.05, lines: 1, size: 7pt) }
        else if bean != none { clamp(bean, inner * 1.05, lines: 2, size: 7pt) },
        v(1pt),
        if roast != none { text(size: 9.5pt, weight: "bold", roast) },
        if when != none { fit(when, inner * 1.1, max: 8pt, min: 6pt) },
        if best != none { fit(best, inner * 1.1, max: 8pt, min: 6pt) },
        if ground and form != none { fit(form, inner * 1.1, max: 8pt, min: 6pt) },
        if l.message != none { wrap(l.message, inner, max: 9.5pt, floor: 8pt, min: 6pt, round: true, style: "italic") },
        if l.qr { v(4pt) },
        if l.qr { image("qr.svg", width: 0.56in) },
      ).filter(x => x != none))))
  } else {
    let pad = 0.15in
    let qr = if l.qr { if tall { 1.15in } else { 0.95in } } else { 0pt }
    let text-w = w - 2 * pad - (if l.qr { qr + 0.14in } else { 0pt })
    // The lines above the notes at a scale `k` (1: the sizes for a 4 × 2 in label), and the
    // roaster's name under them
    let head(k) = (
      wrap(l.name, text-w, max: (if tall { 19pt } else { 16pt }) * k, floor: 12pt, min: 8pt, weight: "bold"),
      if bean != none { clamp(bean, text-w, lines: if tall { 2 } else { 1 }, size: 8pt * k) },
      v(3pt * k),
      if roast != none { text(size: 11pt * k, weight: "bold", roast) },
      if when != none { fit(when, text-w, max: 9.5pt * k, min: 7pt) },
      if best != none { fit(best, text-w, max: 9.5pt * k, min: 7pt) },
      if form != none { fit(form, text-w, max: 9.5pt * k, min: 7pt) },
      if l.message != none { wrap(l.message, text-w, max: 11pt * k, floor: 9pt, min: 7pt, style: "italic") },
    ).filter(x => x != none)
    let foot(k) = if l.roaster != none { text(size: 7.5pt * k, fill: luma(90), l.roaster) }
    let gap = 1.5pt
    let room = h - 2 * pad
    // Everything, the notes whole, at a scale
    let whole(k) = stack(dir: ttb, spacing: gap * k, ..head(k), ..(
      if l.notes != none { v(2pt * k) },
      if l.notes != none { block(width: text-w, text(size: 8pt * k, style: "italic", l.notes)) },
      foot(k),
    ).filter(x => x != none))
    box(width: w, height: h, inset: pad, context {
      // A roomy label (or short text) grows the type and centres it, so no third of the
      // label is left empty; when even the usual sizes do not hold the notes, they take the
      // height that is left, cut
      let scales = if tall { (1.4, 1.3, 1.2, 1.1, 1.0) } else { (1.15, 1.0) }
      let k = scales.find(k => measure(block(width: text-w, whole(k))).height <= room)
      grid(
        columns: (text-w, qr), column-gutter: if l.qr { 0.14in } else { 0pt },
        if k != none {
          block(height: room, align(horizon, whole(k)))
        } else {
          let head = head(1)
          let foot = foot(1)
          let above = measure(block(width: text-w, stack(dir: ttb, spacing: gap, ..head))).height
          let below = if foot != none { measure(foot).height + gap } else { 0pt }
          let left = room - above - below - 2pt - 2 * gap
          stack(dir: ttb, spacing: gap, ..head, ..(
            if l.notes != none and left > 8pt { v(2pt) },
            if l.notes != none and left > 8pt { clamp(l.notes, text-w, height: left, size: 8pt, style: "italic") },
            foot,
          ).filter(x => x != none))
        },
        if l.qr { block(height: room, align(right + if k != none { horizon } else { top }, image("qr.svg", width: qr))) },
      )
    })
  }
}
