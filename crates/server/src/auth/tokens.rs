//! API tokens for scripts and agents: their format, how a presented token is checked, what
//! each access preset may do, and the real-hardware rule (SECURITY.md, docs/control.md
//! "Tokens").
//!
//! A token is `rr_` followed by 40 random alphanumerics (about 238 bits): its prefix tells it
//! apart from the owner key, a double-click selects it, and a secret scanner can find it.
//! Only its SHA-256 is stored (the secret is random, so a slow password hash would add only
//! latency). The owner sees the secret once, when it is created.
//!
//! **Access is decided here and only here**, by method and path, before any handler runs
//! ([`Preset::allows`], called from `auth::require_api_key`). Anything a preset does not name
//! is refused with 403. No preset names token management, the per-roaster "tokens may
//! control" switch or restoring a backup (which would bring back revoked tokens): those are
//! the owner key's alone ([`OWNER_ONLY`]).
//!
//! On top of the preset, a token may change nothing under `/api/roaster/{d}/…` or
//! `/api/sessions/{id}/…` for a roaster other than `virtual_roaster` unless the owner allowed
//! tokens for that roaster ([`hardware_refusal`]).

use serde::{Deserialize, Serialize};
use sqlx::AssertSqlSafe;
use sqlx::SqlitePool;

/// What every token starts with.
pub const TOKEN_PREFIX: &str = "rr_";
/// Random characters after the prefix.
const SECRET_LEN: usize = 40;
/// Characters after the prefix kept in the clear, so a list can show which token is which.
const SHOWN_LEN: usize = 6;
/// `last_used_at` is written at most this often per token.
const TOUCH_INTERVAL_SECS: i64 = 60;

/// A freshly generated token: the secret for the owner (once) and what the database keeps.
pub struct NewSecret {
    pub secret: String,
    pub prefix: String,
    pub hash: String,
}

pub fn generate() -> NewSecret {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut body = String::with_capacity(SECRET_LEN);
    let mut buf = [0u8; 64];
    while body.len() < SECRET_LEN {
        getrandom::fill(&mut buf).expect("the OS random source failed");
        // Rejection sampling keeps every character equally likely: 248 = 4 × 62.
        for b in buf {
            if b < 248 && body.len() < SECRET_LEN {
                body.push(ALPHABET[(b % 62) as usize] as char);
            }
        }
    }
    let secret = format!("{TOKEN_PREFIX}{body}");
    NewSecret {
        prefix: secret[..TOKEN_PREFIX.len() + SHOWN_LEN].to_string(),
        hash: hash(&secret),
        secret,
    }
}

/// Hex SHA-256 of the whole token, prefix included.
pub fn hash(secret: &str) -> String {
    crate::sha256_hex(secret.as_bytes())
}

/// Whether a credential is meant as an API token (rather than the owner key).
pub fn is_token(value: &str) -> bool {
    value.starts_with(TOKEN_PREFIX)
}

fn well_formed(value: &str) -> bool {
    value.strip_prefix(TOKEN_PREFIX).is_some_and(|body| {
        body.len() == SECRET_LEN && body.bytes().all(|b| b.is_ascii_alphanumeric())
    })
}

/// How much a token may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    /// Every read: roasts, profiles, telemetry, settings (not the audit log).
    ReadOnly,
    /// Reads, plus running a roast: the roast's lifecycle and events, Profile drive, live
    /// control commands, cupping notes, the roast plan and the virtual roaster's charge and dump.
    Roast,
    /// Reads, plus the green coffee: beans, their stock (purchases and corrections), the
    /// roast plan, and bag labels (saved designs, assets, brands). For an agent that records
    /// a purchase or designs labels; it cannot touch a roaster.
    Inventory,
    /// Everything except the owner's own (managing tokens, the per-roaster switch, restoring
    /// a backup), and the audit log.
    Admin,
}

/// Paths every preset may GET (and HEAD). `*` is one path segment.
const READ: &[&str] = &[
    "/healthz",
    "/readyz",
    "/version",
    "/metrics",
    "/api-docs/openapi.json",
    "/ws/telemetry",
    "/ws/v2/telemetry",
    "/ws/debug",
    "/api/auth/status",
    "/api/busy",
    "/api/telemetry/rings",
    "/api/roaster/*/samples",
    "/api/roaster/*/telemetry/latest",
    "/api/roaster/*/transports",
    "/api/roaster/*/autotune/runs",
    "/api/roaster/*/autotune/runs/*",
    "/api/roaster/*/autotune/runs/*/samples",
    "/api/roaster/*/autotune/runs/*/design",
    "/api/roaster/*/tune",
    "/api/roaster/*/control/pid/history",
    "/api/roaster/*/fan_assist",
    "/api/roaster/*/gain_schedule",
    "/api/roaster/*/contract",
    // Adding a roaster (docs/add-roaster.md): its newest raw message and the conformance check
    "/api/roaster/*/raw",
    "/api/roaster/*/check",
    // The roaster's own model and the live prediction (docs/prediction.md)
    "/api/roaster/*/model",
    "/api/roaster/*/prediction",
    "/api/sessions/*/prediction",
    "/api/roaster/*/sequence",
    "/api/roaster/*/token-access",
    "/api/roaster/*/setup",
    "/api/devices",
    // Artisan's machine catalogue (docs/artisan-machines.md)
    "/api/artisan/machines",
    // Artisan as a screen: the setup that reads the Modbus slave (docs/artisan-screen.md)
    "/api/artisan/screen.aset",
    "/api/devices/mdns",
    "/api/devices/registry",
    "/api/devices/discovered",
    "/api/devices/*",
    "/api/devices/*/register-map",
    "/api/device-profiles",
    "/api/device-profiles/*",
    "/api/sessions",
    "/api/sessions/*",
    "/api/sessions/*/replay",
    "/api/sessions/*/replay/events",
    "/api/sessions/*/replay/log",
    "/api/sessions/*/follow",
    "/api/sessions/*/telemetry",
    "/api/sessions/*/export/csv",
    "/api/sessions/*/export/artisan",
    "/api/sessions/*/cupping",
    "/api/sessions/*/events",
    "/api/replay",
    "/api/follow",
    "/api/profiles",
    "/api/profiles/*",
    "/api/profiles/*/export",
    "/api/profiles/*/export/artisan",
    "/api/profiles/*/history",
    // Shared profiles: their links (docs/profiles.md, "Sharing a profile")
    "/api/profiles/*/share",
    "/api/profile-shares",
    "/api/settings",
    "/api/settings/*",
    "/api/notifications",
    // Home Assistant discovery: on or off, the broker, the roasters announced
    "/api/home-assistant",
    "/api/about",
    "/api/backups",
    "/api/beans",
    "/api/beans/*",
    // A new bean's first roast and the next one (docs/new-bean.md)
    "/api/beans/*/guide",
    "/api/costs",
    "/api/wanted",
    // Supplier orders: their lines and totals, and what is kept of each receipt; a receipt's
    // file is private (BEANS, auth.rs)
    "/api/orders",
    "/api/orders/*",
    "/api/sessions/*/cost",
    // A roast read and roasts compared (docs/analysis.md)
    "/api/sessions/*/analysis",
    "/api/analysis/compare",
    // Bag labels: reads, so any token can make them (docs/labels.md)
    "/api/labels",
    "/api/labels/calibration.pdf",
    "/api/sessions/*/labels.pdf",
    "/api/sessions/*/labels.svg",
    "/api/labels/printer",
    "/api/label-designs",
    "/api/label-designs/*",
    "/api/label-assets",
    "/api/label-assets/*",
    "/api/label-brands",
    "/api/label-brands/*",
    // Print runs: what was printed when, and the PDF kept with each
    "/api/label-runs",
    "/api/label-runs/*",
    "/api/label-runs/*/pdf",
    "/api/label-sheets",
    // Label printers: as set, and asked how they are (GET only asks)
    "/api/label-printers",
    "/api/label-printers/*",
    "/api/label-printers/find",
    // Share links: the public pages bags' QR codes open (docs/labels.md, "The share page")
    "/api/share",
    "/api/sessions/*/share",
    "/api/plan",
    "/api/plan/*",
    // Roasted coffee (docs/roasted.md)
    "/api/roasted",
    "/api/roasted/*",
    "/api/roasted/combined/*",
    // Traceability: a roast back to its green lot, a coffee to its bags (no prices)
    "/api/trace/roast/*",
    "/api/trace/bean/*",
    // Reports (docs/reports.md): PDFs and their data, made from what any token may read
    "/api/reports",
    "/api/reports/roast/*",
    "/api/reports/compare",
    "/api/reports/cupping",
    "/api/reports/inventory",
    "/api/reports/shopping",
    "/api/reports/coffee/*",
    "/api/reports/profile/*",
    "/api/reports/period",
    "/api/reports/purchases",
    "/api/reports/plan",
    "/api/reports/trace",
    "/api/virtual-roaster",
    "/api/push/key",
    "/api/push/subscriptions",
    // One approval, by its unguessable id: the token that asked waits on it
    "/api/approvals/*",
];

