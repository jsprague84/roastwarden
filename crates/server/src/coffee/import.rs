//! Read one green coffee from a supplier's product link (`docs/green-coffee-sources.md`).
//!
//! The import only previews: it returns a [`Draft`] and saves nothing. The owner or an agent
//! then saves it as a bean, a purchase or a shopping-list entry. The server fetches only
//! `https` pages on the known supplier hosts below (this is a fetch from the server, so no
//! other address may be reached), one product per request, with a timeout, a size cap and an
//! honest User-Agent; a redirect must stay on the same host.
//!
//! A read costs the store as little as possible, for every adapter alike: a page read is kept
//! ten minutes (an agent's retry, or the cart right after an import, reads nothing again), a
//! store that answered 429 or 5xx is not asked a second endpoint for the same product, and a
//! 429's `Retry-After` goes into the error, so the caller waits what the store asked.
//!
//! Adapters, by platform: Shopify's product JSON with a "Label: value" reader for the
//! description, Sweet Maria's specs table on top of it, and WooCommerce's store API with its
//! named attributes (Burman). Each is tested against a synthetic answer shaped after a real
//! page (`testdata/suppliers/`: the supplier's keys and markup, invented prose).

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Bad input or a page this cannot read: the handler answers 400 with the reason.
#[derive(Debug)]
pub struct ImportError(pub String);

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ImportError {}

fn refuse(msg: impl Into<String>) -> anyhow::Error {
    ImportError(msg.into()).into()
}

/// The store answered 429 or 5xx: the store's, not the link's. The handler answers 502 and
/// the caller tries again later; a 429's `Retry-After` says how much later.
#[derive(Debug)]
pub struct StoreBusy {
    pub status: reqwest::StatusCode,
    /// Seconds the store asked to wait (`Retry-After`, as seconds or a date)
    pub retry_after_s: Option<u64>,
    url: String,
}

impl StoreBusy {
    /// 429: the store asked to slow down (a 5xx is the store down)
    pub fn throttled(&self) -> bool {
        self.status == reqwest::StatusCode::TOO_MANY_REQUESTS
    }
}

impl std::fmt::Display for StoreBusy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} answered {}: the store is busy",
            self.url, self.status
        )?;
        match self.retry_after_s {
            Some(s) => write!(f, "; it asked to wait {s} s before the next request"),
            None => f.write_str(" or down, try again in a few minutes"),
        }
    }
}

impl std::error::Error for StoreBusy {}

/// The store's own trouble behind an error, if that is what it was.
pub fn store_busy(e: &anyhow::Error) -> Option<&StoreBusy> {
    e.downcast_ref::<StoreBusy>()
}

/// `Retry-After`: seconds, or an HTTP date (seconds from now, 0 when it has passed).
fn retry_after_s(h: Option<&reqwest::header::HeaderValue>) -> Option<u64> {
    let v = h?.to_str().ok()?.trim();
    if let Ok(s) = v.parse::<u64>() {
        return Some(s);
    }
    let at = chrono::DateTime::parse_from_rfc2822(v).ok()?;
    let left = (at.with_timezone(&chrono::Utc) - chrono::Utc::now()).num_seconds();
    Some(u64::try_from(left).unwrap_or(0))
}

/// What one bag size costs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Size {
    /// As the supplier names it: "5 LB"
    pub label: String,
    /// Read from the label (kg, lb, oz, g), else the shop's weight
    pub weight_g: Option<f64>,
    pub price: Option<f64>,
    pub currency: Option<String>,
    pub sku: Option<String>,
    /// The shop says it can be bought now (absent: it does not say)
    pub available: Option<bool>,
    /// The shop's own id for this bag (a Shopify variant, a WooCommerce product): what a
    /// cart link takes (`docs/green-coffee.md`, "Cart links")
    #[serde(default)]
    pub variant_id: Option<String>,
}

/// A coffee as the supplier describes it, ready to save as a bean (the fields match
/// `BeanRequest`) with its bag sizes and prices.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Draft {
    pub name: String,
    pub supplier: Option<String>,
    pub origin: Option<String>,
    pub region: Option<String>,
    pub producer: Option<String>,
    pub variety: Option<String>,
    pub process: Option<String>,
    pub drying: Option<String>,
    pub altitude: Option<String>,
    pub grade: Option<String>,
    pub screen_size: Option<String>,
    pub certifications: Option<String>,
    pub score: Option<f32>,
    pub roast_recommendation: Option<String>,
    pub cupping_notes: Option<String>,
    pub lot: Option<String>,
    /// When the lot landed at the supplier, as it says
    pub arrival: Option<String>,
    /// The offer
    pub url: String,
    /// The shop's own id for the product
    #[serde(default)]
    pub product_id: Option<String>,
    pub sku: Option<String>,
    /// What the supplier says that has no field of its own
    pub details: BTreeMap<String, String>,
    pub sizes: Vec<Size>,
}

/// The suppliers whose pages are read, by host (with or without `www.`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shop {
    /// Shopify, with Sweet Maria's specs table on top
    SweetMarias,
    /// Shopify: product JSON and a "Label: value" description
    Shopify(&'static str),
    /// WooCommerce store API (Burman)
    Woo(&'static str),
}

/// Other hosts of a known shop, each named on purpose (a store moved to a subdomain); the
/// fetch rules do not change: the page is read from the host in the link.
const ALIASES: &[(&str, &str)] = &[("shop.thetaridgecoffee.com", "thetaridgecoffee.com")];

const SHOPIFY: &[(&str, &str)] = &[
    ("bodhileafcoffee.com", "Bodhi Leaf"),
    ("thetaridgecoffee.com", "Theta Ridge"),
    ("coffeebeancorral.com", "Coffee Bean Corral"),
    ("showroomcoffee.com", "Showroom Coffee"),
    ("happymugcoffee.com", "Happy Mug"),
];

fn shop_for(host: &str) -> Option<Shop> {
    let h = host.trim_start_matches("www.");
    let h = ALIASES
        .iter()
        .find(|(a, _)| *a == h)
        .map_or(h, |(_, main)| *main);
    if h == "sweetmarias.com" {
        return Some(Shop::SweetMarias);
    }
    if h == "burmancoffee.com" {
        return Some(Shop::Woo("Burman Coffee Traders"));
    }
    SHOPIFY
        .iter()
        .find(|(d, _)| *d == h)
        .map(|(_, name)| Shop::Shopify(name))
}

/// The hosts the import reads, for messages and docs.
pub fn known_hosts() -> Vec<&'static str> {
    let mut v = vec!["sweetmarias.com", "burmancoffee.com"];
    v.extend(SHOPIFY.iter().map(|(d, _)| *d));
    v.extend(ALIASES.iter().map(|(a, _)| *a));
    v
}

