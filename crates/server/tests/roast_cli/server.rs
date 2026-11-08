//! Health, status, failures with their codes, the server's backups and the busy gate.

use std::path::Path;
use std::process::Command;

use roastwarden_server::test_support::{Harness, OWNER_KEY};
use serde_json::Value;

use crate::common::{error_code, roast};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn health_status_and_reads_with_and_without_a_token() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;

    let r = roast(&url, None, &["health"]).await;
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(r.json["status"], "ok");
    assert_eq!(r.json["version"], env!("CARGO_PKG_VERSION"));

    let r = roast(&url, None, &["status"]).await;
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(r.json["db_ready"], true);
    assert!(r.json["roasters"].is_array());
    // Where the build stands: a test build has no `git describe`, so no release count
    assert!(r.json["build"].is_object(), "{}", r.json);
    assert!(r.json["build"].get("commits_since_release").is_none());

    let r = roast(&url, None, &["profiles"]).await;
    assert_eq!(r.code, 0);
    assert!(
        r.json.as_array().is_some_and(|p| !p.is_empty()),
        "the seed profiles"
    );

    // By default (no ROASTWARDEN_READS=open) a read needs a token too: exit 5 with the hint
    let closed = Harness::new().await;
    let closed_url = closed.serve().await;
    let r = roast(&closed_url, None, &["profiles"]).await;
    assert_eq!(r.code, 5, "{}", r.json);
    assert_eq!(error_code(&r), "UNAUTHORIZED");
    let read = closed.token("read_only").await;
    assert_eq!(roast(&closed_url, Some(&read), &["profiles"]).await.code, 0);
    let r = roast(&closed_url, Some(&read), &["audit"]).await;
    assert_eq!(error_code(&r), "TOKEN_SCOPE", "the audit log is admin's");
    for cmd in [&["beans"][..], &["roasts"], &["plan"], &["backups"]] {
        let r = roast(&url, None, cmd).await;
        assert_eq!(r.code, 0, "{cmd:?}: {}", r.stderr);
    }
    // The audit log needs the owner key or an admin token, even with reads open
    let r = roast(&url, None, &["audit"]).await;
    assert_eq!(error_code(&r), "UNAUTHORIZED");
    assert_eq!(roast(&url, Some(OWNER_KEY), &["audit"]).await.code, 0);

    // A pipe gets one compact line with no nulls
    let r = roast(&url, None, &["roasts"]).await;
    assert_eq!(r.json, Value::Array(vec![]));

    // Lists are for finding: no timestamps or descriptions; `profile show` has them
    let profiles = roast(&url, None, &["profiles"]).await.json;
    let first = &profiles[0];
    assert!(first.get("description").is_none() && first.get("created_at").is_none());
    // Each says the roaster kind it is for (the drum seeds, migration 051)
    assert!(profiles
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["id"] == "drum-light-filter" && p["roaster_kind"] == "drum_gas"));
    let name = first["name"].as_str().unwrap().to_string();
    let r = roast(&url, None, &["profile", "show", &name]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert!(r.json["description"].is_string());

    // A profile made in the app (private by default) is listed and found by name too
    let made = h
        .owner(
            "POST",
            "/api/profiles",
            Some(serde_json::json!({
                "name": "Owner's medium espresso",
                "points": [{"time_seconds": 0, "target_temp": 30.0}, {"time_seconds": 600, "target_temp": 212.0}]
            })),
        )
        .await;
    assert_eq!(made.json()["is_public"], false);
    let r = roast(&url, None, &["profiles"]).await;
    assert!(
        r.json
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "Owner's medium espresso"),
        "{:?}",
        r.json
    );
    let r = roast(&url, None, &["profile", "show", "owner's medium espresso"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);

    // Finding the call for `roast api` without reading the whole document
    let r = roast(&url, None, &["openapi", "--find", "approve"]).await;
    assert_eq!(r.code, 0);
    let ops = r.json.as_array().unwrap();
    assert!(ops
        .iter()
        .any(|o| o["method"] == "POST" && o["path"] == "/api/approvals/{id}/approve"));
    assert!(r.json.to_string().len() < 1000, "a short list");

    let r = roast(&url, None, &["openapi", "--offline"]).await;
    assert_eq!(r.code, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failures_have_codes_hints_and_exit_codes() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let add = ["bean", "add", "--name", "Kenya"];

    // No credential for a write: 5, and the hint says how to get one
    let r = roast(&url, None, &add).await;
    assert_eq!(
        (r.code, error_code(&r)),
        (5, "UNAUTHORIZED"),
        "{:?}",
        r.json
    );
    assert!(r.json["error"]["hint"]
        .as_str()
        .unwrap()
        .contains("ROASTWARDEN_TOKEN"));

    // A read-only token may not
    let read_only = h.token("read_only").await;
    let r = roast(&url, Some(&read_only), &add).await;
    assert_eq!((r.code, error_code(&r)), (5, "TOKEN_SCOPE"));

    // A token Roastwarden does not know is refused even for a read
    let r = roast(&url, Some("rr_not-a-token"), &["beans"]).await;
    assert_eq!((r.code, error_code(&r)), (5, "TOKEN_INVALID"));

    // Not found: 4, with the names there are
    let r = roast(
        &url,
        Some(OWNER_KEY),
        &["bean", "stock", "add", "Kenya", "1kg"],
    )
    .await;
    assert_eq!((r.code, error_code(&r)), (4, "NOT_FOUND"));
    assert!(r.json["error"]["details"]["known"].is_array());
    let r = roast(&url, None, &["api", "GET", "/api/sessions/nope"]).await;
    assert_eq!((r.code, error_code(&r)), (4, "NOT_FOUND"));

    // Invalid: 6
    let r = roast(
        &url,
        Some(OWNER_KEY),
        &["api", "POST", "/api/beans", "--body", r#"{"name": ""}"#],
    )
    .await;
    assert_eq!(r.code, 6, "{:?}", r.json);

    // Bad arguments: 2 (clap)
    let r = roast(
        &url,
        Some(OWNER_KEY),
        &["bean", "stock", "add", "Kenya", "5 stone"],
    )
    .await;
    assert_eq!(r.code, 2, "{}", r.stderr);

    // The owner key in the environment (as inside the container) is never used by itself
    let url2 = url.clone();
    let owner_env = tokio::task::spawn_blocking(move || {
        Command::new(env!("CARGO_BIN_EXE_roast"))
            .env_remove("ROASTWARDEN_TOKEN")
            .env("ROASTWARDEN_URL", &url2)
            .env("ROASTWARDEN_API_KEY", OWNER_KEY)
            .args(["bean", "add", "--name", "Sneaky"])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(
        owner_env.status.code(),
        Some(5),
        "ROASTWARDEN_API_KEY must not be a credential"
    );

    // Unreachable: 7
    let closed = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let r = roast(&format!("http://127.0.0.1:{closed}"), None, &["health"]).await;
    assert_eq!((r.code, error_code(&r)), (7, "UNREACHABLE"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn backups_by_the_server_and_next_to_the_database() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let admin = h.token("admin").await;
    let db = h.db_path();
    let db = db.to_str().unwrap();

    let r = roast(&url, Some(&admin), &["backups", "--now"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert!(r.json["file"]["name"]
        .as_str()
        .unwrap()
        .starts_with("roastwarden-"));

    // Without the server: a copy, recorded in the run history the server shows (copies are
    // named by the second)
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let r = roast(&url, None, &["backup", "--db", db]).await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    let name = r.json["name"].as_str().unwrap().to_string();
    assert!(Path::new(r.json["backup"].as_str().unwrap()).is_file());
    let r = roast(&url, None, &["backups"]).await;
    assert_eq!(r.json["last"]["trigger"], "cli", "{:?}", r.json);

    // A copy by its name in the backup folder: checked, then staged for the next start
    let r = roast(&url, None, &["restore", &name, "--check", "--db", db]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["restorable"], true);
    let r = roast(&url, None, &["restore", &name, "--db", db]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert!(Path::new(r.json["staged"].as_str().unwrap()).is_file());
    let r = roast(&url, None, &["backups"]).await;
    assert_eq!(r.json["restore_staged"], true);

    // Not a database: 6; no such file: 4
    let junk = h.db_path().with_file_name("junk.db");
    std::fs::write(&junk, "not sqlite").unwrap();
    let r = roast(
        &url,
        None,
        &["restore", junk.to_str().unwrap(), "--check", "--db", db],
    )
    .await;
    assert_eq!(r.code, 6, "{:?}", r.json);
    let r = roast(&url, None, &["restore", "nope.db", "--check", "--db", db]).await;
    assert_eq!((r.code, error_code(&r)), (4, "NO_FILE"));
}

/// `roast busy`: a real roaster with its heater on holds a deploy (exit 1), `status` says so,
/// and `--wait` gives up with exit 1 still.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn busy_holds_for_a_real_roaster() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let read = h.token("read_only").await;
    let r = roast(&url, Some(&read), &["busy"]).await;
    assert_eq!(
        (r.code, &r.json["busy"]),
        (0, &serde_json::json!(false)),
        "{:?}",
        r.json
    );
    // A real roaster with its heater on is busy (exit 1), and `status` says so
    h.owner(
        "POST",
        "/api/test/emit-telemetry/esp32_cli",
        Some(serde_json::json!({ "beanTemp": 150.0, "heater": 40, "heaterApplied": 40 })),
    )
    .await;
    assert!(
        roastwarden_server::test_support::eventually(|| async {
            roast(&url, Some(&read), &["busy"]).await.code == 1
        })
        .await
    );
    let r = roast(&url, Some(&read), &["busy"]).await;
    assert_eq!(
        r.json["roasters"][0]["device_id"], "esp32_cli",
        "{:?}",
        r.json
    );
    assert_eq!(
        r.json["roasters"][0]["reasons"],
        serde_json::json!(["heating"])
    );
    let r = roast(&url, Some(&read), &["status"]).await;
    assert_eq!(r.json["busy"]["busy"], true, "{:?}", r.json);
    // --wait gives up after its time with exit 1 still
    let r = roast(&url, Some(&read), &["busy", "--wait", "0"]).await;
    assert_eq!(r.code, 1);
}
