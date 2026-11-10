//! The period report (`assets/reports/period.typ`, schema `roastwarden.report.period/1`): a
//! month, a range or a year (the year in review): the roasts with a sum row, green roasted and
//! bought, what was spent and the landed cost per kg, each coffee's part, and the stock at
//! the end.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use serde_json::{Value, json};
use sqlx::SqlitePool;

use super::{DASH, Options, ReportError, g1, long_date, meta, money, n1, short_date, weight};
use crate::coffee::costs::{self, MoveIn};
use crate::coffee::inventory::StockMove;
use crate::roasts::types::RoastSession;

/// At most this many roasts in the table
pub const MAX_ROASTS: usize = 600;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// The days a report covers, both ends included
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// A whole calendar year or month was asked for (`year=`, `month=`)
    whole: Whole,
    /// The year or month asked for is not over: it ends today, "so far"
    pub so_far: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Whole {
    Year(i32),
    Month(i32, u32),
    Days,
}

impl Range {
    /// `year=2026`, `month=2026-10`, `from` and `to` (YYYY-MM-DD); default this month so far
    pub fn parse(
        year: Option<&str>,
        month: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        today: NaiveDate,
    ) -> Result<Self, ReportError> {
        let bad = |w: &str, v: &str| {
            ReportError::Bad(format!(
                "{w} '{v}': {}",
                match w {
                    "year" => "YYYY",
                    "month" => "YYYY-MM",
                    _ => "YYYY-MM-DD",
                }
            ))
        };
        let day = |w: &str, v: &str| {
            NaiveDate::parse_from_str(v.trim(), "%Y-%m-%d").map_err(|_| bad(w, v))
        };
        // A year or month still going ends today: the report says "so far"
        let whole = |from: NaiveDate, to: NaiveDate, whole: Whole| {
            let so_far = from <= today && today < to;
            Self {
                from,
                to: if so_far { today } else { to },
                whole,
                so_far,
            }
        };
        if let Some(y) = year {
            let n: i32 = y.trim().parse().map_err(|_| bad("year", y))?;
            let from = NaiveDate::from_ymd_opt(n, 1, 1).ok_or_else(|| bad("year", y))?;
            let to = NaiveDate::from_ymd_opt(n, 12, 31).ok_or_else(|| bad("year", y))?;
            return Ok(whole(from, to, Whole::Year(n)));
        }
        if let Some(m) = month {
            let from = NaiveDate::parse_from_str(&format!("{}-01", m.trim()), "%Y-%m-%d")
                .map_err(|_| bad("month", m))?;
            let next = if from.month() == 12 {
                NaiveDate::from_ymd_opt(from.year() + 1, 1, 1)
            } else {
                NaiveDate::from_ymd_opt(from.year(), from.month() + 1, 1)
            }
            .expect("a month follows");
            return Ok(whole(
                from,
                next - Duration::days(1),
                Whole::Month(from.year(), from.month()),
            ));
        }
        let from = match from {
            Some(f) => day("from", f)?,
            None => NaiveDate::from_ymd_opt(today.year(), today.month(), 1).expect("the first"),
        };
        let to = match to {
            Some(t) => day("to", t)?,
            None => today,
        };
        if to < from {
            return Err(ReportError::Bad(format!("{to} is before {from}")));
        }
        if (to - from).num_days() > 3700 {
            return Err(ReportError::Bad("at most ten years at once".into()));
        }
        Ok(Self {
            from,
            to,
            whole: Whole::Days,
            so_far: false,
        })
    }

    /// A calendar year (whole, or so far): the year in review
    pub fn year(&self) -> Option<i32> {
        match self.whole {
            Whole::Year(y) => Some(y),
            _ => None,
        }
    }

    /// A calendar month (whole, or so far)
    pub fn month(&self) -> Option<(i32, u32)> {
        match self.whole {
            Whole::Month(y, m) => Some((y, m)),
            _ => None,
        }
    }

    pub fn title(&self) -> String {
        if let Some(y) = self.year() {
            return format!("{y} in review");
        }
        if let Some((y, m)) = self.month() {
            return format!("{} {y}", MONTH_NAMES[m as usize - 1]);
        }
        let d = |x: NaiveDate| format!("{} {} {}", x.day(), MONTHS[x.month0() as usize], x.year());
        format!("{} – {}", d(self.from), d(self.to))
    }
}

