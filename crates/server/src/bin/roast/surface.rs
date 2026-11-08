//! The agent surface (docs/agents.md, "The check"): every operation of the API and how an
//! agent reaches it. A `roast` command with the least preset a token needs for it, or only
//! `roast api` (rare), or no agent use, each with its reason. `roast openapi --find` prints
//! the command next to each operation.
//!
//! The tests make the table impossible to forget: an operation without an entry fails, an
//! entry without an operation fails, a command that clap does not know, that `docs/cli.md`
//! does not list or the skill does not mention fails, and so does a preset that cannot reach
//! the call or is more than it needs.

use roastwarden_server::presets::Preset;

/// How an agent reaches one operation.
#[derive(Debug, Clone, Copy)]
pub enum Via {
    /// `roast <command> [--flag]`: the subcommand path and the flags that make this call, and
    /// the least preset a token needs (`ReadOnly` for reads, which need no token at all)
    Cmd(&'static str, Preset),
    /// Only with `roast api METHOD PATH`; why there is no command
    Api(&'static str),
    /// Not for agents; why
    None(&'static str),
}

const R: Preset = Preset::ReadOnly;
const ROAST: Preset = Preset::Roast;
const I: Preset = Preset::Inventory;
const A: Preset = Preset::Admin;

const OWNER: &str = "the owner key's alone (docs/control.md, \"Tokens\")";
const LIVE: &str = "live control is for the person at the roaster (Roast page); an agent drives only virtual_roaster, with `roast api` (the real-hardware rule)";
const TUNE: &str = "tuning runs while someone watches the roaster (docs/autotune-2.md); on virtual_roaster with `roast api`";
const SETUP: &str = "setting up a roaster's connection, done once in Roasters";
const LEGACY: &str = "legacy alias of Profile drive (`/replay`)";
const BROWSER: &str = "a browser signs itself up for Web Push";
const BROWSER_LOGIN: &str =
    "a browser signs itself in for a cookie; `roast` sends its token with every call";
const TEST: &str = "a test hook";
const VIRTUAL: &str = "the simulator, for tests (docs/virtual-roaster.md)";
const ROASTER_PAGE: &str = "a roaster's own settings, set on its page while trying them on a roast";

/// Every operation of the OpenAPI document, by method and path template.
pub const SURFACE: &[(&str, &str, Via)] = &[
    // system
    ("GET", "/healthz", Via::Cmd("health", R)),
    ("GET", "/version", Via::Cmd("health", R)),
    ("GET", "/readyz", Via::Cmd("health --ready", R)),
    ("GET", "/metrics", Via::None("Prometheus scrapes it")),
    ("GET", "/api/about", Via::Cmd("status", R)),
    ("GET", "/api/busy", Via::Cmd("busy", R)),
    ("GET", "/api/auth/status", Via::Cmd("status", R)),
    ("POST", "/api/demo/roaster", Via::None("a demo visitor's browser takes its own simulated roaster (docs/DEPLOY.md, \"A simulated roaster per visitor\"); a token or the owner key is no visitor")),
    ("POST", "/api/admin/mqtt/reset", Via::Api("maintenance after a broker problem, rare (Settings → About offers it)")),
    ("GET", "/api/audit", Via::Cmd("audit", A)),
    ("GET", "/api/telemetry/rings", Via::Api("the server's memory per roaster, for debugging")),
    ("POST", "/api/test/emit-status/{device_id}", Via::None(TEST)),
    ("POST", "/api/test/emit-telemetry/{device_id}", Via::None(TEST)),
    // approvals and tokens
    ("GET", "/api/approvals", Via::None(OWNER)),
    ("GET", "/api/approvals/{id}", Via::Cmd("approval", R)),
    ("POST", "/api/approvals/{id}/approve", Via::None("the owner key's alone: an agent never approves its own request")),
    ("POST", "/api/approvals/{id}/deny", Via::None(OWNER)),
    ("GET", "/api/tokens", Via::None(OWNER)),
    ("POST", "/api/tokens", Via::None(OWNER)),
    ("PUT", "/api/tokens/{id}", Via::None(OWNER)),
    ("DELETE", "/api/tokens/{id}", Via::None(OWNER)),
    ("POST", "/api/tokens/{id}/revoke", Via::None(OWNER)),
    // signed-in browsers (logins.rs)
    ("POST", "/api/auth/login", Via::None(BROWSER_LOGIN)),
    ("POST", "/api/auth/logout", Via::None(BROWSER_LOGIN)),
    ("GET", "/api/auth/logins", Via::None(OWNER)),
    ("DELETE", "/api/auth/logins", Via::None(OWNER)),
    ("DELETE", "/api/auth/logins/{id}", Via::None(OWNER)),
    // backups
    ("GET", "/api/backups", Via::Cmd("backups", R)),
    ("POST", "/api/backups", Via::Cmd("backups --now", A)),
    ("POST", "/api/backups/{name}/check", Via::Cmd("backups --check", A)),
    ("POST", "/api/backups/{name}/restore", Via::None("the owner key's alone (a copy brings back its day's tokens); `roast restore` on the host")),
    ("DELETE", "/api/backups/restore", Via::None(OWNER)),
    // green coffee
    ("GET", "/api/beans", Via::Cmd("beans", R)),
    ("POST", "/api/beans", Via::Cmd("bean add", I)),
    ("POST", "/api/beans/import", Via::Cmd("bean import", I)),
    ("GET", "/api/beans/suggestions", Via::None("the bean form's suggestions; `beans` and `bean show` have the values")),
    ("GET", "/api/beans/{id}", Via::Cmd("bean show", R)),
    ("GET", "/api/beans/{id}/guide", Via::Cmd("bean guide", R)),
    ("PUT", "/api/beans/{id}", Via::Cmd("bean edit", I)),
    ("DELETE", "/api/beans/{id}", Via::Cmd("bean rm", I)),
    ("POST", "/api/beans/{id}/stock", Via::Cmd("bean stock add", I)),
    ("PUT", "/api/beans/{id}/moves/{move_id}", Via::Cmd("bean stock edit", I)),
    ("DELETE", "/api/beans/{id}/moves/{move_id}", Via::Cmd("bean stock rm", I)),
    ("POST", "/api/purchases", Via::Cmd("order", I)),
    // Orders from receipts (docs/green-coffee.md, "Orders")
    ("GET", "/api/orders", Via::Cmd("orders", R)),
    ("POST", "/api/orders", Via::Cmd("order import", I)),
    ("GET", "/api/orders/{id}", Via::Cmd("order show", R)),
    ("DELETE", "/api/orders/{id}", Via::Cmd("order rm", I)),
    ("POST", "/api/orders/{id}/shipped", Via::Cmd("order shipped", I)),
    ("POST", "/api/orders/{id}/received", Via::Cmd("order received", I)),
    ("POST", "/api/orders/{id}/cancel", Via::Cmd("order cancel", I)),
    ("POST", "/api/orders/{id}/receipts", Via::Cmd("order receipt --add", I)),
    ("GET", "/api/orders/{id}/receipts/{receipt_id}", Via::Cmd("order receipt", I)),
    ("DELETE", "/api/orders/{id}/receipts/{receipt_id}", Via::Cmd("order receipt --rm", I)),
    ("GET", "/api/costs", Via::Cmd("costs", R)),
    ("GET", "/api/wanted", Via::Cmd("wanted", R)),
    ("POST", "/api/wanted", Via::Cmd("bean import --want", I)),
    ("PUT", "/api/wanted/{id}", Via::Cmd("wanted qty", I)),
    ("POST", "/api/wanted/cart", Via::Cmd("wanted cart", I)),
    ("DELETE", "/api/wanted/{id}", Via::Cmd("wanted rm", I)),
    ("POST", "/api/wanted/{id}/bought", Via::Cmd("wanted bought", I)),
    // roasted coffee (docs/roasted.md)
    ("GET", "/api/roasted", Via::Cmd("roasted", R)),
    ("GET", "/api/roasted/{id}", Via::Cmd("roasted show", R)),
    ("POST", "/api/roasted/{id}/moves", Via::Cmd("roasted use", I)),
    ("DELETE", "/api/roasted/{id}/moves/{move_id}", Via::Cmd("roasted undo", I)),
    ("POST", "/api/roasted/combined", Via::Cmd("roasted combine", I)),
    ("GET", "/api/roasted/combined/{id}", Via::Cmd("roasted combine", R)),
    ("DELETE", "/api/roasted/combined/{id}", Via::Cmd("roasted uncombine", I)),
    // traceability (docs/reports.md, "Traceability")
    ("GET", "/api/trace/roast/{id}", Via::Cmd("trace", R)),
    ("GET", "/api/trace/bean/{id}", Via::Cmd("trace", R)),
    // the plan
    ("GET", "/api/plan", Via::Cmd("plan", R)),
    ("POST", "/api/plan", Via::Cmd("plan add", I)),
    ("GET", "/api/plan/{id}", Via::Cmd("plan", R)),
    ("PUT", "/api/plan/{id}", Via::Cmd("plan edit", I)),
    ("DELETE", "/api/plan/{id}", Via::Cmd("plan rm", I)),
    ("POST", "/api/plan/reorder", Via::Cmd("plan move", I)),
    // profiles
    ("GET", "/api/profiles", Via::Cmd("profiles", R)),
    ("POST", "/api/profiles", Via::Api("a profile's curve is drawn in the profile editor; `profile save` makes one from a roast, `profile import` from a file")),
    ("POST", "/api/profiles/import/artisan", Via::Cmd("profile import", A)),
    ("GET", "/api/profiles/{id}", Via::Cmd("profile show", R)),
    ("PUT", "/api/profiles/{id}", Via::Cmd("profile edit", A)),
    ("DELETE", "/api/profiles/{id}", Via::Cmd("profile rm", A)),
    ("GET", "/api/profiles/{id}/export", Via::Cmd("profile export", R)),
    ("GET", "/api/profiles/{id}/export/artisan", Via::Cmd("profile export --artisan", R)),
    ("GET", "/api/profiles/{id}/history", Via::Cmd("profile history", R)),
    ("POST", "/api/sessions/{id}/profile", Via::Cmd("profile save", A)),
    ("GET", "/api/profiles/{id}/share", Via::Cmd("profile shares --profile", R)),
    ("POST", "/api/profiles/{id}/share", Via::Cmd("profile share", A)),
    ("GET", "/api/profile-shares", Via::Cmd("profile shares", R)),
    ("DELETE", "/api/profile-shares/{code}", Via::Cmd("profile unshare", A)),
    ("POST", "/api/profiles/import/link", Via::Cmd("profile import-link", A)),
    // roasts
    ("GET", "/api/sessions", Via::Cmd("roasts", R)),
    ("POST", "/api/sessions", Via::Api("a roast is made when it starts at the roaster (Roast page); `plan add` plans one")),
    ("POST", "/api/sessions/import/artisan", Via::Cmd("import", A)),
    ("GET", "/api/sessions/{id}", Via::Cmd("show", R)),
    ("PUT", "/api/sessions/{id}", Via::Cmd("edit", ROAST)),
    ("DELETE", "/api/sessions/{id}", Via::Cmd("rm", A)),
    ("GET", "/api/sessions/{id}/telemetry", Via::Cmd("show --telemetry", R)),
    ("POST", "/api/sessions/{id}/telemetry", Via::None("a sample by hand, a test hook (the roaster records its own)")),
    ("GET", "/api/sessions/{id}/cost", Via::Cmd("show", R)),
    ("GET", "/api/sessions/{id}/analysis", Via::Cmd("review", R)),
    ("GET", "/api/sessions/{id}/prediction", Via::Cmd("review", R)),
    ("GET", "/api/analysis/compare", Via::Cmd("compare", R)),
    ("GET", "/api/sessions/{id}/export/artisan", Via::Cmd("export", R)),
    ("GET", "/api/sessions/{id}/export/csv", Via::Cmd("export --csv", R)),
    ("GET", "/api/sessions/{id}/labels.pdf", Via::Cmd("labels", R)),
    ("GET", "/api/sessions/{id}/labels.svg", Via::Cmd("labels --svg", R)),
    ("GET", "/api/labels", Via::Cmd("labels --list", R)),
    ("GET", "/api/labels/calibration.pdf", Via::Cmd("labels --calibration", R)),
    ("POST", "/api/labels/check", Via::Cmd("labels design check", R)),
    ("POST", "/api/labels/render", Via::Cmd("labels --source", R)),
    ("GET", "/api/labels/printer", Via::Cmd("labels printer", R)),
    ("POST", "/api/sessions/{id}/labels/print", Via::Cmd("labels --print", I)),
    ("GET", "/api/label-designs", Via::Cmd("labels design list", R)),
    ("GET", "/api/label-designs/{id}", Via::Cmd("labels design show", R)),
    ("POST", "/api/label-designs", Via::Cmd("labels design add", I)),
    ("PUT", "/api/label-designs/{id}", Via::Cmd("labels design edit", I)),
    ("DELETE", "/api/label-designs/{id}", Via::Cmd("labels design rm", I)),
    ("POST", "/api/label-designs/{id}/rollback", Via::Cmd("labels design rollback", I)),
    ("PUT", "/api/beans/{id}/label", Via::Cmd("labels design use", I)),
    ("GET", "/api/label-assets", Via::Cmd("labels asset list", R)),
    ("GET", "/api/label-assets/{name}", Via::Cmd("labels asset get", R)),
    ("PUT", "/api/label-assets/{name}", Via::Cmd("labels asset add", I)),
    ("DELETE", "/api/label-assets/{name}", Via::Cmd("labels asset rm", I)),
    ("GET", "/api/label-brands", Via::Cmd("labels brand list", R)),
    ("GET", "/api/label-brands/{id}", Via::Cmd("labels brand show", R)),
    ("PUT", "/api/label-brands/{id}", Via::Cmd("labels brand set", I)),
    ("DELETE", "/api/label-brands/{id}", Via::Cmd("labels brand rm", I)),
    // print runs (docs/labels.md, "Print runs")
    ("POST", "/api/label-runs", Via::Cmd("labels run", I)),
    ("POST", "/api/label-runs/preview", Via::Api("the dashboard's picture of a run before it is made; an agent checks each design with `labels design check` and reads the run it made")),
    ("GET", "/api/label-runs", Via::Cmd("labels runs", R)),
    ("GET", "/api/label-runs/{id}", Via::Cmd("labels runs show", R)),
    ("GET", "/api/label-runs/{id}/pdf", Via::Cmd("labels runs pdf", R)),
    ("POST", "/api/label-runs/{id}/print", Via::Cmd("labels runs print", I)),
    ("POST", "/api/label-runs/{id}/done", Via::Cmd("labels runs done", I)),
    ("POST", "/api/label-runs/{id}/reprint", Via::Cmd("labels reprint", I)),
    ("GET", "/api/label-sheets", Via::Cmd("labels sheets", R)),
    ("GET", "/api/share", Via::Cmd("share list", R)),
    // reports (docs/reports.md)
    ("GET", "/api/reports", Via::Cmd("report list", R)),
    ("GET", "/api/reports/roast/{id}", Via::Cmd("report roast", R)),
    ("GET", "/api/reports/compare", Via::Cmd("report compare", R)),
    ("GET", "/api/reports/cupping", Via::Cmd("report cupping", R)),
    ("GET", "/api/reports/inventory", Via::Cmd("report inventory", R)),
    ("GET", "/api/reports/shopping", Via::Cmd("report shopping", R)),
    ("GET", "/api/reports/coffee/{id}", Via::Cmd("report coffee", R)),
    ("GET", "/api/reports/profile/{id}", Via::Cmd("report profile", R)),
    ("GET", "/api/reports/period", Via::Cmd("report period", R)),
    ("GET", "/api/reports/purchases", Via::Cmd("report purchases", R)),
    ("GET", "/api/reports/plan", Via::Cmd("report plan", R)),
    ("GET", "/api/reports/trace", Via::Cmd("report trace", R)),
    ("GET", "/api/sessions/{id}/share", Via::Cmd("share list --roast", R)),
    ("POST", "/api/sessions/{id}/share", Via::Cmd("share", I)),
    ("DELETE", "/api/share/{code}", Via::Cmd("share revoke", I)),
    ("PUT", "/api/label-sheets/{id}", Via::Cmd("labels sheets --set", I)),
    // label printers (docs/labels.md, "Printers"): configuration, admin
    ("GET", "/api/label-printers", Via::Cmd("labels printer list", R)),
    ("POST", "/api/label-printers", Via::Cmd("labels printer add", A)),
    ("GET", "/api/label-printers/find", Via::Cmd("labels printer find", R)),
    ("POST", "/api/label-printers/certificate", Via::Cmd("labels printer certificate", A)),
    ("GET", "/api/label-printers/{id}", Via::Cmd("labels printer", R)),
    ("PUT", "/api/label-printers/{id}", Via::Cmd("labels printer edit", A)),
    ("DELETE", "/api/label-printers/{id}", Via::Cmd("labels printer rm", A)),
    ("POST", "/api/label-printers/{id}/default", Via::Cmd("labels printer default", A)),
    ("PUT", "/api/label-printers/{id}/calibration/{sheet}", Via::Cmd("labels printer calibrate", A)),
    ("DELETE", "/api/label-printers/{id}/calibration/{sheet}", Via::Cmd("labels printer calibrate --clear", A)),
    ("GET", "/api/sessions/{session_id}/events", Via::Cmd("show", R)),
    ("POST", "/api/sessions/{session_id}/events", Via::Cmd("landmark", ROAST)),
    ("PUT", "/api/sessions/{session_id}/events/{event_id}", Via::Cmd("landmark", ROAST)),
    ("DELETE", "/api/sessions/{session_id}/events/{event_id}", Via::Cmd("landmark --rm", ROAST)),
    ("GET", "/api/sessions/{session_id}/cupping", Via::Cmd("cup", R)),
    ("POST", "/api/sessions/{session_id}/cupping", Via::Cmd("cup", ROAST)),
    ("DELETE", "/api/sessions/{session_id}/cupping", Via::Cmd("cup --rm", ROAST)),
    ("POST", "/api/sessions/{id}/start", Via::Api(LIVE)),
    ("POST", "/api/sessions/{id}/pause", Via::Api(LIVE)),
    ("POST", "/api/sessions/{id}/resume", Via::Api(LIVE)),
    ("POST", "/api/sessions/{id}/complete", Via::Api(LIVE)),
    ("POST", "/api/sessions/{id}/cancel", Via::Api(LIVE)),
    ("POST", "/api/sessions/{id}/drive/resume", Via::Api(LIVE)),
    // Profile drive
    ("GET", "/api/replay", Via::Cmd("live", R)),
    ("GET", "/api/sessions/{id}/replay", Via::Cmd("live", R)),
    ("POST", "/api/sessions/{id}/replay", Via::Api(LIVE)),
    ("PATCH", "/api/sessions/{id}/replay", Via::Api(LIVE)),
    ("DELETE", "/api/sessions/{id}/replay", Via::Api(LIVE)),
    ("POST", "/api/sessions/{id}/replay/pause", Via::Api(LIVE)),
    ("POST", "/api/sessions/{id}/replay/resume", Via::Api(LIVE)),
    ("GET", "/api/sessions/{id}/replay/events", Via::Api("the drive's events, for the Roast page; `live` has the drive's state")),
    ("POST", "/api/sessions/{id}/replay/events/{event_id}", Via::Api(LIVE)),
    ("GET", "/api/sessions/{id}/replay/log", Via::Api("the drive's step log, for debugging a drive")),
    ("GET", "/api/follow", Via::None(LEGACY)),
    ("GET", "/api/sessions/{id}/follow", Via::None(LEGACY)),
    ("POST", "/api/sessions/{id}/follow", Via::None(LEGACY)),
    ("DELETE", "/api/sessions/{id}/follow", Via::None(LEGACY)),
    // roasters
    ("GET", "/api/devices", Via::Cmd("roasters", R)),
    ("GET", "/api/devices/registry", Via::Cmd("roasters", R)),
    ("GET", "/api/devices/discovered", Via::Cmd("roasters", R)),
    ("GET", "/api/devices/mdns", Via::Api(SETUP)),
    // Adding a roaster (docs/add-roaster.md)
    ("POST", "/api/devices", Via::Cmd("roaster add", A)),
    ("POST", "/api/devices/test-connection", Via::Api(SETUP)),
    ("GET", "/api/devices/{id}", Via::Cmd("roaster show", R)),
    ("PUT", "/api/devices/{id}", Via::Cmd("roaster add", A)),
    ("DELETE", "/api/devices/{id}", Via::Api(SETUP)),
    ("POST", "/api/devices/{id}/connections", Via::Cmd("roaster add", A)),
    ("PUT", "/api/devices/{id}/connections/{conn_id}", Via::Cmd("roaster map", A)),
    ("DELETE", "/api/devices/{id}/connections/{conn_id}", Via::Api(SETUP)),
    ("GET", "/api/devices/{id}/register-map", Via::Cmd("roaster map --register", R)),
    // Artisan's machine setups (docs/artisan-machines.md)
    ("GET", "/api/firmware", Via::Cmd("roaster firmware", A)),
    ("POST", "/api/firmware", Via::Cmd("roaster firmware", A)),
    // Roasters' MQTT passwords on the built-in broker (docs/add-roaster.md, "MQTT passwords")
    ("GET", "/api/mqtt/passwords", Via::Cmd("roaster password list", A)),
    ("POST", "/api/mqtt/passwords", Via::Cmd("roaster password add", A)),
    ("PUT", "/api/mqtt/passwords/{roaster_id}", Via::Cmd("roaster password new", A)),
    ("DELETE", "/api/mqtt/passwords/{roaster_id}", Via::Cmd("roaster password remove", A)),
    ("GET", "/api/artisan/machines", Via::Cmd("roaster machines", R)),
    ("POST", "/api/artisan/machines/plan", Via::Cmd("roaster add --artisan", R)),
    // Artisan as a screen (docs/artisan-screen.md)
    ("GET", "/api/artisan/screen.aset", Via::Cmd("artisan-screen --aset", R)),
    ("PUT", "/api/devices/{id}/register-map", Via::Cmd("roaster map --register", A)),
    ("GET", "/api/device-profiles", Via::Api(SETUP)),
    ("POST", "/api/device-profiles", Via::Api(SETUP)),
    ("GET", "/api/device-profiles/{id}", Via::Api(SETUP)),
    ("PUT", "/api/device-profiles/{id}", Via::Api(SETUP)),
    ("DELETE", "/api/device-profiles/{id}", Via::Api(SETUP)),
    ("GET", "/api/roaster/{device_id}/telemetry/latest", Via::Cmd("live", R)),
    ("GET", "/api/roaster/{device_id}/samples", Via::Api("the scope's 2 h of samples; `live` has the latest reading, `show --telemetry` a roast's")),
    ("GET", "/api/roaster/{device_id}/transports", Via::Cmd("roaster show", R)),
    ("GET", "/api/roaster/{device_id}/sequence", Via::Cmd("roaster show", R)),
    ("PUT", "/api/roaster/{device_id}/sequence", Via::Api(ROASTER_PAGE)),
    ("GET", "/api/roaster/{device_id}/fan_assist", Via::Cmd("roaster show", R)),
    ("PUT", "/api/roaster/{device_id}/fan_assist", Via::Api(ROASTER_PAGE)),
    ("GET", "/api/roaster/{device_id}/token-access", Via::Cmd("roaster show", R)),
    ("PUT", "/api/roaster/{device_id}/token-access", Via::None(OWNER)),
    ("GET", "/api/roaster/{device_id}/setup", Via::Cmd("roaster show", R)),
    ("PUT", "/api/roaster/{device_id}/setup", Via::Cmd("roaster control", A)),
    ("GET", "/api/roaster/{device_id}/gain_schedule", Via::Cmd("roaster tuning", R)),
    ("GET", "/api/roaster/{device_id}/contract", Via::Cmd("roaster contract", R)),
    ("GET", "/api/roaster/{device_id}/raw", Via::Cmd("roaster map --try", R)),
    ("POST", "/api/roaster/{device_id}/ingest/preview", Via::Cmd("roaster map --try", R)),
    ("GET", "/api/roaster/{device_id}/check", Via::Cmd("roaster check", R)),
    ("POST", "/api/roaster/{device_id}/check/write", Via::None("the owner key's alone: it sends heater 10 % to the roaster (`roast roaster check --write` with the owner key, on a cold roaster)")),
    ("GET", "/api/roaster/{device_id}/model", Via::Cmd("roaster model", R)),
    ("POST", "/api/roaster/{device_id}/model/fit", Via::Cmd("roaster model --fit", ROAST)),
    ("GET", "/api/roaster/{device_id}/prediction", Via::Cmd("live", R)),
    ("PUT", "/api/roaster/{device_id}/gain_schedule", Via::Api(TUNE)),
    ("GET", "/api/roaster/{device_id}/control/pid/history", Via::Cmd("roaster tuning", R)),
    ("POST", "/api/roaster/{device_id}/control/pid/undo", Via::Api(TUNE)),
    ("POST", "/api/roaster/{device_id}/control/pid", Via::Api(TUNE)),
    ("POST", "/api/roaster/{device_id}/control/ror_limit", Via::Api(TUNE)),
    ("POST", "/api/roaster/{device_id}/control/setpoint", Via::Api(LIVE)),
    ("POST", "/api/roaster/{device_id}/control/fan", Via::Api(LIVE)),
    ("POST", "/api/roaster/{device_id}/control/heater", Via::Api(LIVE)),
    ("POST", "/api/roaster/{device_id}/control/drum", Via::Api(LIVE)),
    ("POST", "/api/roaster/{device_id}/control/damper", Via::Api(LIVE)),
    ("POST", "/api/roaster/{device_id}/control/heater_enable", Via::Api(LIVE)),
    ("POST", "/api/roaster/{device_id}/control/mode", Via::Api(LIVE)),
    ("POST", "/api/roaster/{device_id}/control/emergency_stop", Via::Api(LIVE)),
    ("GET", "/api/roaster/{device_id}/autotune/runs", Via::Cmd("roaster tuning", R)),
    ("GET", "/api/roaster/{device_id}/autotune/runs/{run_id}", Via::Cmd("roaster tuning --run", R)),
    ("POST", "/api/roaster/{device_id}/autotune/runs/{run_id}/apply", Via::Api(TUNE)),
    ("GET", "/api/roaster/{device_id}/autotune/runs/{run_id}/samples", Via::Cmd("roaster tuning --run --samples", R)),
    ("GET", "/api/roaster/{device_id}/autotune/runs/{run_id}/design", Via::Cmd("roaster tuning --run --tau-c", R)),
    ("POST", "/api/roaster/{device_id}/autotune/runs/{run_id}/analyse", Via::Api(TUNE)),
    ("GET", "/api/roaster/{device_id}/tune", Via::Cmd("roaster tuning", R)),
    ("POST", "/api/roaster/{device_id}/tune/start", Via::Api(TUNE)),
    ("POST", "/api/roaster/{device_id}/tune/stop", Via::Api(TUNE)),
    ("GET", "/api/virtual-roaster", Via::Api(VIRTUAL)),
    ("PUT", "/api/virtual-roaster", Via::Api(VIRTUAL)),
    ("POST", "/api/virtual-roaster/charge", Via::Api(VIRTUAL)),
    ("POST", "/api/virtual-roaster/dump", Via::Api(VIRTUAL)),
    // settings and notifications
    ("GET", "/api/settings", Via::Cmd("settings", R)),
    ("PUT", "/api/settings", Via::Cmd("settings set", A)),
    ("GET", "/api/settings/{key}", Via::Cmd("settings", R)),
    ("PUT", "/api/settings/{key}", Via::Cmd("settings set", A)),
    ("GET", "/api/notifications", Via::Cmd("notifications", R)),
    ("GET", "/api/home-assistant", Via::Cmd("home-assistant", R)),
    ("GET", "/api/support-bundle", Via::Cmd("support-bundle", A)),
    ("POST", "/api/notifications/test", Via::Cmd("notifications --test", A)),
    ("GET", "/api/push/subscriptions", Via::Cmd("notifications", R)),
    ("GET", "/api/push/key", Via::None(BROWSER)),
    ("POST", "/api/push/subscriptions", Via::None(BROWSER)),
    ("POST", "/api/push/unsubscribe", Via::None(BROWSER)),
    ("DELETE", "/api/push/subscriptions/{id}", Via::Api("taking a phone or browser off notifications, rare (Settings → Notifications)")),
    ("POST", "/api/push/test", Via::Api("one device's test; `notifications --test push` tests the channel")),
];

/// The command an agent uses for an operation, or what to know instead, for `openapi --find`.
pub fn how(method: &str, path: &str) -> Option<String> {
    SURFACE
        .iter()
        .find(|(m, p, _)| m.eq_ignore_ascii_case(method) && *p == path)
        .map(|(_, _, via)| match via {
            Via::Cmd(cmd, Preset::ReadOnly) => format!("roast {cmd}"),
            Via::Cmd(cmd, preset) => format!("roast {cmd} ({} token)", preset.as_str()),
            Via::Api(why) => format!("roast api: {why}"),
            Via::None(why) => format!("not for agents: {why}"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use roastwarden_server::presets::is_owner_only;
    use std::collections::BTreeSet;

    const CLI_MD: &str = include_str!("../../../../../docs/cli.md");
    const SKILL: &str = include_str!("../../../../../skills/roastwarden/SKILL.md");

    /// Commands that run next to the database, on the host, which an agent with a token never
    /// does (the skill says not to run `roast` inside the container)
    const HOST_ONLY: &[&str] = &["backup", "restore"];

    fn operations() -> BTreeSet<(String, String)> {
        let doc = serde_json::to_value(roastwarden_server::openapi::document()).unwrap();
        let mut ops = BTreeSet::new();
        for (path, item) in doc["paths"].as_object().unwrap() {
            for m in ["get", "post", "put", "patch", "delete"] {
                if item.get(m).is_some() {
                    ops.insert((m.to_uppercase(), path.clone()));
                }
            }
        }
        ops
    }

    #[test]
    fn every_operation_has_one_entry_and_every_entry_an_operation() {
        let ops = operations();
        assert!(ops.len() > 150, "only {} operations", ops.len());
        let mut seen = BTreeSet::new();
        let mut doubled = Vec::new();
        for (m, p, _) in SURFACE {
            if !seen.insert((m.to_string(), p.to_string())) {
                doubled.push(format!("{m} {p}"));
            }
        }
        assert!(doubled.is_empty(), "entries listed twice: {doubled:?}");
        let missing: Vec<_> = ops.difference(&seen).collect();
        assert!(
            missing.is_empty(),
            "operations with no entry in bin/roast/surface.rs (a command, `api` or `none`, with a reason): {missing:?}"
        );
        let stale: Vec<_> = seen.difference(&ops).collect();
        assert!(
            stale.is_empty(),
            "entries for operations that are gone: {stale:?}"
        );
        for (m, p, via) in SURFACE {
            if let Via::Api(why) | Via::None(why) = via {
                assert!(why.len() > 10, "{m} {p}: say why");
            }
        }
    }

    /// `bean stock add` → the clap command, or None.
    fn find<'a>(root: &'a clap::Command, words: &[&str]) -> Option<&'a clap::Command> {
        words.iter().try_fold(root, |cmd, w| cmd.find_subcommand(w))
    }

    fn split(cmd: &str) -> (Vec<&str>, Vec<&str>) {
        cmd.split_whitespace().partition(|w| !w.starts_with("--"))
    }

    #[test]
    fn every_command_named_exists_with_its_flags() {
        let root = crate::cli::Cli::command();
        let mut bad = Vec::new();
        for (m, p, via) in SURFACE {
            let Via::Cmd(cmd, _) = via else { continue };
            let (words, flags) = split(cmd);
            let Some(found) = find(&root, &words) else {
                bad.push(format!("{m} {p}: no command `roast {cmd}`"));
                continue;
            };
            for flag in flags {
                let long = flag.trim_start_matches("--");
                if !found.get_arguments().any(|a| a.get_long() == Some(long)) {
                    bad.push(format!(
                        "{m} {p}: `roast {}` has no {flag}",
                        words.join(" ")
                    ));
                }
            }
        }
        assert!(bad.is_empty(), "{bad:#?}");
    }

    /// Every command a person or agent can run: `bean stock add`, `plan` (which also lists),
    /// but not `bean` alone, which only groups.
    fn runnable(cmd: &clap::Command, path: &str, out: &mut Vec<String>) {
        for sub in cmd.get_subcommands() {
            if sub.get_name() == "help" {
                continue;
            }
            let here = if path.is_empty() {
                sub.get_name().to_string()
            } else {
                format!("{path} {}", sub.get_name())
            };
            if !sub.is_subcommand_required_set() {
                out.push(here.clone());
            }
            runnable(sub, &here, out);
        }
    }

    /// The row of docs/cli.md's command table that documents `cmd`.
    fn cli_md_row(cmd: &str) -> Option<&'static str> {
        CLI_MD.lines().find(|l| {
            l.strip_prefix("| `")
                .and_then(|rest| rest.strip_prefix(cmd))
                .is_some_and(|after| after.starts_with(' ') || after.starts_with('`'))
        })
    }

    fn in_skill(cmd: &str) -> bool {
        let needle = format!("roast {cmd}");
        SKILL.match_indices(&needle).any(|(i, _)| {
            SKILL[i + needle.len()..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric() && c != '-' && c != '_')
        })
    }

    #[test]
    fn every_command_is_in_the_docs_and_the_skill() {
        let mut all = Vec::new();
        runnable(&crate::cli::Cli::command(), "", &mut all);
        assert!(all.len() > 30, "only {} commands", all.len());
        let mut bad = Vec::new();
        for cmd in &all {
            if cli_md_row(cmd).is_none() {
                bad.push(format!("`{cmd}` has no row in docs/cli.md's command table"));
            }
            if !HOST_ONLY.contains(&cmd.as_str()) && !in_skill(cmd) {
                bad.push(format!(
                    "`roast {cmd}` is not in skills/roastwarden/SKILL.md"
                ));
            }
        }
        // and the flags the table names are in the command's row
        for (_, _, via) in SURFACE {
            let Via::Cmd(cmd, _) = via else { continue };
            let (words, flags) = split(cmd);
            let words = words.join(" ");
            if let Some(row) = cli_md_row(&words) {
                for f in flags {
                    if !row.contains(f) {
                        bad.push(format!("docs/cli.md's row for `{words}` does not show {f}"));
                    }
                }
            }
        }
        assert!(bad.is_empty(), "{bad:#?}");
    }

    fn concrete(template: &str) -> String {
        template
            .split('/')
            .map(|s| if s.starts_with('{') { "x1" } else { s })
            .collect::<Vec<_>>()
            .join("/")
    }

    /// The preset named reaches the call, nothing less would, and docs/cli.md says it.
    #[test]
    fn every_preset_named_can_reach_its_call_and_is_the_least() {
        let mut bad = Vec::new();
        for (m, p, via) in SURFACE {
            let path = concrete(p);
            let owner = is_owner_only(m, &path);
            match via {
                Via::Cmd(cmd, preset) => {
                    if owner {
                        bad.push(format!(
                            "{m} {p} is the owner's, yet `roast {cmd}` is named"
                        ));
                    }
                    if !preset.allows(m, &path) {
                        bad.push(format!(
                            "{m} {p}: a {} token cannot make this call",
                            preset.as_str()
                        ));
                    }
                    let less: &[Preset] = match preset {
                        Preset::ReadOnly => &[],
                        Preset::Roast | Preset::Inventory => &[Preset::ReadOnly],
                        Preset::Admin => &[Preset::ReadOnly, Preset::Roast, Preset::Inventory],
                    };
                    if let Some(l) = less.iter().find(|l| l.allows(m, &path)) {
                        bad.push(format!(
                            "{m} {p}: {} is enough, not {}",
                            l.as_str(),
                            preset.as_str()
                        ));
                    }
                    let (words, _) = split(cmd);
                    let words = words.join(" ");
                    if *preset != Preset::ReadOnly {
                        let named = cli_md_row(&words).is_some_and(|row| {
                            row.rsplit('|')
                                .nth(1)
                                .is_some_and(|need| need.contains(preset.as_str()))
                        });
                        if !named {
                            bad.push(format!(
                                "docs/cli.md's row for `{words}` does not name the {} preset",
                                preset.as_str()
                            ));
                        }
                    }
                }
                Via::Api(_) => {
                    if owner {
                        bad.push(format!("{m} {p} is the owner's: `none`, not `api`"));
                    }
                }
                Via::None(_) => {}
            }
        }
        assert!(bad.is_empty(), "{bad:#?}");
    }

    #[test]
    fn find_shows_the_command() {
        assert_eq!(
            how("post", "/api/beans").unwrap(),
            "roast bean add (inventory token)"
        );
        assert_eq!(how("GET", "/api/beans").unwrap(), "roast beans");
        assert!(how("GET", "/api/tokens")
            .unwrap()
            .starts_with("not for agents"));
        assert!(how("GET", "/nothing").is_none());
    }
}
