//! Green coffee: beans and their stock, a supplier's coffee read from its link, the shopping
//! list and what coffee cost.

use std::time::Duration;

use clap::{Args, Subcommand};
use reqwest::Method;
use serde_json::json;

use crate::api::{self, enc, Api};
use crate::cli::Wait;
use crate::commands::approvals::held_or_done;
use crate::commands::{analysis, records};
use crate::out::{self, code, for_finding, Failure, Out};
use crate::text::{
    bean_line, beans_text, carts_text, costs_text, draft_text, grams, parse_weight, wanted_line,
    wanted_text, BeanRow,
};

#[derive(Subcommand, Debug)]
pub enum BeanCommand {
    /// One bean: details, stock ledger and roasts
    Show {
        /// The bean's id or name
        bean: String,
    },
    /// Add a bean; --stock records the first purchase
    Add {
        #[arg(long)]
        name: String,
        #[arg(long)]
        origin: Option<String>,
        #[arg(long)]
        variety: Option<String>,
        /// washed, natural, honey, …
        #[arg(long)]
        process: Option<String>,
        #[arg(long)]
        lot: Option<String>,
        #[arg(long)]
        supplier: Option<String>,
        #[arg(long)]
        notes: Option<String>,
        /// Green moisture, %
        #[arg(long)]
        moisture: Option<f32>,
        /// Green density, g/l
        #[arg(long)]
        density: Option<f32>,
        /// The first purchase, e.g. 5kg
        #[arg(long, value_parser = parse_weight)]
        stock: Option<f64>,
        /// Note "low" below this, e.g. 500g
        #[arg(long, value_parser = parse_weight)]
        low_stock: Option<f64>,
        #[command(flatten)]
        details: DetailArgs,
    },
    /// Read a coffee from a supplier's product link (Sweet Maria's, Bodhi Leaf, Theta Ridge,
    /// Coffee Bean Corral, Showroom, Happy Mug, Burman). Without a flag only a preview: name,
    /// specs and every bag size's price. --add saves it as a bean; --want SIZE puts it on the
    /// shopping list; --bought SIZE records the purchase (a new bean, or --bean)
    Import {
        /// https link to one product (named `link`: `--url` is the server)
        link: String,
        #[arg(long, conflicts_with_all = ["want", "bought"])]
        add: bool,
        /// A size from the preview, e.g. "5 LB"
        #[arg(long, conflicts_with = "bought")]
        want: Option<String>,
        /// A size from the preview, e.g. "5 LB"
        #[arg(long)]
        bought: Option<String>,
        /// With --bought: the order's shipping for this coffee
        #[arg(long)]
        shipping: Option<f64>,
        /// With --want or --bought: buy again for this bean (id or name)
        #[arg(long)]
        bean: Option<String>,
        /// With --want: how many bags of that size (default 1)
        #[arg(long, requires = "want", value_parser = clap::value_parser!(i64).range(1..=100))]
        quantity: Option<i64>,
    },
    /// Before a bean's first roast on a roaster, what to expect (first crack's BT and time on
    /// the profile, the supplier quoted); after a roast, what to change for the next one, in
    /// seconds or °C, while the bean has fewer than three on its profile. Only reads
    Guide(analysis::GuideArgs),
    /// Edit a bean: name, origin, process, notes, low-stock mark, …; --archive hides it;
    /// --clear lot,notes empties fields
    Edit(records::BeanEditArgs),
    /// Record stock: `add` a purchase, `set` what the scale says; `edit` or `rm` one in the
    /// ledger (`bean show` lists their ids)
    #[command(subcommand)]
    Stock(StockCommand),
    /// Delete a bean (one that roasts used is archived instead); may wait for the owner's
    /// approval
    Rm {
        /// The bean's id or name
        bean: String,
        #[command(flatten)]
        wait: Wait,
    },
}

#[derive(Args, Debug, Default)]
pub struct PurchaseArgs {
    /// What the coffee cost for this weight (money, in --currency)
    #[arg(long)]
    pub price: Option<f64>,
    /// This purchase's shipping
    #[arg(long)]
    pub shipping: Option<f64>,
    /// Taxes, duty, fees
    #[arg(long)]
    pub fees: Option<f64>,
    /// ISO code, e.g. USD (default: the server's currency, Settings → Roasting)
    #[arg(long)]
    pub currency: Option<String>,
    #[arg(long)]
    pub supplier: Option<String>,
    /// When it was bought, YYYY-MM-DD (default today)
    #[arg(long)]
    pub date: Option<String>,
    /// e.g. 2026 or 2025/26
    #[arg(long)]
    pub crop_year: Option<String>,
    /// When the lot landed at the supplier, e.g. 2026-07
    #[arg(long)]
    pub arrival: Option<String>,
}

