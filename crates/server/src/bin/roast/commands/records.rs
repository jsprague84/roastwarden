//! Commands that edit records: a roast's details, cupping, export and import; beans and their
//! ledger; orders of several coffees; the plan; profiles. Each is a thin call (or two) to the
//! API, with names where ids are awkward and units as people type them (docs/cli.md).

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Duration;

use clap::Args;
use reqwest::Method;
use serde_json::{json, Value};

use crate::api::{self, enc, Api};
use crate::cli::Wait;
use crate::commands::approvals::held_or_done;
use crate::commands::beans::{DetailArgs, PurchaseArgs};
use crate::commands::roasts::roast_id;
use crate::out::{self, code, Failure, Out};
use crate::text::{bean_line, mmss, parse_clock, parse_weight, BeanRow};

// ---- a roast's record ----

#[derive(Args, Debug)]
pub struct EditArgs {
    /// The roast's id, or `latest` (--roaster narrows it)
    pub roast: String,
    /// With `latest`: the newest roast on this roaster
    #[arg(long)]
    pub roaster: Option<String>,
    #[arg(long)]
    pub name: Option<String>,
    /// Replace the notes (`--add-note` adds a line instead)
    #[arg(long, conflicts_with = "add_note")]
    pub notes: Option<String>,
    /// Add a line to the notes
    #[arg(long)]
    pub add_note: Option<String>,
    /// 1 to 5
    #[arg(long, value_parser = clap::value_parser!(i32).range(1..=5))]
    pub rating: Option<i32>,
    /// The roast level aimed at: extremely-light, very-light, light, medium-light, medium,
    /// medium-dark, dark
    #[arg(long)]
    pub level: Option<String>,
    /// espresso, filter, omni or other
    #[arg(long)]
    pub purpose: Option<String>,
    /// The green bean (id or name)
    #[arg(long)]
    pub bean: Option<String>,
    /// Green weight, e.g. 100g
    #[arg(long, value_parser = parse_weight)]
    pub green: Option<f64>,
    /// Roasted weight (`weigh` does the same)
    #[arg(long, value_parser = parse_weight)]
    pub roasted: Option<f64>,
    /// Colour of the whole beans, on --color-scale
    #[arg(long)]
    pub color_whole: Option<f32>,
    /// Colour of the ground coffee, on --color-scale
    #[arg(long)]
    pub color_ground: Option<f32>,
    /// agtron, tonino, lebrew or other
    #[arg(long)]
    pub color_scale: Option<String>,
    /// Green moisture, %
    #[arg(long)]
    pub moisture_green: Option<f32>,
    /// Roasted moisture, %
    #[arg(long)]
    pub moisture_roasted: Option<f32>,
    /// Green density, g/l
    #[arg(long)]
    pub density_green: Option<f32>,
    /// Room temperature, °C
    #[arg(long)]
    pub ambient: Option<f32>,
    /// Relative humidity, %
    #[arg(long)]
    pub humidity: Option<f32>,
    /// Comma-separated flags, e.g. "scorched,uneven"
    #[arg(long)]
    pub flags: Option<String>,
}

/// `medium-light`, `Medium light` → `medium_light`.
fn level(s: &str) -> String {
    s.trim().to_ascii_lowercase().replace(['-', ' '], "_")
}

pub async fn edit(api: &Api, out: &Out, a: EditArgs) -> Result<u8, Failure> {
    let id = roast_id(api, &a.roast, a.roaster.as_deref()).await?;
    let path = format!("/api/sessions/{}", enc(&id));
    let notes = match a.add_note {
        Some(line) => {
            let now = api.get(&path).await?;
            let old = now["notes"].as_str().unwrap_or("").trim_end().to_string();
            Some(if old.is_empty() {
                line
            } else {
                format!("{old}\n{line}")
            })
        }
        None => a.notes,
    };
    let bean_id = match &a.bean {
        Some(b) => Some(api.bean(b).await?["id"].clone()),
        None => None,
    };
    let mut body = json!({
        "name": a.name, "notes": notes, "rating": a.rating,
        "target_roast_level": a.level.as_deref().map(level),
        "roast_purpose": a.purpose.as_deref().map(level),
        "bean_id": bean_id, "green_weight": a.green, "roasted_weight": a.roasted,
        "color_whole": a.color_whole, "color_ground": a.color_ground,
        "color_scale": a.color_scale, "moisture_green": a.moisture_green,
        "moisture_roasted": a.moisture_roasted, "density_green": a.density_green,
        "ambient_temp": a.ambient, "humidity": a.humidity, "roast_flags": a.flags,
    });
    out::slim(&mut body);
    if body.as_object().is_some_and(serde_json::Map::is_empty) {
        return Err(Failure::new(
            "USAGE",
            "nothing to change: give a field, e.g. --notes or --rating",
        ));
    }
    let changed: Vec<String> = body
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    let one = api.call(Method::PUT, &path, Some(&body)).await?;
    let mut short = serde_json::Map::new();
    for k in ["id", "name", "batch_number"]
        .iter()
        .map(std::string::ToString::to_string)
        .chain(changed)
        .chain(["weight_loss_pct".to_string()])
    {
        if let Some(v) = one.get(&k) {
            short.insert(k, v.clone());
        }
    }
    out.emit(&Value::Object(short), |r| {
        let fields: Vec<String> = r
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(k, _)| !matches!(k.as_str(), "id" | "name" | "batch_number"))
            .map(|(k, v)| format!("{k} {}", v.as_str().map_or(v.to_string(), str::to_string)))
            .collect();
        format!(
            "{}: {}",
            r["name"].as_str().unwrap_or("?"),
            fields.join(", ")
        )
    });
    Ok(code::OK)
}

/// The classic SCA form's attributes (`docs/cupping.md`), as the server stores them: the key,
/// other names, and whether it scores cups (2 a cup over five) rather than quality.
const SCA: [(&str, &[&str], bool); 10] = [
    (
        "fragrance_aroma",
        &["aroma", "fragrance", "fragrance_aroma"],
        false,
    ),
    ("flavor", &["flavor", "flavour"], false),
    ("aftertaste", &["aftertaste"], false),
    ("acidity", &["acidity"], false),
    ("body", &["body"], false),
    ("balance", &["balance"], false),
    ("uniformity", &["uniformity"], true),
    ("clean_cup", &["clean_cup", "clean", "cleancup"], true),
    ("sweetness", &["sweetness"], true),
    ("overall", &["overall"], false),
];