pub struct Input {
    pub range: Range,
    /// Finished roasts in the range, oldest first
    pub roasts: Vec<RoastSession>,
    pub beans: HashMap<String, String>,
    pub profiles: HashMap<String, String>,
    pub cups: HashMap<String, f64>,
    /// Every stock move, oldest first
    pub moves: Vec<StockMove>,
}

pub async fn gather(db: &SqlitePool, range: Range, o: &Options) -> Result<Input, ReportError> {
    let all = sqlx::query_as::<_, RoastSession>(
        "SELECT * FROM roast_sessions WHERE status = 'completed' ORDER BY COALESCE(start_time, created_at)",
    )
    .fetch_all(db)
    .await?;
    let roasts: Vec<RoastSession> = all
        .into_iter()
        .filter(|s| {
            let d = o.local(s.start_time.unwrap_or(s.created_at)).date_naive();
            d >= range.from && d <= range.to
        })
        .take(MAX_ROASTS)
        .collect();
    let beans: HashMap<String, String> =
        sqlx::query_as::<_, (String, String)>("SELECT id, name FROM beans")
            .fetch_all(db)
            .await?
            .into_iter()
            .collect();
    let profiles: HashMap<String, String> =
        sqlx::query_as::<_, (String, String)>("SELECT id, name FROM roast_profiles")
            .fetch_all(db)
            .await?
            .into_iter()
            .collect();
    let cups: HashMap<String, f64> = sqlx::query_as::<_, (String, Option<f64>)>(
        "SELECT session_id, overall_score FROM cupping_scores",
    )
    .fetch_all(db)
    .await?
    .into_iter()
    .filter_map(|(s, f)| f.map(|f| (s, f)))
    .collect();
    let moves = sqlx::query_as::<_, StockMove>("SELECT * FROM bean_stock_moves ORDER BY at, rowid")
        .fetch_all(db)
        .await?;
    Ok(Input {
        range,
        roasts,
        beans,
        profiles,
        cups,
        moves,
    })
}

fn kg(g: f64) -> String {
    format!("{:.2}", g / 1000.0)
}

#[derive(Default)]
struct PerCoffee {
    roasts: usize,
    used_g: f64,
    bought_g: f64,
    spent: f64,
    priced_g: f64,
}

