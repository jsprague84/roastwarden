// Helpers for report designs (docs/reports.md). Import what you use:
//   #import "report.typ": page-frame, section, figures, rtable, roast-chart, strip-chart, phase-bar
// Every number arrives formatted (Rust does the arithmetic); designs lay out.

// Ink and the print palette: Okabe-Ito, a step darker than the dashboard's for paper
#let ink = luma(12%)
#let soft = luma(38%)
#let faint = luma(62%)
#let rule = luma(55%)
#let grid-ink = luma(84%)
#let tint = luma(95%)
#let palette = (
  bt: rgb("#D55E00"),
  et: rgb("#0072B2"),
  ror: rgb("#009E73"),
  profile: luma(45%),
  heater: rgb("#CC79A7"),
  fan: rgb("#56B4E9"),
  accent: rgb("#D55E00"),
  // Two roasts compared: A blue, B vermilion (the change), each roast's RoR in its colour
  a: rgb("#0072B2"),
  b: rgb("#D55E00"),
  a_ror: rgb("#0072B2"),
  b_ror: rgb("#D55E00"),
  // A roaster's declared channels (docs/channels.md), in declaration order
  ch0: rgb("#E69F00"),
  ch1: rgb("#882255"),
  ch2: rgb("#44AA99"),
  ch3: rgb("#999933"),
  ch4: rgb("#AA4499"),
  ch5: rgb("#332288"),
  ch6: rgb("#117733"),
  ch7: rgb("#661100"),
)
// Each series its own dash, so the chart reads in greyscale too
#let dashes = (bt: none, et: "dashed", ror: "dash-dotted", profile: "densely-dotted", heater: none, fan: "dashed",
  a: none, b: "dashed", a_ror: "dotted", b_ror: "dash-dotted",
  ch0: "loosely-dashed", ch1: "dotted", ch2: "loosely-dash-dotted", ch3: "densely-dashed",
  ch4: "loosely-dashed", ch5: "dotted", ch6: "loosely-dash-dotted", ch7: "densely-dashed")
#let weights = (bt: 1.3pt, et: 0.9pt, ror: 0.9pt, profile: 0.9pt, heater: 0.8pt, fan: 0.8pt,
  a: 1.2pt, b: 1.2pt, a_ror: 0.9pt, b_ror: 0.8pt)

// Small capitals for labels above values
#let label-text(body) = text(size: 7pt, fill: soft, tracking: 0.06em, upper(body))

// The page: paper, margins, type, a header from page 2 (subject left, kind right) and a
// footer (Roastwarden and when it was made left, "Page n of N" right). A form to write on
// may take narrower margins.
#let page-frame(r, margin: (left: 18mm, right: 18mm, top: 20mm, bottom: 18mm), body) = {
  let m = r.meta
  set page(
    paper: m.paper,
    margin: margin,
    header: context {
      if counter(page).get().first() > 1 {
        set text(size: 8pt, fill: soft)
        grid(columns: (1fr, auto), m.subject, m.title)
        v(-2pt)
        line(length: 100%, stroke: 0.4pt + grid-ink)
      }
    },
    footer: context {
      set text(size: 7.5pt, fill: soft)
      grid(columns: (1fr, auto),
        [#box(image("brand/mark.svg", height: 8pt), baseline: 1.5pt) Roastwarden #m.version · made #m.made],
        [Page #counter(page).display("1 of 1", both: true)])
    },
  )
  set text(font: "Atkinson Hyperlegible Next", size: 9.5pt, fill: ink, number-width: "tabular",
    lang: m.at("lang", default: "en"))
  set par(leading: 0.55em, spacing: 0.9em)
  show raw: set text(font: "JetBrains Mono NL", size: 0.92em)
  show heading: set text(size: 10.5pt, weight: "bold")
  show heading: set block(above: 1.1em, below: 0.6em)
  body
}

// A section: its title with a hairline under it, kept on one page with what follows. A long
// one (`breakable: true`) may run over a page, its title kept with the first lines.
#let section(title, body, breakable: false) = block(above: 1.1em, breakable: breakable, {
  block(below: 0.5em, sticky: true, {
    text(size: 10pt, weight: "bold", title)
    v(-0.45em)
    line(length: 100%, stroke: 0.4pt + grid-ink)
  })
  body
})

