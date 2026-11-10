//! Roast profiles: the list, and one profile with its history, edits, files and share links.

use std::path::PathBuf;

use clap::Subcommand;

use crate::api::{self, Api, enc};
use crate::cli::Wait;
use crate::commands::records;
use crate::out::{Failure, Out, code, for_finding};
use crate::text::ProfileRow;

#[derive(Subcommand, Debug)]
pub enum ProfileCommand {
    /// A profile with its description, targets, expected ranges, curve and events
    Show {
        /// The profile's id or name
        profile: String,
    },
    /// How its finished roasts went, newest first: first crack, drop, development, weight
    /// loss, colour and how closely each followed the curve
    History {
        profile: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Change its name, description, level, purpose or targets (the curve and events stay)
    /// (admin)
    Edit(Box<records::ProfileEditArgs>),
    /// Save a finished roast as a new profile (admin). ROAST: an id or `latest`
    Save {
        roast: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
        /// With `latest`: the newest roast on this roaster
        #[arg(long)]
        roaster: Option<String>,
    },
    /// Import an Artisan .alog (or a Roastwarden profile export) as a profile (admin)
    Import {
        file: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
    /// Import a profile from its share link (another server's https://host/P/CODE, or a code
    /// of this server's) (admin). The server reads it; --preview only shows it
    ImportLink {
        /// The page's address (or one of its files', or a code of this server's)
        link: String,
        /// Its name here (default: the shared name)
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        preview: bool,
    },
    /// A public link for a profile (admin): its page shows the curve and targets and offers
    /// it as a file to import (Roastwarden JSON, Artisan .alog). The profile's newest link,
    /// else a new one; --new always makes another
    Share {
        profile: String,
        #[arg(long)]
        new: bool,
    },
    /// Profiles' share links, newest first: the address and how often it was opened and
    /// downloaded
    Shares {
        /// Only this profile's (an id or name)
        #[arg(long)]
        profile: Option<String>,
        /// The revoked links too
        #[arg(long)]
        all: bool,
    },
    /// Stop a profile's share link (admin): its page and files are a 404 from now on. CODE:
    /// the 25 characters after /P/, or the address
    Unshare { code: String },
    /// Save a profile as a file: Roastwarden JSON, or --artisan for an .alog
    Export {
        profile: String,
        #[arg(long)]
        artisan: bool,
        /// The file (default: the server's name for it, here); `-` for stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Delete a profile (admin); may wait for the owner's approval
    Rm {
        profile: String,
        #[command(flatten)]
        wait: Wait,
    },
}

/// `roast profiles`: every profile, seeds included.
pub async fn list(api: &Api, out: &Out) -> Result<u8, Failure> {
    let list = api.get("/api/profiles?include_private=true").await?;
    let rows: Vec<ProfileRow> = api::decode(list.clone())?;
    if out.json {
        // The description and the rest are in `profile show`
        let drop = [
            "created_at",
            "updated_at",
            "description",
            "is_public",
            "created_by",
        ];
        out.emit(&for_finding(list, &drop), |_| String::new());
    } else {
        out.emit(&rows, |rows| {
            rows.iter()
                .map(|p| {
                    format!(
                        "{:<38} {:<28} {:<18} {} {}",
                        p.id,
                        p.name,
                        p.roaster_kind.as_deref().unwrap_or("any roaster"),
                        p.roast_level.as_deref().unwrap_or("-"),
                        p.roast_purpose.as_deref().unwrap_or("")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        });
    }
    Ok(code::OK)
}

/// `roast profile …`: the subcommand to its call.
pub async fn run(api: &Api, out: &Out, cmd: ProfileCommand) -> Result<u8, Failure> {
    match cmd {
        ProfileCommand::History { profile, limit } => {
            records::profile_history(api, out, profile, limit).await
        }
        ProfileCommand::Edit(a) => records::profile_edit(api, out, *a).await,
        ProfileCommand::Save {
            roast,
            name,
            description,
            roaster,
        } => records::profile_save(api, out, roast, name, description, roaster).await,
        ProfileCommand::Import { file, name } => {
            records::profile_import(api, out, file, name).await
        }
        ProfileCommand::ImportLink {
            link,
            name,
            preview,
        } => records::profile_import_link(api, out, link, name, preview).await,
        ProfileCommand::Share { profile, new } => {
            records::profile_share(api, out, profile, new).await
        }
        ProfileCommand::Shares { profile, all } => {
            records::profile_shares(api, out, profile, all).await
        }
        ProfileCommand::Unshare { code } => records::profile_unshare(api, out, code).await,
        ProfileCommand::Export {
            profile,
            artisan,
            output,
        } => records::profile_export(api, out, profile, artisan, output).await,
        ProfileCommand::Rm { profile, wait } => records::profile_rm(api, out, profile, wait).await,
        ProfileCommand::Show { profile } => show(api, out, profile).await,
    }
}

/// `roast profile show`: one profile with its targets and ranges.
pub async fn show(api: &Api, out: &Out, profile: String) -> Result<u8, Failure> {
    let p = api.profile(&profile).await?;
    let id = p["id"].as_str().unwrap_or_default().to_string();
    let one = api.get(&format!("/api/profiles/{}", enc(&id))).await?;
    out.emit(&one, |p| {
        let ex = records::expect_text(p);
        format!(
            "{}  {} ({} {}, for {})\n{}\n{}{}",
            p["id"].as_str().unwrap_or("?"),
            p["name"].as_str().unwrap_or("?"),
            p["roast_level"].as_str().unwrap_or("-"),
            p["roast_purpose"].as_str().unwrap_or("-"),
            p["roaster_kind"].as_str().unwrap_or("any roaster"),
            records::auto_drop_text(p),
            if ex.is_empty() {
                String::new()
            } else {
                format!("own ranges: {ex} (the rest from the level)\n")
            },
            p["description"].as_str().unwrap_or("")
        )
    });
    Ok(code::OK)
}