/// Calls every preset may make that change nothing: they lay out labels from a design sent
/// in the body (docs/labels.md, "Label studio"). Each render runs in the sandboxed worker
/// with a time and memory limit, two at a time.
const COMPUTE: &[(&str, &str)] = &[
    ("POST", "/api/labels/check"),
    ("POST", "/api/labels/render"),
    ("POST", "/api/label-runs/preview"),
    // A mapping tried on a roaster's newest raw message: nothing saved, nothing sent
    ("POST", "/api/roaster/*/ingest/preview"),
    // A roaster planned from an Artisan machine setup: nothing saved, nothing sent
    ("POST", "/api/artisan/machines/plan"),
];

/// What "roast" adds to [`READ`]: what the Roast page does during a roast.
const ROAST: &[(&str, &str)] = &[
    ("POST", "/api/roaster/*/control/setpoint"),
    ("POST", "/api/roaster/*/control/fan"),
    ("POST", "/api/roaster/*/control/heater"),
    ("POST", "/api/roaster/*/control/drum"),
    ("POST", "/api/roaster/*/control/damper"),
    ("POST", "/api/roaster/*/control/mode"),
    ("POST", "/api/roaster/*/control/heater_enable"),
    ("POST", "/api/roaster/*/control/emergency_stop"),
    ("POST", "/api/sessions"),
    ("PUT", "/api/sessions/*"),
    ("POST", "/api/sessions/*/start"),
    ("POST", "/api/sessions/*/pause"),
    ("POST", "/api/sessions/*/resume"),
    ("POST", "/api/sessions/*/complete"),
    ("POST", "/api/sessions/*/cancel"),
    ("POST", "/api/sessions/*/drive/resume"),
    ("POST", "/api/sessions/*/replay"),
    ("PATCH", "/api/sessions/*/replay"),
    ("DELETE", "/api/sessions/*/replay"),
    ("POST", "/api/sessions/*/replay/pause"),
    ("POST", "/api/sessions/*/replay/resume"),
    ("POST", "/api/sessions/*/replay/events/*"),
    ("POST", "/api/sessions/*/follow"),
    ("DELETE", "/api/sessions/*/follow"),
    ("POST", "/api/sessions/*/events"),
    ("PUT", "/api/sessions/*/events/*"),
    ("DELETE", "/api/sessions/*/events/*"),
    ("POST", "/api/sessions/*/cupping"),
    ("DELETE", "/api/sessions/*/cupping"),
    ("POST", "/api/virtual-roaster/charge"),
    ("POST", "/api/virtual-roaster/dump"),
    // Refit the roaster's model (docs/prediction.md): it changes no setting
    ("POST", "/api/roaster/*/model/fit"),
];

/// The roast plan: "roast" (the Roast page starts and skips planned roasts) and "inventory".
const PLAN: &[(&str, &str)] = &[
    ("POST", "/api/plan"),
    ("POST", "/api/plan/reorder"),
    ("PUT", "/api/plan/*"),
    ("DELETE", "/api/plan/*"),
];

/// The green beans and their stock: "inventory" and "admin".
const BEANS: &[(&str, &str)] = &[
    ("POST", "/api/beans"),
    ("PUT", "/api/beans/*"),
    ("DELETE", "/api/beans/*"),
    ("POST", "/api/beans/*/stock"),
    ("PUT", "/api/beans/*/moves/*"),
    ("DELETE", "/api/beans/*/moves/*"),
    ("POST", "/api/purchases"),
    // Reads one supplier page from the server: a write preset, not every reader
    ("POST", "/api/beans/import"),
    ("POST", "/api/wanted"),
    ("PUT", "/api/wanted/*"),
    ("DELETE", "/api/wanted/*"),
    ("POST", "/api/wanted/*/bought"),
    // Cart links: the server reads each product again, as the import does
    ("POST", "/api/wanted/cart"),
    // Orders from receipts (docs/green-coffee.md, "Orders")
    ("POST", "/api/orders"),
    ("DELETE", "/api/orders/*"),
    ("POST", "/api/orders/*/shipped"),
    ("POST", "/api/orders/*/received"),
    ("POST", "/api/orders/*/cancel"),
    ("POST", "/api/orders/*/receipts"),
    ("DELETE", "/api/orders/*/receipts/*"),
    // A receipt's file holds the owner's address, phone and card digits: not every reader
    ("GET", "/api/orders/*/receipts/*"),
    // A coffee's own label (design, parameters, brand)
    ("PUT", "/api/beans/*/label"),
    // Roasted coffee used, given or counted (docs/roasted.md)
    ("POST", "/api/roasted/*/moves"),
    ("DELETE", "/api/roasted/*/moves/*"),
    // Roasts in one bag (docs/roasted.md, "Roasts in one bag")
    ("POST", "/api/roasted/combined"),
    ("DELETE", "/api/roasted/combined/*"),
];

/// Saved label designs, their assets and brand kits (docs/labels.md, "Label studio"):
/// "inventory" (the coffee and its bags) and "admin".
const LABELS: &[(&str, &str)] = &[
    ("POST", "/api/label-designs"),
    ("PUT", "/api/label-designs/*"),
    ("DELETE", "/api/label-designs/*"),
    ("POST", "/api/label-designs/*/rollback"),
    ("PUT", "/api/label-assets/*"),
    ("DELETE", "/api/label-assets/*"),
    ("PUT", "/api/label-brands/*"),
    ("DELETE", "/api/label-brands/*"),
];

/// Printing a roast's labels: "roast", "inventory" and "admin". Every print from a token is
/// held for the owner's approval (approvals.rs): someone must load the sheet.
const PRINT: &[(&str, &str)] = &[
    ("POST", "/api/sessions/*/labels/print"),
    // Print runs (docs/labels.md, "Print runs"): making one records it, a PDF to print by hand
    ("POST", "/api/label-runs"),
    ("POST", "/api/label-runs/*/print"),
    ("POST", "/api/label-runs/*/done"),
    ("POST", "/api/label-runs/*/reprint"),
    ("PUT", "/api/label-sheets/*"),
    // Share links (docs/labels.md, "The share page"): a link makes a roast's page public, so
    // only the tokens that may print a label carrying it make or revoke one
    ("POST", "/api/sessions/*/share"),
    ("DELETE", "/api/share/*"),
];