// Key figures in a grid: (label, value, sub, range, state, note). `state` is "within",
// "below", "above" or none; outside a range the range is set in the accent colour, with
// "above" or "below" before it.
#let figures(items, columns: 4) = {
  set text(top-edge: "ascender", bottom-edge: "descender")
  grid(
  columns: (1fr,) * columns,
  column-gutter: 5mm,
  row-gutter: 2.6mm,
  ..items.map(f => {
    let out = f.at("state", default: none) in ("below", "above")
    stack(spacing: 1pt,
      label-text(f.label),
      text(size: if f.at("small", default: false) { 10pt } else { 12.5pt }, weight: "bold", f.value),
      if f.at("sub", default: none) != none { text(size: 8pt, fill: soft, f.sub) },
      if f.at("range", default: none) != none {
        text(size: 7.5pt, fill: if out { palette.accent } else { soft },
          (if out { if f.state == "below" { "below " } else { "above " } } else { "" }) + f.range)
      },
      if f.at("note", default: none) != none { text(size: 7.5pt, fill: soft, f.note) },
    )
  }),
)
}

// A table on booktabs lines: rules above and below, a thin one under the header (repeated
// on every page), no vertical rules, no fills. `align` per column; numbers right.
#let rtable(columns: (), align: (), header: (), rows: (), footer: none, size: 8.5pt) = {
  set text(size: size)
  table(
    columns: columns,
    align: align,
    stroke: none,
    inset: (x: 4pt, y: 3.2pt),
    table.hline(stroke: 0.8pt + ink),
    table.header(..header.map(h => text(weight: "bold", h)), table.hline(stroke: 0.5pt + ink)),
    ..rows.flatten(),
    ..if footer != none { (table.hline(stroke: 0.5pt + ink), ..footer) },
    table.hline(stroke: 0.8pt + ink),
  )
}

// x and y in data units to a point in the plot box of width pw and height ph
#let _sx(ax, v, pw) = (v - ax.min) / (ax.max - ax.min) * pw
#let _sy(ax, v, ph) = ph - (v - ax.min) / (ax.max - ax.min) * ph

#let _path(points, ax-x, ax-y, pw, ph, stroke) = {
  if points.len() < 2 { return none }
  let p0 = points.first()
  curve(stroke: stroke,
    curve.move((_sx(ax-x, p0.at(0), pw), _sy(ax-y, p0.at(1), ph))),
    ..points.slice(1).map(p => curve.line((_sx(ax-x, p.at(0), pw), _sy(ax-y, p.at(1), ph)))))
}

#let _stroke(key, colour: auto) = {
  let paint = if colour == auto { palette.at(key, default: ink) } else { colour }
  let d = dashes.at(key, default: none)
  if d == none { (paint: paint, thickness: weights.at(key, default: 0.9pt), cap: "round", join: "round") }
  else { (paint: paint, thickness: weights.at(key, default: 0.9pt), dash: d, cap: "round", join: "round") }
}

// Positions pushed apart so labels never sit on each other: each at least `gap` after the
// one before, then pulled back inside `hi` (and never before `lo`). `vs` sorted ascending.
#let spread(vs, gap, lo: none, hi: none) = {
  let out = ()
  for v in vs {
    let at = if lo != none { calc.max(v, lo) } else { v }
    if out.len() > 0 and at - out.last() < gap { at = out.last() + gap }
    out.push(at)
  }
  if hi != none and out.len() > 0 and out.last() > hi {
    out.last() = hi
    for k in range(out.len() - 2, -1, step: -1) {
      if out.at(k + 1) - out.at(k) < gap { out.at(k) = out.at(k + 1) - gap }
    }
  }
  out
}

// A beeswarm's levels for marks on a strip: each mark (by x, ascending) takes the lowest level
// (0 on the axis, then upward) where no mark already placed is within `gap` of it.
#let swarm(xs, gap) = {
  let placed = ()
  let levels = ()
  for x in xs {
    let k = 0
    while placed.any(p => p.at(1) == k and calc.abs(p.at(0) - x) < gap) { k += 1 }
    placed.push((x, k))
    levels.push(k)
  }
  levels
}

