//! Purchases (`assets/reports/purchases.typ`, schema `roastwarden.report.purchases/1`): the
//! green coffee bought over a range, order by order with shipping and fees, each supplier's
//! share, and the landed cost per kg over time.

use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, Duration, Utc};
use serde_json::{Value, json};
use sqlx::SqlitePool;

use super::chart;
use super::period::Range;
use super::{DASH, Options, ReportError, long_date, meta, money, short_date};
use crate::coffee::inventory::StockMove;

pub struct Input {
    pub range: Range,
    /// Purchases in the range, oldest first
    pub moves: Vec<StockMove>,
    pub beans: BTreeMap<String, String>,
}

pub async fn gather(db: &SqlitePool, range: Range) -> Result<Input, ReportError> {
    let moves = sqlx::query_as::<_, StockMove>(
        "SELECT * FROM bean_stock_moves WHERE kind = 'purchase' ORDER BY at, rowid",
    )
    .fetch_all(db)
    .await?;
    let beans = sqlx::query_as::<_, (String, String)>("SELECT id, name FROM beans")
        .fetch_all(db)
        .await?
        .into_iter()
        .collect();
    Ok(Input {
        range,
        moves,
        beans,
    })
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn kg(g: f64) -> String {
    format!("{:.2}", g / 1000.0)
}

/// Orders with no supplier recorded, together under this name
const NO_SUPPLIER: &str = "No supplier named";

/// "2 suppliers"; orders with none named are not a supplier: "1 supplier, 2 orders unnamed"
fn supplier_count<V>(suppliers: &BTreeMap<String, (usize, V, V, V)>) -> String {
    let named = suppliers.keys().filter(|k| *k != NO_SUPPLIER).count();
    let unnamed = suppliers.get(NO_SUPPLIER).map_or(0, |s| s.0);
    let n = |k: usize, one: &str, many: &str| format!("{k} {}", if k == 1 { one } else { many });
    match (named, unnamed) {
        (k, 0) => n(k, "supplier", "suppliers"),
        (0, u) => format!("{}, no supplier named", n(u, "order", "orders")),
        (k, u) => format!(
            "{}, {} unnamed",
            n(k, "supplier", "suppliers"),
            n(u, "order", "orders")
        ),
    }
}

struct Order<'a> {
    at: DateTime<Utc>,
    supplier: Option<String>,
    lines: Vec<&'a StockMove>,
}

