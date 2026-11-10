//! Shared profiles (docs/profiles.md, "Sharing a profile"): a public link
//! for a profile and import from such a link. The roast share page (`labels/share.rs`,
//! `/R/<code>`) is the pattern: a 25-character code, a page the server makes with no script,
//! revocable, opens counted without addresses.
//!
//! - `/P/<code>`: the profile's page: its curve on chart paper, level and purpose, targets,
//!   the events it replays, and the two files below. Never its id, who made it, or its roasts.
//! - `/P/<code>/profile.json`: the profile as a [`SharedProfile`] (format
//!   `roastwarden.profile/1`), which `POST /api/profiles/import/artisan` and the import from
//!   a link read; `/P/<code>/profile.alog`: the same as an Artisan file.
//! - The import from a link fetches `https://host/P/<code>/profile.json` from the server, so
//!   it reaches public addresses only: https, the host resolved once and every address it
//!   gives public (no loopback, private, link-local or shared range), that address pinned for
//!   the request, no redirects, 10 s and 2 MB at most. A link to this server, or a bare code,
//!   is read from the database with no request.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::AssertSqlSafe;
use sqlx::SqlitePool;
use std::fmt::Write as _;
use utoipa::ToSchema;

use crate::labels::share::{self as rshare, Curve, PageBrand, esc};
use crate::profiles::types::{
    CreateProfileEventRequest, CreateProfilePointRequest, ProfileAction, ProfileCurves,
    ProfileWithPoints, TriggerKind,
};

/// The shared file's format: a reader refuses anything else
pub const FORMAT: &str = "roastwarden.profile/1";

/// A profile's share link
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct ProfileShareLink {
    pub code: String,
    /// The profile it shows
    pub profile_id: String,
    pub name: String,
    pub created_at: String,
    pub created_by: String,
    /// Set once revoked: the page and its files are then a 404 like an unknown code
    pub revoked_at: Option<String>,
    /// How often the page was opened
    pub views: i64,
    /// How often its files were downloaded (an import from a link is one)
    pub downloads: i64,
    pub last_used_at: Option<String>,
    /// The page's address
    #[sqlx(skip)]
    #[serde(default)]
    pub url: String,
}

const SELECT: &str = "SELECT l.code, l.profile_id, p.name, l.created_at, l.created_by,
        l.revoked_at, l.views, l.downloads, l.last_used_at
    FROM profile_share_links l
    JOIN roast_profiles p ON p.id = l.profile_id";

/// The page's address: `https://host/P/CODE`
pub fn url(base: &str, code: &str) -> String {
    format!("{}/P/{code}", base.trim_end_matches('/'))
}

fn now_text() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// The links, newest first: a profile's or every profile's, the revoked ones too with `all`
pub async fn list(
    db: &SqlitePool,
    profile: Option<&str>,
    all: bool,
) -> Result<Vec<ProfileShareLink>> {
    let sql = format!(
        "{SELECT} WHERE (?1 IS NULL OR l.profile_id = ?1) AND (?2 OR l.revoked_at IS NULL)
         ORDER BY l.created_at DESC, l.rowid DESC"
    );
    Ok(
        sqlx::query_as::<_, ProfileShareLink>(AssertSqlSafe(sql.as_str()))
            .bind(profile)
            .bind(all)
            .fetch_all(db)
            .await?,
    )
}

pub async fn get(db: &SqlitePool, code: &str) -> Result<Option<ProfileShareLink>> {
    let sql = format!("{SELECT} WHERE l.code = ?");
    Ok(
        sqlx::query_as::<_, ProfileShareLink>(AssertSqlSafe(sql.as_str()))
            .bind(code)
            .fetch_optional(db)
            .await?,
    )
}

/// The profile's newest link that is not revoked
pub async fn active(db: &SqlitePool, profile: &str) -> Result<Option<ProfileShareLink>> {
    Ok(list(db, Some(profile), false).await?.into_iter().next())
}

