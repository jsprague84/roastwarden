//! Saved label designs with every version, the assets they draw on (images and fonts), brand
//! kits, and a coffee's own label (migration 030; docs/labels.md, "Label studio"). The
//! built-in designs and the owner's folder stay as they are; a saved design's id may not be
//! one of theirs.

use std::collections::BTreeMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use serde_json::{json, Map, Value};
use sqlx::SqlitePool;
use utoipa::ToSchema;

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// `sam-gift` or `sam-gift@3`: a design and, when given, one of its versions.
pub fn parse_ref(reference: &str) -> Option<(String, Option<i64>)> {
    let (id, version) = match reference.split_once('@') {
        Some((id, v)) => (id, Some(v.parse::<i64>().ok().filter(|v| *v > 0)?)),
        None => (reference, None),
    };
    super::valid_id(id).then(|| (id.to_string(), version))
}

// ---- designs ----

#[derive(Debug, Clone, Serialize, ToSchema, sqlx::FromRow)]
pub struct VersionInfo {
    pub version: i64,
    pub note: Option<String>,
    /// owner, or the token's name
    pub created_by: String,
    pub created_at: String,
}

/// A saved design: its details, the version shown with its source, and every version.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SavedDesign {
    pub id: String,
    /// `saved` (the folder's and built-in designs answer `folder` or `builtin`, version 0)
    pub kind: String,
    pub name: String,
    pub description: Option<String>,
    /// The current version
    pub version: i64,
    /// The version whose source this is (the current one unless asked for `id@N`)
    pub shown: i64,
    pub source: String,
    /// Its `design` dictionary (name, description, params), as the check read it
    pub manifest: Option<Value>,
    /// Newest first
    pub versions: Vec<VersionInfo>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(sqlx::FromRow)]
struct DesignRow {
    id: String,
    name: String,
    description: Option<String>,
    version: i64,
    created_at: String,
    updated_at: String,
}

#[derive(sqlx::FromRow)]
struct ListRow {
    id: String,
    name: String,
    description: Option<String>,
    version: i64,
    updated_at: String,
    manifest: Option<String>,
}

/// The saved designs with their current version's manifest
pub async fn designs(db: &SqlitePool) -> Result<Vec<(DesignRowView, Option<Value>)>> {
    let rows: Vec<ListRow> = sqlx::query_as(
        "SELECT d.id, d.name, d.description, d.version, d.updated_at, v.manifest
         FROM label_designs d JOIN label_design_versions v
           ON v.design_id = d.id AND v.version = d.version
         ORDER BY d.name COLLATE NOCASE",
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                DesignRowView {
                    id: r.id,
                    name: r.name,
                    description: r.description,
                    version: r.version,
                    updated_at: r.updated_at,
                },
                r.manifest.and_then(|m| serde_json::from_str(&m).ok()),
            )
        })
        .collect())
}

pub struct DesignRowView {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub version: i64,
    pub updated_at: String,
}

pub async fn design(
    db: &SqlitePool,
    id: &str,
    version: Option<i64>,
) -> Result<Option<SavedDesign>> {
    let Some(row) = sqlx::query_as::<_, DesignRow>(
        "SELECT id, name, description, version, created_at, updated_at FROM label_designs WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    else {
        return Ok(None);
    };
    let shown = version.unwrap_or(row.version);
    let Some((source, manifest)) = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT source, manifest FROM label_design_versions WHERE design_id = ? AND version = ?",
    )
    .bind(id)
    .bind(shown)
    .fetch_optional(db)
    .await?
    else {
        return Ok(None);
    };
    let versions = sqlx::query_as::<_, VersionInfo>(
        "SELECT version, note, created_by, created_at FROM label_design_versions
         WHERE design_id = ? ORDER BY version DESC",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    Ok(Some(SavedDesign {
        id: row.id,
        kind: "saved".into(),
        name: row.name,
        description: row.description,
        version: row.version,
        shown,
        source,
        manifest: manifest.and_then(|m| serde_json::from_str(&m).ok()),
        versions,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }))
}