/// What "admin" adds to all of them: tuning, configuration, profiles, beans, devices, settings,
/// backups (but not restoring one), push subscriptions, and the test and maintenance
/// endpoints.
const ADMIN: &[(&str, &str)] = &[
    // The audit log holds every change's body: the owner's and admin's
    ("GET", "/api/audit"),
    // The support bundle: setups, the newest roast and the log, secrets removed (support.rs)
    ("GET", "/api/support-bundle"),
    // An ESP32's firmware with a roaster's MQTT password in it (firmware.rs): setup work
    ("GET", "/api/firmware"),
    ("POST", "/api/firmware"),
    // Roasters' MQTT passwords (mqtt_passwords.rs): who may sign in to the built-in broker
    ("GET", "/api/mqtt/passwords"),
    ("POST", "/api/mqtt/passwords"),
    ("PUT", "/api/mqtt/passwords/*"),
    ("DELETE", "/api/mqtt/passwords/*"),
    // Label printers and their calibrations (docs/labels.md, "Printers"): configuration
    ("POST", "/api/label-printers"),
    ("PUT", "/api/label-printers/*"),
    ("DELETE", "/api/label-printers/*"),
    ("POST", "/api/label-printers/*/default"),
    ("PUT", "/api/label-printers/*/calibration/*"),
    ("DELETE", "/api/label-printers/*/calibration/*"),
    ("POST", "/api/label-printers/certificate"),
    ("POST", "/api/roaster/*/control/pid"),
    ("POST", "/api/roaster/*/control/ror_limit"),
    ("POST", "/api/roaster/*/control/pid/undo"),
    ("POST", "/api/roaster/*/autotune/runs/*/apply"),
    ("POST", "/api/roaster/*/autotune/runs/*/analyse"),
    ("POST", "/api/roaster/*/tune/start"),
    ("POST", "/api/roaster/*/tune/stop"),
    ("PUT", "/api/roaster/*/fan_assist"),
    ("PUT", "/api/roaster/*/gain_schedule"),
    ("PUT", "/api/roaster/*/sequence"),
    ("POST", "/api/admin/mqtt/reset"),
    ("POST", "/api/test/emit-telemetry/*"),
    ("POST", "/api/test/emit-status/*"),
    ("DELETE", "/api/sessions/*"),
    ("POST", "/api/sessions/*/telemetry"),
    ("POST", "/api/sessions/*/profile"),
    ("POST", "/api/sessions/import/artisan"),
    ("POST", "/api/profiles"),
    ("PUT", "/api/profiles/*"),
    ("DELETE", "/api/profiles/*"),
    ("POST", "/api/profiles/import/artisan"),
    // A profile's share link makes it public; the import from a link makes a profile and
    // has the server read another server's page
    ("POST", "/api/profiles/*/share"),
    ("DELETE", "/api/profile-shares/*"),
    ("POST", "/api/profiles/import/link"),
    ("PUT", "/api/settings"),
    ("PUT", "/api/settings/*"),
    ("POST", "/api/notifications/test"),
    ("POST", "/api/backups"),
    ("POST", "/api/backups/*/check"),
    ("POST", "/api/devices"),
    ("PUT", "/api/devices/*"),
    ("DELETE", "/api/devices/*"),
    ("POST", "/api/devices/*/connections"),
    ("PUT", "/api/devices/*/connections/*"),
    ("DELETE", "/api/devices/*/connections/*"),
    ("PUT", "/api/devices/*/register-map"),
    // A roaster's setup (docs/roaster-setup.md, "Tokens and the setup"): at control off at
    // once; leaving control on waits for the owner's approval (approvals.rs)
    ("PUT", "/api/roaster/*/setup"),
    ("POST", "/api/devices/test-connection"),
    ("POST", "/api/device-profiles"),
    ("PUT", "/api/device-profiles/*"),
    ("DELETE", "/api/device-profiles/*"),
    ("PUT", "/api/virtual-roaster"),
    ("POST", "/api/push/subscriptions"),
    ("DELETE", "/api/push/subscriptions/*"),
    ("POST", "/api/push/unsubscribe"),
    ("POST", "/api/push/test"),
];

/// What a visitor to a public demo (`ROASTWARDEN_DEMO=1`, docs/DEPLOY.md "A public demo") may
/// do beyond the roast, plan, beans, labels and print rules: profiles and Artisan files, a
/// roast deleted, and the simulated roaster's own tuning and settings. The real-hardware rule
/// still holds, so all of it reaches only `virtual_roaster`.
const DEMO_EXTRA: &[(&str, &str)] = &[
    ("POST", "/api/profiles"),
    ("PUT", "/api/profiles/*"),
    ("DELETE", "/api/profiles/*"),
    ("POST", "/api/profiles/import/artisan"),
    ("POST", "/api/sessions/import/artisan"),
    ("DELETE", "/api/sessions/*"),
    ("PUT", "/api/roaster/*/fan_assist"),
    ("PUT", "/api/roaster/*/gain_schedule"),
    ("PUT", "/api/roaster/*/sequence"),
    ("POST", "/api/roaster/*/control/pid"),
    ("POST", "/api/roaster/*/control/pid/undo"),
    ("POST", "/api/roaster/*/control/ror_limit"),
    ("POST", "/api/roaster/*/tune/start"),
    ("POST", "/api/roaster/*/tune/stop"),
    ("POST", "/api/roaster/*/autotune/runs/*/analyse"),
    ("POST", "/api/roaster/*/autotune/runs/*/apply"),
    ("PUT", "/api/virtual-roaster"),
    // A visitor's own simulated roaster (demo.rs)
    ("POST", "/api/demo/roaster"),
];

/// What the demo leaves out of those rules: anything that has the server reach another host
/// (a supplier's page, a printer) or look around its network (mDNS), makes a public page
/// (share links), or keeps a file a visitor uploads (label assets, receipts). Roaster
/// connections, settings, backups, push, notifications and tokens are in no demo rule at all.
const DEMO_LEFT_OUT: &[(&str, &str)] = &[
    ("POST", "/api/beans/import"),
    ("POST", "/api/wanted/cart"),
    ("POST", "/api/orders/*/receipts"),
    ("GET", "/api/orders/*/receipts/*"),
    ("DELETE", "/api/orders/*/receipts/*"),
    ("PUT", "/api/label-assets/*"),
    ("DELETE", "/api/label-assets/*"),
    ("POST", "/api/sessions/*/labels/print"),
    ("POST", "/api/label-runs/*/print"),
    ("POST", "/api/sessions/*/share"),
    ("DELETE", "/api/share/*"),
    ("GET", "/api/devices/mdns"),
    ("GET", "/api/devices/discovered"),
];

/// Whether a demo visitor may make this request (`auth.rs`, [`crate::auth::Actor::Demo`]).
pub fn demo_allows(method: &str, path: &str) -> bool {
    let method = get_like(method);
    if named(DEMO_LEFT_OUT, method, path) || named(OWNER_ONLY, method, path) {
        return false;
    }
    (method == "GET" && READ.iter().any(|p| path_matches(p, path)))
        || [COMPUTE, ROAST, PLAN, BEANS, LABELS, PRINT, DEMO_EXTRA]
            .iter()
            .any(|rules| named(rules, method, path))
}

