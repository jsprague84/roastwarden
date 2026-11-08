// The roast-day plan sheet (docs/reports.md): the planned roasts in order, the green to weigh
// out and what to expect, the profile's targets, and boxes to write what happened.
#import "report.typ": *

#let design = (
  name: "Roast-day plan",
  description: "The planned roasts in order: green to weigh and the output to expect, the profile's targets, and boxes for charge, first crack, drop and the roasted weight; the green each coffee needs against stock.",
)

// A box to write in, its unit faint in its corner so the number needs none
#let write(w: 16mm, h: 7mm, unit: none) = box(width: w, height: h, stroke: 0.7pt + ink, radius: 0.8mm,
  inset: (right: 1mm, bottom: 0.8mm),
  if unit != none { align(right + bottom, text(size: 6pt, fill: faint, unit)) })

#let roast-block(x, temp) = block(breakable: false, width: 100%, above: 5mm, {
  grid(columns: (auto, 1fr, auto), column-gutter: 3mm, align: (left + bottom, left + bottom, right + bottom),
    text(size: 16pt, weight: "bold", str(x.n)),
    stack(spacing: 3pt,
      text(size: 11pt, weight: "bold", x.title),
      text(size: 8pt, fill: soft, (x.coffee, x.profile).filter(v => v != none and v != "–").join(" · "))),
    stack(spacing: 2pt, align(right, label-text("Weigh")), text(size: 14pt, weight: "bold", x.green)))
  v(0.5mm)
  line(length: 100%, stroke: 0.6pt + ink)
  v(1mm)
  set text(size: 8.5pt)
  grid(columns: (1fr, auto), column-gutter: 4mm,
    {
      if x.targets.len() > 0 {
        grid(columns: (auto, 1fr) * calc.min(2, x.targets.len()), column-gutter: (3mm, 6mm), row-gutter: 1.8mm,
          ..x.targets.map(t => (text(fill: soft, t.label), t.value)).flatten())
      }
      if x.expect != none { v(1mm); text(fill: soft, x.expect) }
      // How this coffee went last time on this profile, or that it is the first
      let l = x.at("last", default: none)
      if l != none {
        v(1mm)
        block(below: 0pt, [#text(weight: "bold")[Last time] #text(fill: soft)[#l.mark, #l.date:] FC #l.fc · drop #l.drop · loss #l.loss])
        if l.note != none { v(1.4mm); text(size: 7.5pt, fill: soft, emph(l.note)) }
      } else if x.at("first", default: false) {
        v(1mm)
        text(fill: soft)[First roast of this coffee on this profile.]
      }
      if x.notes != none and x.notes != "" { v(1mm); emph(x.notes) }
      text(size: 7.5pt, fill: faint)[ · on #x.roaster]
    },
    grid(columns: 4, column-gutter: 2mm, row-gutter: 1.5mm, align: (right + horizon, left + horizon, right + horizon, left + horizon),
      [Charge], write(unit: temp), [Out], write(unit: "g"),
      [FC], write(unit: "m:ss"), [at], write(unit: temp),
      [Drop], write(unit: "m:ss"), [at], write(unit: temp)),
  )
  v(2mm)
  grid(columns: (auto, 1fr), column-gutter: 2mm, text(fill: soft)[Notes], align(bottom, line(length: 100%, stroke: 0.4pt + faint)))
})

#let report(r) = page-frame(r, {
  grid(columns: (1fr, auto), align: (left + bottom, right + bottom),
    stack(spacing: 3.5pt, label-text("Roast day"), text(size: 20pt, weight: "bold", tracking: -0.01em, r.day)),
    if r.roaster != none { text(fill: soft, r.roaster) })
  v(1mm)
  line(length: 100%, stroke: 0.8pt + ink)
  if r.roasts.len() == 0 {
    v(4mm)
    text(fill: soft)[Nothing planned. Plan roasts on Beans → Planned roasts, or with `roast plan add`.]
  } else {
    if r.weigh.len() > 0 {
      v(3mm)
      rtable(
        columns: (1fr, auto, auto, auto, auto),
        align: (left, right, right, right, left),
        header: ([Green to weigh out], [Roasts], [Needed g], [In stock g], []),
        rows: r.weigh.map(w => (w.coffee, str(w.roasts), w.green, w.stock,
          if w.enough { text(fill: soft)[enough] } else { text(fill: palette.accent, weight: "bold")[short by #w.short g] })),
        size: 8.5pt,
      )
    }
    for x in r.roasts { roast-block(x, r.meta.temp) }
  }
  v(1fr)
  text(size: 7.5pt, fill: soft, r.notes)
})
