// Shop: for coffee gifted or sold under your own name. The brand kit on top (logo, name,
// tagline, in its fonts and colours), the coffee in the middle, and across the bottom what a
// bag that is sold needs (docs/labels.md, "The retail check"): what it is ("Whole bean
// coffee"), the net weight in oz and g, and who packed it, where. Without a brand it uses the
// roaster name from the settings. Fonts: DM Serif Display, Atkinson Hyperlegible Next.
#import "roastwarden.typ": fit, wrap, clamp, join, with, tall, hint

#let design = (
  name: "Shop",
  description: "Your brand on top (logo, name and tagline in its fonts and colours), the coffee in the middle, and along the bottom what a bag that is sold needs: whole bean coffee, the net weight in oz and g, your name and place.",
  params: (
    identity: (default: "Whole bean coffee", help: "what is in the bag, as the label says it"),
    rule: (default: true, help: "a line in the accent colour under the brand"),
  ),
)

#let label(l, w, h, round) = {
  let b = l.brand
  let ink = if b.ink != none { rgb(b.ink) } else { black }
  let accent = if b.accent != none { rgb(b.accent) } else { ink }
  let display = if b.font_display != none { b.font_display } else { "DM Serif Display" }
  let body = if b.font_body != none { b.font_body } else { "Atkinson Hyperlegible Next" }
  let who = if b.name != none { b.name } else { l.roaster }
  let packer = join((who, b.place), sep: ", ")
  set text(font: body, size: 8pt, fill: ink, top-edge: "ascender", bottom-edge: "descender")
  set par(leading: 0.15em)
  let coffee = join((l.origin, l.process))
  let roast = join((l.level, l.purpose))
  let dates = join((with("Roasted ", l.roasted), with("best from ", l.best_from)))
  // Ground coffee says so: "Ground coffee" for the identity (unless the owner wrote their
  // own), and a line with the grind and how long it keeps ("Ground · espresso · best until …")
  let ground = l.at("ground", default: false)
  let identity = if ground and l.params.identity == "Whole bean coffee" { "Ground coffee" } else { l.params.identity }
  let form = if ground { join((l.at("form", default: none), with("best until ", l.at("best_until", default: none)))) }
  let net = if l.net != none { l.net } else { l.weight }
  // For sale: the bag's required heights (a retail check or print asks for them)
  let retail = l.at("retail", default: none)
  // Without a brand or a roaster name the top of the label is empty, and nobody "packed" it
  if who == none and b.logo == none {
    hint("no brand and no roaster name: the label's top is empty and it names no packer. Pick a brand (Labels → Brands, brand=…) or set label_roaster_name")
  }

  if round {
    let inner = w * 0.68
    box(width: w, height: h, align(center + horizon, stack(dir: ttb, spacing: 2.5pt, ..(
      if b.logo != none { image(b.logo, height: 0.32in) },
      if who != none { fit(upper(who), inner * 0.9, max: 7pt, min: 5.5pt, tracking: 0.08em, fill: accent) },
      v(1pt),
      wrap(l.name, inner, max: 14pt, floor: 10pt, min: 7pt, round: true, font: display),
      if coffee != none { clamp(coffee, inner * 1.05, lines: 1, size: 7pt) },
      if roast != none { text(size: 7.5pt, weight: "bold", roast) },
      if dates != none { fit(dates, inner * 1.05, max: 7pt, min: 5.5pt) },
      if form != none { fit(form, inner * 1.05, max: 7pt, min: 5.5pt) },
      if l.message != none { wrap(l.message, inner, max: 8pt, floor: 7pt, min: 6pt, round: true, style: "italic") },
      v(2pt),
      fit(identity, inner * 0.95, max: 7pt, min: 5.5pt),
      if net != none { fit(net, inner * 0.9, max: 9pt, min: 7pt, weight: "bold") },
      if b.place != none { fit(b.place, inner * 0.7, max: 6.5pt, min: 5.5pt) },
    ).filter(x => x != none))))
  } else {
    let pad = 0.14in
    let inner = w - 2 * pad
    let roomy = h > 2.5in
    // The bottom 30 %: what it is, the net weight, who packed it
    let foot = h * 0.3
    // On a short label for sale the brand header gives way: the packer line names the brand
    let head = if retail != none and not roomy { none } else {
      let logo = if b.logo != none { box(height: if roomy { 0.42in } else { 0.32in }, image(b.logo, height: 100%)) }
      let words = stack(dir: ttb, spacing: 2pt, ..(
        if who != none { fit(who, inner * 0.7, max: if roomy { 13pt } else { 11pt }, min: 7pt, font: display, fill: accent) },
        if b.tagline != none { fit(b.tagline, inner * 0.7, max: 7pt, min: 5.5pt, style: "italic") },
      ).filter(x => x != none))
      if logo != none { grid(columns: (auto, 1fr), column-gutter: 0.08in, align: horizon, logo, words) }
      else if who != none or b.tagline != none { words }
    }
    let top-pad = pad * 0.9
    let rule = head != none and l.params.rule
    let gap = if roomy { 6pt } else { 3pt }
    // The lines between the brand and the bottom zone, in the order they print; when they do
    // not all fit, the least needed give way (rank: 0 always, then 1, 2, …), so nothing runs
    // into the bottom zone and a gift message is kept before the details
    // For sale: the net weight alone in the bottom 30 % at the bag's height (in capitals:
    // smaller for the same height), what it is and who packed it moved up beside the coffee
    let net-big = if retail != none and net != none {
      tall(upper(net), retail.min_net_mm * 1mm, min: 8pt, weight: "bold")
    }
    let middle = (
      (order: 0, rank: 0, body: wrap(l.name, inner, max: if roomy { 20pt } else { 15pt }, floor: 12pt, min: 8pt, font: display)),
      // What it is, in bold, its capitals at least half the net weight's (the largest print)
      if retail != none { (order: 1, rank: 0, body: tall(identity,
        retail.min_net_mm * 1mm / 2, caps: true, min: 8pt, weight: "bold")) },
      // Who packed it, and where (city, state and ZIP), each at least 1/16 in
      if retail != none and who != none { (order: 6, rank: 0, body:
        tall("Roasted and packed by " + who, retail.min_mm * 1mm, min: 6pt)) },
      if retail != none and b.place != none { (order: 7, rank: 0, body:
        tall(b.place, retail.min_mm * 1mm, min: 6pt)) },
      if coffee != none { (order: 1, rank: 4, body: clamp(coffee, inner, lines: 1, size: 7.5pt)) },
      if roast != none { (order: 2, rank: 3, body: text(size: 8pt, weight: "bold", upper(roast))) },
      if dates != none { (order: 3, rank: 2, body: fit(dates, inner, max: 7.5pt, min: 6pt)) },
      if form != none { (order: 3, rank: 2, body: fit(form, inner, max: 7.5pt, min: 6pt)) },
      if l.message != none { (order: 4, rank: 1, body: wrap(l.message, inner, max: 9pt, floor: 8pt, min: 6.5pt, style: "italic")) },
    ).filter(x => x != none)
    let lines(list) = stack(dir: ttb, spacing: 2pt, ..list.sorted(key: c => c.order).map(c => c.body))
    // The brand, its rule and the lines under it, as one block
    let top-part(list) = block(width: inner, {
      if head != none {
        head
        if rule { v(3pt); line(length: inner, stroke: 0.6pt + accent) }
        v(gap)
      }
      lines(list)
    })
    box(width: w, height: h, inset: (x: pad, top: top-pad, bottom: pad * 0.7), context {
      // Down to the bottom zone; for sale, down to the net weight less its letters' height
      // of space above it
      let limit = if net-big != none {
        h - top-pad - pad * 0.7 - measure(net-big).height - retail.min_net_mm * 1mm * 1.15
      } else { h - foot - top-pad - 2pt }
      let chosen = ()
      for c in middle.sorted(key: c => c.rank) {
        if measure(top-part(chosen + (c,))).height <= limit { chosen.push(c) }
      }
      // The notes last, in the height that is left
      if roomy and l.notes != none {
        let left = limit - measure(top-part(chosen)).height - 2pt
        if left > 9pt {
          chosen.push((order: 5, rank: 5, body: clamp(l.notes, inner, height: left, size: 7.5pt, style: "italic")))
        }
      }
      place(top + left, top-part(chosen))
      // The bottom zone, from the label's bottom edge up
      if net-big != none {
        place(bottom + left, block(width: inner, align(left + bottom, net-big)))
      } else {
        place(bottom + left, block(width: inner, height: foot - pad * 0.7, align(bottom, stack(dir: ttb, spacing: 2pt, ..(
          grid(columns: (1fr, auto), column-gutter: 0.1in, align: bottom,
            fit(identity, inner * 0.42, max: 8pt, min: 6pt),
            if net != none { fit(net, inner * 0.56, max: 10pt, min: 7pt, weight: "bold") }),
          if packer != none { fit("Roasted and packed by " + packer, inner, max: 6.5pt, min: 5.5pt) },
        ).filter(x => x != none)))))
      }
    })
  }
}