// The roast chart: temperature on the left axis, rate of rise on the right, minutes along the
// bottom; phase bands as light tints, events as dotted lines named at the top, and each series
// named at its end instead of a legend. `c` is the report's `chart`.
#let roast-chart(c, height: 76mm) = layout(size => {
  let w = size.width
  let (pl, pr, pt, pb) = (9mm, 9mm, 7mm, 7mm)
  let pw = w - pl - pr
  let ph = height - pt - pb
  let x = c.x
  let y = c.y
  let y2 = c.at("y2", default: none)
  let px(v) = pl + _sx(x, v, pw)
  let py(ax, v) = pt + _sy(ax, v, ph)
  set text(size: 7pt, fill: soft)
  box(width: w, height: height, {
    // Phase bands
    for (i, b) in c.at("bands", default: ()).enumerate() {
      let x0 = px(calc.max(b.from, x.min))
      let x1 = px(calc.min(b.to, x.max))
      if b.key == "development" or b.key == "drying" {
        place(top + left, dx: x0, dy: pt, rect(width: x1 - x0, height: ph, fill: tint, stroke: none))
      }
      place(top + left, dx: x0, dy: pt + ph - 9pt, box(width: x1 - x0, align(center, text(size: 6.5pt, fill: faint, b.label))))
    }
    // Grid and ticks
    for t in y.ticks {
      place(top + left, dx: pl, dy: py(y, t.v), line(length: pw, stroke: 0.35pt + grid-ink))
      place(top + left, dx: 0pt, dy: py(y, t.v) - 4pt, box(width: pl - 2mm, align(right, t.label)))
    }
    if y2 != none {
      for t in y2.ticks {
        place(top + left, dx: pl + pw + 2mm, dy: py(y2, t.v) - 4pt, text(fill: palette.ror, t.label))
      }
    }
    for t in x.ticks {
      place(top + left, dx: px(t.v), dy: pt + ph, line(length: 1.5mm, angle: 90deg, stroke: 0.4pt + rule))
      place(top + left, dx: px(t.v) - 5mm, dy: pt + ph + 2mm, box(width: 10mm, align(center, t.label)))
    }
    place(top + left, dx: pl, dy: pt + ph, line(length: pw, stroke: 0.5pt + rule))
    // Axis names
    place(top + left, dx: 0pt, dy: 0pt, c.at("y-name", default: ""))
    if y2 != none { place(top + right, dx: 0pt, dy: 0pt, text(fill: palette.ror, c.at("y2-name", default: ""))) }
    place(bottom + right, dx: 0pt, dy: 0pt, c.at("x-name", default: "min"))
    // Events: a dotted line and the short name at the top, names pushed apart when close
    let evs = c.at("events", default: ()).filter(e => e.key != "charge").sorted(key: e => e.t)
    let names = spread(evs.map(e => px(e.t)), 9mm, lo: pl, hi: pl + pw)
    for (k, e) in evs.enumerate() {
      let ex = px(e.t)
      // An event of one roast of several (`of`: its series) in that roast's colour
      let of = e.at("of", default: none)
      let paint = if of != none { palette.at(of, default: faint) } else { faint }
      place(top + left, dx: ex, dy: pt, line(length: ph, angle: 90deg, stroke: (paint: paint, thickness: 0.4pt, dash: "dotted")))
      place(top + left, dx: names.at(k) - 8mm, dy: pt - 4.2mm, box(width: 16mm, align(center, text(fill: if of != none { paint } else { ink }, size: 6.5pt, weight: "bold", e.short))))
    }
    // Series, clipped to the plot
    place(top + left, dx: pl, dy: pt, box(width: pw, height: ph, clip: true, {
      for s in c.series {
        let ax = if s.axis == "y2" { y2 } else { y }
        place(top + left, _path(s.points, x, ax, pw, ph, _stroke(s.key)))
      }
    }))
    // Names at the end of each line, pushed apart so they never sit on each other
    let ends = c.series.filter(s => s.points.len() > 0).map(s => {
      let ax = if s.axis == "y2" { y2 } else { y }
      let p = s.points.last()
      (s: s, x: px(p.at(0)), y: calc.max(pt, calc.min(pt + ph, py(ax, p.at(1)))))
    }).sorted(key: e => e.y)
    let ys = spread(ends.map(e => e.y), 7.5pt)
    for (k, e) in ends.enumerate() {
      place(top + left, dx: e.x + 1mm, dy: ys.at(k) - 3.5pt,
        text(size: 6.5pt, weight: "bold", fill: palette.at(e.s.key, default: ink), e.s.label))
    }
  })
})

