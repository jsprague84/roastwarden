//! Cart links from the shopping list (`docs/green-coffee.md`, "Cart links"): per
//! supplier one link that opens a filled cart in the owner's browser. Shopify stores take the
//! UCP permalink their profile advertises (`/.well-known/ucp`, `/buy/<variant>:<qty>,…`), else
//! the older `/cart/<variant>:<qty>,…`; Burman (WooCommerce) its `/checkout-link/?products=
//! <id>:<lb>,…`. Each line's price and availability are read again first, one request at a
//! time a few seconds apart. Nothing is bought: the owner checks out by hand.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::coffee::import::{store_busy, CartShop, Draft, Importer, Size};
use crate::coffee::orders::supplier_key;
use crate::coffee::wanted::{Wanted, WantedService};

/// Between two requests to a supplier: they rate-limit readily (Theta Ridge answered 429 to
/// one plain request during the research)
const PAUSE: Duration = Duration::from_secs(3);
/// Longer links may be cut by a store (the UCP permalink spec's limit)
const MAX_LINK: usize = 2048;
const LB: f64 = 453.59237;

/// Which entries to put in carts: all that are wanted, or some.
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct CartRequest {
    /// Only this supplier's
    pub supplier: Option<String>,
    /// Only these entries
    pub ids: Option<Vec<String>>,
    /// Read nothing from the stores: the bags and prices as the list saw them, the permalink
    /// only if the store's profile was read today (else its `/cart/` link)
    #[serde(default)]
    pub offline: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Carts {
    pub carts: Vec<Cart>,
    /// Entries no cart could take, and why
    pub left_out: Vec<LeftOut>,
}

/// One supplier's cart link.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Cart {
    pub supplier: String,
    pub host: String,
    /// permalink (UCP `/buy`), cart (Shopify's `/cart/`) or checkout_link (WooCommerce)
    #[expect(
        clippy::doc_markdown,
        reason = "OpenAPI description: a change here changes docs/openapi.json"
    )]
    pub how: String,
    /// The link to open in the owner's browser; none when no line can go in it
    pub url: Option<String>,
    pub lines: Vec<CartLine>,
    /// The lines in the link, at the prices read now (no shipping or tax)
    pub total: f64,
    pub currency: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct CartLine {
    pub wanted_id: String,
    pub name: String,
    pub size_label: Option<String>,
    /// Bags (pounds at a store that sells by the pound)
    pub quantity: f64,
    /// What the store's link takes for this bag
    pub variant_id: Option<String>,
    /// One bag's price now (or as seen, when it could not be read again)
    pub price: Option<f64>,
    /// The price the list saw
    pub seen_price: Option<f64>,
    pub available: Option<bool>,
    /// Read again from the store just now
    pub checked: bool,
    /// In the link
    pub in_link: bool,
    /// Why not, or what changed
    pub note: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct LeftOut {
    pub wanted_id: String,
    pub name: String,
    pub why: String,
}

/// The bag the entry chose, in a draft: by its label, else its weight.
fn bag<'a>(d: &'a Draft, w: &Wanted) -> Option<&'a Size> {
    w.size_label
        .as_deref()
        .and_then(|l| {
            d.sizes
                .iter()
                .find(|z| z.label.eq_ignore_ascii_case(l.trim()))
        })
        .or_else(|| w.weight_g.and_then(|g| d.size_for(g)))
        .or_else(|| (d.sizes.len() == 1).then(|| &d.sizes[0]))
}

