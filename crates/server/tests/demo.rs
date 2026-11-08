//! A public demo (`ROASTWARDEN_DEMO=1`, docs/DEPLOY.md "A public demo"): a visitor with no
//! credential roasts on the virtual roaster and records, never reaches another host, never
//! touches a real roaster or the owner's own, and is held to a number of changes a minute.
//! The owner key still does everything. The whole server (`test_support::Harness`).

use axum::http::StatusCode;
use roastwarden_server::test_support::{Harness, DEMO_WRITE_LIMIT};
use serde_json::json;

#[tokio::test]
async fn a_visitor_roasts_on_the_simulator_and_records() {
    let h = Harness::demo().await;

    let s = h.anon("GET", "/api/auth/status", None).await;
    assert_eq!(s.status, StatusCode::OK);
    assert_eq!(s.json()["demo"], json!(true));
    assert_eq!(s.json()["credential"], json!("demo"));

    // Reads need no sign-in
    assert_eq!(
        h.anon("GET", "/api/sessions", None).await.status,
        StatusCode::OK
    );

    // The simulated roaster: charge, heater, a roast
    let r = h.anon("POST", "/api/virtual-roaster/charge", None).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
    let r = h
        .anon(
            "POST",
            "/api/roaster/virtual_roaster/control/heater",
            Some(json!({ "percent": 40 })),
        )
        .await;
    assert!(r.status.is_success(), "heater: {} {:?}", r.status, r.json());
    let r = h
        .anon(
            "POST",
            "/api/sessions",
            Some(json!({ "name": "Demo roast", "device_id": "virtual_roaster" })),
        )
        .await;
    assert!(
        r.status.is_success(),
        "a roast: {} {:?}",
        r.status,
        r.json()
    );

    // Green coffee
    let r = h
        .anon("POST", "/api/beans", Some(json!({ "name": "Demo Kenya" })))
        .await;
    assert!(r.status.is_success(), "a bean: {} {:?}", r.status, r.json());
}