/// `aroma=7.5` → (`fragrance_aroma`, 7.5)
fn attribute(s: &str) -> Result<(&'static str, f64), String> {
    let names = || SCA.iter().map(|(k, ..)| *k).collect::<Vec<_>>().join(", ");
    let (name, score) = s
        .split_once(['=', ':'])
        .ok_or_else(|| format!("'{s}': use NAME=SCORE, e.g. acidity=7.5"))?;
    let key = name
        .trim()
        .to_ascii_lowercase()
        .replace(['-', ' ', '/'], "_");
    let (attr, cups) = SCA
        .iter()
        .find(|(_, aliases, _)| aliases.contains(&key.as_str()))
        .map(|(k, _, cups)| (*k, *cups))
        .ok_or_else(|| format!("unknown attribute '{name}': {}", names()))?;
    let score: f64 = score
        .trim()
        .parse()
        .map_err(|_| format!("'{score}' is not a score (0 to 10)"))?;
    if cups {
        if !(0.0..=10.0).contains(&score) || (score / 2.0).fract() != 0.0 {
            return Err(format!(
                "{attr} {score}: 2 points a cup over five cups, so 0, 2, 4, 6, 8 or 10 (10 when every cup passes)"
            ));
        }
    } else if !(0.0..=10.0).contains(&score) || (score * 4.0).fract() != 0.0 {
        return Err(format!(
            "{attr} {score}: 0 to 10 in quarter points (the form's scale is 6–9.75)"
        ));
    }
    Ok((attr, score))
}

#[derive(Args, Debug)]
pub struct CupArgs {
    /// The roast's id, or `latest` (--roaster narrows it)
    pub roast: String,
    /// Scores to set, NAME=SCORE, on the classic SCA form: fragrance_aroma (aroma), flavor,
    /// aftertaste, acidity, body, balance and overall 0–10 in quarter points (the form's scale
    /// is 6–9.75); uniformity, clean_cup and sweetness 2 a cup over five cups (0, 2, … 10).
    /// Unnamed ones keep their earlier score (on a first cupping 6.0, and 10 for the three
    /// cup attributes)
    #[arg(value_parser = attribute)]
    pub scores: Vec<(&'static str, f64)>,
    /// Cups (of five) with a taint: 2 points each off the total
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=5))]
    pub taint: Option<u8>,
    /// Cups (of five) with a fault: 4 points each off the total
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=5))]
    pub fault: Option<u8>,
    /// The cupping notes (tasting notes)
    #[arg(long)]
    pub notes: Option<String>,
    /// Remove the roast's cupping
    #[arg(long, conflicts_with_all = ["scores", "notes", "taint", "fault"])]
    pub rm: bool,
    #[arg(long)]
    pub roaster: Option<String>,
}

pub async fn cup(api: &Api, out: &Out, a: CupArgs) -> Result<u8, Failure> {
    let id = roast_id(api, &a.roast, a.roaster.as_deref()).await?;
    let path = format!("/api/sessions/{}/cupping", enc(&id));
    if a.rm {
        api.send(Method::DELETE, &path, None, Duration::from_secs(30))
            .await?;
        out.emit(&json!({ "removed": id }), |_| "cupping removed".into());
        return Ok(code::OK);
    }
    let now = api.get(&path).await?;
    if a.scores.is_empty() && a.notes.is_none() && a.taint.is_none() && a.fault.is_none() {
        out.emit(&now, cupping_text);
        return Ok(code::OK);
    }
    // The form scores all ten; keep what is there, the form's start for the rest
    let mut scores: Vec<(String, f64)> = SCA
        .iter()
        .map(|(k, _, cups)| {
            let old = now["attributes"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|x| x["attribute_name"] == *k)
                .and_then(|x| x["score"].as_f64());
            (k.to_string(), old.unwrap_or(if *cups { 10.0 } else { 6.0 }))
        })
        .collect();
    for (k, v) in &a.scores {
        if let Some(s) = scores.iter_mut().find(|(n, _)| n == k) {
            s.1 = *v;
        }
    }
    let kept = |field: &str| now[field].as_u64().unwrap_or(0);
    let body = json!({
        "scoring_framework": "sca",
        "notes": a.notes.or_else(|| now["notes"].as_str().map(str::to_string)),
        "attributes": scores.iter().map(|(n, s)| json!({ "name": n, "score": s })).collect::<Vec<_>>(),
        "taint_cups": a.taint.map_or(kept("taint_cups"), u64::from),
        "fault_cups": a.fault.map_or(kept("fault_cups"), u64::from),
    });
    let saved = api.call(Method::POST, &path, Some(&body)).await?;
    out.emit(&saved, cupping_text);
    Ok(code::OK)
}

fn cupping_text(c: &Value) -> String {
    if c.is_null() {
        return "Not cupped yet".into();
    }
    let n = |v: &Value| v.as_f64().map_or("-".into(), |s| format!("{s:.2}"));
    let mut lines = vec![format!(
        "final {} ({}): total {} − defects {}",
        n(&c["final_score"]),
        c["band"].as_str().unwrap_or("not all ten scored"),
        n(&c["total_score"]),
        c["defects"].as_i64().unwrap_or(0),
    )];
    for x in c["attributes"].as_array().into_iter().flatten() {
        lines.push(format!(
            "  {:<16} {}",
            x["attribute_name"].as_str().unwrap_or("?"),
            n(&x["score"])
        ));
    }
    for (what, field, each) in [("taint", "taint_cups", 2), ("fault", "fault_cups", 4)] {
        if let Some(k) = c[field].as_i64().filter(|k| *k > 0) {
            lines.push(format!("  {what}: {k} cup(s) × {each}"));
        }
    }
    if let Some(n) = c["notes"].as_str() {
        lines.push(format!("  notes: {n}"));
    }
    lines.join("\n")
}