/// One cart's link and lines, from each entry and the bag read for it (pure: the tests'). The
/// endpoint is the store's UCP permalink, if it has one.
pub fn build(
    shop: &CartShop,
    endpoint: Option<&str>,
    entries: &[(&Wanted, Option<Size>, bool)],
) -> Cart {
    let mut lines = Vec::new();
    let mut parts = Vec::new();
    let mut warnings = Vec::new();
    let mut total = 0.0;
    let mut currency = None;
    for (w, z, checked) in entries {
        let qty = w.quantity.max(1) as f64;
        // Burman sells by the pound: the link's quantity is pounds
        let link_qty = if shop.by_the_pound {
            ((w.weight_g.unwrap_or(LB) * qty) / LB).round().max(1.0)
        } else {
            qty
        };
        let mut line = CartLine {
            wanted_id: w.id.clone(),
            name: w.bean_name.clone().unwrap_or_else(|| w.name.clone()),
            size_label: w.size_label.clone(),
            quantity: link_qty,
            variant_id: z.as_ref().and_then(|z| z.variant_id.clone()),
            price: z.as_ref().and_then(|z| z.price).or(w.price),
            seen_price: w.price,
            available: z.as_ref().and_then(|z| z.available),
            checked: *checked,
            in_link: false,
            note: None,
        };
        match (&z, &line.variant_id) {
            (None, _) => line.note = Some("that bag is not on the store's page now".into()),
            (_, None) => {
                line.note =
                    Some("no shop id for this bag: add the link again (`bean import`)".into());
            }
            (Some(z), Some(v)) if z.available == Some(false) => {
                line.note = Some(format!("{} is sold out", z.label));
                let _ = v;
            }
            (Some(z), Some(v)) => {
                line.in_link = true;
                parts.push(format!("{v}:{link_qty}"));
                let each = if shop.by_the_pound { link_qty } else { qty };
                if let Some(p) = z.price {
                    total += p * each;
                }
                currency = currency.or(z.currency.clone()).or(w.currency.clone());
                if let (Some(now), Some(seen)) = (z.price, w.price) {
                    if (now - seen).abs() >= 0.005 && !shop.by_the_pound {
                        line.note = Some(format!("{seen:.2} when listed, {now:.2} now"));
                    }
                }
            }
        }
        lines.push(line);
    }
    let stale: Vec<&str> = lines
        .iter()
        .filter(|l| !l.checked)
        .map(|l| l.name.as_str())
        .collect();
    if !stale.is_empty() && stale.len() == lines.len() {
        warnings.push("Not read from the store again: bags and prices as the list saw them".into());
    } else if !stale.is_empty() {
        warnings.push(format!(
            "Not read from the store again, as the list saw them: {}",
            stale.join(", ")
        ));
    }
    let (how, url) = if parts.is_empty() {
        (
            if shop.woo {
                "checkout_link"
            } else {
                "permalink"
            },
            None,
        )
    } else if shop.woo {
        (
            "checkout_link",
            Some(format!(
                "https://{}/checkout-link/?products={}",
                shop.host,
                parts.join(",")
            )),
        )
    } else {
        match endpoint {
            Some(e) => (
                "permalink",
                Some(format!(
                    "{}/{}?continue_to=/cart",
                    e.trim_end_matches('/'),
                    parts.join(",")
                )),
            ),
            None => (
                "cart",
                Some(format!("https://{}/cart/{}", shop.host, parts.join(","))),
            ),
        }
    };
    if url.is_some() && !shop.woo {
        warnings.push(format!(
            "Opening it replaces the cart at {} in that browser: add anything already there again",
            shop.supplier
        ));
    }
    if url.as_ref().is_some_and(|u| u.len() > MAX_LINK) {
        warnings.push(format!(
            "The link is longer than {MAX_LINK} characters: the store may cut it; make two carts (--supplier and fewer entries)"
        ));
    }
    Cart {
        supplier: shop.supplier.to_string(),
        host: shop.host.clone(),
        how: how.into(),
        url,
        lines,
        total: crate::coffee::costs::cents(total),
        currency,
        warnings,
    }
}