pub fn build(i: &Input, o: &Options) -> Value {
    let kind = super::kind("purchases").expect("the purchases kind");
    let r = i.range;
    let (start, end) = (
        o.zone.day_start(r.from),
        o.zone.day_start(r.to + Duration::days(1)),
    );
    let home = |m: &StockMove| {
        m.currency
            .as_deref()
            .is_none_or(|c| c.eq_ignore_ascii_case(&o.currency))
    };

    // Orders: the moves of one `order_id`, else each purchase alone
    let mut orders: Vec<Order> = Vec::new();
    let mut index: BTreeMap<String, usize> = BTreeMap::new();
    for m in i.moves.iter().filter(|m| m.at >= start && m.at < end) {
        let key = m.order_id.clone().unwrap_or_else(|| m.id.clone());
        match index.get(&key) {
            Some(&k) => orders[k].lines.push(m),
            None => {
                index.insert(key, orders.len());
                orders.push(Order {
                    at: m.at,
                    supplier: m.supplier.clone(),
                    lines: vec![m],
                });
            }
        }
    }
    let name = |b: &str| i.beans.get(b).cloned().unwrap_or_else(|| DASH.into());
    let mut total = (0.0, 0.0, 0.0, 0.0, 0.0); // g, goods, shipping, fees, priced g
    let mut suppliers: BTreeMap<String, (usize, f64, f64, f64)> = BTreeMap::new(); // orders, g, money, priced g
    let mut dots = Vec::new();
    let rows: Vec<Value> = orders
        .iter()
        .map(|ord| {
            let g: f64 = ord.lines.iter().map(|m| f64::from(m.delta_g)).sum();
            let priced: Vec<&&StockMove> = ord.lines.iter().filter(|m| home(m) && m.price.is_some()).collect();
            let goods: f64 = priced.iter().filter_map(|m| m.price).sum();
            let ship: f64 = priced.iter().filter_map(|m| m.shipping).sum();
            let fees: f64 = priced.iter().filter_map(|m| m.fees).sum();
            let pg: f64 = priced.iter().map(|m| f64::from(m.delta_g)).sum();
            let all = goods + ship + fees;
            total.0 += g;
            total.1 += goods;
            total.2 += ship;
            total.3 += fees;
            total.4 += pg;
            let s = suppliers.entry(ord.supplier.clone().unwrap_or_else(|| NO_SUPPLIER.into())).or_default();
            s.0 += 1;
            s.1 += g;
            s.2 += all;
            s.3 += pg;
            let lines: Vec<Value> = ord
                .lines
                .iter()
                .map(|m| {
                    let landed = (home(m) && m.price.is_some()).then(|| {
                        m.price.unwrap_or(0.0) + m.shipping.unwrap_or(0.0) + m.fees.unwrap_or(0.0)
                    });
                    if let Some(l) = landed.filter(|_| m.delta_g > 0.0) {
                        let per = l / (f64::from(m.delta_g) / 1000.0);
                        dots.push(((m.at - start).num_hours() as f64 / 24.0, per, name(&m.bean_id)));
                    }
                    json!({
                        "coffee": name(&m.bean_id),
                        "kg": kg(f64::from(m.delta_g)),
                        "price": match (m.price, home(m)) {
                            (Some(p), true) => money(&o.currency, p),
                            (Some(p), false) => money(m.currency.as_deref().unwrap_or("?"), p),
                            _ => DASH.into(),
                        },
                        "per_kg": landed.filter(|_| m.delta_g > 0.0).map_or(DASH.into(), |l| money(&o.currency, l / (f64::from(m.delta_g) / 1000.0))),
                        "crop": m.crop_year.clone(),
                    })
                })
                .collect();
            json!({
                "date": short_date(&o.local(ord.at)),
                "supplier": ord.supplier.clone().unwrap_or_else(|| DASH.into()),
                "lines": lines,
                "kg": kg(g),
                "goods": if goods > 0.0 { money(&o.currency, goods) } else { DASH.into() },
                "shipping": if ship > 0.0 { money(&o.currency, ship) } else { DASH.into() },
                "fees": if fees > 0.0 { money(&o.currency, fees) } else { DASH.into() },
                "total": if all > 0.0 { money(&o.currency, all) } else { DASH.into() },
                "landed": if pg > 0.0 { money(&o.currency, all / (pg / 1000.0)) } else { DASH.into() },
            })
        })
        .collect();
    let by_supplier: Vec<Value> = suppliers
        .iter()
        .map(|(s, (n, g, m, pg))| {
            json!({
                "supplier": s, "orders": n, "kg": kg(*g),
                "spent": if *m > 0.0 { money(&o.currency, *m) } else { DASH.into() },
                "landed": if *pg > 0.0 { money(&o.currency, m / (pg / 1000.0)) } else { DASH.into() },
            })
        })
        .collect();

    // Landed cost per kg over time: a dot per purchase
    let days = ((end - start).num_hours() as f64 / 24.0).max(1.0);
    let chart = (dots.len() >= 2).then(|| {
        let hi = dots.iter().map(|d| d.1).fold(0.0, f64::max);
        let lo = dots.iter().map(|d| d.1).fold(f64::MAX, f64::min);
        let y = chart::axis((lo * 0.9).floor().max(0.0), (hi * 1.05).ceil(), 4, |v| money(&o.currency, v).trim_end_matches(".00").to_string());
        // A tick on each month's first day (every third when the range is long)
        let mut ticks = Vec::new();
        let mut d = chrono::NaiveDate::from_ymd_opt(r.from.year(), r.from.month(), 1).expect("the first");
        let step = if days > 400.0 { 3 } else { 1 };
        let mut k = 0;
        while d <= r.to {
            let x = (o.zone.day_start(d) - start).num_hours() as f64 / 24.0;
            if x >= 0.0 && k % step == 0 {
                let month = MONTHS[d.month0() as usize];
                ticks.push(chart::Tick {
                    v: x,
                    label: if r.from.year() == r.to.year() {
                        month.to_string()
                    } else {
                        format!("{month} {:02}", d.year() % 100)
                    },
                });
            }
            k += 1;
            d = if d.month() == 12 { chrono::NaiveDate::from_ymd_opt(d.year() + 1, 1, 1) } else { chrono::NaiveDate::from_ymd_opt(d.year(), d.month() + 1, 1) }.expect("a month follows");
        }
        json!({
            "x": chart::Axis { min: 0.0, max: days, ticks },
            "y": y,
            "dots": dots.iter().map(|(x, y, n)| json!({ "x": x, "y": y, "label": n })).collect::<Vec<_>>(),
        })
    });
    let all = total.1 + total.2 + total.3;
    let title = r.title().replace(" in review", "");
    let summary = vec![
        json!({ "label": "Orders", "value": orders.len().to_string(), "sub": supplier_count(&suppliers) }),
        json!({ "label": "Green bought", "value": format!("{} kg", kg(total.0)), "sub": format!("{} purchase{}", orders.iter().map(|o| o.lines.len()).sum::<usize>(), if orders.iter().map(|o| o.lines.len()).sum::<usize>() == 1 { "" } else { "s" }) }),
        json!({ "label": "Spent", "value": if all > 0.0 { money(&o.currency, all) } else { DASH.into() },
                "sub": if total.2 + total.3 > 0.0 { format!("{} shipping and fees", money(&o.currency, total.2 + total.3)) } else { "no shipping recorded".into() } }),
        json!({ "label": "Landed", "value": if total.4 > 0.0 { format!("{}/kg", money(&o.currency, all / (total.4 / 1000.0))) } else { DASH.into() }, "sub": "over the priced purchases" }),
    ];
    json!({
        "schema": kind.schema,
        "meta": meta(kind, &title, o),
        "period": { "title": title, "span": format!("{} to {}{}", long_date(&o.local(start)), long_date(&o.local(end - Duration::seconds(1))), if r.so_far { ", so far" } else { "" }) },
        "summary": summary,
        "orders": rows,
        "by_supplier": by_supplier,
        "chart": chart,
        "totals": {
            "kg": kg(total.0),
            "goods": money(&o.currency, total.1),
            "shipping": if total.2 > 0.0 { money(&o.currency, total.2) } else { DASH.into() },
            "fees": if total.3 > 0.0 { money(&o.currency, total.3) } else { DASH.into() },
            "total": money(&o.currency, all),
        },
        "notes": "Landed cost is the coffee with its share of the order's shipping and fees, per kg. Purchases in another currency are listed in it and left out of the sums.",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::labels::Renderer;
    use crate::reports::period::tests::moves;
    use crate::reports::roast::tests::options;
    use crate::reports::{Format, Paper, Units, kind, render};

    #[tokio::test]
    async fn orders_with_their_shipping_and_landed_cost() {
        let o = options(Units::C, Paper::Letter);
        let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();
        let i = Input {
            range: Range::parse(Some("2026"), None, None, None, today).unwrap(),
            moves: moves()
                .into_iter()
                .filter(|m| m.kind == "purchase")
                .collect(),
            beans: [
                ("pal".to_string(), "Palmichal Vecindario".to_string()),
                ("ken".to_string(), "Kenya AA".to_string()),
            ]
            .into_iter()
            .collect(),
        };
        let d = build(&i, &o);
        assert_eq!(d["period"]["title"], "2026");
        let orders = d["orders"].as_array().unwrap();
        assert_eq!(orders.len(), 2, "the two-coffee order is one");
        assert_eq!(orders[0]["landed"], "$25.00");
        assert_eq!(orders[1]["lines"].as_array().unwrap().len(), 2);
        assert_eq!(orders[1]["total"], "$28.00");
        assert_eq!(d["totals"]["total"], "$53.00");
        assert_eq!(d["summary"][3]["value"], "$26.50/kg");
        assert_eq!(d["chart"]["dots"].as_array().unwrap().len(), 3);
        let r = Renderer::in_process();
        let pdf = render(
            &r,
            kind("purchases").unwrap(),
            d.clone(),
            Format::Pdf,
            1,
            96,
        )
        .await
        .unwrap();
        assert_eq!(pdf.pages, Some(1));
        crate::reports::check::assert_clean("purchases", "purchases", &d);
        if let Ok(dir) = std::env::var("RR_REPORTS_OUT") {
            std::fs::create_dir_all(&dir).unwrap();
            let png = render(&r, kind("purchases").unwrap(), d, Format::Png, 1, 110)
                .await
                .unwrap();
            std::fs::write(format!("{dir}/purchases-1.png"), &png.bytes).unwrap();
        }
    }

    #[test]
    fn orders_with_no_supplier_are_not_a_supplier() {
        let mut s: BTreeMap<String, (usize, f64, f64, f64)> = BTreeMap::new();
        s.insert("Sweet Maria's".into(), (3, 0.0, 0.0, 0.0));
        assert_eq!(supplier_count(&s), "1 supplier");
        s.insert(NO_SUPPLIER.into(), (2, 0.0, 0.0, 0.0));
        assert_eq!(supplier_count(&s), "1 supplier, 2 orders unnamed");
        s.remove("Sweet Maria's");
        assert_eq!(supplier_count(&s), "2 orders, no supplier named");
    }
}
