# Cupping

Cupping a roast and keeping the score with it lets you see which roasts tasted best, and
compare them with how they were roasted. Roastwarden records the **classic SCA cupping form**
(the 2004 protocol, as the CQI Q form lays it out) against each roast, works out the final
score and its band, and prints blank forms to score on.

Cupping is one of the optional modules: switch it on under Settings → Modules (the Enthusiast
and Roastery presets include it).

## Scoring a roast

History → the roast → **Cupping Notes**:

- The seven quality attributes are sliders. Uniformity, Clean Cup and Sweetness are five cups
  each: untick a cup that fails.
- **Taint** and **Fault** count the cups with a defect.
- The line under them reads "Total − defects", then the **Final** score and its band.
- **Tasting Notes** keeps your words. **Save Scores** stores it; a roast has one cupping, and
  saving again replaces it.

## The form

| Attribute | Scale | A fresh form starts at |
|---|---|---|
| Fragrance/Aroma, Flavor, Aftertaste, Acidity, Body, Balance, Overall | 0–10 in quarter points; the form's scale runs 6.00–9.75 (6 good, 7 very good, 8 excellent, 9 outstanding) | 6.00 |
| Uniformity, Clean Cup, Sweetness | 2 points a cup over five cups: 0, 2, 4, 6, 8, 10 | 10 (every cup passes) |

- **Defects**: a cup with a *taint* (noticeable, mostly in the aroma) takes 2 points off, a
  cup with a *fault* (it spoils the taste) 4. At most five cups in all.
- **Total** is the ten attributes' sum. **Final** is the total less the defects, once all ten
  are scored.
- **Band**: 90 and over outstanding, 85 excellent, 80 very good, below 80 below specialty.

The SCA's 2024 Coffee Value Assessment (CVA) scores differently, and the SCA says the two do
not map onto each other. Roastwarden records only the classic form and never converts between
them.

## Cupping sheets

Reports → **Cupping sheet**: a blank form to score on, with a blind three-digit code per
sample and the key on a page of its own (two samples a page with scales, or six with boxes),
or a day's scores as a table ([reports](reports.md)). The official CVA forms may only be
shared unchanged, so Roastwarden links them rather than redrawing them:
<https://sca.coffee/s/CVA-Cupping-Forms-EN.pdf>.

## From the command line

```bash
roast cup '#12' aroma=7.5 flavour=7.25 acidity=7.75 sweetness=8 --taint 1 --notes "stone fruit"
roast report cupping latest --blank
```

Names may be the form's own or `aroma`, `flavour`, `clean`; scores you do not name keep theirs.
A score the form does not allow (sweetness 7, acidity 7.3) is refused with the reason
([the roast command](cli.md)).

<!-- site: end -->

## Design notes

For developers: how cupping works and why. The site publishes only the part above.

A roast is scored on the **classic SCA cupping form** (the 2004 protocol, as the CQI Q form
lays it out).

## The form

| Attribute | Scale | Usual start |
|---|---|---|
| Fragrance/Aroma, Flavor, Aftertaste, Acidity, Body, Balance, Overall | quality, 0–10 in quarter points; the form's scale runs 6.00–9.75 (6 good, 7 very good, 8 excellent, 9 outstanding) | 6.00 |
| Uniformity, Clean Cup, Sweetness | 2 points a cup over the sample's five cups: 0, 2, 4, 6, 8, 10 | 10 (every cup passes) |

- **Defects**: a cup with a *taint* (noticeable, mostly in the aroma) takes 2 points off, a
  cup with a *fault* (it spoils the taste) 4. At most five cups together.
- **Total** = the ten attributes' sum. **Final** = total − defects, once all ten are scored.
- **Band**: 90 and over outstanding, 85 excellent, 80 very good, below 80 below specialty.

The 2024 Coffee Value Assessment (SCA 103 and 104) scores differently (eight 9-point scales,
non-uniform and defective cups, a formula that gives 58–100) and SCA says the two do not
correlate linearly. Roastwarden records only the classic form and **never converts** between
them; CVA scores, if ever wanted, would be stored apart.

Before 0.4.0 the editor and `roast cup` started uniformity, clean cup and sweetness at
6 (so a fresh form lost 12 points) and the server stored the *mean* of the attributes as
the overall score, with no defects (migration 034 recomputed stored scores as totals; none
had defects).

## Recording one

- **Dashboard**: History → a roast → Cupping Notes. The seven quality attributes are
  sliders; uniformity, clean cup and sweetness are five cups each (untick a cup that fails);
  taint and fault are counts of cups. The line under them reads "Total − defects", then the
  final score and its band.
- **Agents**: `roast cup <roast> NAME=SCORE … [--taint N] [--fault N] [--notes …]`
  (`docs/cli.md`). Names may be the keys or `aroma`, `flavour`, `clean`; scores not named
  keep theirs (on a first cupping 6.00, and 10 for the three cup attributes).
- **API**: `POST /api/sessions/{id}/cupping` with `attributes` (`name`, `score`),
  `taint_cups`, `fault_cups`, `notes` replaces the roast's cupping. A score the form does not
  allow (sweetness 7, acidity 7.3, a name it does not have, six defect cups) is a 400 with
  the reason; an unknown roast 404. The answer and `GET` carry `final_score`,
  `total_score`, `defects`, `band` (null until all ten are scored) and the attributes in
  the form's order.

## Cupping sheets

Reports → **Cupping sheet** ([`reports.md`](reports.md)): a blank form to score on, after this
form, with a blind three-digit code per sample and the key on a page of its own (two samples
a page with scales, or six with boxes), and a day's scores as a table with a dot strip per
attribute. Agents: `roast report cupping latest --blank`, `roast report cupping`. The official
CVA forms may only be shared unchanged, so Roastwarden links them rather than redrawing them:
<https://sca.coffee/s/CVA-Cupping-Forms-EN.pdf>.
