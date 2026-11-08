//! The shopping list (`assets/reports/shopping.typ`, schema `roastwarden.report.shopping/1`):
//! what is on the list, one short purchase order per supplier with what each bag cost when
//! it was seen and the shipping that supplier usually adds; then the coffees running low.

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use sqlx::SqlitePool;

use super::inventory::{self, Row};
use super::{ahead, day_month, long_date, meta, money, short_date, Options, ReportError, DASH};
use crate::coffee::inventory::StockMove;
use crate::coffee::wanted::{Wanted, WantedService};

/// Coffees that run out within this many days are listed as running low
const SOON_DAYS: i64 = 30;

pub struct Input {
    pub wanted: Vec<Wanted>,
    pub stock: inventory::Input,
}

pub async fn gather(db: &SqlitePool) -> Result<Input, ReportError> {
    let wanted = WantedService::new(db.clone())
        .list(false)
        .await?
        .into_iter()
        .filter(|w| w.status == "wanted")
        .collect();
    Ok(Input {
        wanted,
        stock: inventory::gather(db).await?,
    })
}

/// `www.sweetmarias.com/products/…` → `sweetmarias.com`
fn host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let h = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    h.strip_prefix("www.").unwrap_or(h).to_string()
}

/// The shipping a supplier's orders usually carried: the mean per order of the purchases
/// with shipping (an order is its `order_id`, else the purchase), and how many there were
fn usual_shipping(moves: &[StockMove], supplier: &str) -> Option<(f64, usize)> {
    let mut per_order: HashMap<String, f64> = HashMap::new();
    for m in moves.iter().filter(|m| {
        m.kind == "purchase"
            && m.currency.is_none()
            && m.supplier
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case(supplier))
    }) {
        if let Some(s) = m.shipping {
            *per_order
                .entry(m.order_id.clone().unwrap_or_else(|| m.id.clone()))
                .or_default() += s;
        }
    }
    (!per_order.is_empty()).then(|| {
        (
            per_order.values().sum::<f64>() / per_order.len() as f64,
            per_order.len(),
        )
    })
}

pub fn build(i: &Input, o: &Options) -> Value {
    let kind = super::kind("shopping").expect("the shopping kind");
    let today = o.local(o.now);
    // One order per supplier, in the order the list was made
    let mut orders: BTreeMap<String, Vec<&Wanted>> = BTreeMap::new();
    for w in &i.wanted {
        orders
            .entry(
                w.supplier
                    .clone()
                    .unwrap_or_else(|| "No supplier named".into()),
            )
            .or_default()
            .push(w);
    }
    let mut all_total = 0.0;
    let mut priced_all = true;
    let mut shipping_all = true;
    let blocks: Vec<Value> = orders
        .iter()
        .map(|(supplier, lines)| {
            let mut subtotal = 0.0;
            let mut priced = true;
            let mut any = false;
            let rows: Vec<Value> = lines
                .iter()
                .map(|w| {
                    let foreign = w
                        .currency
                        .as_deref()
                        .filter(|c| !c.eq_ignore_ascii_case(&o.currency));
                    let bags = w.quantity.max(1) as f64;
                    match (w.price, foreign) {
                        (Some(p), None) => {
                            subtotal += p * bags;
                            any = true;
                        }
                        _ => priced = false,
                    }
                    let each = |m: String| if bags > 1.0 { format!("{m} × {bags:.0}") } else { m };
                    json!({
                        "coffee": w.bean_name.clone().unwrap_or_else(|| w.name.clone()),
                        "again": w.bean_id.is_some(),
                        "size": each(w.size_label.clone().unwrap_or_else(|| w.weight_g.map_or(DASH.into(), |g| format!("{} g", super::grams(g))))),
                        "price": match (w.price, foreign) {
                            (Some(p), Some(c)) => each(super::money(c, p)),
                            (Some(p), None) => each(money(&o.currency, p)),
                            _ => DASH.into(),
                        },
                        "seen": day_month(&o.local(w.updated_at)),
                        "link": w.url.clone(),
                        "notes": w.notes.clone(),
                    })
                })
                .collect();
            let shipping = usual_shipping(&i.stock.moves, supplier);
            shipping_all &= shipping.is_some();
            let total = subtotal + shipping.map_or(0.0, |s| s.0);
            all_total += total;
            priced_all &= priced;
            json!({
                "supplier": supplier,
                "site": lines.iter().find_map(|w| w.url.as_deref()).map(host),
                "lines": rows,
                "subtotal": if any { money(&o.currency, subtotal) } else { DASH.into() },
                "shipping": shipping.map_or_else(|| "not known yet".to_string(), |(s, _)| format!("about {}", money(&o.currency, s))),
                "shipping_basis": shipping.map(|(_, n)| format!("the mean of {n} earlier order{}", if n == 1 { "" } else { "s" })),
                "total": if any || shipping.is_some() { money(&o.currency, total) } else { DASH.into() },
                "complete": priced,
            })
        })
        .collect();

    // Running low: below the low mark, or out within a month at the recent pace
    let soon = o.now + Duration::days(SOON_DAYS);
    let rows: Vec<Row> = inventory::rows(&i.stock, o.now);
    let listed: Vec<String> = i.wanted.iter().filter_map(|w| w.bean_id.clone()).collect();
    let low: Vec<Value> = rows
        .iter()
        .filter(|r| r.bean.low || r.runs_out.is_some_and(|t: DateTime<Utc>| t <= soon))
        .map(|r| {
            json!({
                "coffee": r.bean.name,
                "left": format!("{} kg", format_kg(f64::from(r.bean.stock_g))),
                "runs_out": r.runs_out.map_or(DASH.into(), |t| ahead(&o.local(t), &o.local(o.now))),
                "why": if r.bean.low { "below its low mark" } else { "runs out within a month" },
                "supplier": r.bean.supplier.clone().unwrap_or_else(|| DASH.into()),
                "last_cost": r.bean.cost_per_kg.map_or(DASH.into(), |c| format!("{}/kg", money(&o.currency, c))),
                "listed": listed.contains(&r.bean.id),
            })
        })
        .collect();
    let lines: usize = i.wanted.len();
    json!({
        "schema": kind.schema,
        "meta": meta(kind, &short_date(&today), o),
        "as_of": long_date(&today),
        "summary": [
            { "label": "On the list", "value": format!("{lines} bag{}", if lines == 1 { "" } else { "s" }),
              "sub": format!("from {} supplier{}", orders.len(), if orders.len() == 1 { "" } else { "s" }) },
            { "label": "About",
              "value": if lines == 0 { DASH.to_string() } else { money(&o.currency, all_total) },
              "sub": if lines == 0 { "nothing on the list".to_string() }
                     else if !priced_all { "some prices not known".to_string() }
                     else if shipping_all { "with the usual shipping".to_string() }
                     else { "some shipping not known yet".to_string() } },
            { "label": "Running low", "value": low.len().to_string(),
              "sub": format!("below the low mark or out within {SOON_DAYS} days") },
        ],
        "orders": blocks,
        "low": low,
        "notes": "Prices as each bag was last seen on the supplier's page; they change. Shipping is what this supplier's earlier orders carried, as recorded with the purchases. Mark a bag bought on Beans → Shopping list (or roast wanted bought), and its purchase is recorded.",
    })
}