// A short 0–100 % strip under the chart (heater and fan), on the chart's time axis. Each
// line is named at its end (`tag`, else its label and "%"); `title` names the strip itself.
#let strip-chart(c, series, height: 13mm, title: none) = layout(size => {
  let w = size.width
  let (pl, pr) = (9mm, 9mm)
  let pw = w - pl - pr
  let ph = height - 2mm
  let ax = (min: 0, max: 100)
  set text(size: 6.5pt, fill: soft)
  box(width: w, height: height, {
    place(top + left, dx: pl, dy: 0pt, line(length: pw, stroke: 0.3pt + grid-ink))
    place(top + left, dx: pl, dy: ph, line(length: pw, stroke: 0.4pt + rule))
    place(top + left, dx: 0pt, dy: -3pt, box(width: pl - 2mm, align(right, "100")))
    place(top + left, dx: 0pt, dy: ph - 4pt, box(width: pl - 2mm, align(right, "0")))
    if title != none { place(top + left, dx: pl + 1mm, dy: 0.6mm, text(weight: "bold", title)) }
    place(top + left, dx: pl, dy: 0pt, box(width: pw, height: ph, clip: true, {
      for s in series { place(top + left, _path(s.points, c.x, ax, pw, ph, _stroke(s.key))) }
    }))
    // Names at the end of each line, pushed apart (heater and fan often end close)
    let ends = series.filter(s => s.points.len() > 0).map(s => (s: s, y: _sy(ax, s.points.last().at(1), ph))).sorted(key: e => e.y)
    let ys = spread(ends.map(e => e.y), 7pt, lo: 0pt, hi: ph + 2mm)
    for (k, e) in ends.enumerate() {
      place(top + left, dx: pl + pw + 1mm, dy: ys.at(k) - 3.5pt,
        text(weight: "bold", fill: palette.at(e.s.key, default: ink), e.s.at("tag", default: e.s.label + " %")))
    }
  })
})