/// Requests only the owner key may make (or anyone, when `ROASTWARDEN_API_KEY` is unset), reads
/// included: tokens, the per-roaster switches (token access, control level), and restoring a backup, which would bring back
/// the tokens and switches of the day it was taken.
pub const OWNER_ONLY: &[(&str, &str)] = &[
    // The signed-in browsers (logins.rs)
    ("GET", "/api/auth/logins"),
    ("DELETE", "/api/auth/logins"),
    ("DELETE", "/api/auth/logins/*"),
    ("GET", "/api/tokens"),
    ("POST", "/api/tokens"),
    ("PUT", "/api/tokens/*"),
    // Approving is the owner's alone, so an agent can never approve itself
    ("GET", "/api/approvals"),
    ("POST", "/api/approvals/*/approve"),
    ("POST", "/api/approvals/*/deny"),
    ("POST", "/api/tokens/*/revoke"),
    ("DELETE", "/api/tokens/*"),
    ("PUT", "/api/roaster/*/token-access"),
    // The write check sends heater 10 % to a roaster (docs/add-roaster.md)
    ("POST", "/api/roaster/*/check/write"),
    ("POST", "/api/backups/*/restore"),
    ("DELETE", "/api/backups/restore"),
];

/// Routes no token uses, for a reason other than ownership: pages for a browser, and the
/// device WebSocket, which a roaster opens with the owner key.
#[cfg(test)]
const NOT_FOR_TOKENS: &[(&str, &str)] = &[
    // A browser signs in and out with a cookie; a script sends its token with each call
    ("POST", "/api/auth/login"),
    ("POST", "/api/auth/logout"),
    // A demo visitor's browser takes its own simulated roaster (demo.rs)
    ("POST", "/api/demo/roaster"),
    ("GET", "/app"),
    ("GET", "/app/*"),
    ("GET", "/docs"),
    ("GET", "/ws/device/*/telemetry"),
    // The public share page: for anyone with the link, no token needed
    ("GET", "/R/*"),
    ("GET", "/r/*"),
    // A shared profile's page and files: the same
    ("GET", "/P/*"),
    ("GET", "/p/*"),
    ("GET", "/P/*/*"),
    ("GET", "/p/*/*"),
];

fn get_like(method: &str) -> &str {
    if method == "HEAD" { "GET" } else { method }
}

fn named(rules: &[(&str, &str)], method: &str, path: &str) -> bool {
    rules
        .iter()
        .any(|(m, p)| *m == method && path_matches(p, path))
}

impl Preset {
    pub const ALL: [Preset; 4] = [
        Preset::ReadOnly,
        Preset::Roast,
        Preset::Inventory,
        Preset::Admin,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Preset::ReadOnly => "read_only",
            Preset::Roast => "roast",
            Preset::Inventory => "inventory",
            Preset::Admin => "admin",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == s)
    }

    pub fn label(self) -> &'static str {
        match self {
            Preset::ReadOnly => "read-only",
            Preset::Roast => "roast",
            Preset::Inventory => "inventory",
            Preset::Admin => "admin",
        }
    }

    /// Whether a token with this preset may make this request. Deny by default: the method
    /// and every path segment must match a rule exactly (`*` is one non-empty segment).
    pub fn allows(self, method: &str, path: &str) -> bool {
        let method = get_like(method);
        let read = (method == "GET" && READ.iter().any(|p| path_matches(p, path)))
            || named(COMPUTE, method, path);
        match self {
            Preset::ReadOnly => read,
            Preset::Roast => {
                read || [ROAST, PLAN, PRINT]
                    .iter()
                    .any(|rules| named(rules, method, path))
            }
            Preset::Inventory => {
                read || [BEANS, PLAN, LABELS, PRINT]
                    .iter()
                    .any(|rules| named(rules, method, path))
            }
            Preset::Admin => {
                read || [ROAST, PLAN, BEANS, LABELS, PRINT, ADMIN]
                    .iter()
                    .any(|rules| named(rules, method, path))
            }
        }
    }
}

/// Whether only the owner key may make this request.
pub fn is_owner_only(method: &str, path: &str) -> bool {
    named(OWNER_ONLY, get_like(method), path)
}

/// `pattern` and `path` have the same segments, `*` standing for any one non-empty segment.
/// One trailing slash on the path is ignored. Nothing is decoded or normalised, exactly as
/// the router does not, so a path that matches here is the path the router sees.
pub fn path_matches(pattern: &str, path: &str) -> bool {
    let path = match path.strip_suffix('/') {
        Some(p) if !p.is_empty() => p,
        _ => path,
    };
    let mut want = pattern.split('/');
    let mut have = path.split('/');
    loop {
        match (want.next(), have.next()) {
            (None, None) => return true,
            (Some("*"), Some(seg)) if !seg.is_empty() => {}
            (Some(a), Some(b)) if a == b => {}
            _ => return false,
        }
    }
}

/// The token a request authenticated with. Put on the request (and its response) by the
/// auth middleware and named in the audit row it writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenAuth {
    pub id: i64,
    pub name: String,
    pub preset: Preset,
    /// Its gated calls wait for the owner (`approvals.rs`)
    pub require_approval: bool,
}

/// What became of a presented token.
#[derive(Debug)]
pub enum TokenCheck {
    /// Good; `touch` says whether its last-used time is due for an update.
    Valid {
        token: TokenAuth,
        touch: bool,
    },
    Malformed,
    Unknown,
    Expired {
        id: i64,
        name: String,
    },
    Revoked {
        id: i64,
        name: String,
    },
    /// The lookup itself failed; refuse, and the log says why.
    Error,
}

/// A token as the database keeps it (never with its secret).
#[derive(Debug, Clone, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct ApiToken {
    pub id: i64,
    pub name: String,
    pub prefix: String,
    pub preset: String,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub last_used_ip: Option<String>,
    pub revoked_at: Option<String>,
    /// Starting a roast on a physical roaster and deletes wait for the owner's approval
    /// (docs/control.md, "Approvals")
    pub require_approval: bool,
}

const COLUMNS: &str = "id, name, prefix, preset, created_at, expires_at, last_used_at, \
     last_used_ip, revoked_at, require_approval";

fn parse_time(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|t| t.with_timezone(&chrono::Utc))
}

pub fn now_text() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Look a presented token up and judge it at `now`.
pub async fn check(db: &SqlitePool, value: &str, now: chrono::DateTime<chrono::Utc>) -> TokenCheck {
    if !well_formed(value) {
        return TokenCheck::Malformed;
    }
    let row = sqlx::query_as::<_, ApiToken>(AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM api_tokens WHERE token_hash = ?"
    )))
    .bind(hash(value))
    .fetch_optional(db)
    .await;
    match row {
        Ok(Some(row)) => judge(row, now),
        Ok(None) => TokenCheck::Unknown,
        Err(e) => {
            tracing::error!(error = %e, "API token lookup failed");
            TokenCheck::Error
        }
    }
}

/// Judge a token by its id at `now`: a browser signed in with it (`logins.rs`).
pub async fn check_id(db: &SqlitePool, id: i64, now: chrono::DateTime<chrono::Utc>) -> TokenCheck {
    match get(db, id).await {
        Ok(Some(row)) => judge(row, now),
        Ok(None) => TokenCheck::Unknown,
        Err(e) => {
            tracing::error!(error = %e, "API token lookup failed");
            TokenCheck::Error
        }
    }
}

