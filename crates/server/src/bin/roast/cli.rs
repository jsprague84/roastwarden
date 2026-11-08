//! The command line's shape: `roast` and its commands as clap reads them, with the examples
//! `roast --help` ends with. A family's own arguments live beside its code in `commands/`.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::commands::beans::{BeanCommand, WantedCommand};
use crate::commands::plans::PlanCommand;
use crate::commands::profiles::ProfileCommand;
use crate::commands::roasters::RoasterCommand;
use crate::commands::settings::SettingsCommand;
use crate::commands::{
    analysis, labels, orders, records, reports, roasted, roasts, server, settings, share, status,
};
use crate::setup;

pub const EXAMPLES: &str = "\
Examples:
  roast login https://roastwarden.example       # once: the server and a token, kept on this machine
  roast skill install claude-code               # the Agent Skill for claude-code, hermes or agents
  export ROASTWARDEN_URL=https://roastwarden.example ROASTWARDEN_TOKEN=rr_…   # or this, each time
  roast status                                  # version, problems, roasters, last backup
  roast roasts --limit 5                        # newest roasts with their numbers
  roast show <roast-id>                         # one roast, its events and its numbers
  roast beans [--low]                           # green beans and their stock
  roast openapi --find plan                     # which call does it: method, path, summary
  roast bean add --name 'Ethiopia Guji' --origin Ethiopia --process natural --stock 5kg
  roast bean stock add 'Ethiopia Guji' 5kg --price 46 --shipping 9.50 --supplier \"Sweet Maria's\"
  roast costs --months 3                        # spent, bought, roasted and its cost, by month
  roast bean import https://www.sweetmarias.com/products/…   # preview a supplier's coffee
  roast bean import <link> --want '5 LB'        # on the shopping list; --bought '5 LB' records it
  roast wanted                                  # the shopping list; wanted bought <id> when it arrives
  roast bean stock set 'Ethiopia Guji' 3200g    # a correction after weighing
  roast plan add --bean 'Ethiopia Guji' --weight 100g --profile 'Light espresso'
  roast backups --now                           # a database copy before a bulk change
  roast bean rm 'Old lot' --wait                # waits for the owner's approval if needed
  roast order --shipping 12 --line 'Kenya AA=2lb@18.50' --line 'Brazil=5lb@34.90'
  roast weigh latest 85.75 --roaster esp32_roaster_01      # after a roast: weight loss
  roast edit latest --add-note 'bright, thin' --rating 4   # notes, rating, colour, level
  roast cup latest acidity=7.5 body=7 --notes 'stone fruit'
  roast roasted pack latest 2x100g --ground espresso       # bags packed: the grams come off
  roast roasted combine '#16' '#17'             # two batches in one bag: one lot, one label
  roast trace '#12'                             # a bag's batch back to its green lot
  roast share latest --message 'Happy birthday, Sam'       # its public page, for a bag's QR code
  roast profile history 'Light espresso' --limit 5         # how its roasts went
  roast live                                    # what each roaster reads now (only reads)
  roast api GET '/api/sessions?limit=1'         # any API call, with the token

Units: temperatures in °C; weights in grams (5000, 5000g, 5kg, 2lb and 12oz are read too).
Output: one compact JSON line when stdout is not a terminal (or with --json), null fields left
out; errors are {\"error\": {code, message, hint}}.
Exit codes: 0 done · 1 ran, but the answer is a failure · 2 bad arguments · 3 waiting on a
human (an approval) · 4 not found · 5 not allowed · 6 invalid or conflicting · 7 Roastwarden
unreachable.";

/// roast — use and administer Roastwarden from the command line
#[derive(Parser, Debug)]
#[command(name = "roast", author, version, about, long_about = None, after_help = EXAMPLES)]
#[command(propagate_version = true)]
pub struct Cli {
    /// The Roastwarden server [default: the one `roast login` saved, else http://localhost:8080]
    #[arg(short, long, global = true, env = "ROASTWARDEN_URL")]
    pub url: Option<String>,

    /// API token (`rr_…`, from Settings → Tokens) [default: the one `roast login` saved]. The
    /// owner key works too, passed on purpose
    #[arg(
        short,
        long,
        global = true,
        env = "ROASTWARDEN_TOKEN",
        hide_env_values = true
    )]
    pub token: Option<String>,

    /// Answer in JSON even on a terminal
    #[arg(long, global = true, conflicts_with = "text")]
    pub json: bool,

    /// Answer in text even when piped
    #[arg(long, global = true)]
    pub text: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum SkillCommand {
    /// Write the skill where an agent finds it: claude-code (~/.claude/skills), hermes
    /// (~/.hermes/skills/devops), agents (~/.agents/skills: Codex and others), or --dir.
    /// Names the saved server in it; the old SKILL.md is kept as SKILL.md.bak
    Install {
        #[arg(value_enum)]
        agent: Option<setup::Agent>,
        /// Write SKILL.md and label-design.md into this folder instead
        #[arg(long, conflicts_with = "agent")]
        dir: Option<PathBuf>,
    },
    /// Print the skill (for an agent with no skills folder: put it in its instructions)
    Show {
        /// The label design guide that goes with it instead
        #[arg(long)]
        label_design: bool,
    },
}

