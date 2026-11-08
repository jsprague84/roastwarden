//! The JSON shape of every API response the dashboard, the CLI and agents read, kept in
//! `crates/server/testdata/api-shapes.json`: each response's status with its values replaced
//! by their JSON types (`string`, `integer`, `number`, `boolean`, `null`; arrays by the union
//! of their elements, a key missing from some elements as `absent`; keys sorted). A body that
//! is not JSON is recorded by its content type. Moving handlers to typed responses (utoipa)
//! must not change a key or a type, and this test is how that is checked. After a deliberate
//! change, regenerate with
//! `UPDATE_API_SHAPES=1 cargo test -p roastwarden-server api_shapes`
//! and read the diff.

use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

use crate::test_support::{eventually, Harness, Reply, ReplyBody};

const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/api-shapes.json");
const VR: &str = crate::sim::virtual_roaster::DEVICE_ID;

/// A value's shape: its type, recursively.
fn shape(v: &Value) -> Value {
    match v {
        Value::Null => json!("null"),
        Value::Bool(_) => json!("boolean"),
        Value::Number(n) if n.is_i64() || n.is_u64() => json!("integer"),
        Value::Number(_) => json!("number"),
        Value::String(_) => json!("string"),
        Value::Array(items) => {
            let merged = items.iter().map(shape).reduce(union);
            json!([merged.unwrap_or(json!("empty"))])
        }
        Value::Object(map) => {
            let sorted: BTreeMap<&String, Value> = map.iter().map(|(k, v)| (k, shape(v))).collect();
            Value::Object(sorted.into_iter().map(|(k, v)| (k.clone(), v)).collect())
        }
    }
}

/// Two shapes as one: objects key by key (a key missing on one side is `absent`), scalars as
/// `a|b`.
fn union(a: Value, b: Value) -> Value {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let keys: BTreeSet<String> = x.keys().chain(y.keys()).cloned().collect();
            let mut out = Map::new();
            for k in keys {
                let l = x.get(&k).cloned().unwrap_or(json!("absent"));
                let r = y.get(&k).cloned().unwrap_or(json!("absent"));
                out.insert(k, union(l, r));
            }
            Value::Object(out)
        }
        (Value::Array(x), Value::Array(y)) => {
            let l = x.into_iter().next().unwrap_or(json!("empty"));
            let r = y.into_iter().next().unwrap_or(json!("empty"));
            if l == json!("empty") {
                json!([r])
            } else if r == json!("empty") {
                json!([l])
            } else {
                json!([union(l, r)])
            }
        }
        (x, y) if x == y => x,
        (x, y) => {
            let mut parts: BTreeSet<String> = BTreeSet::new();
            for v in [x, y] {
                match v {
                    Value::String(s) => parts.extend(s.split('|').map(String::from)),
                    other => {
                        parts.insert(other.to_string());
                    }
                }
            }
            json!(parts.into_iter().collect::<Vec<_>>().join("|"))
        }
    }
}

/// A replay state is a snapshot of a running drive: what it has sent so far and its log of
/// recent steps depend on how many ticks ran before the call, which a busy CI runner makes
/// fewer. Those fields are kept by their declared type, not by what this run saw.
fn loosen(mut shape: Value) -> Value {
    // A label check's fields printed and not printed depend on the day's data
    if let Some(fields) = shape.pointer_mut("/report/fields") {
        *fields =
            json!({ "missing": "varies: the label's data", "shown": "varies: the label's data" });
    }
    let Some(state) = shape.as_object_mut() else {
        return shape;
    };
    if !(state.contains_key("run_id") && state.contains_key("recent")) {
        return shape;
    }
    state.insert("recent".into(), json!("varies: the drive's log"));
    state.insert("last_sent_at".into(), json!("null|string"));
    if let Some(Value::Object(channel)) = state.get_mut("channels").and_then(|c| c.get_mut(0)) {
        channel.insert("last_sent".into(), json!("null|number"));
        channel.insert("target".into(), json!("null|number"));
    }
    shape
}

struct Recorder<'a> {
    h: &'a Harness,
    shapes: BTreeMap<String, Value>,
}

impl Recorder<'_> {
    fn keep(&mut self, name: &str, reply: &Reply) {
        let body = match &reply.body {
            ReplyBody::Empty => json!("no body"),
            ReplyBody::Json(v) => loosen(shape(v)),
            ReplyBody::Text => json!(format!(
                "text: {}",
                reply.content_type.as_deref().unwrap_or("unknown")
            )),
        };
        if !reply.status.is_success() {
            // Shown with --nocapture: why a call was refused
            eprintln!("{name}: {} {}", reply.status, reply.json());
        }
        self.shapes.insert(
            name.to_string(),
            json!({ "status": reply.status.as_u16(), "body": body }),
        );
    }

    /// A call with the owner key, kept under `name`; its JSON body (Null when there is none)
    async fn owner(&mut self, name: &str, method: &str, uri: &str, body: Option<Value>) -> Value {
        let reply = self.h.owner(method, uri, body).await;
        self.keep(name, &reply);
        reply.json()
    }

    /// A call with an API token
    async fn token(
        &mut self,
        name: &str,
        token: &str,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> Value {
        let reply = crate::test_support::call(&self.h.app, method, uri, Some(token), body).await;
        self.keep(name, &reply);
        reply.json()
    }

    async fn anon(&mut self, name: &str, method: &str, uri: &str) -> Value {
        let reply = self.h.anon(method, uri, None).await;
        self.keep(name, &reply);
        reply.json()
    }
}