fn judge(row: ApiToken, now: chrono::DateTime<chrono::Utc>) -> TokenCheck {
    if row.revoked_at.is_some() {
        return TokenCheck::Revoked {
            id: row.id,
            name: row.name,
        };
    }
    if let Some(exp) = row.expires_at.as_deref() {
        // An expiry that does not parse counts as passed: refuse rather than guess.
        if parse_time(exp).is_none_or(|t| t <= now) {
            return TokenCheck::Expired {
                id: row.id,
                name: row.name,
            };
        }
    }
    let Some(preset) = Preset::parse(&row.preset) else {
        tracing::error!(token_id = row.id, preset = %row.preset, "API token has an unknown preset");
        return TokenCheck::Error;
    };
    let touch = row
        .last_used_at
        .as_deref()
        .and_then(parse_time)
        .is_none_or(|at| (now - at).num_seconds() >= TOUCH_INTERVAL_SECS);
    TokenCheck::Valid {
        token: TokenAuth {
            id: row.id,
            name: row.name,
            preset,
            require_approval: row.require_approval,
        },
        touch,
    }
}

/// Record that a token was just used, from `ip`.
pub async fn touch(db: &SqlitePool, id: i64, ip: Option<String>) {
    if let Err(e) =
        sqlx::query("UPDATE api_tokens SET last_used_at = ?, last_used_ip = ? WHERE id = ?")
            .bind(now_text())
            .bind(ip)
            .bind(id)
            .execute(db)
            .await
    {
        tracing::warn!(error = %e, token_id = id, "could not record a token's last use");
    }
}

pub async fn list(db: &SqlitePool) -> anyhow::Result<Vec<ApiToken>> {
    Ok(sqlx::query_as::<_, ApiToken>(AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM api_tokens ORDER BY revoked_at IS NOT NULL, created_at DESC"
    )))
    .fetch_all(db)
    .await?)
}

/// Why a token could not be created (a 400 or 409, said in words).
#[derive(Debug)]
pub enum CreateError {
    Invalid(String),
    NameTaken,
    Db(anyhow::Error),
}

/// Create a token; the secret is in the result and nowhere else.
pub async fn create(
    db: &SqlitePool,
    name: &str,
    preset: Preset,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    require_approval: bool,
) -> Result<(ApiToken, String), CreateError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 60 {
        return Err(CreateError::Invalid(
            "a token needs a name of 1 to 60 characters".into(),
        ));
    }
    let new = generate();
    let expires = expires_at.map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
    let rows = sqlx::query_as::<_, ApiToken>(AssertSqlSafe(format!(
        "INSERT INTO api_tokens (name, prefix, token_hash, preset, created_at, expires_at,
         require_approval) VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING {COLUMNS}"
    )))
    .bind(name)
    .bind(&new.prefix)
    .bind(&new.hash)
    .bind(preset.as_str())
    .bind(now_text())
    .bind(expires)
    .bind(require_approval)
    .fetch_all(db)
    .await;
    match rows {
        Ok(mut rows) => rows
            .pop()
            .map(|row| (row, new.secret))
            .ok_or_else(|| CreateError::Db(anyhow::anyhow!("the insert returned no row"))),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(CreateError::NameTaken),
        Err(e) => Err(CreateError::Db(e.into())),
    }
}

/// One token (never with its secret).
pub async fn get(db: &SqlitePool, id: i64) -> anyhow::Result<Option<ApiToken>> {
    Ok(sqlx::query_as::<_, ApiToken>(AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM api_tokens WHERE id = ?"
    )))
    .bind(id)
    .fetch_optional(db)
    .await?)
}

/// Whether the token is good now: not revoked, not expired, a known preset.
pub fn active(row: &ApiToken, now: chrono::DateTime<chrono::Utc>) -> Option<TokenAuth> {
    match judge(row.clone(), now) {
        TokenCheck::Valid { token, .. } => Some(token),
        _ => None,
    }
}

/// Turn approvals on or off for a token. `None` when there is no such token.
pub async fn set_require_approval(
    db: &SqlitePool,
    id: i64,
    on: bool,
) -> anyhow::Result<Option<ApiToken>> {
    let mut rows = sqlx::query_as::<_, ApiToken>(AssertSqlSafe(format!(
        "UPDATE api_tokens SET require_approval = ? WHERE id = ? RETURNING {COLUMNS}"
    )))
    .bind(on)
    .bind(id)
    .fetch_all(db)
    .await?;
    Ok(rows.pop())
}

/// Revoke a token now. `None` when there is no such token.
pub async fn revoke(db: &SqlitePool, id: i64) -> anyhow::Result<Option<ApiToken>> {
    let mut rows = sqlx::query_as::<_, ApiToken>(AssertSqlSafe(format!(
        "UPDATE api_tokens SET revoked_at = COALESCE(revoked_at, ?) WHERE id = ? RETURNING {COLUMNS}"
    )))
    .bind(now_text())
    .bind(id)
    .fetch_all(db)
    .await?;
    Ok(rows.pop())
}

/// Remove a token for good (its audit rows keep its name). Whether there was one.
pub async fn delete(db: &SqlitePool, id: i64) -> anyhow::Result<bool> {
    let r = sqlx::query("DELETE FROM api_tokens WHERE id = ?")
        .bind(id)
        .execute(db)
        .await?;
    Ok(r.rows_affected() > 0)
}

/// The owner's per-roaster switch.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct RoasterAccess {
    pub device_id: String,
    /// Tokens may change things for this roaster (always true for the virtual roaster).
    pub tokens_may_control: bool,
    /// The virtual roaster needs no switch.
    pub always: bool,
    pub updated_at: Option<String>,
}

pub async fn roaster_access(db: &SqlitePool, device_id: &str) -> anyhow::Result<RoasterAccess> {
    if crate::sim::virtual_roaster::is_simulated(device_id) {
        return Ok(RoasterAccess {
            device_id: device_id.to_string(),
            tokens_may_control: true,
            always: true,
            updated_at: None,
        });
    }
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT tokens_may_control, updated_at FROM roaster_token_access WHERE device_id = ?",
    )
    .bind(device_id)
    .fetch_optional(db)
    .await?;
    Ok(RoasterAccess {
        device_id: device_id.to_string(),
        tokens_may_control: row.as_ref().is_some_and(|r| r.0 != 0),
        always: false,
        updated_at: row.map(|r| r.1),
    })
}

pub async fn set_roaster_access(
    db: &SqlitePool,
    device_id: &str,
    allowed: bool,
) -> anyhow::Result<RoasterAccess> {
    sqlx::query(
        "INSERT INTO roaster_token_access (device_id, tokens_may_control, updated_at)
         VALUES (?, ?, ?)
         ON CONFLICT(device_id) DO UPDATE SET
           tokens_may_control = excluded.tokens_may_control, updated_at = excluded.updated_at",
    )
    .bind(device_id)
    .bind(allowed as i64)
    .bind(now_text())
    .execute(db)
    .await?;
    roaster_access(db, device_id).await
}

/// The real-hardware rule. For a token's request that changes something, the roaster it
/// concerns (from `/api/roaster/{d}/…`, or the session's roaster for `/api/sessions/{id}/…`),
/// when that roaster is not the virtual one and the owner has not allowed tokens for it.
/// `Err` when the switch could not be read: refuse.
pub async fn hardware_refusal(
    db: &SqlitePool,
    method: &str,
    path: &str,
) -> Result<Option<String>, ()> {
    if matches!(method, "GET" | "HEAD" | "OPTIONS") {
        return Ok(None);
    }
    if finished_roast_record(db, method, path).await || never_reaches_a_roaster(method, path) {
        return Ok(None);
    }
    let Some(device) = crate::auth::audit::device_for(db, path).await else {
        return Ok(None);
    };
    match roaster_access(db, &device).await {
        Ok(a) if a.tokens_may_control => Ok(None),
        Ok(_) => Ok(Some(device)),
        Err(e) => {
            tracing::error!(error = %e, %device, "could not read the roaster's token access");
            Err(())
        }
    }
}

