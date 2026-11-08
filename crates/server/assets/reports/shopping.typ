// The shopping list (docs/reports.md): a short purchase order per supplier, what each bag
// cost when seen and the shipping that supplier usually adds; then what is running low.
#import "report.typ": *

#let design = (
  name: "Shopping list",
  description: "The coffees on the list as one short order per supplier, prices as seen and the usual shipping; then the coffees running low.",
)

#let report(r) = page-frame(r, {
  grid(columns: (1fr, auto), align: (left + bottom, right + bottom),
    stack(spacing: 3.5pt, label-text("Shopping list"), text(size: 20pt, weight: "bold", tracking: -0.01em, r.as_of)),
    text(fill: soft, "money in " + r.meta.currency))
  v(1mm)
  line(length: 100%, stroke: 0.8pt + ink)
  v(3mm)
  figures(r.summary, columns: 3)

  if r.orders.len() == 0 {
    v(6mm)
    text(fill: soft)[Nothing on the list. Add a coffee from a supplier's link (Beans → Shopping list, or `roast bean import <link> --want`).]
  }
  for o in r.orders {
    block(breakable: false, above: 7mm, {
      grid(columns: (1fr, auto), align: (left + bottom, right + bottom),
        text(size: 13pt, weight: "bold", o.supplier),
        if o.site != none { text(size: 8.5pt, fill: soft, o.site) })
      v(1mm)
      rtable(
        columns: (1fr, auto, auto, auto),
        align: (left, left, right, right),
        header: ([Coffee], [Bag], [Seen], [Price]),
        rows: o.lines.map(l => (
          stack(spacing: 2pt,
            text(weight: "bold", l.coffee) + if l.again { text(size: 7.5pt, fill: soft)[ · again] },
            if l.link != none { text(size: 6.5pt, fill: soft, l.link) },
            if l.notes != none { text(size: 7.5pt, style: "italic", l.notes) }),
          l.size, l.seen, l.price,
        )),
        footer: (
          [], [], text(fill: soft)[Subtotal], o.subtotal,
          [], [], text(fill: soft)[Shipping], stack(spacing: 2pt, o.shipping, if o.shipping_basis != none { text(size: 6.5pt, fill: soft, o.shipping_basis) }),
          [], [], text(weight: "bold")[Total], text(weight: "bold", o.total),
        ),
        size: 8.5pt,
      )
      if not o.complete { text(size: 7.5pt, fill: palette.accent)[A bag here has no price in this currency: the total leaves it out.] }
    })
  }

  if r.low.len() > 0 {
    section[Running low][
      #rtable(
        columns: (1fr, auto, auto, auto, auto),
        align: (left, right, right, left, right),
        header: ([Coffee], [Left], [Runs out], [Last from], [Cost]),
        rows: r.low.map(l => (
          stack(spacing: 2pt, text(weight: "bold", l.coffee), text(size: 7pt, fill: if l.listed { soft } else { palette.accent }, l.why + if l.listed { " · on the list" } else { " · not on the list" })),
          l.left, l.runs_out, l.supplier, l.last_cost,
        )),
        size: 8.5pt,
      )
    ]
  }
  v(1fr)
  text(size: 7.5pt, fill: soft, r.notes)
})