/// A new link for a profile (the caller has checked the profile exists)
pub async fn create(db: &SqlitePool, profile: &str, by: &str) -> Result<ProfileShareLink> {
    let code = rshare::new_code();
    sqlx::query(
        "INSERT INTO profile_share_links (code, profile_id, created_at, created_by)
         VALUES (?, ?, ?, ?)",
    )
    .bind(&code)
    .bind(profile)
    .bind(now_text())
    .bind(by)
    .execute(db)
    .await?;
    get(db, &code)
        .await?
        .ok_or_else(|| anyhow::anyhow!("the share link vanished"))
}

/// Revoke a link (again is fine); false when there is no such link
pub async fn revoke(db: &SqlitePool, code: &str) -> Result<bool> {
    sqlx::query(
        "UPDATE profile_share_links SET revoked_at = ? WHERE code = ? AND revoked_at IS NULL",
    )
    .bind(now_text())
    .bind(code)
    .execute(db)
    .await?;
    Ok(get(db, code).await?.is_some())
}

/// What a link was used for, counted
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Use {
    View,
    Download,
}

/// The link a page or file is asked for, counted; none when unknown or revoked
pub async fn used(db: &SqlitePool, code: &str, what: Use) -> Result<Option<ProfileShareLink>> {
    let column = match what {
        Use::View => "views",
        Use::Download => "downloads",
    };
    let n = sqlx::query(AssertSqlSafe(format!(
        "UPDATE profile_share_links SET {column} = {column} + 1, last_used_at = ?
         WHERE code = ? AND revoked_at IS NULL"
    )))
    .bind(now_text())
    .bind(code)
    .execute(db)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok(None);
    }
    get(db, code).await
}

// ---- the file ----

/// A profile as shared: what another server needs to roast it, nothing about this one (no
/// ids, no author, no roasts). The flat form `POST /api/profiles/import/artisan` reads.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SharedProfile {
    /// `roastwarden.profile/1`
    pub format: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub target_total_time: Option<i32>,
    #[serde(default)]
    pub target_first_crack: Option<i32>,
    #[serde(default)]
    pub target_end_temp: Option<f32>,
    #[serde(default)]
    pub preheat_temp: Option<f32>,
    #[serde(default)]
    pub charge_temp: Option<f32>,
    /// The coffee it was made for
    #[serde(default)]
    pub beans: Option<String>,
    #[serde(default)]
    pub batch_size_g: Option<f32>,
    /// The roaster it was made on, as its owner named it
    #[serde(default)]
    pub roaster: Option<String>,
    #[serde(default)]
    pub roast_level: Option<String>,
    #[serde(default)]
    pub roast_purpose: Option<String>,
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub expectations: Option<serde_json::Value>,
    #[serde(default)]
    pub auto_drop: Option<String>,
    #[serde(default)]
    pub roaster_kind: Option<String>,
    pub points: Vec<CreateProfilePointRequest>,
    #[serde(default)]
    pub events: Vec<CreateProfileEventRequest>,
    #[serde(default)]
    pub curves: Option<ProfileCurves>,
    /// Who shared it: the brand kit's name, when there is one
    #[serde(default)]
    pub shared_by: Option<String>,
}

/// The profile as a shared file
pub fn document(p: &ProfileWithPoints, shared_by: Option<String>) -> SharedProfile {
    let r = &p.profile;
    SharedProfile {
        format: FORMAT.into(),
        name: r.name.clone(),
        description: r.description.clone(),
        target_total_time: r.target_total_time,
        target_first_crack: r.target_first_crack,
        target_end_temp: r.target_end_temp,
        preheat_temp: r.preheat_temp,
        charge_temp: r.charge_temp,
        beans: r.beans.clone(),
        batch_size_g: r.batch_size_g,
        roaster: r.roaster.clone(),
        roast_level: r.roast_level.clone(),
        roast_purpose: r.roast_purpose.clone(),
        expectations: r.expectations.clone().map(|j| j.0),
        auto_drop: r.auto_drop.clone(),
        roaster_kind: r.roaster_kind.clone(),
        points: p
            .points
            .iter()
            .map(|pt| CreateProfilePointRequest {
                time_seconds: pt.time_seconds,
                target_temp: pt.target_temp,
                fan_speed: pt.fan_speed,
                notes: pt.notes.clone(),
                target_env_temp: pt.target_env_temp,
                heater_pct: pt.heater_pct,
            })
            .collect(),
        events: p
            .events
            .iter()
            .map(|e| CreateProfileEventRequest {
                trigger_kind: Some(e.trigger_kind),
                at_seconds: e.at_seconds,
                at_bt: e.at_bt,
                action: Some(e.action),
                value: e.value,
                label: e.label.clone(),
            })
            .collect(),
        curves: p.curves.clone().map(|c| ProfileCurves {
            // A roast of this server's: its id means nothing elsewhere
            source_session_id: None,
            ..c
        }),
        shared_by,
    }
}