/// Write what the server sent to `output` (`-` for stdout), else to the server's file name
/// in the current folder; the answer says where.
pub(crate) async fn save_file(
    api: &Api,
    out: &Out,
    path: &str,
    output: Option<PathBuf>,
    fallback: &str,
) -> Result<u8, Failure> {
    let (bytes, name) = api.download(path).await?;
    if output.as_deref() == Some(std::path::Path::new("-")) {
        use std::io::Write;
        std::io::stdout()
            .write_all(&bytes)
            .map_err(|e| Failure::new("FAILED", e.to_string()))?;
        return Ok(code::OK);
    }
    let file =
        output.unwrap_or_else(|| PathBuf::from(safe_name(name.as_deref().unwrap_or(fallback))));
    std::fs::write(&file, &bytes)
        .map_err(|e| Failure::new("FAILED", format!("could not write {}: {e}", file.display())))?;
    let answer = json!({ "file": file.display().to_string(), "bytes": bytes.len() });
    out.emit(&answer, |a| {
        format!(
            "wrote {} ({} bytes)",
            a["file"].as_str().unwrap_or("?"),
            a["bytes"]
        )
    });
    Ok(code::OK)
}

/// A file name from the server, without folders.
fn safe_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let clean: String = base
        .chars()
        .map(|c| if c.is_control() { '_' } else { c })
        .collect();
    if clean.trim().is_empty() || clean.starts_with('.') {
        format!("roast{clean}")
    } else {
        clean
    }
}

#[derive(Args, Debug)]
pub struct ExportArgs {
    /// The roast's id, or `latest` (--roaster narrows it)
    pub roast: String,
    /// CSV, one row a sample, instead of the Artisan .alog
    #[arg(long)]
    pub csv: bool,
    /// The file (default: the server's name for it, here); `-` for stdout
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    #[arg(long)]
    pub roaster: Option<String>,
}

pub async fn export(api: &Api, out: &Out, a: ExportArgs) -> Result<u8, Failure> {
    let id = roast_id(api, &a.roast, a.roaster.as_deref()).await?;
    let (kind, ext) = if a.csv {
        ("csv", "csv")
    } else {
        ("artisan", "alog")
    };
    save_file(
        api,
        out,
        &format!("/api/sessions/{}/export/{kind}", enc(&id)),
        a.output,
        &format!("roast.{ext}"),
    )
    .await
}

#[derive(Args, Debug)]
pub struct ImportArgs {
    /// The Artisan .alog file
    pub file: PathBuf,
    /// The roaster it was roasted on (id or name)
    #[arg(long)]
    pub roaster: Option<String>,
    #[arg(long)]
    pub name: Option<String>,
}

fn read_file(path: &std::path::Path) -> Result<String, Failure> {
    std::fs::read_to_string(path).map_err(|e| Failure {
        status: None,
        details: None,
        ..Failure::new("NO_FILE", format!("{}: {e}", path.display()))
    })
}

pub async fn import(api: &Api, out: &Out, a: ImportArgs) -> Result<u8, Failure> {
    let content = read_file(&a.file)?;
    let device = match &a.roaster {
        Some(r) => Some(api.roaster(r).await?["device_id"].clone()),
        None => None,
    };
    let mut body = json!({ "alog_content": content, "device_id": device, "name": a.name });
    out::slim(&mut body);
    let s = api
        .call(Method::POST, "/api/sessions/import/artisan", Some(&body))
        .await?;
    out.emit(
        &json!({ "id": s["id"], "name": s["name"], "device_id": s["device_id"] }),
        |r| {
            format!(
                "imported {} ({})",
                r["name"].as_str().unwrap_or("?"),
                r["id"].as_str().unwrap_or("?")
            )
        },
    );
    Ok(code::OK)
}

pub async fn rm(api: &Api, out: &Out, roast: String, wait: Wait) -> Result<u8, Failure> {
    if roast.eq_ignore_ascii_case("latest") {
        return Err(Failure::new(
            "USAGE",
            "a delete takes the roast's id, not `latest` (`roast roasts` lists them)",
        ));
    }
    let one = api.get(&format!("/api/sessions/{}", enc(&roast))).await?;
    let name = one["name"].as_str().unwrap_or("?").to_string();
    let reply = api
        .send(
            Method::DELETE,
            &format!("/api/sessions/{}", enc(&roast)),
            None,
            Duration::from_secs(60),
        )
        .await?;
    held_or_done(api, out, reply, wait, move |_| format!("deleted {name}")).await
}

// ---- green coffee ----

#[derive(Args, Debug)]
pub struct BeanEditArgs {
    /// The bean's id or name
    pub bean: String,
    /// A new name
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub origin: Option<String>,
    #[arg(long)]
    pub variety: Option<String>,
    #[arg(long)]
    pub process: Option<String>,
    #[arg(long)]
    pub lot: Option<String>,
    #[arg(long)]
    pub supplier: Option<String>,
    /// The bean's notes (tasting notes go here too)
    #[arg(long)]
    pub notes: Option<String>,
    #[arg(long)]
    pub moisture: Option<f32>,
    #[arg(long)]
    pub density: Option<f32>,
    /// Note "low" below this, e.g. 500g
    #[arg(long, value_parser = parse_weight)]
    pub low_stock: Option<f64>,
    /// Hide it from the lists (it stays with its roasts)
    #[arg(long, conflicts_with = "unarchive")]
    pub archive: bool,
    #[arg(long)]
    pub unarchive: bool,
    /// Clear fields, comma-separated: e.g. --clear lot,notes
    #[arg(long, value_delimiter = ',')]
    pub clear: Vec<String>,
    #[command(flatten)]
    pub details: DetailArgs,
}

pub async fn bean_edit(api: &Api, out: &Out, a: BeanEditArgs) -> Result<u8, Failure> {
    let b = api.bean(&a.bean).await?;
    let id = b["id"].as_str().unwrap_or_default().to_string();
    let d = a.details;
    let archived = if a.archive {
        Some(true)
    } else if a.unarchive {
        Some(false)
    } else {
        None
    };
    let mut body = json!({
        "name": a.name, "origin": a.origin, "variety": a.variety, "process": a.process,
        "lot": a.lot, "supplier": a.supplier, "notes": a.notes, "moisture_pct": a.moisture,
        "density_g_l": a.density, "low_stock_g": a.low_stock, "archived": archived,
        "region": d.region, "producer": d.producer, "altitude": d.altitude, "grade": d.grade,
        "certifications": d.certifications, "score": d.score, "url": d.link,
        "label_notes": d.label_notes, "short_name": d.short_name,
    });
    out::slim(&mut body);
    const CLEARABLE: [&str; 16] = [
        "origin",
        "variety",
        "process",
        "lot",
        "supplier",
        "notes",
        "moisture",
        "density",
        "low_stock",
        "region",
        "producer",
        "altitude",
        "grade",
        "link",
        "label_notes",
        "short_name",
    ];
    for field in &a.clear {
        let f = field.trim().replace('-', "_");
        if !CLEARABLE.contains(&f.as_str()) {
            return Err(Failure::new(
                "USAGE",
                format!("--clear {field}: one of {}", CLEARABLE.join(", ")),
            ));
        }
        let key = match f.as_str() {
            "moisture" => "moisture_pct",
            "density" => "density_g_l",
            "low_stock" => "low_stock_g",
            "link" => "url",
            other => other,
        };
        body[key] = Value::Null;
    }
    if body.as_object().is_some_and(serde_json::Map::is_empty) {
        return Err(Failure::new(
            "USAGE",
            "nothing to change: give a field to set or --clear",
        ));
    }
    let bean = api
        .call(
            Method::PUT,
            &format!("/api/beans/{}", enc(&id)),
            Some(&body),
        )
        .await?;
    let row: BeanRow = api::decode(bean.clone())?;
    out.emit(&bean, |_| format!("edited: {}", bean_line(&row)));
    Ok(code::OK)
}

