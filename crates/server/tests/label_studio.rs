//! Label Studio in the dashboard (docs/labels.md, "Label Studio"): what the
//! gallery and the design page read from the server. Built-in designs carry their name,
//! description and settings; each design says which coffees use it; a check lays a design out
//! on a coffee or with a brand kit that is not saved; Typst's errors come with their span in
//! the source as a browser editor counts it.

#![allow(
    clippy::unwrap_used,
    reason = "test code: a panic is the failure, and the helpers are not #[test] functions"
)]

use roastwarden_server::test_support::Harness;
use serde_json::{json, Value};

fn lines(report: &Value) -> String {
    report["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["text"].as_str().unwrap_or_default().to_string())
        .collect::<Vec<_>>()
        .join(" | ")
}

#[tokio::test]
async fn the_gallery_reads_built_in_names_settings_and_the_coffees_using_a_design() {
    let h = Harness::new().await;
    let bean = h
        .owner(
            "POST",
            "/api/beans",
            Some(json!({"name": "Ethiopia Guji", "origin": "Ethiopia"})),
        )
        .await
        .json();
    let id = bean["id"].as_str().unwrap();
    let r = h
        .owner(
            "PUT",
            &format!("/api/beans/{id}/label"),
            Some(json!({"design": "cherry", "params": {"art": "leaf"}})),
        )
        .await;
    assert!(r.status.is_success(), "{:?}", r.json());

    let list = h.owner("GET", "/api/label-designs", None).await.json();
    let find = |id: &str| {
        list.as_array()
            .unwrap()
            .iter()
            .find(|d| d["id"] == id)
            .cloned()
            .unwrap()
    };
    let cherry = find("cherry");
    assert_eq!(cherry["name"], "Cherry", "the manifest's short name");
    assert!(cherry["description"]
        .as_str()
        .unwrap()
        .contains("coffee branch"));
    assert_eq!(cherry["params"], json!(["art", "ink"]));
    assert_eq!(cherry["used_by"], json!(["Ethiopia Guji"]));
    let shop = find("shop");
    assert_eq!(shop["kind"], "builtin");
    assert!(shop["used_by"].is_null(), "nobody uses it: no key");

    // A built-in design to start from has its name and manifest too
    let one = h.owner("GET", "/api/label-designs/shop", None).await.json();
    assert_eq!(one["name"], "Shop");
    assert!(one["manifest"]["params"]["identity"].is_object());
}

#[tokio::test]
async fn a_check_lays_a_design_out_on_a_coffee_or_with_a_brand_not_saved() {
    let h = Harness::new().await;
    let bean = h
        .owner(
            "POST",
            "/api/beans",
            Some(json!({"name": "Kenya Nyeri AA", "origin": "Kenya", "process": "washed", "region": "Nyeri"})),
        )
        .await
        .json();
    let c = h
        .owner(
            "POST",
            "/api/labels/check",
            Some(json!({"design": "plain", "bean": bean["id"]})),
        )
        .await
        .json();
    let text = lines(&c["report"]);
    assert!(text.contains("Kenya Nyeri AA"), "{text}");
    assert!(text.contains("Nyeri, Kenya"), "{text}");
    assert_eq!(c["data"], "sample", "a coffee on a sample roast");

    let missing = h
        .owner(
            "POST",
            "/api/labels/check",
            Some(json!({"design": "plain", "bean": "nope"})),
        )
        .await;
    assert_eq!(missing.status.as_u16(), 404);

    let kit =
        json!({"name": "Hill Street Roasting", "place": "Portland, OR 97201", "accent": "#8a3b12"});
    let c = h
        .owner(
            "POST",
            "/api/labels/check",
            Some(json!({"design": "shop", "brand_kit": kit, "weight_g": 340})),
        )
        .await
        .json();
    assert_eq!(c["ok"], true, "{c}");
    let text = lines(&c["report"]);
    assert!(text.contains("Hill Street Roasting"), "{text}");
    assert!(text.contains("Net wt 11.9 oz (340 g)"), "{text}");
    // Nothing was saved
    assert_eq!(
        h.owner("GET", "/api/label-brands", None).await.json(),
        json!([])
    );

    let bad = h
        .owner(
            "POST",
            "/api/labels/check",
            Some(json!({"design": "shop", "brand_kit": {"name": "x", "ink": "brown"}})),
        )
        .await;
    assert_eq!(bad.status.as_u16(), 400);
}

#[tokio::test]
async fn typst_errors_carry_their_span_as_an_editor_counts_it() {
    let h = Harness::new().await;
    // "é" is two bytes and one UTF-16 unit; the span is after it
    let source = "// Café\n#let label(l, w, h, round) = text(l.nmae)\n";
    let c = h
        .owner("POST", "/api/labels/check", Some(json!({"source": source})))
        .await
        .json();
    let e = &c["errors"][0];
    assert_eq!(e["file"], "design.typ", "{c}");
    assert_eq!(e["line"], 2);
    let from = e["from"].as_u64().unwrap() as usize;
    let to = e["to"].as_u64().unwrap() as usize;
    let utf16: Vec<u16> = source.encode_utf16().collect();
    let span = String::from_utf16(&utf16[from..to]).unwrap();
    assert!(span.contains("nmae"), "the span is {span:?}");
}