/// A checked link: https, a known host, the product's handle or slug.
#[derive(Debug, Clone)]
struct Link {
    shop: Shop,
    host: String,
    handle: String,
    url: String,
}

fn parse_link(raw: &str) -> Result<Link> {
    let u = reqwest::Url::parse(raw.trim()).map_err(|_| refuse("that is not a link"))?;
    if u.scheme() != "https" {
        return Err(refuse("only https links are read"));
    }
    let host = u.host_str().unwrap_or_default().to_ascii_lowercase();
    let shop = shop_for(&host).ok_or_else(|| {
        refuse(format!(
            "{host} is not a supplier this reads; known: {}",
            known_hosts().join(", ")
        ))
    })?;
    if u.port().is_some() || !u.username().is_empty() {
        return Err(refuse("a supplier link has no port or user name"));
    }
    let segs: Vec<&str> = u
        .path_segments()
        .map(std::iter::Iterator::collect)
        .unwrap_or_default();
    let handle = match shop {
        // …/products/<handle>, also under /collections/<c>/products/<handle>
        Shop::SweetMarias | Shop::Shopify(_) => segs
            .iter()
            .position(|s| *s == "products")
            .and_then(|i| segs.get(i + 1))
            .map(|h| h.trim_end_matches(".json").to_string()),
        // …/product/<category>/<slug>/
        Shop::Woo(_) => segs
            .iter()
            .rev()
            .find(|s| !s.is_empty())
            .filter(|_| segs.contains(&"product"))
            .map(std::string::ToString::to_string),
    }
    .filter(|h| {
        !h.is_empty()
            && h.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    })
    .ok_or_else(|| refuse("that link is not one product's page"))?;
    let mut clean = u.clone();
    clean.set_query(None);
    clean.set_fragment(None);
    Ok(Link {
        shop,
        host,
        handle,
        url: clean.to_string(),
    })
}

/// A store a cart link can be built for (`cart.rs`): its host as in the link, its name, and
/// how it takes a cart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CartShop {
    pub host: String,
    pub supplier: &'static str,
    /// WooCommerce's `/checkout-link/` (Burman); else Shopify
    pub woo: bool,
    /// The quantity is pounds (Burman's coffees are simple products sold by the pound)
    pub by_the_pound: bool,
}

/// The store behind a product link, for its cart.
pub fn cart_shop(url: &str) -> Result<CartShop> {
    let l = parse_link(url)?;
    let (supplier, woo) = match l.shop {
        Shop::SweetMarias => ("Sweet Maria's", false),
        Shop::Shopify(name) => (name, false),
        Shop::Woo(name) => (name, true),
    };
    Ok(CartShop {
        host: l.host,
        supplier,
        woo,
        by_the_pound: woo,
    })
}

/// The cart permalink a store's UCP profile advertises (`/.well-known/ucp`:
/// `ucp.capabilities["dev.ucp.shopping.permalink"][].config.endpoint`), if it is https on the
/// same store: "<https://www.sweetmarias.com/buy>".
pub fn permalink_endpoint(profile: &str, host: &str) -> Option<String> {
    let v: Value = serde_json::from_str(profile).ok()?;
    let ours = shop_for(host)?;
    v["ucp"]["capabilities"]["dev.ucp.shopping.permalink"]
        .as_array()?
        .iter()
        .filter_map(|c| c["config"]["endpoint"].as_str())
        .find_map(|e| {
            let u = reqwest::Url::parse(e).ok()?;
            let h = u.host_str()?.to_ascii_lowercase();
            (u.scheme() == "https"
                && u.port().is_none()
                && u.query().is_none()
                && shop_for(&h) == Some(ours))
            .then(|| e.trim_end_matches('/').to_string())
        })
}

/// A store's permalink endpoint, read once a day.
const PROFILE_FOR: Duration = Duration::from_hours(24);

/// host → (when read, the permalink endpoint)
type Profiles = std::sync::Arc<
    std::sync::Mutex<std::collections::HashMap<String, (std::time::Instant, Option<String>)>>,
>;

/// A page read is answered from memory this long: an agent's retry after a refusal, or the
/// cart right after an import, costs the store nothing. Prices move slower than that.
const KEEP_FOR: Duration = Duration::from_mins(10);
/// Pages kept at most (the shopping list's worth; a page is under a megabyte)
const MAX_PAGES: usize = 32;

/// url → (when read, the body)
type Pages = std::sync::Arc<
    std::sync::Mutex<std::collections::HashMap<String, (std::time::Instant, String)>>,
>;

// ------------------------------------------------------------------ fetching --

const USER_AGENT: &str = concat!(
    "Roastwarden/",
    env!("CARGO_PKG_VERSION"),
    " (green coffee import; one product page when its owner asks)"
);
const MAX_BYTES: usize = 4 << 20;

#[derive(Clone)]
pub struct Importer {
    http: reqwest::Client,
    profiles: Profiles,
    pages: Pages,
}

impl Default for Importer {
    fn default() -> Self {
        Self::new()
    }
}

