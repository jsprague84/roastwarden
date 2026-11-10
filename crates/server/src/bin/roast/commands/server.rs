//! The server itself: its backups, the audit log, the API's document, any call to it and a
//! support bundle.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Duration;

use clap::Args;
use reqwest::Method;
use serde_json::{Value, json};

use crate::api::{self, Api, enc};
use crate::cli::Wait;
use crate::commands::approvals::held_or_done;
use crate::commands::records;
use crate::out::{Failure, Out, code, for_finding};
use crate::surface;
use crate::text::{AuditRow, bytes};

/// The OpenAPI document this roast was built with (`openapi --offline`)
pub const OPENAPI: &str = include_str!("../../../../../../docs/openapi.json");

/// `roast openapi`: the API's document, or the operations matching --find.
pub async fn openapi(
    api: &Api,
    out: &Out,
    offline: bool,
    find: Option<String>,
) -> Result<u8, Failure> {
    let doc = if offline {
        // The checked-in document, which a test keeps equal to the server's: building
        // it from the router would link the whole server (Typst included) into roast
        serde_json::from_str(OPENAPI).map_err(|e| Failure::new("FAILED", e.to_string()))?
    } else {
        api.get("/api-docs/openapi.json").await?
    };
    let Some(find) = find else {
        println!("{}", serde_json::to_string_pretty(&doc).unwrap_or_default());
        return Ok(code::OK);
    };
    let ops = find_operations(&doc, &find);
    out.emit(&ops, |ops| {
        if ops.is_empty() {
            return format!("No operation matches {find:?}");
        }
        ops.iter()
            .map(|o| {
                format!(
                    "{:<6} {:<48} {}\n       → {}",
                    o["method"].as_str().unwrap_or(""),
                    o["path"].as_str().unwrap_or(""),
                    o["summary"].as_str().unwrap_or(""),
                    o["roast"].as_str().unwrap_or("?")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    });
    Ok(code::OK)
}

/// The operations of an OpenAPI document whose path, operation id or summary contain `find`
/// (any case): method, path, id and summary, and what a request body is, by name.
pub fn find_operations(doc: &Value, find: &str) -> Vec<Value> {
    let needle = find.to_lowercase();
    let mut out = Vec::new();
    for (path, item) in doc["paths"].as_object().into_iter().flatten() {
        for method in ["get", "post", "put", "patch", "delete"] {
            let Some(op) = item.get(method) else { continue };
            let id = op["operationId"].as_str().unwrap_or("");
            let summary = op["summary"].as_str().unwrap_or("");
            let hay = format!("{path} {id} {summary}").to_lowercase();
            if !hay.contains(&needle) {
                continue;
            }
            let body = op["requestBody"]["content"]["application/json"]["schema"]["$ref"]
                .as_str()
                .and_then(|r| r.rsplit('/').next())
                .map(str::to_string);
            out.push(json!({
                "method": method.to_uppercase(),
                "path": path,
                "id": id,
                "summary": summary,
                "body": body,
                "roast": surface::how(method, path),
            }));
        }
    }
    out
}

#[derive(Args, Debug)]
pub struct AuditArgs {
    #[arg(long, default_value_t = 20)]
    pub limit: u32,
    /// Only this roaster
    #[arg(long)]
    pub roaster: Option<String>,
    /// Only these kinds, comma-separated: control, roast, profile, beans, labels, roaster,
    /// settings, access, backup, other
    #[arg(long)]
    pub kind: Option<String>,
    /// Text anywhere in the path, body, token name, user, roaster, address or client
    #[arg(long)]
    pub find: Option<String>,
    /// Only requests sent with the token of this name (not `--token`: that is the
    /// credential roast sends)
    #[arg(long = "by", value_name = "TOKEN_NAME")]
    pub by_token: Option<String>,
    /// Only refused or failed requests
    #[arg(long)]
    pub failed: bool,
}

/// `roast audit`: the audit log, newest first, filtered on the server.
pub async fn audit(api: &Api, out: &Out, a: AuditArgs) -> Result<u8, Failure> {
    let AuditArgs {
        limit,
        roaster,
        kind,
        find,
        by_token,
        failed,
    } = a;
    let mut path = format!("/api/audit?limit={}", limit.clamp(1, 500));
    for (key, value) in [
        ("device_id", roaster),
        ("kind", kind),
        ("q", find),
        ("token", by_token),
    ] {
        if let Some(v) = value {
            let _ = write!(path, "&{key}={}", enc(&v));
        }
    }
    if failed {
        path.push_str("&failed=true");
    }
    let mut list = api.get(&path).await?;
    // A request body is JSON held in a string: hand it over as JSON
    for item in list.as_array_mut().into_iter().flatten() {
        if let Some(parsed) = item["body"]
            .as_str()
            .and_then(|b| serde_json::from_str::<Value>(b).ok())
        {
            item["body"] = parsed;
        }
    }
    let rows: Vec<AuditRow> = api::decode(list.clone())?;
    if out.json {
        // The browser's user agent says nothing an agent needs
        out.emit(&for_finding(list, &["client"]), |_| String::new());
    } else {
        out.emit(&rows, |rows| {
            rows.iter()
                .map(|e| {
                    format!(
                        "{}  {:<8} {:<6} {:<44} {}  {}",
                        e.at.get(..16).unwrap_or(&e.at),
                        e.kind,
                        e.method,
                        e.path,
                        e.status,
                        e.token_name
                            .as_deref()
                            .map_or_else(|| e.actor.clone(), |t| format!("token {t}"))
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        });
    }
    Ok(code::OK)
}

/// `roast backups`: the copies and the last attempt; --now writes one, --check checks one.
pub async fn backups(
    api: &Api,
    out: &Out,
    now: bool,
    check: Option<String>,
) -> Result<u8, Failure> {
    if let Some(name) = check {
        let r = api
            .call(
                Method::POST,
                &format!("/api/backups/{}/check", enc(&name)),
                None,
            )
            .await?;
        out.emit(&r, |r| serde_json::to_string_pretty(r).unwrap_or_default());
        return Ok(code::OK);
    }
    if now {
        let created = api.call(Method::POST, "/api/backups", None).await?;
        out.emit(&created, |c| {
            format!(
                "wrote {} ({}){}",
                c["file"]["name"].as_str().unwrap_or("?"),
                bytes(c["file"]["bytes"].as_u64().unwrap_or(0)),
                match c["removed"].as_u64().unwrap_or(0) {
                    0 => String::new(),
                    n => format!(", removed {n} older"),
                }
            )
        });
        return Ok(code::OK);
    }
    let b = api.get("/api/backups").await?;
    out.emit(&b, |b| {
        let last = &b["last"];
        let last = if last.is_null() {
            "no backup yet".to_string()
        } else if let Some(e) = last["error"].as_str() {
            format!(
                "last attempt FAILED at {}: {e}",
                last["started_at"].as_str().unwrap_or("?")
            )
        } else {
            format!(
                "last {} ({}) {}",
                last["file_name"].as_str().unwrap_or("?"),
                last["trigger"].as_str().unwrap_or("?"),
                last["started_at"].as_str().unwrap_or("?")
            )
        };
        let files = b["files"].as_array().cloned().unwrap_or_default();
        let mut lines = vec![format!(
            "{}; {} copies kept in {} (keep {}), next {}",
            last,
            files.len(),
            b["dir"].as_str().unwrap_or("?"),
            b["keep"],
            b["next_run"].as_str().unwrap_or("off")
        )];
        if b["restore_staged"] == json!(true) {
            lines.push("a restore is staged: it goes in at the next start".into());
        }
        for f in files {
            lines.push(format!(
                "  {}  {}",
                f["name"].as_str().unwrap_or("?"),
                bytes(f["bytes"].as_u64().unwrap_or(0))
            ));
        }
        lines.join("\n")
    });
    Ok(code::OK)
}

#[derive(Args, Debug)]
pub struct ApiArgs {
    /// GET, POST, PUT, PATCH or DELETE
    pub method: String,
    /// The path, e.g. /api/sessions?limit=1
    pub path: String,
    /// A JSON body, or - to read it from stdin
    #[arg(long)]
    pub body: Option<String>,
    #[command(flatten)]
    pub wait: Wait,
}

/// `roast api`: any call with the token and the error handling; a held call waits with --wait.
pub async fn call(api: &Api, out: &Out, a: ApiArgs) -> Result<u8, Failure> {
    let ApiArgs {
        method,
        path,
        body,
        wait,
    } = a;
    let method = Method::from_bytes(method.to_ascii_uppercase().as_bytes())
        .map_err(|_| Failure::new("USAGE", format!("not an HTTP method: {method}")))?;
    let body = match body.as_deref() {
        None => None,
        Some("-") => {
            let mut text = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)
                .map_err(|e| Failure::new("USAGE", e.to_string()))?;
            Some(text)
        }
        Some(text) => Some(text.to_string()),
    };
    let body: Option<Value> = body
        .map(|t| {
            serde_json::from_str(&t)
                .map_err(|e| Failure::new("USAGE", format!("--body is not JSON: {e}")))
        })
        .transpose()?;
    let path = if path.starts_with('/') {
        path
    } else {
        format!("/{path}")
    };
    let reply = api
        .send(method, &path, body.as_ref(), Duration::from_secs(60))
        .await?;
    held_or_done(api, out, reply, wait, |b| match b {
        Value::Null => "done".into(),
        Value::String(s) => s.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    })
    .await
}

/// `roast support-bundle`: the bundle, written to a file (admin).
pub async fn support_bundle(api: &Api, out: &Out, output: Option<PathBuf>) -> Result<u8, Failure> {
    records::save_file(
        api,
        out,
        "/api/support-bundle",
        output,
        "roastwarden-support.json",
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn operations_are_found_by_path_id_or_summary() {
        let doc = json!({ "paths": {
            "/api/plan": {
                "get": { "operationId": "list_plans", "summary": "The queue" },
                "post": { "operationId": "create_plan", "summary": "Plan a roast",
                          "requestBody": { "content": { "application/json": { "schema": { "$ref": "#/components/schemas/PlanRequest" } } } } }
            },
            "/api/beans": { "get": { "operationId": "list_beans", "summary": "The green beans" } }
        }});
        let found = find_operations(&doc, "PLAN");
        assert_eq!(found.len(), 2);
        assert_eq!(found[1]["body"], "PlanRequest");
        assert_eq!(find_operations(&doc, "green").len(), 1);
        assert_eq!(
            find_operations(&doc, "nothing"),
            [] as [serde_json::Value; 0]
        );
    }
}
