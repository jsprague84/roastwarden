// The compare report (docs/reports.md): two roasts as an A/B experiment on one page, from
// `roast compare`'s numbers. A is the roast B is measured against.
#import "report.typ": *

#let design = (
  name: "Compare two roasts",
  description: "Two roasts as an A/B experiment: BT and RoR of both on one chart with heater and fan, the phases on one time scale, every number side by side with B − A.",
)

// One roast's head: its letter in its colour, its name, coffee, date and profile
#let who(x) = grid(columns: (auto, 1fr), column-gutter: 2.5mm, align: (left + top, left + top),
  box(width: 7mm, height: 7mm, radius: 1mm, fill: palette.at(lower(x.mark)),
    align(center + horizon, text(size: 13pt, weight: "bold", fill: white, x.mark))),
  stack(spacing: 2.5pt,
    text(size: 11pt, weight: "bold", x.label + if x.name != x.label { " · " + x.name } else { "" }),
    text(size: 8pt, fill: soft, (x.coffee, x.date).filter(v => v != none and v != "" and v != x.name).join(" · ")),
    text(size: 8pt, fill: soft, (x.profile, x.green, if x.charge_bt != "–" { "charged at " + x.charge_bt }, x.roaster).filter(v => v != none and v != "").join(" · "))))

// Every number of one part of the roast: A, B and B − A
#let numbers(rows) = rtable(
  columns: (1fr, auto, auto, auto),
  align: (left, right, right, right),
  header: ([], [A], [B], [B − A]),
  rows: rows.map(r => (
    [#r.label#if r.unit != "" { text(fill: soft)[ #r.unit] }],
    r.a, r.b,
    if r.diff == "same" or r.diff == "–" { text(fill: soft, r.diff) } else { text(weight: "bold", r.diff) },
  )),
  size: 7.6pt,
)

#let report(r) = page-frame(r, {
  label-text("Compare two roasts")
  v(1.5mm)
  grid(columns: (1fr, 1fr), column-gutter: 6mm, who(r.roasts.at(0)), who(r.roasts.at(1)))
  v(1.5mm)
  line(length: 100%, stroke: 0.8pt + ink)
  v(2.5mm)
  figures(r.summary)
  v(1.5mm)

  figure(pdf.artifact(roast-chart(r.chart, height: 60mm)), alt: r.chart.alt)
  v(-1mm)
  for s in r.strips { strip-chart(r.chart, s.series, height: 9mm, title: s.title) }
  if r.overlay != "" {
    v(0.5mm)
    text(size: 8pt, r.overlay)
  }
  v(1.5mm)
  if r.phases.any(p => p.parts.len() > 0) { phase-bars(r.phases) }
  v(1mm)

  grid(columns: (1fr, 1fr), column-gutter: 6mm,
    numbers(r.start),
    numbers(r.rest),
  )

  // What each roast was for, as its notes say
  let noted = r.roasts.filter(x => x.notes != none)
  if noted.len() > 0 {
    section(breakable: true)[Notes][
      #set text(size: 8pt)
      #for x in noted {
        block(above: 1.2mm, grid(columns: (5mm, 1fr), column-gutter: 1mm,
          text(weight: "bold", fill: palette.at(lower(x.mark)), x.mark), x.notes))
      }
      #v(1mm)
      #text(size: 7.5pt, fill: soft, r.notes)
    ]
  } else {
    v(1mm)
    text(size: 7.5pt, fill: soft, r.notes)
  }
})