/// A new design at version 1. False when the id is taken.
#[allow(clippy::too_many_arguments)]
pub async fn create(
    db: &SqlitePool,
    id: &str,
    name: &str,
    description: Option<&str>,
    source: &str,
    manifest: Option<&Value>,
    note: Option<&str>,
    by: &str,
) -> Result<bool> {
    let mut tx = db.begin().await?;
    let at = now();
    let inserted = sqlx::query(
        "INSERT INTO label_designs (id, name, description, version, created_at, updated_at)
         VALUES (?, ?, ?, 1, ?, ?) ON CONFLICT (id) DO NOTHING",
    )
    .bind(id)
    .bind(name)
    .bind(description)
    .bind(&at)
    .bind(&at)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if inserted == 0 {
        return Ok(false);
    }
    sqlx::query(
        "INSERT INTO label_design_versions (design_id, version, source, manifest, note, created_by, created_at)
         VALUES (?, 1, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(source)
    .bind(manifest.map(Value::to_string))
    .bind(note)
    .bind(by)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(true)
}

/// What an edit changes: a new source makes a new version; name and description are kept
/// unless given.
pub struct Edit<'a> {
    pub source: Option<(&'a str, Option<&'a Value>)>,
    pub name: Option<&'a str>,
    pub description: Option<&'a str>,
    pub note: Option<&'a str>,
    pub by: &'a str,
}

/// Apply an edit; the design's current version after it, or none when there is no design.
pub async fn edit(db: &SqlitePool, id: &str, e: Edit<'_>) -> Result<Option<i64>> {
    // It reads the version, then writes the next one: the write lock first (db::begin_write).
    let mut tx = crate::db::begin_write(db).await?;
    let Some((current,)) =
        sqlx::query_as::<_, (i64,)>("SELECT version FROM label_designs WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
    else {
        return Ok(None);
    };
    let at = now();
    let mut version = current;
    if let Some((source, manifest)) = e.source {
        version = current + 1;
        sqlx::query(
            "INSERT INTO label_design_versions (design_id, version, source, manifest, note, created_by, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(version)
        .bind(source)
        .bind(manifest.map(Value::to_string))
        .bind(e.note)
        .bind(e.by)
        .bind(&at)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "UPDATE label_designs SET version = ?, name = COALESCE(?, name),
         description = COALESCE(?, description), updated_at = ? WHERE id = ?",
    )
    .bind(version)
    .bind(e.name)
    .bind(e.description)
    .bind(&at)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some(version))
}

pub async fn delete_design(db: &SqlitePool, id: &str) -> Result<bool> {
    let mut tx = db.begin().await?;
    sqlx::query("DELETE FROM label_design_versions WHERE design_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let n = sqlx::query("DELETE FROM label_designs WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    tx.commit().await?;
    Ok(n > 0)
}

/// The beans whose label is this design (any version)
pub async fn beans_using(db: &SqlitePool, id: &str) -> Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT name FROM beans WHERE label_design = ? OR label_design LIKE ? ORDER BY name",
    )
    .bind(id)
    .bind(format!("{id}@%"))
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

/// Every design a coffee uses as its label, with the coffees (names), by design id
pub async fn label_users(db: &SqlitePool) -> Result<BTreeMap<String, Vec<String>>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT label_design, name FROM beans WHERE label_design IS NOT NULL ORDER BY name",
    )
    .fetch_all(db)
    .await?;
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (design, bean) in rows {
        let id = design.split('@').next().unwrap_or_default().to_string();
        out.entry(id).or_default().push(bean);
    }
    Ok(out)
}

// ---- assets ----

/// An image or a font a design can use by its name.
#[derive(Debug, Clone, Serialize, ToSchema, sqlx::FromRow)]
pub struct Asset {
    /// What a design names: `image("logo.svg")`, or a font file
    pub name: String,
    /// image | font
    pub kind: String,
    pub content_type: String,
    pub size: i64,
    pub sha256: String,
    /// A font's family, as `text(font: …)` names it
    pub family: Option<String>,
    /// Where it came from, its licence
    pub note: Option<String>,
    pub created_by: String,
    pub created_at: String,
}

/// The largest image and font, bytes
pub const MAX_IMAGE: usize = 2 << 20;
pub const MAX_FONT: usize = 8 << 20;

/// Names the drivers and the check use themselves
const RESERVED: [&str; 4] = ["qr.svg", "design.typ", "roastwarden.typ", "main.typ"];