// The roast's phases as one bar: each phase's share of the roast, its name, time and %
#let phase-bar(phases, height: 5.5mm) = {
  let fills = (drying: tint, maillard: white, development: luma(88%))
  grid(columns: phases.map(p => p.frac * 1fr), rows: (height, auto), column-gutter: 0pt, row-gutter: 2pt,
    ..phases.map(p => rect(width: 100%, height: height, fill: fills.at(p.key, default: white), stroke: 0.5pt + rule)),
    ..phases.map(p => align(center, text(size: 7.5pt)[#p.name #h(3pt) #text(fill: soft)[#p.time · #p.pct %]])))
}

// Label–value pairs in columns, for small facts
#let facts(items, columns: 2) = grid(
  columns: (auto, 1fr) * columns, column-gutter: (3mm, 8mm) * columns, row-gutter: 2.4mm,
  ..items.map(f => (text(fill: soft, f.label), f.value)).flatten())

// Small multiples: one small BT chart per roast on the same axes, so they compare like with
// like. `m` has `x` and `y` axes and `panels`: (title, sub, points, marks: ((t, label),)).
#let small-multiples(m, columns: 3, height: 34mm) = {
  // A panel keeps its title with its chart; rows of panels may run over a page
  let panel(p) = block(breakable: false, layout(size => {
    let w = size.width
    let (pl, pb, pt) = (6mm, 4mm, 1mm)
    let pw = w - pl - 1mm
    let ph = height - pb - pt
    set text(size: 6pt, fill: soft)
    stack(spacing: 1.2mm,
      text(size: 7.5pt, fill: ink, weight: "bold", p.title) + h(1.5mm) + text(size: 6.5pt, p.sub),
      box(width: w, height: height, {
        for t in m.y.ticks {
          place(top + left, dx: pl, dy: pt + _sy(m.y, t.v, ph), line(length: pw, stroke: 0.3pt + grid-ink))
          place(top + left, dx: 0pt, dy: pt + _sy(m.y, t.v, ph) - 3pt, box(width: pl - 1mm, align(right, t.label)))
        }
        for t in m.x.ticks {
          place(top + left, dx: pl + _sx(m.x, t.v, pw) - 3mm, dy: pt + ph + 0.8mm, box(width: 6mm, align(center, t.label)))
        }
        place(top + left, dx: pl, dy: pt + ph, line(length: pw, stroke: 0.4pt + rule))
        for k in p.marks {
          let x = pl + _sx(m.x, k.t, pw)
          place(top + left, dx: x, dy: pt, line(length: ph, angle: 90deg, stroke: (paint: faint, thickness: 0.3pt, dash: "dotted")))
          place(top + left, dx: x + 0.6mm, dy: pt, text(size: 5.5pt, fill: soft, k.label))
        }
        place(top + left, dx: pl, dy: pt, box(width: pw, height: ph, clip: true,
          _path(p.points, m.x, m.y, pw, ph, _stroke("bt"))))
      }))
  }))
  grid(columns: (1fr,) * columns, column-gutter: 5mm, row-gutter: 4mm, ..m.panels.map(panel))
}

// Several BT curves on one chart, aligned at charge: every roast thin and light grey, the
// newest in the BT colour, the profile dark grey and dashed; the named ones labelled at the
// plot's right edge, pushed apart. `c` as the roast chart's; with `y2` the roasts' rate of
// rise too (series on `axis: "y2"`, key `ror`: the newest in the RoR colour, the rest faint),
// its ticks right of the plot.
#let overlay-chart(c, height: 70mm) = layout(size => {
  let w = size.width
  let y2 = c.at("y2", default: none)
  let (pl, pr, pt, pb) = (9mm, if y2 != none { 23mm } else { 16mm }, 3mm, 7mm)
  let pw = w - pl - pr
  let ph = height - pt - pb
  let px(v) = pl + _sx(c.x, v, pw)
  let ax(s) = if s.at("axis", default: "y") == "y2" { y2 } else { c.y }
  let py(s, v) = pt + _sy(ax(s), v, ph)
  // Names sit right of the plot, past the RoR ticks when there are any
  let name-x = pl + pw + if y2 != none { 8mm } else { 1mm }
  set text(size: 7pt, fill: soft)
  box(width: w, height: height, {
    for t in c.y.ticks {
      place(top + left, dx: pl, dy: pt + _sy(c.y, t.v, ph), line(length: pw, stroke: 0.35pt + grid-ink))
      place(top + left, dx: 0pt, dy: pt + _sy(c.y, t.v, ph) - 4pt, box(width: pl - 2mm, align(right, t.label)))
    }
    if y2 != none {
      for t in y2.ticks {
        place(top + left, dx: pl + pw + 1.5mm, dy: pt + _sy(y2, t.v, ph) - 4pt, text(fill: palette.ror, t.label))
      }
      place(top + left, dx: pl + pw + 1.5mm, dy: -2mm, text(fill: palette.ror, c.at("y2-name", default: "")))
    }
    for t in c.x.ticks {
      place(top + left, dx: px(t.v) - 5mm, dy: pt + ph + 2mm, box(width: 10mm, align(center, t.label)))
    }
    place(top + left, dx: pl, dy: pt + ph, line(length: pw, stroke: 0.5pt + rule))
    place(top + left, dx: 0pt, dy: -2mm, c.at("y-name", default: ""))
    place(bottom + right, dx: 0pt, dy: 0pt, c.at("x-name", default: "min"))
    // Events of the newest roast: dotted lines named at the top
    for e in c.at("events", default: ()) {
      place(top + left, dx: px(e.t), dy: pt, line(length: ph, angle: 90deg, stroke: (paint: faint, thickness: 0.4pt, dash: "dotted")))
      place(top + left, dx: px(e.t) + 0.6mm, dy: pt, text(size: 6pt, fill: soft, e.short))
    }
    // The older roasts first, so the newest and the profile draw over them
    let order(s) = if s.key == "profile" or s.at("newest", default: false) { 1 } else { 0 }
    place(top + left, dx: pl, dy: pt, box(width: pw, height: ph, clip: true, {
      for s in c.series.sorted(key: order) {
        let newest = s.at("newest", default: false)
        let stroke = if s.key == "profile" { (paint: palette.profile, thickness: 1.1pt, dash: "dashed") }
          else if s.key == "ror" and newest { (paint: palette.ror, thickness: 0.9pt, dash: "dash-dotted") }
          else if s.key == "ror" { (paint: luma(76%), thickness: 0.45pt) }
          else if newest { (paint: palette.bt, thickness: 1.2pt) }
          else { (paint: luma(62%), thickness: 0.6pt) }
        place(top + left, _path(s.points, c.x, ax(s), pw, ph, stroke))
      }
    }))
    let named = c.series.filter(s => s.at("label", default: none) != none and s.points.len() > 0)
    let ends = named.map(s => (s: s, y: calc.max(pt, calc.min(pt + ph, py(s, s.points.last().at(1)))))).sorted(key: e => e.y)
    let ys = spread(ends.map(e => e.y), 7pt, lo: pt, hi: pt + ph)
    for (k, e) in ends.enumerate() {
      let newest = e.s.at("newest", default: false)
      let fill = if e.s.key == "profile" { palette.profile } else if e.s.key == "ror" and newest { palette.ror } else if newest { palette.bt } else { soft }
      // A faint lead from the line's end to its name at the edge
      let ex = px(e.s.points.last().at(0))
      if ex < name-x - 1mm {
        place(top + left, dx: ex, dy: e.y, line(start: (0pt, 0pt), end: (name-x - 0.4mm - ex, ys.at(k) - e.y), stroke: (paint: fill, thickness: 0.3pt, dash: "dotted")))
      }
      place(top + left, dx: name-x, dy: ys.at(k) - 3.5pt, text(size: 6.5pt, weight: "bold", fill: fill, e.s.label))
    }
  })
})

