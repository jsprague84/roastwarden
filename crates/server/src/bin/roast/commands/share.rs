//! `roast share`: a roast's public share page, what a bag's QR code opens; its links listed and
//! revoked.

use std::fmt::Write as _;

use clap::{Args, Subcommand};
use reqwest::Method;
use serde_json::{json, Value};

use crate::api::{enc, Api};
use crate::commands::labels::tz;
use crate::commands::roasts::roast_id;
use crate::out::{code, Failure, Out};

#[derive(Args, Debug)]
#[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
pub struct ShareCmd {
    #[command(subcommand)]
    pub sub: Option<ShareSub>,
    #[command(flatten)]
    pub args: ShareArgs,
}

#[derive(Subcommand, Debug)]
pub enum ShareSub {
    /// Every share link, newest first: the coffee, the address, how often it was opened
    List {
        /// The revoked links too
        #[arg(long)]
        all: bool,
        /// Only this roast's (an id or `latest`)
        #[arg(long)]
        roast: Option<String>,
    },
    /// Stop a link: its page is a 404 from now on, as for a code that never was. Bags already
    /// printed with it open nothing; `roast share ROAST` makes a new one
    Revoke {
        /// The link's code (the 25 characters after /R/), or its address
        code: String,
    },
}

#[derive(Args, Debug)]
pub struct ShareArgs {
    /// The roast: its id, or `latest` (--roaster narrows it)
    #[arg(required = true)]
    pub roast: Option<String>,
    /// The line for the person it is for, on the page ("Happy birthday, Sam"); "" clears it
    #[arg(long)]
    pub message: Option<String>,
    /// Another link even when the roast has one (for someone else, with their own message)
    #[arg(long)]
    pub new: bool,
    /// With `latest`: the newest roast on this roaster
    #[arg(long)]
    pub roaster: Option<String>,
}

/// "Kenya Nyeri AA (batch #5): https://…/R/CODE" and how it has been used
fn share_line(l: &Value) -> String {
    let mut s = format!(
        "{}{}: {}",
        l["name"].as_str().unwrap_or("?"),
        l["batch"]
            .as_i64()
            .map(|b| format!(" (batch #{b})"))
            .unwrap_or_default(),
        l["url"].as_str().unwrap_or("?")
    );
    if let Some(m) = l["message"].as_str() {
        let _ = write!(s, "\n  message: {m:?}");
    }
    let scans = l["scans"].as_i64().unwrap_or(0);
    let _ = write!(
        s,
        "\n  opened {} · made {} by {}",
        match scans {
            0 => "never".to_string(),
            1 => "once".to_string(),
            n => format!("{n} times"),
        },
        l["created_at"].as_str().unwrap_or("?"),
        l["created_by"].as_str().unwrap_or("?")
    );
    if let Some(r) = l["revoked_at"].as_str() {
        let _ = write!(s, " · revoked {r}");
    }
    s
}

pub async fn share(api: &Api, out: &Out, cmd: ShareCmd) -> Result<u8, Failure> {
    match cmd.sub {
        Some(ShareSub::List { all, roast }) => {
            let list = match roast {
                Some(r) => {
                    let id = roast_id(api, &r, None).await?;
                    let mut l = api
                        .get(&format!("/api/sessions/{}/share", enc(&id)))
                        .await?;
                    if !all
                        && let Some(a) = l.as_array_mut() {
                            a.retain(|x| x["revoked_at"].is_null());
                        }
                    l
                }
                None => api.get(&format!("/api/share?all={all}")).await?,
            };
            out.emit(&list, |l| {
                let lines: Vec<String> =
                    l.as_array().into_iter().flatten().map(share_line).collect();
                if lines.is_empty() {
                    "no share links".into()
                } else {
                    lines.join("\n")
                }
            });
            Ok(code::OK)
        }
        Some(ShareSub::Revoke { code: c }) => {
            let c = c.trim().trim_end_matches('/');
            let c = c.rsplit('/').next().unwrap_or(c);
            let l = api
                .call(Method::DELETE, &format!("/api/share/{}", enc(c)), None)
                .await?;
            out.emit(&l, |l| format!("revoked: {}", share_line(l)));
            Ok(code::OK)
        }
        None => {
            let a = cmd.args;
            let reference = a.roast.unwrap_or_else(|| "latest".into());
            let id = roast_id(api, &reference, a.roaster.as_deref()).await?;
            let mut b = json!({ "new": a.new, "tz": tz() });
            if let Some(m) = &a.message {
                b["message"] = json!(m);
            }
            let l = api
                .call(
                    Method::POST,
                    &format!("/api/sessions/{}/share", enc(&id)),
                    Some(&b),
                )
                .await?;
            out.emit(&l, share_line);
            Ok(code::OK)
        }
    }
}