#[tokio::test]
async fn a_visitor_reaches_no_other_host_and_not_the_owners_own() {
    let h = Harness::demo().await;
    let refused = |r: &roastwarden_server::test_support::Reply, code: &str| {
        assert_eq!(r.status, StatusCode::FORBIDDEN, "{:?}", r.json());
        assert_eq!(r.json()["error"]["code"], json!(code), "{:?}", r.json());
    };
    // Another host: a supplier's page, a roaster's connection, another server's profile
    let r = h
        .anon(
            "POST",
            "/api/beans/import",
            Some(json!({ "url": "https://www.sweetmarias.com/x.html" })),
        )
        .await;
    refused(&r, "DEMO");
    let r = h
        .anon(
            "POST",
            "/api/devices",
            Some(json!({ "device_id": "scan", "name": "x" })),
        )
        .await;
    refused(&r, "DEMO");
    let r = h
        .anon(
            "POST",
            "/api/profiles/import/link",
            Some(json!({ "url": "http://10.0.0.1/P/x" })),
        )
        .await;
    refused(&r, "DEMO");
    // The server itself and the owner's own
    refused(
        &h.anon(
            "PUT",
            "/api/settings/public_url",
            Some(json!({ "value": "x" })),
        )
        .await,
        "DEMO",
    );
    refused(&h.anon("GET", "/api/tokens", None).await, "DEMO");
    refused(&h.anon("GET", "/api/audit", None).await, "DEMO");
    refused(&h.anon("POST", "/api/backups", None).await, "DEMO");
    // A real roaster, by the same rule as tokens
    let r = h
        .anon(
            "POST",
            "/api/roaster/esp32_roaster_01/control/heater",
            Some(json!({ "percent": 40 })),
        )
        .await;
    refused(&r, "REAL_HARDWARE");

    // A wrong key is still a wrong key, not a visitor
    let r = roastwarden_server::test_support::call(
        &h.app,
        "GET",
        "/api/sessions",
        Some("not-the-owner-key-at-all"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);

    // The owner key does everything
    let r = h.owner("GET", "/api/tokens", None).await;
    assert_eq!(r.status, StatusCode::OK);
    let s = h.owner("GET", "/api/auth/status", None).await;
    assert_eq!(s.json()["credential"], json!("owner key"));
}

#[tokio::test]
async fn a_visitor_has_a_number_of_changes_a_minute() {
    let h = Harness::demo().await;
    for i in 0..DEMO_WRITE_LIMIT {
        let r = h
            .anon(
                "POST",
                "/api/beans",
                Some(json!({ "name": format!("Bean {i}") })),
            )
            .await;
        assert!(r.status.is_success(), "change {i}: {}", r.status);
    }
    let r = h
        .anon(
            "POST",
            "/api/beans",
            Some(json!({ "name": "One too many" })),
        )
        .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(r.json()["error"]["code"], json!("DEMO_BUSY"));
    // Reads go on
    assert_eq!(
        h.anon("GET", "/api/beans", None).await.status,
        StatusCode::OK
    );
}

/// The `rw_visitor=…` pair from a reply's Set-Cookie.
fn visitor_cookie(r: &roastwarden_server::test_support::Reply) -> String {
    r.set_cookie
        .iter()
        .find(|c| c.starts_with("rw_visitor="))
        .and_then(|c| c.split(';').next())
        .expect("a visitor cookie")
        .to_string()
}

#[tokio::test]
async fn each_visitor_gets_a_simulated_roaster_of_their_own() {
    use roastwarden_server::test_support::eventually;
    let h = Harness::demo().await;

    // The first call makes the roaster and the cookie; the next one finds the same roaster
    let a = h.anon("POST", "/api/demo/roaster", None).await;
    assert_eq!(a.status, StatusCode::OK, "{:?}", a.json());
    let a_cookie = visitor_cookie(&a);
    assert!(a.set_cookie[0].contains("HttpOnly"));
    let a_id = a.json()["device_id"].as_str().unwrap().to_string();
    assert!(a_id.starts_with("virtual_") && a_id.len() == 16, "{a_id}");
    assert_eq!(a.json()["shared"], json!(false));
    let again = h
        .with_cookie(&a_cookie, "198.51.100.1", "POST", "/api/demo/roaster", None)
        .await;
    assert_eq!(again.json()["device_id"], json!(a_id));
    assert!(again.set_cookie.is_empty(), "no new cookie");

    let b = h
        .with_cookie("", "198.51.100.2", "POST", "/api/demo/roaster", None)
        .await;
    let b_cookie = visitor_cookie(&b);
    let b_id = b.json()["device_id"].as_str().unwrap().to_string();
    assert_ne!(a_id, b_id);

    // Each sees their own and the shared roaster, not the other's
    let list = |cookie: String| {
        let h = &h;
        async move {
            h.with_cookie(&cookie, "198.51.100.1", "GET", "/api/devices", None)
                .await
                .json()
                .as_array()
                .unwrap()
                .iter()
                .map(|d| d["device_id"].as_str().unwrap().to_string())
                .collect::<Vec<_>>()
        }
    };
    let seen = list(a_cookie.clone()).await;
    assert!(seen.contains(&a_id) && !seen.contains(&b_id), "{seen:?}");
    let all = h.owner("GET", "/api/devices", None).await.json();
    let all = all.as_array().unwrap();
    assert!(
        all.iter().any(|d| d["device_id"] == json!(b_id)),
        "the owner sees all"
    );

    // The roaster runs: beans in, heat, its own telemetry
    let r = h
        .with_cookie(
            &a_cookie,
            "198.51.100.1",
            "POST",
            &format!("/api/virtual-roaster/charge?roaster={a_id}"),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
    assert_eq!(r.json()["loaded"], json!(true));
    assert_eq!(r.json()["device_id"], json!(a_id));
    let r = h
        .with_cookie(
            &a_cookie,
            "198.51.100.1",
            "POST",
            &format!("/api/roaster/{a_id}/control/fan"),
            Some(json!({ "percent": 90 })),
        )
        .await;
    assert!(
        r.status.is_success(),
        "own control: {} {:?}",
        r.status,
        r.json()
    );
    let fan_at_90 = eventually(|| async {
        h.owner("GET", &format!("/api/virtual-roaster?roaster={a_id}"), None)
            .await
            .json()["fan"]
            == json!(90.0)
    })
    .await;
    assert!(fan_at_90, "the command reached the visitor's simulation");
    let shared = h.owner("GET", "/api/virtual-roaster", None).await.json();
    assert_ne!(shared["fan"], json!(90.0), "not the shared roaster's");

    // A roast on it
    let r = h
        .with_cookie(
            &a_cookie,
            "198.51.100.1",
            "POST",
            "/api/sessions",
            Some(json!({ "name": "Mine", "device_id": a_id })),
        )
        .await;
    assert!(
        r.status.is_success(),
        "a roast: {} {:?}",
        r.status,
        r.json()
    );

    // The other visitor may not change it
    for (method, uri, body) in [
        (
            "POST",
            format!("/api/roaster/{a_id}/control/heater"),
            Some(json!({ "percent": 80 })),
        ),
        (
            "POST",
            format!("/api/virtual-roaster/dump?roaster={a_id}"),
            None,
        ),
    ] {
        let r = h
            .with_cookie(&b_cookie, "198.51.100.2", method, &uri, body)
            .await;
        assert_eq!(r.status, StatusCode::FORBIDDEN, "{uri}: {:?}", r.json());
        assert_eq!(r.json()["error"]["code"], json!("DEMO"), "{uri}");
    }
    // Nor may a visitor with no cookie; the shared one stays open to all
    let r = h
        .anon(
            "POST",
            &format!("/api/roaster/{b_id}/control/fan"),
            Some(json!({ "percent": 50 })),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = h
        .with_cookie(
            &b_cookie,
            "198.51.100.2",
            "POST",
            "/api/roaster/virtual_roaster/control/fan",
            Some(json!({ "percent": 50 })),
        )
        .await;
    assert!(r.status.is_success(), "shared: {} {:?}", r.status, r.json());

    // The owner key is no visitor
    let r = h.owner("POST", "/api/demo/roaster", None).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    assert_eq!(r.json()["error"]["code"], json!("NOT_DEMO"));
}

#[tokio::test]
async fn a_full_demo_shares_its_roaster() {
    let h = Harness::demo().await;
    // Three a household; the fourth shares
    for _ in 0..3 {
        let r = h
            .with_cookie("", "203.0.113.7", "POST", "/api/demo/roaster", None)
            .await;
        assert_eq!(r.json()["shared"], json!(false));
    }
    let r = h
        .with_cookie("", "203.0.113.7", "POST", "/api/demo/roaster", None)
        .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json()["shared"], json!(true));
    assert_eq!(r.json()["device_id"], json!("virtual_roaster"));
}

#[tokio::test]
async fn only_a_demo_has_visitors() {
    let h = Harness::new().await;
    let r = h.anon("POST", "/api/demo/roaster", None).await;
    assert!(
        r.status == StatusCode::NOT_FOUND || r.status == StatusCode::UNAUTHORIZED,
        "{}",
        r.status
    );
    assert_eq!(r.set_cookie, [] as [std::string::String; 0]);
    let s = h.anon("GET", "/api/auth/status", None).await.json();
    assert_eq!(s["demo_stats"], json!(null));
}

#[tokio::test]
async fn a_counting_demo_names_its_stats_server_and_allows_it() {
    let w = "54040c0f-7339-4a00-bbdb-2985abfff097";
    let h = Harness::demo_with_stats(Some(("https://stats.example.com", w))).await;
    let s = h.anon("GET", "/api/auth/status", None).await;
    assert_eq!(
        s.json()["demo_stats"],
        json!({ "url": "https://stats.example.com", "website": w })
    );
    let csp = s.csp.unwrap();
    assert!(
        csp.contains(
            "script-src 'self' 'unsafe-inline' https://unpkg.com https://stats.example.com;"
        ),
        "{csp}"
    );
    assert!(
        csp.contains("connect-src 'self' ws: wss: https://stats.example.com;"),
        "{csp}"
    );
    // A demo that does not count names none
    let h = Harness::demo().await;
    let s = h.anon("GET", "/api/auth/status", None).await;
    assert_eq!(s.json()["demo_stats"], json!(null));
    assert!(!s.csp.unwrap().contains("stats"));
}

#[tokio::test]
async fn a_demo_tells_crawlers_its_front_page_only() {
    let h = Harness::demo().await;
    let r = h.anon("GET", "/robots.txt", None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.content_type.as_deref(), Some("text/plain"));
    let body = h.text("/robots.txt").await;
    for line in [
        "User-agent: *",
        "Allow: /$",
        "Allow: /_app/",
        "Disallow: /\n",
        "Sitemap: https://roastwarden.com/sitemap.xml",
    ] {
        assert!(body.contains(line), "{line:?} in {body}");
    }
    // Off a demo the SPA answers as before: no robots of its own
    let h = Harness::new().await;
    let r = h.anon("GET", "/robots.txt", None).await;
    assert_ne!(r.content_type.as_deref(), Some("text/plain"));
}