#[derive(Args, Debug)]
pub struct MoveEditArgs {
    /// The bean's id or name
    pub bean: String,
    /// The purchase or correction (`bean show` lists the ids)
    pub move_id: String,
    /// The weight it added (or, negative, took)
    #[arg(long, value_parser = parse_weight, allow_hyphen_values = true)]
    pub weight: Option<f64>,
    #[arg(long)]
    pub note: Option<String>,
    #[command(flatten)]
    pub purchase: PurchaseArgs,
}

pub async fn move_edit(api: &Api, out: &Out, a: MoveEditArgs) -> Result<u8, Failure> {
    let b = api.bean(&a.bean).await?;
    let id = b["id"].as_str().unwrap_or_default().to_string();
    let p = a.purchase;
    let mut body = json!({
        "delta_g": a.weight, "note": a.note, "price": p.price, "shipping": p.shipping,
        "fees": p.fees, "currency": p.currency, "supplier": p.supplier, "at": p.date,
        "crop_year": p.crop_year, "arrival": p.arrival,
    });
    out::slim(&mut body);
    if body.as_object().is_some_and(serde_json::Map::is_empty) {
        return Err(Failure::new("USAGE", "nothing to change: e.g. --price 46"));
    }
    let r = api
        .call(
            Method::PUT,
            &format!("/api/beans/{}/moves/{}", enc(&id), enc(&a.move_id)),
            Some(&body),
        )
        .await?;
    out.emit(&r, |m| format!("edited: {}", move_line(m)));
    Ok(code::OK)
}

/// A purchase or correction in one line.
fn move_line(m: &Value) -> String {
    format!(
        "{}  {}  {}{}{}{}",
        m["id"].as_str().unwrap_or("?"),
        m["at"].as_str().unwrap_or("?").get(..10).unwrap_or("?"),
        crate::text::grams(m["delta_g"].as_f64().unwrap_or(0.0)),
        m["price"]
            .as_f64()
            .map(|p| format!(" for {p:.2}"))
            .unwrap_or_default(),
        m["shipping"]
            .as_f64()
            .map(|p| format!(" + {p:.2} shipping"))
            .unwrap_or_default(),
        m["note"]
            .as_str()
            .map(|n| format!("  {n}"))
            .unwrap_or_default(),
    )
}

pub async fn move_rm(
    api: &Api,
    out: &Out,
    bean: String,
    move_id: String,
    wait: Wait,
) -> Result<u8, Failure> {
    let b = api.bean(&bean).await?;
    let id = b["id"].as_str().unwrap_or_default().to_string();
    let reply = api
        .send(
            Method::DELETE,
            &format!("/api/beans/{}/moves/{}", enc(&id), enc(&move_id)),
            None,
            Duration::from_secs(60),
        )
        .await?;
    held_or_done(api, out, reply, wait, move |_| format!("removed {move_id}")).await
}

#[derive(Args, Debug)]
pub struct OrderArgs {
    #[command(subcommand)]
    pub cmd: Option<crate::commands::orders::OrderCommand>,
    /// One coffee of the order, BEAN=WEIGHT[@PRICE]: e.g. "Kenya Nyeri AA=2lb@18.50" (the
    /// price for that weight); repeat for each coffee. Add a new bean first (`bean add`)
    #[arg(long = "line", required = true, value_name = "BEAN=WEIGHT[@PRICE]")]
    pub lines: Vec<String>,
    #[arg(long)]
    pub supplier: Option<String>,
    /// The order's shipping, split over the coffees by weight
    #[arg(long)]
    pub shipping: Option<f64>,
    /// The order's taxes and fees, split by weight
    #[arg(long)]
    pub fees: Option<f64>,
    /// ISO code (default: the server's currency)
    #[arg(long)]
    pub currency: Option<String>,
    /// YYYY-MM-DD (default today)
    #[arg(long)]
    pub date: Option<String>,
    #[arg(long)]
    pub note: Option<String>,
}

/// `Kenya Nyeri AA=2lb@18.50` → ("Kenya Nyeri AA", 907.2, Some(18.5))
fn order_line(s: &str) -> Result<(String, f64, Option<f64>), String> {
    let (bean, rest) = s
        .rsplit_once('=')
        .ok_or_else(|| format!("'{s}': use BEAN=WEIGHT[@PRICE], e.g. \"Kenya AA=2lb@18.50\""))?;
    let (weight, price) = match rest.split_once('@') {
        Some((w, p)) => (
            w,
            Some(
                p.trim()
                    .trim_start_matches('$')
                    .parse::<f64>()
                    .map_err(|_| format!("'{p}' is not a price"))?,
            ),
        ),
        None => (rest, None),
    };
    let g = parse_weight(weight)?;
    if g <= 0.0 || bean.trim().is_empty() {
        return Err(format!("'{s}': a bean and a weight above 0"));
    }
    Ok((bean.trim().to_string(), g, price))
}

