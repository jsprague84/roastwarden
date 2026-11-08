// The period report (docs/reports.md): a month, a range or a year in review: the roasts with
// a sum row, green roasted and bought, each coffee's part, and the stock at the end.
#import "report.typ": *

#let design = (
  name: "Period report",
  description: "A month, a range or a year in review: roasts with a sum row, green roasted and bought, spend and landed cost, each coffee's part and the stock at the end.",
)

#let report(r) = page-frame(r, {
  let p = r.period
  stack(spacing: 3.5pt,
    label-text(if p.year != none [Year in review] else [Roasting]),
    text(size: 22pt, weight: "bold", tracking: -0.01em, p.title),
    text(size: 9pt, fill: soft, p.span))
  v(1mm)
  line(length: 100%, stroke: 0.8pt + ink)
  v(3mm)
  figures(r.summary)
  if r.highlights.len() > 0 {
    v(4mm)
    facts(r.highlights, columns: 2)
  }
  if r.by_month.len() > 0 {
    section[Green roasted, month by month][
      #bars(r.by_month.map(m => (label: m.label, v: m.green_kg, text: if m.roasts > 0 { m.green } else { "" })), unit: "kg")
      #v(2mm)
      #rtable(
        columns: (1fr, auto, auto, auto, auto),
        align: (left, right, right, right, right),
        header: ([Month], [Roasts], [Green kg], [Bought kg], [Spent]),
        // A run of empty months is one line
        rows: r.month_rows.map(m => if m.empty {
          (text(fill: soft, m.label), table.cell(colspan: 4, align: right, text(fill: soft)[nothing roasted or bought]))
        } else { (m.label, str(m.roasts), m.green, m.bought, m.spent) }),
        size: 8pt,
      )
    ]
  }
  if r.coffees.len() > 0 {
    section[Each coffee][
      #rtable(
        columns: (1fr, auto, auto, auto, auto, auto, auto, auto),
        align: (left,) + (right,) * 7,
        header: ([Coffee], [Roasts], [Used kg], [Bought kg], [Spent], [Landed/kg], [Stock kg], [Worth]),
        rows: r.coffees.map(c => (c.coffee, str(c.roasts), c.used, c.bought, c.spent, c.per_kg, c.stock, c.value)),
        size: 8pt,
      )
    ]
  }
  if r.roasts.len() > 0 {
    section[The roasts][
      #rtable(
        columns: (auto, auto, 1fr, 1fr, auto, auto, auto, auto),
        align: (left, left, left, left, right, right, right, right),
        header: ([], [Date], [Roast · coffee], [Profile], [Green g], [Out g], [Loss %], [Cup]),
        rows: r.roasts.map(x => (text(weight: "bold", x.mark), x.date,
          if x.name == x.coffee { x.coffee } else { stack(spacing: 1.5pt, x.name, text(size: 7pt, fill: soft, x.coffee)) },
          x.profile, x.green, x.out, x.loss, x.cup)),
        footer: (text(weight: "bold")[#r.totals.roasts roasts], [], [], [], text(weight: "bold", r.totals.green), text(weight: "bold", r.totals.out), text(weight: "bold", r.totals.loss), []),
        size: 8pt,
      )
      #if r.cut { text(size: 7.5pt, fill: palette.accent)[Only the first 600 roasts are listed.] }
    ]
  } else {
    v(4mm)
    text(fill: soft)[No roast in this period.]
  }
  v(2mm)
  text(size: 7.5pt, fill: soft, r.notes)
})