impl Importer {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(15))
            // A redirect may only stay on the host it started from, over https.
            .redirect(reqwest::redirect::Policy::custom(|a| {
                let same = a
                    .previous()
                    .first()
                    .and_then(|u| u.host_str().map(str::to_string));
                if a.previous().len() > 5 {
                    a.error("too many redirects")
                } else if a.url().scheme() == "https"
                    && a.url().host_str().map(str::to_string) == same
                {
                    a.follow()
                } else {
                    a.stop()
                }
            }))
            .build()
            .expect("reqwest client");
        Self {
            http,
            profiles: Profiles::default(),
            pages: Pages::default(),
        }
    }

    /// The page as it was read within [`KEEP_FOR`], if it was.
    fn kept(&self, url: &str) -> Option<String> {
        let m = self.pages.lock().ok()?;
        m.get(url)
            .filter(|(at, _)| at.elapsed() < KEEP_FOR)
            .map(|(_, body)| body.clone())
    }

    /// Keep a page just read; the stale ones go, and the oldest when there are too many.
    fn keep(&self, url: &str, body: &str) {
        let Ok(mut m) = self.pages.lock() else { return };
        m.retain(|_, (at, _)| at.elapsed() < KEEP_FOR);
        while m.len() >= MAX_PAGES {
            let Some(oldest) = m
                .iter()
                .min_by_key(|(_, (at, _))| *at)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            m.remove(&oldest);
        }
        m.insert(
            url.to_string(),
            (std::time::Instant::now(), body.to_string()),
        );
    }

    /// The permalink endpoint if the profile was read today: no request.
    pub fn cached_permalink(&self, host: &str) -> Option<String> {
        let m = self.profiles.lock().ok()?;
        m.get(host)
            .filter(|(at, _)| at.elapsed() < PROFILE_FOR)
            .and_then(|(_, e)| e.clone())
    }

    /// A store's cart permalink endpoint from its UCP profile (cached a day; none: the
    /// store's `/cart/` link). A 429 is not remembered.
    pub async fn permalink(&self, host: &str) -> Option<String> {
        if let Some((at, e)) = self.profiles.lock().ok()?.get(host)
            && at.elapsed() < PROFILE_FOR {
                return e.clone();
            }
        let e = match self.get(&format!("https://{host}/.well-known/ucp")).await {
            Ok(p) => permalink_endpoint(&p, host),
            Err(e) if store_busy(&e).is_some_and(StoreBusy::throttled) => return None,
            Err(_) => None,
        };
        if let Ok(mut m) = self.profiles.lock() {
            m.insert(host.to_string(), (std::time::Instant::now(), e.clone()));
        }
        e
    }

    /// A product's bags as the store has them now, for a cart: prices, availability and each
    /// bag's id (one request; Sweet Maria's page with its specs is not read again).
    pub async fn sizes(&self, url: &str) -> Result<Draft> {
        let link = parse_link(url)?;
        match link.shop {
            Shop::SweetMarias => {
                let base = format!("https://{}", link.host);
                let json = self.shopify_product(&base, &link.handle).await?;
                shopify(&json, "Sweet Maria's", &link.url)
            }
            Shop::Shopify(_) | Shop::Woo(_) => self.draft(url).await,
        }
    }

    /// One page, over the network or from the pages kept ([`KEEP_FOR`]).
    async fn get(&self, url: &str) -> Result<String> {
        if let Some(body) = self.kept(url) {
            return Ok(body);
        }
        let mut r = self
            .http
            .get(url)
            .send()
            .await
            .with_context(|| format!("could not reach {url}"))?;
        let status = r.status();
        // Busy or down is the store's, not the link's: a 502 to try again later, not a 400
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
            return Err(StoreBusy {
                status,
                retry_after_s: retry_after_s(r.headers().get(reqwest::header::RETRY_AFTER)),
                url: url.to_string(),
            }
            .into());
        }
        if !status.is_success() {
            bail!(refuse(format!("{url} answered {status}")));
        }
        let mut body = Vec::new();
        while let Some(chunk) = r.chunk().await? {
            body.extend_from_slice(&chunk);
            if body.len() > MAX_BYTES {
                bail!(refuse(format!(
                    "{url} is larger than {} MB",
                    MAX_BYTES >> 20
                )));
            }
        }
        let body = String::from_utf8_lossy(&body).into_owned();
        self.keep(url, &body);
        Ok(body)
    }

    /// A Shopify product: the theme's `/products/<handle>.js` (it says which bags can be
    /// bought; the `.json` does not), else `/products/<handle>.json`. One request when the
    /// first answers; the second only when the theme has no `.js` (a 404, a page instead of
    /// JSON), never after a 429 or a 5xx, when a second hit would cost the store the same
    /// answer again, nor when it could not be reached.
    async fn shopify_product(&self, base: &str, handle: &str) -> Result<String> {
        match self.get(&format!("{base}/products/{handle}.js")).await {
            Ok(js) if js.trim_start().starts_with('{') => Ok(js),
            Ok(_) => self.get(&format!("{base}/products/{handle}.json")).await,
            Err(e) if not_readable(&e).is_some() => {
                self.get(&format!("{base}/products/{handle}.json")).await
            }
            Err(e) => Err(e),
        }
    }

    /// Read the product behind `url`: a preview, nothing is saved.
    pub async fn draft(&self, url: &str) -> Result<Draft> {
        let link = parse_link(url)?;
        let base = format!("https://{}", link.host);
        match link.shop {
            Shop::SweetMarias => {
                let json = self.shopify_product(&base, &link.handle).await?;
                let page = self
                    .get(&format!("{base}/products/{}", link.handle))
                    .await?;
                sweet_marias(&json, &page, &link.url)
            }
            Shop::Shopify(name) => {
                let json = self.shopify_product(&base, &link.handle).await?;
                shopify(&json, name, &link.url)
            }
            Shop::Woo(name) => {
                let json = self
                    .get(&format!(
                        "{base}/wp-json/wc/store/v1/products?slug={}",
                        link.handle
                    ))
                    .await?;
                woo(&json, name, &link.url)
            }
        }
    }
}

// ------------------------------------------------------------------- reading --

