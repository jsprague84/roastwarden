//! A call Roastwarden holds for the owner's approval: printed with the exit code that says a
//! human must act, or waited for with `--wait`; and `roast approval`.

use std::time::Duration;

use reqwest::Method;
use serde_json::Value;

use crate::api::{self, Api, enc};
use crate::cli::Wait;
use crate::out::{Failure, Out, code};

/// The answer of a call Roastwarden may hold for the owner's approval (202). Without --wait the
/// held request is printed and the exit code says a human must act; with it, roast waits for
/// the answer and prints the decided request (its `result` is what the call answered).
pub async fn held_or_done(
    api: &Api,
    out: &Out,
    reply: api::Reply,
    wait: Wait,
    human: impl FnOnce(&Value) -> String,
) -> Result<u8, Failure> {
    if reply.status != 202 || reply.body.get("approval").is_none() {
        out.emit(&reply.body, human);
        return Ok(code::OK);
    }
    let held = reply.body;
    if !wait.wait {
        out.emit(&held, |h| {
            format!(
                "Waiting for the owner's approval: {}. They approve or deny it at {}\nRun again with --wait, or `roast approval {} --wait`, to wait for the answer.",
                h["approval"]["summary"].as_str().unwrap_or("?"),
                h["approval"]["url"].as_str().unwrap_or("?"),
                h["approval"]["id"].as_str().unwrap_or("?"),
            )
        });
        return Ok(code::NEEDS_HUMAN);
    }
    if !out.json {
        eprintln!(
            "Waiting for the owner's approval of \"{}\" ({})…",
            held["approval"]["summary"].as_str().unwrap_or("?"),
            held["approval"]["url"].as_str().unwrap_or("?")
        );
    }
    let decided = wait_for(api, held["approval"].clone(), wait).await?;
    out.emit(&decided, approval_text);
    Ok(approval_code(&decided))
}

/// Poll an approval (each poll waits up to 25 s on the server) until it is decided or the
/// wait runs out.
pub async fn wait_for(api: &Api, mut a: Value, wait: Wait) -> Result<Value, Failure> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(wait.wait_timeout);
    while matches!(a["status"].as_str(), Some("pending" | "approved"))
        && tokio::time::Instant::now() < deadline
    {
        let id = a["id"].as_str().unwrap_or_default().to_string();
        a = api
            .send(
                Method::GET,
                &format!("/api/approvals/{}?wait=25", enc(&id)),
                None,
                Duration::from_secs(40),
            )
            .await?
            .body;
    }
    Ok(a)
}

pub fn approval_code(a: &Value) -> u8 {
    match a["status"].as_str() {
        Some("done") => code::OK,
        Some("pending" | "approved") => code::NEEDS_HUMAN,
        _ => code::FAILED,
    }
}

pub fn approval_text(a: &Value) -> String {
    let summary = a["summary"].as_str().unwrap_or("?");
    match a["status"].as_str() {
        Some("done") => format!("Approved and done: {summary}"),
        Some("denied") => format!("Denied: {summary}. Nothing was done."),
        Some("expired") => format!("Expired without an answer: {summary}. Nothing was done."),
        Some("failed") => format!(
            "Approved, but the call failed ({}): {summary}",
            a["result_status"].as_i64().unwrap_or(0)
        ),
        _ => format!(
            "Still waiting for the owner: {summary} ({})",
            a["url"].as_str().unwrap_or("?")
        ),
    }
}

/// `roast approval`: one approval request, waited for with --wait.
pub async fn approval(api: &Api, out: &Out, id: String, wait: Wait) -> Result<u8, Failure> {
    let a = api.get(&format!("/api/approvals/{}", enc(&id))).await?;
    let a = if wait.wait {
        wait_for(api, a, wait).await?
    } else {
        a
    };
    out.emit(&a, approval_text);
    Ok(approval_code(&a))
}
