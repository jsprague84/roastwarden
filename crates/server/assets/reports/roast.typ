// The roast report (docs/reports.md): one roast on a page, more only when it has more to say.
#import "report.typ": *

#let design = (
  name: "Roast report",
  description: "One roast: the coffee, the numbers against what was expected, the curve with its phases and events, then notes, cupping and cost.",
)

#let report(r) = page-frame(r, {
  let ro = r.roast
  // The coffee and the roast
  grid(
    columns: (1fr, auto),
    align: (left + bottom, right + bottom),
    column-gutter: 6mm,
    stack(spacing: 3.5pt,
      label-text(if ro.batch != none [Roast report · batch #ro.batch] else [Roast report]),
      text(size: 21pt, weight: "bold", tracking: -0.01em, ro.coffee),
      text(size: 9.5pt, fill: soft, (ro.origin, ro.at("region", default: none), ro.at("producer", default: none), ro.variety, ro.process)
        .filter(x => x != none and x != "").join(" · ")),
    ),
    stack(spacing: 3.5pt,
      text(size: 10pt, weight: "bold", ro.date),
      text(size: 8.5pt, fill: soft, ro.time + " · " + ro.roaster),
      text(size: 8.5pt, fill: soft, (ro.profile, (ro.purpose, ro.level).filter(x => x != none).join(", "))
        .filter(x => x != none and x != "").join(" · ")),
    ),
  )
  v(1mm)
  line(length: 100%, stroke: 0.8pt + ink)
  v(2.5mm)
  figures(r.figures)
  v(1.5mm)

  // The curve, its heater and fan, its phases
  figure(
    pdf.artifact(roast-chart(r.chart)),
    alt: r.chart.alt,
  )
  v(-1mm)
  if r.chart.strip.len() > 0 { strip-chart(r.chart, r.chart.strip) }
  v(1.5mm)
  if r.phases.len() > 0 { phase-bar(r.phases) }
  v(2mm)

  // Phases and events side by side
  grid(columns: (1fr, 1.25fr), column-gutter: 7mm,
    rtable(
      columns: (1fr, auto, auto, auto, auto),
      align: (left, right, right, right, right),
      header: ([Phase], [Time], [%], [ΔBT #r.meta.temp], [RoR]),
      rows: r.phases.map(p => (p.name, p.time, p.pct, p.dt, p.ror)),
    ),
    rtable(
      columns: (1fr, auto, auto, auto, auto, auto),
      align: (left, right, right, right, right, right),
      header: ([Event], [Time], [BT], [ET], [RoR], [Heat]),
      rows: r.events.map(e => (e.name, e.time, e.bt, e.et, e.ror, e.heater)),
    ),
  )
  text(size: 7.5pt, fill: soft)[Temperatures #r.meta.temp, rate of rise #r.meta.rate, heater %. Drying runs from charge to dry end (its ΔBT and RoR from the turning point when one is marked), Maillard to first crack, development to drop.]

  // Notes, cupping, cost: on page 2 when page 1 is full
  if ro.notes != none and ro.notes != "" {
    section[Notes][#ro.notes]
  }
  let cup = r.at("cupping", default: none)
  if cup != none {
    section[Cupping #h(1fr) #text(weight: "regular", size: 8.5pt, fill: soft)[classic SCA form]][#grid(columns: (auto, 1fr), column-gutter: 8mm,
      stack(spacing: 3pt,
        text(size: 22pt, weight: "bold", cup.final),
        text(size: 8.5pt, fill: soft, cup.band),
        text(size: 8pt, fill: soft)[total #cup.total − defects #cup.defects],
      ),
      {
        rtable(
          columns: (1fr,) * cup.attributes.len(),
          align: (center,) * cup.attributes.len(),
          header: cup.attributes.map(a => text(size: 7.5pt, a.short)),
          rows: (cup.attributes.map(a => a.score),),
          size: 8.5pt,
        )
        if cup.notes != none { emph(cup.notes) }
      },
    )]
  }
})
