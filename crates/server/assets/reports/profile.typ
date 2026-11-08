// The profile report (docs/reports.md): a profile's targets, its roasts ranked with an
// average row, their curves overlaid on the profile's, and how its numbers spread.
#import "report.typ": *

#let design = (
  name: "Profile report",
  description: "A profile's targets, its roasts in a ranking table with an average row, their BT overlaid on the profile's, and a dot strip each for first crack, drop, DTR and loss.",
)

#let report(r) = page-frame(r, {
  stack(spacing: 3.5pt,
    label-text("Profile"),
    text(size: 21pt, weight: "bold", tracking: -0.01em, r.profile.name))
  v(1mm)
  line(length: 100%, stroke: 0.8pt + ink)
  v(3mm)
  figures(r.summary)
  if r.facts.len() > 0 {
    v(4mm)
    facts(r.facts, columns: 3)
  }
  if r.profile.description != none and r.profile.description != "" {
    v(3mm)
    text(size: 8.5pt, fill: soft, r.profile.description)
  }
  if r.roasts.len() == 0 {
    v(4mm)
    text(fill: soft)[No finished roast of this profile yet: its target curve, and what a roast of it should do.]
    let prof = r.overlay.series.filter(s => s.key == "profile" and s.points.len() > 1)
    if prof.len() > 0 {
      section[The target curve][
        #figure(pdf.artifact(overlay-chart(r.overlay)), alt: "The profile's target BT from charge")
        #if r.curve.len() > 0 {
          v(2mm)
          // Minute by minute, to roast against by hand
          let rows = calc.ceil(r.curve.len() / 2)
          let cols = (r.curve.slice(0, rows), r.curve.slice(rows))
          grid(columns: (1fr, 1fr), column-gutter: 8mm, ..cols.map(c => rtable(
            columns: (auto, 1fr),
            align: (left, right),
            header: ([From charge], [BT #r.meta.temp]),
            rows: c.map(x => (x.t, x.bt)),
            size: 8.5pt,
          )))
        }
      ]
    }
    if r.expected.len() > 0 {
      section[What to expect][
        #rtable(
          columns: (auto, 1fr, auto),
          align: (left, right, left),
          header: ([], [Range], [From]),
          rows: r.expected.map(x => (x.label, x.value, text(fill: soft, x.from))),
          size: 8.5pt,
        )
      ]
    }
  } else {
  section[Its roasts][
    #rtable(
      columns: (auto, auto, 1fr, auto, auto, auto, auto, auto, auto, auto, auto),
      align: (left, left, left) + (right,) * 8,
      header: ([], [Date], [Roast · coffee], [Loss %], [FC], [FC #r.meta.temp], [Drop], [Drop #r.meta.temp], [DTR %], [RMS], [Cup]),
      rows: r.roasts.map(x => (text(weight: "bold", x.mark), x.date,
        if x.name == x.coffee { x.coffee } else { stack(spacing: 1.5pt, x.name, text(size: 7pt, fill: soft, x.coffee)) },
        x.loss, x.fc, x.fc_bt, x.drop, x.drop_bt, x.dtr, x.rmse, x.cup)),
      footer: (text(weight: "bold")[Average], [], [], ..(r.average.loss, r.average.fc, r.average.fc_bt, r.average.drop, r.average.drop_bt, r.average.dtr, r.average.rmse, r.average.cup).map(v => text(weight: "bold", v))),
      size: 8pt,
    )
  ]
  section[The curves][
    #figure(pdf.artifact(overlay-chart(r.overlay)), alt: "The BT of " + str(r.overlaid) + " roasts from charge, overlaid on the profile's")
  ]
  if r.strips.len() > 0 {
    section[How the numbers spread][
      #measure-strips(r.strips)
      #text(size: 7.5pt, fill: soft)[Each ring is a roast, by batch number; the newest is filled.]
    ]
  }
  }
  v(2mm)
  text(size: 7.5pt, fill: soft, r.notes)
})