pub fn build(i: &Input, o: &Options) -> Value {
    let kind = super::kind("period").expect("the period kind");
    let r = i.range;
    let start: DateTime<Utc> = o.zone.day_start(r.from);
    let end: DateTime<Utc> = o.zone.day_start(r.to + Duration::days(1));
    let name = |b: Option<&String>| {
        b.and_then(|b| i.beans.get(b))
            .cloned()
            .unwrap_or_else(|| DASH.into())
    };

    // The roasts, with a sum row
    let mut green = 0.0;
    let mut out = 0.0;
    let mut losses = Vec::new();
    let rows: Vec<Value> = i
        .roasts
        .iter()
        .map(|s| {
            green += s.green_weight.map_or(0.0, f64::from);
            out += s.roasted_weight.map_or(0.0, f64::from);
            if let Some(l) = s.weight_loss_pct {
                losses.push(f64::from(l));
            }
            json!({
                "mark": s.batch_number.map_or_else(|| DASH.to_string(), |n| format!("#{n}")),
                "date": short_date(&o.local(s.start_time.unwrap_or(s.created_at))),
                "name": s.name,
                "coffee": s.bean_id.as_ref().and_then(|b| i.beans.get(b)).cloned().unwrap_or_else(|| s.name.clone()),
                "profile": s.profile_id.as_ref().and_then(|p| i.profiles.get(p)).cloned().unwrap_or_else(|| DASH.into()),
                "roaster": s.device_id,
                "green": s.green_weight.map_or(DASH.into(), |g| g1(f64::from(g))),
                "out": s.roasted_weight.map_or(DASH.into(), |g| g1(f64::from(g))),
                "loss": n1(s.weight_loss_pct.map(f64::from)),
                "cup": i.cups.get(&s.id).map_or(DASH.into(), |c| format!("{c:.2}")),
            })
        })
        .collect();
    let mean_loss = (!losses.is_empty()).then(|| losses.iter().sum::<f64>() / losses.len() as f64);

    // Green: bought, roasted and corrected in the range, by coffee; stock and value at the end
    let mut per: BTreeMap<String, PerCoffee> = BTreeMap::new();
    let mut bought_g = 0.0;
    let mut spent = 0.0;
    let mut priced_g = 0.0;
    let mut orders: HashSet<String> = HashSet::new();
    for m in i.moves.iter().filter(|m| m.at >= start && m.at < end) {
        let e = per.entry(m.bean_id.clone()).or_default();
        match m.kind.as_str() {
            "purchase" => {
                e.bought_g += f64::from(m.delta_g);
                bought_g += f64::from(m.delta_g);
                orders.insert(m.order_id.clone().unwrap_or_else(|| m.id.clone()));
                let mi = move_in(m);
                if let Some(c) = mi.purchase_cost(&o.currency) {
                    e.spent += c;
                    e.priced_g += f64::from(m.delta_g);
                    spent += c;
                    priced_g += f64::from(m.delta_g);
                }
            }
            "roast" => {
                e.used_g -= f64::from(m.delta_g);
                if m.session_id.is_some() {
                    e.roasts += 1;
                }
            }
            _ => {}
        }
    }
    // Stock at the end, and its value, from each coffee's ledger up to then
    let mut by_bean: BTreeMap<&str, Vec<MoveIn>> = BTreeMap::new();
    for m in i.moves.iter().filter(|m| m.at < end) {
        by_bean
            .entry(m.bean_id.as_str())
            .or_default()
            .push(move_in(m));
    }
    let mut stock_end = 0.0;
    let mut value_end = 0.0;
    let mut stock_rows: HashMap<&str, (f64, Option<f64>)> = HashMap::new();
    for (b, ms) in &by_bean {
        let l = costs::walk(ms, &o.currency);
        stock_end += l.stock_g.max(0.0);
        value_end += l.stock_value().unwrap_or(0.0);
        stock_rows.insert(b, (l.stock_g, l.stock_value()));
    }
    let coffees: Vec<Value> = per
        .iter()
        .map(|(b, p)| {
            let (stock, value) = stock_rows.get(b.as_str()).copied().unwrap_or((0.0, None));
            json!({
                "coffee": name(Some(b)),
                "roasts": p.roasts,
                "used": if p.used_g > 0.0 { kg(p.used_g) } else { DASH.into() },
                "bought": if p.bought_g > 0.0 { kg(p.bought_g) } else { DASH.into() },
                "spent": if p.spent > 0.0 { money(&o.currency, p.spent) } else { DASH.into() },
                "per_kg": if p.priced_g > 0.0 { money(&o.currency, p.spent / (p.priced_g / 1000.0)) } else { DASH.into() },
                "stock": kg(stock.max(0.0)),
                "value": value.map_or(DASH.into(), |v| money(&o.currency, v)),
            })
        })
        .collect();

    // By month, for a range of two months or more
    let mut months: BTreeMap<(i32, u32), (usize, f64, f64, f64)> = BTreeMap::new();
    let mut d = NaiveDate::from_ymd_opt(r.from.year(), r.from.month(), 1).expect("the first");
    while d <= r.to {
        months.insert((d.year(), d.month()), (0, 0.0, 0.0, 0.0));
        d = if d.month() == 12 {
            NaiveDate::from_ymd_opt(d.year() + 1, 1, 1)
        } else {
            NaiveDate::from_ymd_opt(d.year(), d.month() + 1, 1)
        }
        .expect("a month follows");
    }
    for s in &i.roasts {
        let t = o.local(s.start_time.unwrap_or(s.created_at));
        if let Some(e) = months.get_mut(&(t.year(), t.month())) {
            e.0 += 1;
            e.1 += s.green_weight.map_or(0.0, f64::from);
        }
    }
    for m in i
        .moves
        .iter()
        .filter(|m| m.at >= start && m.at < end && m.kind == "purchase")
    {
        let t = o.local(m.at);
        if let Some(e) = months.get_mut(&(t.year(), t.month())) {
            e.2 += f64::from(m.delta_g);
            e.3 += move_in(m).purchase_cost(&o.currency).unwrap_or(0.0);
        }
    }
    let many_years = r.from.year() != r.to.year();
    let month_label = |y: i32, m: u32| {
        if many_years {
            format!("{} {}", MONTHS[m as usize - 1], y % 100)
        } else {
            MONTHS[m as usize - 1].to_string()
        }
    };
    let by_month: Vec<Value> = if months.len() >= 2 {
        months
            .iter()
            .map(|((y, m), (n, g, b, c))| {
                json!({
                    "label": month_label(*y, *m),
                    "roasts": n, "green_kg": g / 1000.0, "green": if *g > 0.0 { kg(*g) } else { DASH.into() },
                    "bought": if *b > 0.0 { kg(*b) } else { DASH.into() },
                    "spent": if *c > 0.0 { money(&o.currency, *c) } else { DASH.into() },
                    "empty": *n == 0 && *b <= 0.0,
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    // The table: a run of months with nothing roasted or bought is one line ("Jan – Sep")
    let mut month_rows: Vec<Value> = Vec::new();
    let mut run: Option<(String, String, usize)> = None;
    let end_run = |run: &mut Option<(String, String, usize)>, rows: &mut Vec<Value>| {
        if let Some((first, last, n)) = run.take() {
            rows.push(json!({
                "label": if n == 1 { first } else { format!("{first} – {last}") },
                "roasts": 0, "green": DASH, "bought": DASH, "spent": DASH,
                "empty": true, "months": n,
            }));
        }
    };
    for (((y, m), _), row) in months.iter().zip(&by_month) {
        if row["empty"] == true {
            let label = month_label(*y, *m);
            run = Some(match run.take() {
                Some((first, _, n)) => (first, label, n + 1),
                None => (label.clone(), label, 1),
            });
        } else {
            end_run(&mut run, &mut month_rows);
            month_rows.push(row.clone());
        }
    }
    end_run(&mut run, &mut month_rows);

    // A few lines worth saying of a year
    let mut highlights = Vec::new();
    if !i.roasts.is_empty() {
        let mut count: HashMap<&str, usize> = HashMap::new();
        for s in &i.roasts {
            if let Some(b) = &s.bean_id {
                *count.entry(b).or_default() += 1;
            }
        }
        if let Some((b, n)) = count
            .iter()
            .max_by_key(|(b, n)| (**n, std::cmp::Reverse(**b)))
        {
            highlights.push(json!({ "label": "Roasted most", "value": format!("{} ({n} roast{})", name(Some(&b.to_string())), if *n == 1 { "" } else { "s" }) }));
        }
        let mut pc: HashMap<&str, usize> = HashMap::new();
        for s in &i.roasts {
            if let Some(p) = &s.profile_id {
                *pc.entry(p).or_default() += 1;
            }
        }
        if let Some((p, n)) = pc.iter().max_by_key(|(p, n)| (**n, std::cmp::Reverse(**p))) {
            highlights.push(json!({ "label": "Profile", "value": format!("{} ({n})", i.profiles.get(*p).cloned().unwrap_or_else(|| p.to_string())) }));
        }
        if let Some((s, c)) = i
            .roasts
            .iter()
            .filter_map(|s| i.cups.get(&s.id).map(|c| (s, *c)))
            .max_by(|a, b| a.1.total_cmp(&b.1))
        {
            highlights.push(json!({ "label": "Cupped best", "value": format!("{} {} ({c:.2})",
                s.batch_number.map_or(String::new(), |n| format!("#{n}")), name(s.bean_id.as_ref())) }));
        }
        let first = i.roasts.first().expect("not empty");
        let last = i.roasts.last().expect("not empty");
        highlights.push(
            json!({ "label": "First and last", "value": format!("{} and {}",
            short_date(&o.local(first.start_time.unwrap_or(first.created_at))),
            short_date(&o.local(last.start_time.unwrap_or(last.created_at)))) }),
        );
    }

    let roasters: std::collections::BTreeSet<&str> =
        i.roasts.iter().map(|s| s.device_id.as_str()).collect();
    let summary = vec![
        json!({ "label": "Roasts", "value": i.roasts.len().to_string(),
                "sub": format!("on {} roaster{}", roasters.len(), if roasters.len() == 1 { "" } else { "s" }) }),
        json!({ "label": "Green roasted", "value": weight(green),
                "sub": format!("{} out · loss {}", weight(out), mean_loss.map_or(DASH.into(), |l| format!("{l:.1} %"))) }),
        json!({ "label": "Bought", "value": weight(bought_g),
                "sub": if spent > 0.0 { format!("{} in {} order{}", money(&o.currency, spent), orders.len(), if orders.len() == 1 { "" } else { "s" }) } else { "nothing priced".into() },
                "note": (spent > 0.0).then(|| format!("{}/kg landed", money(&o.currency, spent / (priced_g / 1000.0)))) }),
        json!({ "label": if r.so_far { "Stock now" } else { "Stock at the end" }, "value": weight(stock_end),
                "sub": format!("worth {}", money(&o.currency, value_end)) }),
    ];
    let title = r.title();
    json!({
        "schema": kind.schema,
        "meta": meta(kind, &title, o),
        "period": {
            "title": title,
            "span": format!("{} to {}{}", long_date(&o.local(start)), long_date(&o.local(end - Duration::seconds(1))), if r.so_far { ", so far" } else { "" }),
            "year": r.year(),
            "so_far": r.so_far,
        },
        "summary": summary,
        "highlights": highlights,
        "by_month": by_month,
        "month_rows": month_rows,
        "coffees": coffees,
        "roasts": rows,
        "totals": {
            "roasts": i.roasts.len(),
            "green": weight(green), "out": weight(out),
            "loss": n1(mean_loss),
        },
        "cut": i.roasts.len() >= MAX_ROASTS,
        "notes": "Green roasted is the roasts' green weight; bought is the purchases recorded in the period, spent with their shipping and fees, landed per kg over the priced ones. Stock and its worth at the end are each coffee's ledger up to then, at the weighted average cost.",
    })
}

fn move_in(m: &StockMove) -> MoveIn {
    MoveIn {
        id: m.id.clone(),
        delta_g: f64::from(m.delta_g),
        kind: m.kind.clone(),
        price: m.price,
        shipping: m.shipping,
        fees: m.fees,
        currency: m.currency.clone(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::labels::Renderer;
    use crate::reports::roast::tests::{fixture, options};
    use crate::reports::{Format, Paper, Units, kind, render};

    /// A ledger move; `money` is a purchase's (price, shipping, order)
    pub(crate) fn mv(
        id: &str,
        bean: &str,
        at: &str,
        g: f32,
        money: Option<(f64, f64, Option<&str>)>,
    ) -> StockMove {
        let kind = if money.is_some() { "purchase" } else { "roast" };
        StockMove {
            id: id.into(),
            bean_id: bean.into(),
            at: at.parse().unwrap(),
            delta_g: g,
            kind: kind.into(),
            session_id: (kind == "roast").then(|| format!("s-{id}")),
            note: None,
            price: money.map(|m| m.0),
            shipping: money.map(|m| m.1),
            fees: None,
            currency: None,
            supplier: Some("Sweet Maria's".into()),
            crop_year: None,
            arrival: None,
            order_id: money.and_then(|m| m.2).map(str::to_string),
            cost: None,
            cost_per_kg: None,
        }
    }

    /// Two coffees, bought in September and October (one order of two), roasted in October
    pub(crate) fn moves() -> Vec<StockMove> {
        vec![
            mv(
                "p1",
                "pal",
                "2026-09-10T15:00:00Z",
                1000.0,
                Some((20.0, 5.0, None)),
            ),
            mv(
                "p2",
                "pal",
                "2026-10-05T15:00:00Z",
                500.0,
                Some((10.0, 3.0, Some("o1"))),
            ),
            mv(
                "p3",
                "ken",
                "2026-10-05T15:00:00Z",
                500.0,
                Some((12.0, 3.0, Some("o1"))),
            ),
            mv("r1", "pal", "2026-10-24T18:00:00Z", -100.0, None),
            mv("r2", "pal", "2026-10-25T18:00:00Z", -100.0, None),
        ]
    }

    fn input(range: Range) -> Input {
        let f = fixture();
        let mut a = f.session.clone();
        a.id = "a".into();
        a.bean_id = Some("pal".into());
        let mut b = a.clone();
        b.id = "b".into();
        b.batch_number = Some(4);
        b.start_time = Some("2026-10-24T18:00:00Z".parse().unwrap());
        Input {
            range,
            roasts: vec![b, a],
            beans: [
                ("pal".to_string(), "Palmichal Vecindario".to_string()),
                ("ken".to_string(), "Kenya AA".to_string()),
            ]
            .into_iter()
            .collect(),
            profiles: [(
                "fluidbed-espresso-short".to_string(),
                "Light espresso".to_string(),
            )]
            .into_iter()
            .collect(),
            cups: [("a".to_string(), 84.5)].into_iter().collect(),
            moves: moves(),
        }
    }

    #[test]
    fn ranges() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();
        let r = Range::parse(None, Some("2026-10"), None, None, today).unwrap();
        assert_eq!(
            (r.from.to_string(), r.to.to_string()),
            ("2026-10-01".into(), "2026-10-26".into())
        );
        assert!(r.so_far);
        let sep = Range::parse(None, Some("2026-09"), None, None, today).unwrap();
        assert_eq!(
            (sep.to.to_string(), sep.so_far),
            ("2026-09-30".into(), false)
        );
        let y = Range::parse(Some("2026"), None, None, None, today).unwrap();
        assert_eq!(y.title(), "2026 in review");
        assert_eq!((y.to.to_string(), y.so_far), ("2026-10-26".into(), true));
        assert_eq!(r.to.to_string(), "2026-10-26", "this month so far");
        assert_eq!(r.title(), "October 2026");
        let d = Range::parse(None, None, None, None, today).unwrap();
        assert_eq!(
            (d.from.to_string(), d.to.to_string()),
            ("2026-10-01".into(), "2026-10-26".into())
        );
        assert_eq!(d.title(), "1 Oct 2026 – 26 Oct 2026");
        assert!(Range::parse(None, None, Some("2026-10-20"), Some("2026-10-01"), today).is_err());
        assert!(Range::parse(None, Some("October"), None, None, today).is_err());
    }

    #[tokio::test]
    async fn a_month_and_a_year() {
        let o = options(Units::C, Paper::Letter);
        let today = NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();
        let m = build(
            &input(Range::parse(None, Some("2026-10"), None, None, today).unwrap()),
            &o,
        );
        assert_eq!(m["summary"][0]["value"], "2");
        assert_eq!(m["summary"][1]["value"], "200 g");
        // October: the order of two, 1 kg for 22 + 6 shipping
        assert_eq!(m["summary"][2]["value"], "1.00 kg");
        assert_eq!(m["summary"][2]["sub"], "$28.00 in 1 order");
        assert_eq!(m["summary"][2]["note"], "$28.00/kg landed");
        // At the end: 1300 g of Palmichal at its average, 500 g of Kenya
        assert_eq!(m["summary"][3]["value"], "1.80 kg");
        assert_eq!(m["roasts"][0]["mark"], "#4", "oldest first");
        assert_eq!(m["totals"]["green"], "200 g");
        assert_eq!(m["roasts"][0]["green"], "100.0", "one decimal in a table");
        assert_eq!(
            m["by_month"].as_array().unwrap().len(),
            0,
            "one month has no bars"
        );
        let y = build(
            &input(Range::parse(Some("2026"), None, None, None, today).unwrap()),
            &o,
        );
        assert_eq!(y["period"]["title"], "2026 in review");
        // 2026 on 26 October: the year so far, January to October
        assert_eq!(y["period"]["so_far"], true);
        assert_eq!(
            y["period"]["span"],
            "Thursday 1 January 2026 to Monday 26 October 2026, so far"
        );
        assert_eq!(y["by_month"].as_array().unwrap().len(), 10);
        assert_eq!(y["by_month"][8]["bought"], "1.00", "September");
        // January to August had nothing: one line
        let rows = y["month_rows"].as_array().unwrap();
        assert_eq!(rows.len(), 3, "{rows:?}");
        assert_eq!(rows[0]["label"], "Jan – Aug");
        assert_eq!(rows[0]["months"], 8);
        assert_eq!(rows[1]["label"], "Sep");
        // A year gone by is whole
        let last = Range::parse(Some("2025"), None, None, None, today).unwrap();
        assert!(!last.so_far);
        assert_eq!(last.to.to_string(), "2025-12-31");
        assert!(
            y["highlights"]
                .as_array()
                .unwrap()
                .iter()
                .any(|h| h["label"] == "Cupped best")
        );
        let r = Renderer::in_process();
        for (name, d) in [("period-month", m), ("period-year", y)] {
            let pdf = render(&r, kind("period").unwrap(), d.clone(), Format::Pdf, 1, 96)
                .await
                .unwrap();
            assert!(pdf.pages.unwrap() <= 2, "{name}");
            crate::reports::check::assert_clean("period", name, &d);
            if let Ok(dir) = std::env::var("RR_REPORTS_OUT") {
                std::fs::create_dir_all(&dir).unwrap();
                let png = render(&r, kind("period").unwrap(), d, Format::Png, 1, 110)
                    .await
                    .unwrap();
                std::fs::write(format!("{dir}/{name}-1.png"), &png.bytes).unwrap();
            }
        }
    }
}
