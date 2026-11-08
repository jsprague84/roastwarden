//! Runs the real `roast` binary against a served harness and reads what it printed; the shared fixtures.

use std::process::Command;

use roastwarden_server::test_support::Harness;
use serde_json::Value;

/// What `roast` printed (one JSON line on a pipe) and its exit code.
pub struct Ran {
    pub code: i32,
    pub json: Value,
    pub stderr: String,
}

/// Run `roast` with `url` and `token` and nothing from this environment.
pub async fn roast(url: &str, token: Option<&str>, args: &[&str]) -> Ran {
    let url = url.to_string();
    let token = token.map(str::to_string);
    let args: Vec<String> = args.iter().map(std::string::ToString::to_string).collect();
    // The server runs on this test's runtime: wait for the process off it
    tokio::task::spawn_blocking(move || {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_roast"));
        for var in [
            "ROASTWARDEN_URL",
            "ROASTWARDEN_TOKEN",
            "ROASTWARDEN_API_KEY",
            "ROASTWARDEN_DB_PATH",
            "ROASTWARDEN_BACKUP_DIR",
            "RUSTROAST_URL",
            "RUSTROAST_TOKEN",
        ] {
            cmd.env_remove(var);
        }
        // Never what `roast login` saved on the machine running the tests
        cmd.env(
            "ROASTWARDEN_CONFIG",
            "/nonexistent/roastwarden-test/roast.json",
        );
        cmd.env("ROASTWARDEN_URL", &url);
        if let Some(t) = &token {
            cmd.env("ROASTWARDEN_TOKEN", t);
        }
        let output = cmd.args(&args).output().expect("the roast binary");
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let json = serde_json::from_str(stdout.trim()).unwrap_or(Value::String(stdout));
        Ran {
            code: output.status.code().unwrap_or(-1),
            json,
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        }
    })
    .await
    .unwrap()
}

pub fn error_code(r: &Ran) -> &str {
    r.json["error"]["code"].as_str().unwrap_or("")
}

/// A finished roast on `device` (BT rising 0.3 °C/s, CHARGE at 5 s, FC at 381 s, DROP at
/// 611 s): its id.
pub async fn finished_roast(h: &Harness, device: &str, name: &str) -> String {
    let s = h
        .owner(
            "POST",
            "/api/sessions",
            Some(serde_json::json!({ "name": name, "device_id": device, "green_weight": 100.0 })),
        )
        .await
        .json();
    let id = s["id"].as_str().unwrap().to_string();
    h.owner("POST", &format!("/api/sessions/{id}/start"), None)
        .await;
    for sec in (0..=620).step_by(5) {
        let bt = 25.0 + 0.3 * f64::from(sec);
        h.owner(
            "POST",
            &format!("/api/sessions/{id}/telemetry"),
            Some(serde_json::json!({ "elapsed_seconds": sec, "bean_temp": bt })),
        )
        .await;
    }
    for (kind, at) in [
        ("charge", 5.0),
        ("drying_end", 200.0),
        ("first_crack_start", 381.0),
        ("drop", 611.0),
    ] {
        h.owner(
            "POST",
            &format!("/api/sessions/{id}/events"),
            Some(serde_json::json!({ "event_type": kind, "elapsed_seconds": at })),
        )
        .await;
    }
    h.owner("POST", &format!("/api/sessions/{id}/complete"), None)
        .await;
    id
}
