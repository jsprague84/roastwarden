// The coffee report (docs/reports.md): one green coffee, what is known of it, its stock and
// cost, its roasts as a table and overlaid on one chart with RoR, its cuppings and its ledger.
#import "report.typ": *

#let design = (
  name: "Coffee report",
  description: "One green coffee: its facts, stock and cost, its roasts as a table and overlaid on one chart with their RoR, its cuppings, and its ledger with a running balance.",
)

#let report(r) = page-frame(r, {
  let c = r.coffee
  // The closing note goes in the last section, so it never makes a page of its own
  let last = if r.ledger.len() > 0 { "ledger" } else if r.cuppings.len() > 0 { "cuppings" } else if r.roasts.len() > 0 { "curves" } else { none }
  let closing = { v(2mm); text(size: 7.5pt, fill: soft, r.notes) }
  let close(key) = if key == last { closing }
  stack(spacing: 3.5pt,
    label-text("Coffee"),
    text(size: 21pt, weight: "bold", tracking: -0.01em, c.name),
    if c.line != "" { text(size: 9.5pt, fill: soft, c.line) })
  v(1mm)
  line(length: 100%, stroke: 0.8pt + ink)
  v(3mm)
  figures(r.summary)
  if r.facts.len() > 0 {
    v(4mm)
    facts(r.facts, columns: 3)
  }
  if c.notes != none {
    v(3mm)
    text(size: 8.5pt)[#text(fill: soft)[The supplier's notes · ] #emph(c.notes)]
  }
  if c.own_notes != none and c.own_notes != "" {
    v(1mm)
    text(size: 8.5pt)[#text(fill: soft)[Your notes · ] #c.own_notes]
  }

  if r.roasts.len() > 0 {
    section[Its roasts][
      #rtable(
        columns: (auto, auto, 1fr, auto, auto, auto, auto, auto, auto, auto),
        align: (left, left, left, right, right, right, right, right, right, right),
        header: ([], [Date], [Roast · profile], [Green g], [Loss %], [FC], [Drop #r.meta.temp], [Dev.], [DTR %], [Cup]),
        rows: r.roasts.map(x => (text(weight: "bold", x.mark), x.date,
          if x.name == x.profile { x.name } else { stack(spacing: 1.5pt, x.name, text(size: 7pt, fill: soft, x.profile)) },
          x.green, x.loss, x.fc, x.drop_bt, x.dev, x.dtr, x.cup)),
        size: 8pt,
      )
    ]
    // One chart: every roast's BT and RoR from charge, the newest in colour
    section[The curves, overlaid][
      #figure(pdf.artifact(overlay-chart(r.overlay, height: 72mm)),
        alt: "The BT and rate of rise of " + str(r.overlaid) + " roasts from charge, overlaid; the newest in colour")
      #text(size: 7.5pt, fill: soft)[Each roast's BT (left axis) and rate of rise (right) from charge; the newest in colour with its first crack and drop marked, the others grey.#if r.more_roasts > 0 [ The newest #r.overlaid shown; #r.more_roasts more in the table.]]
      #close("curves")
    ]
  } else {
    v(4mm)
    text(fill: soft)[Not roasted yet.]
  }

  if r.cuppings.len() > 0 {
    section[Cuppings][
      #rtable(
        columns: (auto, auto, auto, 1fr),
        align: (left, right, left, left),
        header: ([], [Final], [Band], [Notes]),
        rows: r.cuppings.map(x => (text(weight: "bold", x.mark), text(weight: "bold", x.final), x.band, if x.notes != none { x.notes } else { [] })),
        size: 8.5pt,
      )
      #close("cuppings")
    ]
  }

  if r.ledger.len() > 0 {
    // A long ledger runs over pages; a short one keeps together with the closing note
    section(breakable: r.ledger.len() > 25)[The ledger][
      #rtable(
        columns: (auto, 1fr, auto, auto, auto, auto),
        align: (left, left, right, right, right, right),
        header: ([Date], [What], [Change g], [Stock g], [Cost], [Cost/kg after]),
        rows: r.ledger.map(x => (x.date, stack(spacing: 2pt, x.what, if x.note != none { text(size: 7pt, fill: soft, x.note) }), x.change, x.balance, x.cost, x.per_kg)),
        size: 8pt,
      )
      #if r.ledger_cut > 0 { text(size: 7.5pt, fill: soft)[The oldest #r.ledger_cut moves are left out.] }
      #close("ledger")
    ]
  }
  if last == none { closing }
})
