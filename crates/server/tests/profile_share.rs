//! Shared profiles (docs/profiles.md, "Sharing a profile"): a profile's
//! public page at `/P/<code>` with its files, and the import from a link. Anyone opens the
//! page and files with no login (counted); the file says nothing about this server; an
//! import from this server's own link is read here; a link to a private address is never
//! fetched; a read-only token cannot publish a profile; revoked is the same 404 as unknown.
//! The whole server (`test_support::Harness`).

#![allow(
    clippy::unwrap_used,
    reason = "test code: a panic is the failure, and the helpers are not #[test] functions"
)]

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use roastwarden_server::test_support::{Harness, call};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn profile(h: &Harness) -> String {
    let p = h
        .owner(
            "POST",
            "/api/profiles",
            Some(json!({
                "name": "Kenya light filter",
                "description": "Bright and quick.",
                "roast_level": "light", "roast_purpose": "filter",
                "roaster_kind": "fluid_bed_electric",
                "target_total_time": 570, "target_first_crack": 465, "target_end_temp": 210.0,
                "charge_temp": 30.0, "batch_size_g": 100.0,
                "points": [
                    { "time_seconds": 0, "target_temp": 30.0, "fan_speed": 100 },
                    { "time_seconds": 240, "target_temp": 160.0, "fan_speed": 100 },
                    { "time_seconds": 465, "target_temp": 200.0, "fan_speed": 82 },
                    { "time_seconds": 570, "target_temp": 210.0, "fan_speed": 80 }
                ],
                "events": [
                    { "trigger_kind": "time", "at_seconds": 240.0, "action": "fan", "value": 85.0 },
                    { "trigger_kind": "bt", "at_bt": 160.0, "action": "landmark", "label": "drying_end" }
                ]
            })),
        )
        .await;
    assert!(p.status.is_success(), "{:?}", p.json());
    p.json()["id"].as_str().unwrap().to_string()
}

/// A GET with no credential: status, headers and the body as text
async fn get(h: &Harness, uri: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    let request = Request::get(uri).body(Body::empty()).unwrap();
    let res = h.app.clone().oneshot(request).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = to_bytes(res.into_body(), 8 << 20).await.unwrap();
    (status, headers, String::from_utf8_lossy(&bytes).to_string())
}

