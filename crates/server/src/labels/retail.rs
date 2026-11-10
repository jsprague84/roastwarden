//! The retail check (docs/labels.md, "The retail check"): what a US
//! label on whole-bean coffee that is sold must carry, read from the check's layout report.
//! 21 CFR 101.3 (the statement of identity), 101.7 (the net quantity; it was 101.105 until
//! 2016) and 101.5 (the name and place of business), with the type heights of 101.7(i) that
//! follow the area of the **bag's front** (the principal display panel), not the sticker's.
//! A reading of the rules, not legal advice; every result lists what a check cannot know.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use super::engine::{Issue, Report, TextLine};

/// A bag the labels go on: its front's size sets the minimum type height
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct Bag {
    pub id: String,
    pub name: String,
    /// The front panel's width and height, inches (a gusseted bag's face as it lies flat)
    pub front_w: f64,
    pub front_h: f64,
}

impl Bag {
    pub fn area(&self) -> f64 {
        self.front_w * self.front_h
    }
}

/// Common bags (sizes from suppliers' listings; the owner's own go in `label_bags`)
pub fn builtin_bags() -> Vec<Bag> {
    let b = |id: &str, name: &str, w: f64, h: f64| Bag {
        id: id.into(),
        name: name.into(),
        front_w: w,
        front_h: h,
    };
    vec![
        b("sample-2oz", "2 oz sample bag", 3.5, 5.5),
        b("pouch-8oz", "8 oz stand-up pouch", 5.125, 8.125),
        b("pouch-12oz", "12 oz valve pouch", 6.75, 10.5),
        b("pouch-16oz", "1 lb valve pouch", 7.5, 11.5),
        b("tin-tie-8oz", "8 oz tin-tie bag", 4.25, 9.25),
    ]
}

impl Bag {
    /// What is wrong with it, if anything
    pub fn check(&self) -> Result<(), String> {
        if !super::valid_id(&self.id) {
            return Err(format!(
                "a bag's id is letters, digits, - and _: {:?}",
                self.id
            ));
        }
        if self.name.trim().is_empty() {
            return Err("a bag has a name".into());
        }
        if !(self.front_w > 0.5 && self.front_h > 0.5 && self.front_w < 40.0 && self.front_h < 40.0)
        {
            return Err(format!(
                "{}: the front is inches across and down, 0.5 to 40",
                self.id
            ));
        }
        Ok(())
    }
}

/// The bags: the built-in ones and the owner's (`label_bags`, a JSON list; one with a
/// built-in id replaces it)
pub async fn bags(db: &sqlx::SqlitePool) -> Vec<Bag> {
    let mut all = builtin_bags();
    if let Some(own) = crate::auth::settings::get(db, "label_bags").await
        && let Ok(list) = serde_json::from_str::<Vec<Bag>>(&own) {
            for b in list.into_iter().filter(|b| b.check().is_ok()) {
                all.retain(|x| x.id != b.id);
                all.push(b);
            }
        }
    all
}

/// Check the `label_bags` setting's value (settings.rs)
pub fn check_bags(value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Ok(());
    }
    let list: Vec<Bag> = serde_json::from_str(value)
        .map_err(|e| format!("label_bags is a JSON list of bags: {e}"))?;
    list.iter().try_for_each(Bag::check)
}