pub async fn order(api: &Api, out: &Out, a: OrderArgs) -> Result<u8, Failure> {
    let mut lines = Vec::new();
    for l in &a.lines {
        let (bean, g, price) = order_line(l).map_err(|m| Failure::new("USAGE", m))?;
        let b = api.bean(&bean).await?;
        let mut line = json!({ "bean_id": b["id"], "weight_g": g, "price": price });
        out::slim(&mut line);
        lines.push(line);
    }
    let mut body = json!({
        "supplier": a.supplier, "shipping": a.shipping, "fees": a.fees,
        "currency": a.currency, "at": a.date, "note": a.note, "lines": lines,
    });
    out::slim(&mut body);
    let mut r = api
        .call(Method::POST, "/api/purchases", Some(&body))
        .await?;
    // The stock and cost per kg each bean has now
    let mut beans = Vec::new();
    for l in &lines {
        let id = l["bean_id"].as_str().unwrap_or_default();
        let b = api.get(&format!("/api/beans/{}", enc(id))).await?;
        beans.push(
            json!({ "id": b["id"], "name": b["name"], "stock_g": b["stock_g"],
            "cost_per_kg": b["cost_per_kg"], "currency": b["currency"] }),
        );
    }
    r["beans"] = json!(beans);
    out.emit(&r, |r| {
        let mut text = vec!["recorded:".to_string()];
        for m in r["moves"].as_array().into_iter().flatten() {
            text.push(format!("  {}", move_line(m)));
        }
        for b in r["beans"].as_array().into_iter().flatten() {
            let row: BeanRow = serde_json::from_value(b.clone()).unwrap_or_default();
            text.push(format!("  {}", bean_line(&row)));
        }
        text.join("\n")
    });
    Ok(code::OK)
}

// ---- the plan ----

#[derive(Args, Debug)]
pub struct PlanEditArgs {
    /// The planned roast's id (`roast plan` lists them)
    pub id: String,
    #[arg(long)]
    pub bean: Option<String>,
    #[arg(long, value_parser = parse_weight)]
    pub weight: Option<f64>,
    #[arg(long)]
    pub profile: Option<String>,
    /// The roaster (id or name); `any` for any roaster
    #[arg(long)]
    pub roaster: Option<String>,
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub notes: Option<String>,
}

pub async fn plan_edit(api: &Api, out: &Out, a: PlanEditArgs) -> Result<u8, Failure> {
    let mut body = json!({ "green_weight_g": a.weight, "name": a.name, "notes": a.notes });
    out::slim(&mut body);
    if let Some(b) = a.bean {
        body["bean_id"] = api.bean(&b).await?["id"].clone();
    }
    if let Some(p) = a.profile {
        body["profile_id"] = api.profile(&p).await?["id"].clone();
    }
    if let Some(r) = a.roaster {
        body["device_id"] = if r.eq_ignore_ascii_case("any") {
            Value::Null
        } else {
            api.roaster(&r).await?["device_id"].clone()
        };
    }
    if body.as_object().is_some_and(serde_json::Map::is_empty) {
        return Err(Failure::new(
            "USAGE",
            "nothing to change: e.g. --weight 120g",
        ));
    }
    let plan = api
        .call(
            Method::PUT,
            &format!("/api/plan/{}", enc(&a.id)),
            Some(&body),
        )
        .await?;
    let row: crate::text::PlanRow = api::decode(plan.clone())?;
    out.emit(&plan, |_| {
        format!("planned: {}", crate::text::plan_line(&row))
    });
    Ok(code::OK)
}

pub async fn plan_rm(api: &Api, out: &Out, id: String, wait: Wait) -> Result<u8, Failure> {
    let reply = api
        .send(
            Method::DELETE,
            &format!("/api/plan/{}", enc(&id)),
            None,
            Duration::from_secs(60),
        )
        .await?;
    held_or_done(api, out, reply, wait, move |_| {
        format!("removed {id} from the plan")
    })
    .await
}

pub async fn plan_move(api: &Api, out: &Out, id: String, to: usize) -> Result<u8, Failure> {
    let list = api.get("/api/plan").await?;
    let mut ids: Vec<String> = list
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p["id"].as_str().map(str::to_string))
        .collect();
    let Some(from) = ids.iter().position(|x| *x == id) else {
        return Err(Failure {
            status: Some(404),
            ..Failure::new(
                "NOT_FOUND",
                format!("{id} is not in the queue (`roast plan`)"),
            )
        });
    };
    let moved = ids.remove(from);
    ids.insert(to.clamp(1, ids.len() + 1) - 1, moved);
    let r = api
        .call(
            Method::POST,
            "/api/plan/reorder",
            Some(&json!({ "ids": ids })),
        )
        .await?;
    let rows: Vec<crate::text::PlanRow> = api::decode(r.clone())?;
    out.emit(&r, |_| {
        rows.iter()
            .enumerate()
            .map(|(i, p)| format!("{:>2}. {}", i + 1, crate::text::plan_line(p)))
            .collect::<Vec<_>>()
            .join("\n")
    });
    Ok(code::OK)
}

// ---- profiles ----

pub async fn profile_save(
    api: &Api,
    out: &Out,
    roast: String,
    name: Option<String>,
    description: Option<String>,
    roaster: Option<String>,
) -> Result<u8, Failure> {
    let id = roast_id(api, &roast, roaster.as_deref()).await?;
    let mut body = json!({ "name": name, "description": description });
    out::slim(&mut body);
    let p = api
        .call(
            Method::POST,
            &format!("/api/sessions/{}/profile", enc(&id)),
            Some(&body),
        )
        .await?;
    out.emit(&json!({ "id": p["id"], "name": p["name"] }), |p| {
        format!(
            "saved as profile {} ({})",
            p["name"].as_str().unwrap_or("?"),
            p["id"].as_str().unwrap_or("?")
        )
    });
    Ok(code::OK)
}

pub async fn profile_import(
    api: &Api,
    out: &Out,
    file: PathBuf,
    name: Option<String>,
) -> Result<u8, Failure> {
    let content = read_file(&file)?;
    let mut body = json!({ "alog_content": content, "name": name });
    out::slim(&mut body);
    let p = api
        .call(Method::POST, "/api/profiles/import/artisan", Some(&body))
        .await?;
    out.emit(&json!({ "id": p["id"], "name": p["name"] }), |p| {
        format!(
            "imported profile {} ({})",
            p["name"].as_str().unwrap_or("?"),
            p["id"].as_str().unwrap_or("?")
        )
    });
    Ok(code::OK)
}

/// "Light espresso: https://…/P/CODE" and how it has been used
fn profile_share_line(l: &Value) -> String {
    let times = |n: i64| match n {
        0 => "never".to_string(),
        1 => "once".to_string(),
        n => format!("{n} times"),
    };
    let mut s = format!(
        "{}: {}\n  opened {}, downloaded {} · made {} by {}",
        l["name"].as_str().unwrap_or("?"),
        l["url"].as_str().unwrap_or("?"),
        times(l["views"].as_i64().unwrap_or(0)),
        times(l["downloads"].as_i64().unwrap_or(0)),
        l["created_at"].as_str().unwrap_or("?"),
        l["created_by"].as_str().unwrap_or("?")
    );
    if let Some(r) = l["revoked_at"].as_str() {
        let _ = write!(s, " · revoked {r}");
    }
    s
}