/// What is under a roast's path but never reaches its roaster, whatever the roast's state:
/// printing its labels and sharing it (docs/labels.md). The token's preset still decides.
fn never_reaches_a_roaster(method: &str, path: &str) -> bool {
    let seg: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    matches!(
        (method, seg.as_slice()),
        (
            "POST",
            ["api", "sessions", _, "labels", "print"]
                | ["api", "sessions", _, "share"]
                | ["api", "roaster", _, "ingest", "preview"]
        ) | ("PUT", ["api", "roaster", _, "setup"])
    )
}

/// The record of a finished roast: its details and weights, landmarks, cupping, and saving it
/// as a profile. None of it reaches the roaster (the roast sequence and Profile drive act only
/// on an active roast), so the real-hardware rule does not hold it back: an agent can enter
/// the roasted weight or move first crack after the owner roasted. Starting, resuming or
/// driving a roast, and anything on a roast not yet finished, stay behind the switch.
async fn finished_roast_record(db: &SqlitePool, method: &str, path: &str) -> bool {
    let seg: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    let (id, rest) = match seg.as_slice() {
        ["api", "sessions", id, rest @ ..] if !id.is_empty() => (*id, rest),
        _ => return false,
    };
    let record = matches!(
        (method, rest),
        ("PUT", [])
            | ("POST", ["events" | "profile"])
            | ("PUT" | "DELETE", ["events", _])
            | ("POST" | "DELETE", ["cupping"])
    );
    if !record {
        return false;
    }
    sqlx::query_scalar::<_, String>("SELECT status FROM roast_sessions WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .is_some_and(|st| st == "completed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_have_the_documented_shape() {
        let t = generate();
        assert!(t.secret.starts_with("rr_"));
        assert_eq!(t.secret.len(), 43);
        assert!(well_formed(&t.secret));
        assert_eq!(t.prefix, &t.secret[..9]);
        assert_eq!(t.hash, hash(&t.secret));
        assert_eq!(t.hash.len(), 64);
        assert_ne!(generate().secret, t.secret);
    }

    #[test]
    fn malformed_tokens_are_recognised() {
        assert!(!well_formed("rr_short"));
        assert!(!well_formed(&format!("rr_{}", "a".repeat(41))));
        assert!(!well_formed(&format!("rr_{}-", "a".repeat(39))));
        assert!(!well_formed(&format!("xy_{}", "a".repeat(40))));
        assert!(well_formed(&format!("rr_{}", "aZ9".repeat(13) + "q")));
        assert!(!is_token("an-owner-key"));
    }

    #[test]
    fn printing_and_sharing_a_roast_never_reach_its_roaster() {
        assert!(never_reaches_a_roaster(
            "POST",
            "/api/sessions/abc/labels/print"
        ));
        assert!(never_reaches_a_roaster("POST", "/api/sessions/abc/share"));
        assert!(!never_reaches_a_roaster("POST", "/api/sessions/abc/start"));
        assert!(!never_reaches_a_roaster(
            "POST",
            "/api/sessions/abc/share/x"
        ));
        assert!(!never_reaches_a_roaster("PUT", "/api/sessions/abc/share"));
    }

    #[test]
    fn paths_match_segment_by_segment() {
        assert!(path_matches("/api/sessions", "/api/sessions"));
        assert!(path_matches("/api/sessions", "/api/sessions/"));
        assert!(path_matches("/api/sessions/*", "/api/sessions/12"));
        assert!(!path_matches("/api/sessions/*", "/api/sessions"));
        assert!(!path_matches("/api/sessions/*", "/api/sessions//"));
        assert!(!path_matches("/api/sessions/*", "/api/sessions/1/start"));
        assert!(!path_matches(
            "/api/sessions/*/start",
            "/api/sessions//start"
        ));
        assert!(!path_matches("/api/sessions", "/API/sessions"));
        assert!(!path_matches("/api/sessions", "/api/sessionsx"));
    }

    #[test]
    fn the_demo_roasts_and_records_but_reaches_nowhere() {
        let allowed = [
            ("GET", "/api/sessions"),
            ("GET", "/api/settings"),
            ("POST", "/api/roaster/virtual_roaster/control/heater"),
            ("POST", "/api/virtual-roaster/charge"),
            ("POST", "/api/sessions"),
            ("POST", "/api/sessions/abc/replay"),
            ("POST", "/api/sessions/abc/cupping"),
            ("DELETE", "/api/sessions/abc"),
            ("POST", "/api/plan"),
            ("POST", "/api/beans"),
            ("POST", "/api/beans/abc/stock"),
            ("POST", "/api/purchases"),
            ("POST", "/api/label-designs"),
            ("POST", "/api/labels/render"),
            ("POST", "/api/label-runs"),
            ("POST", "/api/label-runs/abc/reprint"),
            ("POST", "/api/profiles"),
            ("POST", "/api/profiles/import/artisan"),
            ("PUT", "/api/roaster/virtual_roaster/fan_assist"),
            ("POST", "/api/roaster/virtual_roaster/tune/start"),
        ];
        for (m, p) in allowed {
            assert!(demo_allows(m, p), "the demo should allow {m} {p}");
        }
        let refused = [
            // Another host, or the demo host's network
            ("POST", "/api/devices"),
            ("POST", "/api/devices/abc/connections"),
            ("POST", "/api/devices/test-connection"),
            ("POST", "/api/beans/import"),
            ("POST", "/api/wanted/cart"),
            ("POST", "/api/profiles/import/link"),
            ("POST", "/api/label-printers"),
            ("POST", "/api/sessions/abc/labels/print"),
            ("POST", "/api/label-runs/abc/print"),
            ("POST", "/api/push/subscriptions"),
            ("POST", "/api/notifications/test"),
            ("GET", "/api/devices/mdns"),
            ("GET", "/api/devices/discovered"),
            // A public page, a visitor's file
            ("POST", "/api/sessions/abc/share"),
            ("POST", "/api/profiles/abc/share"),
            ("PUT", "/api/label-assets/logo.png"),
            ("POST", "/api/orders/abc/receipts"),
            ("GET", "/api/orders/abc/receipts/1"),
            // The server itself
            ("PUT", "/api/settings"),
            ("PUT", "/api/settings/public_url"),
            ("POST", "/api/backups"),
            ("POST", "/api/backups/abc/restore"),
            ("PUT", "/api/roaster/virtual_roaster/setup"),
            ("PUT", "/api/roaster/virtual_roaster/token-access"),
            ("POST", "/api/roaster/virtual_roaster/check/write"),
            ("POST", "/api/tokens"),
            ("GET", "/api/tokens"),
            ("GET", "/api/audit"),
            ("GET", "/api/support-bundle"),
            ("GET", "/api/approvals"),
            ("POST", "/api/test/emit-telemetry/virtual_roaster"),
            ("GET", "/ws/device/virtual_roaster/telemetry"),
        ];
        for (m, p) in refused {
            assert!(!demo_allows(m, p), "the demo should refuse {m} {p}");
        }
    }

    #[test]
    fn presets_allow_exactly_what_they_name() {
        use Preset::*;
        let all: &[Preset] = &[ReadOnly, Roast, Inventory, Admin];
        let roast: &[Preset] = &[Roast, Admin];
        let plan: &[Preset] = &[Roast, Inventory, Admin];
        let beans: &[Preset] = &[Inventory, Admin];
        let admin: &[Preset] = &[Admin];
        let none: &[Preset] = &[];
        let cases: &[(&str, &str, &[Preset])] = &[
            ("GET", "/api/sessions", all),
            ("HEAD", "/api/sessions", all),
            ("GET", "/version", all),
            ("GET", "/ws/v2/telemetry", all),
            ("GET", "/api/audit", admin),
            ("GET", "/api/roaster/virtual_roaster/token-access", all),
            (
                "POST",
                "/api/roaster/virtual_roaster/control/setpoint",
                roast,
            ),
            (
                "POST",
                "/api/roaster/virtual_roaster/control/emergency_stop",
                roast,
            ),
            ("POST", "/api/roaster/virtual_roaster/control/pid", admin),
            ("POST", "/api/roaster/virtual_roaster/tune/start", admin),
            ("PUT", "/api/roaster/virtual_roaster/fan_assist", admin),
            ("POST", "/api/sessions", roast),
            ("POST", "/api/sessions/abc/start", roast),
            ("POST", "/api/sessions/abc/events", roast),
            ("PATCH", "/api/sessions/abc/replay", roast),
            ("DELETE", "/api/sessions/abc", admin),
            ("POST", "/api/sessions/import/artisan", admin),
            ("POST", "/api/plan", plan),
            ("POST", "/api/plan/reorder", plan),
            ("PUT", "/api/plan/abc", plan),
            ("DELETE", "/api/plan/abc", plan),
            ("GET", "/api/beans", all),
            ("POST", "/api/beans", beans),
            ("PUT", "/api/beans/abc", beans),
            ("DELETE", "/api/beans/abc", beans),
            ("POST", "/api/beans/abc/stock", beans),
            ("PUT", "/api/beans/abc/moves/m1", beans),
            ("DELETE", "/api/beans/abc/moves/m1", beans),
            ("POST", "/api/purchases", beans),
            ("POST", "/api/beans/import", beans),
            ("GET", "/api/wanted", all),
            ("POST", "/api/wanted", beans),
            ("PUT", "/api/wanted/w1", beans),
            ("DELETE", "/api/wanted/w1", beans),
            ("POST", "/api/wanted/w1/bought", beans),
            ("POST", "/api/wanted/cart", beans),
            ("GET", "/api/orders", all),
            ("GET", "/api/orders/o1", all),
            ("POST", "/api/orders", beans),
            ("POST", "/api/orders/o1/received", beans),
            ("DELETE", "/api/orders/o1", beans),
            ("POST", "/api/orders/o1/receipts", beans),
            // A receipt's file: not for a read-only token
            ("GET", "/api/orders/o1/receipts/r1", beans),
            ("HEAD", "/api/orders/o1/receipts/r1", beans),
            ("GET", "/api/beans/suggestions", all),
            ("GET", "/api/costs", all),
            ("GET", "/api/sessions/abc/cost", all),
            ("POST", "/api/profiles", admin),
            ("PUT", "/api/settings/alarms", admin),
            ("PUT", "/api/settings", admin),
            ("GET", "/api/settings/notify_gotify_token", all),
            ("GET", "/api/notifications", all),
            ("GET", "/api/home-assistant", all),
            ("GET", "/api/support-bundle", admin),
            ("POST", "/api/notifications/test", admin),
            ("GET", "/api/about", all),
            ("GET", "/api/busy", all),
            ("POST", "/api/backups", admin),
            ("POST", "/api/backups/x.db/check", admin),
            ("POST", "/api/virtual-roaster/charge", roast),
            ("PUT", "/api/virtual-roaster", admin),
            ("GET", "/api/approvals/Zx9_abc", all),
            // The owner's alone
            ("GET", "/api/approvals", none),
            ("POST", "/api/approvals/Zx9_abc/approve", none),
            ("POST", "/api/approvals/Zx9_abc/deny", none),
            ("PUT", "/api/tokens/1", none),
            ("GET", "/api/tokens", none),
            ("POST", "/api/tokens", none),
            ("POST", "/api/tokens/1/revoke", none),
            ("DELETE", "/api/tokens/1", none),
            ("PUT", "/api/roaster/virtual_roaster/token-access", none),
            ("POST", "/api/backups/x.db/restore", none),
            ("DELETE", "/api/backups/restore", none),
            ("GET", "/ws/device/virtual_roaster/telemetry", none),
            ("GET", "/", none),
            ("GET", "/settings/tokens", none),
            // Near misses
            ("GET", "/api/sessions/1/events/extra", none),
            ("GET", "/api/sessions/../tokens", none),
            ("GET", "/api/%74okens", none),
            ("PATCH", "/api/sessions/1", none),
            ("POST", "/api/roaster//control/setpoint", none),
        ];
        for (method, path, allowed) in cases {
            for preset in Preset::ALL {
                assert_eq!(
                    preset.allows(method, path),
                    allowed.contains(&preset),
                    "{preset:?} {method} {path}"
                );
            }
        }
        assert!(is_owner_only("GET", "/api/tokens"));
        assert!(is_owner_only("HEAD", "/api/tokens"));
        assert!(is_owner_only("PUT", "/api/roaster/esp32/token-access"));
        assert!(!is_owner_only("GET", "/api/roaster/esp32/token-access"));
        assert!(!is_owner_only("POST", "/api/sessions"));
    }

    /// Every `.route(…)` in the server's source, as (METHOD, path) with axum's `{param}`
    /// segments. Reads the source rather than the router, which cannot list its routes.
    fn source_routes() -> Vec<(String, String)> {
        // The routes outside `routes!` are in `api_router`; `router` above it in the same file
        // adds the demo's /robots.txt, which is not the API's; the file's tests are not scanned
        let build = include_str!("../app/build.rs");
        let from = build
            .find("fn api_router")
            .expect("app/build.rs has api_router");
        let to = build.find("#[cfg(test)]").unwrap_or(build.len());
        let api_router = &build[from..to];
        let files = [
            api_router,
            include_str!("../routes/about.rs"),
            include_str!("../routes/analysis.rs"),
            include_str!("../routes/approvals.rs"),
            include_str!("../routes/tune.rs"),
            include_str!("../routes/backups.rs"),
            include_str!("../routes/contract.rs"),
            include_str!("../routes/control.rs"),
            include_str!("../routes/devices.rs"),
            include_str!("../routes/fan_assist.rs"),
            include_str!("../routes/firmware.rs"),
            include_str!("../routes/mqtt_passwords.rs"),
            include_str!("../routes/home_assistant.rs"),
            include_str!("../routes/support.rs"),
            include_str!("../routes/inventory.rs"),
            include_str!("../routes/labels.rs"),
            include_str!("../routes/label_studio.rs"),
            include_str!("../routes/label_runs.rs"),
            include_str!("../routes/label_printers.rs"),
            include_str!("../routes/notifications.rs"),
            include_str!("../routes/orders.rs"),
            include_str!("../routes/predict.rs"),
            include_str!("../routes/profiles.rs"),
            include_str!("../routes/profile_share.rs"),
            include_str!("../routes/push.rs"),
            include_str!("../routes/replay.rs"),
            include_str!("../routes/reports.rs"),
            include_str!("../routes/roasted.rs"),
            include_str!("../routes/roast_events.rs"),
            include_str!("../routes/schedule.rs"),
            include_str!("../routes/sequence.rs"),
            include_str!("../routes/sessions.rs"),
            include_str!("../routes/settings.rs"),
            include_str!("../routes/share.rs"),
            include_str!("../routes/system.rs"),
            include_str!("../routes/telemetry.rs"),
            include_str!("../routes/tokens.rs"),
            include_str!("../routes/trace.rs"),
            include_str!("../routes/tuning.rs"),
            include_str!("../routes/virtual_roaster.rs"),
        ];
        // A new routes file must be added above.
        let modules = include_str!("../routes/mod.rs")
            .lines()
            .filter(|l| l.starts_with("pub mod "))
            .count();
        assert_eq!(modules, files.len() - 1, "a routes/ file is not scanned");

        let mut out = Vec::new();
        for file in files {
            let src: String = file
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");
            let b = src.as_bytes();
            let mut i = 0;
            while let Some(at) = src[i..].find(".route(") {
                let open = i + at + ".route(".len();
                let q1 = open + src[open..].find('"').unwrap();
                let q2 = q1 + 1 + src[q1 + 1..].find('"').unwrap();
                let path = &src[q1 + 1..q2];
                let (mut depth, mut p) = (1, open);
                while depth > 0 {
                    match b[p] {
                        b'(' => depth += 1,
                        b')' => depth -= 1,
                        _ => {}
                    }
                    p += 1;
                }
                let span = &src[q2 + 1..p];
                for m in ["get", "post", "put", "delete", "patch"] {
                    let mut j = 0;
                    while let Some(k) = span[j..].find(&format!("{m}(")) {
                        let start = j + k;
                        let before = span[..start].chars().next_back();
                        if !before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
                            out.push((m.to_ascii_uppercase(), path.to_string()));
                        }
                        j = start + m.len();
                    }
                }
                i = p;
            }
        }
        // Routes registered with `routes!` are in the OpenAPI document, not in `.route(` calls
        for (path, item) in crate::openapi::document().paths.paths {
            let ops = [
                ("GET", &item.get),
                ("POST", &item.post),
                ("PUT", &item.put),
                ("DELETE", &item.delete),
                ("PATCH", &item.patch),
            ];
            for (m, op) in ops {
                if op.is_some() {
                    out.push((m.to_string(), path.clone()));
                }
            }
        }
        out
    }

    fn concrete(template: &str) -> String {
        template
            .split('/')
            .map(|s| if s.starts_with('{') { "x1" } else { s })
            .collect::<Vec<_>>()
            .join("/")
    }

    /// Every route the server answers is classified: some preset may use it, or it is the
    /// owner's, or it is not for tokens. A route added without a rule fails here (and is
    /// refused to every token meanwhile, which is the safe failure).
    #[test]
    fn every_route_is_classified() {
        let routes = source_routes();
        assert!(routes.len() > 130, "the scan found only {}", routes.len());
        assert!(routes.contains(&("PATCH".into(), "/api/sessions/{id}/replay".into())));
        assert!(routes.contains(&("POST".into(), "/api/tokens/{id}/revoke".into())));
        let mut unclassified = Vec::new();
        for (method, template) in &routes {
            let path = concrete(template);
            let classified = Preset::ALL.iter().any(|p| p.allows(method, &path))
                || is_owner_only(method, &path)
                || named(NOT_FOR_TOKENS, method, &path);
            if !classified {
                unclassified.push(format!("{method} {template}"));
            }
        }
        assert!(
            unclassified.is_empty(),
            "routes no rule mentions: {unclassified:?}"
        );

        // and every rule names a route that exists
        let exists = |method: &str, pattern: &str| {
            routes
                .iter()
                .any(|(m, t)| m == method && path_matches(pattern, &concrete(t)))
        };
        let mut stale = Vec::new();
        for p in READ {
            if !exists("GET", p) {
                stale.push(format!("GET {p}"));
            }
        }
        for (m, p) in ROAST
            .iter()
            .chain(COMPUTE)
            .chain(LABELS)
            .chain(PRINT)
            .chain(PLAN)
            .chain(BEANS)
            .chain(ADMIN)
            .chain(OWNER_ONLY)
            .chain(NOT_FOR_TOKENS)
        {
            if !exists(m, p) {
                stale.push(format!("{m} {p}"));
            }
        }
        assert!(
            stale.is_empty(),
            "rules for routes that do not exist: {stale:?}"
        );
    }

    /// What a token may do is in the OpenAPI document, so an agent can read how to call it.
    /// The WebSockets are described in docs/websocket-v2.md instead.
    #[test]
    fn every_route_a_preset_allows_is_documented() {
        let doc = serde_json::to_value(crate::openapi::document()).unwrap();
        let paths = doc["paths"].as_object().unwrap();
        let documented = |method: &str, path: &str| {
            paths.iter().any(|(template, item)| {
                path_matches(&template_pattern(template), path)
                    && item.get(method.to_ascii_lowercase()).is_some()
            })
        };
        let mut missing = Vec::new();
        let reads: Vec<(&str, &str)> = READ.iter().map(|p| ("GET", *p)).collect();
        for (method, pattern) in reads
            .iter()
            .chain(ROAST)
            .chain(PLAN)
            .chain(BEANS)
            .chain(ADMIN)
        {
            if pattern.starts_with("/ws/") || *pattern == "/api-docs/openapi.json" {
                continue;
            }
            if !documented(method, &pattern.replace('*', "x1")) {
                missing.push(format!("{method} {pattern}"));
            }
        }
        assert!(missing.is_empty(), "allowed but undocumented: {missing:?}");
    }

    /// `/api/sessions/{id}` as a rule pattern, `/api/sessions/*`
    fn template_pattern(template: &str) -> String {
        template
            .split('/')
            .map(|s| if s.starts_with('{') { "*" } else { s })
            .collect::<Vec<_>>()
            .join("/")
    }

    #[test]
    fn presets_round_trip_their_names() {
        for p in Preset::ALL {
            assert_eq!(Preset::parse(p.as_str()), Some(p));
            assert_eq!(
                serde_json::to_value(p).unwrap(),
                serde_json::json!(p.as_str())
            );
        }
        assert_eq!(Preset::parse("full_admin"), None);
    }

    fn row(
        expires_at: Option<&str>,
        revoked_at: Option<&str>,
        last_used_at: Option<&str>,
    ) -> ApiToken {
        ApiToken {
            id: 7,
            name: "agent".into(),
            prefix: "rr_abcdef".into(),
            preset: "roast".into(),
            created_at: "2026-10-01T00:00:00Z".into(),
            expires_at: expires_at.map(Into::into),
            last_used_at: last_used_at.map(Into::into),
            last_used_ip: None,
            revoked_at: revoked_at.map(Into::into),
            require_approval: false,
        }
    }

    #[test]
    fn tokens_are_judged_by_revocation_expiry_and_preset() {
        let now = parse_time("2026-10-24T12:00:00Z").unwrap();
        assert!(matches!(
            judge(row(None, None, None), now),
            TokenCheck::Valid { touch: true, .. }
        ));
        assert!(matches!(
            judge(row(None, None, Some("2026-10-24T11:59:30Z")), now),
            TokenCheck::Valid { touch: false, .. }
        ));
        assert!(matches!(
            judge(row(Some("2026-10-24T11:00:00Z"), None, None), now),
            TokenCheck::Expired { id: 7, .. }
        ));
        assert!(matches!(
            judge(row(Some("not a time"), None, None), now),
            TokenCheck::Expired { .. }
        ));
        assert!(matches!(
            judge(
                row(
                    Some("2027-01-01T00:00:00Z"),
                    Some("2026-10-02T00:00:00Z"),
                    None
                ),
                now
            ),
            TokenCheck::Revoked { .. }
        ));
        let mut bad = row(None, None, None);
        bad.preset = "full_admin".into();
        assert!(matches!(judge(bad, now), TokenCheck::Error));
    }
}
