//! `roast` — the command line for Roastwarden, built for agents (and fine for people).
//!
//! Remote commands talk to a running server (`ROASTWARDEN_URL`) with an API token
//! (`ROASTWARDEN_TOKEN`; the owner key only when passed on purpose with `--token`):
//! status, roasts, beans and stock, the roast plan, profiles, the audit log, backups. They
//! never prompt, answer one JSON document when stdout is not a terminal (or with `--json`),
//! and exit with codes an agent can branch on (`out::code`). `backup` and `restore` run next
//! to the database instead (`docker exec roastwarden roast backup`); `health` needs no token
//! and is the container's healthcheck.

#![allow(
    clippy::doc_markdown,
    clippy::doc_link_with_quotes,
    reason = "clap prints these doc comments as `roast --help`: plain text, not Markdown"
)]

mod api;
mod cli;
mod commands;
mod local;
mod out;
mod setup;
mod surface;
mod text;

use std::process::ExitCode;

use clap::Parser;

use api::Api;
use cli::{Cli, Command, SkillCommand};
use commands::settings::SettingsCommand;
use commands::{
    analysis, approvals, beans, labels, orders, plans, profiles, records, reports, roasted,
    roasters, roasts, server, settings, share, status,
};
use out::{code, Failure, Out};

fn main() -> ExitCode {
    // RUSTROAST_URL and RUSTROAST_TOKEN from before the rename still work, with a note. Adopted
    // here, before the runtime starts its threads: the environment is written while nothing
    // else can read it.
    let adopted = roastwarden_server::legacy::adopt_env();
    if let Some(w) = roastwarden_server::legacy::env_warning(&adopted) {
        eprintln!("roast: {w}");
    }
    let cli = Cli::parse();
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("the async runtime")
        .block_on(roast(cli))
}

/// The command run: the local ones here, the rest through [`run`].
async fn roast(cli: Cli) -> ExitCode {
    let out = Out::new(cli.json, cli.text);
    roastwarden_server::install_crypto_provider();
    // Never the owner key from ROASTWARDEN_API_KEY on its own: inside the container that would
    // let anything that can `docker exec` act as the owner by accident (an agent under test did).
    // A flag or the environment first, then what `roast login` saved, then this machine.
    let saved = setup::load();
    let url = cli
        .url
        .clone()
        .or_else(|| saved.url.clone())
        .unwrap_or_else(|| setup::DEFAULT_URL.to_string());
    let token = cli.token.clone().or_else(|| saved.token.clone());
    let api = Api::new(&url, token);
    let result = match cli.command {
        Command::Login { address } => {
            let address = address.or(cli.url.clone()).or(saved.url.clone());
            match setup::login(&out, address, cli.token.clone()).await {
                Ok(code) => code,
                Err(f) => out.fail(&f),
            }
        }
        Command::Logout => local_code(&out, setup::logout(&out)),
        Command::Skill(SkillCommand::Install { agent, dir }) => {
            let server = cli.url.clone().or(saved.url.clone());
            let kept = cli.url.is_none() && saved.token.is_some();
            local_code(
                &out,
                setup::install(&out, agent, dir, server.as_deref(), kept),
            )
        }
        Command::Skill(SkillCommand::Show { label_design }) => {
            local_code(&out, setup::show(label_design))
        }
        Command::Backup { db } => local_result(&out, local::backup(&out, &db).await),
        Command::Restore { file, check, db } => {
            local_result(&out, local::restore(&out, &db, &file, check).await)
        }
        command => match run(&api, &out, command).await {
            Ok(code) => code,
            Err(f) => out.fail(&f),
        },
    };
    ExitCode::from(result)
}

fn local_code(out: &Out, r: Result<u8, Failure>) -> u8 {
    r.unwrap_or_else(|f| out.fail(&f))
}

fn local_result(out: &Out, r: Result<(), Failure>) -> u8 {
    match r {
        Ok(()) => code::OK,
        Err(f) => out.fail(&f),
    }
}

/// A remote command to its family's function; the local ones never reach here.
async fn run(api: &Api, out: &Out, command: Command) -> Result<u8, Failure> {
    match command {
        Command::Health { ready } => status::health(api, out, ready).await,
        Command::Status => status::status(api, out).await,
        Command::Busy { wait } => status::busy(api, out, wait).await,
        Command::ArtisanScreen(a) => status::artisan_screen(api, out, a).await,
        Command::Roasts { roaster, limit } => roasts::list(api, out, roaster, limit).await,
        Command::Show(a) => roasts::show(api, out, a).await,
        Command::Weigh(a) => roasts::weigh(api, out, a).await,
        Command::Landmark(a) => roasts::landmark(api, out, a).await,
        Command::Review(a) => analysis::review(api, out, a).await,
        Command::Compare(a) => analysis::compare(api, out, a).await,
        Command::Edit(a) => records::edit(api, out, a).await,
        Command::Cup(a) => records::cup(api, out, a).await,
        Command::Export(a) => records::export(api, out, a).await,
        Command::Import(a) => records::import(api, out, a).await,
        Command::Rm { roast, wait } => records::rm(api, out, roast, wait).await,
        Command::Labels(a) => labels::labels(api, out, a).await,
        Command::Share(a) => share::share(api, out, a).await,
        Command::Report(a) => reports::report(api, out, a).await,
        Command::Trace { subject, roaster } => reports::trace(api, out, subject, roaster).await,
        Command::Roasted(a) => roasted::roasted(api, out, a).await,
        Command::Beans { archived, low } => beans::list(api, out, archived, low).await,
        Command::Bean(cmd) => beans::run(api, out, cmd).await,
        Command::Wanted { cmd, all } => beans::wanted(api, out, cmd, all).await,
        Command::Costs { months } => beans::costs(api, out, months).await,
        Command::Order(records::OrderArgs { cmd: Some(c), .. }) => orders::run(api, out, c).await,
        Command::Order(a) => records::order(api, out, a).await,
        Command::Orders(a) => orders::list(api, out, a).await,
        Command::Plan { add, roaster, done } => plans::run(api, out, add, roaster, done).await,
        Command::Profiles => profiles::list(api, out).await,
        Command::Profile(cmd) => profiles::run(api, out, cmd).await,
        Command::Roasters => roasters::roasters(api, out).await,
        Command::Roaster(cmd) => roasters::run(api, out, cmd).await,
        Command::Live { roaster } => roasters::live(api, out, roaster).await,
        Command::Notifications { test } => settings::notifications(api, out, test).await,
        Command::HomeAssistant(a) => settings::home_assistant(api, out, a).await,
        Command::Settings { cmd, key } => match cmd {
            Some(SettingsCommand::Set { pairs }) => settings::settings_set(api, out, pairs).await,
            None => settings::settings(api, out, key).await,
        },
        Command::Modules { preset, on, off } => settings::modules(api, out, preset, on, off).await,
        Command::SupportBundle { output } => server::support_bundle(api, out, output).await,
        Command::Audit(a) => server::audit(api, out, a).await,
        Command::Backups { now, check } => server::backups(api, out, now, check).await,
        Command::Openapi { offline, find } => server::openapi(api, out, offline, find).await,
        Command::Api(a) => server::call(api, out, a).await,
        Command::Approval { id, wait } => approvals::approval(api, out, id, wait).await,
        Command::Backup { .. }
        | Command::Restore { .. }
        | Command::Login { .. }
        | Command::Logout
        | Command::Skill(_) => unreachable!("handled in main"),
    }
}