/// What an upload is: (kind, content type, a font's family), or why it cannot be one.
pub fn check_asset(
    name: &str,
    data: &[u8],
) -> Result<(&'static str, &'static str, Option<String>), String> {
    let good_name = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    if !good_name {
        return Err(format!(
            "an asset's name is letters, digits, '.', '-' and '_' (up to 64), with its type's ending: {name:?}"
        ));
    }
    if RESERVED.contains(&name) {
        return Err(format!("{name} is the server's own; choose another name"));
    }
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let image = |ct: &'static str, ok: bool, what: &str| {
        if data.len() > MAX_IMAGE {
            Err(format!("an image is at most {} MB", MAX_IMAGE >> 20))
        } else if !ok {
            Err(format!("{name} is not {what}"))
        } else {
            Ok(("image", ct, None))
        }
    };
    match ext.as_str() {
        "svg" => image(
            "image/svg+xml",
            std::str::from_utf8(data).is_ok_and(|t| t.contains("<svg")),
            "an SVG",
        ),
        "png" => image("image/png", data.starts_with(b"\x89PNG\r\n\x1a\n"), "a PNG"),
        "jpg" | "jpeg" => image(
            "image/jpeg",
            data.starts_with(&[0xFF, 0xD8, 0xFF]),
            "a JPEG",
        ),
        "ttf" | "otf" => {
            if data.len() > MAX_FONT {
                return Err(format!("a font is at most {} MB", MAX_FONT >> 20));
            }
            let mut families: Vec<String> =
                typst::text::Font::iter(typst::foundations::Bytes::new(data.to_vec()))
                    .map(|f| f.info().family.clone())
                    .collect();
            families.dedup();
            if families.is_empty() {
                return Err(format!("{name} is not a font Typst can read"));
            }
            let ct = if ext == "otf" { "font/otf" } else { "font/ttf" };
            Ok(("font", ct, Some(families.join(", "))))
        }
        _ => Err(format!(
            "{name}: an asset is an image (.svg, .png, .jpg) or a font (.ttf, .otf)"
        )),
    }
}

pub async fn assets(db: &SqlitePool) -> Result<Vec<Asset>> {
    Ok(sqlx::query_as::<_, Asset>(
        "SELECT name, kind, content_type, size, sha256, family, note, created_by, created_at
         FROM label_assets ORDER BY kind, name COLLATE NOCASE",
    )
    .fetch_all(db)
    .await?)
}

pub async fn asset(db: &SqlitePool, name: &str) -> Result<Option<Asset>> {
    Ok(sqlx::query_as::<_, Asset>(
        "SELECT name, kind, content_type, size, sha256, family, note, created_by, created_at
         FROM label_assets WHERE name = ?",
    )
    .bind(name)
    .fetch_optional(db)
    .await?)
}

pub async fn asset_data(db: &SqlitePool, name: &str) -> Result<Option<(String, Vec<u8>)>> {
    Ok(sqlx::query_as::<_, (String, Vec<u8>)>(
        "SELECT content_type, data FROM label_assets WHERE name = ?",
    )
    .bind(name)
    .fetch_optional(db)
    .await?)
}

/// Add or replace an asset; true when it is new.
#[allow(clippy::too_many_arguments)]
pub async fn put_asset(
    db: &SqlitePool,
    name: &str,
    kind: &str,
    content_type: &str,
    data: &[u8],
    family: Option<&str>,
    note: Option<&str>,
    by: &str,
) -> Result<bool> {
    use sha2::Digest;
    let sha = format!("{:x}", sha2::Sha256::digest(data));
    let existed = asset(db, name).await?.is_some();
    sqlx::query(
        "INSERT INTO label_assets (name, kind, content_type, data, size, sha256, family, note, created_by, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (name) DO UPDATE SET kind = excluded.kind, content_type = excluded.content_type,
           data = excluded.data, size = excluded.size, sha256 = excluded.sha256,
           family = excluded.family, note = COALESCE(excluded.note, label_assets.note),
           created_by = excluded.created_by, created_at = excluded.created_at",
    )
    .bind(name)
    .bind(kind)
    .bind(content_type)
    .bind(data)
    .bind(data.len() as i64)
    .bind(sha)
    .bind(family)
    .bind(note)
    .bind(by)
    .bind(now())
    .execute(db)
    .await?;
    Ok(!existed)
}

pub async fn delete_asset(db: &SqlitePool, name: &str) -> Result<bool> {
    Ok(sqlx::query("DELETE FROM label_assets WHERE name = ?")
        .bind(name)
        .execute(db)
        .await?
        .rows_affected()
        > 0)
}