// Phases of several roasts, one bar each on the same time scale, so a longer roast is a
// longer bar: `rows` ((mark, total, parts: ((key, name, time, pct, frac),), rest),), where
// `frac` is the phase's share of the longest roast and `rest` what is left of it.
#let phase-bars(rows, height: 5mm) = {
  let fills = (drying: tint, maillard: white, development: luma(88%))
  set text(size: 7pt)
  grid(columns: (8mm, 1fr, auto), column-gutter: 2mm, row-gutter: 1.6mm, align: horizon,
    ..rows.map(r => (
      text(size: 8pt, weight: "bold", r.mark),
      {
        let cols = r.parts.map(p => p.frac * 1fr) + if r.rest > 0.0005 { (r.rest * 1fr,) } else { () }
        grid(columns: cols, column-gutter: 0pt,
          ..r.parts.map(p => rect(width: 100%, height: height, fill: fills.at(p.key, default: white), stroke: 0.5pt + rule,
            align(center + horizon, text(size: 6.5pt)[#p.name #text(fill: soft)[#p.time]]))),
          ..if r.rest > 0.0005 { ([],) } else { () })
      },
      text(fill: soft, r.total),
    )).flatten())
}

// A dot strip per measure, each on its own axis: every roast a ring with its mark, the
// newest filled; rings that would touch stack upward in a beeswarm, clear of the tick labels
// under the axis, and the row grows to hold them. `rows`:
// (label, unit, axis: (min, max, ticks), dots: ((mark, v, newest),)).
#let measure-strips(rows, label-width: 30mm, row-height: 12mm) = layout(size => {
  let pw = size.width - label-width - 6mm
  let step = 4.3mm
  set text(size: 7.5pt)
  for row in rows {
    let ax = row.axis
    let x(v) = label-width + _sx(ax, v, pw)
    let dots = row.dots.sorted(key: d => d.v)
    let levels = swarm(dots.map(d => x(d.v)), 4.4mm)
    let top-level = calc.max(0, ..levels)
    box(width: size.width, height: row-height + top-level * step, {
      let y = row-height / 2 - 1mm + top-level * step
      place(top + left, dx: 0pt, dy: y - 2mm, box(width: label-width - 3mm,
        stack(spacing: 1.5pt, text(weight: "bold", row.label), text(size: 6.5pt, fill: soft, row.unit))))
      place(top + left, dx: label-width, dy: y, line(length: pw, stroke: 0.4pt + rule))
      for t in ax.ticks {
        place(top + left, dx: x(t.v), dy: y - 0.8mm, line(length: 1.6mm, angle: 90deg, stroke: 0.4pt + rule))
        place(top + left, dx: x(t.v) - 5mm, dy: y + 2.4mm, box(width: 10mm, align(center, text(size: 6pt, fill: soft, t.label))))
      }
      for (k, d) in dots.enumerate() {
        let newest = d.at("newest", default: false)
        place(top + left, dx: x(d.v) - 2mm, dy: y - 2mm - levels.at(k) * step,
          box(width: 4mm, height: 4mm, radius: 50%, stroke: 0.7pt + ink, fill: if newest { ink } else { white },
            align(center + horizon, text(size: 5.5pt, weight: "bold", fill: if newest { white } else { ink }, d.mark))))
      }
    })
  }
})

// Bars, one per item, value printed above: months of green roasted, say. `items`:
// ((label, v, text),); the bars are outlines with a light tint, to spare toner.
#let bars(items, height: 32mm, unit: "") = layout(size => {
  let n = items.len()
  if n == 0 { return }
  let hi = calc.max(..items.map(x => x.v), 0.001)
  let gap = 2mm
  let bw = (size.width - gap * (n - 1)) / n
  let ph = height - 9mm
  set text(size: 6.5pt, fill: soft)
  box(width: size.width, height: height, {
    place(top + left, dy: ph + 4mm, line(length: size.width, stroke: 0.5pt + rule))
    for (k, x) in items.enumerate() {
      let h = x.v / hi * ph
      let dx = k * (bw + gap)
      if h > 0pt {
        place(top + left, dx: dx, dy: ph + 4mm - h, rect(width: bw, height: h, fill: tint, stroke: 0.6pt + ink))
      }
      place(top + left, dx: dx, dy: ph + 4mm - h - 3.6mm, box(width: bw, align(center, text(fill: ink, x.text))))
      place(top + left, dx: dx, dy: ph + 5.2mm, box(width: bw, align(center, x.label)))
    }
    if unit != "" { place(top + right, text(unit)) }
  })
})

