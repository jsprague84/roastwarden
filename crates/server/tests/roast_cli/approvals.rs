//! Calls the owner must approve: a delete, a roast started on real hardware.

use roastwarden_server::test_support::Harness;

use crate::common::{error_code, roast};

/// A token that requires approval (the default for new tokens): a delete is held, the owner
/// approves or denies it, and `--wait` sees the answer. An agent cannot approve itself.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_delete_waits_for_the_owner() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let token = h.token("inventory").await;
    let t = Some(token.as_str());
    for name in ["Old lot", "Second lot"] {
        let r = roast(&url, t, &["bean", "add", "--name", name]).await;
        assert_eq!(r.code, 0, "{:?}", r.json);
    }

    // Held: exit 3, the request with its link
    let r = roast(&url, t, &["bean", "rm", "Old lot"]).await;
    assert_eq!(r.code, 3, "{:?} {}", r.json, r.stderr);
    let approval = &r.json["approval"];
    assert_eq!(approval["status"], "pending");
    assert_eq!(approval["summary"], "Delete bean \"Old lot\"");
    assert!(approval["url"]
        .as_str()
        .unwrap()
        .contains("/settings/approvals?id="));
    let id = approval["id"].as_str().unwrap().to_string();

    // The same call again is the same request
    let r = roast(&url, t, &["bean", "rm", "Old lot"]).await;
    assert_eq!(r.json["approval"]["id"], id.as_str());

    // The token may read it, not approve it
    let r = roast(&url, t, &["approval", &id]).await;
    assert_eq!((r.code, r.json["status"].as_str()), (3, Some("pending")));
    let approve = format!("/api/approvals/{id}/approve");
    let r = roast(&url, t, &["api", "POST", &approve]).await;
    assert_eq!((r.code, error_code(&r)), (5, "OWNER_ONLY"));
    let r = roast(&url, t, &["api", "GET", "/api/approvals"]).await;
    assert_eq!((r.code, error_code(&r)), (5, "OWNER_ONLY"));

    // The owner approves: the delete runs as the token, and the audit row names it
    let a = h.owner("POST", &approve, None).await.json();
    assert_eq!(a["status"], "done", "{a:?}");
    assert_eq!(a["result_status"], 200);
    assert_eq!(a["result"]["result"], "deleted");
    let r = roast(&url, t, &["approval", &id]).await;
    assert_eq!((r.code, r.json["status"].as_str()), (0, Some("done")));
    let r = roast(&url, t, &["beans"]).await;
    assert!(!r.json.to_string().contains("Old lot"));
    let audit = h.owner("GET", "/api/audit?limit=50", None).await.json();
    let deletes: Vec<_> = audit
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["method"] == "DELETE" && e["token_name"] == "test inventory")
        .map(|e| e["status"].as_i64().unwrap())
        .collect();
    assert_eq!(
        deletes,
        vec![200, 202, 202],
        "the run, then the two held calls"
    );
    let r = roast(&url, None, &["api", "POST", &approve]).await;
    assert_eq!(r.code, 5, "no credential");
    let again = h.owner("POST", &approve, None).await;
    assert_eq!(again.status.as_u16(), 409, "decided once");

    // --wait sees a denial: exit 1, and nothing was done
    let waiting = {
        let url = url.clone();
        let token = token.clone();
        tokio::spawn(async move {
            roast(&url, Some(&token), &["bean", "rm", "Second lot", "--wait"]).await
        })
    };
    let mut pending = None;
    for _ in 0..100 {
        let list = h.owner("GET", "/api/approvals", None).await.json();
        if let Some(p) = list
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["status"] == "pending")
        {
            pending = p["id"].as_str().map(str::to_string);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let pending = pending.expect("the second delete is held");
    let a = h
        .owner("POST", &format!("/api/approvals/{pending}/deny"), None)
        .await
        .json();
    assert_eq!(a["status"], "denied");
    let r = waiting.await.unwrap();
    assert_eq!((r.code, r.json["status"].as_str()), (1, Some("denied")));
    let r = roast(&url, t, &["beans"]).await;
    assert!(r.json.to_string().contains("Second lot"));
}

/// Starting a roast is held only on a physical roaster, and only after the owner's switch
/// let tokens near it: an approval never bypasses the real-hardware rule.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn starting_a_roast_on_real_hardware_waits_for_the_owner() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let token = h.token("roast").await;
    let t = Some(token.as_str());
    let create = |device: &str| {
        format!(r#"{{"name": "agent roast", "device_id": "{device}", "green_weight": 100}}"#)
    };

    // The virtual roaster: no approval
    let r = roast(
        &url,
        t,
        &[
            "api",
            "POST",
            "/api/sessions",
            "--body",
            &create("virtual_roaster"),
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let start = format!("/api/sessions/{}/start", r.json["id"].as_str().unwrap());
    let r = roast(&url, t, &["api", "POST", &start]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["status"], "active");
    let r = roast(
        &url,
        t,
        &[
            "api",
            "POST",
            &format!("{}/complete", start.trim_end_matches("/start")),
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);

    // A physical roaster: refused until the owner allows tokens there, then held
    let r = roast(
        &url,
        t,
        &[
            "api",
            "POST",
            "/api/sessions",
            "--body",
            &create("esp32_test"),
        ],
    )
    .await;
    let start = format!("/api/sessions/{}/start", r.json["id"].as_str().unwrap());
    let r = roast(&url, t, &["api", "POST", &start]).await;
    assert_eq!((r.code, error_code(&r)), (5, "REAL_HARDWARE"));
    let allowed = h
        .owner(
            "PUT",
            "/api/roaster/esp32_test/token-access",
            Some(serde_json::json!({ "tokens_may_control": true })),
        )
        .await;
    assert!(allowed.status.is_success());
    let r = roast(&url, t, &["api", "POST", &start]).await;
    assert_eq!(r.code, 3, "{:?}", r.json);
    assert_eq!(r.json["approval"]["action"], "roast.start");
    assert_eq!(r.json["approval"]["device_id"], "esp32_test");
    let id = r.json["approval"]["id"].as_str().unwrap().to_string();
    let a = h
        .owner("POST", &format!("/api/approvals/{id}/approve"), None)
        .await
        .json();
    assert_eq!(a["status"], "done", "{a:?}");
    assert_eq!(a["result"]["status"], "active");

    // A token made without approvals runs it at once
    let free = h
        .owner(
            "POST",
            "/api/tokens",
            Some(serde_json::json!({ "name": "trusted", "preset": "roast", "require_approval": false })),
        )
        .await
        .json();
    assert_eq!(free["token"]["require_approval"], false);
}