/// Every font asset's bytes, for a render
pub async fn fonts(db: &SqlitePool) -> Result<Vec<Vec<u8>>> {
    let rows: Vec<(Vec<u8>,)> =
        sqlx::query_as("SELECT data FROM label_assets WHERE kind = 'font' ORDER BY name")
            .fetch_all(db)
            .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

/// The image assets whose names appear in any of `texts` (the design, its parameters, the
/// brand): what a render may read, without sending every image to every render.
pub async fn images_named(db: &SqlitePool, texts: &[&str]) -> Result<BTreeMap<String, ByteBuf>> {
    let names: Vec<(String,)> =
        sqlx::query_as("SELECT name FROM label_assets WHERE kind = 'image'")
            .fetch_all(db)
            .await?;
    let mut out = BTreeMap::new();
    for (name,) in names {
        if texts.iter().any(|t| t.contains(&name)) {
            if let Some((_, data)) = asset_data(db, &name).await? {
                out.insert(name, ByteBuf::from(data));
            }
        }
    }
    Ok(out)
}

// ---- brands ----

/// A brand kit: what a design reads as `l.brand` (every key there, `none` when unset).
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct Brand {
    #[serde(default)]
    pub id: String,
    /// The roaster's name on a label
    pub name: String,
    pub tagline: Option<String>,
    /// City and state (a label for coffee that is sold names the packer's place)
    pub place: Option<String>,
    pub website: Option<String>,
    /// An image asset's name: `image(l.brand.logo, width: 0.6in)`
    pub logo: Option<String>,
    /// Colours, `#rrggbb`: `rgb(l.brand.ink)`
    pub ink: Option<String>,
    pub accent: Option<String>,
    pub paper: Option<String>,
    /// Font families: `text(font: l.brand.font_display)`
    pub font_display: Option<String>,
    pub font_body: Option<String>,
    pub font_script: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

impl Brand {
    /// What is wrong with it, if anything
    pub fn check(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("a brand has a name".into());
        }
        for (k, v) in [
            ("ink", &self.ink),
            ("accent", &self.accent),
            ("paper", &self.paper),
        ] {
            if let Some(c) = v {
                let hex = c.strip_prefix('#').unwrap_or("");
                if !(hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())) {
                    return Err(format!("{k} is a colour like #2b2118, not {c:?}"));
                }
            }
        }
        Ok(())
    }

    /// `l.brand`: every key, `none` when unset
    pub fn value(b: Option<&Brand>) -> Value {
        let b = b.cloned().unwrap_or_default();
        let opt = |v: Option<String>| v.filter(|s| !s.trim().is_empty());
        json!({
            "id": (!b.id.is_empty()).then_some(b.id),
            "name": (!b.name.is_empty()).then_some(b.name),
            "tagline": opt(b.tagline),
            "place": opt(b.place),
            "website": opt(b.website),
            "logo": opt(b.logo),
            "ink": opt(b.ink),
            "accent": opt(b.accent),
            "paper": opt(b.paper),
            "font_display": opt(b.font_display),
            "font_body": opt(b.font_body),
            "font_script": opt(b.font_script),
        })
    }
}

const BRAND_COLUMNS: &str = "id, name, tagline, place, website, logo, ink, accent, paper, \
    font_display, font_body, font_script, created_at, updated_at";

pub async fn brands(db: &SqlitePool) -> Result<Vec<Brand>> {
    Ok(sqlx::query_as::<_, Brand>(&format!(
        "SELECT {BRAND_COLUMNS} FROM label_brands ORDER BY name COLLATE NOCASE"
    ))
    .fetch_all(db)
    .await?)
}

pub async fn brand(db: &SqlitePool, id: &str) -> Result<Option<Brand>> {
    Ok(sqlx::query_as::<_, Brand>(&format!(
        "SELECT {BRAND_COLUMNS} FROM label_brands WHERE id = ?"
    ))
    .bind(id)
    .fetch_optional(db)
    .await?)
}