/// A shared file as read back: the right format, with points
pub fn read_document(text: &str) -> Result<SharedProfile, String> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|_| "not a shared Roastwarden profile (not JSON)")?;
    match v.get("format").and_then(|f| f.as_str()) {
        Some(FORMAT) => {}
        Some(f) if f.starts_with("roastwarden.profile/") => {
            return Err(format!(
                "a shared profile in a newer format ({f}); update this server to import it"
            ));
        }
        _ => return Err("not a shared Roastwarden profile".into()),
    }
    let d: SharedProfile =
        serde_json::from_value(v).map_err(|e| format!("a shared profile it cannot read: {e}"))?;
    if d.points.is_empty() {
        return Err("the shared profile has no curve".into());
    }
    Ok(d)
}

// ---- the page ----

fn clock(s: f64) -> String {
    let s = s.max(0.0).round() as i64;
    format!("{}:{:02}", s / 60, s % 60)
}

fn f_of(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

/// `medium_light` → "Medium light"
fn words(key: &str) -> String {
    let t = key.replace('_', " ");
    let mut c = t.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

fn kind_name(kind: &str) -> String {
    match kind {
        "fluid_bed_electric" => "an electric fluid bed".into(),
        "drum_electric" => "an electric drum".into(),
        "drum_gas" => "a gas drum".into(),
        "logger" => "a logger (no control)".into(),
        other => words(other),
    }
}

fn landmark_name(label: &str) -> Option<&'static str> {
    Some(match label {
        "drying_end" => "Dry end",
        "first_crack_start" => "First crack",
        "second_crack_start" => "Second crack",
        "drop" => "Drop",
        _ => return None,
    })
}

/// The profile's curve: its BT points, the landmarks it marks (else its first crack target)
/// and the drop at its last point
pub fn curve(p: &ProfileWithPoints) -> Option<Curve> {
    let bt: Vec<(f32, f32)> = p
        .points
        .iter()
        .filter(|pt| pt.target_temp.is_finite())
        .map(|pt| (pt.time_seconds as f32, pt.target_temp))
        .collect();
    if bt.len() < 2 {
        return None;
    }
    let temp_at = |t: f32| -> Option<f32> {
        let i = bt.partition_point(|q| q.0 <= t);
        if i == 0 {
            return Some(bt[0].1);
        }
        if i >= bt.len() {
            return bt.last().map(|q| q.1);
        }
        let (t0, v0) = bt[i - 1];
        let (t1, v1) = bt[i];
        Some(v0 + (v1 - v0) * ((t - t0) / (t1 - t0).max(1e-3)))
    };
    let time_at_bt = |b: f32| -> Option<f32> {
        bt.windows(2)
            .find(|w| w[0].1 <= b && w[1].1 >= b)
            .map(|w| w[0].0 + (w[1].0 - w[0].0) * ((b - w[0].1) / (w[1].1 - w[0].1).max(1e-3)))
    };
    let mut marks: Vec<(&'static str, f32, Option<f32>)> = Vec::new();
    for e in &p.events {
        if e.action != ProfileAction::Landmark {
            continue;
        }
        let Some(word) = e.label.as_deref().and_then(landmark_name) else {
            continue;
        };
        let t = match e.trigger_kind {
            TriggerKind::Time => e.at_seconds.map(|s| s as f32),
            TriggerKind::Bt => e.at_bt.and_then(|b| time_at_bt(b as f32)),
        };
        if let Some(t) = t
            && !marks.iter().any(|m| m.0 == word)
        {
            marks.push((word, t, temp_at(t)));
        }
    }
    let end = bt.last().map(|q| q.0);
    let mut first_crack = marks.iter().find(|m| m.0 == "First crack").map(|m| m.1);
    if first_crack.is_none()
        && let Some(fc) = p.profile.target_first_crack
    {
        first_crack = Some(fc as f32);
        marks.push(("First crack", fc as f32, temp_at(fc as f32)));
    }
    let drop = marks
        .iter()
        .find(|m| m.0 == "Drop")
        .map(|m| m.1)
        .or(p.profile.target_total_time.map(|t| t as f32))
        .or(end);
    if !marks.iter().any(|m| m.0 == "Drop")
        && let Some(d) = drop
    {
        marks.push(("Drop", d, temp_at(d)));
    }
    marks.sort_by(|a, b| a.1.total_cmp(&b.1));
    Some(Curve {
        drop_temp: drop.and_then(temp_at),
        bt,
        events: marks,
        first_crack,
        drop,
    })
}

/// One event as a line: "4:00 · fan 85 %", "at 160 °C · Dry end"
fn event_line(e: &CreateProfileEventRequest) -> Option<String> {
    let when = match e.trigger_kind.unwrap_or(TriggerKind::Time) {
        TriggerKind::Time => clock(e.at_seconds?),
        TriggerKind::Bt => format!("at {:.0} °C", e.at_bt?),
    };
    let what = match e.action? {
        ProfileAction::Setpoint => format!("setpoint {:.0} °C", e.value?),
        ProfileAction::Fan => format!("fan {:.0} %", e.value?),
        ProfileAction::Heater => format!("heater {:.0} %", e.value?),
        ProfileAction::Mode => format!("{} mode", e.label.as_deref()?),
        ProfileAction::Landmark => {
            let l = e.label.as_deref()?;
            landmark_name(l).map_or_else(|| words(l), str::to_string)
        }
        ProfileAction::Note => format!("note: {}", e.label.as_deref()?),
    };
    Some(format!("{when} · {what}"))
}

/// The page as HTML: no script, nothing loaded from elsewhere; the files linked relatively
pub fn html(
    p: &ProfileWithPoints,
    code: &str,
    page_url: &str,
    brand: Option<&PageBrand>,
) -> String {
    let d = document(p, None);
    let r = &p.profile;
    let title = match brand {
        Some(b) => format!("{} · a roast profile from {}", r.name, b.name),
        None => format!("{} · a roast profile", r.name),
    };
    let mut s = rshare::head(&title, &rshare::style(brand));
    s.push_str("<body><main>");
    if let Some(b) = brand {
        s.push_str(r#"<header class="brand">"#);
        if let Some(logo) = &b.logo {
            let _ = write!(s, r#"<img src="{}" alt="">"#, esc(logo));
        }
        let _ = write!(s, r#"<div><div class="n">{}</div>"#, esc(&b.name));
        if let Some(t) = b.tagline.as_deref().filter(|t| !t.trim().is_empty()) {
            let _ = write!(s, r#"<div class="t">{}</div>"#, esc(t));
        }
        s.push_str("</div></header>");
    }
    let _ = write!(s, "<h1>{}</h1>", esc(&r.name));
    let for_kind = r
        .roaster_kind
        .as_deref()
        .map(|k| format!("A roast profile for {}", kind_name(k)));
    let _ = write!(
        s,
        r#"<p class="origin">{}</p>"#,
        esc(for_kind.as_deref().unwrap_or("A roast profile"))
    );
    let pill = match (r.roast_level.as_deref(), r.roast_purpose.as_deref()) {
        (Some(l), Some(p)) => Some(format!("{} roast for {p}", words(l))),
        (Some(l), None) => Some(format!("{} roast", words(l))),
        (None, Some(p)) => Some(format!("For {p}")),
        (None, None) => None,
    };
    if let Some(pl) = pill {
        let _ = write!(s, r#"<span class="pill">{}</span>"#, esc(&pl));
    }
    if let Some(desc) = r.description.as_deref().filter(|v| !v.trim().is_empty()) {
        let _ = write!(
            s,
            r#"<p class="notes" style="margin-top:18px">{}</p>"#,
            esc(desc)
        );
    }
    if let Some(c) = curve(p) {
        s.push_str("<h2>The curve</h2><figure>");
        s.push_str(&rshare::curve_svg(&c));
        let mut parts = Vec::new();
        if let Some(dr) = c.drop {
            parts.push(format!("Roast time {}", clock(f64::from(dr))));
        }
        if let (Some(fc), Some(dr)) = (c.first_crack, c.drop)
            && dr > fc
            && dr > 0.0
        {
            parts.push(format!(
                "development {} ({:.0} %)",
                clock(f64::from(dr - fc)),
                (dr - fc) / dr * 100.0
            ));
        }
        if let Some(t) = c.drop_temp {
            let t = f64::from(t);
            parts.push(format!("drop at {t:.0} °C ({:.0} °F)", f_of(t)));
        }
        if !parts.is_empty() {
            let _ = write!(s, "<figcaption>{}</figcaption>", esc(&parts.join(" · ")));
        }
        s.push_str("</figure>");
    }
    let mut facts: Vec<(&str, String)> = Vec::new();
    if let Some(t) = r.charge_temp {
        let t = f64::from(t);
        facts.push(("Charge", format!("{t:.0} °C ({:.0} °F)", f_of(t))));
    }
    if let Some(t) = r.target_first_crack {
        facts.push(("First crack", clock(f64::from(t))));
    }
    if let Some(t) = r.target_total_time {
        facts.push(("Drop", clock(f64::from(t))));
    }
    if let Some(t) = r.target_end_temp {
        let t = f64::from(t);
        facts.push(("Drop temperature", format!("{t:.0} °C ({:.0} °F)", f_of(t))));
    }
    if let Some(g) = r.batch_size_g {
        facts.push(("Batch", format!("{g:.0} g green")));
    }
    if let Some(b) = r.beans.as_deref().filter(|v| !v.trim().is_empty()) {
        facts.push(("Made for", b.to_string()));
    }
    if let Some(a) = r.auto_drop.as_deref() {
        facts.push((
            "Auto DROP",
            match a {
                "bt" => "at its drop temperature".into(),
                "development" => "at its development time".into(),
                _ => "at its drop time".into(),
            },
        ));
    }
    if !facts.is_empty() {
        s.push_str("<h2>Targets</h2><dl>");
        for (k, v) in facts {
            let _ = write!(s, "<dt>{k}</dt><dd>{}</dd>", esc(&v));
        }
        s.push_str("</dl>");
    }
    let lines: Vec<String> = d.events.iter().filter_map(event_line).collect();
    if !lines.is_empty() {
        s.push_str("<h2>What it does</h2><dl>");
        for l in lines {
            let (k, v) = l.split_once(" · ").unwrap_or(("", &l));
            let _ = write!(s, "<dt>{}</dt><dd>{}</dd>", esc(k), esc(v));
        }
        s.push_str("</dl>");
    }
    let _ = write!(
        s,
        r#"<h2>Use it</h2><div class="brew"><b>In Roastwarden</b><p>Profiles → Import from a link, and paste <code>{}</code> (or <code>roast profile import-link</code> with it).</p></div><div class="brew" style="margin-top:12px"><b>Files</b><p><a href="{code}/profile.json" download>profile.json</a> (Roastwarden) · <a href="{code}/profile.alog" download>profile.alog</a> (Artisan: Roast → Background)</p></div>"#,
        esc(page_url)
    );
    s.push_str("<footer>");
    if let Some(b) = brand {
        let _ = write!(s, "Shared by {}", esc(&b.name));
        if let Some(pl) = b.place.as_deref().filter(|v| !v.trim().is_empty()) {
            let _ = write!(s, " · {}", esc(pl));
        }
        s.push_str(" · ");
    }
    let _ = write!(s, "Made with {} Roastwarden", crate::ops::brand::html(14));
    s.push_str("</footer></main></body></html>");
    s
}

// ---- import from a link ----

/// Where a link points
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// `https://host[:port]` (or `http://` for this server's own)
    pub base: String,
    pub host: String,
    pub port: u16,
    pub https: bool,
    pub code: String,
}

impl Link {
    /// The page's address
    pub fn page(&self) -> String {
        url(&self.base, &self.code)
    }
    fn file(&self) -> String {
        format!("{}/profile.json", self.page())
    }
}

/// A link as pasted: the page (`/P/CODE`, `/p/code`) or one of its files
pub fn parse_link(text: &str) -> Result<Link, String> {
    let t = text.trim();
    let u = reqwest::Url::parse(t).map_err(|_| format!("not a link: {t:?}"))?;
    let https = match u.scheme() {
        "https" => true,
        "http" => false,
        s => {
            return Err(format!(
                "a {s}: link; a shared profile's starts with https://"
            ));
        }
    };
    if !u.username().is_empty() || u.password().is_some() {
        return Err("a link with a user name in it".into());
    }
    let host = u
        .host_str()
        .ok_or("a link with no host")?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    let port = u.port_or_known_default().unwrap_or(443);
    let segs: Vec<&str> = u
        .path_segments()
        .map(|s| s.filter(|p| !p.is_empty()).collect())
        .unwrap_or_default();
    let code = match segs.as_slice() {
        [p, c] | [p, c, "profile.json" | "profile.alog"] if p.eq_ignore_ascii_case("p") => {
            rshare::normal(c)
        }
        _ => None,
    }
    .ok_or("not a shared profile's link (https://host/P/ and 25 letters and digits)")?;
    let base = match u.port() {
        Some(p) => format!("{}://{host}:{p}", u.scheme()),
        None => format!("{}://{host}", u.scheme()),
    };
    Ok(Link {
        base,
        host,
        port,
        https,
        code,
    })
}

/// What a pasted link is: a code of this server's, or another server's link
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Here(String),
    There(Link),
}

/// Read a pasted link (or a bare code: this server's) against this server's own address
/// (`link_base`)
pub fn source(text: &str, own_base: &str) -> Result<Source, String> {
    if let Some(code) = rshare::normal(text) {
        return Ok(Source::Here(code));
    }
    let link = parse_link(text)?;
    let own = reqwest::Url::parse(own_base).ok();
    let own_host = own
        .as_ref()
        .and_then(|u| u.host_str())
        .map(|h| h.trim_end_matches('.').to_ascii_lowercase());
    let own_port = own.as_ref().and_then(reqwest::Url::port_or_known_default);
    if own_host.as_deref() == Some(link.host.as_str()) && own_port == Some(link.port) {
        return Ok(Source::Here(link.code));
    }
    if !link.https {
        return Err("another server's link must be https://".into());
    }
    Ok(Source::There(link))
}

/// An address on the public internet: not loopback, private, link-local, shared (CGNAT),
/// multicast, documentation or reserved
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => public_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return public_v4(v4);
            }
            let s = v6.segments();
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00 // unique local
                || (s[0] & 0xffc0) == 0xfe80 // link-local
                || (s[0] & 0xffc0) == 0xfec0 // site-local (deprecated)
                || (s[0] == 0x2001 && s[1] == 0x0db8) // documentation
                || (s[0] == 0x0064 && s[1] == 0xff9b) // NAT64: an IPv4 address inside
                || (s[0] == 0x2002) // 6to4: an IPv4 address inside
                || (s[0] == 0x2001 && s[1] == 0)) // Teredo
        }
    }
}

