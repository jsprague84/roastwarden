// Green inventory (docs/reports.md): what is in stock, what it is worth, when each runs out;
// or a stock-take sheet to weigh against.
#import "report.typ": *

#let design = (
  name: "Green inventory",
  description: "The green coffee in stock: kg, cost per kg, value, age, use, and when each runs out at that pace; or a stock-take sheet to weigh against.",
)

#let title-block(r, label, title, side) = {
  grid(columns: (1fr, auto), align: (left + bottom, right + bottom),
    stack(spacing: 3.5pt, label-text(label), text(size: 20pt, weight: "bold", tracking: -0.01em, title)),
    side)
  v(1mm)
  line(length: 100%, stroke: 0.8pt + ink)
  v(3mm)
}

#let stock-report(r) = {
  title-block(r, "Green inventory", r.as_of, text(fill: soft, "weights in kg · money in " + r.meta.currency))
  figures(r.summary)
  v(5mm)
  rtable(
    columns: (1fr, auto, auto, auto, auto, auto, auto, auto, auto),
    align: (left, left, right, right, right, right, right, right, right),
    header: ([Coffee], [Bought], [Crop], [Stock kg], [Planned], [Cost/kg], [Value], [Use], [Runs out]),
    rows: r.rows.map(x => (
      stack(spacing: 2pt, text(weight: "bold", x.coffee), text(size: 7pt, fill: soft, x.sub)),
      stack(spacing: 2pt, x.bought, text(size: 7pt, fill: if x.age_mark != none { palette.accent } else { soft },
        x.age + if x.age_mark != none { " · " + x.age_mark } else { "" })),
      x.crop,
      if x.low { text(fill: palette.accent, weight: "bold", "▼ " + x.stock) } else { x.stock },
      x.planned, x.cost_kg, x.value, x.use,
      text(weight: if x.runs_out != "–" { "bold" } else { "regular" }, x.runs_out),
    )),
    footer: (text(weight: "bold")[Total], [], [], text(weight: "bold", r.totals.stock), r.totals.planned, [], text(weight: "bold", r.totals.value), [], []),
    size: 8.5pt,
  )
  v(2mm)
  text(size: 7.5pt, fill: soft, r.notes)
}

#let count-sheet(r) = {
  title-block(r, "Stock-take", r.as_of, grid(columns: 2, column-gutter: 4mm, row-gutter: 3mm,
    [Date], box(width: 32mm, line(length: 100%, stroke: 0.4pt + faint)),
    [Counted by], box(width: 32mm, line(length: 100%, stroke: 0.4pt + faint))))
  text(size: 9pt, fill: soft)[Weigh each coffee's green (tare the bag or tin) and write the grams; the difference is counted − in the app.]
  v(3mm)
  let write = box(width: 100%, height: 7.5mm, stroke: 0.7pt + ink, radius: 0.8mm)
  set text(size: 9pt)
  table(
    columns: (1fr, auto, 26mm, 24mm, 36mm),
    align: (left + horizon, right + horizon, center + horizon, center + horizon, left + horizon),
    stroke: none,
    inset: (x: 4pt, y: 2.4mm),
    table.hline(stroke: 0.8pt + ink),
    table.header(text(weight: "bold")[Coffee], text(weight: "bold")[In the app (g)], text(weight: "bold")[Counted (g)], text(weight: "bold")[Difference], text(weight: "bold")[Note], table.hline(stroke: 0.5pt + ink)),
    ..r.rows.map(x => (
      stack(spacing: 2pt, text(weight: "bold", x.coffee), text(size: 7pt, fill: soft, x.sub)),
      x.grams, write, write, align(bottom, line(length: 100%, stroke: 0.4pt + faint)),
      table.hline(stroke: 0.4pt + grid-ink),
    )).flatten(),
  )
  v(2mm)
  text(size: 7.5pt, fill: soft, r.notes)
}

#let report(r) = page-frame(r, {
  if r.mode == "count" { count-sheet(r) } else { stock-report(r) }
})