/// Text from HTML: line breaks at block ends, tags dropped, entities decoded, blank lines
/// collapsed.
pub fn html_text(html: &str) -> String {
    let mut s = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find('<') {
        s.push_str(&rest[..i]);
        let Some(j) = rest[i..].find('>') else {
            rest = "";
            break;
        };
        let tag = rest[i + 1..i + j]
            .trim_start_matches('/')
            .to_ascii_lowercase();
        let name: String = tag
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect();
        if matches!(
            name.as_str(),
            "br" | "p"
                | "div"
                | "li"
                | "tr"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "td"
                | "th"
        ) {
            s.push('\n');
        }
        rest = &rest[i + j + 1..];
    }
    s.push_str(rest);
    let decoded = decode_entities(&s);
    decoded
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let end = after.find(';').filter(|e| *e <= 10);
        let Some(e) = end else {
            out.push('&');
            rest = after;
            continue;
        };
        let ent = &after[..e];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "nbsp" => Some(' '),
            "ndash" => Some('–'),
            "mdash" => Some('—'),
            "apos" | "rsquo" | "lsquo" => Some('\''),
            "quot" | "rdquo" | "ldquo" => Some('"'),
            _ if ent.starts_with("#x") || ent.starts_with("#X") => {
                u32::from_str_radix(&ent[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
            }
            _ if ent.starts_with('#') => ent[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => out.push(c),
            None => {
                out.push('&');
                out.push_str(ent);
                out.push(';');
            }
        }
        rest = &after[e + 1..];
    }
    out.push_str(rest);
    out
}

/// "Label: value" lines, labels lower case without the colon; the first of a label wins.
fn label_lines(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let k = k.trim().to_lowercase();
        let v = v.trim();
        if k.is_empty() || k.len() > 30 || v.is_empty() || k.split_whitespace().count() > 4 {
            continue;
        }
        if !out.iter().any(|(o, _)| *o == k) {
            out.push((k, v.to_string()));
        }
    }
    out
}

/// Put a labelled value into the draft's field for it; anything else goes to `details`.
fn assign(d: &mut Draft, label: &str, value: &str) {
    let v = value.trim();
    if v.is_empty() {
        return;
    }
    let set = |f: &mut Option<String>| {
        if f.is_none() {
            *f = Some(v.to_string());
        }
    };
    match label
        .trim()
        .trim_end_matches(':')
        .trim()
        .to_lowercase()
        .as_str()
    {
        "country" | "origin" | "country of origin" => set(&mut d.origin),
        "region" | "sub region" | "subregion" => set(&mut d.region),
        "varietal" | "varietals" | "variety" | "varieties" | "cultivar detail" | "cultivar" => {
            set(&mut d.variety);
        }
        "process" | "processing" | "process method" | "processing method" => set(&mut d.process),
        "drying method" | "drying" => set(&mut d.drying),
        "altitude" | "elevation" | "growing altitude" => set(&mut d.altitude),
        "grade" => set(&mut d.grade),
        "screen" | "screen size" | "appearance" => set(&mut d.screen_size),
        "farm" | "producer" | "grower" | "station" | "washing station" | "cooperative" => {
            set(&mut d.producer);
        }
        "cupping notes" | "tasting notes" | "flavor notes" | "notes" => set(&mut d.cupping_notes),
        "recommended roast" | "roast recommendations" | "roast recommendation" | "roast" => {
            set(&mut d.roast_recommendation);
        }
        "arrival date" | "arrival" | "arrived" => set(&mut d.arrival),
        "lot" | "lot #" | "lot number" => set(&mut d.lot),
        "certification" | "certifications" => set(&mut d.certifications),
        "name" | "description" => {}
        other => {
            d.details
                .entry(titled(other))
                .or_insert_with(|| v.to_string());
        }
    }
}