fn public_v4(v4: Ipv4Addr) -> bool {
    let o = v4.octets();
    !(v4.is_loopback()
        || v4.is_private()
        || v4.is_link_local()
        || v4.is_broadcast()
        || v4.is_documentation()
        || v4.is_unspecified()
        || v4.is_multicast()
        || o[0] == 0
        || (o[0] == 100 && (o[1] & 0xc0) == 64) // shared address space (CGNAT)
        || (o[0] == 192 && o[1] == 0 && o[2] == 0) // IETF protocol assignments
        || (o[0] == 198 && (o[1] & 0xfe) == 18) // benchmarking
        || o[0] >= 240) // reserved
}

const USER_AGENT: &str = concat!(
    "Roastwarden/",
    env!("CARGO_PKG_VERSION"),
    " (profile import; one shared profile when its owner asks)"
);
const MAX_BYTES: usize = 2 << 20;

/// Why a fetch failed: the link is not active there (a 404 here too), or the other server
/// could not be read (502)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    NotFound(String),
    Failed(String),
}

impl From<String> for FetchError {
    fn from(s: String) -> Self {
        FetchError::Failed(s)
    }
}

/// Fetch a shared profile's file from another server: public addresses only, the one checked
/// pinned for the request (a second lookup cannot point elsewhere), no redirects
pub async fn fetch(link: &Link) -> Result<String, FetchError> {
    let addrs: Vec<SocketAddr> = tokio::time::timeout(
        Duration::from_secs(5),
        // An IPv6 literal without its brackets
        tokio::net::lookup_host((
            link.host.trim_start_matches('[').trim_end_matches(']'),
            link.port,
        )),
    )
    .await
    .map_err(|_| format!("{} did not resolve in time", link.host))?
    .map_err(|_| format!("{} does not resolve", link.host))?
    .collect();
    let Some(first) = addrs.first().copied() else {
        return Err(format!("{} does not resolve", link.host).into());
    };
    if let Some(bad) = addrs.iter().find(|a| !is_public(a.ip())) {
        return Err(FetchError::Failed(format!(
            "{} is a private or local address ({}): the server imports from public links only",
            link.host,
            bad.ip()
        )));
    }
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&link.host, first)
        .build()
        .map_err(|e| format!("could not make the request: {e}"))?;
    let mut res = client
        .get(link.file())
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| format!("could not read {}: {}", link.page(), short(&e)))?;
    match res.status().as_u16() {
        200 => {}
        404 | 410 => {
            return Err(FetchError::NotFound(format!(
                "{} is not active (revoked, or never was)",
                link.page()
            )));
        }
        s => return Err(format!("{} answered {s}", link.page()).into()),
    }
    let mut body: Vec<u8> = Vec::new();
    while let Some(chunk) = res
        .chunk()
        .await
        .map_err(|e| format!("could not read {}: {}", link.page(), short(&e)))?
    {
        body.extend_from_slice(&chunk);
        if body.len() > MAX_BYTES {
            return Err(FetchError::Failed(
                "the shared profile is larger than 2 MB".into(),
            ));
        }
    }
    String::from_utf8(body).map_err(|_| FetchError::Failed("the shared profile is not text".into()))
}

