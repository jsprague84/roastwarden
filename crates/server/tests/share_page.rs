//! The share page (docs/labels.md, "The share page"): a roast's
//! public page at `/R/<code>`, what a bag's QR code opens. The owner makes a link, anyone
//! opens it with no login (counted), its headers keep it out of search engines and let it
//! load nothing; revoked or unknown is the same 404. A token that may only read cannot
//! publish a roast, by a link or by a label's QR code. The whole server
//! (`test_support::Harness`).

#![allow(
    clippy::unwrap_used,
    reason = "test code: a panic is the failure, and the helpers are not #[test] functions"
)]

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use roastwarden_server::test_support::{call, Harness};
use serde_json::{json, Value};
use tower::ServiceExt;

/// A finished roast of a coffee: ten minutes of samples, charge, first crack and drop
async fn roast(h: &Harness, name: &str) -> String {
    let bean = h
        .owner(
            "POST",
            "/api/beans",
            Some(
                json!({"name": name, "origin": "Kenya", "region": "Nyeri", "process": "Washed",
                "variety": "SL28", "cupping_notes": "blackcurrant, grapefruit",
                "url": "https://www.sweetmarias.com/kenya-nyeri.html"}),
            ),
        )
        .await
        .json();
    let s = h
        .owner(
            "POST",
            "/api/sessions",
            Some(json!({ "name": "batch", "device_id": "bench_roaster",
                "bean_id": bean["id"], "target_roast_level": "light", "roast_purpose": "filter" })),
        )
        .await
        .json();
    let id = s["id"].as_str().unwrap().to_string();
    h.owner("POST", &format!("/api/sessions/{id}/start"), None)
        .await;
    for t in (0..=600).step_by(10) {
        let bt = 90.0 + 0.19 * f64::from(t);
        let r = h
            .owner(
                "POST",
                &format!("/api/sessions/{id}/telemetry"),
                Some(json!({ "elapsed_seconds": t, "bean_temp": bt })),
            )
            .await;
        assert!(r.status.is_success(), "{:?}", r.json());
    }
    for (kind, at) in [
        ("charge", 0.0),
        ("first_crack_start", 470.0),
        ("drop", 600.0),
    ] {
        h.owner(
            "POST",
            &format!("/api/sessions/{id}/events"),
            Some(json!({ "event_type": kind, "elapsed_seconds": at })),
        )
        .await;
    }
    h.owner("POST", &format!("/api/sessions/{id}/complete"), None)
        .await;
    id
}

/// A GET with no credential: status, headers, body
async fn open(h: &Harness, path: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    let res = h
        .app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let body = to_bytes(res.into_body(), 4 << 20).await.unwrap();
    (status, headers, String::from_utf8(body.to_vec()).unwrap())
}