#[tokio::test]
async fn a_shared_profile_is_public_imports_and_revokes() {
    let h = Harness::new().await;
    let id = profile(&h).await;

    let made = h
        .owner(
            "POST",
            &format!("/api/profiles/{id}/share"),
            Some(json!({})),
        )
        .await;
    assert_eq!(made.status, StatusCode::CREATED, "{:?}", made.json());
    let link = made.json();
    let code = link["code"].as_str().unwrap().to_string();
    assert_eq!(code.len(), 25);
    let url = link["url"].as_str().unwrap().to_string();
    assert!(url.ends_with(&format!("/P/{code}")), "{url}");
    // Again: the same link
    let again = h
        .owner("POST", &format!("/api/profiles/{id}/share"), None)
        .await;
    assert_eq!(again.status, StatusCode::OK);
    assert_eq!(again.json()["code"], json!(code));

    // The page: no login, nothing loaded, not indexed; the profile and its files, not its id
    let (st, hd, html) = get(&h, &format!("/P/{code}")).await;
    assert_eq!(st, StatusCode::OK);
    assert!(
        hd[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap()
            .starts_with("default-src 'none'")
    );
    assert_eq!(hd["x-robots-tag"], "noindex, nofollow");
    assert!(html.contains("Kenya light filter") && html.contains("Light roast for filter"));
    assert!(html.contains("an electric fluid bed") && html.contains("<svg"));
    assert!(html.contains("fan 85 %") && html.contains("Dry end"));
    assert!(html.contains(&format!("{code}/profile.json")));
    assert!(!html.contains(&id) && !html.contains("<script"));
    let (st, _, _) = get(&h, &format!("/p/{}", code.to_lowercase())).await;
    assert_eq!(st, StatusCode::OK);

    // The file: the shared format, readable from any site, nothing about this server
    let (st, hd, body) = get(&h, &format!("/P/{code}/profile.json")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(hd[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
    let doc: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(doc["format"], "roastwarden.profile/1");
    assert_eq!(doc["points"].as_array().unwrap().len(), 4);
    assert_eq!(doc["events"].as_array().unwrap().len(), 2);
    for private in ["id", "profile_id", "created_by", "created_at", "is_public"] {
        assert!(
            !body.contains(&format!("\"{private}\"")),
            "{private} in {body}"
        );
    }
    let (st, _, alog) = get(&h, &format!("/P/{code}/profile.alog")).await;
    assert_eq!(st, StatusCode::OK);
    assert!(alog.contains("timex"), "an Artisan file");
    assert!(
        !alog.contains(&id.replace('-', "")),
        "the profile's id in the .alog"
    );
    let (st, _, _) = get(&h, &format!("/P/{code}/other.txt")).await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // Counted
    let links = h
        .owner("GET", &format!("/api/profiles/{id}/share"), None)
        .await
        .json();
    assert_eq!(links[0]["views"], json!(2));
    assert_eq!(links[0]["downloads"], json!(2));

    // The file imports as it is (the Artisan import reads it)
    let r = h
        .owner(
            "POST",
            "/api/profiles/import/artisan",
            Some(json!({ "alog_content": body, "name": "From the file" })),
        )
        .await;
    assert!(r.status.is_success(), "{:?}", r.json());
    assert_eq!(r.json()["roast_level"], "light");

    // From this server's own link: read here, no request. A preview saves nothing.
    let before = h.owner("GET", "/api/profiles", None).await.json();
    let n = before.as_array().unwrap().len();
    let p = h
        .owner(
            "POST",
            "/api/profiles/import/link",
            Some(json!({ "url": url, "preview": true })),
        )
        .await;
    assert_eq!(p.status, StatusCode::OK, "{:?}", p.json());
    assert!(p.json()["saved"].is_null());
    assert_eq!(p.json()["profile"]["name"], "Kenya light filter");
    let after = h.owner("GET", "/api/profiles", None).await.json();
    assert_eq!(after.as_array().unwrap().len(), n);
    let saved = h
        .owner(
            "POST",
            "/api/profiles/import/link",
            Some(json!({ "url": format!("{url}/profile.json"), "name": "Imported" })),
        )
        .await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json());
    let s = &saved.json()["saved"];
    assert_eq!(s["name"], "Imported");
    assert_eq!(s["points"].as_array().unwrap().len(), 4);
    assert_eq!(s["events"].as_array().unwrap().len(), 2);
    let desc = s["description"].as_str().unwrap();
    assert!(
        desc.starts_with("Bright and quick.") && desc.contains(&format!("Imported from {url}")),
        "{desc}"
    );
    // A bare code is this server's too
    let r = h
        .owner(
            "POST",
            "/api/profiles/import/link",
            Some(json!({ "url": code, "preview": true })),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);

    // Never a private or local address, whatever the name resolves to
    for bad in [
        format!("https://127.0.0.1/P/{code}"),
        format!("https://localhost:8443/P/{code}"),
        format!("https://192.168.1.172/P/{code}"),
        format!("https://[::1]/P/{code}"),
    ] {
        let r = h
            .owner(
                "POST",
                "/api/profiles/import/link",
                Some(json!({ "url": bad, "preview": true })),
            )
            .await;
        assert_eq!(r.status, StatusCode::BAD_GATEWAY, "{bad}: {:?}", r.json());
        assert!(
            r.json().to_string().contains("private or local"),
            "{bad}: {:?}",
            r.json()
        );
    }
    // Another server over http, or not a profile link at all
    for bad in [
        format!("http://roast.example.com/P/{code}"),
        format!("https://roast.example.com/R/{code}"),
        "file:///etc/passwd".to_string(),
    ] {
        let r = h
            .owner(
                "POST",
                "/api/profiles/import/link",
                Some(json!({ "url": bad, "preview": true })),
            )
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{bad}");
    }

    // A read-only token sees the links but cannot publish or import
    let ro = h.token("read_only").await;
    let r = call(
        &h.app,
        "GET",
        &format!("/api/profiles/{id}/share"),
        Some(&ro),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    for (m, uri, body) in [
        ("POST", format!("/api/profiles/{id}/share"), json!({})),
        (
            "POST",
            "/api/profiles/import/link".to_string(),
            json!({ "url": url }),
        ),
        ("DELETE", format!("/api/profile-shares/{code}"), json!({})),
    ] {
        let r = call(&h.app, m, &uri, Some(&ro), Some(body)).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN, "{m} {uri}");
    }

    // Revoked: the page, its files and the import are a 404, like an unknown code
    let r = h
        .owner("DELETE", &format!("/api/profile-shares/{code}"), None)
        .await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(r.json()["revoked_at"].is_string());
    let (st, _, gone) = get(&h, &format!("/P/{code}")).await;
    let (st2, _, unknown) = get(&h, "/P/AAAAAAAAAAAAAAAAAAAAAAAAA").await;
    assert_eq!((st, st2), (StatusCode::NOT_FOUND, StatusCode::NOT_FOUND));
    assert_eq!(gone, unknown);
    let (st, _, _) = get(&h, &format!("/P/{code}/profile.json")).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let r = h
        .owner(
            "POST",
            "/api/profiles/import/link",
            Some(json!({ "url": url })),
        )
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    // Its own code: the hint is to ask for a new link, not to look for the profile
    assert_eq!(
        r.json()["error"]["code"],
        "LINK_NOT_ACTIVE",
        "{:?}",
        r.json()
    );
    // A new one after a revoke
    let r = h
        .owner("POST", &format!("/api/profiles/{id}/share"), None)
        .await;
    assert_eq!(r.status, StatusCode::CREATED);
    assert_ne!(r.json()["code"], json!(code));
    let all = h
        .owner("GET", "/api/profile-shares?all=true", None)
        .await
        .json();
    assert_eq!(all.as_array().unwrap().len(), 2);
    let active = h.owner("GET", "/api/profile-shares", None).await.json();
    assert_eq!(active.as_array().unwrap().len(), 1);

    // A profile deleted takes its links with it
    let r = h
        .owner("DELETE", &format!("/api/profiles/{id}"), None)
        .await;
    assert!(r.status.is_success());
    let active = h.owner("GET", "/api/profile-shares", None).await.json();
    assert_eq!(active.as_array().unwrap().len(), 0);
}
