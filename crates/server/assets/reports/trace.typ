// Traceability (docs/reports.md): from the green coffee to every bag, or a bag's roast back
// to its green lot. The coffee and its purchases; each roast with where its coffee went.
#import "report.typ": *

#let design = (
  name: "Traceability",
  description: "The green coffee (supplier, lot, origin, process, purchases), each roast (batch, date, roaster, profile, green → roasted, loss) and where it went: packed bags, label runs, gifts, use, discards and what is left.",
)

#let report(r) = page-frame(r, {
  stack(spacing: 3.5pt, label-text("Traceability"), text(size: 20pt, weight: "bold", tracking: -0.01em, r.title))
  v(1mm)
  line(length: 100%, stroke: 0.8pt + ink)
  v(3mm)
  figures(r.summary, columns: 4)

  let c = r.coffee
  if c == none {
    section[The green coffee][
      #text(fill: soft)[No green coffee is recorded for this roast: pick its coffee on the roast (History) to trace it.]
    ]
  } else {
    section[The green coffee][
      #text(size: 12pt, weight: "bold", c.name)
      #v(1mm)
      #facts(c.facts, columns: 2)
      #if c.link != none { v(1mm); text(size: 7pt, fill: soft, c.link) }
      #if c.purchases.len() > 0 {
        v(2mm)
        rtable(
          columns: (auto, auto, 1fr, auto, auto),
          align: (left, right, left, left, left),
          header: ([Bought], [Green], [Supplier], [Crop], [Arrived]),
          rows: c.purchases.map(p => (
            p.date, p.amount,
            stack(spacing: 2pt, p.supplier, if p.note != none { text(size: 7pt, fill: soft, p.note) }),
            p.crop, p.arrival,
          )),
          size: 8.5pt,
        )
      } else {
        v(1mm)
        text(size: 8pt, fill: soft)[No purchase recorded.]
      }
    ]
  }

  if r.roasts.len() == 0 {
    section[Roasts][#text(fill: soft)[No finished roast of this coffee yet.]]
  }
  for x in r.roasts {
    section(breakable: true, [#x.mark #h(4pt) #text(weight: "regular", x.name)], {
      grid(columns: (1fr, 1fr, 1fr, 1fr), column-gutter: 5mm,
        stack(spacing: 3pt, label-text("Roasted"), x.date),
        stack(spacing: 3pt, label-text("Roaster · profile"), x.roaster + " · " + x.profile),
        stack(spacing: 3pt, label-text("Green → roasted"), x.weights + "  (" + x.loss + ")"),
        stack(spacing: 3pt, label-text("Left"), x.left + if x.best_until != none { text(fill: soft)[ · best until #x.best_until] }),
      )
      if x.rows.len() > 0 {
        v(1.5mm)
        rtable(
          columns: (auto, auto, auto, 1fr),
          align: (left, left, right, left),
          header: ([Date], [Where it went], [Amount], []),
          rows: x.rows.map(m => (m.date, m.what, m.amount, text(fill: soft, m.detail))),
          size: 8.5pt,
        )
      } else {
        v(1mm)
        text(size: 8pt, fill: soft)[Nothing recorded yet: all of it is still here.]
      }
    })
  }
  v(1fr)
  text(size: 7.5pt, fill: soft, r.notes)
})