/// `--wait` for the owner's approval of a held call
#[derive(Args, Debug, Clone, Copy)]
pub struct Wait {
    /// If Roastwarden holds the call for the owner's approval, wait for the answer
    #[arg(long)]
    pub wait: bool,
    /// How long to wait, in seconds
    #[arg(long, default_value_t = 3600)]
    pub wait_timeout: u64,
}

// Parsed once per run: its size does not matter.
#[allow(clippy::large_enum_variant)]
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Is Roastwarden up? (no token; the container healthcheck). Exit 1 with --ready when MQTT
    /// or the database is not ready
    Health {
        #[arg(long)]
        ready: bool,
    },
    /// Version, problems, roasters online, the plan, low beans and the last backup
    Status,
    /// Keep the server's address and an API token on this machine (~/.config/roastwarden/
    /// roast.json, only you can read it), checked against the server first; then `roast` and
    /// an agent's shell need no environment. The token is asked for (not shown), or read from
    /// stdin, or taken from --token / ROASTWARDEN_TOKEN
    Login {
        /// The server, e.g. https://roastwarden.example [default: --url, ROASTWARDEN_URL]
        address: Option<String>,
    },
    /// Forget the server and token `roast login` saved
    Logout,
    /// The Agent Skill that teaches an AI agent to use roast (built into this binary)
    #[command(subcommand)]
    Skill(SkillCommand),
    /// Is a roaster in the middle of something a restart would cut (a profile drive, the
    /// server's autotune, cooling after DROP, its heater on, a roast with hot beans)? Exit 1
    /// while a real roaster is busy; the virtual roaster never counts. Ask before a deploy,
    /// a restart or a firmware flash
    Busy {
        /// Wait up to this many minutes for every real roaster to be idle (checks every 30 s)
        #[arg(long, value_name = "MINUTES")]
        wait: Option<u64>,
    },
    /// Artisan as a screen (docs/artisan-screen.md): whether the server's Modbus slave is on,
    /// the roaster it serves, read-only or with writes, when Artisan last read; --aset FILE
    /// saves the Artisan setup that reads it (Help → Load Settings in Artisan)
    ArtisanScreen(status::ArtisanScreenArgs),
    /// Roasts, newest first, with their numbers (loss, development, FC, …)
    Roasts {
        /// Only this roaster (its id, e.g. virtual_roaster)
        #[arg(long)]
        roaster: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// One roast: its details, numbers, events, cupping and coffee cost (--telemetry for
    /// every sample). ROAST: its id, or `latest` (--roaster narrows it)
    Show(roasts::ShowArgs),
    /// A roast read: marks and phases, RoR (early peak, by minute, the late dip), BT against
    /// its profile (overshoot after CHARGE, RMS), the heater and fan (opening, headroom late)
    /// and its numbers against what was expected. --vs lines it up against another roast.
    /// ROAST: its id, `#12` (batch number) or `latest`
    Review(analysis::ReviewArgs),
    /// Several roasts lined up against the first (or --ref): every number with its
    /// difference, and from when each one's BT stays within 0.5 °C of the reference's
    Compare(analysis::CompareArgs),
    /// Edit a roast's record: name, notes, rating, level and purpose, bean, weights, colour,
    /// moisture. Absent fields are kept. On a finished roast this reaches no roaster
    Edit(records::EditArgs),
    /// A roast's cupping on the classic SCA form: show it, or set scores (`acidity=7.5
    /// sweetness=8`; the others are kept), --taint/--fault cups and --notes; --rm removes it
    Cup(records::CupArgs),
    /// Save a roast as a file: the Artisan .alog, or --csv
    Export(records::ExportArgs),
    /// Import a roast Artisan recorded (.alog) as a finished roast (admin)
    Import(records::ImportArgs),
    /// Bag labels for a roast: a PDF to print at "Actual size" on a label sheet (5163, 6874,
    /// 5294 or the owner's), in a design (plain, gift, the owner's, or --source FILE), from
    /// label N; --svg for a preview, --calibration for the sheet's calibration page, --list
    /// for the choices. `labels design check` checks a design before it is used
    Labels(labels::LabelsCmd),
    /// A report as a PDF the server makes (docs/reports.md): `report roast latest` writes the
    /// roast's report here; --format png for one page to look at, json for its data;
    /// `report list` for the kinds
    Report(reports::ReportCmd),
    /// A roast's public share page, what a bag's QR code opens (`labels --qr share`): its
    /// link, made when it has none; --message sets the line for the person it is for, --new
    /// makes another. `share list` lists the links, `share revoke CODE` stops one
    Share(share::ShareCmd),
    /// Delete a roast, by its id (admin); may wait for the owner's approval
    Rm {
        roast: String,
        #[command(flatten)]
        wait: Wait,
    },
    /// Record a roast's roasted weight (and green weight): weight loss and what a dose cost
    /// follow. ROAST: its id, or `latest` (the newest roast; --roaster narrows it)
    Weigh(roasts::WeighArgs),
    /// Move a roast's landmark, or add it: e.g. first crack off a lone early crack to where
    /// steady cracking began. A finished roast's numbers (DTR, development RoR, AUC) follow
    Landmark(roasts::LandmarkArgs),
    /// Green beans with their stock
    Beans {
        /// Archived beans too
        #[arg(long)]
        archived: bool,
        /// Only beans below their low-stock mark
        #[arg(long)]
        low: bool,
    },
    /// Show or add a bean, or record stock
    #[command(subcommand)]
    Bean(BeanCommand),
    /// The shopping list: coffees wanted (--all: also those bought); `wanted bought <id>`
    /// records the purchase when it arrives
    Wanted {
        #[command(subcommand)]
        cmd: Option<WantedCommand>,
        #[arg(long)]
        all: bool,
    },
    /// Roasted coffee: what is ready to drink, what is resting and until when, for espresso
    /// and filter, how fast it goes and when to roast again; `roasted use|give|toss|count`
    /// records what happened to it (docs/roasted.md)
    Roasted(roasted::RoastedArgs),
    /// Traceability: a roast (`#12` from a bag, `latest`, its id) back to its green coffee
    /// (supplier, lot, purchases) and where its coffee went; or a coffee (name or id) to every
    /// roast and bag: packed bags, label runs, gifts, use, discards, what is left
    Trace {
        /// `#12`, `latest` or a roast's id; or a coffee's name or id
        subject: String,
        /// With `latest` or a batch number: on this roaster
        #[arg(long)]
        roaster: Option<String>,
    },
    /// Green coffee by month: spent, bought, roasted, and what the roasted coffee cost
    Costs {
        #[arg(long, default_value_t = 12)]
        months: u32,
    },
    /// One order of several coffees that came, with one shipping charge split over them by
    /// weight: --line "Kenya Nyeri AA=2lb@18.50" --line "Brazil Cerrado=5lb@34.90" --shipping 12.
    /// Or from its receipt: `order import` (checked, ordered → shipped → received), `order
    /// shipped|received|cancel|receipt|show|rm` (docs/green-coffee.md, "Orders")
    #[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
    Order(records::OrderArgs),
    /// Supplier orders recorded from receipts: open ones (ordered, shipped), or --all
    Orders(orders::OrdersArgs),
    /// The roasters: set up, heard on MQTT, online, firmware; and any waiting to be set up
    Roasters,
    /// One roaster: `roaster show` (connections, CHARGE/DROP sequence, fan assist, whether
    /// tokens may control it), `roaster contract` (firmware, caps, are its gains the held
    /// ones), `roaster model` (its thermal model and how well it predicts), `roaster tuning`
    /// (PID gains, gain schedule, autotune runs)
    #[command(subcommand)]
    Roaster(RoasterCommand),
    /// What each roaster reads now (BT, ET, RoR, setpoint, heater, fan), its running roast,
    /// Profile drive and the prediction (BT in 1 and 2 min, first crack, DROP, and why). Only
    /// reads
    Live {
        /// Only this roaster (id or name)
        #[arg(long)]
        roaster: Option<String>,
    },
    /// Notification channels (push, Gotify, ntfy, Matrix, webhook), how each one's last send
    /// went, the phones signed up, open problems; --test CHANNEL sends a test (admin)
    Notifications {
        #[arg(long, value_name = "CHANNEL")]
        test: Option<String>,
    },
    /// Home Assistant discovery (docs/home-assistant.md): on or off, the broker, the roasters
    /// announced and their entities. --on / --off and the options change it (admin). Read-only
    /// in Home Assistant: no entity sends a command
    HomeAssistant(settings::HomeAssistantArgs),
    /// A support bundle for a bug report (docs/support.md): versions, the roasters' setups, the
    /// newest roast, settings, problems and the recent log, every secret removed, no green
    /// coffee, orders or receipts; one JSON file (admin). Read it before you share it
    SupportBundle {
        /// Where to write it (`-`: stdout); default the server's name in this folder
        #[arg(short, long, value_name = "FILE")]
        output: Option<std::path::PathBuf>,
    },
    /// Settings: all of them, or one KEY (secrets show only whether they are set; the alarm
    /// rules are `roast_alarms`); `settings set KEY=VALUE…` writes (admin)
    #[command(args_conflicts_with_subcommands = true)]
    Settings {
        #[command(subcommand)]
        cmd: Option<SettingsCommand>,
        key: Option<String>,
    },
    /// The dashboard's optional parts that are on (reports, cupping, tuning, agents, costs,
    /// labels) and their preset; --preset, --on and --off change them (admin). An off module
    /// leaves the dashboard, not the API (docs/lean-home.md)
    Modules {
        /// Start from a preset: home (none), enthusiast (reports, cupping, tuning, agents) or
        /// roastery (all six)
        #[arg(long, value_parser = ["home", "enthusiast", "roastery"])]
        preset: Option<String>,
        /// Turn these on (several: --on reports cupping)
        #[arg(long, num_args = 1.., value_name = "MODULE")]
        on: Vec<String>,
        /// Turn these off
        #[arg(long, num_args = 1.., value_name = "MODULE")]
        off: Vec<String>,
    },
    /// The roast plan (the queue of planned roasts); `plan add` adds one
    Plan {
        #[command(subcommand)]
        add: Option<PlanCommand>,
        /// Only plans for this roaster (and those for any roaster)
        #[arg(long)]
        roaster: Option<String>,
        /// Plans that already ran too
        #[arg(long)]
        done: bool,
    },
    /// Roast profiles, seed profiles included (`profile show` has the description and curve)
    Profiles,
    /// One profile
    #[command(subcommand)]
    Profile(ProfileCommand),
    /// The audit log, newest first: every change and control command, with who sent it
    Audit(server::AuditArgs),
    /// The server's database backups: the copies, the last attempt, the next nightly run.
    /// With --now, write one now (admin token), e.g. before a bulk change
    Backups {
        #[arg(long, conflicts_with = "check")]
        now: bool,
        /// Check one copy (integrity, a schema this version knows) without touching anything
        /// (admin)
        #[arg(long, value_name = "NAME")]
        check: Option<String>,
    },
    /// The API's OpenAPI document (--offline: the one this roast was built with). With
    /// --find, only the operations whose path, id or summary contain the text: a short list
    /// to find the call for `roast api`
    Openapi {
        #[arg(long)]
        offline: bool,
        /// e.g. --find plan, --find "start"
        #[arg(long)]
        find: Option<String>,
    },
    /// Any API call, with the token and the error handling: `roast api GET /api/beans`.
    /// A call held for approval exits 3 (or, with --wait, waits for the owner)
    Api(server::ApiArgs),
    /// An approval request (the id from a held call); with --wait, wait for the owner's answer
    Approval {
        id: String,
        #[command(flatten)]
        wait: Wait,
    },

    /// [local] Write a copy of the database into the backup folder (the same copy as the
    /// nightly one), then keep only the newest `backup_keep` copies. Run it next to the
    /// database: docker exec roastwarden roast backup
    Backup {
        /// The database (default ROASTWARDEN_DB_PATH, else data/roastwarden.db)
        #[arg(
            long,
            env = "ROASTWARDEN_DB_PATH",
            default_value = "data/roastwarden.db"
        )]
        db: String,
    },
    /// [local] Check a copy (integrity, a Roastwarden schema this version knows) and stage it;
    /// the server puts it in place at its next start and moves the current database into
    /// before-restore-<time>/. Then: docker restart roastwarden
    Restore {
        /// The copy: a path, or a name in the backup folder (`roast backups` lists them)
        file: PathBuf,
        /// Only check the copy; stage nothing (safe while the server runs)
        #[arg(long)]
        check: bool,
        #[arg(
            long,
            env = "ROASTWARDEN_DB_PATH",
            default_value = "data/roastwarden.db"
        )]
        db: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_command_line_is_well_formed() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    /// A subcommand argument with a global flag's id silently gets the global's value
    /// (`bean import <url>` once took `--url`, `audit --token` the API token).
    #[test]
    fn no_argument_reuses_a_global_flag() {
        use clap::CommandFactory;
        let cli = Cli::command();
        let globals: Vec<String> = cli
            .get_arguments()
            .filter(|a| a.is_global_set())
            .map(|a| a.get_id().to_string())
            .collect();
        fn walk(cmd: &clap::Command, globals: &[String], path: &str, bad: &mut Vec<String>) {
            for sub in cmd.get_subcommands() {
                let here = format!("{path} {}", sub.get_name());
                for a in sub.get_arguments() {
                    if !a.is_global_set() && globals.contains(&a.get_id().to_string()) {
                        bad.push(format!("{here}: {}", a.get_id()));
                    }
                }
                walk(sub, globals, &here, bad);
            }
        }
        let mut bad = Vec::new();
        walk(&cli, &globals, "roast", &mut bad);
        assert!(bad.is_empty(), "{bad:?}");
    }
}
