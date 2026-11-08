//! Settings → About (`docs/notifications.md`, "About"): what is running and whether it is
//! well: versions, revision, uptime, MQTT and the database, the last backup, open problems,
//! where the changelog and the source of this revision are, and the licence. Also `GET /api/busy`: whether a roaster is in the
//! middle of something a restart would cut (`busy.rs`; the deploy waits on it).

use std::sync::OnceLock;
use std::time::Instant;

use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::control::busy::Busy;
use crate::notify::Problem;
use crate::ops::backups::BackupRun;

static STARTED: OnceLock<(Instant, chrono::DateTime<chrono::Utc>)> = OnceLock::new();
static BROKER: OnceLock<crate::ops::broker::BrokerInfo> = OnceLock::new();

/// Call once at start, so uptime counts from then.
pub fn mark_started() {
    STARTED.get_or_init(|| (Instant::now(), chrono::Utc::now()));
}

/// The built-in MQTT broker, when the server runs one (broker.rs)
pub fn set_broker(info: crate::ops::broker::BrokerInfo) {
    let _ = BROKER.set(info);
}

/// The built-in MQTT broker's info, if it runs (`home_assistant.rs`)
pub fn broker_info() -> Option<crate::ops::broker::BrokerInfo> {
    BROKER.get().cloned()
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(about))
        .routes(routes!(busy))
}

#[derive(Serialize, ToSchema)]
pub struct About {
    /// `roastwarden-server`
    name: &'static str,
    version: &'static str,
    /// The git commit the image was built from, or `unknown`
    revision: String,
    /// Where this build stands against the releases (`git describe` when it was built)
    build: Build,
    started_at: chrono::DateTime<chrono::Utc>,
    uptime_seconds: u64,
    mqtt_ready: bool,
    db_ready: bool,
    /// The newest migration applied
    migration: Option<String>,
    /// The newest backup attempt, nightly or by hand
    last_backup: Option<BackupRun>,
    /// System problems that have not cleared (docs/notifications.md)
    problems: Vec<Problem>,
    /// The CHANGELOG at the running revision, in the repository the image was built from
    /// (`ROASTWARDEN_REPO_URL`, which CI stamps); null when that is not known
    changelog_url: Option<String>,
    /// The source of the running revision (`ROASTWARDEN_REPO_URL`): AGPL-3.0 section 13 asks a
    /// server to offer its users its source; null when the repository is not known
    source_url: Option<String>,
    /// Roastwarden's licence, an SPDX id. The image serves its text and the third-party notices
    /// at /licenses/LICENSE.txt, /licenses/server.txt and /licenses/dashboard.txt
    licence: &'static str,
    /// Whether a roaster is in the middle of something a restart would cut (`GET /api/busy`)
    busy: Busy,
    /// The Modbus slave that lets Artisan be a screen (docs/artisan-screen.md); null: off
    artisan_screen: Option<crate::transports::modbus::ScreenStatus>,
    /// The MQTT broker the server runs itself (ROASTWARDEN_BROKER_ADDR; docs/DEPLOY.md, "One
    /// container"); null: the server uses a broker of its own (MQTT_BROKER_HOST)
    #[expect(
        clippy::doc_markdown,
        reason = "OpenAPI description: a change here changes docs/openapi.json"
    )]
    mqtt_broker: Option<crate::ops::broker::BrokerInfo>,
    /// The dashboard's optional parts that are on, their preset, and whether this is a new
    /// install's first run (docs/lean-home.md, "Modules")
    #[serde(flatten)]
    modules: crate::auth::modules::Modules,
}

/// Where a build stands against the releases: the version moves only at a release, so a build
/// after one says how many commits past it it is (CI stamps `ROASTWARDEN_DESCRIBE`, `git describe
/// --tags --long`, and `ROASTWARDEN_COMMITTED_AT`; both absent in a local build).
#[derive(Serialize, ToSchema, Debug, PartialEq)]
pub struct Build {
    /// The newest release this build contains (its tag without the `v`, "0.4.0")
    release: Option<String>,
    /// Commits after that release: 0 is the release itself, more is unreleased work
    commits_since_release: Option<u32>,
    /// When the commit was made
    committed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// "v0.4.0-66-gc9fd7cf" → ("0.4.0", 66); anything else (no tag: a bare hash) → none
fn parse_describe(d: &str) -> (Option<String>, Option<u32>) {
    let mut parts = d.trim().rsplitn(3, '-');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(hash), Some(n), Some(tag)) if hash.starts_with('g') && !tag.is_empty() => {
            match n.parse() {
                Ok(n) => (Some(tag.trim_start_matches('v').to_string()), Some(n)),
                Err(_) => (None, None),
            }
        }
        _ => (None, None),
    }
}

