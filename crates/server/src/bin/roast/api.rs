//! The Roastwarden API from `roast`: requests with the token,
//! the error body (`docs/control.md`, "Errors") turned into a [`Failure`], and lookups of a
//! bean, profile or roaster by id or by name.

use reqwest::Method;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::fmt::Write as _;
use std::time::Duration;

use crate::out::Failure;

pub struct Api {
    base: String,
    token: Option<String>,
    http: reqwest::Client,
}

/// What came back: the status and the body (Null when empty, a string when not JSON).
pub struct Reply {
    pub status: u16,
    pub body: Value,
}

/// What a [`Api::transfer`] sends.
pub enum Upload<'a> {
    None,
    Json(&'a Value),
    /// The bytes and their content type
    Raw(&'a [u8], &'a str),
}

impl Api {
    pub fn new(url: &str, token: Option<String>) -> Self {
        Self {
            base: url.trim_end_matches('/').to_string(),
            token: token.filter(|t| !t.trim().is_empty()),
            http: reqwest::Client::new(),
        }
    }

    /// One call to `{url}{path}`. The token goes with every call when there is one: reads
    /// are open, but a bad token is refused even for a read, which is better found early.
    pub async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        wait: Duration,
    ) -> Result<Reply, Failure> {
        let url = format!("{}{path}", self.base);
        let mut req = self.http.request(method, &url).timeout(wait);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        if let Some(b) = body {
            req = req.json(b);
        }
        let response = req.send().await.map_err(|e| {
            Failure::new(
                "UNREACHABLE",
                format!("Could not reach Roastwarden at {}: {e}", self.base),
            )
        })?;
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        let body: Value = if text.trim().is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap_or(Value::String(text))
        };
        if status >= 400 {
            return Err(failure_from(status, &body));
        }
        Ok(Reply { status, body })
    }

    pub async fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, Failure> {
        Ok(self
            .send(method, path, body, Duration::from_secs(60))
            .await?
            .body)
    }

    pub async fn get(&self, path: &str) -> Result<Value, Failure> {
        self.call(Method::GET, path, None).await
    }

    /// A file the server makes (an export, a PDF): its bytes and the name it suggests.
    pub async fn download(&self, path: &str) -> Result<(Vec<u8>, Option<String>), Failure> {
        self.transfer(Method::GET, path, Upload::None).await
    }

    /// A call that sends or answers bytes rather than JSON: a PDF from a JSON body, a file
    /// sent as it is (an asset). Answers the bytes and the file name the server suggests.
    pub async fn transfer(
        &self,
        method: Method,
        path: &str,
        upload: Upload<'_>,
    ) -> Result<(Vec<u8>, Option<String>), Failure> {
        let (bytes, name, _) = self.transfer_with_headers(method, path, upload).await?;
        Ok((bytes, name))
    }

    /// The host name or address of the server `roast` calls (`192.168.1.50`), if it has one.
    pub fn host(&self) -> Option<String> {
        reqwest::Url::parse(&self.base)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
    }

    /// [`Api::transfer`], with the response's headers.
    pub async fn transfer_with_headers(
        &self,
        method: Method,
        path: &str,
        upload: Upload<'_>,
    ) -> Result<(Vec<u8>, Option<String>, reqwest::header::HeaderMap), Failure> {
        let url = format!("{}{path}", self.base);
        let mut req = self
            .http
            .request(method, &url)
            .timeout(Duration::from_secs(120));
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        req = match upload {
            Upload::None => req,
            Upload::Json(v) => req.json(v),
            Upload::Raw(bytes, ty) => req
                .header(reqwest::header::CONTENT_TYPE, ty)
                .body(bytes.to_vec()),
        };
        let response = req.send().await.map_err(|e| {
            Failure::new(
                "UNREACHABLE",
                format!("Could not reach Roastwarden at {}: {e}", self.base),
            )
        })?;
        let status = response.status().as_u16();
        let name = response
            .headers()
            .get(reqwest::header::CONTENT_DISPOSITION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split("filename=").nth(1))
            .map(|n| n.trim().trim_matches('"').to_string())
            .filter(|n| !n.is_empty());
        let headers = response.headers().clone();
        let bytes = response.bytes().await.unwrap_or_default().to_vec();
        if status >= 400 {
            let text = String::from_utf8_lossy(&bytes);
            let body = serde_json::from_str(&text).unwrap_or(Value::String(text.to_string()));
            return Err(failure_from(status, &body));
        }
        Ok((bytes, name, headers))
    }

    /// A bean by id or by name (ignoring case), archived ones included.
    pub async fn bean(&self, reference: &str) -> Result<Value, Failure> {
        let list = self.get("/api/beans?archived=true").await?;
        pick(list, reference, "bean", "name")
    }

    /// A profile by id or by name (ignoring case).
    pub async fn profile(&self, reference: &str) -> Result<Value, Failure> {
        // Every profile: one owner, so "private" (the default for a created one) hides nothing
        let list = self.get("/api/profiles?include_private=true").await?;
        pick(list, reference, "profile", "name")
    }

    /// A roaster by roaster id (`esp32_roaster_01`) or by its name in Roasters.
    /// Set up in Roasters, or only heard on MQTT (the virtual roaster often is).
    pub async fn roaster(&self, reference: &str) -> Result<Value, Failure> {
        let list = self.get("/api/devices").await?;
        let mut items: Vec<Value> = list
            .as_array()
            .into_iter()
            .flatten()
            .map(|d| {
                let mut d = d.clone();
                // A device's `id` is its registry id; people and agents use `device_id`
                d["registry_id"] = d["id"].clone();
                d["id"] = d["device_id"].clone();
                d
            })
            .collect();
        let heard = self
            .get("/api/devices/registry")
            .await
            .unwrap_or(Value::Null);
        // The simulator's id is fixed: valid whether it runs or not
        let ids = heard["devices"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|d| d["device_id"].as_str())
            .chain(["virtual_roaster"]);
        for id in ids {
            if !items.iter().any(|i| i["id"] == id) {
                items.push(json!({ "id": id, "device_id": id, "name": id }));
            }
        }
        pick(Value::Array(items), reference, "roaster", "name")
    }
}

