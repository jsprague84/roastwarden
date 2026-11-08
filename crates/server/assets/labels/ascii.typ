// ASCII: a typewriter ticket. A drawing in characters beside the details, all monospaced.
// Fonts: JetBrains Mono NL (SIL Open Font License).
#import "roastwarden.typ": fit, wrap, clamp, join, with, ascii

#let arts = (
  cup: "   ( (\n    ) )\n ........\n |      |]\n \\      /\n  `----'",
  bean: "  .-\"\"-.\n /  ||  \\\n|   ||   |\n \\  ||  /\n  '-..-'",
  pot: "   _\n _|_|_\n|  |  |\n|  |  |)\n|_____|",
  cherry: "    \\\\ /\n     \\/\n  .-.  .-.\n (   )(   )\n  '-'  '-'",
)

#let design = (
  name: "ASCII",
  description: "Monospaced details beside a drawing made of characters. art is cup, bean, pot, cherry, none, or your own lines.",
  params: (
    art: (default: "cup", choices: ("cup", "bean", "pot", "cherry", "none"), help: "the drawing, or your own ASCII art (several lines)"),
    rule: (default: "-", help: "the character of the rule under the name"),
  ),
)

#let mono = "JetBrains Mono NL"

#let label(l, w, h, round) = {
  set text(font: mono, size: 7.5pt, fill: black, top-edge: "ascender", bottom-edge: "descender")
  set par(leading: 0.25em)
  let art = arts.at(l.params.art, default: l.params.art)
  let art = if l.params.art == "none" { none } else { art }
  // A line of the ticket: a word, dots to a column, the value
  let row(key, value) = if value == none { none } else {
    let k = key + " " + "." * calc.max(1, 10 - key.len()) + " "
    text(fill: luma(80), k) + value
  }
  let roast = join((l.level, l.purpose), sep: " / ")
  let rows = (
    row("roasted", l.roasted),
    row("best from", l.best_from),
    row("batch", with("#", l.batch)),
    row("net", l.weight),
  ).filter(r => r != none)
  if round {
    let inner = w * 0.68
    box(width: w, height: h, align(center + horizon, stack(dir: ttb, spacing: 3pt, ..(
      if art != none { ascii(art, size: 6.5pt) },
      wrap(upper(l.name), inner, max: 10pt, floor: 7.5pt, min: 6pt, round: true, font: mono, weight: "bold"),
      if roast != none { fit(upper(roast), inner, max: 7pt, min: 5.5pt, font: mono) },
      // One line for both dates: a long message keeps its room inside the circle
      if l.roasted != none or l.best_from != none {
        fit(join((with("roasted ", l.roasted), with("best ", l.best_from)), sep: " · "), inner * 1.05, max: 6.5pt, min: 5.5pt, font: mono)
      },
      if l.message != none { wrap(l.message, inner, max: 7.5pt, floor: 6.5pt, min: 6pt, round: true, font: mono, style: "italic") },
    ).filter(x => x != none))))
  } else {
    let pad = 0.14in
    let art-w = if art == none { 0pt } else { 1.05in }
    let text-w = w - 2 * pad - art-w - (if art == none { 0pt } else { 0.1in })
    box(width: w, height: h, inset: pad, grid(
      columns: (art-w, text-w), rows: (h - 2 * pad,), align: horizon,
      column-gutter: if art == none { 0pt } else { 0.1in },
      if art != none { align(center, ascii(art, size: if h > 2.5in { 10pt } else { 8.5pt })) },
      stack(dir: ttb, spacing: if h > 2.5in { 3pt } else { 2pt }, ..(
        wrap(upper(l.name), text-w, max: 13pt, floor: 9pt, min: 7pt, font: mono, weight: "bold"),
        text(fill: luma(60), (l.params.rule * 60).slice(0, calc.min(60, int(text-w / 4.5pt)))),
        if roast != none { text(weight: "bold", upper(roast)) },
        if join((l.origin, l.process)) != none { clamp(join((l.origin, l.process)), text-w, lines: 1, font: mono, size: 7pt) },
        // The rows as one block, a line each
        if rows.len() > 0 { block(text(size: 7pt, rows.join(linebreak()))) },
        if l.message != none { wrap("> " + l.message, text-w, max: 8pt, floor: 7pt, min: 6pt, font: mono) },
        if l.notes != none and h > 2.5in { clamp(l.notes, text-w, lines: 3, font: mono, size: 7pt) },
      ).filter(x => x != none)),
    ))
  }
}