/// Where a build stands, for this version: a release commit is built and deployed before its
/// tag exists (DEVELOPMENT.md, "Releasing": push, check the deploy, then tag), so `git describe`
/// still names the release before it. A version other than the tag's is that release itself.
fn stand(describe: &str, version: &str) -> (Option<String>, Option<u32>) {
    match parse_describe(describe) {
        (Some(tag), _) if tag != version => (Some(version.to_string()), Some(0)),
        other => other,
    }
}

pub fn build() -> Build {
    let (release, commits_since_release) = std::env::var("ROASTWARDEN_DESCRIBE")
        .map_or((None, None), |d| stand(&d, env!("CARGO_PKG_VERSION")));
    Build {
        release,
        commits_since_release,
        committed_at: std::env::var("ROASTWARDEN_COMMITTED_AT")
            .ok()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t.trim()).ok())
            .map(|t| t.with_timezone(&chrono::Utc)),
    }
}

pub fn revision() -> String {
    std::env::var("ROASTWARDEN_REVISION").unwrap_or_else(|_| "unknown".into())
}

/// What a link into the repository names: a release by its tag, which the public copy shares
/// (its commit ids differ: the public copy leaves the private files out of every commit), other
/// builds by
/// their commit, `main` when neither is known (a local build).
#[derive(Debug, PartialEq)]
pub enum SourceRef {
    Tag(String),
    Commit(String),
    Main,
}

impl SourceRef {
    /// The ref for a build: its release's tag when it is that release, else its commit.
    pub fn of(revision: &str, build: &Build) -> Self {
        if let (Some(release), Some(0)) = (&build.release, build.commits_since_release) {
            if release == env!("CARGO_PKG_VERSION") {
                return Self::Tag(format!("v{release}"));
            }
        }
        if revision.len() >= 7 && revision.chars().all(|c| c.is_ascii_hexdigit()) {
            Self::Commit(revision.to_string())
        } else {
            Self::Main
        }
    }
}

/// GitHub's paths (`/tree/<ref>`, `/blob/<ref>/<file>`) or Forgejo's (`/src/commit/<sha>`).
fn is_github(repo: &str) -> bool {
    repo.split("://")
        .nth(1)
        .is_some_and(|rest| rest.starts_with("github.com/"))
}

/// A file in the repository at `at` (the CHANGELOG).
pub fn file_url(repo: &str, at: &SourceRef, file: &str) -> String {
    let repo = repo.trim_end_matches('/');
    if is_github(repo) {
        return format!(
            "{}/{file}",
            source_url(repo, at).replacen("/tree/", "/blob/", 1)
        );
    }
    format!("{}/{file}", source_url(repo, at))
}

/// The repository's files at `at`. Under it, a file's path opens that file on both hosts.
pub fn source_url(repo: &str, at: &SourceRef) -> String {
    let repo = repo.trim_end_matches('/');
    match (is_github(repo), at) {
        (true, SourceRef::Tag(r) | SourceRef::Commit(r)) => format!("{repo}/tree/{r}"),
        (true, SourceRef::Main) => format!("{repo}/tree/main"),
        (false, SourceRef::Tag(t)) => format!("{repo}/src/tag/{t}"),
        (false, SourceRef::Commit(c)) => format!("{repo}/src/commit/{c}"),
        (false, SourceRef::Main) => format!("{repo}/src/branch/main"),
    }
}

/// What is running and whether it is well (Settings → About).
#[utoipa::path(get, path = "/api/about", tag = "system",
    responses((status = 200, body = About)))]
async fn about(State(state): State<AppState>) -> Json<About> {
    let (started, started_at) = *STARTED.get_or_init(|| (Instant::now(), chrono::Utc::now()));
    let db_ok = crate::db::is_up(&state.db).await;
    let migration = crate::db::latest_migration(&state.db).await;
    let last_backup = crate::ops::backups::latest_run(&state.db, None)
        .await
        .ok()
        .flatten();
    let problems = state.notifier.problems().await.unwrap_or_default();
    // Where the image was built from (CI stamps it); none: About shows no link
    let repo = std::env::var("ROASTWARDEN_REPO_URL")
        .ok()
        .filter(|r| !r.trim().is_empty());
    let revision = revision();
    let build = build();
    let at = SourceRef::of(&revision, &build);
    Json(About {
        name: env!("CARGO_PKG_NAME"),
        version: env!("CARGO_PKG_VERSION"),
        changelog_url: repo.as_deref().map(|r| file_url(r, &at, "CHANGELOG.md")),
        source_url: repo.as_deref().map(|r| source_url(r, &at)),
        licence: env!("CARGO_PKG_LICENSE"),
        revision,
        build,
        started_at,
        uptime_seconds: started.elapsed().as_secs(),
        mqtt_ready: state.mqtt.is_ready(),
        db_ready: db_ok,
        migration,
        last_backup,
        problems,
        busy: crate::control::busy::check(&state).await,
        artisan_screen: crate::transports::modbus::status(&state).await,
        // Its roasters as they are now: the dashboard adds them without a restart
        mqtt_broker: BROKER.get().cloned().map(|mut b| {
            b.devices = state
                .mqtt_passwords
                .ids()
                .into_iter()
                .map(|(id, _)| id)
                .collect();
            b
        }),
        modules: crate::auth::modules::read(&state.db).await,
    })
}

