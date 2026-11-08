# Bag labels

Roastwarden prints labels for bags of roasted coffee on standard laser sticker sheets. A label
is filled from the roast, its coffee and your settings: the coffee's name and origin, the roast
level and purpose, the roast date and when it is best from, the tasting notes, the weight and
a QR code. Each label is laid out by a design, a small Typst file, so you can use one of the
built-in designs or write your own. Print the PDF yourself, or let the server send it straight
to a network printer.

Labels is an optional module: turn it on under Settings → Modules (the Roastery preset has
it). It adds **Labels** to the top bar (Label Studio) and **Labels…** to each roast in History.

## Labels for a roast

1. Open the roast's labels: History → the roast → **Labels…**, the **Labels** link on the
   Roast page's after-roast summary once the roast is weighed, or a lot under **Roasted
   coffee** on Beans.
2. Pick the **Label sheet** and where to **Start at label** on a part-used sheet: click a
   label in the preview. Grey labels are left blank. The sheet remembers where the last print
   left off.
3. Fill in the label:

   | Field | What it does |
   |---|---|
   | Design | The coffee's own label ("Its own: …") if it has one, else **plain**, or any other design. Its settings show under it. |
   | Copies | How many labels. |
   | Net weight | The roast's roasted weight unless you type one (a 250 g bag: 250). |
   | In the bag | **Whole bean** or **Ground**, with the grind ("espresso", "filter", your own word). A ground bag gets an earlier best-until date. |
   | Message | A line for a gift ("for Sam, with love"). A design that prints its own "To" calls it **To** and wants just the name. |
   | Brand | The coffee's brand, else the default one (below, "Label Studio"). |
   | Tasting notes | Leave empty for the roast's cupping notes, else the coffee's. |
   | On the label | Untick what should not print. |
   | QR code opens | **This roast in Roastwarden** (needs your login), **Its share page** (anyone with the bag, below), **The coffee at its supplier**, or **No QR code**. |

4. **Use for <the coffee>** keeps the design, its settings and the brand as that coffee's own
   label, so its next roasts start with it.
5. **Download PDF**, or **Print on …** when a printer is set up. Print a PDF at **Actual
   size** (not "Fit to page") with the paper type "Labels", then press **Printed it** so the
   sheet's next free label moves on. Downloading the same labels twice is one run, not two.

The preview is the server's own rendering of the first sheet, the same layout as the PDF:
what you see is what prints.

When the roast has roasted coffee left, a tick box can take the bags off it as packed when the
labels are made ([roasted coffee](roasted.md)).

### Label sheets

Built in: Avery **5163** (4 × 2 in, 10 a sheet, the common bag label), **6874** (3.75 × 3 in,
6 a sheet, room for notes and a QR code) and **5294** (2.5 in round, 12 a sheet). Buy sheets
sold for **laser** printers: inkjet stock can smear toner or let adhesive ooze in the fuser.
Most office lasers are black only, so the built-in designs are black; black on kraft paper is
the usual coffee look.

Other stock goes in the `label_sheets` setting, a JSON list in inches (a sheet with a built-in
id replaces it). Generic "compatible" sheets can differ from Avery's layout, so measure yours:

```sh
echo '[{"id":"OL125","name":"OnlineLabels OL125","page_w":8.5,"page_h":11,"w":4,"h":2,
  "cols":2,"rows":5,"left":0.16,"top":0.5,"pitch_x":4.14,"pitch_y":2}]' \
  | roast settings set label_sheets=-
```

`top` and `left` are the page margins to the first label, `pitch_x` and `pitch_y` from one
label's edge to the next; add `"round": true` for round labels.

### What a label says

- **The name**: the coffee's label name when it has one (a short name for a long one, "Kenya
  Gatugi"), else its name.
- **A blend** (batches of different coffees in one bag, [roasted coffee](roasted.md)): the
  bag's name, else its coffees' ("Guji + Kenya Nyeri"); the origin line gives each country
  with its share ("60 % Ethiopia · 40 % Kenya"), and the variety and process each coffee's.
  Its notes are only the ones you type: one coffee's would not describe the blend.
- **The notes**, the first found of: the notes you type; the coffee's **Label notes** (Beans →
  the bean → Edit, a few words such as "Chocolate · coconut · apple", at most 120
  characters; with none, the form suggests some from the supplier's notes); the roast's
  cupping notes; the supplier's notes, cut after a whole sentence.
- **Best from**: the roast date plus a rest, set in Settings → Roasting:

  | Rest | Default |
  |---|---|
  | Espresso, light | 10 days |
  | Espresso, medium or darker | 7 days |
  | Filter, light | 7 days |
  | Filter, medium or darker | 4 days |

  Omni roasts read as filter; a roast with no level reads as light. A ground bag's best-until
  is the pack date plus "Ground, at its best for" (14 days), or the lot's own date if that is
  earlier.
- **The weight** in grams and ounces ("250 g · 8.8 oz"), rounded down, never more than is in
  the bag.
- **The roaster's name**: "Roaster name on labels" in Settings → Roasting, or a brand.

Dates are in your time zone: the browser's, or the `time_zone` setting.

### Built-in designs

| Design | Look |
|---|---|
| plain | Everything at a glance in the dashboard's typeface, the QR code on the right; a centred stack on round labels. The type grows to fill a roomy label. |
| gift | A serif name, the message in a script face, a double frame; the QR code opens the share page by default. |
| script | The name in a script face over a drawn flourish. Settings: a script `title` instead of the name ("Sam's Blend"), a small `top` line, the `flourish`. |
| ascii | A typewriter ticket with a drawing in characters: `art` is cup, bean, pot, cherry, none or your own lines. |
| cherry | Line art of a coffee branch with cherries, a leaf or a roasted bean; `ink` sets a colour for a colour printer. |
| shop | For coffee sold under your name: the brand on top, the coffee in the middle, and in the bottom 30 % what a bag that is sold needs (what it is, the net weight, "Roasted and packed by" and your place). |

## Label Studio

**Labels** in the top bar. Its tabs:

| Tab | What it is for |
|---|---|
| Designs | Every design on sample data, at the shape of the sheet picked. Open one to edit it, or **New design** from the template or another design. |
| Print | A print run: one sheet of labels for one roast or several (a gift run of three coffees), each roast with its own design, message and copies. |
| History | What was printed and when: download a run's PDF again, **Printed it**, **Reprint** (the labels exactly as they were, all or some), and the sheet's next free label (**new sheet** after loading a fresh one). |
| Printers | Where labels print (below). |
| Coffees | Each coffee's own label: design, settings, brand. |
| Assets | Images (SVG, PNG, JPEG, up to 2 MB) and fonts (TTF, OTF, up to 8 MB) a design can use. |
| Brands | Brand kits: your roaster's name, tagline, place, website, logo, colours and fonts, previewed on the shop design. **Make default** uses one when a coffee has none. |

## Printing to a printer

Roastwarden can send labels straight to a network printer over IPP, the protocol AirPrint and
IPP Everywhere printers speak. Without one, download the PDF and print it yourself.

**Add a printer**: Label Studio → Printers → **Add a printer**, with its address. Most
printers: `ipp://its-address:631/ipp/print`; the printer's own web page or a configuration
report shows it. **Find printers on the network** looks for 4 s, but only on the server's own
network segment: in Docker that needs host networking ([running it](DEPLOY.md)), and a printer
on another subnet is never found. Typing the address always works.

When it is added, and each time the Printers page opens, the printer is asked what it can do
and how it is (a door open, no paper). Per printer you can set:

| Setting | Choices |
|---|---|
| Tray | The one holding the sheet's size (the default), or a named tray. |
| Colour | Black unless a label has colour, Always black, Colour. |
| Send labels as | Whatever it takes best (PDF, else raster), PDF, PWG Raster, Apple URF. Printers without PDF get a raster of the same layout. |

One printer is the **default** (**Make default**); a print can name another, and a tray and
colour, under the Labels page's **Printer**.

**An `ipps` printer** makes its own certificate, so no authority vouches for it. **Read its
certificate** shows its SHA-256 fingerprint: compare it with the one on the printer's own page,
tick "trust it", and Roastwarden talks only to a printer showing that same certificate. Some
printers' TLS is too old to be accepted; use `ipp://` for those.