// Dots over time: one per purchase at its date and value (landed cost per kg, say).
// `c` has `x` (days, month ticks), `y` and `dots`: ((x, y, label),).
#let dots-over-time(c, height: 42mm) = layout(size => {
  let w = size.width
  let (pl, pt, pb) = (14mm, 2mm, 6mm)
  let pw = w - pl - 2mm
  let ph = height - pt - pb
  set text(size: 6.5pt, fill: soft)
  box(width: w, height: height, {
    for t in c.y.ticks {
      place(top + left, dx: pl, dy: pt + _sy(c.y, t.v, ph), line(length: pw, stroke: 0.3pt + grid-ink))
      place(top + left, dx: 0pt, dy: pt + _sy(c.y, t.v, ph) - 3pt, box(width: pl - 2mm, align(right, t.label)))
    }
    for t in c.x.ticks {
      place(top + left, dx: pl + _sx(c.x, t.v, pw), dy: pt + ph, line(length: 1.4mm, angle: 90deg, stroke: 0.4pt + rule))
      place(top + left, dx: pl + _sx(c.x, t.v, pw) - 5mm, dy: pt + ph + 1.8mm, box(width: 10mm, align(center, t.label)))
    }
    place(top + left, dx: pl, dy: pt + ph, line(length: pw, stroke: 0.5pt + rule))
    for d in c.dots {
      place(top + left, dx: pl + _sx(c.x, d.x, pw) - 1.3mm, dy: pt + _sy(c.y, d.y, ph) - 1.3mm,
        circle(radius: 1.3mm, fill: palette.accent, stroke: 0.5pt + white))
    }
  })
})