/// The bag a retail check is for: `asked` (a bag's id, or `label` for a label that is the
/// whole front), else the `label_bag` setting, else the 12 oz pouch
pub async fn bag_for(
    db: &sqlx::SqlitePool,
    asked: &str,
    label_w_in: f64,
    label_h_in: f64,
) -> Result<Bag, String> {
    let asked = asked.trim();
    let id = if asked.is_empty() || asked == "default" || asked == "true" {
        crate::auth::settings::get(db, "label_bag")
            .await
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "pouch-12oz".into())
    } else {
        asked.to_string()
    };
    if id == "label" {
        return Ok(Bag {
            id: "label".into(),
            name: "the label as the whole front".into(),
            front_w: label_w_in,
            front_h: label_h_in,
        });
    }
    let all = bags(db).await;
    all.iter().find(|b| b.id == id).cloned().ok_or_else(|| {
        format!(
            "no bag {id:?}: {} or label (GET /api/labels lists them)",
            all.iter()
                .map(|b| b.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

/// 101.7(i): the least height of the net quantity's letters for a panel of `area` square
/// inches, inches
pub fn min_letter_in(area: f64) -> f64 {
    if area <= 5.0 {
        1.0 / 16.0
    } else if area <= 25.0 {
        1.0 / 8.0
    } else if area <= 100.0 {
        3.0 / 16.0
    } else if area <= 400.0 {
        1.0 / 4.0
    } else {
        1.0 / 2.0
    }
}

/// What the retail check found
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct Retail {
    /// Nothing to fix
    pub ok: bool,
    /// The bag it was checked for, with its front's area
    pub bag: String,
    /// The net quantity's least letter height for that bag, mm
    pub min_net_mm: f64,
    /// What to fix (also in the report's issues, as `retail_*`)
    pub issues: Vec<Issue>,
    /// What it found right
    pub passed: Vec<String>,
    /// What no check of a design can know
    pub cannot_check: Vec<String>,
}

const MM_PER_IN: f64 = 25.4;

fn issue(kind: &str, message: String) -> Issue {
    Issue {
        kind: format!("retail_{kind}"),
        message,
    }
}

/// A line's letter height as the rules measure it: the capitals when it is all capitals,
/// else the lowercase "o" (the x-height), mm
fn letter_mm(l: &TextLine) -> f64 {
    let letters: String = l.text.chars().filter(|c| c.is_alphabetic()).collect();
    let caps = !letters.is_empty() && letters.chars().all(|c| !c.is_lowercase());
    if caps {
        l.cap_mm
    } else {
        l.x_mm
    }
}

/// The size (pt) at which this line's letters reach `mm`
fn size_for(l: &TextLine, mm: f64) -> f64 {
    let now = letter_mm(l);
    if now <= 0.0 {
        return 0.0;
    }
    (l.pt * mm / now * 2.0).ceil() / 2.0
}

fn numbers(text: &str, unit: &str) -> Vec<f64> {
    let t = text.to_lowercase();
    let mut out = Vec::new();
    let mut i = 0;
    let b = t.as_bytes();
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let s = i;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.' || b[i] == b',') {
                i += 1;
            }
            let num: String = t[s..i].replace(',', ".");
            let rest = t[i..].trim_start();
            let unit_follows = rest.starts_with(unit)
                && rest[unit.len()..]
                    .chars()
                    .next()
                    .is_none_or(|c| !c.is_alphabetic());
            if unit_follows
                && let Ok(v) = num.trim_end_matches('.').parse::<f64>() {
                    out.push(v);
                }
        } else {
            i += 1;
        }
    }
    out
}

/// Read a label's report against the rules for a bag of this front
pub fn check(report: &Report, bag: &Bag, brand: &Value) -> Retail {
    let area = bag.area();
    let min_net_mm = (min_letter_in(area) * MM_PER_IN * 10.0).round() / 10.0;
    let floor_mm = (MM_PER_IN / 16.0 * 10.0).round() / 10.0; // 1.6 mm: everything required
    let mut issues = Vec::new();
    let mut passed = Vec::new();
    let lines = &report.lines;
    let largest_cap = lines.iter().map(|l| l.cap_mm).fold(0.0, f64::max);

    // 101.3: the statement of identity, bold, at least half the largest print
    match lines.iter().find(|l| l.text.to_lowercase().contains("coffee")) {
        None => issues.push(issue(
            "identity",
            "no statement of identity: say what is in the bag, e.g. \"Whole bean coffee\" (21 CFR 101.3)".into(),
        )),
        Some(l) => {
            let mut fine = true;
            if !l.bold {
                fine = false;
                issues.push(issue(
                    "identity",
                    format!("\"{}\" is not bold: the statement of identity is in bold type (101.3(d))", l.text),
                ));
            }
            if l.cap_mm + 0.05 < largest_cap / 2.0 {
                fine = false;
                issues.push(issue(
                    "identity",
                    format!(
                        "\"{}\" is {:.1} mm tall (capitals) beside a largest print of {:.1} mm: make it at least half that, {:.1} mm, about {} pt",
                        l.text,
                        l.cap_mm,
                        largest_cap,
                        largest_cap / 2.0,
                        (l.pt * (largest_cap / 2.0) / l.cap_mm.max(0.01) * 2.0).ceil() / 2.0
                    ),
                ));
            }
            if fine {
                passed.push(format!("statement of identity: \"{}\"", l.text));
            }
        }
    }

    // 101.7: the net quantity, both units, never rounded up, in the bottom 30 %, tall enough
    let net = lines.iter().find(|l| {
        let t = l.text.to_lowercase();
        t.contains("net wt") || t.contains("net weight") || t.contains("net wt.")
    });
    match net {
        None => issues.push(issue(
            "net",
            "no net quantity: \"Net wt 11.9 oz (340 g)\" in the label's bottom 30 % (21 CFR 101.7)"
                .into(),
        )),
        Some(l) => {
            let mut fine = true;
            let oz = numbers(&l.text, "oz");
            let g = numbers(&l.text, "g");
            let kg = numbers(&l.text, "kg");
            let lb = numbers(&l.text, "lb");
            let grams = g.first().copied().or(kg.first().map(|k| k * 1000.0));
            let ounces = oz.first().copied().or(lb.first().map(|p| p * 16.0));
            if ounces.is_none() || grams.is_none() {
                fine = false;
                issues.push(issue(
                    "net",
                    format!("\"{}\": give the weight in ounces (or pounds) and in grams: the Fair Packaging and Labeling Act asks for both", l.text),
                ));
            }
            if let (Some(o), Some(g)) = (ounces, grams)
                && o > g / 28.349_523_125 + 0.005 {
                    fine = false;
                    issues.push(issue(
                        "net",
                        format!("\"{}\": {o} oz is more than {g} g holds ({:.2} oz): a label may not round up", l.text, g / 28.349_523_125),
                    ));
                }
            let bottom = if report.round { 0.66 } else { 0.70 } * report.h;
            if l.y + 0.05 < bottom {
                fine = false;
                issues.push(issue(
                    "net",
                    format!(
                        "\"{}\" starts {:.1} mm from the top: put it in the bottom {} of the label (below {:.1} mm)",
                        l.text,
                        l.y,
                        if report.round { "34 % of its height" } else { "30 %" },
                        bottom
                    ),
                ));
            }
            let h = letter_mm(l);
            if h + 0.05 < min_net_mm {
                fine = false;
                issues.push(issue(
                    "net_size",
                    format!(
                        "\"{}\": its letters are {h:.1} mm, a {} ({area:.1} sq in front) needs {min_net_mm:.1} mm: about {} pt in {}{}",
                        l.text,
                        bag.name,
                        size_for(l, min_net_mm),
                        l.font,
                        if l.cap_mm > 0.0 && letter_mm(l) == l.x_mm {
                            format!(" (in capitals about {} pt)", (l.pt * min_net_mm / l.cap_mm * 2.0).ceil() / 2.0)
                        } else {
                            String::new()
                        }
                    ),
                ));
            }
            if !l.bold {
                fine = false;
                issues.push(issue(
                    "net",
                    format!(
                        "\"{}\" is not bold: the net quantity is in bold type",
                        l.text
                    ),
                ));
            }
            // Blank space: above and below at least the letter height
            let gap_above = lines
                .iter()
                .filter(|o| {
                    !std::ptr::eq(*o, l)
                        && o.y + o.h <= l.y + 0.01
                        && o.x < l.x + l.w
                        && l.x < o.x + o.w
                })
                .map(|o| l.y - (o.y + o.h))
                .fold(f64::INFINITY, f64::min);
            if gap_above < h * 0.95 {
                fine = false;
                issues.push(issue(
                    "net_space",
                    format!("\"{}\" has {gap_above:.1} mm of space above it: leave at least its letter height, {h:.1} mm", l.text),
                ));
            }
            if fine {
                passed.push(format!(
                    "net quantity: \"{}\" ({h:.1} mm letters, bottom 30 %)",
                    l.text
                ));
            }
        }
    }

    // 101.5: the name and place of business
    let name = brand["name"].as_str().filter(|n| !n.trim().is_empty());
    let named = name.and_then(|n| {
        lines
            .iter()
            .find(|l| l.text.to_lowercase().contains(&n.to_lowercase()))
    });
    match (name, named) {
        (None, _) => issues.push(issue(
            "packer",
            "no brand kit: the label must name who packed or sold it, with city, state and ZIP (21 CFR 101.5); set the brand's name and place".into(),
        )),
        (Some(n), None) => issues.push(issue(
            "packer",
            format!("\"{n}\" is not on the label: name who packed or sold it (101.5)"),
        )),
        (Some(_), Some(l)) => {
            if letter_mm(l) + 0.05 < floor_mm {
                issues.push(issue(
                    "packer",
                    format!("\"{}\": its letters are {:.1} mm; everything required is at least 1/16 in, {floor_mm} mm (about {} pt)", l.text, letter_mm(l), size_for(l, floor_mm)),
                ));
            } else {
                passed.push(format!("name: \"{}\"", l.text));
            }
        }
    }
    let state_zip = lines.iter().find(|l| {
        let t = &l.text;
        let has_state = t
            .split(|c: char| !c.is_ascii_alphabetic())
            .any(|w| w.len() == 2 && w.chars().all(|c| c.is_ascii_uppercase()));
        has_state && t.contains(',')
    });
    match state_zip {
        None => issues.push(issue(
            "packer",
            "no city and state (\"Portland, OR 97201\"): set the brand's place (101.5)".into(),
        )),
        Some(l) => {
            let zip = l
                .text
                .split(|c: char| !c.is_ascii_digit())
                .any(|w| w.len() == 5);
            if zip {
                passed.push(format!("place: \"{}\"", l.text));
            } else {
                issues.push(issue(
                    "packer",
                    format!("\"{}\": add the ZIP code, and the street unless your business is listed in a city or telephone directory (101.5(d))", l.text),
                ));
            }
        }
    }

    // Words that are nutrition claims lose coffee's exemption from Nutrition Facts
    let claims = [
        "calorie",
        "sugar free",
        "sugar-free",
        "fat free",
        "fat-free",
        "low fat",
        "healthy",
        "antioxidant",
        "protein",
    ];
    for l in lines {
        let t = l.text.to_lowercase();
        if let Some(c) = claims.iter().find(|c| t.contains(*c)) {
            issues.push(issue(
                "claim",
                format!("\"{}\" says \"{c}\": a nutrition claim loses coffee's exemption from Nutrition Facts (101.9(j)(4))", l.text),
            ));
        }
    }

    Retail {
        ok: issues.is_empty(),
        bag: format!(
            "{} ({:.2} × {:.2} in front, {area:.1} sq in)",
            bag.name, bag.front_w, bag.front_h
        ),
        min_net_mm,
        issues,
        passed,
        cannot_check: vec![
            "that the label goes on the bag's front, the side most likely shown".into(),
            "the bag's real front size (pick its bag type)".into(),
            "contrast on the real label stock".into(),
            "that each bag holds the weight it states".into(),
            "whether a blend or a flavoured coffee needs more words or an ingredient list".into(),
            "a second front (a bag shown either way needs both)".into(),
            "your state's rules (cottage food, permits, weights and measures)".into(),
            "claims such as organic or fair trade, which need their own certification".into(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn line(text: &str, y: f64, pt: f64, bold: bool, caps: bool) -> TextLine {
        // Atkinson Hyperlegible Next: x-height 0.496 em, cap height 0.668 em
        let mm = |em: f64| (em * pt * 25.4 / 72.0 * 10.0).round() / 10.0;
        TextLine {
            text: text.into(),
            font: "Atkinson Hyperlegible Next".into(),
            pt,
            x: 4.0,
            y,
            w: 40.0,
            h: mm(if caps { 0.668 } else { 0.9 }),
            bold,
            x_mm: mm(0.496),
            cap_mm: mm(0.668),
        }
    }

    fn report(lines: Vec<TextLine>, h: f64) -> Report {
        serde_json::from_value(json!({
            "ok": true, "w": 101.6, "h": h, "issues": [], "lines": lines,
            "fields": { "shown": [], "missing": [] }
        }))
        .unwrap()
    }

    #[test]
    fn the_type_height_follows_the_bag_front_not_the_sticker() {
        assert_eq!(min_letter_in(4.0), 1.0 / 16.0);
        assert_eq!(min_letter_in(19.25), 1.0 / 8.0);
        assert_eq!(min_letter_in(70.9), 3.0 / 16.0);
        let pouch = builtin_bags()
            .into_iter()
            .find(|b| b.id == "pouch-12oz")
            .unwrap();
        assert!((pouch.area() - 70.875).abs() < 0.01);
    }

    #[test]
    fn a_label_for_sale_passes_or_says_what_is_missing() {
        let brand = json!({ "name": "Hill Street Roasting", "place": "Portland, OR 97201" });
        let pouch = builtin_bags()
            .into_iter()
            .find(|b| b.id == "pouch-12oz")
            .unwrap();
        // Everything right: identity bold, the net weight in capitals at 4.8 mm, in the bottom 30 %
        let good = report(
            vec![
                line("Costa Rica Palmichal", 8.0, 15.0, false, false),
                line("Whole bean coffee", 16.0, 11.0, true, false),
                line(
                    "Hill Street Roasting, Portland, OR 97201",
                    22.0,
                    10.0,
                    false,
                    false,
                ),
                line("NET WT 12 OZ (341 G)", 40.0, 21.0, true, true),
            ],
            50.8,
        );
        let r = check(&good, &pouch, &brand);
        assert!(r.ok, "{:?}", r.issues);
        assert_eq!(r.min_net_mm, 4.8);
        assert_ne!(r.cannot_check, [] as [std::string::String; 0]);

        // The shop design's old small net line: too small, too high, rounded up, no ZIP
        let bad = report(
            vec![
                line("Costa Rica Palmichal", 8.0, 15.0, false, false),
                line("Whole bean coffee", 28.0, 8.0, false, false),
                line("Net wt 12 oz (340 g)", 30.0, 10.0, true, false),
                line(
                    "Roasted and packed by Hill Street Roasting, Portland, OR",
                    44.0,
                    6.5,
                    false,
                    false,
                ),
            ],
            50.8,
        );
        let r = check(&bad, &pouch, &brand);
        assert!(!r.ok);
        let kinds: Vec<&str> = r.issues.iter().map(|i| i.kind.as_str()).collect();
        assert!(kinds.contains(&"retail_identity"), "not bold: {kinds:?}");
        assert!(kinds.contains(&"retail_net_size"), "{kinds:?}");
        assert!(
            kinds.contains(&"retail_net"),
            "not in the bottom 30 %: {kinds:?}"
        );
        assert!(kinds.contains(&"retail_packer"), "no ZIP: {kinds:?}");
        let size = r
            .issues
            .iter()
            .find(|i| i.kind == "retail_net_size")
            .unwrap();
        assert!(size.message.contains("needs 4.8 mm"), "{}", size.message);
        assert!(
            r.issues
                .iter()
                .any(|i| i.message.contains("rounds up") || i.message.contains("round up")),
            "12 oz is more than 340 g: {:?}",
            r.issues
        );
    }
}