fn titled(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Grams from a size label: "5 LB", "132 LBS / 60 KG" (kg wins), "3 pounds", "12 oz", "500g".
pub fn label_grams(label: &str) -> Option<f64> {
    let l = label.to_lowercase().replace(',', ".");
    let mut best: Option<(u8, f64)> = None;
    let words: Vec<&str> = l
        .split(|c: char| c.is_whitespace() || c == '/' || c == '(' || c == ')')
        .filter(|w| !w.is_empty())
        .collect();
    for (i, w) in words.iter().enumerate() {
        // "5lb" or "5 lb"
        let num_end = w
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(w.len());
        let (num, unit) = if num_end == w.len() {
            (
                w.parse::<f64>().ok(),
                words.get(i + 1).copied().unwrap_or(""),
            )
        } else {
            (w[..num_end].parse::<f64>().ok(), &w[num_end..])
        };
        let Some(n) = num else { continue };
        let (rank, g) = match unit.trim_end_matches('.') {
            "kg" | "kgs" | "kilo" | "kilos" | "kilogram" | "kilograms" => (3, n * 1000.0),
            "g" | "gram" | "grams" => (2, n),
            "lb" | "lbs" | "pound" | "pounds" => (1, n * 453.59237),
            "oz" | "ounce" | "ounces" => (0, n * 28.349_523_125),
            _ => continue,
        };
        if best.is_none_or(|(r, _)| rank > r) {
            best = Some((rank, g));
        }
    }
    best.map(|(_, g)| (g * 10.0).round() / 10.0)
}

fn s(v: &Value) -> Option<String> {
    v.as_str()
        .map(|t| decode_entities(t).trim().to_string())
        .filter(|t| !t.is_empty())
}

/// The product object of either Shopify answer, and whether it is the theme's `.js` (prices
/// in cents, `weight` in grams, `description`) rather than `.json` (`{"product": …}`, prices
/// as decimal strings, `grams`, `body_html`).
fn shopify_product_value(v: &Value) -> (&Value, bool) {
    match v.get("product") {
        Some(p) => (p, false),
        None => (v, true),
    }
}

/// An id as text, whether the shop sends a number or a string.
fn id_text(v: &Value) -> Option<String> {
    v.as_u64().map(|n| n.to_string()).or_else(|| s(v))
}

/// Shopify's `/products/<handle>.js` or `/products/<handle>.json`.
pub fn shopify(json: &str, supplier: &str, url: &str) -> Result<Draft> {
    let v: Value =
        serde_json::from_str(json).map_err(|_| refuse("the shop's answer is not product JSON"))?;
    let (p, theme) = shopify_product_value(&v);
    let title = s(&p["title"]).ok_or_else(|| refuse("the product has no title"))?;
    let mut d = Draft {
        name: clean_title(&title),
        supplier: Some(supplier.to_string()),
        url: url.to_string(),
        product_id: id_text(&p["id"]),
        ..Default::default()
    };
    let body = p["body_html"]
        .as_str()
        .or(p["description"].as_str())
        .unwrap_or_default();
    let text = html_text(body);
    for (k, val) in label_lines(&text) {
        assign(&mut d, &k, &val);
    }
    // Prose before the first labelled line is the description.
    if d.cupping_notes.is_none()
        && let Some(first) = text.lines().find(|l| !l.contains(':') && l.len() > 40) {
            d.cupping_notes = Some(first.to_string());
        }
    let tags: Vec<String> = match &p["tags"] {
        Value::String(t) => t.split(',').map(|x| x.trim().to_string()).collect(),
        Value::Array(a) => a.iter().filter_map(s).collect(),
        _ => vec![],
    };
    for t in &tags {
        let tl = t.to_lowercase();
        if d.certifications.is_none()
            && (tl == "organic" || tl.contains("fair trade") || tl == "fairtrade")
        {
            d.certifications = Some(titled(&tl));
        }
    }
    if d.origin.is_none() {
        // Bodhi Leaf tags the country by name.
        d.origin = tags
            .iter()
            .find(|t| {
                crate::coffee::picklists::COUNTRIES
                    .iter()
                    .any(|c| c.eq_ignore_ascii_case(t))
            })
            .cloned();
    }
    for var in p["variants"].as_array().into_iter().flatten() {
        let label = s(&var["title"]).unwrap_or_else(|| "Default".into());
        let shop_grams = var["grams"].as_f64().or(var["weight"].as_f64());
        let grams = label_grams(&label).or_else(|| shop_grams.filter(|g| *g > 0.0));
        let price = match &var["price"] {
            Value::String(x) => x.parse().ok(),
            // The theme's `.js` counts in cents.
            n if theme => n.as_f64().map(|c| c / 100.0),
            n => n.as_f64(),
        };
        d.sizes.push(Size {
            label,
            weight_g: grams,
            price,
            currency: s(&var["price_currency"]),
            sku: s(&var["sku"]),
            available: var["available"].as_bool(),
            variant_id: id_text(&var["id"]),
        });
    }
    d.sku = common_sku(&d.sizes);
    Ok(d)
}

/// What every bag's SKU shares ("GCX-8647-001", "GCX-8647-005" → "GCX-8647"): the coffee's,
/// not one bag size's.
fn common_sku(sizes: &[Size]) -> Option<String> {
    let skus: Vec<&str> = sizes.iter().filter_map(|z| z.sku.as_deref()).collect();
    let first = *skus.first()?;
    if skus.len() == 1 {
        return Some(first.to_string());
    }
    let mut n = first.len();
    for s in &skus[1..] {
        n = n.min(
            first
                .bytes()
                .zip(s.bytes())
                .take_while(|(a, b)| a == b)
                .count(),
        );
    }
    let p = first[..n].trim_end_matches(['-', '_', '/', '.']);
    (p.len() >= 3).then(|| p.to_string())
}

/// "Honduras Organic Copan - Green" → "Honduras Organic Copan".
fn clean_title(t: &str) -> String {
    let t = t.trim();
    for suffix in [
        " - Green",
        " - GREEN",
        " – Green",
        " (Green)",
        " Green Coffee Beans",
    ] {
        if let Some(x) = t.strip_suffix(suffix) {
            return x.trim().to_string();
        }
    }
    t.to_string()
}

/// Sweet Maria's: the Shopify JSON for name and sizes, the page's specs table and score.
pub fn sweet_marias(json: &str, page: &str, url: &str) -> Result<Draft> {
    let mut d = shopify(json, "Sweet Maria's", url)?;
    let v: Value = serde_json::from_str(json)?;
    // Country from the deepest category tag: "Category_Sweet Marias/Green Coffee/Africa/Ethiopia"
    // (`.json` sends the tags as one string, `.js` as a list).
    let (p, _) = shopify_product_value(&v);
    let tags: Vec<String> = match &p["tags"] {
        Value::String(t) => t.split(',').map(|x| x.trim().to_string()).collect(),
        Value::Array(a) => a.iter().filter_map(s).collect(),
        _ => vec![],
    };
    let deepest = tags
        .iter()
        .map(std::string::String::as_str)
        .filter(|t| t.starts_with("Category_Sweet Marias/Green Coffee/"))
        .max_by_key(|t| t.matches('/').count());
    if let Some(t) = deepest
        && t.matches('/').count() >= 3 {
            d.origin = t.rsplit('/').next().map(str::to_string);
        }
    let mut specs = specs_table(page);
    // "country origin" is the continent there; the tag has the country.
    specs.retain(|(k, _)| k != "country origin");
    for (k, val) in &specs {
        assign(&mut d, k, val);
    }
    if let Some(score) = between(page, "<span class=\"score\">", "</span>") {
        d.score = score.trim().parse().ok();
    }
    Ok(d)
}

/// The specs table's `th.col.label` / `td.col.data` pairs, first of each label.
fn specs_table(page: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut rest = page;
    while let Some(i) = rest.find("class=\"col label\"") {
        rest = &rest[i..];
        let Some(label) = between(rest, ">", "</th>") else {
            break;
        };
        let label = html_text(label).to_lowercase();
        let Some(td) = rest.find("class=\"col data\"") else {
            break;
        };
        rest = &rest[td..];
        let Some(value) = between(rest, ">", "</td>") else {
            break;
        };
        let value = html_text(value);
        if !label.is_empty() && !value.is_empty() && !out.iter().any(|(k, _)| *k == label) {
            out.push((label, value));
        }
    }
    out
}

fn between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let i = s.find(start)? + start.len();
    let j = s[i..].find(end)?;
    Some(&s[i..i + j])
}