fn id(v: &Value) -> String {
    match &v["id"] {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        other => panic!("no id in {v} ({other})"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn api_shapes_are_unchanged() {
    let h = Harness::new().await;
    let mut r = Recorder {
        h: &h,
        shapes: BTreeMap::new(),
    };

    // ── Health, version, auth, errors
    r.anon("GET /healthz", "GET", "/healthz").await;
    r.anon("GET /readyz", "GET", "/readyz").await;
    r.anon("GET /version", "GET", "/version").await;
    r.owner("GET /metrics", "GET", "/metrics", None).await;
    r.anon("GET /api/sessions (anonymous)", "GET", "/api/sessions")
        .await;
    // Its content has a test of its own (openapi.rs): only that it is served
    let doc = h.anon("GET", "/api-docs/openapi.json", None).await;
    assert_eq!(doc.json()["openapi"], "3.1.0");
    r.shapes.insert(
        "GET /api-docs/openapi.json".into(),
        json!({ "status": doc.status.as_u16(), "body": "the OpenAPI document" }),
    );
    r.anon(
        "GET /api/auth/status (anonymous)",
        "GET",
        "/api/auth/status",
    )
    .await;
    r.owner("GET /api/auth/status", "GET", "/api/auth/status", None)
        .await;
    // Signing a browser in (logins.rs): the cookie is in a header, the body the status
    let login = h
        .anon(
            "POST",
            "/api/auth/login",
            Some(json!({ "credential": crate::test_support::OWNER_KEY })),
        )
        .await;
    r.keep("POST /api/auth/login", &login);
    let wrong = h
        .anon(
            "POST",
            "/api/auth/login",
            Some(json!({ "credential": "wrong" })),
        )
        .await;
    r.keep("POST /api/auth/login (wrong key)", &wrong);
    r.owner("GET /api/auth/logins", "GET", "/api/auth/logins", None)
        .await;
    let login_id = login.json()["login_id"].as_i64().expect("a login id");
    r.owner(
        "DELETE /api/auth/logins/{id}",
        "DELETE",
        &format!("/api/auth/logins/{login_id}"),
        None,
    )
    .await;
    r.owner(
        "DELETE /api/auth/logins",
        "DELETE",
        "/api/auth/logins",
        None,
    )
    .await;
    r.anon("POST /api/auth/logout", "POST", "/api/auth/logout")
        .await;
    r.anon("POST /api/sessions (anonymous)", "POST", "/api/sessions")
        .await;
    r.owner("GET /api/nope (unknown path)", "GET", "/api/nope", None)
        .await;
    r.owner("GET /api/about", "GET", "/api/about", None).await;
    r.owner("GET /api/busy", "GET", "/api/busy", None).await;

    // ── Settings
    r.owner("GET /api/settings", "GET", "/api/settings", None)
        .await;
    r.owner(
        "PUT /api/settings/{key}",
        "PUT",
        "/api/settings/ror_window_seconds",
        Some(json!({ "value": "30" })),
    )
    .await;
    r.owner(
        "PUT /api/settings/{key} (out of range)",
        "PUT",
        "/api/settings/ror_window_seconds",
        Some(json!({ "value": "9999" })),
    )
    .await;
    r.owner(
        "GET /api/settings/{key}",
        "GET",
        "/api/settings/ror_window_seconds",
        None,
    )
    .await;
    r.owner(
        "PUT /api/settings",
        "PUT",
        "/api/settings",
        Some(json!({ "values": { "auto_dry_temp": "150", "follow_hysteresis_c": "2" } })),
    )
    .await;

    // ── The virtual roaster, its telemetry through the MQTT loopback
    r.owner(
        "PUT /api/virtual-roaster",
        "PUT",
        "/api/virtual-roaster",
        Some(json!({ "enabled": true })),
    )
    .await;
    let latest = format!("/api/roaster/{VR}/telemetry/latest");
    assert!(
        eventually(|| async { h.owner("GET", &latest, None).await.status.is_success() }).await,
        "the virtual roaster sends telemetry"
    );
    r.owner(
        "GET /api/virtual-roaster",
        "GET",
        "/api/virtual-roaster",
        None,
    )
    .await;
    r.owner(
        "GET /api/roaster/{id}/telemetry/latest",
        "GET",
        &latest,
        None,
    )
    .await;
    r.owner(
        "GET /api/roaster/{id}/telemetry/latest (unknown roaster)",
        "GET",
        "/api/roaster/nobody/telemetry/latest",
        None,
    )
    .await;
    r.owner(
        "GET /api/roaster/{id}/transports",
        "GET",
        &format!("/api/roaster/{VR}/transports"),
        None,
    )
    .await;
    // The gain scheduler records the roaster's boot on a sample after the first one lands:
    // wait for it, or the shape depends on which came first (as it did in CI).
    let contract = format!("/api/roaster/{VR}/contract");
    assert!(
        eventually(|| async {
            h.owner("GET", &contract, None).await.json()["gains"]["last_boot"].is_object()
        })
        .await,
        "the scheduler sees the virtual roaster's boot"
    );
    r.owner(
        "GET /api/roaster/{id}/contract",
        "GET",
        &format!("/api/roaster/{VR}/contract"),
        None,
    )
    .await;
    r.owner(
        "GET /api/roaster/{id}/contract (unknown roaster)",
        "GET",
        "/api/roaster/nobody/contract",
        None,
    )
    .await;
    r.owner(
        "GET /api/roaster/{id}/samples",
        "GET",
        &format!("/api/roaster/{VR}/samples"),
        None,
    )
    .await;
    r.owner(
        "GET /api/telemetry/rings",
        "GET",
        "/api/telemetry/rings",
        None,
    )
    .await;
    r.owner(
        "GET /api/devices/registry",
        "GET",
        "/api/devices/registry",
        None,
    )
    .await;
    r.owner("GET /api/devices/mdns", "GET", "/api/devices/mdns", None)
        .await;
    r.owner(
        "POST /api/test/emit-status/{id}",
        "POST",
        "/api/test/emit-status/test_roaster",
        None,
    )
    .await;
    r.owner(
        "POST /api/test/emit-telemetry/{id}",
        "POST",
        "/api/test/emit-telemetry/test_roaster",
        None,
    )
    .await;

    // ── Control, on the virtual roaster
    let control = |c: &str| format!("/api/roaster/{VR}/control/{c}");
    r.owner(
        "POST /api/roaster/{id}/control/setpoint",
        "POST",
        &control("setpoint"),
        Some(json!({ "value": 180.0 })),
    )
    .await;
    r.owner(
        "POST /api/roaster/{id}/control/setpoint (bad body)",
        "POST",
        &control("setpoint"),
        Some(json!({ "value": "hot" })),
    )
    .await;
    r.owner(
        "POST /api/roaster/{id}/control/fan",
        "POST",
        &control("fan"),
        Some(json!({ "percent": 80 })),
    )
    .await;
    r.owner(
        "POST /api/roaster/{id}/control/heater",
        "POST",
        &control("heater"),
        Some(json!({ "percent": 40 })),
    )
    .await;
    r.owner(
        "POST /api/roaster/{id}/control/mode",
        "POST",
        &control("mode"),
        Some(json!({ "mode": "auto" })),
    )
    .await;
    r.owner(
        "POST /api/roaster/{id}/control/heater_enable",
        "POST",
        &control("heater_enable"),
        Some(json!({ "enabled": true })),
    )
    .await;
    r.owner(
        "POST /api/roaster/{id}/control/pid",
        "POST",
        &control("pid"),
        Some(json!({ "kp": 2.0, "ki": 0.1, "kd": 1.0 })),
    )
    .await;
    r.owner(
        "POST /api/roaster/{id}/control/ror_limit",
        "POST",
        &control("ror_limit"),
        Some(json!({ "max": 20.0 })),
    )
    .await;
    r.owner(
        "GET /api/roaster/{id}/control/pid/history",
        "GET",
        &control("pid/history"),
        None,
    )
    .await;
    r.owner(
        "POST /api/roaster/{id}/control/pid/undo",
        "POST",
        &control("pid/undo"),
        None,
    )
    .await;

    // ── Autotune
    let tune = |p: &str| format!("/api/roaster/{VR}/autotune/{p}");
    // A firmware run from before 3.5.0 (docs/autotune-2.md, "Stored gains and undo"): stored
    // runs stay readable
    let old = crate::control::tuning::AutotuneRun::from_results(
        VR,
        &json!({ "state": "complete", "quality": "good", "ku": 20.0, "tu": 60.0, "bias": 61.5,
                "fan": 75.0, "target_temperature": 150.0, "tuning_method": "tyreus_luyben_pi",
                "recommended_kp": 6.25, "recommended_ki": 0.05, "recommended_kd": 10.0,
                "original_kp": 18.79, "original_ki": 0.1, "original_kd": 5.0,
                "warnings": ["asymmetric"] }),
    );
    crate::control::tuning::insert_run(&h.state.db, &old)
        .await
        .unwrap();
    let runs = tune("runs");
    assert!(
        eventually(|| async {
            h.owner("GET", &runs, None).await.json()["runs"]
                .as_array()
                .is_some_and(|r| !r.is_empty())
        })
        .await,
        "the autotune run is stored"
    );
    let run = h.owner("GET", &runs, None).await.json()["runs"][0].clone();
    let run_id = id(&run);
    let rule = run["rules"][0]["rule"].as_str().unwrap().to_string();
    r.owner(
        "GET /api/roaster/{id}/autotune/runs/{run_id}",
        "GET",
        &tune(&format!("runs/{run_id}")),
        None,
    )
    .await;
    r.owner(
        "POST /api/roaster/{id}/autotune/runs/{run_id}/apply (to the schedule)",
        "POST",
        &tune(&format!("runs/{run_id}/apply")),
        Some(json!({ "rule": rule, "to": "schedule" })),
    )
    .await;
    r.owner(
        "POST /api/roaster/{id}/autotune/runs/{run_id}/apply",
        "POST",
        &tune(&format!("runs/{run_id}/apply")),
        Some(json!({ "rule": rule })),
    )
    .await;
    r.owner(
        "GET /api/roaster/{id}/autotune/runs",
        "GET",
        &tune("runs"),
        None,
    )
    .await;

    // ── The server's autotune (docs/autotune-2.md): no test yet, then a finished run made on
    // a simulated roaster, as the test loop stores it
    let srv = |p: &str| format!("/api/roaster/{VR}/tune{p}");
    r.owner(
        "GET /api/roaster/{id}/tune (none yet)",
        "GET",
        &srv(""),
        None,
    )
    .await;
    r.owner(
        "POST /api/roaster/{id}/tune/stop (none running)",
        "POST",
        &srv("/stop"),
        None,
    )
    .await;
    r.owner(
        "POST /api/roaster/{id}/tune/start (bad level)",
        "POST",
        &srv("/start"),
        Some(json!({ "setpoint": 400.0 })),
    )
    .await;
    let stored = crate::control::tune::tests::simulated_run(VR);
    crate::control::tuning::insert_server_run(&h.state.db, &stored)
        .await
        .unwrap();
    let srun = stored.run.id.clone();
    r.owner(
        "GET /api/roaster/{id}/autotune/runs/{run_id} (server run)",
        "GET",
        &tune(&format!("runs/{srun}")),
        None,
    )
    .await;
    r.owner(
        "GET /api/roaster/{id}/autotune/runs/{run_id}/samples",
        "GET",
        &tune(&format!("runs/{srun}/samples")),
        None,
    )
    .await;
    r.owner(
        "GET /api/roaster/{id}/autotune/runs/{run_id}/design",
        "GET",
        &tune(&format!("runs/{srun}/design?rule=simc_pi&tau_c=2")),
        None,
    )
    .await;
    r.owner(
        "POST /api/roaster/{id}/autotune/runs/{run_id}/analyse",
        "POST",
        &tune(&format!("runs/{srun}/analyse")),
        None,
    )
    .await;
    r.owner(
        "POST /api/roaster/{id}/autotune/runs/{run_id}/apply (server run, to the schedule)",
        "POST",
        &tune(&format!("runs/{srun}/apply")),
        Some(json!({ "rule": "simc_pi", "tau_c": 1.5, "to": "schedule" })),
    )
    .await;

    // ── Per-roaster settings, each written back as read
    let dev = |p: &str| format!("/api/roaster/{VR}/{p}");
    for p in [
        "fan_assist",
        "gain_schedule",
        "sequence",
        "token-access",
        "setup",
    ] {
        let got = r
            .owner(
                &format!("GET /api/roaster/{{id}}/{p}"),
                "GET",
                &dev(p),
                None,
            )
            .await;
        let (uri, body) = match p {
            // Tokens may always control the virtual roaster: the switch is for the others
            "token-access" => (
                "/api/roaster/test_roaster/token-access".to_string(),
                json!({ "tokens_may_control": true }),
            ),
            // The virtual roaster's setup is fixed: set another's (docs/roaster-setup.md)
            "setup" => (
                "/api/roaster/test_roaster/setup".to_string(),
                json!({ "kind": "drum_gas", "control": "manual" }),
            ),
            // The GET adds live state next to the config
            "fan_assist" => (dev(p), got.get("config").cloned().unwrap_or(got)),
            _ => (dev(p), got),
        };
        r.owner(
            &format!("PUT /api/roaster/{{id}}/{p}"),
            "PUT",
            &uri,
            Some(body),
        )
        .await;
    }

    // ── Profiles
    r.owner(
        "GET /api/profiles",
        "GET",
        "/api/profiles?include_private=true",
        None,
    )
    .await;
    let seed = "fluidbed-espresso";
    r.owner(
        "GET /api/profiles/{id}",
        "GET",
        &format!("/api/profiles/{seed}"),
        None,
    )
    .await;
    let profile = r
        .owner(
            "POST /api/profiles",
            "POST",
            "/api/profiles",
            Some(json!({
                "name": "Shape test",
                "description": "for the api-shapes golden",
                "target_total_time": 600,
                "target_first_crack": 420,
                "target_end_temp": 210.0,
                "charge_temp": 180.0,
                "roast_level": "light",
                "roast_purpose": "filter",
                "points": [
                    { "time_seconds": 0, "target_temp": 180.0, "fan_speed": 90 },
                    { "time_seconds": 300, "target_temp": 170.0, "fan_speed": 80 },
                    { "time_seconds": 600, "target_temp": 210.0, "fan_speed": 70 }
                ],
                "events": [
                    { "trigger_kind": "time", "at_seconds": 120.0, "action": "fan", "value": 85.0 }
                ]
            })),
        )
        .await;
    let profile_id = id(&profile);
    r.owner(
        "POST /api/profiles (no points)",
        "POST",
        "/api/profiles",
        Some(json!({ "name": "Empty", "points": [] })),
    )
    .await;
    let mut edited = profile.clone();
    edited["description"] = json!("edited");
    r.owner(
        "PUT /api/profiles/{id}",
        "PUT",
        &format!("/api/profiles/{profile_id}"),
        Some(edited),
    )
    .await;
    r.owner(
        "GET /api/profiles/{id}/export",
        "GET",
        &format!("/api/profiles/{profile_id}/export"),
        None,
    )
    .await;
    let alog = h
        .owner(
            "GET",
            &format!("/api/profiles/{profile_id}/export/artisan"),
            None,
        )
        .await;
    r.keep("GET /api/profiles/{id}/export/artisan", &alog);
    // Shared profiles (docs/profiles.md, "Sharing a profile"): the link, then an import
    // from it (this server's own link: read here, no request)
    let plink = r
        .owner(
            "POST /api/profiles/{id}/share",
            "POST",
            &format!("/api/profiles/{profile_id}/share"),
            Some(json!({})),
        )
        .await;
    let pcode = plink["code"].as_str().unwrap_or_default().to_string();
    let purl = plink["url"].as_str().unwrap_or_default().to_string();
    r.owner(
        "GET /api/profiles/{id}/share",
        "GET",
        &format!("/api/profiles/{profile_id}/share"),
        None,
    )
    .await;
    r.owner(
        "GET /api/profile-shares",
        "GET",
        "/api/profile-shares",
        None,
    )
    .await;
    r.owner(
        "POST /api/profiles/import/link (preview)",
        "POST",
        "/api/profiles/import/link",
        Some(json!({ "url": purl, "preview": true })),
    )
    .await;
    r.owner(
        "POST /api/profiles/import/link",
        "POST",
        "/api/profiles/import/link",
        Some(json!({ "url": purl, "name": "Shape import" })),
    )
    .await;
    r.owner(
        "DELETE /api/profile-shares/{code}",
        "DELETE",
        &format!("/api/profile-shares/{pcode}"),
        None,
    )
    .await;

    // ── Beans and the plan
    let bean = r
        .owner(
            "POST /api/beans",
            "POST",
            "/api/beans",
            Some(json!({
                "name": "Ethiopia Guji", "origin": "Ethiopia", "variety": "Heirloom",
                "process": "washed", "supplier": "Test", "moisture_pct": 10.5,
                "density_g_l": 700.0, "low_stock_g": 500.0, "stock_g": 2000.0,
                "region": "Guji", "producer": "Hambela", "altitude": "2000 masl",
                "grade": "Grade 1", "screen_size": "15-17", "certifications": "Organic",
                "drying": "Raised beds", "score": 89.5, "roast_recommendation": "City",
                "cupping_notes": "jasmine", "url": "https://www.sweetmarias.com/products/x",
                "sku": "GCX-1", "details": { "Packaging": "GrainPro" }
            })),
        )
        .await;
    let bean_id = id(&bean);
    r.owner("GET /api/beans", "GET", "/api/beans", None).await;
    // A new bean's guide (docs/new-bean.md) with no plan, so no profile
    r.owner(
        "GET /api/beans/{id}/guide (no profile)",
        "GET",
        &format!("/api/beans/{bean_id}/guide"),
        None,
    )
    .await;
    r.owner(
        "PUT /api/beans/{id}",
        "PUT",
        &format!("/api/beans/{bean_id}"),
        Some(json!({ "notes": "floral" })),
    )
    .await;
    r.owner(
        "POST /api/beans/{id}/stock",
        "POST",
        &format!("/api/beans/{bean_id}/stock"),
        Some(
            json!({ "delta_g": 1000.0, "note": "bought more", "price": 24.5,
                      "shipping": 6.0, "currency": "usd", "supplier": "Test",
                      "crop_year": "2026", "arrival": "2026-07", "at": "2026-10-20" }),
        ),
    )
    .await;
    r.owner(
        "GET /api/beans/suggestions",
        "GET",
        "/api/beans/suggestions",
        None,
    )
    .await;
    r.owner(
        "POST /api/beans/import (unknown host)",
        "POST",
        "/api/beans/import",
        Some(json!({ "url": "https://example.com/products/x" })),
    )
    .await;
    let wanted = r
        .owner(
            "POST /api/wanted",
            "POST",
            "/api/wanted",
            Some(json!({ "size": "5 LB", "notes": "for espresso", "draft": {
                "name": "Kenya Nyeri", "supplier": "Test", "origin": "Kenya",
                "url": "https://www.sweetmarias.com/products/kenya", "details": {},
                "sizes": [{ "label": "5 LB", "weight_g": 2268.0, "price": 52.0, "variant_id": "4001" }] },
                "quantity": 2 })),
        )
        .await;
    let wanted_id = id(&wanted);
    r.owner("GET /api/wanted", "GET", "/api/wanted?all=true", None)
        .await;
    r.owner(
        "PUT /api/wanted/{id}",
        "PUT",
        &format!("/api/wanted/{wanted_id}"),
        Some(json!({ "price": 50.0 })),
    )
    .await;
    // Cart links (cart.rs): offline, so the test reads nothing from the store
    r.owner(
        "POST /api/wanted/cart",
        "POST",
        "/api/wanted/cart",
        Some(json!({ "offline": true })),
    )
    .await;
    r.owner(
        "POST /api/wanted/{id}/bought",
        "POST",
        &format!("/api/wanted/{wanted_id}/bought"),
        Some(json!({ "shipping": 7.0, "at": "2026-10-21" })),
    )
    .await;
    r.owner(
        "DELETE /api/wanted/{id}",
        "DELETE",
        &format!("/api/wanted/{wanted_id}"),
        None,
    )
    .await;
    r.owner(
        "POST /api/purchases",
        "POST",
        "/api/purchases",
        Some(
            json!({ "supplier": "Test", "shipping": 12.0, "fees": 1.5, "note": "order 1",
                      "lines": [{ "bean_id": bean_id, "weight_g": 500.0, "price": 11.0 }] }),
        ),
    )
    .await;
    // Orders from receipts (orders.rs): the dry run, the order, its states and receipts
    let receipt = json!({ "supplier": "Test", "order_number": "1001", "placed_at": "2026-10-20",
        "currency": "USD", "subtotal": 21.0, "shipping": 8.0, "tax": 0.2, "total": 29.2,
        "lines": [
            { "name": "Kenya Nyeri", "bag": "1 LB", "quantity": 2, "price": 18.0, "bean_id": bean_id },
            { "kind": "other", "name": "Colour card", "price": 3.0, "taxed": true }] });
    r.owner(
        "POST /api/orders (dry run)",
        "POST",
        "/api/orders?dry_run=true",
        Some(receipt.clone()),
    )
    .await;
    let order = r
        .owner(
            "POST /api/orders",
            "POST",
            "/api/orders",
            Some(receipt.clone()),
        )
        .await;
    let order_id = order["order"]["id"].as_str().expect("order id").to_string();
    r.owner(
        "POST /api/orders (the same again)",
        "POST",
        "/api/orders",
        Some(receipt),
    )
    .await;
    let eml = h
        .owner_raw(
            "POST",
            &format!(
                "/api/orders/{order_id}/receipts?file_name=order-1001.eml&message_id=%3Ca%40b%3E"
            ),
            b"From: shop@example.com\r\nSubject: Order 1001\r\n\r\nThank you".to_vec(),
        )
        .await;
    r.keep("POST /api/orders/{id}/receipts", &eml);
    let receipt_id = id(&eml.json());
    let file = h
        .owner(
            "GET",
            &format!("/api/orders/{order_id}/receipts/{receipt_id}"),
            None,
        )
        .await;
    r.keep("GET /api/orders/{id}/receipts/{receipt_id}", &file);
    r.anon(
        "GET /api/orders/{id}/receipts/{receipt_id} (anonymous)",
        "GET",
        &format!("/api/orders/{order_id}/receipts/{receipt_id}"),
    )
    .await;
    r.owner(
        "POST /api/orders/{id}/shipped",
        "POST",
        &format!("/api/orders/{order_id}/shipped"),
        Some(json!({ "carrier": "USPS", "tracking": "9400", "at": "2026-10-22" })),
    )
    .await;
    r.owner(
        "POST /api/orders/{id}/received",
        "POST",
        &format!("/api/orders/{order_id}/received"),
        Some(json!({ "at": "2026-10-24", "lines": [{ "line": "1", "weight_g": 900.0 }] })),
    )
    .await;
    r.owner("GET /api/orders", "GET", "/api/orders?status=all", None)
        .await;
    r.owner(
        "GET /api/orders/{id}",
        "GET",
        &format!("/api/orders/{order_id}"),
        None,
    )
    .await;
    r.owner(
        "DELETE /api/orders/{id}/receipts/{receipt_id}",
        "DELETE",
        &format!("/api/orders/{order_id}/receipts/{receipt_id}"),
        None,
    )
    .await;
    let other = h
        .owner(
            "POST",
            "/api/orders",
            Some(json!({ "supplier": "Test", "order_number": "1002", "total": 9.0,
                "lines": [{ "name": "Kenya Nyeri", "bag": "1 LB", "price": 9.0, "bean_id": bean_id }] })),
        )
        .await
        .json();
    let other_id = other["order"]["id"].as_str().expect("order id").to_string();
    r.owner(
        "POST /api/orders/{id}/cancel",
        "POST",
        &format!("/api/orders/{other_id}/cancel"),
        None,
    )
    .await;
    r.owner(
        "DELETE /api/orders/{id}",
        "DELETE",
        &format!("/api/orders/{other_id}"),
        None,
    )
    .await;
    let plan = r
        .owner(
            "POST /api/plan",
            "POST",
            "/api/plan",
            Some(json!({
                "device_id": VR, "bean_id": bean_id, "profile_id": profile_id,
                "green_weight_g": 100.0, "name": "Planned", "notes": "first"
            })),
        )
        .await;
    let plan_id = id(&plan);
    let plan2 = h
        .owner("POST", "/api/plan", Some(json!({ "name": "Second" })))
        .await
        .json();
    r.owner("GET /api/plan", "GET", "/api/plan", None).await;
    r.owner(
        "GET /api/plan/{id}",
        "GET",
        &format!("/api/plan/{plan_id}"),
        None,
    )
    .await;
    r.owner(
        "PUT /api/plan/{id}",
        "PUT",
        &format!("/api/plan/{plan_id}"),
        Some(json!({ "notes": "edited" })),
    )
    .await;
    r.owner(
        "POST /api/plan/reorder",
        "POST",
        "/api/plan/reorder",
        Some(json!({ "ids": [id(&plan2), plan_id] })),
    )
    .await;

    // ── A roast on the virtual roaster, driven by the profile
    let session = r
        .owner(
            "POST /api/sessions",
            "POST",
            "/api/sessions",
            Some(json!({
                "name": "Shape roast", "device_id": VR, "profile_id": profile_id,
                "bean_id": bean_id, "green_weight": 100.0, "planned_roast_id": plan_id
            })),
        )
        .await;
    let sid = id(&session);
    let s = |p: &str| format!("/api/sessions/{sid}{p}");
    r.owner("POST /api/sessions/{id}/start", "POST", &s("/start"), None)
        .await;
    r.owner(
        "POST /api/sessions/{id}/replay",
        "POST",
        &s("/replay"),
        Some(json!({})),
    )
    .await;
    // Once the drive has sent its first command (last_sent_at is null before)
    let replay = s("/replay");
    assert!(
        eventually(|| async {
            h.owner("GET", &replay, None).await.json()["last_sent_at"].is_string()
        })
        .await,
        "the drive sends"
    );
    r.owner("GET /api/sessions/{id}/replay", "GET", &replay, None)
        .await;
    r.owner(
        "PATCH /api/sessions/{id}/replay",
        "PATCH",
        &s("/replay"),
        Some(json!({ "lookahead_seconds": 10.0 })),
    )
    .await;
    r.owner(
        "POST /api/sessions/{id}/replay/pause",
        "POST",
        &s("/replay/pause"),
        Some(json!({ "channel": "fan" })),
    )
    .await;
    r.owner(
        "POST /api/sessions/{id}/replay/resume",
        "POST",
        &s("/replay/resume"),
        Some(json!({ "channel": "fan" })),
    )
    .await;
    let events = r
        .owner(
            "GET /api/sessions/{id}/replay/events",
            "GET",
            &s("/replay/events"),
            None,
        )
        .await;
    if let Some(first) = events
        .as_array()
        .or_else(|| events["events"].as_array())
        .and_then(|e| e.first())
    {
        r.owner(
            "POST /api/sessions/{id}/replay/events/{event_id}",
            "POST",
            &s(&format!("/replay/events/{}", id(first))),
            Some(json!({ "op": "skip" })),
        )
        .await;
    }
    r.owner(
        "GET /api/sessions/{id}/replay/log",
        "GET",
        &s("/replay/log"),
        None,
    )
    .await;
    r.owner("GET /api/replay", "GET", "/api/replay", None).await;
    r.owner("GET /api/follow", "GET", "/api/follow", None).await;
    r.owner("GET /api/sessions/{id}/follow", "GET", &s("/follow"), None)
        .await;
    r.owner(
        "POST /api/sessions/{id}/drive/resume (already running)",
        "POST",
        &s("/drive/resume"),
        None,
    )
    .await;

    let event = r
        .owner(
            "POST /api/sessions/{id}/events",
            "POST",
            &s("/events"),
            Some(
                json!({ "event_type": "first_crack_start", "elapsed_seconds": 420.0,
                          "temperature": 196.0, "notes": "loud" }),
            ),
        )
        .await;
    r.owner("GET /api/sessions/{id}/events", "GET", &s("/events"), None)
        .await;
    r.owner(
        "PUT /api/sessions/{id}/events/{event_id}",
        "PUT",
        &s(&format!("/events/{}", id(&event))),
        Some(
            json!({ "event_type": "first_crack_start", "elapsed_seconds": 425.0,
                      "temperature": 197.0 }),
        ),
    )
    .await;
    // Enough bean temperature for a profile and an Artisan file
    // A sample at 0 s too: the profile's charge temperature is the BT there
    for t in 0..40 {
        let t = f64::from(t) * 15.0;
        h.owner(
            "POST",
            &s("/telemetry"),
            Some(json!({ "elapsed_seconds": t, "bean_temp": 80.0 + t / 5.0, "fan": 80.0 })),
        )
        .await;
    }
    r.owner(
        "POST /api/sessions/{id}/telemetry",
        "POST",
        &s("/telemetry"),
        Some(
            json!({ "elapsed_seconds": 5.0, "bean_temp": 150.0, "env_temp": 200.0,
                      "rate_of_rise": 10.0, "heater": 50.0, "fan": 80.0, "setpoint": 180.0 }),
        ),
    )
    .await;
    // A live sample from the virtual roaster too (it carries `extras`), so the shape does
    // not depend on whether one landed in time.
    for _ in 0..50 {
        let t = h.owner("GET", &s("/telemetry"), None).await.json();
        let live = t["telemetry"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|p| p.get("extras").is_some()));
        if live {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    r.owner(
        "GET /api/sessions/{id}/telemetry",
        "GET",
        &s("/telemetry"),
        None,
    )
    .await;
    r.owner("POST /api/sessions/{id}/pause", "POST", &s("/pause"), None)
        .await;
    r.owner(
        "POST /api/sessions/{id}/resume",
        "POST",
        &s("/resume"),
        None,
    )
    .await;
    r.owner(
        "DELETE /api/sessions/{id}/replay",
        "DELETE",
        &s("/replay"),
        None,
    )
    .await;
    r.owner(
        "POST /api/sessions/{id}/complete",
        "POST",
        &s("/complete"),
        None,
    )
    .await;
    r.owner(
        "PUT /api/sessions/{id}",
        "PUT",
        &s(""),
        Some(
            json!({ "notes": "sweet", "roasted_weight": 85.0, "color_whole": 70.0,
                      "color_scale": "agtron", "rating": 4 }),
        ),
    )
    .await;
    r.owner("GET /api/sessions/{id}", "GET", &s(""), None).await;
    r.owner(
        "GET /api/sessions",
        "GET",
        &format!("/api/sessions?device_id={VR}&limit=10"),
        None,
    )
    .await;
    r.owner(
        "GET /api/sessions/{id} (unknown)",
        "GET",
        "/api/sessions/nope",
        None,
    )
    .await;
    // Roasted coffee (docs/roasted.md): the roast above, weighed, is a lot resting
    let used = r
        .owner(
            "POST /api/roasted/{id}/moves",
            "POST",
            &format!("/api/roasted/{sid}/moves"),
            Some(json!({ "kind": "given", "grams": 20, "recipient": "Sam", "note": "a gift" })),
        )
        .await;
    r.owner(
        "POST /api/roasted/{id}/moves (more than is left)",
        "POST",
        &format!("/api/roasted/{sid}/moves"),
        Some(json!({ "kind": "used", "grams": 500 })),
    )
    .await;
    r.owner("GET /api/roasted", "GET", "/api/roasted?tz=-420", None)
        .await;
    r.owner(
        "GET /api/roasted/{id}",
        "GET",
        &format!("/api/roasted/{sid}"),
        None,
    )
    .await;
    let given = used["moves"][0]["id"]
        .as_str()
        .expect("the gift")
        .to_string();
    r.owner(
        "DELETE /api/roasted/{id}/moves/{move_id}",
        "DELETE",
        &format!("/api/roasted/{sid}/moves/{given}"),
        None,
    )
    .await;
    r.owner(
        "GET /api/roasted/{id} (unknown)",
        "GET",
        "/api/roasted/nope",
        None,
    )
    .await;
    // Roasts in one bag (docs/roasted.md, "Roasts in one bag"): one roast alone is refused
    r.owner(
        "POST /api/roasted/combined (one roast)",
        "POST",
        "/api/roasted/combined",
        Some(json!({ "roasts": [sid] })),
    )
    .await;
    r.owner(
        "GET /api/roasted/combined/{id} (unknown)",
        "GET",
        "/api/roasted/combined/nope",
        None,
    )
    .await;
    r.owner(
        "DELETE /api/roasted/combined/{id} (unknown)",
        "DELETE",
        "/api/roasted/combined/nope",
        None,
    )
    .await;
    // Packed bags (docs/roasted.md, "Packed bags") and the trace of the roast and its bean
    r.owner(
        "POST /api/roasted/{id}/moves (packed)",
        "POST",
        &format!("/api/roasted/{sid}/moves"),
        Some(json!({ "kind": "packed", "bags": 1, "bag_g": 10, "grind": "espresso" })),
    )
    .await;
    r.owner(
        "GET /api/trace/roast/{id}",
        "GET",
        &format!("/api/trace/roast/{sid}?tz=-420"),
        None,
    )
    .await;
    r.owner(
        "GET /api/trace/bean/{id}",
        "GET",
        &format!("/api/trace/bean/{bean_id}?tz=-420"),
        None,
    )
    .await;
    r.owner(
        "GET /api/trace/roast/{id} (unknown)",
        "GET",
        "/api/trace/roast/nope",
        None,
    )
    .await;
    // The analysis of a roast depends on the samples that landed, so only its refusals are
    // kept here (its schema is in openapi.json; analysis.rs tests the numbers).
    r.owner(
        "GET /api/sessions/{id}/analysis (unknown)",
        "GET",
        "/api/sessions/nope/analysis",
        None,
    )
    .await;
    r.owner(
        "GET /api/beans/{id}/guide (unknown)",
        "GET",
        "/api/beans/nope/guide",
        None,
    )
    .await;
    // After a roast the lines depend on the samples that landed: only the refusal here
    r.owner(
        "GET /api/beans/{id}/guide (after no such roast)",
        "GET",
        &format!("/api/beans/{bean_id}/guide?after=nope"),
        None,
    )
    .await;
    r.owner(
        "GET /api/analysis/compare (one roast)",
        "GET",
        &format!("/api/analysis/compare?roasts={sid}"),
        None,
    )
    .await;
    // The roaster's model (docs/prediction.md): the virtual roaster here has no roast with a
    // DROP, so it is the prior with no score. A live prediction and a roast's score depend
    // on the clock and the samples that landed: only their refusals are kept.
    r.owner(
        "GET /api/roaster/{id}/model",
        "GET",
        &format!("/api/roaster/{VR}/model"),
        None,
    )
    .await;
    r.owner(
        "GET /api/roaster/{id}/prediction (no roast)",
        "GET",
        "/api/roaster/nobody/prediction",
        None,
    )
    .await;
    r.owner(
        "GET /api/sessions/{id}/prediction (unknown)",
        "GET",
        "/api/sessions/nope/prediction",
        None,
    )
    .await;
    r.owner(
        "GET /api/profiles/{id}/history",
        "GET",
        &format!("/api/profiles/{profile_id}/history?device_id={VR}"),
        None,
    )
    .await;
    let detail = r
        .owner(
            "GET /api/beans/{id}",
            "GET",
            &format!("/api/beans/{bean_id}"),
            None,
        )
        .await;
    r.owner("GET /api/sessions/{id}/cost", "GET", &s("/cost"), None)
        .await;
    r.owner("GET /api/costs", "GET", "/api/costs?months=6", None)
        .await;
    let purchase = detail["moves"]
        .as_array()
        .and_then(|ms| ms.iter().find(|m| m["note"] == "bought more"))
        .and_then(|m| m["id"].as_str())
        .expect("the purchase")
        .to_string();
    r.owner(
        "PUT /api/beans/{id}/moves/{move_id}",
        "PUT",
        &format!("/api/beans/{bean_id}/moves/{purchase}"),
        Some(json!({ "price": 25.0, "fees": null })),
    )
    .await;
    r.owner(
        "DELETE /api/beans/{id}/moves/{move_id}",
        "DELETE",
        &format!("/api/beans/{bean_id}/moves/{purchase}"),
        None,
    )
    .await;
    r.owner(
        "POST /api/sessions/{id}/cupping",
        "POST",
        &s("/cupping"),
        Some(json!({ "scoring_framework": "sca", "notes": "jasmine",
                      "attributes": [{ "name": "aroma", "score": 8.0 },
                                     { "name": "acidity", "score": 7.5 }] })),
    )
    .await;
    r.owner(
        "GET /api/sessions/{id}/cupping",
        "GET",
        &s("/cupping"),
        None,
    )
    .await;
    r.owner(
        "POST /api/sessions/{id}/profile",
        "POST",
        &s("/profile"),
        Some(json!({ "name": "From the shape roast" })),
    )
    .await;
    r.owner(
        "GET /api/sessions/{id}/export/csv",
        "GET",
        &s("/export/csv"),
        None,
    )
    .await;
    r.owner("GET /api/labels", "GET", "/api/labels", None).await;
    r.owner(
        "GET /api/sessions/{id}/labels.pdf",
        "GET",
        &s("/labels.pdf?count=2&start=2&design=gift&message=for%20Sam"),
        None,
    )
    .await;
    r.owner(
        "GET /api/sessions/{id}/labels.svg",
        "GET",
        &s("/labels.svg?sheet=5294"),
        None,
    )
    .await;
    // Reports (docs/reports.md): the kinds, a roast's report and its data, of a finished
    // roast with fixed samples (the shape roast is still running: its data would vary)
    let done = h
        .owner(
            "POST",
            "/api/sessions",
            Some(
                json!({ "name": "Report roast", "device_id": "bench_roaster",
                         "green_weight": 100.0, "bean_id": bean_id, "profile_id": profile_id }),
            ),
        )
        .await
        .json();
    let rid = id(&done);
    h.owner("POST", &format!("/api/sessions/{rid}/start"), None)
        .await;
    for sec in (0..=620).step_by(10) {
        let t = f64::from(sec);
        h.owner(
            "POST",
            &format!("/api/sessions/{rid}/telemetry"),
            Some(json!({ "elapsed_seconds": t, "bean_temp": 25.0 + 0.3 * t,
                         "env_temp": 40.0 + 0.3 * t, "heater": 80.0, "fan": 90.0 })),
        )
        .await;
    }
    for (kind, at) in [
        ("charge", 0.0),
        ("drying_end", 240.0),
        ("first_crack_start", 460.0),
        ("drop", 600.0),
    ] {
        h.owner(
            "POST",
            &format!("/api/sessions/{rid}/events"),
            Some(json!({ "event_type": kind, "elapsed_seconds": at })),
        )
        .await;
    }
    h.owner("POST", &format!("/api/sessions/{rid}/complete"), None)
        .await;
    h.owner(
        "PUT",
        &format!("/api/sessions/{rid}"),
        Some(json!({ "roasted_weight": 85.5, "color_ground": 74.0, "color_scale": "agtron" })),
    )
    .await;
    let ten: Vec<Value> = crate::roasts::cupping::ATTRIBUTES
        .iter()
        .map(|(k, ..)| json!({ "name": k, "score": 8.0 }))
        .collect();
    h.owner(
        "POST",
        &format!("/api/sessions/{rid}/cupping"),
        Some(json!({ "attributes": ten, "taint_cups": 1, "notes": "stone fruit" })),
    )
    .await;
    r.owner("GET /api/reports", "GET", "/api/reports", None)
        .await;
    r.owner(
        "GET /api/reports/roast/{id}",
        "GET",
        &format!("/api/reports/roast/{rid}?tz=-300"),
        None,
    )
    .await;
    r.owner(
        "GET /api/reports/roast/{id}?format=json",
        "GET",
        &format!("/api/reports/roast/{rid}?format=json&units=F&paper=a4"),
        None,
    )
    .await;
    r.owner(
        "GET /api/reports/cupping",
        "GET",
        &format!("/api/reports/cupping?roasts={rid}&blank=true&samples=1"),
        None,
    )
    .await;
    r.owner(
        "GET /api/reports/cupping?blank=true&format=json",
        "GET",
        &format!("/api/reports/cupping?roasts={rid}&blank=true&samples=1&format=json"),
        None,
    )
    .await;
    r.owner(
        "GET /api/reports/cupping?format=json",
        "GET",
        &format!("/api/reports/cupping?roasts={rid}&format=json"),
        None,
    )
    .await;
    // Two roasts compared: only its refusal, as the analysis it reads changes shape with the
    // samples that landed
    r.owner(
        "GET /api/reports/compare (the same roast twice)",
        "GET",
        &format!("/api/reports/compare?roasts={rid},{rid}&format=json"),
        None,
    )
    .await;
    r.owner(
        "GET /api/reports/inventory",
        "GET",
        "/api/reports/inventory?format=json",
        None,
    )
    .await;
    r.owner(
        "GET /api/reports/inventory?count=true",
        "GET",
        "/api/reports/inventory?count=true&format=json",
        None,
    )
    .await;
    r.owner(
        "GET /api/reports/shopping",
        "GET",
        "/api/reports/shopping?format=json",
        None,
    )
    .await;
    r.owner(
        "GET /api/reports/coffee/{id}",
        "GET",
        &format!("/api/reports/coffee/{bean_id}?format=json"),
        None,
    )
    .await;
    r.owner(
        "GET /api/reports/period",
        "GET",
        "/api/reports/period?format=json",
        None,
    )
    .await;
    // From the dated purchases above to today, not the default "this month" (on the 1st it held
    // only today's, and the chart's shape changed with the date)
    r.owner(
        "GET /api/reports/purchases",
        "GET",
        "/api/reports/purchases?format=json&from=2026-10-01",
        None,
    )
    .await;
    r.owner(
        "GET /api/reports/plan",
        "GET",
        "/api/reports/plan?format=json",
        None,
    )
    .await;
    r.owner(
        "GET /api/reports/trace",
        "GET",
        &format!("/api/reports/trace?roast={sid}&format=json"),
        None,
    )
    .await;
    r.owner(
        "GET /api/reports/profile/{id}",
        "GET",
        &format!("/api/reports/profile/{profile_id}?format=json&roaster=bench_roaster"),
        None,
    )
    .await;
    r.owner(
        "GET /api/labels/calibration.pdf",
        "GET",
        "/api/labels/calibration.pdf?sheet=6874",
        None,
    )
    .await;
    // The label studio: a check of a saved design on the roast, a draft that does not
    // compile, and a one-off render
    r.owner(
        "POST /api/labels/check",
        "POST",
        "/api/labels/check",
        Some(json!({ "design": "gift", "sheet": "6874", "roast": sid, "png": 50 })),
    )
    .await;
    r.owner(
        "POST /api/labels/check (does not compile)",
        "POST",
        "/api/labels/check",
        Some(json!({ "source": "#let label(l, w, h, round) = text(l.nmae)" })),
    )
    .await;
    r.owner(
        "POST /api/labels/check (a coffee, a brand kit not saved)",
        "POST",
        "/api/labels/check",
        Some(json!({ "design": "shop", "bean": bean_id,
                     "brand_kit": { "name": "Hill Street Roasting", "place": "Portland, OR" } })),
    )
    .await;
    r.owner(
        "POST /api/labels/check (for sale on a 12 oz pouch)",
        "POST",
        "/api/labels/check",
        Some(json!({ "design": "shop", "sheet": "6874", "retail": "pouch-12oz",
                     "brand_kit": { "name": "Hill Street Roasting", "place": "Portland, OR 97201" } })),
    )
    .await;
    r.owner(
        "POST /api/labels/render",
        "POST",
        "/api/labels/render",
        Some(json!({ "source": "#let label(l, w, h, round) = text(l.name)", "format": "svg" })),
    )
    .await;
    // The label printer: none is set here, so nothing is sent
    r.owner(
        "GET /api/labels/printer",
        "GET",
        "/api/labels/printer",
        None,
    )
    .await;
    r.owner(
        "POST /api/sessions/{id}/labels/print (no printer)",
        "POST",
        &s("/labels/print"),
        Some(json!({ "count": 1 })),
    )
    .await;
    // Print runs: a run of the roast in two designs, its PDF, a reprint, marked printed
    let run_ask = json!({ "sheet": "5163", "note": "gifts", "items": [
        { "roast": sid, "count": 2, "design": "gift", "message": "for Sam" },
        { "roast": sid, "design": "plain", "weight_g": 10, "grind": "espresso", "pack": true } ] });
    r.owner(
        "POST /api/label-runs/preview",
        "POST",
        "/api/label-runs/preview",
        Some(run_ask.clone()),
    )
    .await;
    let run = r
        .owner(
            "POST /api/label-runs",
            "POST",
            "/api/label-runs",
            Some(run_ask),
        )
        .await;
    let run_id = run["id"].as_str().unwrap_or_default().to_string();
    r.owner("GET /api/label-runs", "GET", "/api/label-runs", None)
        .await;
    r.owner(
        "GET /api/label-runs/{id}",
        "GET",
        &format!("/api/label-runs/{run_id}"),
        None,
    )
    .await;
    r.owner(
        "GET /api/label-runs/{id}/pdf",
        "GET",
        &format!("/api/label-runs/{run_id}/pdf"),
        None,
    )
    .await;
    r.owner(
        "POST /api/label-runs/{id}/print (no printer)",
        "POST",
        &format!("/api/label-runs/{run_id}/print"),
        None,
    )
    .await;
    r.owner(
        "POST /api/label-runs/{id}/reprint",
        "POST",
        &format!("/api/label-runs/{run_id}/reprint"),
        Some(json!({ "labels": [3], "start": 2 })),
    )
    .await;
    r.owner(
        "POST /api/label-runs/{id}/done",
        "POST",
        &format!("/api/label-runs/{run_id}/done"),
        None,
    )
    .await;
    r.owner("GET /api/label-sheets", "GET", "/api/label-sheets", None)
        .await;
    // Share links (docs/labels.md, "The share page"): made, made again (the same), listed,
    // a label with its QR code to the page, revoked
    let link = r
        .owner(
            "POST /api/sessions/{id}/share",
            "POST",
            &s("/share"),
            Some(json!({ "message": "Happy birthday, Sam", "tz": 0 })),
        )
        .await;
    let code = link["code"].as_str().unwrap_or_default().to_string();
    r.owner(
        "POST /api/sessions/{id}/share (the roast's link)",
        "POST",
        &s("/share"),
        Some(json!({})),
    )
    .await;
    r.owner("GET /api/sessions/{id}/share", "GET", &s("/share"), None)
        .await;
    r.owner("GET /api/share", "GET", "/api/share", None).await;
    r.owner(
        "GET /api/sessions/{id}/labels.svg?qr=share",
        "GET",
        &s("/labels.svg?qr=share"),
        None,
    )
    .await;
    r.owner(
        "DELETE /api/share/{code}",
        "DELETE",
        &format!("/api/share/{code}"),
        None,
    )
    .await;
    // Label printers: one that does not answer (nothing listens on port 9), a calibration
    // from two corners; finding printers (mDNS) and reading a certificate need a network
    r.owner(
        "POST /api/label-printers (does not answer)",
        "POST",
        "/api/label-printers",
        Some(
            json!({ "id": "office", "name": "Office laser", "url": "ipp://127.0.0.1:9/ipp/print" }),
        ),
    )
    .await;
    r.owner(
        "GET /api/label-printers",
        "GET",
        "/api/label-printers",
        None,
    )
    .await;
    r.owner(
        "GET /api/label-printers/{id}",
        "GET",
        "/api/label-printers/office",
        None,
    )
    .await;
    r.owner(
        "PUT /api/label-printers/{id}",
        "PUT",
        "/api/label-printers/office",
        Some(json!({ "name": "Office laser", "url": "ipp://127.0.0.1:9/ipp/print", "tray": "tray-1", "color": "monochrome" })),
    )
    .await;
    r.owner(
        "PUT /api/label-printers/{id}/calibration/{sheet}",
        "PUT",
        "/api/label-printers/office/calibration/5163",
        Some(json!({ "first": [1.0, 0.4], "last": [1.2, 0.6] })),
    )
    .await;
    r.owner(
        "DELETE /api/label-printers/{id}/calibration/{sheet}",
        "DELETE",
        "/api/label-printers/office/calibration/5163",
        None,
    )
    .await;
    r.owner(
        "POST /api/label-printers/{id}/default",
        "POST",
        "/api/label-printers/office/default",
        None,
    )
    .await;
    r.owner(
        "DELETE /api/label-printers/{id}",
        "DELETE",
        "/api/label-printers/office",
        None,
    )
    .await;
    r.owner(
        "PUT /api/label-sheets/{id}",
        "PUT",
        "/api/label-sheets/5163",
        Some(json!({ "next": 1 })),
    )
    .await;
    // Saved designs, assets, brands and a coffee's label (the label studio)
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10"/></svg>"#;
    let asset = h
        .owner_raw("PUT", "/api/label-assets/mark.svg", svg.to_vec())
        .await;
    r.keep("PUT /api/label-assets/{name}", &asset);
    r.owner("GET /api/label-assets", "GET", "/api/label-assets", None)
        .await;
    let file = h.owner("GET", "/api/label-assets/mark.svg", None).await;
    r.keep("GET /api/label-assets/{name}", &file);
    r.owner(
        "PUT /api/label-brands/{id}",
        "PUT",
        "/api/label-brands/home",
        Some(json!({ "name": "Hill Street", "logo": "mark.svg", "ink": "#2b2118" })),
    )
    .await;
    r.owner("GET /api/label-brands", "GET", "/api/label-brands", None)
        .await;
    r.owner(
        "GET /api/label-brands/{id}",
        "GET",
        "/api/label-brands/home",
        None,
    )
    .await;
    let source = "#let design = (name: \"Mark\", params: (size: 12))\n#let label(l, w, h, round) = box(inset: 0.2in, text(size: l.params.size * 1pt, l.name))";
    r.owner(
        "POST /api/label-designs",
        "POST",
        "/api/label-designs",
        Some(json!({ "id": "mark", "source": source, "note": "first" })),
    )
    .await;
    r.owner(
        "POST /api/label-designs (does not compile)",
        "POST",
        "/api/label-designs",
        Some(json!({ "id": "broken", "source": "#let label(l, w, h, round) = text(l.nmae)" })),
    )
    .await;
    r.owner(
        "PUT /api/label-designs/{id}",
        "PUT",
        "/api/label-designs/mark",
        Some(json!({ "source": source.replace("12", "14"), "note": "bigger" })),
    )
    .await;
    r.owner(
        "POST /api/label-designs/{id}/rollback",
        "POST",
        "/api/label-designs/mark/rollback",
        Some(json!({ "version": 1 })),
    )
    .await;
    r.owner("GET /api/label-designs", "GET", "/api/label-designs", None)
        .await;
    r.owner(
        "GET /api/label-designs/{id}",
        "GET",
        "/api/label-designs/mark@2",
        None,
    )
    .await;
    r.owner(
        "GET /api/label-designs/{id} (built in)",
        "GET",
        "/api/label-designs/cherry",
        None,
    )
    .await;
    r.owner(
        "PUT /api/beans/{id}/label",
        "PUT",
        &format!("/api/beans/{bean_id}/label"),
        Some(json!({ "design": "mark@2", "params": { "size": 16 }, "brand": "home" })),
    )
    .await;
    r.owner(
        "PUT /api/beans/{id}/label (clear)",
        "PUT",
        &format!("/api/beans/{bean_id}/label"),
        Some(json!({})),
    )
    .await;
    r.owner(
        "DELETE /api/label-designs/{id}",
        "DELETE",
        "/api/label-designs/mark",
        None,
    )
    .await;
    r.owner(
        "DELETE /api/label-brands/{id}",
        "DELETE",
        "/api/label-brands/home",
        None,
    )
    .await;
    r.owner(
        "DELETE /api/label-assets/{name}",
        "DELETE",
        "/api/label-assets/mark.svg",
        None,
    )
    .await;
    let session_alog = h.owner("GET", &s("/export/artisan"), None).await;
    r.keep("GET /api/sessions/{id}/export/artisan", &session_alog);

    // Artisan imports, from the exports above
    let profile_alog = h
        .text(&format!("/api/profiles/{profile_id}/export/artisan"))
        .await;
    r.owner(
        "POST /api/profiles/import/artisan",
        "POST",
        "/api/profiles/import/artisan",
        Some(json!({ "alog_content": profile_alog, "name": "Imported" })),
    )
    .await;
    let roast_alog = h.text(&s("/export/artisan")).await;
    let imported = r
        .owner(
            "POST /api/sessions/import/artisan",
            "POST",
            "/api/sessions/import/artisan",
            Some(json!({ "alog_content": roast_alog, "device_id": VR, "name": "Imported roast" })),
        )
        .await;
    r.owner(
        "DELETE /api/sessions/{id}",
        "DELETE",
        &format!("/api/sessions/{}", id(&imported)),
        None,
    )
    .await;

    // A second roast, cancelled (which discards it)
    let other = h
        .owner(
            "POST",
            "/api/sessions",
            Some(json!({ "name": "Cancelled", "device_id": VR })),
        )
        .await
        .json();
    let other_id = id(&other);
    h.owner("POST", &format!("/api/sessions/{other_id}/start"), None)
        .await;
    r.owner(
        "POST /api/sessions/{id}/cancel",
        "POST",
        &format!("/api/sessions/{other_id}/cancel"),
        None,
    )
    .await;

    // ── Device configuration
    let device = r
        .owner(
            "POST /api/devices",
            "POST",
            "/api/devices",
            Some(
                json!({ "name": "Bench", "device_id": "bench_01", "description": "test",
                          "location": "garage" }),
            ),
        )
        .await;
    let device_id = id(&device);
    r.owner("GET /api/devices", "GET", "/api/devices", None)
        .await;
    r.owner(
        "GET /api/devices/{id} (unknown)",
        "GET",
        "/api/devices/nobody",
        None,
    )
    .await;
    r.owner(
        "PUT /api/devices/{id}",
        "PUT",
        &format!("/api/devices/{device_id}"),
        Some(json!({ "location": "kitchen" })),
    )
    .await;
    let conn = r
        .owner(
            "POST /api/devices/{id}/connections",
            "POST",
            &format!("/api/devices/{device_id}/connections"),
            Some(json!({ "protocol": "mqtt", "enabled": false, "priority": 1,
                          "config": { "topic_prefix": "roaster/bench_01" } })),
        )
        .await;
    if conn.get("id").is_some() {
        r.owner(
            "PUT /api/devices/{id}/connections/{conn_id}",
            "PUT",
            &format!("/api/devices/{device_id}/connections/{}", id(&conn)),
            Some(json!({ "priority": 2 })),
        )
        .await;
    }
    r.owner(
        "GET /api/devices/{id}",
        "GET",
        &format!("/api/devices/{device_id}"),
        None,
    )
    .await;
    if conn.get("id").is_some() {
        r.owner(
            "DELETE /api/devices/{id}/connections/{conn_id}",
            "DELETE",
            &format!("/api/devices/{device_id}/connections/{}", id(&conn)),
            None,
        )
        .await;
    }
    r.owner(
        "PUT /api/devices/{id}/register-map",
        "PUT",
        &format!("/api/devices/{device_id}/register-map"),
        Some(
            json!([{ "register_type": "holding", "address": 0, "name": "bt",
                       "data_type": "int16", "scale_factor": 0.1, "unit": "°C",
                       "writable": false }]),
        ),
    )
    .await;
    r.owner(
        "GET /api/devices/{id}/register-map",
        "GET",
        &format!("/api/devices/{device_id}/register-map"),
        None,
    )
    .await;
    r.owner(
        "GET /api/devices/discovered",
        "GET",
        "/api/devices/discovered",
        None,
    )
    .await;
    // Artisan's machine setups (docs/artisan-machines.md)
    r.owner(
        "GET /api/artisan/machines",
        "GET",
        "/api/artisan/machines",
        None,
    )
    .await;
    r.owner(
        "POST /api/artisan/machines/plan",
        "POST",
        "/api/artisan/machines/plan",
        Some(json!({ "machine": "Berto/Autonics_Control" })),
    )
    .await;
    r.owner(
        "POST /api/artisan/machines/plan (unknown)",
        "POST",
        "/api/artisan/machines/plan",
        Some(json!({ "machine": "Nobody/Nothing" })),
    )
    .await;
    let dp = r
        .owner(
            "POST /api/device-profiles",
            "POST",
            "/api/device-profiles",
            Some(json!({ "name": "Bench profile", "default_setpoint": 180.0,
                          "default_fan": 80.0, "max_temp": 240.0 })),
        )
        .await;
    r.owner(
        "GET /api/device-profiles",
        "GET",
        "/api/device-profiles",
        None,
    )
    .await;
    if dp.get("id").is_some() {
        let dp_id = id(&dp);
        r.owner(
            "GET /api/device-profiles/{id}",
            "GET",
            &format!("/api/device-profiles/{dp_id}"),
            None,
        )
        .await;
        r.owner(
            "PUT /api/device-profiles/{id}",
            "PUT",
            &format!("/api/device-profiles/{dp_id}"),
            Some(json!({ "name": "Bench profile", "max_temp": 230.0 })),
        )
        .await;
        r.owner(
            "DELETE /api/device-profiles/{id}",
            "DELETE",
            &format!("/api/device-profiles/{dp_id}"),
            None,
        )
        .await;
    }
    r.owner(
        "DELETE /api/devices/{id}",
        "DELETE",
        &format!("/api/devices/{device_id}"),
        None,
    )
    .await;

    // ── Tokens
    let token = r
        .owner(
            "POST /api/tokens",
            "POST",
            "/api/tokens",
            Some(json!({ "name": "agent", "preset": "read_only", "expires_in_days": 30 })),
        )
        .await;
    let token_id = token["token"]["id"].as_i64().expect("a token id");
    r.owner("GET /api/tokens", "GET", "/api/tokens", None).await;
    r.owner(
        "PUT /api/tokens/{id}",
        "PUT",
        &format!("/api/tokens/{token_id}"),
        Some(json!({ "require_approval": false })),
    )
    .await;

    // ── Approvals: a token's deletes held; one approved, one denied
    let agent = h
        .owner(
            "POST",
            "/api/tokens",
            Some(json!({ "name": "inventory agent", "preset": "inventory" })),
        )
        .await
        .json();
    let agent = agent["secret"].as_str().expect("a secret").to_string();
    let mut held = Vec::new();
    for name in ["Held lot", "Held lot 2"] {
        let bean = h
            .owner("POST", "/api/beans", Some(json!({ "name": name })))
            .await
            .json();
        let reply = r
            .token(
                "DELETE /api/beans/{id} (held for approval)",
                &agent,
                "DELETE",
                &format!("/api/beans/{}", id(&bean)),
                None,
            )
            .await;
        held.push(reply["approval"]["id"].as_str().expect("held").to_string());
    }
    r.token(
        "GET /api/approvals/{id}",
        &agent,
        "GET",
        &format!("/api/approvals/{}", held[0]),
        None,
    )
    .await;
    r.owner("GET /api/approvals", "GET", "/api/approvals", None)
        .await;
    r.owner(
        "POST /api/approvals/{id}/approve",
        "POST",
        &format!("/api/approvals/{}/approve", held[0]),
        None,
    )
    .await;
    r.owner(
        "POST /api/approvals/{id}/deny",
        "POST",
        &format!("/api/approvals/{}/deny", held[1]),
        None,
    )
    .await;
    r.owner(
        "POST /api/approvals/{id}/approve (already decided)",
        "POST",
        &format!("/api/approvals/{}/approve", held[1]),
        None,
    )
    .await;
    r.owner(
        "GET /api/approvals/{id}?wait (decided)",
        "GET",
        &format!("/api/approvals/{}?wait=5", held[0]),
        None,
    )
    .await;
    r.owner(
        "POST /api/tokens/{id}/revoke",
        "POST",
        &format!("/api/tokens/{token_id}/revoke"),
        None,
    )
    .await;
    r.owner(
        "DELETE /api/tokens/{id}",
        "DELETE",
        &format!("/api/tokens/{token_id}"),
        None,
    )
    .await;

    // ── Web Push and notification channels (nothing is sent: no device, no channel)
    r.owner("GET /api/push/key", "GET", "/api/push/key", None)
        .await;
    r.owner(
        "GET /api/push/subscriptions",
        "GET",
        "/api/push/subscriptions",
        None,
    )
    .await;
    r.owner(
        "POST /api/push/subscriptions (bad key)",
        "POST",
        "/api/push/subscriptions",
        Some(json!({ "endpoint": "https://push.example/abc",
                      "keys": { "p256dh": "x", "auth": "y" }, "name": "phone" })),
    )
    .await;
    r.owner(
        "POST /api/push/test",
        "POST",
        "/api/push/test",
        Some(json!({})),
    )
    .await;
    r.owner(
        "POST /api/push/unsubscribe",
        "POST",
        "/api/push/unsubscribe",
        Some(json!({ "endpoint": "https://push.example/abc" })),
    )
    .await;
    r.owner("GET /api/notifications", "GET", "/api/notifications", None)
        .await;
    r.owner(
        "GET /api/home-assistant",
        "GET",
        "/api/home-assistant",
        None,
    )
    .await;
    r.owner(
        "POST /api/notifications/test (no url)",
        "POST",
        "/api/notifications/test",
        Some(json!({ "channel": "gotify", "values": {} })),
    )
    .await;

    // ── Backups
    let made = r
        .owner("POST /api/backups", "POST", "/api/backups", None)
        .await;
    r.owner("GET /api/backups", "GET", "/api/backups", None)
        .await;
    let name = made["file"]["name"].as_str().expect("a backup name");
    r.owner(
        "POST /api/backups/{name}/check",
        "POST",
        &format!("/api/backups/{name}/check"),
        None,
    )
    .await;
    r.owner(
        "POST /api/backups/{name}/restore",
        "POST",
        &format!("/api/backups/{name}/restore"),
        None,
    )
    .await;
    r.owner(
        "DELETE /api/backups/restore",
        "DELETE",
        "/api/backups/restore",
        None,
    )
    .await;

    // ── Deletes, then the admin reset
    r.owner(
        "DELETE /api/sessions/{id}/cupping",
        "DELETE",
        &s("/cupping"),
        None,
    )
    .await;
    r.owner(
        "DELETE /api/sessions/{id}/events/{event_id}",
        "DELETE",
        &s(&format!("/events/{}", id(&event))),
        None,
    )
    .await;
    r.owner(
        "DELETE /api/plan/{id}",
        "DELETE",
        &format!("/api/plan/{}", id(&plan2)),
        None,
    )
    .await;
    r.owner(
        "DELETE /api/beans/{id}",
        "DELETE",
        &format!("/api/beans/{bean_id}"),
        None,
    )
    .await;
    r.owner(
        "DELETE /api/profiles/{id}",
        "DELETE",
        &format!("/api/profiles/{profile_id}"),
        None,
    )
    .await;
    r.owner(
        "POST /api/virtual-roaster/charge",
        "POST",
        "/api/virtual-roaster/charge",
        None,
    )
    .await;
    r.owner(
        "POST /api/virtual-roaster/dump",
        "POST",
        "/api/virtual-roaster/dump",
        None,
    )
    .await;
    r.owner(
        "POST /api/roaster/{id}/control/emergency_stop",
        "POST",
        &control("emergency_stop"),
        None,
    )
    .await;
    r.owner(
        "POST /api/admin/mqtt/reset",
        "POST",
        "/api/admin/mqtt/reset",
        None,
    )
    .await;

    // The audit log last, once the writes above have landed
    let pool = h.state.db.clone();
    assert!(
        eventually(|| async {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM audit_log WHERE path LIKE '%mqtt/reset'",
            )
            .fetch_one(&pool)
            .await
            .unwrap_or(0)
                > 0
        })
        .await
    );
    r.owner("GET /api/audit", "GET", "/api/audit?limit=500", None)
        .await;

    compare(&r.shapes);
}

fn compare(shapes: &BTreeMap<String, Value>) {
    let got = serde_json::to_value(shapes).unwrap();
    if std::env::var("UPDATE_API_SHAPES").is_ok() {
        std::fs::create_dir_all(std::path::Path::new(GOLDEN).parent().unwrap()).unwrap();
        std::fs::write(GOLDEN, serde_json::to_string_pretty(&got).unwrap() + "\n").unwrap();
        return;
    }
    let want: Value = serde_json::from_str(
        &std::fs::read_to_string(GOLDEN).expect("testdata/api-shapes.json (UPDATE_API_SHAPES=1)"),
    )
    .unwrap();
    let names: BTreeSet<&String> = want
        .as_object()
        .unwrap()
        .keys()
        .chain(got.as_object().unwrap().keys())
        .collect();
    let diffs: Vec<String> = names
        .into_iter()
        .filter(|name| want.get(*name) != got.get(*name))
        .map(|name| {
            format!(
                "{name}:\n  want {}\n  got  {}",
                want.get(name).map_or("(none)".into(), Value::to_string),
                got.get(name).map_or("(none)".into(), Value::to_string)
            )
        })
        .collect();
    assert!(
        diffs.is_empty(),
        "API shapes changed (UPDATE_API_SHAPES=1 to accept):\n{}",
        diffs.join("\n")
    );
}

#[test]
fn shapes_union_array_elements() {
    let v = json!([{ "a": 1, "b": null }, { "a": 2, "b": "x", "c": true }]);
    assert_eq!(
        shape(&v),
        json!([{ "a": "integer", "b": "null|string", "c": "absent|boolean" }])
    );
    assert_eq!(shape(&json!([])), json!(["empty"]));
    assert_eq!(shape(&json!([1, 2.5])), json!(["integer|number"]));
}