/// The carts for the shopping list (or part of it), each line read again first.
pub async fn carts(wanted: &WantedService, importer: &Importer, req: CartRequest) -> Result<Carts> {
    let mut left_out = Vec::new();
    let mut groups: BTreeMap<String, (CartShop, Vec<Wanted>)> = BTreeMap::new();
    let want_supplier = req.supplier.as_deref().map(supplier_key);
    for w in wanted.list(false).await? {
        if w.status != "wanted" {
            continue;
        }
        if req.ids.as_ref().is_some_and(|ids| !ids.contains(&w.id)) {
            continue;
        }
        let Some(url) = w.url.clone() else {
            if want_supplier.is_none() {
                left_out.push(LeftOut {
                    wanted_id: w.id.clone(),
                    name: w.name.clone(),
                    why: "no link to the supplier's page".into(),
                });
            }
            continue;
        };
        let shop = match crate::coffee::import::cart_shop(&url) {
            Ok(s) => s,
            Err(e) => {
                left_out.push(LeftOut {
                    wanted_id: w.id.clone(),
                    name: w.name.clone(),
                    why: e.to_string(),
                });
                continue;
            }
        };
        if want_supplier.as_ref().is_some_and(|k| {
            *k != supplier_key(shop.supplier)
                && Some(k.clone()) != w.supplier.as_deref().map(supplier_key)
        }) {
            continue;
        }
        groups
            .entry(shop.host.clone())
            .or_insert_with(|| (shop, Vec::new()))
            .1
            .push(w);
    }
    let mut out = Vec::new();
    let mut first = true;
    for (_, (shop, entries)) in groups {
        let mut pause = || {
            let wait = !first;
            first = false;
            wait
        };
        let endpoint = if shop.woo {
            None
        } else if req.offline {
            importer.cached_permalink(&shop.host)
        } else {
            if pause() {
                tokio::time::sleep(PAUSE).await;
            }
            importer.permalink(&shop.host).await
        };
        let mut read = Vec::new();
        // Some(the wait the store asked for) once it answered 429
        let mut slowed: Option<Option<u64>> = None;
        for w in &entries {
            let stored = w.draft.as_deref().and_then(|d| bag(d, w)).cloned();
            if req.offline || slowed.is_some() {
                read.push((w, stored, false));
                continue;
            }
            if pause() {
                tokio::time::sleep(PAUSE).await;
            }
            match importer.sizes(w.url.as_deref().unwrap_or_default()).await {
                Ok(d) => {
                    let z = bag(&d, w).cloned();
                    read.push((w, z, true));
                }
                Err(e) => {
                    // A 429: stop asking this store; the rest go as the list saw them
                    if let Some(b) = store_busy(&e).filter(|b| b.throttled()) {
                        slowed = Some(b.retry_after_s);
                    }
                    read.push((w, stored, false));
                }
            }
        }
        let mut cart = build(&shop, endpoint.as_deref(), &read);
        if let Some(wait) = slowed {
            let when = wait.map_or_else(|| "in a while".to_string(), |s| format!("in {s} s"));
            cart.warnings.insert(
                0,
                format!(
                    "{} asked to slow down (429): try again {when}",
                    shop.supplier
                ),
            );
        }
        out.push(cart);
    }
    Ok(Carts {
        carts: out,
        left_out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coffee::import::{cart_shop, permalink_endpoint, shopify, woo};

    fn entry(id: &str, d: &Draft, size: &str, qty: i64, price: Option<f64>) -> Wanted {
        Wanted {
            id: id.into(),
            name: d.name.clone(),
            supplier: d.supplier.clone(),
            url: Some(d.url.clone()),
            bean_id: None,
            size_label: Some(size.into()),
            weight_g: d
                .sizes
                .iter()
                .find(|z| z.label == size)
                .and_then(|z| z.weight_g),
            price,
            currency: None,
            notes: None,
            quantity: qty,
            draft: Some(sqlx::types::Json(d.clone())),
            status: "wanted".into(),
            move_id: None,
            order_line_id: None,
            bought_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            bean_name: None,
        }
    }

    #[test]
    fn the_permalink_is_the_profiles_on_the_same_store() {
        let p = include_str!("../../testdata/suppliers/ucp-profile.json");
        assert_eq!(
            permalink_endpoint(p, "www.sweetmarias.com").as_deref(),
            Some("https://www.sweetmarias.com/buy")
        );
        // Another store's profile pointing at Sweet Maria's is not believed
        assert_eq!(permalink_endpoint(p, "bodhileafcoffee.com"), None);
        assert_eq!(permalink_endpoint("{}", "www.sweetmarias.com"), None);
        let evil = p.replace(
            "https://www.sweetmarias.com/buy",
            "https://evil.example/buy",
        );
        assert_eq!(permalink_endpoint(&evil, "www.sweetmarias.com"), None);
    }

    #[test]
    fn a_shopify_cart_links_each_bag_and_quantity_and_leaves_out_the_sold_out() {
        let d = shopify(
            include_str!("../../testdata/suppliers/sweetmarias-8647.js"),
            "Sweet Maria's",
            "https://www.sweetmarias.com/products/ethiopia-dry-process-kercha-haruse-8647",
        )
        .unwrap();
        let sold = shopify(
            include_str!("../../testdata/suppliers/sweetmarias-soldout.js"),
            "Sweet Maria's",
            "https://www.sweetmarias.com/products/sold-out",
        )
        .unwrap();
        let one = &d.sizes[0];
        let two = &d.sizes[1];
        let a = entry("w1", &d, &one.label, 2, one.price.map(|p| p - 1.0));
        let b = entry("w2", &d, &two.label, 1, two.price);
        let c = entry("w3", &sold, &sold.sizes[0].label, 1, None);
        let shop = cart_shop(&d.url).unwrap();
        let cart = build(
            &shop,
            Some("https://www.sweetmarias.com/buy"),
            &[
                (&a, Some(one.clone()), true),
                (&b, Some(two.clone()), true),
                (&c, Some(sold.sizes[0].clone()), true),
            ],
        );
        let v1 = one.variant_id.clone().unwrap();
        let v2 = two.variant_id.clone().unwrap();
        assert_eq!(
            cart.url.as_deref(),
            Some(
                format!("https://www.sweetmarias.com/buy/{v1}:2,{v2}:1?continue_to=/cart").as_str()
            )
        );
        assert_eq!(cart.how, "permalink");
        assert_eq!(
            cart.total,
            crate::coffee::costs::cents(one.price.unwrap() * 2.0 + two.price.unwrap())
        );
        assert!(!cart.lines[2].in_link);
        assert!(cart.lines[2].note.as_deref().unwrap().contains("sold out"));
        // The price moved since it was listed
        assert!(cart.lines[0].note.as_deref().unwrap().contains("now"));
        assert!(cart
            .warnings
            .iter()
            .any(|w| w.contains("replaces the cart")));

        // No permalink in the profile: Shopify's older /cart/ link
        let cart = build(&shop, None, &[(&b, Some(two.clone()), false)]);
        assert_eq!(cart.how, "cart");
        assert_eq!(
            cart.url.as_deref(),
            Some(format!("https://www.sweetmarias.com/cart/{v2}:1").as_str())
        );
        assert!(cart
            .warnings
            .iter()
            .any(|w| w.contains("Not read from the store again")));
    }

    #[test]
    fn burman_takes_pounds_in_a_checkout_link() {
        let d = woo(
            include_str!("../../testdata/suppliers/burman-ethiopian-aricha.json"),
            "Burman Coffee Traders",
            "https://burmancoffee.com/product/green-coffee-beans/ethiopian-aricha-washed/",
        )
        .unwrap();
        let mut w = entry("w1", &d, "1 lb", 1, d.sizes[0].price);
        w.weight_g = Some(5.0 * 453.6);
        let shop = cart_shop(&d.url).unwrap();
        assert!(shop.woo);
        let cart = build(&shop, None, &[(&w, Some(d.sizes[0].clone()), true)]);
        let id = d.product_id.clone().unwrap();
        assert_eq!(
            cart.url.as_deref(),
            Some(format!("https://burmancoffee.com/checkout-link/?products={id}:5").as_str())
        );
        assert_eq!(cart.lines[0].quantity, 5.0);
        assert!(!cart.warnings.iter().any(|w| w.contains("replaces")));
    }
}
