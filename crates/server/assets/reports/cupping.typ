// The cupping sheet (docs/reports.md, docs/cupping.md): a blank form to score on, after the
// classic SCA protocol, with blind codes and the key on a page of its own; or a session's
// scores as a table.
#import "report.typ": *

#let design = (
  name: "Cupping sheet",
  description: "A blank form after the classic SCA protocol, two samples a page or six in a grid, blind codes with the key apart; or a session's scores as a table.",
)

// A quality scale from 6 to 10 in quarter points, whole points labelled under their ticks,
// clear of them
#let qscale(width) = box(width: width, height: 6mm, {
  let x(v) = (v - 6) / 4 * width
  place(top + left, dy: 2.4mm, line(length: width, stroke: 0.6pt + ink))
  for i in range(17) {
    let v = 6 + i * 0.25
    let (dy, len) = if calc.rem(i, 4) == 0 { (0.9mm, 3mm) } else if calc.rem(i, 2) == 0 { (1.5mm, 1.8mm) } else { (1.9mm, 1mm) }
    place(top + left, dx: x(v), dy: dy, line(length: len, angle: 90deg, stroke: 0.5pt + ink))
    if calc.rem(i, 4) == 0 {
      place(top + left, dx: x(v) - 3mm, dy: 4.6mm, box(width: 6mm, align(center, text(size: 6pt, fill: soft, top-edge: "cap-height", str(int(v))))))
    }
  }
})

// Cups to tick, numbered faintly
#let cup-boxes(n) = stack(dir: ltr, spacing: 1.8mm,
  ..range(n).map(i => box(width: 4.4mm, height: 4.4mm, stroke: 0.6pt + ink, radius: 0.7mm,
    align(center + horizon, text(size: 5.5pt, fill: faint, str(i + 1))))))

// A box to write a number in
#let write-box(width: 14mm, height: 6.8mm, weight: 0.7pt) = box(width: width, height: height, stroke: weight + ink, radius: 0.8mm)

// A short vertical intensity mark: a line with five ticks, named at its ends
#let vmark(top-word, bottom-word) = box(height: 6mm, width: 13mm, {
  place(top + left, dx: 1.2mm, dy: 0.3mm, line(length: 5.4mm, angle: 90deg, stroke: 0.5pt + ink))
  for k in range(5) { place(top + left, dx: 0.3mm, dy: 0.3mm + k * 1.35mm, line(length: 1.8mm, stroke: 0.4pt + ink)) }
  place(top + left, dx: 3mm, dy: -0.4mm, text(size: 5.5pt, fill: soft, top-word))
  place(top + left, dx: 3mm, dy: 3.6mm, text(size: 5.5pt, fill: soft, bottom-word))
})

#let extras = (
  fragrance_aroma: stack(dir: ltr, spacing: 1mm, vmark("dry", ""), vmark("break", "")),
  acidity: vmark("high", "low"),
  body: vmark("heavy", "thin"),
)