pub async fn profile_share(
    api: &Api,
    out: &Out,
    profile: String,
    new: bool,
) -> Result<u8, Failure> {
    let p = api.profile(&profile).await?;
    let id = p["id"].as_str().unwrap_or_default().to_string();
    let l = api
        .call(
            Method::POST,
            &format!("/api/profiles/{}/share", enc(&id)),
            Some(&json!({ "new": new })),
        )
        .await?;
    out.emit(&l, profile_share_line);
    Ok(code::OK)
}

pub async fn profile_shares(
    api: &Api,
    out: &Out,
    profile: Option<String>,
    all: bool,
) -> Result<u8, Failure> {
    let list = match profile {
        Some(p) => {
            let p = api.profile(&p).await?;
            let id = p["id"].as_str().unwrap_or_default().to_string();
            let mut l = api
                .get(&format!("/api/profiles/{}/share", enc(&id)))
                .await?;
            if !all {
                if let Some(a) = l.as_array_mut() {
                    a.retain(|x| x["revoked_at"].is_null());
                }
            }
            l
        }
        None => api.get(&format!("/api/profile-shares?all={all}")).await?,
    };
    out.emit(&list, |l| {
        let lines: Vec<String> = l
            .as_array()
            .into_iter()
            .flatten()
            .map(profile_share_line)
            .collect();
        if lines.is_empty() {
            "no profile share links".into()
        } else {
            lines.join("\n")
        }
    });
    Ok(code::OK)
}

pub async fn profile_unshare(api: &Api, out: &Out, code: String) -> Result<u8, Failure> {
    let c = code.trim().trim_end_matches('/');
    let c = c
        .trim_end_matches("/profile.json")
        .trim_end_matches("/profile.alog");
    let c = c.rsplit('/').next().unwrap_or(c);
    let l = api
        .call(
            Method::DELETE,
            &format!("/api/profile-shares/{}", enc(c)),
            None,
        )
        .await?;
    out.emit(&l, |l| format!("revoked: {}", profile_share_line(l)));
    Ok(code::OK)
}

pub async fn profile_import_link(
    api: &Api,
    out: &Out,
    url: String,
    name: Option<String>,
    preview: bool,
) -> Result<u8, Failure> {
    let mut body = json!({ "url": url, "name": name, "preview": preview });
    out::slim(&mut body);
    let r = api
        .call(Method::POST, "/api/profiles/import/link", Some(&body))
        .await?;
    out.emit(&r, |r| {
        let p = &r["profile"];
        let what = format!(
            "{} ({} {}, {} points, {} events) from {}",
            p["name"].as_str().unwrap_or("?"),
            p["roast_level"].as_str().unwrap_or("-"),
            p["roast_purpose"].as_str().unwrap_or("-"),
            p["points"].as_array().map_or(0, Vec::len),
            p["events"].as_array().map_or(0, Vec::len),
            r["source"].as_str().unwrap_or("?")
        );
        match r["saved"]["id"].as_str() {
            Some(id) => format!(
                "imported profile {} ({id}): {what}",
                r["saved"]["name"].as_str().unwrap_or("?")
            ),
            None => format!("would import {what} (--preview: nothing saved)"),
        }
    });
    Ok(code::OK)
}

pub async fn profile_export(
    api: &Api,
    out: &Out,
    profile: String,
    artisan: bool,
    output: Option<PathBuf>,
) -> Result<u8, Failure> {
    let p = api.profile(&profile).await?;
    let id = p["id"].as_str().unwrap_or_default().to_string();
    let (path, ext) = if artisan {
        (format!("/api/profiles/{}/export/artisan", enc(&id)), "alog")
    } else {
        (format!("/api/profiles/{}/export", enc(&id)), "json")
    };
    save_file(api, out, &path, output, &format!("{id}.{ext}")).await
}

#[derive(Args, Debug)]
pub struct ProfileEditArgs {
    /// The profile's id or name
    pub profile: String,
    /// A new name
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
    /// extremely-light … dark (as `edit --level`)
    #[arg(long)]
    pub level: Option<String>,
    /// espresso, filter, omni or other
    #[arg(long)]
    pub purpose: Option<String>,
    /// The roaster kind it is written for (the profile library lists the roaster's own kind
    /// first): fluid-bed-electric, drum-electric, drum-gas, logger, other, or any
    #[arg(long)]
    pub kind: Option<String>,
    /// First crack target, from CHARGE: 7:50
    #[arg(long)]
    pub target_fc: Option<String>,
    /// Total time target (the drop), from CHARGE: 10:05
    #[arg(long)]
    pub target_total: Option<String>,
    /// Drop temperature target, °C
    #[arg(long)]
    pub target_drop_temp: Option<f32>,
    #[arg(long)]
    pub charge_temp: Option<f32>,
    #[arg(long)]
    pub preheat_temp: Option<f32>,
    /// The batch it is made for, e.g. 100g
    #[arg(long, value_parser = parse_weight)]
    pub batch: Option<f64>,
    /// Which coffees it suits
    #[arg(long)]
    pub beans: Option<String>,
    /// Auto DROP: Profile drive marks DROP by itself at the profile's drop, after a countdown
    /// the Roast page can cancel. `time` (the drop time), `bt` (the drop temperature, from
    /// first crack on), `development` (drop minus first crack, after first crack is marked)
    /// or `off`
    #[arg(long, value_parser = ["time", "bt", "development", "off"])]
    pub auto_drop: Option<String>,
    /// The profile's own expected range, over its level's default (docs/roast-guidance.md):
    /// KEY=LOW-HIGH, KEY one of development (m:ss or s), loss (%), dtr (%), color (Agtron,
    /// ground); KEY=off goes back to the level's. Repeat for more: --expect development=1:45-2:45
    #[arg(long = "expect", value_name = "KEY=LOW-HIGH")]
    pub expect: Vec<String>,
}

/// `--expect` names → the profile's `expectations` keys.
const EXPECT_KEYS: [(&str, &str); 4] = [
    ("development", "development_s"),
    ("loss", "weight_loss_pct"),
    ("dtr", "dtr_pct"),
    ("color", "color_ground"),
];