/// What a supplier publishes about a coffee (`bean add`).
#[derive(Args, Debug, Default)]
pub struct DetailArgs {
    #[arg(long)]
    pub region: Option<String>,
    /// Farm, station, cooperative or grower
    #[arg(long)]
    pub producer: Option<String>,
    /// As published, e.g. "1900-2200 masl"
    #[arg(long)]
    pub altitude: Option<String>,
    #[arg(long)]
    pub grade: Option<String>,
    /// Comma-separated, e.g. "Organic, Fairtrade"
    #[arg(long)]
    pub certifications: Option<String>,
    /// The supplier's cupping score
    #[arg(long)]
    pub score: Option<f32>,
    /// The offer's link (not `--url`: that is the server)
    #[arg(long)]
    pub link: Option<String>,
    /// A few tasting words its labels print, e.g. "chocolate · coconut · apple" (instead of
    /// the supplier's prose; `bean show` has a suggestion)
    #[arg(long)]
    pub label_notes: Option<String>,
    /// The name its labels print when its own is long, e.g. "Kenya Gatugi"
    #[arg(long)]
    pub short_name: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum WantedCommand {
    /// It arrived: the bean (new from the link's details, or the entry's) gets the purchase
    Bought {
        id: String,
        /// What arrived, if not the entry's size
        #[arg(long, value_parser = parse_weight)]
        weight: Option<f64>,
        /// What it cost, if not the price on the list
        #[arg(long)]
        price: Option<f64>,
        #[arg(long)]
        shipping: Option<f64>,
        /// YYYY-MM-DD (default today)
        #[arg(long)]
        date: Option<String>,
    },
    /// Take it off the list
    Rm { id: String },
    /// Per supplier one link that opens a filled cart in the owner's browser (each bag's
    /// price and availability read again first); nothing is bought. A Shopify link replaces
    /// the cart in that browser
    Cart {
        /// Only this supplier's
        #[arg(long)]
        supplier: Option<String>,
        /// Only these entries (ids from `roast wanted`)
        #[arg(long = "id")]
        ids: Vec<String>,
        /// Use the prices and bags as the list saw them; read nothing again
        #[arg(long)]
        offline: bool,
    },
    /// How many bags of the entry's size
    Qty {
        id: String,
        #[arg(value_parser = clap::value_parser!(i64).range(1..=100))]
        quantity: i64,
    },
}

#[derive(Subcommand, Debug)]
pub enum StockCommand {
    /// A purchase (or, negative, a loss): the weight is added to the stock. --price and the
    /// rest go with a purchase; the bean's cost per kg is the average of what is left
    Add {
        /// The bean's id or name
        bean: String,
        /// e.g. 5kg, 2500g, 2lb
        #[arg(value_parser = parse_weight, allow_hyphen_values = true)]
        weight: f64,
        #[arg(long)]
        note: Option<String>,
        #[command(flatten)]
        purchase: PurchaseArgs,
    },
    /// A correction: the stock becomes this weight
    Set {
        bean: String,
        #[arg(value_parser = parse_weight)]
        weight: f64,
        #[arg(long)]
        note: Option<String>,
    },
    /// Fix a purchase or correction: its weight, price, shipping, date, …
    Edit(records::MoveEditArgs),
    /// Take a purchase or correction out of the ledger; may wait for the owner's approval
    Rm {
        bean: String,
        move_id: String,
        #[command(flatten)]
        wait: Wait,
    },
}

/// `roast beans`: the green beans and their stock.
pub async fn list(api: &Api, out: &Out, archived: bool, low: bool) -> Result<u8, Failure> {
    let path = if archived {
        "/api/beans?archived=true"
    } else {
        "/api/beans"
    };
    let mut list = api.get(path).await?;
    if low
        && let Some(items) = list.as_array_mut() {
            items.retain(|b| b["low"] == json!(true));
        }
    let beans: Vec<BeanRow> = api::decode(list.clone())?;
    if out.json {
        out.emit(&for_finding(list, &["created_at", "updated_at"]), |_| {
            String::new()
        });
    } else {
        out.emit(&beans, |b| beans_text(b));
    }
    Ok(code::OK)
}

/// `roast bean …`: the subcommand to its call.
pub async fn run(api: &Api, out: &Out, cmd: BeanCommand) -> Result<u8, Failure> {
    match cmd {
        BeanCommand::Show { bean } => {
            let b = api.bean(&bean).await?;
            let id = b["id"].as_str().unwrap_or_default().to_string();
            let detail = api.get(&format!("/api/beans/{}", enc(&id))).await?;
            out.emit(&detail, |d| {
                let row: BeanRow = serde_json::from_value(d.clone()).unwrap_or_default();
                let mut lines = vec![bean_line(&row)];
                let b = d;
                if let Some(n) = b["short_name"].as_str() {
                    lines.push(format!("  label name: {n}"));
                }
                match (b["label_notes"].as_str(), b["label_notes_suggested"].as_str()) {
                    (Some(n), _) => lines.push(format!("  label notes: {n}")),
                    (None, Some(s)) => lines.push(format!(
                        "  label notes: none (suggested from the supplier's: {s}; set with --label-notes)"
                    )),
                    _ => {}
                }
                for m in d["moves"].as_array().into_iter().flatten().take(10) {
                    lines.push(format!(
                        "  {}  {:>9}  {}{}  {}",
                        m["at"].as_str().unwrap_or("?").get(..10).unwrap_or("?"),
                        grams(m["delta_g"].as_f64().unwrap_or(0.0)),
                        m["kind"].as_str().unwrap_or("?"),
                        m["cost"]
                            .as_f64()
                            .map(|c| format!(" {c:.2}"))
                            .unwrap_or_default(),
                        m["note"].as_str().unwrap_or("")
                    ));
                }
                lines.join("\n")
            });
            Ok(code::OK)
        }
        BeanCommand::Add {
            name,
            origin,
            variety,
            process,
            lot,
            supplier,
            notes,
            moisture,
            density,
            stock,
            low_stock,
            details: d,
        } => {
            let mut body = json!({
                "name": name, "origin": origin, "variety": variety, "process": process,
                "lot": lot, "supplier": supplier, "notes": notes, "moisture_pct": moisture,
                "density_g_l": density, "stock_g": stock, "low_stock_g": low_stock,
                "region": d.region, "producer": d.producer, "altitude": d.altitude,
                "grade": d.grade, "certifications": d.certifications, "score": d.score,
                "url": d.link, "label_notes": d.label_notes, "short_name": d.short_name,
            });
            out::slim(&mut body);
            let bean = api.call(Method::POST, "/api/beans", Some(&body)).await?;
            let row: BeanRow = api::decode(bean.clone())?;
            out.emit(&bean, |_| format!("added: {}", bean_line(&row)));
            Ok(code::OK)
        }
        BeanCommand::Import {
            link,
            add,
            want,
            bought,
            shipping,
            bean,
            quantity,
        } => {
            let draft = api
                .call(
                    Method::POST,
                    "/api/beans/import",
                    Some(&json!({ "url": link })),
                )
                .await?;
            let bean_id = match &bean {
                Some(b) => Some(
                    api.bean(b).await?["id"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                ),
                None => None,
            };
            if add {
                // The draft's fields are a bean's; sizes and arrival are left out by the server.
                let created = api.call(Method::POST, "/api/beans", Some(&draft)).await?;
                let row: BeanRow = api::decode(created.clone())?;
                out.emit(&created, |_| format!("added: {}", bean_line(&row)));
                return Ok(code::OK);
            }
            if let Some(size) = want.as_ref().or(bought.as_ref()) {
                let mut body = json!({ "draft": draft, "size": size, "bean_id": bean_id,
                    "quantity": quantity });
                out::slim(&mut body);
                let w = api.call(Method::POST, "/api/wanted", Some(&body)).await?;
                if bought.is_none() {
                    out.emit(&w, |w| format!("wanted: {}", wanted_line(w)));
                    return Ok(code::OK);
                }
                let id = w["id"].as_str().unwrap_or_default().to_string();
                let mut body = json!({ "shipping": shipping });
                out::slim(&mut body);
                let r = api
                    .call(
                        Method::POST,
                        &format!("/api/wanted/{}/bought", enc(&id)),
                        Some(&body),
                    )
                    .await?;
                let row: BeanRow = api::decode(r["bean"].clone())?;
                out.emit(&r, |_| format!("bought: {}", bean_line(&row)));
                return Ok(code::OK);
            }
            out.emit(&draft, draft_text);
            Ok(code::OK)
        }
        BeanCommand::Rm { bean, wait } => {
            let b = api.bean(&bean).await?;
            let id = b["id"].as_str().unwrap_or_default().to_string();
            let reply = api
                .send(
                    Method::DELETE,
                    &format!("/api/beans/{}", enc(&id)),
                    None,
                    Duration::from_secs(60),
                )
                .await?;
            let name = b["name"].as_str().unwrap_or("?").to_string();
            held_or_done(api, out, reply, wait, move |r| match r["result"].as_str() {
                Some("archived") => format!("archived {name} (roasts use it)"),
                _ => format!("deleted {name}"),
            })
            .await
        }
        BeanCommand::Edit(a) => records::bean_edit(api, out, a).await,
        BeanCommand::Guide(a) => analysis::guide(api, out, a).await,
        BeanCommand::Stock(StockCommand::Edit(a)) => records::move_edit(api, out, a).await,
        BeanCommand::Stock(StockCommand::Rm {
            bean,
            move_id,
            wait,
        }) => records::move_rm(api, out, bean, move_id, wait).await,
        BeanCommand::Stock(cmd) => {
            let (bean, body) = match cmd {
                StockCommand::Add {
                    bean,
                    weight,
                    note,
                    purchase: p,
                } => {
                    if weight == 0.0 {
                        return Err(Failure::new("USAGE", "the weight must not be 0"));
                    }
                    (
                        bean,
                        json!({
                            "delta_g": weight, "note": note, "price": p.price,
                            "shipping": p.shipping, "fees": p.fees, "currency": p.currency,
                            "supplier": p.supplier, "at": p.date, "crop_year": p.crop_year,
                            "arrival": p.arrival,
                        }),
                    )
                }
                StockCommand::Set { bean, weight, note } => {
                    (bean, json!({ "set_g": weight, "note": note }))
                }
                StockCommand::Edit(_) | StockCommand::Rm { .. } => unreachable!("matched above"),
            };
            let b = api.bean(&bean).await?;
            let id = b["id"].as_str().unwrap_or_default().to_string();
            let mut body = body;
            out::slim(&mut body);
            let updated = api
                .call(
                    Method::POST,
                    &format!("/api/beans/{}/stock", enc(&id)),
                    Some(&body),
                )
                .await?;
            let row: BeanRow = api::decode(updated.clone())?;
            out.emit(&updated, |_| bean_line(&row));
            Ok(code::OK)
        }
    }
}

/// `roast wanted`: the shopping list, and what happens to an entry.
pub async fn wanted(
    api: &Api,
    out: &Out,
    cmd: Option<WantedCommand>,
    all: bool,
) -> Result<u8, Failure> {
    match cmd {
        None => {
            let list = api
                .get(if all {
                    "/api/wanted?all=true"
                } else {
                    "/api/wanted"
                })
                .await?;
            let list = for_finding(list, &["draft", "created_at", "updated_at"]);
            out.emit(&list, wanted_text);
            Ok(code::OK)
        }
        Some(WantedCommand::Bought {
            id,
            weight,
            price,
            shipping,
            date,
        }) => {
            let mut body = json!({ "weight_g": weight, "price": price,
                "shipping": shipping, "at": date });
            out::slim(&mut body);
            let r = api
                .call(
                    Method::POST,
                    &format!("/api/wanted/{}/bought", enc(&id)),
                    Some(&body),
                )
                .await?;
            let row: BeanRow = api::decode(r["bean"].clone())?;
            out.emit(&r, |_| format!("bought: {}", bean_line(&row)));
            Ok(code::OK)
        }
        Some(WantedCommand::Cart {
            supplier,
            ids,
            offline,
        }) => {
            let mut body = json!({ "supplier": supplier, "offline": offline,
                "ids": (!ids.is_empty()).then_some(ids) });
            out::slim(&mut body);
            let r = api
                .send(
                    Method::POST,
                    "/api/wanted/cart",
                    Some(&body),
                    Duration::from_secs(300),
                )
                .await?
                .body;
            out.emit(&r, carts_text);
            Ok(code::OK)
        }
        Some(WantedCommand::Qty { id, quantity }) => {
            let r = api
                .call(
                    Method::PUT,
                    &format!("/api/wanted/{}", enc(&id)),
                    Some(&json!({ "quantity": quantity })),
                )
                .await?;
            out.emit(&r, |w| format!("wanted: {}", wanted_line(w)));
            Ok(code::OK)
        }
        Some(WantedCommand::Rm { id }) => {
            api.send(
                Method::DELETE,
                &format!("/api/wanted/{}", enc(&id)),
                None,
                Duration::from_secs(30),
            )
            .await?;
            out.emit(&json!({ "removed": id }), |_| format!("removed {id}"));
            Ok(code::OK)
        }
    }
}

/// `roast costs`: green coffee by month.
pub async fn costs(api: &Api, out: &Out, months: u32) -> Result<u8, Failure> {
    let report = api.get(&format!("/api/costs?months={months}")).await?;
    out.emit(&report, costs_text);
    Ok(code::OK)
}
