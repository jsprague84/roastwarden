//! `roast labels design|asset|brand …`: saved designs and their versions, a coffee's own design, the
//! images and fonts designs use, the brand kits.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Duration;

use clap::Subcommand;
use reqwest::Method;
use serde_json::{Value, json};

use super::check::{CheckArgs, check};
use super::{params, read_file};
use crate::api::{Api, Upload, enc};
use crate::cli::Wait;
use crate::commands::approvals::held_or_done;
use crate::commands::records::save_file;
use crate::out::{Failure, Out, code};

#[derive(Subcommand, Debug)]
pub enum DesignCmd {
    /// Check one label of a design before saving or printing it: a .typ file (a draft) or a
    /// design's id. Answers Typst's errors with file and line, or a layout report: every line
    /// of text with its font, size and box (mm), and issues (overflow, near_edge, overlap,
    /// too_long, cut, tiny_text, qr_small, qr_crowded, typst warnings such as an unknown
    /// font). --png writes the label as a picture to look at. With no --roast, sample data.
    /// Exit 0 clean, 1 issues to fix, 6 it does not compile
    Check(Box<CheckArgs>),
    /// Every design: saved (with its version and parameters), the owner's folder, built in
    List,
    /// One design with its versions and its source; --source prints only the source (or
    /// writes it with -o), to start a new design from
    Show {
        /// The design: `sam-gift`, `sam-gift@2` for a version, or a built-in (`plain`)
        design: String,
        #[arg(long)]
        source: bool,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Save a new design from a .typ file (inventory or admin): it must compile, and the
    /// answer has the check's report on sample data
    Add {
        /// Its id: letters, digits, - and _ (`sam-gift`)
        id: String,
        file: PathBuf,
        /// Default: the manifest's `design.name`
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
        /// What it is, kept with the version
        #[arg(long)]
        note: Option<String>,
        /// The sheet the save checks it on (default 5163)
        #[arg(long)]
        sheet: Option<String>,
    },
    /// A new version from a .typ file (checked first), or a new name or description
    Edit {
        id: String,
        file: Option<PathBuf>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
        /// What changed, kept with the version
        #[arg(long)]
        note: Option<String>,
        #[arg(long)]
        sheet: Option<String>,
    },
    /// Go back to an earlier version: it becomes the next one, nothing is lost
    Rollback {
        id: String,
        /// The version to go back to
        #[arg(value_name = "VERSION")]
        to: i64,
        #[arg(long)]
        note: Option<String>,
    },
    /// Give a coffee (bean) its label: the design (`id` or `id@N`), its parameters (--set)
    /// and brand its roasts' labels use unless told otherwise; --clear takes it off
    Use {
        /// The design; leave out with --clear
        #[arg(required_unless_present = "clear")]
        design: Option<String>,
        /// The bean, by name or id
        #[arg(long)]
        bean: String,
        #[arg(long = "set", value_name = "KEY=VALUE")]
        set: Vec<String>,
        /// A brand kit's id
        #[arg(long)]
        brand: Option<String>,
        #[arg(long, conflicts_with_all = ["design", "set", "brand"])]
        clear: bool,
    },
    /// Delete a saved design and its versions (inventory or admin); may wait for the
    /// owner's approval. Refused while a bean uses it
    Rm {
        id: String,
        #[command(flatten)]
        wait: Wait,
    },
}

#[derive(Subcommand, Debug)]
pub enum AssetCmd {
    /// The images and fonts there are, with a font's family
    List,
    /// Add (or replace) an image (.svg, .png, .jpg, up to 2 MB) or a font (.ttf, .otf, up
    /// to 8 MB); a design names it by its name (`image("logo.svg")`) or a font's family
    Add {
        file: PathBuf,
        /// Default: the file's name
        #[arg(long)]
        name: Option<String>,
        /// Where it came from, its licence
        #[arg(long)]
        note: Option<String>,
    },
    /// Save an asset's file here
    Get {
        name: String,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Delete an asset; may wait for the owner's approval
    Rm {
        name: String,
        #[command(flatten)]
        wait: Wait,
    },
}

// Parsed once per run: its size does not matter.
#[allow(clippy::large_enum_variant)]
#[derive(Subcommand, Debug)]
pub enum BrandCmd {
    /// The brand kits
    List,
    /// One brand kit
    Show { id: String },
    /// Add a brand kit or change one: the flags given change, the others are kept
    Set {
        /// Its id: letters, digits, - and _ (`home`)
        id: String,
        /// The roaster's name on a label
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        tagline: Option<String>,
        /// City and state
        #[arg(long)]
        place: Option<String>,
        #[arg(long)]
        website: Option<String>,
        /// An image asset's name (`roast labels asset add logo.svg` first)
        #[arg(long)]
        logo: Option<String>,
        /// Colours, #rrggbb
        #[arg(long)]
        ink: Option<String>,
        #[arg(long)]
        accent: Option<String>,
        #[arg(long)]
        paper: Option<String>,
        /// Font families (`roast labels --list` has them)
        #[arg(long)]
        font_display: Option<String>,
        #[arg(long)]
        font_body: Option<String>,
        #[arg(long)]
        font_script: Option<String>,
    },
    /// Delete a brand kit; may wait for the owner's approval
    Rm {
        id: String,
        #[command(flatten)]
        wait: Wait,
    },
}

pub async fn design(api: &Api, out: &Out, cmd: DesignCmd) -> Result<u8, Failure> {
    match cmd {
        DesignCmd::Check(a) => check(api, out, *a).await,
        DesignCmd::List => {
            let list = api.get("/api/label-designs").await?;
            out.emit(&list, |l| {
                l.as_array()
                    .into_iter()
                    .flatten()
                    .map(|d| {
                        let v = d["version"]
                            .as_i64()
                            .map(|v| format!("@{v}"))
                            .unwrap_or_default();
                        let params = d["params"]
                            .as_array()
                            .map(|p| {
                                let names: Vec<&str> =
                                    p.iter().filter_map(|x| x.as_str()).collect();
                                format!("  params: {}", names.join(", "))
                            })
                            .unwrap_or_default();
                        format!(
                            "{:<16} {:<8} {}{params}",
                            format!("{}{v}", d["id"].as_str().unwrap_or("?")),
                            d["kind"].as_str().unwrap_or(""),
                            d["name"].as_str().unwrap_or("")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
            Ok(code::OK)
        }
        DesignCmd::Show {
            design,
            source,
            output,
        } => {
            let d = api
                .get(&format!("/api/label-designs/{}", enc(&design)))
                .await?;
            let src = d["source"].as_str().unwrap_or_default().to_string();
            if let Some(file) = output {
                std::fs::write(&file, &src).map_err(|e| {
                    Failure::new("FAILED", format!("could not write {}: {e}", file.display()))
                })?;
                let answer = json!({ "file": file.display().to_string(), "design": d["id"], "version": d["shown"] });
                out.emit(&answer, |a| {
                    format!("wrote {}", a["file"].as_str().unwrap_or("?"))
                });
                return Ok(code::OK);
            }
            if source {
                print!("{src}");
                return Ok(code::OK);
            }
            out.emit(&d, |d| {
                let mut lines = vec![format!(
                    "{} ({}, version {} of {}): {}",
                    d["id"].as_str().unwrap_or("?"),
                    d["kind"].as_str().unwrap_or("saved"),
                    d["shown"],
                    d["version"],
                    d["name"].as_str().unwrap_or("")
                )];
                if let Some(desc) = d["description"].as_str() {
                    lines.push(desc.to_string());
                }
                for v in d["versions"].as_array().into_iter().flatten() {
                    lines.push(format!(
                        "  v{} {} by {}{}",
                        v["version"],
                        v["created_at"].as_str().unwrap_or(""),
                        v["created_by"].as_str().unwrap_or("?"),
                        v["note"]
                            .as_str()
                            .map(|n| format!(": {n}"))
                            .unwrap_or_default()
                    ));
                }
                lines.push("--source prints the source; -o FILE writes it".into());
                lines.join("\n")
            });
            Ok(code::OK)
        }
        DesignCmd::Add {
            id,
            file,
            name,
            description,
            note,
            sheet,
        } => {
            let body = json!({ "id": id, "source": read_file(&file)?, "name": name,
                "description": description, "note": note, "sheet": sheet });
            let saved = api
                .call(Method::POST, "/api/label-designs", Some(&strip_nulls(body)))
                .await?;
            out.emit(&saved, saved_text);
            Ok(code::OK)
        }
        DesignCmd::Edit {
            id,
            file,
            name,
            description,
            note,
            sheet,
        } => {
            let source = match &file {
                Some(f) => Some(read_file(f)?),
                None => None,
            };
            let body = json!({ "source": source, "name": name, "description": description,
                "note": note, "sheet": sheet });
            let saved = api
                .call(
                    Method::PUT,
                    &format!("/api/label-designs/{}", enc(&id)),
                    Some(&strip_nulls(body)),
                )
                .await?;
            out.emit(&saved, saved_text);
            Ok(code::OK)
        }
        DesignCmd::Rollback { id, to, note } => {
            let body = strip_nulls(json!({ "version": to, "note": note }));
            let saved = api
                .call(
                    Method::POST,
                    &format!("/api/label-designs/{}/rollback", enc(&id)),
                    Some(&body),
                )
                .await?;
            out.emit(&saved, saved_text);
            Ok(code::OK)
        }
        DesignCmd::Use {
            design,
            bean,
            set,
            brand,
            clear,
        } => {
            let b = api.bean(&bean).await?;
            let id = b["id"].as_str().unwrap_or_default().to_string();
            let body = if clear {
                json!({ "params": {} })
            } else {
                strip_nulls(json!({ "design": design, "params": params(&set)?, "brand": brand }))
            };
            let l = api
                .call(
                    Method::PUT,
                    &format!("/api/beans/{}/label", enc(&id)),
                    Some(&body),
                )
                .await?;
            let name = b["name"].as_str().unwrap_or("?").to_string();
            out.emit(&l, |l| match l["design"].as_str() {
                Some(d) => format!(
                    "{name}'s labels: {d}{}",
                    l["brand"]
                        .as_str()
                        .map(|b| format!(", brand {b}"))
                        .unwrap_or_default()
                ),
                None => format!("{name} has no label of its own"),
            });
            Ok(code::OK)
        }
        DesignCmd::Rm { id, wait } => {
            let reply = api
                .send(
                    Method::DELETE,
                    &format!("/api/label-designs/{}", enc(&id)),
                    None,
                    Duration::from_secs(60),
                )
                .await?;
            held_or_done(api, out, reply, wait, move |_| format!("deleted {id}")).await
        }
    }
}

/// A saved design, for a person: its version, and the save's check
fn saved_text(s: &Value) -> String {
    let d = &s["design"];
    let mut lines = vec![format!(
        "saved {}@{}: {}",
        d["id"].as_str().unwrap_or("?"),
        d["version"],
        d["name"].as_str().unwrap_or("")
    )];
    if let Some(issues) = s["report"]["issues"].as_array() {
        if issues.is_empty() {
            lines.push("check: ok".into());
        }
        for i in issues {
            lines.push(format!(
                "  {}: {}",
                i["kind"].as_str().unwrap_or("?"),
                i["message"].as_str().unwrap_or("")
            ));
        }
    }
    lines.join("\n")
}

/// `null`s out of a body, so absent flags are absent fields
fn strip_nulls(mut v: Value) -> Value {
    if let Some(o) = v.as_object_mut() {
        o.retain(|_, x| !x.is_null());
    }
    v
}

pub async fn asset(api: &Api, out: &Out, cmd: AssetCmd) -> Result<u8, Failure> {
    match cmd {
        AssetCmd::List => {
            let list = api.get("/api/label-assets").await?;
            out.emit(&list, |l| {
                l.as_array()
                    .into_iter()
                    .flatten()
                    .map(|a| {
                        format!(
                            "{:<28} {:<5} {:>8} bytes{}",
                            a["name"].as_str().unwrap_or("?"),
                            a["kind"].as_str().unwrap_or(""),
                            a["size"],
                            a["family"]
                                .as_str()
                                .map(|f| format!("  family {f}"))
                                .unwrap_or_default()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
            Ok(code::OK)
        }
        AssetCmd::Add { file, name, note } => {
            let bytes = std::fs::read(&file).map_err(|e| {
                Failure::new("NO_FILE", format!("could not read {}: {e}", file.display()))
            })?;
            let name = name.unwrap_or_else(|| {
                file.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("asset")
                    .to_string()
            });
            let mut path = format!("/api/label-assets/{}", enc(&name));
            if let Some(n) = &note {
                let _ = write!(path, "?note={}", enc(n));
            }
            let (body, _) = api
                .transfer(
                    Method::PUT,
                    &path,
                    Upload::Raw(&bytes, "application/octet-stream"),
                )
                .await?;
            let a: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
            out.emit(&a, |a| {
                format!(
                    "{} ({}, {} bytes){}",
                    a["name"].as_str().unwrap_or("?"),
                    a["kind"].as_str().unwrap_or("?"),
                    a["size"],
                    a["family"]
                        .as_str()
                        .map(|f| format!(": font family \"{f}\""))
                        .unwrap_or_default()
                )
            });
            Ok(code::OK)
        }
        AssetCmd::Get { name, output } => {
            let path = format!("/api/label-assets/{}", enc(&name));
            save_file(api, out, &path, output, &name).await
        }
        AssetCmd::Rm { name, wait } => {
            let reply = api
                .send(
                    Method::DELETE,
                    &format!("/api/label-assets/{}", enc(&name)),
                    None,
                    Duration::from_secs(60),
                )
                .await?;
            held_or_done(api, out, reply, wait, move |_| format!("deleted {name}")).await
        }
    }
}

pub async fn brand(api: &Api, out: &Out, cmd: BrandCmd) -> Result<u8, Failure> {
    let text = |b: &Value| {
        let mut lines = vec![format!(
            "{}: {}",
            b["id"].as_str().unwrap_or("?"),
            b["name"].as_str().unwrap_or("")
        )];
        for k in [
            "tagline",
            "place",
            "website",
            "logo",
            "ink",
            "accent",
            "paper",
            "font_display",
            "font_body",
            "font_script",
        ] {
            if let Some(v) = b[k].as_str() {
                lines.push(format!("  {k}: {v}"));
            }
        }
        lines.join("\n")
    };
    match cmd {
        BrandCmd::List => {
            let list = api.get("/api/label-brands").await?;
            out.emit(&list, |l| {
                l.as_array()
                    .into_iter()
                    .flatten()
                    .map(text)
                    .collect::<Vec<_>>()
                    .join("\n")
            });
            Ok(code::OK)
        }
        BrandCmd::Show { id } => {
            let b = api.get(&format!("/api/label-brands/{}", enc(&id))).await?;
            out.emit(&b, text);
            Ok(code::OK)
        }
        BrandCmd::Set {
            id,
            name,
            tagline,
            place,
            website,
            logo,
            ink,
            accent,
            paper,
            font_display,
            font_body,
            font_script,
        } => {
            // The flags given change; the rest is kept
            let mut b = match api.get(&format!("/api/label-brands/{}", enc(&id))).await {
                Ok(b) => b,
                Err(f) if f.code == "NOT_FOUND" => json!({}),
                Err(f) => return Err(f),
            };
            for (k, v) in [
                ("name", name),
                ("tagline", tagline),
                ("place", place),
                ("website", website),
                ("logo", logo),
                ("ink", ink),
                ("accent", accent),
                ("paper", paper),
                ("font_display", font_display),
                ("font_body", font_body),
                ("font_script", font_script),
            ] {
                if let Some(v) = v {
                    b[k] = if v.is_empty() { Value::Null } else { json!(v) };
                }
            }
            if b["name"].as_str().is_none_or(str::is_empty) {
                return Err(Failure::new("USAGE", "a new brand needs --name"));
            }
            let saved = api
                .call(
                    Method::PUT,
                    &format!("/api/label-brands/{}", enc(&id)),
                    Some(&strip_nulls(b)),
                )
                .await?;
            out.emit(&saved, text);
            Ok(code::OK)
        }
        BrandCmd::Rm { id, wait } => {
            let reply = api
                .send(
                    Method::DELETE,
                    &format!("/api/label-brands/{}", enc(&id)),
                    None,
                    Duration::from_secs(60),
                )
                .await?;
            held_or_done(api, out, reply, wait, move |_| format!("deleted {id}")).await
        }
    }
}