**At the printer**: load the label sheet before printing, in the tray the print asks for (the
one holding paper of the sheet's size: on many lasers the hand-fed slot), face up as the
tray's picture shows. A print sent to an empty tray waits; feed the sheet and confirm on the
printer. A stopped printer (door open, out of paper) is refused before anything is sent.
Roastwarden follows the job and says what the printer did: printed, still printing, waiting
for paper, or aborted. A job that waited for paper still ends up "printed" in History.

**Approval**: a print asked for with an API token (an agent, a script) always waits for your
approval, whatever the token's settings: it uses a sheet of labels, and someone must load it.
The approval says what to load ("load a 5163 sheet; labels 1–3 used"). From the dashboard
signed in with the owner key, a print goes at once.

### Calibration

Printers feed paper a little off. To line the labels up:

1. Open **Calibrate this sheet** on a roast's Labels page, or **Calibration** for a sheet under
   Label Studio → Printers.
2. Print the **Calibration page** on plain paper. It has an L 3 mm inside every label corner
   (a circle and its centre on round labels), a mm scale across each label edge, and a 100 mm
   rule to check the printer did not scale the page.
3. Hold it over a sheet of labels against the light. Where each label's edge crosses its
   scale (a tick each mm, the long tick where the edge should be) is how far the marks must
   move.
4. Enter it as **Right, mm** and **Down, mm** (positive is right and down) and **Save offset**.
   Print the page again to check.

On the Printers page you can enter both the first and the last label's marks. They must agree
within 0.5 mm: if not, the printer is scaling the page (print at Actual size) or the sheet
feeds askew, which no offset fixes. An offset is kept per printer and sheet, with the sheet's
own offset as the fallback.

Most lasers cannot print in the outer 4 mm or so of the page; the built-in designs keep text
well inside each label, and the calibration page leaves out a scale that would fall there.

## The share page

A bag you give away has a QR code that should open something for the person holding it, not a
login. Each roast can have a public **share page** at `<your address>/R/<code>`:

- **What it shows**: your brand, the gift line if the link has one, the coffee's name, origin
  and producer, the roast level and purpose, the roast date and when it is at its best ("Resting:
  at its best in 3 days"), the tasting notes, variety, process and batch number, the roast
  curve with its events and times, a brew suggestion from the roast's purpose and level and
  your doses (Settings → Roasting), and your place and website.
- **What it never shows**: the roast's id, its cost, your stock, a street address.
- **Making one**: History → the roast → **Share page** → **Make a share link**, with an
  optional line for the person it is for. Labels with **QR code opens: Its share page** make
  the link when the roast has none (the **gift** design does by default). **Another link**
  makes a second one for someone else, with its own line.
- **Revoke** makes the page a "not found", the same as a code that never existed. Bags printed
  with it open nothing.
- **Scans** are counted (how many, and the last time), without addresses.

The code is 25 random characters: it cannot be guessed. The page loads no scripts and is kept
out of search engines. The address comes from the `public_url` setting (else the address the
request came to), so set it to the address people outside reach your server at. If your
server sits behind a login proxy, let `/R/` through without the login (and rate limit it);
everything else can stay behind it.

## Coffee that is sold: the retail check

Gifts need none of this. For coffee you sell in the US, a design page's **For sale on** choice
(or `roast labels design check --retail pouch-12oz`) reads the laid-out label against the FDA
rules (21 CFR 101.3, 101.5 and 101.7): a bold statement of identity ("Whole bean coffee"), the
net weight in ounces and grams in the bottom 30 % of the label, never rounded up, its letters
as tall as the bag's front needs, and the packer's name, city, state and ZIP. It also flags
words that are nutrition claims ("sugar free"). The type height comes from the bag's front,
not the sticker: pick the bag (`sample-2oz`, `pouch-8oz`, `pouch-12oz`, `pouch-16oz`,
`tin-tie-8oz`, your own in the `label_bags` setting, or "The label is the whole front"). The
**shop** design passes for a 12 oz pouch on 5163 and 6874. It is a reading of the rules, not
legal advice: it cannot know the bag's real size, contrast on your stock, that each bag holds
what it states, or state rules.

## Writing a design

A design is one Typst file (Typst 0.15) that draws **one** label. Roastwarden places it at
every label on the sheet, clipped to the label (a circle on round labels), with the sheet's
offset: a design never deals with sheets. Edit one in Label Studio → Designs → a design: the
Typst source on the left with completion and syntax errors as you type, and beside it the
server's check of the label on sample data, redrawn as you type. **Save** makes a new version
with a note; **Compare**, **Open** and **Back to this** work with older versions, so nothing is
lost and a reprint can match what was printed. A built-in design is saved under a new id.
Ctrl+S saves, Ctrl+Enter checks now.

### The file

```typst
#import "roastwarden.typ": fit, clamp, join, with, ascii

#let design = (                       // optional: a name, a description, settings
  name: "Mine",
  description: "What it is for",
  params: (art: (default: "cup", choices: ("cup", "none"), help: "the drawing")),
)

#let label(l, w, h, round) = {
  // l: the label's fields; w, h: the label's size; round: a round label
  set text(font: "Atkinson Hyperlegible Next", top-edge: "ascender", bottom-edge: "descender")
  fit(l.name, w - 0.3in, max: 16pt, weight: "bold")
  if l.qr { place(right + bottom, image("qr.svg", width: 0.6in)) }
}
```

`label(l, w, h, round)` is the only thing a design must have. Every field of `l` is text, or
`none` when it is unknown or hidden, so handle `none` (`join` and `with` below do it for you).
The `top-edge` and `bottom-edge` line keeps stacked lines from crowding.

**Settings** (`params`) show as a form under the design. A setting's default sets its type: a
value typed as text becomes a number or true/false when the default is one. Colours stay text:
`rgb(l.params.ink)`. The manifest may also set the QR code's default, `qr: "share"` (`app`,
`share`, `supplier` or `none`), and name the message field, `message: "Dedication"` (or
`message: false` for none).

### The fields

| Field | What it holds |
|---|---|
| `name`, `full_name` | The label name (else the coffee's name); the coffee's whole name |
| `origin`, `variety`, `process` | From the coffee ("region, country · producer" for origin). A blend's origin is each country with its share, "60 % Ethiopia · 40 % Kenya", and its varieties and processes are each one's ("natural / washed") |
| `blend`, `components` | True for a blend of coffees in one bag ([roasted coffee](roasted.md)); its coffees, the largest share first, each with `name`, `full_name`, `origin`, `region`, `variety`, `process` and `share` (whole percent), for a design that lists them |
| `level`, `purpose` | "Light", "Espresso" |
| `roasted`, `best_from` | Dates as printed; `roasted_iso`, `best_from_iso` ("2026-10-25") to print a date your own way; `rest_days`. For roasts in one bag, `roasted` is the days ("6–7 Nov 2026") and `roasted_last_iso` the newest day |
| `form`, `ground`, `grind` | "Whole bean" or "Ground · espresso"; true for a ground bag; the grind |
| `best_until`, `best_until_iso` | The bag's best-until date |
| `weight`, `net`, `weight_g` | "250 g · 8.8 oz"; "Net wt 8.8 oz (250 g)" as a US label states it; the grams as a number |
| `batch` | The batch number; "16+17" for roasts in one bag ([roasted coffee](roasted.md)) |
| `notes`, `message`, `roaster` | Tasting notes, the gift line, your roaster's name |
| `brew` | How to brew it, or `none`: `method`, `dose_g`, `out_g`, `ratio` ("1:2.5"), `temp_c`, `time`, `grind`, and `line`, the recipe in a sentence |
| `qr`, `url`, `qr_to`, `qr_modules` | Whether a QR code is wanted (draw it with `image("qr.svg")`), the address, where it goes (`share`, `app`, `link`), its width in squares |
| `brand` | The brand kit: `name`, `tagline`, `place`, `website`, `logo` (an asset's name), `ink`, `accent`, `paper` (`#rrggbb`), `font_display`, `font_body`, `font_script`; each `none` when unset, so a design must cope with no brand |
| `params` | The design's settings, defaults filled in |
| `retail` | Only in a retail check or print: `bag`, `area_sq_in`, `min_net_mm`, `min_mm` |

Hiding a field on the Labels page makes it `none`; hiding the weight hides all three weights,
hiding the dates hides their other forms too.

### The helpers

From `roastwarden.typ`:

| Helper | What it does |
|---|---|
| `fit(text, width, max:, min:, ..style)` | One line at the largest size that fits. |
| `wrap(text, width, max:, floor:, min:, lines: 2, round:, ..style)` | One line down to `floor`, then up to `lines` lines of about equal length down to `min`; `round: true` keeps it inside a circle. |
| `clamp(text, width, lines:, height:, ..style)` | At most N lines, or as many as fit in `height`; cuts after the last whole sentence when that keeps at least half of what fits, else at a word with "…". |
| `join((a, b, c), sep: " · ")` | The parts that are there, joined. |
| `with("Roasted ", value)` | The word and the value, or `none` when the value is `none`. |
| `qr-width(l, module:, min:, max:)` | The QR code's width for squares of `module` (0.4 mm by default). |
| `qr-caption(l)` | What the code opens in a few words ("Scan for this roast and how to brew it"). |
| `ascii(art, size:, font:)` | Monospaced lines exactly as written. |
| `tall(text, height, caps:, min:, ..style)` | The size at which the letters reach a height, measured as labelling rules do. |
| `hint(text)` | A note of your own for the check. |

`fit`, `wrap` and `clamp` tell the check when they shrink, wrap, cannot fit or cut.

### Fonts, images and your own files

Built in, all under the SIL Open Font License: Atkinson Hyperlegible Next, DM Serif Display,
Cormorant Garamond, Pinyon Script, and JetBrains Mono NL (the no-ligatures cut, so `->` in
ASCII art stays two characters). Fonts added under Label Studio → Assets are in every render,
named by their family; images are read by name, `image("logo.svg")`.

You can also keep designs and fonts as files in the `labels` folder beside the database
(`/app/data/labels/` in the container): `designs/<id>.typ` (letters, digits, `-`, `_`) and
`fonts/*.ttf` or `*.otf`. They are read at each render, no restart, and a design there can use
images from its folder. `roast labels --list` shows the folder, the designs and every font
family.

Renders run in a sandbox: no network, no files beyond the design's own, at most 10 s and about
1 GB. Typst packages (`@preview/…`) are not available.

### The check

The check lays out one label and reports, worst first:

| Issue | Meaning |
|---|---|
| `overflow` | Text, an image or a shape past the label's edge (on a round label, past the circle). |
| `overlap` | Text on text. |
| `too_long` | A `fit` or `wrap` at its smallest and still too long. |
| `cut` | A `clamp` dropped words, with how much of the label was still free. |
| `qr_small`, `qr_crowded` | QR squares under 0.33 mm; text within two squares of the code. |
| `near_edge` | Within 1.5 mm of the edge: a printer can land up to 1 mm off. |
| `tiny_text` | Under 5.5 pt. |
| `typst` | A warning, such as an unknown font falling back to another. |
| `param`, `empty` | A setting the design does not declare; nothing printed. |
| `dropped`, `hint` | Soft: the name, roast date or best-from has a value but did not print (hide the field if that is on purpose); your own `hint`. |

It also lists every line of text with its font, size and box in mm, and which fields printed.
A design that does not compile shows Typst's message and line (`design.typ:12: unknown
variable: nmae`), underlined in the editor; the last good picture stays, dimmed. A design must
compile to be saved; its other issues do not stop a save.

From the command line: `roast labels design check mine.typ --png` checks a draft file and
writes the label as a picture, `--diff plain` says what changed against another design, and
`roast labels design add mine mine.typ` saves it. The Agent Skill carries a design guide with
patterns for ASCII art, calligraphy and drawings, so an AI agent can write and check designs
for you.

## From the command line

`roast labels latest` makes the newest roast's labels as a PDF (`--print` sends them to the
printer), `roast labels run` makes a print run, `roast labels printer …` manages printers and
`roast share <roast>` makes a share link. Every option is in [the roast CLI](cli.md).

<!-- site: end -->

## Design notes

For developers: how bag labels work and why. The site publishes only the part above.

Labels for bags of roasted coffee on laser sticker sheets,
filled from the roast, its bean and profile, laid out by Typst on the server and printed from a
PDF. History → a roast → **Labels…**; **Labels** in the top bar is Label Studio (designs,
coffees' labels, assets, brands; below); `roast labels` for agents (`docs/cli.md`).

## Using it

1. A roast's **Labels** (`/history/<id>/labels`): from History → the roast → **Labels…**, from
   the after-roast summary on the Roast page once the roast is weighed (**Labels** ·
   **Report**), or from a lot under **Roasted coffee** on Beans.
2. Pick the **sheet** (Avery 5163, 6874, 5294 round, or your own) and the label to **start
   at** on a part-used sheet: click it in the preview (grey labels are left blank).
3. The roast's labels, the same editor as each roast on Label Studio → **Print**: the
   **design** ("Its own: …" for a coffee with its own label, else plain, or any other; its
   settings show under it), how many **copies**, the **net weight** (the roast's roasted
   weight unless typed: a 250 g bag, 250), the **message** for a gift ("for Sam, with
   love"), a **brand** (the coffee's, else Settings → Roasting's), **tasting notes** (the
   roast's cupping notes, else the bean's) and what shows (untick what should not be on the
   label). A design that prints its own "To" (stamp, noel-tag: a `to` switch, a `from` line,
   "To:" in its description) calls the message **To** and wants just the name; a design with
   a text setting `to` of its own has no Message field; a design can name the field itself
   (`message: "Dedication"` in its manifest, or `message: false`).
4. **Use for <the coffee>** keeps the design, its settings and the brand as that coffee's
   label.
5. **Download PDF**, **Print on …** (when a label printer is set, below, "Printing") and,
   after a Download, **Printed it** stay in view: above the preview on a wide window, at the
   foot of the window on a phone. Print the PDF at **Actual size** (not "Fit to page"),
   paper type "Labels", on sheets sold for **laser** printers, then **Printed it**: the
   sheet's next free label moves on there and then (below, "Print runs"). A second Download
   of the same labels is the same run, not a second copy.

The preview is the server's own rendering of the first page (an SVG of the same layout as
the PDF, `POST /api/label-runs/preview`, the body a Download sends), so what you see is
what prints.

### Label Studio (the Labels section)

Label Studio has its own section, `/labels` (`ui-structure.md`):

- **Designs**: every design on sample data as the server lays it out, at the shape of the
  sheet picked; open one to edit it, or **New design** (from the template or a design).
- **A design's page**: the Typst source in an editor (CodeMirror 6 with
  `codemirror-lang-typst`: Typst 0.15's syntax, completion of its functions, folding, syntax
  errors as you type) and, beside it, the check the server runs 400 ms after typing stops
  (`POST /api/labels/check`, the same an agent runs): the label at 160 dpi, what to fix, the
  whole sheet (`POST /api/labels/render`), and the report as an agent reads it. Typst's
  errors carry their span (`from`, `to`: UTF-16 offsets in the design's file) and are
  underlined there; while the source does not compile the last good picture stays, dimmed.
  The data is sample coffee, a roast, a coffee (`bean`: its name and origin on a sample
  roast) and a brand; the design's settings are a form. Save makes a new version (with a
  note); Compare shows a version beside the editor, Open loads it, Back to this rolls back
  (a new version). A built-in or folder design is saved under a new id. Ctrl+S saves,
  Ctrl+Enter checks now.
- **Print** and **History**: print runs (below, "Print runs"); **Printers** (below).
- **Coffees**: each coffee's own label (design, settings, brand) with a preview on that
  coffee.
- **Assets**: drop or pick images and fonts; font samples; the Typst line that uses one.
- **Brands**: brand kits with a live preview on the **shop** design (`brand_kit`: a kit
  given whole in a check or render, not saved); **Make default** sets `label_brand`.

### Calibration

Printers feed paper a little off. **Calibrate this sheet** (folded under the form): print the
calibration page on plain paper (an L 3 mm inside every label corner, for round labels a
circle 3 mm inside and its centre, a mm scale across each label edge, and a 100 mm rule to
check the printer did not scale), hold it over a sheet of labels against the light, and read
where each label's edge crosses its scales: a tick each mm, the long tick where the edge
should be. That is how far the marks must move, mm right and down; enter it. Nothing on the
page prints within 4.5 mm of the paper's edge (the printer's margin is 4.2 mm): a scale that
would is left out, and the label's other scales read the same offset. **Save offset** keeps it
(`label_offsets`, per sheet) for every browser and for `roast labels`; print the page again to
check. Acceptance: within 1 mm of every label edge on the LaserJet it was built against.

### Best from

Roast date (the DROP's date in the owner's time zone: the request's `tz`, the browser's or the
CLI machine's, else the `time_zone` setting, else UTC) plus a rest by purpose and level:
**espresso 10 days light, 7 darker; filter 7 days light, 4 darker** (omni and other read as
filter; light means up to medium-light; no level reads as light), from roasters' published rest
advice (the research behind the rest windows). Change them in Settings → Roasting
(`rest_days_espresso_light`, `rest_days_espresso`, `rest_days_filter_light`,
`rest_days_filter`), with the optional roaster name printed on labels (`label_roaster_name`).
Roasted coffee's "ready from" is the same date (`roasted.md`).

### Whole bean or ground

A label (`lean-home.md`) is for **whole bean** (the default) or **ground** coffee with a grind:
`form` (`whole` or `ground`) and `grind` ("espresso", "filter", "moka", "French press", or the
owner's word; a grind alone means ground) on a label request, a run's item, or `roast labels …
--ground espresso`. The label's data gets `form` ("Whole bean", "Ground · espresso"), `ground`
(bool), `grind`, and `best_until` / `best_until_iso`: the lot's best-until (`roasted.md`), or
for a ground bag the earlier of that and today (the pack date) plus `best_until_days_ground`
(Settings → Roasting, default 14). "Best from" is unchanged. **Plain** prints the form line
under "Best from" ("Whole bean"; a ground bag "Ground · espresso · best until 15 Nov 2026"); on
a round label only a ground bag's, the bean line giving it room. **Shop** says "Ground coffee"
for the statement of identity when the bag is ground (unless the owner wrote their own) and adds
the ground form line; a whole-bean bag keeps its usual statement and no form line. The other
designs print no form line. `hide: ["form"]` leaves it off. A print run can take the bags off
the roast as packed (`pack`, `roasted.md`, "Packed bags").

## Stock: yes, standard laser label sheets

US Letter label sheets for laser printers are standard (Avery and generic brands). Buy sheets
marked **laser**: inkjet-only stock can smear toner or let adhesive ooze in the fuser. Set
the printer's media type to "Labels" (hotter fuser). Most LaserJets are mono, so labels are
designed for black only; kraft paper with black print is the usual coffee look.

Geometry from gLabels' Avery US templates (inches; top and left are page margins to the first
label, pitch is label edge to next label edge):

| Avery | Label | Per sheet | Top | Left | H pitch | V pitch | Use |
|---|---|---|---|---|---|---|---|
| 5163 | 4 × 2 | 10 | 0.5 | 0.1625 | 4.1875 | 2 | the common bag label |
| 6874 | 3.75 × 3 | 6 | 0.625 | 0.375 | 4.0 | 3.375 | room for tasting notes and a QR |
| 5294 | 2.5 round | 12 | 0.425 | 0.21 | 2.75 | 2.515 | round seal (margins to the circle's box) |
| 5164 | 4 × 3⅓ | 6 | 0.5 | 0.15625 | 4.1875 | 3.333 | large |
| 5126 | 8.5 × 5.5 | 2 | 0 | 0 | — | 5.5 | half sheet |

Not yet measured: Avery 22807 (2" round, 12-up) and 22808 / 22830 (2.5" round, 9-up; 22808 is
kraft). Generic "compatible" sheets can differ from Avery's layout (OnlineLabels OL125 is
4 × 2 but pitch 4.14"), so templates are data, editable, with offsets.

Sources: glabels-qt `templates/avery-us-templates.xml`; Avery help pages for 22807/22808;
OnlineLabels OL125 / OL2682; OnTime and AA Labels on laser vs inkjet stock.

Your own stock: the `label_sheets` setting, a JSON list of sheets in inches (one with a
built-in id replaces it), e.g. OnlineLabels OL125:
`[{"id":"OL125","name":"OnlineLabels OL125","page_w":8.5,"page_h":11,"w":4,"h":2,"cols":2,"rows":5,"left":0.16,"top":0.5,"pitch_x":4.14,"pitch_y":2}]`
(`"round": true` for round labels). `roast settings set label_sheets=- < sheets.json`.

## What goes on a label

From the roast, its bean and the settings; each field can be left off (`hide`): `origin`
(region, country · producer), `variety`, `process`, `level` (level and purpose), `roasted`
(date), `batch`, `best_from`, `form` (whole bean or ground), `weight` (g and oz), `notes`, `qr`
(a QR code), `roaster`.
The name is the bean's **label name** when it has one (a short name for a long one, "Kenya
Gatugi"), else the bean's name, else the roast's. A gift `message` is printed only when given.

**The notes**: what the label says it tastes like, first found of

1. the notes asked for (`--notes`, the Labels page's Notes);
2. the bean's **label notes**, a few tasting words in the owner's words, "Chocolate ·
   coconut · apple" (Beans → the bean → Edit → Label notes; `roast bean edit <bean>
   --label-notes "…"`; at most 120 characters). With none, the bean shows a suggestion
   picked from the supplier's notes (tasting words from the flavour wheel, in their order,
   at most four: `label_notes_suggested`, the form's "Use …" link); an agent writes better
   ones from the same prose (the skill says how);
3. the roast's own cupping notes;
4. the supplier's cupping notes, whole sentences up to about 140 characters (a longer first
   sentence cut after a clause, with "…"), never cut mid-word.

A design's `clamp` cuts after the last whole sentence when that keeps at least half of
what fits, so even the supplier's prose ends at a full stop.

**Words**: the level and the purpose print as words, "Light · Espresso"; the weight
everywhere as "250 g · 8.8 oz" rounded down like the net weight, never more than is in the
bag. **How to brew it** (`l.brew`) is the share page's recipe, one source for both: the
owner's dose (Settings → Roasting), 1:2.5 for a light espresso at 94 °C (1:2.2 at 93 °C
medium, 1:2 at 91 °C dark), 27–30 s; filter 1:16 at 96/94/90 °C, about 3 minutes; none for
an omni roast.

The QR code opens the roast in Roastwarden (`<public_url>/history/<id>`, `app`, the default),
its public **share page** (`share`, below), the coffee's offer at its supplier (`supplier`, the bean's link), nothing (`none`), or any address: `qr` in a
request, `--qr` for agents, the **QR code** choice on the Labels page; with none asked, the
design's own default (its manifest's `qr`: the **gift** design says `share`, as a gift's
recipient has no login for the app), then the `label_qr` setting. A design's default that
cannot be met (no share link and a token that may not make one, a coffee with no supplier
link) falls back to `app`. Making a gift's PDF or printing it makes the roast's share link if
it has none; a preview shows a placeholder code.

FDA labelling (statement of identity, net quantity in oz and g in the bottom 30 % of the front
panel, the packer's name and place) matters only if coffee is ever sold; a hobbyist's own and
gift bags need none of it. Coffee beans are exempt from nutrition facts (21 CFR 101.9(j)(4)).
The retail check (below) reads a design against them.

## Designs

A design is one Typst file (Typst 0.15) that draws one label:

```typst
#import "roastwarden.typ": fit, clamp, join, with, ascii

#let design = (                       // optional: a name, a description, parameters
  name: "Mine",
  params: (art: (default: "cup", choices: ("cup", "none"), help: "the drawing")),
)

#let label(l, w, h, round) = {
  // l: the label's fields (text, or none when unknown or hidden), l.params (the design's
  // parameters, defaults filled in), l.qr (true when the QR code is wanted:
  // image("qr.svg")); w, h: the label's size; round: a round label
  set text(font: "Atkinson Hyperlegible Next")
  fit(l.name, w - 0.3in, max: 16pt, weight: "bold")
}
```

The driver places it at every label position (clipped to the label, a circle for round
ones), after the start label, with the sheet's offset; designs never deal with sheets.
Fields: `name` (the label name, else the bean's), `full_name` (the bean's whole name),
`origin`, `variety`, `process`, `level` ("Light"), `purpose` ("Espresso"), `roasted`, `batch`
(number; "16+17", text, for a combined lot: `roasted.md`, "Roasts in one bag"), `blend`
(bool) and `components` (a blend's coffees: `name`, `full_name`, `origin`, `region`,
`variety`, `process`, `share`; empty otherwise, and when `origin` is hidden), `best_from`,
`roasted_iso` and `best_from_iso` ("2026-10-25": for a design that
prints a date its own way, "29 Oct" or the weekday), `rest_days` (number; hiding `roasted`
or `best_from` hides these too), `form` ("Whole bean", "Ground · espresso"), `ground` (bool),
`grind`, `best_until` and `best_until_iso` (the bag's: a ground bag's is earlier; hiding
`best_from` hides them), `weight` ("250 g · 8.8 oz"), `net` ("Net wt 8.8 oz (250 g)": ounces
and grams as a US label states them, never more than is in the bag, with pounds and ounces
from 1 to 4 lb), `weight_g` (number), `notes`, `message`, `roaster`, `brew` (how to brew
it, or none: `method`, `dose_g`, `out_g`, `out_is` "Yield" or "Water", `ratio` "1:2.5",
`temp_c`, `time`, `grind`, `line` the recipe in a sentence), `qr` (bool), `url`, `qr_to`
(where the code goes: `share`, `app` or `link`), `qr_modules` (the code's width in squares),
`brand` (below) and `params`. A design's manifest may name its QR code's default, `qr:
"share"` (`app`, `share`, `supplier` or `none`). Hiding `weight` hides all three weights. A parameter's default sets its type: a value given as text (`--set size=12`, the
dashboard's form) becomes an int, float or bool when the default is one; colours stay text
(`rgb(l.params.ink)`).

Helpers in `roastwarden.typ`: `fit(text, width, max:, min:, ..style)` (one line, the largest
size that fits), `wrap(text, width, max:, floor:, min:, lines: 2, round:, ..style)` (one
line down to `floor`, then up to `lines` lines of about equal length down to `min`, narrower
inside a circle with `round: true`: the built-in designs' name and gift message),
`clamp(text, width, lines:, height:, ..style)` (at most N lines, or as many as fit in
`height`, cut after the last whole sentence when that keeps at least half of what fits, else
at a word with "…"), `join((a, b, c), sep: " · ")` (the parts that are there), `with("Roasted ", value)`,
`qr-width(l, module:, min:, max:)` (the QR code's width for squares of `module`, 0.4 mm:
`image("qr.svg", width: qr-width(l))`), `qr-caption(l)` (what the code opens in a few words,
"Scan for this roast and how to brew it"), `hint(text)` (a soft note for the check),
`ascii(art, size:, font:)` (monospaced lines exactly as written), `tall(text, height, caps:,
min:, ..style)` (the size at which its letters reach a height, as labelling rules measure
them: "The retail check"). `fit`, `wrap` and `clamp` leave a
note for the check (`metadata` labelled `<rr-note>`) when they shrink, wrap, cannot fit, or
cut.
Tip: `set text(top-edge: "ascender", bottom-edge: "descender")` so stacked lines keep their
space.

Built in (`crates/server/assets/labels/`):

- **plain**: Atkinson Hyperlegible Next (the dashboard's type), everything at a glance, the QR
  code on the right; a centred stack on round labels. When everything, the notes whole,
  fits, the type grows (up to 1.4× on a tall label, 1.15× on a 4 × 2 in one) and the block
  is centred, so a roomy label is not a third empty; when it does not, the notes take the
  height that is left.
- **gift**: DM Serif Display for the name, Pinyon Script for the message, Cormorant Garamond
  for the rest, a double frame (two rings on round labels), the QR code only where there is
  room (6874), to the share page by default, sized by its squares, with a caption beside it.
- **script**: the name in Pinyon Script over a drawn flourish, small capitals in Cormorant
  Garamond. Parameters: `title` (a script line instead of the name, "Sam's Blend"), `top`
  (the small line above), `flourish`.
- **ascii**: a typewriter ticket in JetBrains Mono NL, a drawing in characters beside dotted
  rows. `art`: cup, bean, pot, cherry, none, or your own lines (`--set art=@cup.txt`);
  `rule`: the rule's character.
- **cherry**: line art of a coffee branch with two cherries (or a leaf, or a roasted bean),
  drawn with Typst's shapes, a DM Serif Display name. `art`, `ink` (a colour for a colour
  printer).
- **shop**: for coffee gifted or sold under the owner's name. The brand kit on
  top (logo, name and tagline in its fonts and colours), the coffee in the middle, and in the
  bottom 30 % what a bag that is sold needs: what it is (`identity`, "Whole bean coffee"),
  `l.net`, and "Roasted and packed by" the brand's name and place. The Brands page previews
  a brand on it.

**Your own designs and fonts** go in the `labels` folder beside the database: in the image
`/app/data/labels/designs/<id>.typ` (the id: letters, digits, `-`, `_`) and
`/app/data/labels/fonts/*.ttf|otf`; `GET /api/labels` (or `roast labels --list`) shows the
folder, the designs and every font family. Read at each render: no restart. A design can
include images from its folder (`image("logo.svg")`; only files whose names the design
mentions are read). Draft one with `roast labels design check` (below), then copy it in:

```sh
# on the server's host
docker exec roastwarden mkdir -p /app/data/labels/designs /app/data/labels/fonts
docker cp mine.typ roastwarden:/app/data/labels/designs/
```

A design that does not compile answers 422 with Typst's message and line
(`design.typ:12: unknown variable: nmae`), shown above the preview. The built-in fonts are
all under the SIL Open Font License (`crates/server/assets/fonts/OFL-*.txt`): Atkinson
Hyperlegible Next, DM Serif Display, Cormorant Garamond, Pinyon Script, JetBrains Mono NL
(the no-ligatures cut, so `->` in ASCII art stays two characters).

## Designing a label: the check

`POST /api/labels/check` (`roast labels design check draft.typ`) lays out **one** label of a
design, saved or sent as source, on a page of the label's size and without clipping, with a
roast's data or sample data, and answers:

- `errors`: Typst's, each with `file`, `line`, `column`, `message`, `hints`, when it does not
  compile (and no report);
- `report.issues`, worst first: `overflow` (text, an image or a shape past the edge, by how
  much and which edge; on a round label text corners are measured against the circle),
  `overlap` (text on text), `too_long` (a `fit` or `wrap` at its minimum and still too long), `qr_small`
  (squares under 0.33 mm, 13 mil) and `qr_crowded` (text within two squares), `cut` (a `clamp`
  dropped words; "while 21 mm of the label's height is free" when more than the margins
  was left empty), `typst` (a warning: an unknown font falls back to another), `param` (a
  parameter the design does not declare), `empty`, `near_edge` (within 1.5 mm: a printer
  lands up to 1 mm off), `tiny_text` (under 5.5 pt); and two soft ones that leave the
  report `ok`: `dropped` (the name, the roast date or the best-from date has a
  value and did not print: what a bag's label is for; hide the field to leave it off on
  purpose) and `hint` (the design's own note: shop with no brand and no roaster name says
  its top is empty and it names no packer);
- `report.free`: the height nothing printed in, above the first line or image and below the
  last (`top_mm`, `bottom_mm`; frames and drawn shapes do not count);
- `report.lines`: each line of text as laid out (pieces on one baseline joined), its font,
  size and ink box in mm; `report.images` (the QR code with its square size, SVG, raster);
  `report.fields`: the fields with a value that printed and that did not (found by their
  words, whatever the case, punctuation or slug; a date in its usual forms, "29 Oct",
  "October 29", "2026-10-29"; the weight by either part; the message also without its
  leading "for" or "to", for a design that prints its own "To": "for Sam" as "To Sam");
  `report.manifest` and `report.params`; `report.notes` (what `fit`, `wrap` and `clamp` did);
- with `png` (dots per inch, 36 to 200), the label as a PNG with its edge dashed.

The report comes from the laid-out frames (Typst's own boxes for every glyph run, shape and
image, through groups, transforms and clips), so it is exact where a picture is approximate;
text first, pixels only when asked keeps an agent's turns cheap: a 4 × 2 in label at 100 dpi is
about 100 image tokens, at 150 dpi about 240. Why: vision models misjudge fine geometry
(BlindTest), structured feedback drives the best render-loop agents (Playwright MCP's snapshots,
DeTikZify's compiler-guided search), and Typst is thin in models' training data with syntax that
changed in 0.13, 0.14 and 0.15, so the line and hint of each error matter.

`POST /api/labels/render` makes the PDF (or SVG) of labels from a design's source that is
not saved, or a saved one with parameters: `roast labels <roast> --source draft.typ --set
art=cup`. Both are calls any token may make (they change nothing); a POST needs a token.

**Agents**: the skill's [`label-design.md`](../skills/roastwarden/label-design.md) is the
design guide (the contract, a template, the helpers and fonts, Typst 0.15's traps, patterns
for ASCII art, calligraphy and drawings, printing). A test compiles every example in it.

### The sandbox

Every render, the owner's designs and an agent's alike, runs in a child process: the server
binary started as `roastwarden-server label-render`, reading one request (CBOR: the design, the
data, the fonts and the files it names) on stdin and answering on stdout (`labels/sandbox.rs`,
`labels/engine.rs`). Typst has no step limit for `for` loops: nested loops over `range(100000)`
run for hours, and `range(200000000)` reaches 6 GB. The worker sets its own limits before it
reads the request: 1 GB of address space, 15 s of CPU, no file writes, 16 file handles; the
server kills it after 10 s and answers the design's error ("took longer than 10 s", "needed more
than 1024 MB"). It gets an empty environment, and its Typst world has no files beyond the
request and no network (`@preview/…` packages are refused with a hint). Two renders run at once;
more wait. Tests render in the server process; `ROASTWARDEN_LABEL_SANDBOX=0` does too.
`tests/label_sandbox.rs` runs a runaway loop and a 3 GB array against the real binary.

## Saved designs, assets and brands

Designs worth keeping are saved in the database (migration 030), so the nightly backup has
them, with **every version**: `sam-gift` is the current one, `sam-gift@2` a version, so a
reprint can match what was printed. A save runs the check on sample data first and refuses a
design that does not compile (422 with Typst's lines); its issues come back with the save
but do not stop it. Editing the source makes the next version; **rollback** copies an old
version's source into the next one, so nothing is lost. A saved design's id may not be a
built-in's or a folder design's. Each version keeps who saved it (`owner`, or the token's
name) and a note.

- `GET /api/label-designs` every design (saved with version and parameters, folder, built
  in); `GET /api/label-designs/{id}` (or `{id}@N`) its source and versions, also for a
  built-in to start from; `POST /api/label-designs`, `PUT /api/label-designs/{id}`,
  `POST /api/label-designs/{id}/rollback`, `DELETE /api/label-designs/{id}` (refused while a
  coffee uses it).
- **Assets**: images (SVG, PNG, JPEG up to 2 MB) and fonts (TTF, OTF up to 8 MB) by name:
  `PUT /api/label-assets/{name}` with the file as the body. A design reads an image by its
  name (`image("logo.svg")`); a render gets only the images whose names its source, its
  parameters or its brand's logo mention. Every font asset is in every render, named by its
  family (`roast labels asset list` shows it).
- **Brand kits** (`PUT /api/label-brands/{id}`): the roaster's name, tagline, place, website,
  a logo (an image asset), colours (`#rrggbb`: ink, accent, paper) and fonts (display, body,
  script). A design reads `l.brand` with every key there (`none` when unset), so it must
  handle a missing brand. The brand comes from the request (`brand`), else the bean's, else
  the `label_brand` setting.
- **A coffee's own label** (`PUT /api/beans/{id}/label`, `roast labels design use`): a design
  (`id`, or `id@N` to pin a version), its parameters and a brand. A roast of that bean gets
  them unless the request names a design or a source (its parameters then add to the
  bean's).

Who: reads for every token; saves, edits, assets, brands and a coffee's label for
`inventory` and `admin` tokens (the coffee and its bags); deletes of designs, assets and
brands are held for the owner's approval for a token that asks first.

## Print runs

A **run** (migration 031) is one sheet of labels
(or more, when they spill over) for one roast or several at once: a gift run of three
coffees, each roast with its own design (its coffee's, unless one is picked), settings,
brand, copies, message and QR code, its labels kept together in the order given. Label
Studio → **Print** builds one (pick roasts, see the whole sheet, click the first label; each
roast has the roast page's editor, its settings, brand, notes and fields under **More**);
a roast's Labels page makes a run of that roast's labels; both have the same **Download
PDF**, **Print on …** and **Printed it**; an agent uses `roast labels
run` (`docs/cli.md`).

- **Kept**: each run is saved with what each label carried (the roast's data as it was,
  the design's `id@version` and its source, the settings, the brand as read, the QR code's
  target), the sheet and first label, who made it, and its PDF. Label Studio → **History**
  lists them; a run's PDF can be downloaded again at any time.
- **Printed**: a run is sent to the label printer (`POST /api/label-runs/{id}/print`; a
  token's print is held for the owner's approval, which says what to load: "6 labels (…) on
  a 5163 sheet from label 3 (labels 1–2 used)"). Roastwarden keeps the printer's job states
  with their times, following the job for 45 s and then every 5 s for up to 15 minutes, so
  a print that waited for paper ends up "printed" (or "aborted") in History. A run made as
  a PDF and printed from a browser is marked **Printed it** (`…/done`): on the page that
  made it, or in History.
- **Made once**: a make with the same labels as a run made in the last 24 hours and not yet
  sent or printed (the same sheet, first label, printer, note, and every item as laid out:
  design and source, settings, brand, data, QR code) answers that run (200, `reused:
  true`) instead of keeping a second copy, so downloading twice and marking it printed once
  moves the sheet on once. A changed label (one more copy, another message) is a new run;
  `new: true` (`roast labels run --new`) always makes one; a reprint is always new.
- **The next free label**: each sheet remembers where its part-used sheet is
  (`label_sheet_use`). It moves on when a job completes or a PDF is marked printed, never
  merely when a run is made; a run starts there unless told otherwise; a full sheet means a
  new one next (label 1). **New sheet** (History, `roast labels sheets --set 5163=1`)
  starts again by hand.
- **Reprint**: a new run with a run's labels exactly as they were (the same data, design
  source and QR codes, not today's), all of them or the ones ticked, from the sheet's next
  free label or another; the sheet's offset is today's.
- **One sheet, several designs**: the render world holds each design in its own folder
  (`g1/design.typ` with its own `g1/qr.svg`; anything else it names falls back to the root),
  so a design that draws `image("qr.svg")` gets its own roast's QR code with no change.
  A run holds 1 to 30 roasts and 1 to 200 labels.

`GET /api/label-runs` (newest first), `GET /api/label-runs/{id}`, `GET …/pdf`,
`POST /api/label-runs` (make: `{sheet, start, items: [{roast, count, design, params, brand,
message, notes, weight_g, form, grind, pack, hide, qr}], note, printer, new}`; 201 a new run,
200 the same labels' run not yet printed; `pack` takes the item's bags off its roast,
`roasted.md` "Packed bags", and the run answers them as `packed`), `POST /api/label-runs/preview` (the first sheet as
SVG, nothing kept), `POST …/print`, `POST …/done`, `POST …/reprint` (`{start, labels: [3, 4],
note}`), `GET /api/label-sheets`, `PUT /api/label-sheets/{id}` (`{next}`). Making and printing
runs: `roast`, `inventory` and `admin` tokens; reads for every token.

## Printing

Roastwarden can send labels straight to a printer over IPP (the protocol AirPrint
and IPP Everywhere printers speak): **Print** on the Labels page, `roast labels latest
--print` for an agent.

1. Set the printer up once: Label Studio → **Printers** (below, "Printers"), e.g.
   `ipp://192.168.1.143:631/ipp/print`. The printer this was built against is an HP Color
   LaserJet Pro M252dw (`avahi-browse -rt _ipp._tcp` on the LAN finds it; its `rp` is
   `ipp/print`). mDNS does not cross subnets, so a server on another subnet does not find it
   itself; the address is typed. (An older `label_printer_url` setting becomes the default
   printer with migration 032, and is read only while no printer is set up.)
2. `GET /api/labels/printer` (`roast labels printer`) asks the default printer (or
   `?printer=`) what it can do and how it is: make and model, state and reasons
   (`door-open-error`, `media-empty-error`), the formats it takes, the "labels" paper type,
   actual size, colour. It prints nothing.
3. A print asks the printer's state (a stopped printer answers 409 with its reasons, nothing
   is sent), picks the format (PDF, else PWG Raster, else URF, or the printer's `send`), and
   sends Print-Job with `print-scaling` `none` (the M252dw's default is `auto`, which would
   shrink the sheet), `media-col` with the sheet's size, `media-type` `labels` (a hotter
   fuser) when offered and the tray, and `print-color-mode` when a colour is set.

**At the printer** (the M252dw): load the label sheet in **tray 1**, the hand-fed slot,
face up as the printer's tray picture shows, **before** printing; it then prints at once.
A print sent with tray 1 empty waits (`stopped`, "the printer is waiting: feed the 5163
sheet into tray-1 and press OK on the printer"): feed the sheet, then press **OK** on its
screen. Tray 2, the cassette, keeps plain paper. Roastwarden asks for
the tray that holds paper of the sheet's size (`media-col-ready`), which on the M252dw is
tray 1.

After sending, Roastwarden follows the job (Get-Job-Attributes every 3 s, up to 45 s) and
answers what the printer did: printed, still printing, waiting for paper, or **aborted**
(an error, 502: "the printer took the job and then aborted it"). A printer accepts a job
before it reads it, so "accepted" was never proof of a print.

**Why the job is sent as it is**: a print can be accepted and then aborted with 0 pages. Two
causes, found by test prints on plain paper: (1) a paper description that names a type without
a size (the printer substitutes, then aborts), so the job carries `media-size` from the sheet's
page, `media-type`, and the tray; (2) the HP answers a Print-Job before it has read the whole
PDF, and an HTTP client that stops sending once the answer arrives leaves a cut-off document.
So Roastwarden writes the whole IPP request onto the connection itself and then reads the
answer by its length (the printer keeps the connection open even when asked to close it). The
same bytes sent with `curl` or `ipptool` print every time.

**Approval**: a print from a token is always held for the owner's approval, whatever the
token's "ask first" switch says: it uses a sheet of labels, and someone must load it. The
approval says what to load: "Print 2 labels of "Guji 1 (#7)" in sam-gift@2 on 5163 from
label 4 (load a 5163 sheet; labels 1–3 used)". Approving sends it. The owner key (the
dashboard) prints at once. Roastwarden holds the job, not the printer: `job-hold-until` is
optional in IPP, the M252dw does not offer it, and a job left held on a printer is easily
lost.

The M252dw's unprintable margin is 4.2 mm on each side (`media-col-ready`). On the Avery
5163 the outer labels' edges are inside it (4.1 mm from the paper's left edge, 3.8 mm from
its right), so nothing may print in a label's outer 0.5 mm there; the built-in designs keep
text at least 0.14 in (3.6 mm) inside a label (7.4 mm from the paper's edge), and the
calibration page prints nothing within 4.5 mm of it.

## The retail check

For coffee that is **sold** (gifts need none of
it): `POST /api/labels/check` with `retail` (a bag type), `roast labels design check
--retail pouch-12oz`, or the design page's **For sale on** choice, reads the laid-out label
against the US rules, 21 CFR 101.3, 101.5 and 101.7 (the net quantity rule was 101.105
until 2016). A reading of the rules, not legal advice.

- **The bag's front sets the type height**, not the sticker: the principal display panel is
  the side of the package most likely shown, and a spot label's minimum comes from that
  panel's area (Wisconsin's rule says so outright). 101.7(i): 1/16 in up to 5 sq in, 1/8 in
  to 25, **3/16 in to 100** (a 12 oz valve pouch, 6¾ × 10½ in, is 70.9 sq in: 4.8 mm), 1/4
  in to 400, 1/2 in above. Bag types (`labels --list`): `sample-2oz`, `pouch-8oz`,
  `pouch-12oz`, `pouch-16oz`, `tin-tie-8oz`, and the owner's in the `label_bags` setting
  (`[{"id":"kraft-12","name":"12 oz kraft","front_w":6.0,"front_h":9.5}]`); `label` when
  the label is the whole front; the default is `label_bag`, else `pouch-12oz`.
- **Heights are letters, measured as the rules do**: the lowercase "o" (the font's x-height)
  in mixed case, the capitals when a line is all capitals. The report gives each line's
  `x_mm`, `cap_mm` and whether it is `bold`, from the font's own metrics.
- **What it checks**: the **statement of identity** (a line with "coffee", bold, its
  capitals at least half the largest print's, FDA's guide); the **net quantity** (a "Net wt"
  line with ounces or pounds and grams, both required by the Fair Packaging and Labeling Act;
  never rounded up: "12 oz (340 g)" claims more than 340 g holds; in the label's bottom
  30 %, 34 % of a round label's height; bold; its letters at the bag's height; the space
  above it at least its letter height); the **name and place of business** (the brand's
  name, and a line with city, state and ZIP; the street unless listed in a directory; at
  least 1/16 in); and words that are nutrition claims ("sugar free", "healthy"), which lose
  coffee's exemption from Nutrition Facts (101.9(j)(4)). Findings are `retail_identity`,
  `retail_net`, `retail_net_size`, `retail_net_space`, `retail_packer`, `retail_claim`, with
  the size to use ("needs 4.8 mm: about 27 pt in Atkinson Hyperlegible Next").
- **What no check can know**, listed with every answer: that the label goes on the front,
  the bag's real size, contrast on the real stock, that each bag holds what it states,
  whether a blend or flavour needs an ingredient list, a second front, state rules (cottage
  food, permits, weights and measures), and claims such as organic.
- **Designs meet it with the label's data**: a check or print with `retail` gives the label
  `l.retail` (`bag`, `area_sq_in`, `min_net_mm`, `min_mm` 1.6), and the helper `tall(body,
  height, caps:)` sizes text until its letters reach a height. The **shop** design uses them:
  the net weight alone in the bottom 30 %, in capitals at the bag's height, "Whole bean
  coffee" in bold at half its capitals, "Roasted and packed by …" and the brand's place (with
  the ZIP) at 1/16 in or more; on a 4 × 2 label the brand header gives way. It passes for a 12
  oz pouch on 5163 and 6874 (tested). `l.net` already never rounds up.

`check --diff <design@N>` (the CLI) lays out another design or version on the same data and
says what changed: lines added, gone, moved or resized (mm), issues new and gone.

## Printers

More than one printer (migration 032): Label Studio → **Printers**, `roast labels printer …`
for agents (admin tokens change them).

- **Each printer** has an id, a name and an address, and is asked what it can do when it is
  added and whenever it is checked (Get-Printer-Attributes: state, formats, PWG and URF
  resolutions and colour spaces, colour modes, trays and their paper); the answer is kept
  with the time. The Printers page asks each printer again when it opens, showing the last
  answer with its age ("asked 4 days ago") meanwhile. One printer is the **default**: prints go there, and runs are laid out for
  its calibration, unless another is named (`printer` in a run or a print, `--printer`).
- **Tray and colour**: a printer may name the tray to ask for (else the tray holding paper
  of the sheet's size) and a colour (`monochrome` or `color`; else black unless a label has
  colour on it and the printer prints colour); a print may override both.
- **Printers without PDF** get raster made from the same layout (`labels/raster.rs`, our own
  encoder: no Rust crate that writes it was found): **PWG Raster** (PWG 5102.4, what
  IPP Everywhere printers must take) when the printer lists `image/pwg-raster` with its
  resolutions, else **Apple URF** (AirPrint; the M252dw's raster is URF and PCLm, not PWG)
  from `urf-supported`. Rendered at the lowest resolution of 300 dpi or more it offers; 600
  and up is rendered at 300 and each pixel sent twice across and down (a quarter of the
  memory: a Letter page at 300 dpi is 34 MB of pixels). Pages without colour go as 8-bit
  grey. Each line is coded as the specs say (a repeat count, then runs of a pixel or of
  literal pixels); the tests read every page back. `send` on a printer forces `pdf`, `pwg`
  or `urf`.
- **`ipps`**: printers' certificates are self-signed, name no address and expire (the
  M252dw's: SHA-1, no subjectAltName, expired 2025-01-01), so no authority vouches for them.
  Roastwarden trusts one by its SHA-256: **Read its certificate** (`roast labels printer
  certificate <address>`) shows the fingerprint, which the owner compares with the
  printer's own page before ticking "trust it"; afterwards only a printer showing that
  certificate is reached (a custom rustls verifier: fingerprint in constant time, name,
  expiry and issuer not checked, the handshake's signatures checked as usual). Another
  certificate is refused and the error says the new fingerprint. The M252dw stays on
  `ipp://`: its TLS is RSA key exchange only, which rustls refuses.
- **Calibration per printer and sheet** (`label_calibrations`): where a printer's labels land
  on a sheet, falling back to the sheet's own offset (`label_offsets`), then none. Print
  the calibration page for that printer, measure the first label's marks, and the last's:
  the two must agree within 0.5 mm, else the printer is scaling the page (print at Actual
  size) or the sheet feeds askew, which no offset fixes, and the save says so. The tray
  measured from is kept.
- **Finding printers** (`GET /api/label-printers/find`, 4 s of mDNS `_ipp._tcp` and
  `_ipps._tcp`) finds printers only on the server's own network segment: in Docker that
  needs host networking (`docs/DEPLOY.md`); on a server a subnet away from the printer, it
  finds none. Typing the address always works.

`GET /api/label-printers`, `POST /api/label-printers`, `GET|PUT|DELETE
/api/label-printers/{id}`, `POST …/{id}/default`, `PUT|DELETE
…/{id}/calibration/{sheet}` (`{x_mm, y_mm}` or `{first: [r, d], last: [r, d]}`),
`POST /api/label-printers/certificate`, `GET /api/label-printers/find`. Reads for every
token; changes for `admin`; a token's delete may be held for approval.

## The share page

A bag given away or sold has a QR code that should open something for the person holding it, not
a login. A roast's **share page** (migration 033), `<public_url>/R/<code>`, is made by the
server for anyone with the link:

- **What it shows**: the brand (logo, name, tagline), the gift line when the link has one,
  the coffee's name, origin and producer, the roast level and purpose, the roast date and
  **best from** (and, on the day it is opened, "Resting: at its best in 3 days" or "Ready:
  roasted 9 days ago"), the tasting notes, variety, process and batch number, the roast
  curve on chart paper (°C and °F, development shaded, dry end, first crack and drop marked;
  roast time, development time and %, drop temperature), the brew recipe from the
  purpose, the level and the owner's doses (Settings → Roasting; the same recipe a label
  reads as `l.brew`, "What goes on a label"), and the brand's place and website.
  Never the roast's id, its cost, stock or a street address. Light and dark, phone first;
  the brand's ink, accent and paper colours.
- **The link**: a 25-character code (base32, 125 random bits: not guessable, no counter).
  The QR code spells the whole address in capitals (`HTTPS://HOST/R/CODE`; scheme and host
  ignore case and `/R/` and `/r/` both route), so it uses QR's alphanumeric mode and comes
  out smaller. A roast has one link unless the owner makes another (`new`: someone else,
  their own message); its labels use its newest. A label with `qr: share` makes the link
  when the roast has none, but only for a caller who may make links, and only when the
  labels are made (a PDF, a print, a print run): a preview or a check of a roast with no
  link shows a placeholder code (`SAMPLESHARECODE2345ABCDEF`, the same size of QR code) and
  makes nothing.
- **Revoking** a link makes its page a 404, the same page and status as a code that never
  existed. Bags printed with it open nothing; sharing again makes a new code.
- **Scans** are counted (how many, the last time), without addresses; the page is not in
  the audit log (only `/api/` writes are).
- **The page loads nothing**: no script at all, the logo inline, its own Content Security
  Policy (`default-src 'none'`), `Referrer-Policy: no-referrer`, `X-Robots-Tag: noindex`
  and `Cache-Control: no-store` (a revoke takes effect at once).
- **Behind a login proxy**: the page needs a router of its own without the login, rate
  limited (with Traefik, a router for `/R/` listed before the one that signs people in);
  everything else still needs it. A shared `secure-chain` middleware would set its own `Referrer-Policy`;
  the router's own `no-referrer` middleware, listed before it, keeps the page's (checked
  through the proxy).

`POST /api/sessions/{id}/share` (`{message?, new?, tz?}`: the roast's newest link, a message
given replaces its line; else a new one, 201, its page dated by `tz`, else by the offset the
`time_zone` setting has when it is made, else UTC), `GET /api/sessions/{id}/share` (the roast's
links, revoked too), `GET /api/share[?all=true]`, `DELETE /api/share/{code}`. Reads for every
token; making and revoking for the tokens that may print (`roast`, `inventory`, `admin`),
since a link makes a roast public. Neither reaches a roaster, so the real-hardware rule does
not hold them back (nor printing a roast's labels). Dashboard: History → a roast → **Share
page**; agents: `roast share` (`docs/cli.md`).

## How it is built

- `crates/server/src/labels/`: `sheets.rs` the sheets and offsets, `designs.rs` designs and
  fonts, `facts.rs` a roast's facts and the label data, `notes.rs` the tasting notes, `mod.rs`
  the job (`qr.svg` from the `qrcode` crate). `labels/engine.rs`: a Typst `World` (the driver,
  the design, `roastwarden.typ`, the request's files), PDF (`typst-pdf`), SVG (`typst-svg`),
  PNG (`typst-render`) and the check's report. `labels/sandbox.rs`: the worker process.
  `labels/store.rs`: saved designs, versions, assets, brands, a bean's label (migration
  030). `labels/print.rs`: IPP (the `ipp` crate's messages over the server's HTTP client).
  `routes/label_studio.rs`: the store's routes. `labels/share.rs` and `routes/share.rs`:
  share links and the public page (`/R/{code}`, routed in `app/build.rs`).
  Typst's memo cache is trimmed after each render (`comemo::evict`).
- `crates/server/src/routes/labels.rs`: `GET /api/labels` (choices),
  `GET /api/sessions/{id}/labels.pdf|.svg?sheet=&design=&params=&count=&start=&weight_g=&message=&notes=&hide=&qr=&tz=` (`params` a JSON object),
  `GET /api/labels/calibration.pdf?sheet=`, `POST /api/labels/check`,
  `POST /api/labels/render`. Every token preset may make labels.
- The dashboard page: `apps/dashboard/src/routes/history/[id]/labels/`, logic in
  `lib/labels/labels.ts`.
- To look at the designs while changing them:
  `RR_LABELS_OUT=/tmp/out cargo test -p roastwarden-server --lib labels::tests::every` writes
  each design on each sheet (PDF, and an SVG with a gift message and outlines).

### Typst, not printpdf (a spike)

One 5163 sheet with a QR code in each, at typst 0.15.1 and printpdf 0.12.8 (release builds):

| | Typst (library) | printpdf, hand layout |
|---|---|---|
| Output | shaped text (kerning, ligatures, script fonts join), wrapping and fitting built in | glyphs placed one by one: no kerning or ligatures without its HTML-layout feature; wrapping by hand |
| Designs | a `.typ` file, editable without a rebuild, draftable in typst.app | Rust code |
| Preview | the same layout as SVG (`typst-svg`) | a second renderer to write |
| Render | 25 ms first sheet, 4 ms after | 28 ms |
| Program size (spike alone, stripped) | 40 MB | 2.2 MB |
| Dependencies | ~371 crates (no feature flags to trim) | ~103 |
| Clean release build | 6 min 27 s | 1 min 23 s |

`cargo audit`: Typst's bibliography support brings quick-xml 0.38 (RUSTSEC-2026-0194/0195,
denial of service on hostile XML) through citationberg 0.7.0, the latest; ignored with the
reason in `.cargo/audit.toml` (labels never call `bibliography()`), to drop when citationberg
updates. Also unmaintained warnings for rustybuzz and ttf-parser (Typst's font stack).

Typst won because gift labels want display and script faces: a script font without shaping falls
apart, and designs as files beat designs as code. The costs: a bigger image (the stripped server
64 MB, about 45 MB of it Typst; release builds strip symbols, and `roast` embeds
`docs/openapi.json` rather than building it from the router, so the CLI does not link Typst: 11
MB) and a longer cold build, and Typst is 0.x (two API changes between 0.13 and 0.15 met in the
spike: `FileId::new` takes a `RootedPath`, `PagedDocument` moved to `typst-layout`).

## Label Studio: what it is for

Label creation and printing fully driven by an agent, designs saved and reused per coffee,
ASCII art, pictures, calligraphy, branding; for hobbyists who gift or sell small batches; an
agent's design loop that is at the leading edge and cheap in turns. The research behind it
covered agent visual-design loops, Typst for LLMs, sandboxing Typst, IPP printing, and what
small roasters put on bags.

Labels are a first-class part of Roastwarden, with a section of their own. Its parts, each
with its agent path, are above: the sandbox and the check (with the agents' design guide,
`skills/roastwarden/label-design.md`, its examples tested), saved designs with versions,
assets and brand kits (the folder designs are a read-only source), the Label Studio section
with its editor and live check, print runs and their history, printers (several, raster for
printers without PDF, `ipps` by a pinned fingerprint), the retail check and `check --diff`,
and the public share page as the QR code's target.

The same engine makes reports (`Kind::Report`, [`reports.md`](reports.md)): a report
design's `report(data)` lays out a whole document, with `report.typ`'s helpers.

## Later

- A PNG at label size for a thermal printer (Brother QL, DYMO) if one appears.

Thermal printers are direct thermal: prints fade in months and kraft stock is rare; for a few
bags a week, laser sheets are cheaper and last.
