// Purchases (docs/reports.md): the green coffee bought over a range, order by order with
// shipping and fees, each supplier's share, and the landed cost per kg over time.
#import "report.typ": *

#let design = (
  name: "Purchases",
  description: "Green coffee bought over a range: each order with its coffees, shipping, fees and landed cost per kg; each supplier's share; landed cost over time.",
)

#let report(r) = page-frame(r, {
  stack(spacing: 3.5pt,
    label-text("Purchases"),
    text(size: 22pt, weight: "bold", tracking: -0.01em, r.period.title),
    text(size: 9pt, fill: soft, r.period.span))
  v(1mm)
  line(length: 100%, stroke: 0.8pt + ink)
  v(3mm)
  figures(r.summary)
  if r.orders.len() == 0 {
    v(4mm)
    text(fill: soft)[Nothing bought in this period.]
  } else {
    if r.chart != none {
      section[Landed cost per kg, purchase by purchase][#dots-over-time(r.chart)]
    }
    if r.by_supplier.len() > 1 {
      section[By supplier][
        #rtable(columns: (1fr, auto, auto, auto, auto), align: (left, right, right, right, right),
          header: ([Supplier], [Orders], [kg], [Spent], [Landed/kg]),
          rows: r.by_supplier.map(s => (s.supplier, str(s.orders), s.kg, s.spent, s.landed)), size: 8.5pt)
      ]
    }
    section[Order by order][
      #rtable(
        columns: (auto, 1fr, auto, auto, auto, auto, auto, auto),
        align: (left, left, right, right, right, right, right, right),
        header: ([Date], [Supplier · coffees], [kg], [Goods], [Shipping], [Fees], [Total], [Landed/kg]),
        rows: r.orders.map(x => (
          x.date,
          stack(spacing: 2pt, text(weight: "bold", x.supplier),
            ..x.lines.map(l => text(size: 7.5pt)[#l.coffee #text(fill: soft)[· #l.kg kg · #l.price#if l.crop != none [ · crop #l.crop]]])),
          x.kg, x.goods, x.shipping, x.fees, text(weight: "bold", x.total), x.landed,
        )),
        footer: (text(weight: "bold")[Total], [], text(weight: "bold", r.totals.kg), r.totals.goods, r.totals.shipping, r.totals.fees, text(weight: "bold", r.totals.total), []),
        size: 8pt,
      )
    ]
  }
  v(2mm)
  text(size: 7.5pt, fill: soft, r.notes)
})