fn short(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "it took too long".into()
    } else if e.is_connect() {
        "could not connect".into()
    } else {
        e.to_string()
    }
}

/// The line an imported profile's description ends with: where it came from, and when
pub fn imported_line(page: &str) -> String {
    format!("Imported from {page} on {}.", Utc::now().format("%Y-%m-%d"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CODE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXY";

    #[test]
    fn links_are_read_in_every_form() {
        for t in [
            "https://roast.example.com/P/ABCDEFGHIJKLMNOPQRSTUVWXY",
            "https://Roast.Example.com/p/abcdefghijklmnopqrstuvwxy/",
            "https://roast.example.com/P/ABCDEFGHIJKLMNOPQRSTUVWXY/profile.json",
            "  https://roast.example.com/P/ABCDEFGHIJKLMNOPQRSTUVWXY/profile.alog ",
        ] {
            let l = parse_link(t).unwrap();
            assert_eq!(l.code, CODE, "{t}");
            assert_eq!(l.base, "https://roast.example.com");
            assert_eq!(l.port, 443);
            assert_eq!(
                l.file(),
                "https://roast.example.com/P/ABCDEFGHIJKLMNOPQRSTUVWXY/profile.json"
            );
        }
        let l = parse_link("https://h.example:8443/P/ABCDEFGHIJKLMNOPQRSTUVWXY").unwrap();
        assert_eq!(l.base, "https://h.example:8443");
        for bad in [
            "ftp://h.example/P/ABCDEFGHIJKLMNOPQRSTUVWXY",
            "https://h.example/R/ABCDEFGHIJKLMNOPQRSTUVWXY",
            "https://h.example/P/ABC",
            "https://u:p@h.example/P/ABCDEFGHIJKLMNOPQRSTUVWXY",
            "https://h.example/x/P/ABCDEFGHIJKLMNOPQRSTUVWXY",
            "hello",
        ] {
            assert!(
                parse_link(bad).is_err() && source(bad, "https://me.example").is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_code_or_this_servers_own_link_is_read_here() {
        let own = "https://me.example";
        assert_eq!(source(CODE, own), Ok(Source::Here(CODE.into())));
        assert_eq!(
            source(&format!("https://ME.example/P/{CODE}"), own),
            Ok(Source::Here(CODE.into()))
        );
        // A local server over http: its own links are read here, with no request
        assert_eq!(
            source(
                &format!("http://localhost:18080/p/{CODE}"),
                "http://localhost:18080"
            ),
            Ok(Source::Here(CODE.into()))
        );
        // Another server must be https
        assert!(source(&format!("http://other.example/P/{CODE}"), own).is_err());
        assert!(matches!(
            source(&format!("https://other.example/P/{CODE}"), own),
            Ok(Source::There(_))
        ));
        // The same host on another port is another server
        assert!(matches!(
            source(&format!("https://me.example:8443/P/{CODE}"), own),
            Ok(Source::There(_))
        ));
    }

    #[test]
    fn only_public_addresses_are_fetched() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.172",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "192.0.2.10",
            "198.18.0.1",
            "224.0.0.1",
            "255.255.255.255",
            "::1",
            "::",
            "fd00::1",
            "fe80::1",
            "::ffff:192.168.1.1",
            "::ffff:127.0.0.1",
            "2001:db8::1",
            "64:ff9b::a00:1",
            "2002:c0a8:101::1",
        ] {
            assert!(!is_public(ip.parse().unwrap()), "{ip}");
        }
        for ip in [
            "1.1.1.1",
            "93.184.216.34",
            "2606:4700:4700::1111",
            "::ffff:8.8.8.8",
        ] {
            assert!(is_public(ip.parse().unwrap()), "{ip}");
        }
    }

    #[test]
    fn a_shared_file_must_say_its_format() {
        let ok = r#"{"format":"roastwarden.profile/1","name":"A","points":[{"time_seconds":0,"target_temp":30}]}"#;
        assert_eq!(read_document(ok).unwrap().name, "A");
        assert!(
            read_document(r#"{"name":"A","points":[{"time_seconds":0,"target_temp":30}]}"#)
                .unwrap_err()
                .contains("not a shared")
        );
        assert!(
            read_document(r#"{"format":"roastwarden.profile/2","name":"A","points":[]}"#)
                .unwrap_err()
                .contains("newer format")
        );
        assert!(
            read_document(r#"{"format":"roastwarden.profile/1","name":"A","points":[]}"#)
                .unwrap_err()
                .contains("no curve")
        );
        assert!(read_document("<html>").is_err());
    }

    #[test]
    fn events_read_as_lines() {
        let e = |k, s: Option<f64>, b: Option<f64>, a, v: Option<f64>, l: Option<&str>| {
            CreateProfileEventRequest {
                trigger_kind: Some(k),
                at_seconds: s,
                at_bt: b,
                action: Some(a),
                value: v,
                label: l.map(str::to_string),
            }
        };
        assert_eq!(
            event_line(&e(
                TriggerKind::Time,
                Some(240.0),
                None,
                ProfileAction::Fan,
                Some(85.0),
                None
            )),
            Some("4:00 · fan 85 %".into())
        );
        assert_eq!(
            event_line(&e(
                TriggerKind::Bt,
                None,
                Some(160.0),
                ProfileAction::Landmark,
                None,
                Some("drying_end")
            )),
            Some("at 160 °C · Dry end".into())
        );
        assert_eq!(
            event_line(&e(
                TriggerKind::Time,
                None,
                None,
                ProfileAction::Fan,
                Some(85.0),
                None
            )),
            None
        );
    }
}