fn assert_page_headers(h: &axum::http::HeaderMap) {
    assert_eq!(h[header::CONTENT_TYPE], "text/html; charset=utf-8");
    let csp = h[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
    assert!(csp.starts_with("default-src 'none'"), "its own CSP: {csp}");
    assert_eq!(h[header::REFERRER_POLICY], "no-referrer");
    assert_eq!(h["x-robots-tag"], "noindex, nofollow");
    assert_eq!(h[header::CACHE_CONTROL], "no-store");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_shared_roast_opens_for_anyone_until_revoked() {
    let h = Harness::new().await;
    let id = roast(&h, "Kenya Nyeri AA").await;

    let r = h
        .owner(
            "POST",
            &format!("/api/sessions/{id}/share"),
            Some(json!({"message": "Happy birthday, Sam", "tz": -420})),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json());
    let link = r.json();
    let code = link["code"].as_str().unwrap().to_string();
    assert_eq!(code.len(), 25);
    assert!(link["url"]
        .as_str()
        .unwrap()
        .ends_with(&format!("/R/{code}")));
    assert_eq!(link["name"], "Kenya Nyeri AA");
    assert_eq!(link["created_by"], "owner");

    // Sharing again is the same link; a message given replaces its message
    let again = h
        .owner(
            "POST",
            &format!("/api/sessions/{id}/share"),
            Some(json!({})),
        )
        .await;
    assert_eq!(again.status, StatusCode::OK);
    assert_eq!(again.json()["code"], code.as_str());
    assert_eq!(again.json()["message"], "Happy birthday, Sam");

    // Anyone opens it, no login
    let (status, headers, body) = open(&h, &format!("/R/{code}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_page_headers(&headers);
    assert!(body.contains("<h1>Kenya Nyeri AA</h1>"), "{body}");
    assert!(body.contains("Happy birthday, Sam"));
    assert!(body.contains("Nyeri, Kenya"));
    assert!(body.contains("Light roast for filter"));
    assert!(body.contains("blackcurrant, grapefruit"));
    assert!(body.contains("<svg"), "the curve");
    assert!(body.contains("First crack 7:50"));
    assert!(body.contains("Filter"), "a brew suggestion");
    assert!(!body.contains(&id), "never the roast's id");
    assert!(!body.contains("<script"), "no script");
    // in either case, as typed or scanned
    let (status, _, _) = open(&h, &format!("/r/{}", code.to_lowercase())).await;
    assert_eq!(status, StatusCode::OK);

    // Counted
    let links = h
        .owner("GET", &format!("/api/sessions/{id}/share"), None)
        .await
        .json();
    assert_eq!(links[0]["scans"], 2);
    assert!(links[0]["last_scan_at"].is_string());

    // Revoked: the same 404 as a code that never was
    let r = h.owner("DELETE", &format!("/api/share/{code}"), None).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
    assert!(r.json()["revoked_at"].is_string());
    let (status, headers, gone) = open(&h, &format!("/R/{code}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_page_headers(&headers);
    let (status, _, unknown) = open(&h, "/R/ABCDEFGHIJKLMNOPQRSTUVWXY").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(gone, unknown, "revoked and unknown look the same");
    let (status, _, _) = open(&h, "/R/not-a-code").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // A revoked link is listed only when asked; sharing again makes a new one
    let active = h.owner("GET", "/api/share", None).await.json();
    assert_eq!(active, json!([]));
    let all = h.owner("GET", "/api/share?all=true", None).await.json();
    assert_eq!(all.as_array().unwrap().len(), 1);
    let fresh = h
        .owner(
            "POST",
            &format!("/api/sessions/{id}/share"),
            Some(json!({})),
        )
        .await;
    assert_eq!(fresh.status, StatusCode::CREATED);
    assert_ne!(fresh.json()["code"], code.as_str());
    assert!(fresh.json()["message"].is_null());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_token_that_only_reads_cannot_publish_a_roast() {
    let h = Harness::new().await;
    let shared = roast(&h, "Kenya Nyeri AA").await;
    let private = roast(&h, "Colombia Huila").await;
    let reader = h.token("read_only").await;
    let printer = h.token("inventory").await;

    // Making a link needs a token that may print
    let r = call(
        &h.app,
        "POST",
        &format!("/api/sessions/{private}/share"),
        Some(&reader),
        Some(json!({})),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{:?}", r.json());
    let r = call(
        &h.app,
        "POST",
        &format!("/api/sessions/{shared}/share"),
        Some(&printer),
        Some(json!({})),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json());
    assert_eq!(r.json()["created_by"], "test inventory");
    let code = r.json()["code"].as_str().unwrap().to_string();

    // A label's QR code: the link it has, for anyone who may make labels
    let label = |roast: &str, kind: &str, token: &str| {
        let (app, uri, token) = (
            h.app.clone(),
            format!("/api/sessions/{roast}/labels.{kind}?qr=share"),
            token.to_string(),
        );
        async move { call(&app, "GET", &uri, Some(&token), None).await }
    };
    let links = |roast: &str| {
        let (h, roast) = (&h, roast.to_string());
        async move {
            let v: Value = h
                .owner("GET", &format!("/api/sessions/{roast}/share"), None)
                .await
                .json();
            v.as_array().unwrap().len()
        }
    };
    let r = label(&shared, "svg", &reader).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
    // A preview or a check of a roast with no link shows a placeholder and makes none
    let r = label(&private, "svg", &reader).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
    let r = h
        .owner(
            "GET",
            &format!("/api/sessions/{private}/labels.svg?qr=share"),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);
    let source = "#let label(l, w, h, round) = text(size: 6pt, l.url)\n";
    let c = h
        .owner(
            "POST",
            "/api/labels/check",
            Some(json!({"source": source, "roast": private, "qr": "share"})),
        )
        .await
        .json();
    assert!(
        c["report"]["lines"][0]["text"]
            .as_str()
            .unwrap()
            .ends_with("/R/SAMPLESHARECODE2345ABCDEF"),
        "{c}"
    );
    let r = h
        .owner(
            "POST",
            "/api/label-runs/preview",
            Some(json!({"items": [{"roast": private, "qr": "share"}]})),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(links(&private).await, 0, "previews and checks make no link");
    // The labels themselves: no new link from a reader
    let r = label(&private, "pdf", &reader).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{:?}", r.json());
    assert_eq!(links(&private).await, 0);
    // The owner's PDF makes one
    let r = h
        .owner(
            "GET",
            &format!("/api/sessions/{private}/labels.pdf?qr=share"),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(links(&private).await, 1);
    // and a check now shows it
    let c = h
        .owner(
            "POST",
            "/api/labels/check",
            Some(json!({"source": source, "roast": private, "qr": "share"})),
        )
        .await
        .json();
    assert!(
        !c["report"]["lines"][0]["text"]
            .as_str()
            .unwrap()
            .contains("SAMPLESHARECODE"),
        "{c}"
    );

    // Revoking is for the tokens that may print too
    let r = call(
        &h.app,
        "DELETE",
        &format!("/api/share/{code}"),
        Some(&reader),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    // The label's address is the share page's
    let source = "#let label(l, w, h, round) = text(size: 6pt, l.url)\n";
    let c = h
        .owner(
            "POST",
            "/api/labels/check",
            Some(json!({"source": source, "roast": shared, "qr": "share"})),
        )
        .await
        .json();
    let text: String = c["report"]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["text"].as_str().unwrap_or_default().to_string())
        .collect();
    assert!(text.contains(&format!("/R/{code}")), "{text}");
    // or the coffee's offer at its supplier
    let c = h
        .owner(
            "POST",
            "/api/labels/check",
            Some(json!({"source": source, "roast": shared, "qr": "supplier"})),
        )
        .await
        .json();
    assert_eq!(
        c["report"]["lines"][0]["text"], "https://www.sweetmarias.com/kenya-nyeri.html",
        "{c}"
    );
    // and with no QR code, there is none
    let source =
        "#let label(l, w, h, round) = text(size: 6pt, if l.qr { \"with\" } else { \"without\" })\n";
    let c = h
        .owner(
            "POST",
            "/api/labels/check",
            Some(json!({"source": source, "roast": shared, "qr": "none"})),
        )
        .await
        .json();
    assert_eq!(c["report"]["lines"][0]["text"], "without", "{c}");
    // A bad QR choice is a 400
    let r = h
        .owner(
            "GET",
            &format!("/api/sessions/{shared}/labels.svg?qr=somewhere"),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
}

/// A gift's QR code opens the share page unless asked (the gift design's manifest
/// says `qr: "share"`), with a caption beside it; a token that may not make a link gets the
/// app's page instead of an error, and a preview makes no link.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_gift_label_opens_the_share_page_by_default() {
    let h = Harness::new().await;
    let id = roast(&h, "Kenya Nyeri AA").await;
    let reader = h.token("read_only").await;
    let links = || async {
        let v: Value = h
            .owner("GET", &format!("/api/sessions/{id}/share"), None)
            .await
            .json();
        v.as_array().unwrap().len()
    };
    // A check of the gift on 6874 (room for the code): the placeholder and its caption
    let c = h
        .owner(
            "POST",
            "/api/labels/check",
            Some(json!({"design": "gift", "roast": id, "sheet": "6874"})),
        )
        .await
        .json();
    let lines: Vec<&str> = c["report"]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|l| l["text"].as_str())
        .collect();
    assert!(
        lines.iter().any(|l| l.contains("how to brew it")),
        "{lines:?}"
    );
    assert_eq!(c["ok"], true, "{c}");
    assert_eq!(links().await, 0, "a check makes no link");
    // The plain design keeps the app's page; a reader's gift PDF gets it too, no error
    let r = h
        .owner("GET", &format!("/api/sessions/{id}/labels.pdf"), None)
        .await;
    assert_eq!(r.status, StatusCode::OK);
    let r = call(
        &h.app,
        "GET",
        &format!("/api/sessions/{id}/labels.pdf?design=gift&sheet=6874"),
        Some(&reader),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
    assert_eq!(links().await, 0);
    // Asked for the app, no link either
    let r = h
        .owner(
            "GET",
            &format!("/api/sessions/{id}/labels.pdf?design=gift&qr=app"),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(links().await, 0);
    // The owner's gift PDF makes the link
    let r = h
        .owner(
            "GET",
            &format!("/api/sessions/{id}/labels.pdf?design=gift"),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(links().await, 1);
}