/// WooCommerce's store API (`/wp-json/wc/store/v1/products?slug=…`): one product in a list.
pub fn woo(json: &str, supplier: &str, url: &str) -> Result<Draft> {
    let v: Value =
        serde_json::from_str(json).map_err(|_| refuse("the shop's answer is not product JSON"))?;
    let p = v
        .as_array()
        .and_then(|a| a.first())
        .ok_or_else(|| refuse("the shop has no such product"))?;
    let name = s(&p["name"]).ok_or_else(|| refuse("the product has no name"))?;
    let mut d = Draft {
        name,
        supplier: Some(supplier.to_string()),
        url: url.to_string(),
        product_id: id_text(&p["id"]),
        sku: s(&p["sku"]),
        ..Default::default()
    };
    for a in p["attributes"].as_array().into_iter().flatten() {
        let label = s(&a["name"]).unwrap_or_default();
        let val = a["terms"]
            .as_array()
            .map(|t| {
                t.iter()
                    .filter_map(|x| s(&x["name"]))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        // Burman's "Origin:" is the region; the country is a category.
        if label.trim_end_matches(':').eq_ignore_ascii_case("origin") {
            assign(&mut d, "region", &val);
        } else {
            assign(&mut d, &label, &val);
        }
    }
    let cats: Vec<String> = p["categories"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| s(&c["name"]))
        .collect();
    d.origin = cats
        .iter()
        .find(|c| {
            crate::coffee::picklists::COUNTRIES
                .iter()
                .any(|k| k.eq_ignore_ascii_case(c))
        })
        .cloned();
    let notes = html_text(p["short_description"].as_str().unwrap_or_default());
    d.cupping_notes = notes
        .lines()
        .filter(|l| l.len() > 40)
        .max_by_key(|l| l.len())
        .map(str::to_string);
    let prices = &p["prices"];
    let minor = prices["currency_minor_unit"].as_u64().unwrap_or(2) as i32;
    let price = prices["price"]
        .as_str()
        .and_then(|x| x.parse::<f64>().ok())
        .map(|x| x / 10f64.powi(minor));
    // Burman sells green coffee by the pound.
    d.sizes.push(Size {
        label: "1 lb".into(),
        weight_g: Some(453.6),
        price,
        currency: s(&prices["currency_code"]),
        sku: d.sku.clone(),
        available: p["is_in_stock"].as_bool(),
        // Burman's coffees are simple products: the product is what a cart takes, by the pound.
        variant_id: d.product_id.clone(),
    });
    Ok(d)
}

impl Draft {
    /// The size that matches a wanted weight (within 2 %), for "bought 5 lb".
    pub fn size_for(&self, grams: f64) -> Option<&Size> {
        self.sizes.iter().find(|z| {
            z.weight_g
                .is_some_and(|w| (w - grams).abs() <= grams * 0.02)
        })
    }
}

pub fn not_readable(e: &anyhow::Error) -> Option<&ImportError> {
    e.downcast_ref::<ImportError>()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SM_JSON: &str = include_str!("../../testdata/suppliers/sweetmarias-8647.json");
    const SM_HTML: &str = include_str!("../../testdata/suppliers/sweetmarias-8647.html");
    const BODHI: &str = include_str!("../../testdata/suppliers/bodhileaf-honduras-copan.json");
    const THETA: &str = include_str!("../../testdata/suppliers/thetaridge-ethiopian-harrar.json");
    const BURMAN: &str = include_str!("../../testdata/suppliers/burman-ethiopian-aricha.json");
    const SM_JS: &str = include_str!("../../testdata/suppliers/sweetmarias-8647.js");
    const SM_SOLD_OUT: &str = include_str!("../../testdata/suppliers/sweetmarias-soldout.js");

    #[test]
    fn the_theme_js_gives_the_same_draft_with_availability_and_ids() {
        let url = "https://www.sweetmarias.com/products/ethiopia-dry-process-kercha-haruse-8647";
        let js = sweet_marias(SM_JS, SM_HTML, url).unwrap();
        let json = sweet_marias(SM_JSON, SM_HTML, url).unwrap();
        assert_eq!(js.name, json.name);
        assert_eq!(js.origin.as_deref(), Some("Ethiopia"), "tags as a list");
        assert_eq!(js.region, json.region);
        assert_eq!(
            js.cupping_notes, json.cupping_notes,
            "description, not body_html"
        );
        assert_eq!(js.sku, json.sku);
        assert_eq!(js.product_id.as_deref(), Some("1000000000001"));
        let prices = |d: &Draft| d.sizes.iter().map(|z| z.price).collect::<Vec<_>>();
        assert_eq!(prices(&js), prices(&json), "cents become dollars");
        let weights = |d: &Draft| d.sizes.iter().map(|z| z.weight_g).collect::<Vec<_>>();
        assert_eq!(weights(&js), weights(&json));
        let five = js.size_for(5.0 * 453.59237).unwrap();
        assert_eq!(five.variant_id.as_deref(), Some("1000000000103"));
        assert_eq!(five.available, Some(true));
        assert_eq!(js.sizes.last().unwrap().available, Some(false));
        // The `.json` says nothing about availability, but has each bag's id and currency.
        assert!(json.sizes.iter().all(|z| z.available.is_none()));
        assert_eq!(json.sizes[0].variant_id.as_deref(), Some("1"));
        assert_eq!(json.sizes[0].currency.as_deref(), Some("USD"));
    }

    #[test]
    fn a_sold_out_coffee_shows_its_one_size_not_available() {
        let d = sweet_marias(
            SM_SOLD_OUT,
            "",
            "https://www.sweetmarias.com/products/burundi-dry-process-synthetic-hill-8999",
        )
        .unwrap();
        assert_eq!(d.origin.as_deref(), Some("Burundi"));
        assert_eq!(d.sizes.len(), 1);
        let z = &d.sizes[0];
        assert_eq!(
            (z.label.as_str(), z.weight_g, z.price, z.available),
            ("Default Title", Some(449.0), Some(8.45), Some(false))
        );
    }

    #[test]
    fn sweet_marias_reads_the_json_the_specs_table_and_the_score() {
        let url = "https://www.sweetmarias.com/products/ethiopia-dry-process-kercha-haruse-8647";
        let d = sweet_marias(SM_JSON, SM_HTML, url).unwrap();
        assert_eq!(d.name, "Ethiopia Dry Process Kercha Haruse");
        assert_eq!(d.supplier.as_deref(), Some("Sweet Maria's"));
        assert_eq!(d.origin.as_deref(), Some("Ethiopia"));
        assert_eq!(d.region.as_deref(), Some("Kercha, Guji Zone"));
        assert_eq!(d.process.as_deref(), Some("Dry Process (Natural)"));
        assert_eq!(d.drying.as_deref(), Some("Raised Bed Sun-Dried"));
        assert_eq!(d.arrival.as_deref(), Some("Jul 2026"));
        assert_eq!(d.grade.as_deref(), Some("Grade 1"));
        assert_eq!(d.variety.as_deref(), Some("Heirloom Varieties"));
        assert_eq!(d.roast_recommendation.as_deref(), Some("City to City+"));
        assert_eq!(d.score, Some(92.6));
        assert_eq!(d.details["Packaging"], "GrainPro Liner");
        assert!(d.cupping_notes.as_deref().unwrap().contains("citron"));
        let five = d.size_for(5.0 * 453.59237).unwrap();
        assert_eq!((five.label.as_str(), five.price), ("5 LB", Some(46.0)));
        assert_eq!(five.sku.as_deref(), Some("GCX-8647-005"));
        let big = d.sizes.last().unwrap();
        assert_eq!(big.weight_g, Some(60_000.0), "kg wins over lb in a label");
        assert_eq!(d.sizes.len(), 7);
        assert_eq!(d.sku.as_deref(), Some("GCX-8647"));
    }

    #[test]
    fn shopify_descriptions_are_read_as_labelled_lines() {
        let d = shopify(
            BODHI,
            "Bodhi Leaf",
            "https://bodhileafcoffee.com/products/honduras-copan-organic-green",
        )
        .unwrap();
        assert_eq!(d.name, "Honduras Organic Copan");
        assert_eq!(d.origin.as_deref(), Some("Honduras"));
        assert_eq!(d.region.as_deref(), Some("Copán, Corquín"));
        assert_eq!(d.process.as_deref(), Some("Washed"));
        assert_eq!(d.altitude.as_deref(), Some("1200-1700M"));
        assert_eq!(
            d.cupping_notes.as_deref(),
            Some("Nougat, Red Apple, Brownie")
        );
        assert_eq!(
            d.roast_recommendation.as_deref(),
            Some("City+ to Full City+")
        );
        assert_eq!(d.certifications.as_deref(), Some("Organic"));
        assert_eq!(
            d.details["Good for"],
            "Auto-drip, Full immersion, Espresso, Cold Brew"
        );
        assert_eq!(
            d.sizes.iter().map(|z| z.price).collect::<Vec<_>>(),
            vec![Some(8.86), Some(42.09), Some(155.94)]
        );

        let t = shopify(
            THETA,
            "Theta Ridge",
            "https://thetaridgecoffee.com/products/ethiopian-harrar",
        )
        .unwrap();
        assert_eq!(t.origin.as_deref(), Some("Ethiopia"));
        assert_eq!(t.process.as_deref(), Some("Natural"));
        assert_eq!(t.variety.as_deref(), Some("Heirloom"));
        assert_eq!(t.altitude.as_deref(), Some("1800 M"));
        assert_eq!(
            t.size_for(3.0 * 453.59237).map(|z| z.price),
            Some(Some(19.8))
        );
    }

    #[test]
    fn woocommerce_reads_named_attributes() {
        let d = woo(
            BURMAN,
            "Burman Coffee Traders",
            "https://burmancoffee.com/product/green-coffee-beans/ethiopian-aricha-washed/",
        )
        .unwrap();
        assert_eq!(
            d.name,
            "Ethiopian Yirgacheffe Washed Gr. 2 – Aricha Station"
        );
        assert_eq!(d.origin.as_deref(), Some("Ethiopia"));
        assert_eq!(
            d.region.as_deref(),
            Some("Aricha Kebele, Yirga Chefe Woreda, Gedeo Zone")
        );
        assert_eq!(d.process.as_deref(), Some("Washed"));
        assert_eq!(d.lot.as_deref(), Some("0072"));
        assert_eq!(d.arrival.as_deref(), Some("09/30/26"));
        assert_eq!(d.altitude.as_deref(), Some("1900 – 2200 masl"));
        assert!(d.producer.as_deref().unwrap().starts_with("Smallholder"));
        let lb = &d.sizes[0];
        assert_eq!(
            (lb.price, lb.currency.as_deref()),
            (Some(6.98), Some("USD"))
        );
        assert!(d.product_id.is_some());
        assert_eq!(
            lb.variant_id, d.product_id,
            "a simple product: the product is the bag"
        );
    }

    #[test]
    fn only_https_product_links_on_known_hosts_are_read() {
        let l = parse_link("https://www.sweetmarias.com/collections/green-coffee/products/ethiopia-dry-process-kercha-haruse-8647?variant=1#x").unwrap();
        assert_eq!(l.shop, Shop::SweetMarias);
        assert_eq!(l.handle, "ethiopia-dry-process-kercha-haruse-8647");
        assert!(!l.url.contains('?'));
        let b = parse_link(
            "https://burmancoffee.com/product/green-coffee-beans/ethiopian-aricha-washed/",
        )
        .unwrap();
        assert_eq!(b.handle, "ethiopian-aricha-washed");
        // Theta Ridge's store is on its own subdomain: a named alias, read from that host.
        let t = parse_link("https://shop.thetaridgecoffee.com/products/ethiopian-harrar").unwrap();
        assert_eq!(t.shop, Shop::Shopify("Theta Ridge"));
        assert_eq!(t.host, "shop.thetaridgecoffee.com");
        assert!(known_hosts().contains(&"shop.thetaridgecoffee.com"));
        for bad in [
            "http://www.sweetmarias.com/products/x",
            "https://example.com/products/x",
            "https://sweetmarias.com.evil.example/products/x",
            "https://evil.thetaridgecoffee.com/products/x",
            "https://shop.sweetmarias.com/products/x",
            "https://www.sweetmarias.com:8443/products/x",
            "https://user@www.sweetmarias.com/products/x",
            "https://www.sweetmarias.com/collections/green-coffee",
            "https://www.sweetmarias.com/products/../../admin",
            "file:///etc/passwd",
            "not a link",
        ] {
            let e = parse_link(bad).unwrap_err();
            assert!(not_readable(&e).is_some(), "{bad}: {e}");
        }
    }

    /// A store that answers each request in turn (status line, extra header lines, body) and
    /// counts them; one more request than answers is refused.
    async fn store(
        answers: &'static [(&'static str, &'static str, &'static str)],
    ) -> (String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = hits.clone();
        tokio::spawn(async move {
            for (code, headers, body) in answers {
                let (mut c, _) = l.accept().await.unwrap();
                counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let mut buf = [0u8; 1024];
                let _ = c.read(&mut buf).await;
                let _ = c
                    .write_all(
                        format!(
                            "HTTP/1.1 {code}\r\n{headers}content-length: {}\r\nconnection: close\r\n\r\n{body}",
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .await;
            }
        });
        (format!("http://{addr}"), hits)
    }

    fn hits(h: &std::sync::Arc<std::sync::atomic::AtomicUsize>) -> usize {
        h.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// A store that is busy (429) or down (5xx) is not a bad link: a 502 to try again later;
    /// a missing page (404) is the link's
    #[tokio::test]
    async fn a_busy_store_is_not_a_bad_link() {
        let (base, _) = store(&[
            ("429 Too Many Requests", "", ""),
            ("503 Service Unavailable", "", ""),
            ("404 Not Found", "", ""),
        ])
        .await;
        let i = Importer::new();
        let url = format!("{base}/products/x");
        for busy in [true, true, false] {
            let e = i.get(&url).await.unwrap_err();
            assert_eq!(not_readable(&e).is_none(), busy, "{e}");
            assert_eq!(store_busy(&e).is_some(), busy, "{e}");
        }
    }

    /// Under a 429 the store is asked once: no `.json` after the `.js`, and the wait it asked
    /// for goes into the error, so the caller waits that instead of guessing.
    #[tokio::test]
    async fn a_throttled_store_is_asked_once_and_its_wait_passed_on() {
        let (base, hits_) = store(&[
            ("429 Too Many Requests", "retry-after: 45\r\n", ""),
            ("200 OK", "", "{\"product\":{}}"),
        ])
        .await;
        let e = Importer::new()
            .shopify_product(&base, "x")
            .await
            .unwrap_err();
        let busy = store_busy(&e).expect("the store's trouble");
        assert!(busy.throttled());
        assert_eq!(busy.retry_after_s, Some(45));
        assert!(e.to_string().contains("asked to wait 45 s"), "{e}");
        assert_eq!(hits(&hits_), 1, "the .json was not asked for");

        // Down (5xx) is final too
        let (base, hits_) = store(&[
            ("502 Bad Gateway", "", ""),
            ("200 OK", "", "{\"product\":{}}"),
        ])
        .await;
        let e = Importer::new()
            .shopify_product(&base, "x")
            .await
            .unwrap_err();
        assert!(!store_busy(&e).expect("busy").throttled());
        assert_eq!(hits(&hits_), 1);
    }

    /// The `.json` is read when the theme has no `.js` (404) or answers a page instead of JSON
    #[tokio::test]
    async fn the_json_is_read_only_when_the_theme_has_no_js() {
        for first in [
            ("404 Not Found", "", ""),
            ("200 OK", "", "<html>a page</html>"),
        ] {
            let answers: &'static [_] = Box::leak(Box::new([
                first,
                ("200 OK", "", "{\"product\":{\"id\":1}}"),
            ]));
            let (base, hits_) = store(answers).await;
            let json = Importer::new().shopify_product(&base, "x").await.unwrap();
            assert_eq!(json, "{\"product\":{\"id\":1}}");
            assert_eq!(hits(&hits_), 2, "{}", first.0);
        }
    }

    /// A page read is kept: the same link again reads nothing from the store
    #[tokio::test]
    async fn a_page_read_is_kept_so_a_retry_costs_the_store_nothing() {
        let (base, hits_) = store(&[("200 OK", "", "{\"a\":1}")]).await;
        let i = Importer::new();
        let url = format!("{base}/products/x.js");
        assert_eq!(i.get(&url).await.unwrap(), "{\"a\":1}");
        assert_eq!(i.get(&url).await.unwrap(), "{\"a\":1}", "from memory");
        assert_eq!(hits(&hits_), 1);
        assert!(i.kept(&format!("{base}/products/y.js")).is_none());
    }

    #[test]
    fn retry_after_is_seconds_or_a_date() {
        let h = |v: &str| reqwest::header::HeaderValue::from_str(v).unwrap();
        assert_eq!(retry_after_s(Some(&h("45"))), Some(45));
        assert_eq!(retry_after_s(Some(&h(" 7 "))), Some(7));
        assert_eq!(retry_after_s(Some(&h("soon"))), None);
        assert_eq!(retry_after_s(None), None);
        let soon = (chrono::Utc::now() + chrono::Duration::seconds(90)).to_rfc2822();
        let s = retry_after_s(Some(&h(&soon))).unwrap();
        assert!((88..=90).contains(&s), "{s}");
        let past = (chrono::Utc::now() - chrono::Duration::seconds(90)).to_rfc2822();
        assert_eq!(retry_after_s(Some(&h(&past))), Some(0));
    }

    #[test]
    fn sizes_and_text() {
        assert_eq!(label_grams("5 LB"), Some(2268.0));
        assert_eq!(label_grams("132 LBS / 60 KG"), Some(60000.0));
        assert_eq!(label_grams("3 pounds"), Some(1360.8));
        assert_eq!(label_grams("12oz"), Some(340.2));
        assert_eq!(label_grams("Default Title"), None);
        assert_eq!(html_text("<p>A&amp;B<br>C &#8211; D</p>"), "A&B\nC – D");
    }
}
