//! The roast plan: the queue of planned roasts, one added, changed, moved or taken off.

use std::fmt::Write as _;

use clap::Subcommand;
use reqwest::Method;
use serde_json::json;

use crate::api::{self, enc, Api};
use crate::cli::Wait;
use crate::commands::records;
use crate::out::{self, code, for_finding, Failure, Out};
use crate::text::{parse_weight, plan_line, PlanRow};

#[derive(Subcommand, Debug)]
pub enum PlanCommand {
    /// Change a planned roast: bean, weight, profile, roaster (`any`), name, notes
    Edit(records::PlanEditArgs),
    /// Take a roast off the plan
    Rm {
        id: String,
        #[command(flatten)]
        wait: Wait,
    },
    /// Move a planned roast in the queue: --to 1 makes it next
    Move {
        id: String,
        #[arg(long)]
        to: usize,
    },
    /// Plan a roast; it joins the end of the queue
    Add {
        /// The bean's id or name
        #[arg(long)]
        bean: Option<String>,
        /// Green weight, e.g. 100g
        #[arg(long, value_parser = parse_weight)]
        weight: Option<f64>,
        /// The profile's id or name
        #[arg(long)]
        profile: Option<String>,
        /// The roaster's id or name (default: any roaster)
        #[arg(long)]
        roaster: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        notes: Option<String>,
    },
}

/// `roast plan`: the queue, or a subcommand on it.
pub async fn run(
    api: &Api,
    out: &Out,
    add: Option<PlanCommand>,
    roaster: Option<String>,
    done: bool,
) -> Result<u8, Failure> {
    match add {
        Some(PlanCommand::Edit(a)) => records::plan_edit(api, out, a).await,
        Some(PlanCommand::Rm { id, wait }) => records::plan_rm(api, out, id, wait).await,
        Some(PlanCommand::Move { id, to }) => records::plan_move(api, out, id, to).await,
        Some(PlanCommand::Add {
            bean,
            weight,
            profile,
            roaster,
            name,
            notes,
        }) => {
            let mut body = json!({ "green_weight_g": weight, "name": name, "notes": notes });
            if let Some(b) = bean {
                body["bean_id"] = api.bean(&b).await?["id"].clone();
            }
            if let Some(p) = profile {
                body["profile_id"] = api.profile(&p).await?["id"].clone();
            }
            if let Some(r) = roaster {
                body["device_id"] = api.roaster(&r).await?["device_id"].clone();
            }
            out::slim(&mut body);
            let plan = api.call(Method::POST, "/api/plan", Some(&body)).await?;
            let row: PlanRow = api::decode(plan.clone())?;
            out.emit(&plan, |_| format!("planned: {}", plan_line(&row)));
            Ok(code::OK)
        }
        None => {
            let mut path = format!("/api/plan?done={done}");
            if let Some(r) = roaster {
                let _ = write!(path, "&device_id={}", enc(&r));
            }
            let list = api.get(&path).await?;
            let rows: Vec<PlanRow> = api::decode(list.clone())?;
            if out.json {
                out.emit(&for_finding(list, &["created_at", "updated_at"]), |_| {
                    String::new()
                });
            } else {
                out.emit(&rows, |rows| {
                    if rows.is_empty() {
                        return "Nothing planned".into();
                    }
                    rows.iter()
                        .enumerate()
                        .map(|(i, p)| format!("{:>2}. {}", i + 1, plan_line(p)))
                        .collect::<Vec<_>>()
                        .join("\n")
                });
            }
            Ok(code::OK)
        }
    }
}