/// The stored expectations with `--expect` applied: `null` when none is left.
fn apply_expect(stored: &Value, specs: &[String]) -> Result<Value, String> {
    let mut map = stored.as_object().cloned().unwrap_or_default();
    for spec in specs {
        let (name, range) = spec
            .split_once('=')
            .ok_or_else(|| format!("'{spec}': use KEY=LOW-HIGH, e.g. development=1:45-2:45"))?;
        let name = name.trim();
        let key = EXPECT_KEYS
            .iter()
            .find(|(short, long)| name == *short || name == *long)
            .map(|(_, long)| *long)
            .ok_or_else(|| {
                format!("unknown range '{name}' (one of development, loss, dtr, color)")
            })?;
        if range.trim() == "off" {
            map.remove(key);
            continue;
        }
        let num = |s: &str| -> Result<f64, String> {
            if key == "development_s" {
                parse_clock(s)
            } else {
                s.trim()
                    .parse::<f64>()
                    .map_err(|_| format!("'{s}' is not a number"))
            }
        };
        let (lo, hi) = range
            .split_once('-')
            .ok_or_else(|| format!("'{spec}': give LOW-HIGH, e.g. {name}=1:45-2:45"))?;
        let (lo, hi) = (num(lo)?, num(hi)?);
        if lo > hi {
            return Err(format!("'{spec}': low is above high"));
        }
        map.insert(key.into(), json!([lo, hi]));
    }
    Ok(if map.is_empty() {
        Value::Null
    } else {
        Value::Object(map)
    })
}