/// Read a body into a type roast reads fields from.
pub fn decode<T: DeserializeOwned>(body: Value) -> Result<T, Failure> {
    serde_json::from_value(body.clone()).map_err(|e| Failure {
        details: Some(Box::new(body)),
        ..Failure::new(
            "BAD_RESPONSE",
            format!(
                "Roastwarden answered something roast does not understand ({e}); are the two the same version?"
            ),
        )
    })
}

/// The API's error body (`{success: false, error: {code, message, details}}`) as a failure;
/// anything else (a proxy's page, plain text) gets a code from the status.
fn failure_from(status: u16, body: &Value) -> Failure {
    let (code, message, details) = match body.get("error") {
        Some(Value::Object(e)) => (
            e.get("code")
                .and_then(Value::as_str)
                .unwrap_or_else(|| default_code(status))
                .to_string(),
            e.get("message")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            e.get("details").cloned().map(Box::new),
        ),
        _ => (
            default_code(status).to_string(),
            match body {
                Value::String(s) if !s.trim().is_empty() => s.chars().take(300).collect::<String>(),
                _ => format!("Roastwarden answered {status}"),
            },
            None,
        ),
    };
    Failure {
        code,
        message,
        status: Some(status),
        details,
    }
}

fn default_code(status: u16) -> &'static str {
    match status {
        400 => "BAD_REQUEST",
        401 => "UNAUTHORIZED",
        403 => "FORBIDDEN",
        404 => "NOT_FOUND",
        409 => "CONFLICT",
        422 => "UNPROCESSABLE",
        429 => "TOO_MANY_FAILURES",
        503 => "UNAVAILABLE",
        s if s >= 500 => "INTERNAL_ERROR",
        _ => "ERROR",
    }
}

/// The item of `list` whose `id` is `reference`, else the one whose `name_key` is, ignoring
/// case. No match is a NOT_FOUND that lists the names there are; two are AMBIGUOUS.
pub fn pick(list: Value, reference: &str, kind: &str, name_key: &str) -> Result<Value, Failure> {
    let reference = reference.trim();
    let items = match list {
        Value::Array(items) => items,
        other => {
            return Err(Failure {
                details: Some(Box::new(other)),
                ..Failure::new("BAD_RESPONSE", format!("expected a list of {kind}s"))
            })
        }
    };
    let id_of = |v: &Value| match &v["id"] {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    };
    let name_of = |v: &Value| v[name_key].as_str().unwrap_or("").to_string();
    if let Some(found) = items.iter().find(|v| id_of(v) == reference) {
        return Ok(found.clone());
    }
    let mut found: Vec<&Value> = items
        .iter()
        .filter(|v| name_of(v).eq_ignore_ascii_case(reference))
        .collect();
    match found.len() {
        1 => Ok(found.remove(0).clone()),
        0 => Err(Failure {
            status: Some(404),
            details: Some(Box::new(json!({
                "known": items.iter().map(|v| json!({ "id": id_of(v), "name": name_of(v) })).collect::<Vec<_>>()
            }))),
            ..Failure::new(
                "NOT_FOUND",
                format!("No {kind} with the id or name {reference:?}"),
            )
        }),
        _ => Err(Failure {
            details: Some(Box::new(json!({
                "ids": found.iter().map(|v| id_of(v)).collect::<Vec<_>>()
            }))),
            ..Failure::new(
                "AMBIGUOUS",
                format!("More than one {kind} is called {reference:?}"),
            )
        }),
    }
}

/// Percent-encode a query value.
pub fn enc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_error_body_and_plain_text_become_failures() {
        let f = failure_from(
            403,
            &json!({ "success": false, "error": { "code": "REAL_HARDWARE", "message": "m" } }),
        );
        assert_eq!((f.code.as_str(), f.status), ("REAL_HARDWARE", Some(403)));
        let f = failure_from(502, &json!("Bad Gateway"));
        assert_eq!(
            (f.code.as_str(), f.message.as_str()),
            ("INTERNAL_ERROR", "Bad Gateway")
        );
    }

    #[test]
    fn references_are_ids_or_names() {
        let items = json!([
            { "id": "a1", "name": "Ethiopia Guji" },
            { "id": "b2", "name": "Brazil" },
            { "id": "c3", "name": "brazil" },
        ]);
        let p = |r: &str| pick(items.clone(), r, "bean", "name");
        assert_eq!(p("a1").unwrap()["id"], "a1");
        assert_eq!(p("ethiopia guji").unwrap()["id"], "a1");
        assert_eq!(p("Brazil").unwrap_err().code, "AMBIGUOUS");
        assert_eq!(p("c3").unwrap()["id"], "c3");
        let missing = p("Kenya").unwrap_err();
        assert_eq!(missing.code, "NOT_FOUND");
        assert_eq!(
            missing.details.unwrap()["known"][0]["name"],
            "Ethiopia Guji"
        );
    }

    #[test]
    fn query_values_are_encoded() {
        assert_eq!(enc("a b/ü"), "a%20b%2F%C3%BC");
    }
}