// One sample: its code and roast, the ten attributes in the form's order, defects, totals,
// notes; every score box in one column on the right
#let sample-block(s, attrs, cups) = block(breakable: false, width: 100%, {
  set text(size: 8.5pt)
  grid(columns: (auto, 1fr, auto), column-gutter: 5mm, align: (left + bottom, left + bottom, right + bottom),
    stack(spacing: 2pt, label-text("Sample"), text(font: "JetBrains Mono NL", size: 16pt, weight: "bold", s.code)),
    stack(spacing: 2pt,
      label-text("Roasted"),
      text(size: 9pt, if s.roasted != none [#s.roasted #text(fill: soft)[· rested #s.rest]] else [#box(width: 30mm, line(length: 100%, stroke: 0.4pt + faint))])),
    stack(dir: ltr, spacing: 2mm, align(bottom, label-text("Roast level")), write-box(width: 16mm)),
  )
  v(1mm)
  line(length: 100%, stroke: 0.6pt + ink)
  let cols = (27mm, 1fr, 30mm, 28mm)
  grid(
    columns: cols,
    row-gutter: 1.6mm,
    inset: (y: 0pt),
    align: (left + horizon, left + horizon, left + horizon, right + horizon),
    ..attrs.map(a => (
      text(weight: "bold", a.label),
      if a.kind == "cups" { cup-boxes(cups) } else { qscale(92mm) },
      extras.at(a.key, default: []),
      write-box(),
    )).flatten(),
    grid.hline(stroke: 0.4pt + grid-ink),
    text(weight: "bold")[Defects],
    stack(dir: ltr, spacing: 2.5mm,
      align(horizon, text(fill: soft)[Taint ×2]), cup-boxes(cups),
      h(3mm),
      align(horizon, text(fill: soft)[Fault ×4]), cup-boxes(cups)),
    [],
    align(right, stack(dir: ltr, spacing: 1.5mm, align(horizon, text(weight: "bold")[−]), write-box())),
    grid.hline(stroke: 0.4pt + grid-ink),
    align(top, pad(top: 1.5mm, text(fill: soft)[Notes])),
    // Two lines to write on
    stack(v(5mm), line(length: 100%, stroke: 0.4pt + faint), v(5.5mm), line(length: 100%, stroke: 0.4pt + faint)),
    align(right, stack(dir: ltr, spacing: 2mm, align(horizon)[Total], write-box(width: 14mm))),
    align(right, stack(dir: ltr, spacing: 2mm, align(horizon, text(weight: "bold", size: 9.5pt)[Final]), write-box(width: 16mm, height: 8mm, weight: 1.1pt))),
  )
})

// Six samples a page: a box for each score, for cuppers who write numbers rather than marks
#let grid-sheet(r) = {
  let q = r.attributes
  let cols = (21mm,) + (1fr,) * q.len() + (12mm, 12mm, 15mm)
  let cell(w: 100%, h: 10mm, weight: 0.6pt) = box(width: w, height: h, stroke: weight + ink, radius: 0.6mm)
  grid(columns: (1fr, auto), align: bottom,
    stack(spacing: 2pt, label-text("Cupping sheet"), text(size: 14pt, weight: "bold", r.meta.subject)),
    [Cupper #box(width: 45mm, line(length: 100%, stroke: 0.4pt + faint))])
  v(3mm)
  set text(size: 8pt)
  table(
    columns: cols,
    stroke: none,
    inset: (x: 1.2mm, y: 1.6mm),
    align: center + horizon,
    table.hline(stroke: 0.8pt + ink),
    table.header(
      align(left, text(weight: "bold")[Sample]),
      ..q.map(a => text(weight: "bold", size: 7pt, a.short + if a.kind == "cups" { "\ncups ×2" } else { "" })),
      text(weight: "bold", size: 7pt)[Total], text(weight: "bold", size: 7pt)[Defects], text(weight: "bold", size: 7.5pt)[Final],
      table.hline(stroke: 0.5pt + ink),
    ),
    ..r.samples.map(s => (
      table.cell(align: left + horizon, stack(spacing: 2pt,
        text(font: "JetBrains Mono NL", size: 13pt, weight: "bold", s.code),
        text(size: 6.5pt, fill: soft, if s.roasted != none [#s.roasted · #s.rest] else []))),
      ..q.map(a => cell()),
      cell(), cell(), cell(weight: 1.1pt),
      table.cell(colspan: q.len() + 4, align: left + bottom, inset: (x: 1.2mm, top: 1mm, bottom: 3mm),
        grid(columns: (auto, 1fr), column-gutter: 2mm, row-gutter: 7mm, align: bottom,
          text(fill: soft)[Notes], line(length: 100%, stroke: 0.4pt + faint),
          [], line(length: 100%, stroke: 0.4pt + faint))),
      table.hline(stroke: 0.4pt + grid-ink),
    )).flatten(),
  )
}

#let blank(r) = {
  let per = r.per_page
  if per > 2 {
    grid-sheet(r)
  } else {
  // A title line on every page: the session, and a line for the cupper
  let title = grid(columns: (1fr, auto), align: bottom,
    stack(spacing: 2pt, label-text("Cupping sheet"), text(size: 14pt, weight: "bold", r.meta.subject)),
    [Cupper #box(width: 45mm, line(length: 100%, stroke: 0.4pt + faint))])
  for (i, s) in r.samples.enumerate() {
    if calc.rem(i, per) == 0 {
      if i > 0 { pagebreak() }
      title
      v(3mm)
    } else {
      v(1fr)
    }
    sample-block(s, r.attributes, r.cups)
  }
  }
  // The key on a page of its own, to keep apart until the cups are scored
  if r.key.len() > 0 {
    pagebreak()
    section[The key: which sample is which][
      #text(fill: soft)[Keep this page apart until every cup is scored.]
      #v(2mm)
      #rtable(columns: (auto, 1fr, auto), align: (left, left, right), header: ([Sample], [Roast], [Roasted]),
        rows: r.key.map(k => (text(font: "JetBrains Mono NL", weight: "bold", k.code), k.label, k.roasted)), size: 10pt)
    ]
  }
  v(1fr)
  text(size: 7.5pt, fill: soft, r.protocol)
}

// A roast's mark: its batch number in a small ring
#let mark(m, size: 4.2mm) = box(width: size, height: size, radius: 50%, stroke: 0.7pt + ink, fill: white,
  align(center + horizon, text(size: 6pt, weight: "bold", m)))

// One dot strip per attribute on a shared 6–10 axis, each roast its mark; marks that would
// touch stack upward in a beeswarm and the row grows to hold them
#let dot-strips(st, width: 100%) = layout(size => {
  let lw = 32mm
  let pw = size.width - lw - 4mm
  let x(v) = lw + (v - st.min) / (st.max - st.min) * pw
  let rh = 10mm
  let step = 4.5mm
  set text(size: 8pt)
  // Each row: its dots by value, their levels, and where its axis sits
  let rows = ()
  let tall = 0mm
  for row in st.rows {
    let dots = row.dots.sorted(key: d => d.v)
    let levels = swarm(dots.map(d => x(d.v)), 4.5mm)
    let lift = calc.max(0, ..levels) * step
    rows.push((label: row.label, dots: dots, levels: levels, y: tall + rh / 2 + lift))
    tall += rh + lift
  }
  box(width: size.width, height: tall + 8mm, {
    for t in st.ticks {
      let whole = calc.fract(t) == 0
      place(top + left, dx: x(t), dy: 0pt, line(length: tall, angle: 90deg,
        stroke: if whole { 0.4pt + grid-ink } else { (paint: grid-ink, thickness: 0.3pt, dash: "dotted") }))
      if whole { place(top + left, dx: x(t) - 4mm, dy: tall + 1.5mm, box(width: 8mm, align(center, text(fill: soft, str(int(t)))))) }
    }
    for row in rows {
      let y = row.y
      place(top + left, dx: 0pt, dy: y - 1.6mm, box(width: lw - 3mm, text(weight: "bold", row.label)))
      place(top + left, dx: lw, dy: y, line(length: pw, stroke: 0.4pt + rule))
      for (k, d) in row.dots.enumerate() {
        place(top + left, dx: x(d.v) - 2.1mm, dy: y - 2.1mm - row.levels.at(k) * step, mark(d.mark))
      }
    }
  })
})

#let filled(r) = {
  grid(columns: (1fr, auto), align: bottom,
    stack(spacing: 2pt, label-text("Cupping"), text(size: 16pt, weight: "bold", r.session.date)),
    align(right, stack(spacing: 2pt, text(fill: soft, str(r.session.roasts) + " roasts · classic SCA form"), if r.session.at("best", default: none) != none { text(size: 9pt)[Best: #r.session.best] })))
  v(1mm)
  line(length: 100%, stroke: 0.8pt + ink)
  v(2mm)
  let q = r.attributes
  let cols = (1fr,) + (12.6mm,) * 7 + (17mm, 11.5mm, 6mm, 15mm)
  let quality = q.enumerate().filter(((i, a)) => a.kind == "quality")
  rtable(
    columns: cols,
    align: (left,) + (right,) * 11,
    header: ([Roast],) + quality.map(((i, a)) => a.short) + ([U CC S], [Total], [−], [Final]),
    rows: r.rows.map(row => (
      stack(spacing: 2pt, stack(dir: ltr, spacing: 1.5mm, mark(row.mark), align(horizon, text(weight: "bold", row.label))), text(size: 7pt, fill: soft, row.sub)),
      ..quality.map(((i, a)) => row.scores.at(i)),
      text(size: 7.5pt, row.cups),
      row.total, row.defects,
      stack(spacing: 2pt, text(weight: "bold", row.final), text(size: 6.5pt, fill: soft, row.band)),
    )),
    size: 8pt,
  )
  section[The seven quality scores, roast by roast][
    #dot-strips(r.strip)
  ]
  section[Notes][
    #for row in r.rows.filter(row => row.notes != none) {
      grid(columns: (6mm, 1fr), column-gutter: 2mm, mark(row.mark), [#text(weight: "bold", row.label) #h(2mm) #row.notes])
      v(1.5mm)
    }
  ]
  v(1fr)
  text(size: 7.5pt, fill: soft, r.protocol)
}

#let report(r) = page-frame(r, margin: if r.mode == "blank" { (left: 16mm, right: 16mm, top: 14mm, bottom: 16mm) } else { (left: 18mm, right: 18mm, top: 20mm, bottom: 18mm) }, {
  if r.mode == "blank" { blank(r) } else { filled(r) }
})