fn format_kg(g: f64) -> String {
    format!("{:.2}", g / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coffee::inventory::{BeanRequest, InventoryService, PurchaseFields, StockRequest};
    use crate::coffee::wanted::WantedRequest;
    use crate::labels::Renderer;
    use crate::reports::roast::tests::options;
    use crate::reports::{kind, render, Format, Paper, Units};

    #[tokio::test]
    async fn one_order_per_supplier_with_its_usual_shipping_and_what_runs_low() {
        let db = crate::db::open_memory().await;
        let inv = InventoryService::new(db.clone());
        // Two earlier Sweet Maria's orders: $8 and $12 shipping, so about $10
        let kenya = inv
            .create_bean(BeanRequest {
                name: Some("Kenya Nyeri AA".into()),
                supplier: Some(Some("Sweet Maria's".into())),
                low_stock_g: Some(1000.0),
                ..Default::default()
            })
            .await
            .unwrap();
        for (g, ship) in [(500.0, 8.0), (300.0, 12.0)] {
            inv.add_stock(
                &kenya.id,
                StockRequest {
                    delta_g: Some(g),
                    purchase: PurchaseFields {
                        price: Some(10.0),
                        shipping: Some(ship),
                        supplier: Some("Sweet Maria's".into()),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        }
        let w = WantedService::new(db.clone());
        for (name, supplier, price) in [
            ("Ethiopia Guji", Some("Sweet Maria's"), Some(46.5)),
            ("Colombia Huila", Some("Bodhi Leaf"), None),
        ] {
            w.add(WantedRequest {
                name: Some(name.into()),
                supplier: supplier.map(str::to_string),
                url: Some("https://www.sweetmarias.com/products/x".into()),
                price,
                size: Some("5 LB".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        }
        let i = gather(&db).await.unwrap();
        let o = options(Units::C, Paper::Letter);
        let d = build(&i, &o);
        let orders = d["orders"].as_array().unwrap();
        assert_eq!(orders.len(), 2);
        let sm = orders
            .iter()
            .find(|o| o["supplier"] == "Sweet Maria's")
            .unwrap();
        assert_eq!(sm["site"], "sweetmarias.com");
        assert_eq!(sm["subtotal"], "$46.50");
        assert_eq!(sm["shipping"], "about $10.00");
        assert_eq!(sm["shipping_basis"], "the mean of 2 earlier orders");
        assert_eq!(sm["total"], "$56.50");
        let bodhi = orders
            .iter()
            .find(|o| o["supplier"] == "Bodhi Leaf")
            .unwrap();
        assert_eq!(bodhi["complete"], false);
        assert_eq!(bodhi["shipping"], "not known yet");
        // Kenya: 800 g, below its 1 kg mark, not on the list
        assert_eq!(d["low"][0]["coffee"], "Kenya Nyeri AA");
        assert_eq!(d["low"][0]["why"], "below its low mark");
        assert_eq!(d["low"][0]["listed"], false);
        assert_eq!(d["summary"][2]["value"], "1");

        let r = Renderer::in_process();
        let pdf = render(&r, kind("shopping").unwrap(), d.clone(), Format::Pdf, 1, 96)
            .await
            .unwrap();
        assert_eq!(pdf.pages, Some(1));
        crate::reports::check::assert_clean("shopping", "shopping", &d);
        if let Ok(dir) = std::env::var("RR_REPORTS_OUT") {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(format!("{dir}/shopping.pdf"), &pdf.bytes).unwrap();
            let png = render(&r, kind("shopping").unwrap(), d, Format::Png, 1, 110)
                .await
                .unwrap();
            std::fs::write(format!("{dir}/shopping-1.png"), &png.bytes).unwrap();
        }
    }
}