/// Add or replace a brand; true when it is new.
pub async fn put_brand(db: &SqlitePool, b: &Brand) -> Result<bool> {
    let existed = brand(db, &b.id).await?.is_some();
    let at = now();
    sqlx::query(
        "INSERT INTO label_brands (id, name, tagline, place, website, logo, ink, accent, paper,
           font_display, font_body, font_script, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (id) DO UPDATE SET name = excluded.name, tagline = excluded.tagline,
           place = excluded.place, website = excluded.website, logo = excluded.logo,
           ink = excluded.ink, accent = excluded.accent, paper = excluded.paper,
           font_display = excluded.font_display, font_body = excluded.font_body,
           font_script = excluded.font_script, updated_at = excluded.updated_at",
    )
    .bind(&b.id)
    .bind(&b.name)
    .bind(&b.tagline)
    .bind(&b.place)
    .bind(&b.website)
    .bind(&b.logo)
    .bind(&b.ink)
    .bind(&b.accent)
    .bind(&b.paper)
    .bind(&b.font_display)
    .bind(&b.font_body)
    .bind(&b.font_script)
    .bind(&at)
    .bind(&at)
    .execute(db)
    .await?;
    Ok(!existed)
}

pub async fn delete_brand(db: &SqlitePool, id: &str) -> Result<bool> {
    Ok(sqlx::query("DELETE FROM label_brands WHERE id = ?")
        .bind(id)
        .execute(db)
        .await?
        .rows_affected()
        > 0)
}

// ---- a coffee's label ----

/// The label a bean gets unless asked otherwise.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct BeanLabel {
    /// A design: `sam-gift`, or `sam-gift@3` for one version
    pub design: Option<String>,
    /// Its parameters
    #[serde(default)]
    pub params: Map<String, Value>,
    /// A brand's id
    pub brand: Option<String>,
}

pub async fn bean_label(db: &SqlitePool, bean_id: &str) -> Result<Option<BeanLabel>> {
    let row: Option<(Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT label_design, label_params, label_brand FROM beans WHERE id = ?")
            .bind(bean_id)
            .fetch_optional(db)
            .await?;
    Ok(row.map(|(design, params, brand)| BeanLabel {
        design,
        params: params
            .and_then(|p| serde_json::from_str(&p).ok())
            .unwrap_or_default(),
        brand,
    }))
}

pub async fn set_bean_label(db: &SqlitePool, bean_id: &str, l: &BeanLabel) -> Result<bool> {
    let params = (!l.params.is_empty()).then(|| Value::Object(l.params.clone()).to_string());
    Ok(sqlx::query(
        "UPDATE beans SET label_design = ?, label_params = ?, label_brand = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&l.design)
    .bind(params)
    .bind(&l.brand)
    .bind(chrono::Utc::now())
    .bind(bean_id)
    .execute(db)
    .await?
    .rows_affected()
        > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_name_a_design_and_maybe_a_version() {
        assert_eq!(parse_ref("sam-gift"), Some(("sam-gift".into(), None)));
        assert_eq!(parse_ref("sam-gift@3"), Some(("sam-gift".into(), Some(3))));
        assert_eq!(parse_ref("sam-gift@0"), None);
        assert_eq!(parse_ref("../x"), None);
        assert_eq!(parse_ref("a@b"), None);
    }

    #[test]
    fn assets_are_what_their_names_say() {
        assert_eq!(
            check_asset("logo.svg", b"<svg xmlns='http://www.w3.org/2000/svg'/>")
                .unwrap()
                .0,
            "image"
        );
        assert!(check_asset("logo.svg", b"not svg").is_err());
        assert!(check_asset("qr.svg", b"<svg/>").is_err());
        assert!(check_asset("../x.png", b"\x89PNG\r\n\x1a\n").is_err());
        assert!(check_asset("x.gif", b"GIF89a").is_err());
        assert!(check_asset("x.ttf", b"nope").is_err());
        let font = include_bytes!("../../assets/fonts/PinyonScript-Regular.ttf");
        let (kind, ct, family) = check_asset("Pinyon.ttf", font).unwrap();
        assert_eq!(
            (kind, ct, family.as_deref()),
            ("font", "font/ttf", Some("Pinyon Script"))
        );
    }

    #[test]
    fn a_brand_is_checked_and_every_key_reaches_the_design() {
        let b = Brand {
            id: "home".into(),
            name: "Hill Street Roasting".into(),
            ink: Some("#2b2118".into()),
            ..Brand::default()
        };
        assert!(b.check().is_ok());
        let v = Brand::value(Some(&b));
        assert_eq!(v["name"], "Hill Street Roasting");
        assert!(v.as_object().unwrap().contains_key("logo"));
        assert!(Brand {
            ink: Some("brown".into()),
            ..b.clone()
        }
        .check()
        .is_err());
        assert_eq!(Brand::value(None)["name"], Value::Null);
    }
}