/// A profile's own expected ranges in words ("development 1:45–2:45 · loss 14–16 %"), or "".
pub fn expect_text(p: &Value) -> String {
    let Some(map) = p["expectations"].as_object() else {
        return String::new();
    };
    EXPECT_KEYS
        .iter()
        .filter_map(|(short, long)| {
            let r = map.get(*long)?.as_array()?;
            let (lo, hi) = (r.first()?.as_f64()?, r.get(1)?.as_f64()?);
            Some(match *long {
                "development_s" => format!(
                    "{short} {}–{}",
                    mmss(Some(lo.round() as i64)),
                    mmss(Some(hi.round() as i64))
                ),
                "color_ground" => format!("{short} {lo}–{hi} Agtron"),
                _ => format!("{short} {lo}–{hi} %"),
            })
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// A profile's auto DROP in words, from its targets.
pub fn auto_drop_text(p: &Value) -> String {
    let total = p["target_total_time"].as_i64();
    let fc = p["target_first_crack"].as_i64();
    match p["auto_drop"].as_str() {
        Some("time") => format!("auto DROP at the drop time, {}", mmss(total)),
        Some("bt") => format!(
            "auto DROP at the drop BT, {} °C (from first crack on)",
            p["target_end_temp"]
        ),
        Some("development") => format!(
            "auto DROP {} after first crack",
            mmss(total.zip(fc).map(|(t, f)| t - f))
        ),
        _ => "auto DROP off".into(),
    }
}

pub async fn profile_edit(api: &Api, out: &Out, a: ProfileEditArgs) -> Result<u8, Failure> {
    let found = api.profile(&a.profile).await?;
    let id = found["id"].as_str().unwrap_or_default().to_string();
    let path = format!("/api/profiles/{}", enc(&id));
    let p = api.get(&path).await?;
    let secs = |s: &Option<String>| -> Result<Option<i64>, Failure> {
        s.as_deref()
            .map(|t| parse_clock(t).map(|v| v.round() as i64))
            .transpose()
            .map_err(|m| Failure::new("USAGE", m))
    };
    let fc = secs(&a.target_fc)?;
    let total = secs(&a.target_total)?;
    let nothing = a.name.is_none()
        && a.description.is_none()
        && a.level.is_none()
        && a.purpose.is_none()
        && a.kind.is_none()
        && fc.is_none()
        && total.is_none()
        && a.target_drop_temp.is_none()
        && a.charge_temp.is_none()
        && a.preheat_temp.is_none()
        && a.batch.is_none()
        && a.beans.is_none()
        && a.auto_drop.is_none()
        && a.expect.is_empty();
    if nothing {
        return Err(Failure::new(
            "USAGE",
            "nothing to change: e.g. --target-fc 7:50",
        ));
    }
    // The update replaces the metadata and points; events and curves stay when left out.
    let points: Vec<Value> = p["points"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|x| {
            json!({
                "time_seconds": x["time_seconds"], "target_temp": x["target_temp"],
                "fan_speed": x["fan_speed"], "notes": x["notes"],
                "target_env_temp": x["target_env_temp"], "heater_pct": x["heater_pct"],
            })
        })
        .collect();
    let keep = |key: &str| p[key].clone();
    let mut body = json!({
        "name": a.name.map_or(keep("name"), Value::from),
        "description": a.description.map_or(keep("description"), Value::from),
        "target_first_crack": fc.map_or(keep("target_first_crack"), Value::from),
        "target_total_time": total.map_or(keep("target_total_time"), Value::from),
        "target_end_temp": a.target_drop_temp.map_or(keep("target_end_temp"), Value::from),
        "charge_temp": a.charge_temp.map_or(keep("charge_temp"), Value::from),
        "preheat_temp": a.preheat_temp.map_or(keep("preheat_temp"), Value::from),
        "batch_size_g": a.batch.map_or(keep("batch_size_g"), Value::from),
        "beans": a.beans.map_or(keep("beans"), Value::from),
        "roaster": keep("roaster"),
        "points": points,
    });
    if let Some(l) = &a.level {
        body["roast_level"] = json!(level(l));
    }
    if let Some(pu) = &a.purpose {
        body["roast_purpose"] = json!(level(pu));
    }
    if let Some(k) = &a.kind {
        // The server checks it against the setup kinds; `any` is none.
        body["roaster_kind"] = match level(k).as_str() {
            "any" => Value::Null,
            k => json!(k),
        };
    }
    if let Some(ad) = &a.auto_drop {
        body["auto_drop"] = if ad == "off" { Value::Null } else { json!(ad) };
    }
    if !a.expect.is_empty() {
        body["expectations"] =
            apply_expect(&p["expectations"], &a.expect).map_err(|m| Failure::new("USAGE", m))?;
    }
    let saved = api.call(Method::PUT, &path, Some(&body)).await?;
    let short = json!({
        "id": saved["id"], "name": saved["name"], "roast_level": saved["roast_level"],
        "roast_purpose": saved["roast_purpose"], "target_first_crack": saved["target_first_crack"],
        "target_total_time": saved["target_total_time"], "target_end_temp": saved["target_end_temp"],
        "auto_drop": saved["auto_drop"], "expectations": saved["expectations"],
        "roaster_kind": saved["roaster_kind"],
    });
    out.emit(&short, |s| {
        let ex = expect_text(s);
        format!(
            "{} (for {}): FC {}, drop {} at {} °C; {}{}",
            s["name"].as_str().unwrap_or("?"),
            s["roaster_kind"].as_str().unwrap_or("any roaster"),
            mmss(s["target_first_crack"].as_i64()),
            mmss(s["target_total_time"].as_i64()),
            s["target_end_temp"],
            auto_drop_text(s),
            if ex.is_empty() {
                String::new()
            } else {
                format!("; own ranges: {ex}")
            }
        )
    });
    Ok(code::OK)
}

pub async fn profile_rm(api: &Api, out: &Out, profile: String, wait: Wait) -> Result<u8, Failure> {
    let p = api.profile(&profile).await?;
    let id = p["id"].as_str().unwrap_or_default().to_string();
    let name = p["name"].as_str().unwrap_or("?").to_string();
    let reply = api
        .send(
            Method::DELETE,
            &format!("/api/profiles/{}", enc(&id)),
            None,
            Duration::from_secs(60),
        )
        .await?;
    held_or_done(api, out, reply, wait, move |_| format!("deleted {name}")).await
}

pub async fn profile_history(
    api: &Api,
    out: &Out,
    profile: String,
    limit: usize,
) -> Result<u8, Failure> {
    let p = api.profile(&profile).await?;
    let id = p["id"].as_str().unwrap_or_default().to_string();
    let mut list = api
        .get(&format!("/api/profiles/{}/history", enc(&id)))
        .await?;
    if let Some(items) = list.as_array_mut() {
        items.truncate(limit.max(1));
    }
    out.emit(&list, |rows| {
        let rows = rows.as_array().cloned().unwrap_or_default();
        if rows.is_empty() {
            return "No finished roasts with this profile".into();
        }
        rows.iter()
            .map(|r| {
                let t = |k: &str| mmss(r[k].as_f64().map(|v| v.round() as i64));
                format!(
                    "{}  #{:<4} {:<24} FC {} @ {}  drop {} @ {}  dev {}  loss {}",
                    r["roasted_at"]
                        .as_str()
                        .unwrap_or("?")
                        .get(..10)
                        .unwrap_or("?"),
                    r["batch_number"]
                        .as_i64()
                        .map_or("-".into(), |n| n.to_string()),
                    r["name"]
                        .as_str()
                        .unwrap_or("?")
                        .chars()
                        .take(24)
                        .collect::<String>(),
                    t("fc_s"),
                    r["fc_bt"]
                        .as_f64()
                        .map_or("-".into(), |v| format!("{v:.1}")),
                    t("drop_s"),
                    r["drop_bt"]
                        .as_f64()
                        .map_or("-".into(), |v| format!("{v:.1}")),
                    t("development_s"),
                    r["weight_loss_pct"]
                        .as_f64()
                        .map_or("-".into(), |v| format!("{v:.1}%")),
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    });
    Ok(code::OK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expect_sets_merges_and_clears_a_profiles_own_ranges() {
        let specs = |s: &[&str]| {
            s.iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
        };
        let set = apply_expect(&Value::Null, &specs(&["development=1:45-2:45"])).unwrap();
        assert_eq!(set, json!({ "development_s": [105.0, 165.0] }));
        // merged with what is stored; full keys and seconds work too
        let both = apply_expect(&set, &specs(&["weight_loss_pct=14-16.5"])).unwrap();
        assert_eq!(
            both,
            json!({ "development_s": [105.0, 165.0], "weight_loss_pct": [14.0, 16.5] })
        );
        assert_eq!(
            expect_text(&json!({ "expectations": both })),
            "development 1:45–2:45 · loss 14–16.5 %"
        );
        let secs = apply_expect(&Value::Null, &specs(&["development=105-165"])).unwrap();
        assert_eq!(secs, set);
        // off goes back to the level's; none left is null
        assert_eq!(
            apply_expect(&set, &specs(&["development=off"])).unwrap(),
            Value::Null
        );
        assert!(apply_expect(&Value::Null, &specs(&["crema=1-2"])).is_err());
        assert!(apply_expect(&Value::Null, &specs(&["loss=16-14"])).is_err());
        assert!(apply_expect(&Value::Null, &specs(&["loss=15"])).is_err());
        assert!(apply_expect(&Value::Null, &specs(&["development"])).is_err());
    }

    #[test]
    fn cupping_attributes_take_their_short_names() {
        assert_eq!(attribute("aroma=7.5").unwrap(), ("fragrance_aroma", 7.5));
        assert_eq!(attribute("Clean Cup=8").unwrap(), ("clean_cup", 8.0));
        assert_eq!(attribute("acidity:7.25").unwrap(), ("acidity", 7.25));
        assert!(attribute("acidity=11").is_err());
        assert!(attribute("crema=7").is_err());
        assert!(attribute("acidity").is_err());
    }

    #[test]
    fn order_lines_read_bean_weight_and_price() {
        assert_eq!(
            order_line("Kenya Nyeri AA=2lb@18.50").unwrap(),
            ("Kenya Nyeri AA".into(), 907.2, Some(18.5))
        );
        assert_eq!(
            order_line("Brazil=5kg").unwrap(),
            ("Brazil".into(), 5000.0, None)
        );
        assert_eq!(order_line("A=B=1kg@$9").unwrap().0, "A=B");
        assert!(order_line("Brazil").is_err());
        assert!(order_line("Brazil=0g").is_err());
        assert!(order_line("=1kg").is_err());
    }

    #[test]
    fn levels_take_the_spelling_people_use() {
        assert_eq!(level("Medium-light"), "medium_light");
        assert_eq!(level(" very light "), "very_light");
    }

    #[test]
    fn file_names_from_the_server_stay_in_this_folder() {
        assert_eq!(safe_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_name("batch 4.alog"), "batch 4.alog");
        assert_eq!(safe_name(".alog"), "roast.alog");
    }
}