/// Whether a roaster is in the middle of something a server restart would cut: a profile
/// drive, the server's autotune, the cooling after DROP, its heater on, or an open roast with
/// hot beans. `busy` is true when a real roaster is; the virtual roaster is listed but never
/// counts. A deploy waits while it is true (docs/DEPLOY.md).
#[utoipa::path(get, path = "/api/busy", tag = "system",
    responses((status = 200, body = Busy)))]
async fn busy(State(state): State<AppState>) -> Json<Busy> {
    Json(crate::control::busy::check(&state).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_says_how_far_past_its_release_it_is() {
        assert_eq!(
            parse_describe("v0.4.0-66-gc9fd7cf"),
            (Some("0.4.0".into()), Some(66))
        );
        assert_eq!(
            parse_describe("v0.5.0-0-g1234567\n"),
            (Some("0.5.0".into()), Some(0))
        );
        // A tag with dashes in it
        assert_eq!(
            parse_describe("v1.0.0-rc.1-3-gabcdef0"),
            (Some("1.0.0-rc.1".into()), Some(3))
        );
        // The release commit, built before its tag: the release itself
        assert_eq!(
            stand("v0.4.0-68-g48a262b", "0.5.0"),
            (Some("0.5.0".into()), Some(0))
        );
        assert_eq!(
            stand("v0.5.0-3-gabcdef0", "0.5.0"),
            (Some("0.5.0".into()), Some(3))
        );
        assert_eq!(stand("48a262b", "0.5.0"), (None, None));
        // No tag reachable: `--always` gives the hash alone
        assert_eq!(parse_describe("c9fd7cf"), (None, None));
        assert_eq!(parse_describe(""), (None, None));
    }

    #[test]
    fn source_links_name_the_release_tag_or_the_commit() {
        let at = |release: Option<&str>, n: Option<u32>, rev: &str| {
            SourceRef::of(
                rev,
                &Build {
                    release: release.map(String::from),
                    commits_since_release: n,
                    committed_at: None,
                },
            )
        };
        let v = env!("CARGO_PKG_VERSION");
        // The release itself: its tag, the same in the public copy
        assert_eq!(
            at(Some(v), Some(0), "5778d46bb83ac94d"),
            SourceRef::Tag(format!("v{v}"))
        );
        // Work past it, or an older release's build: the commit
        assert_eq!(
            at(Some(v), Some(3), "5778d46bb83ac94d"),
            SourceRef::Commit("5778d46bb83ac94d".into())
        );
        assert_eq!(
            at(Some("0.0.1"), Some(0), "5778d46"),
            SourceRef::Commit("5778d46".into())
        );
        assert_eq!(at(None, None, "unknown"), SourceRef::Main);

        let commit = SourceRef::Commit("5778d46bb83ac94d".into());
        let tag = SourceRef::Tag("v0.42.0".into());
        // Forgejo
        let fj = "https://git.example.org/me/rr/";
        assert_eq!(
            file_url(fj, &commit, "CHANGELOG.md"),
            "https://git.example.org/me/rr/src/commit/5778d46bb83ac94d/CHANGELOG.md"
        );
        assert_eq!(
            file_url(fj, &SourceRef::Main, "CHANGELOG.md"),
            "https://git.example.org/me/rr/src/branch/main/CHANGELOG.md"
        );
        assert_eq!(
            source_url(fj, &tag),
            "https://git.example.org/me/rr/src/tag/v0.42.0"
        );
        assert_eq!(
            source_url(fj, &commit),
            "https://git.example.org/me/rr/src/commit/5778d46bb83ac94d"
        );
        // GitHub
        let gh = "https://github.com/me/rr";
        assert_eq!(
            source_url(gh, &tag),
            "https://github.com/me/rr/tree/v0.42.0"
        );
        assert_eq!(
            source_url(gh, &SourceRef::Main),
            "https://github.com/me/rr/tree/main"
        );
        assert_eq!(
            file_url(gh, &tag, "CHANGELOG.md"),
            "https://github.com/me/rr/blob/v0.42.0/CHANGELOG.md"
        );
        assert!(!is_github("https://git.example.org/github.com/x"));
        assert_eq!(env!("CARGO_PKG_LICENSE"), "AGPL-3.0-or-later");
    }
}
