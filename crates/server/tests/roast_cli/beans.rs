//! Green coffee: a purchase recorded, beans and the plan fixed, cart links from the shopping list.

use roastwarden_server::test_support::{Harness, OWNER_KEY};
use serde_json::Value;

use crate::common::{error_code, roast};

/// "I bought 5 kg of Ethiopia Guji": the bean, more stock, a correction and a planned roast,
/// with an `inventory` token, which cannot touch a roaster.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_records_a_purchase_and_plans_a_roast() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let token = h.token("inventory").await;
    let t = Some(token.as_str());

    let r = roast(
        &url,
        t,
        &[
            "bean",
            "add",
            "--name",
            "Ethiopia Guji",
            "--origin",
            "Ethiopia",
            "--process",
            "natural",
            "--stock",
            "5kg",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert_eq!(r.json["stock_g"], 5000.0);
    assert!(r.json.get("lot").is_none(), "nulls are left out");

    // What its labels print: label notes and a label name, then cleared
    let r = roast(
        &url,
        t,
        &[
            "bean",
            "edit",
            "ethiopia guji",
            "--label-notes",
            "Blueberry · cocoa",
            "--short-name",
            "Guji",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert_eq!(r.json["label_notes"], "Blueberry · cocoa");
    assert_eq!(r.json["short_name"], "Guji");
    let r = roast(
        &url,
        t,
        &["bean", "edit", "ethiopia guji", "--clear", "short_name"],
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert!(r.json.get("short_name").is_none(), "{:?}", r.json);
    assert_eq!(r.json["label_notes"], "Blueberry · cocoa");

    // A priced purchase: the 5 kg held at no known cost count at its price, 22 / 0.9072 kg.
    let r = roast(
        &url,
        t,
        &[
            "bean",
            "stock",
            "add",
            "ethiopia guji",
            "2lb",
            "--price",
            "19",
            "--shipping",
            "3",
            "--supplier",
            "Sweet Maria's",
            "--date",
            "2026-10-20",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["stock_g"].as_f64().unwrap().round(), 5907.0);
    assert_eq!(r.json["cost_per_kg"], 24.25);
    let r = roast(&url, t, &["costs", "--months", "24"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let oct = r.json["months"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["month"] == "2026-10")
        .cloned()
        .unwrap();
    assert_eq!(oct["spent"], 22.0);
    // The shopping list: wanted, then bought becomes a new bean with a priced purchase.
    let r = roast(
        &url,
        t,
        &[
            "api",
            "POST",
            "/api/wanted",
            "--body",
            r#"{"name": "Kenya Nyeri", "supplier": "Sweet Maria's", "weight_g": 1000, "price": 20}"#,
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let wanted = r.json["id"].as_str().unwrap().to_string();
    let r = roast(&url, t, &["wanted"]).await;
    assert_eq!(r.json.as_array().map(Vec::len), Some(1), "{:?}", r.json);
    let r = roast(&url, t, &["wanted", "bought", &wanted, "--shipping", "5"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["bean"]["name"], "Kenya Nyeri");
    assert_eq!(r.json["bean"]["cost_per_kg"], 25.0);
    let r = roast(&url, t, &["wanted"]).await;
    assert_eq!(r.json, Value::Array(vec![]));
    // Only known supplier hosts are read; the link is not the server's --url.
    let r = roast(
        &url,
        t,
        &["bean", "import", "https://example.com/products/x"],
    )
    .await;
    assert_eq!((r.code, error_code(&r)), (6, "BAD_REQUEST"), "{:?}", r.json);

    // A price goes with a purchase only.
    let r = roast(
        &url,
        t,
        &[
            "bean",
            "stock",
            "add",
            "ethiopia guji",
            "-100g",
            "--price",
            "1",
        ],
    )
    .await;
    assert_eq!((r.code, error_code(&r)), (6, "BAD_REQUEST"), "{:?}", r.json);

    let r = roast(
        &url,
        t,
        &[
            "bean",
            "stock",
            "set",
            "Ethiopia Guji",
            "3200g",
            "--note",
            "weighed",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["stock_g"], 3200.0);

    let r = roast(&url, t, &["bean", "show", "Ethiopia Guji"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let kinds: Vec<&str> = r.json["moves"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds.len(), 3, "{kinds:?}");
    assert!(kinds.contains(&"purchase") && kinds.contains(&"adjust"));

    let profiles = roast(&url, t, &["profiles"]).await.json;
    let profile = profiles[0]["name"].as_str().unwrap().to_string();
    let r = roast(
        &url,
        t,
        &[
            "plan",
            "add",
            "--bean",
            "Ethiopia Guji",
            "--weight",
            "100g",
            "--profile",
            &profile,
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["bean_name"], "Ethiopia Guji");
    assert_eq!(r.json["profile_name"], profile.as_str());
    assert_eq!(r.json["green_weight_g"], 100.0);

    let r = roast(&url, t, &["plan"]).await;
    assert_eq!(r.json.as_array().map(Vec::len), Some(1));
    assert!(r.json[0].get("created_at").is_none());

    // Filtered in roast, not with jq: 3200 g is above the 500 g mark
    let r = roast(&url, t, &["beans", "--low"]).await;
    assert_eq!(r.json, Value::Array(vec![]));

    let r = roast(&url, t, &["status"]).await;
    assert_eq!(r.json["planned_roasts"], 1);

    // Every write is in the audit log under the token's name (the owner reads it)
    let r = roast(&url, Some(OWNER_KEY), &["audit", "--limit", "50"]).await;
    let by_token = r.json.as_array().unwrap().iter().filter(|e| {
        e["token_name"] == "test inventory" && e["method"] == "POST" && e["path"] == "/api/plan"
    });
    assert_eq!(by_token.count(), 1, "{:?}", r.json);

    // Nothing on a roaster, not even the virtual one
    let r = roast(
        &url,
        t,
        &[
            "api",
            "POST",
            "/api/roaster/virtual_roaster/control/setpoint",
            "--body",
            r#"{"value": 150}"#,
        ],
    )
    .await;
    assert_eq!((r.code, error_code(&r)), (5, "TOKEN_SCOPE"), "{:?}", r.json);

    // The log filtered by kind, token, text and outcome on the server
    let owner = Some(OWNER_KEY);
    let r = roast(
        &url,
        owner,
        &[
            "audit",
            "--kind",
            "control",
            "--by",
            "test inventory",
            "--failed",
        ],
    )
    .await;
    let rows = r.json.as_array().unwrap();
    assert_eq!(rows.len(), 1, "{:?}", r.json);
    assert_eq!(rows[0]["kind"], "control");
    assert_eq!(rows[0]["status"], 403);
    let r = roast(
        &url,
        owner,
        &["audit", "--kind", "beans", "--find", "/api/plan"],
    )
    .await;
    let rows = r.json.as_array().unwrap();
    assert!(
        !rows.is_empty() && rows.iter().all(|e| e["kind"] == "beans"),
        "{:?}",
        r.json
    );
    let r = roast(&url, owner, &["audit", "--kind", "colour"]).await;
    assert_eq!(r.code, 6, "{:?}", r.json);
    let r = roast(&url, t, &["backups", "--now"]).await;
    assert_eq!((r.code, error_code(&r)), (5, "TOKEN_SCOPE"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_fixes_the_green_coffee_and_the_plan() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let inv = h.token("inventory").await;
    let t = Some(inv.as_str());

    for name in ["Kenya Nyeri AA", "Brazil Cerrado"] {
        let r = roast(&url, t, &["bean", "add", "--name", name, "--lot", "L1"]).await;
        assert_eq!(r.code, 0, "{:?}", r.json);
    }
    // One order, shipping split by weight: 2 lb and 3 lb of 5 lb
    let r = roast(
        &url,
        t,
        &[
            "order",
            "--supplier",
            "Sweet Maria's",
            "--shipping",
            "10",
            "--line",
            "Kenya Nyeri AA=2lb@20",
            "--line",
            "brazil cerrado=3lb@30",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let moves = r.json["moves"].as_array().unwrap();
    assert_eq!(moves.len(), 2);
    let ship: Vec<f64> = moves
        .iter()
        .map(|m| m["shipping"].as_f64().unwrap())
        .collect();
    assert!(
        (ship[0] - 4.0).abs() < 0.01 && (ship[1] - 6.0).abs() < 0.01,
        "{ship:?}"
    );
    let kenya = &r.json["beans"][0];
    assert!((kenya["stock_g"].as_f64().unwrap() - 907.2).abs() < 0.1);
    assert!(kenya["cost_per_kg"].as_f64().unwrap() > 26.0, "{kenya}");
    let r = roast(&url, t, &["order", "--line", "Nobody=1kg"]).await;
    assert_eq!(r.code, 4, "{:?}", r.json);
    let r = roast(&url, t, &["order", "--line", "Kenya Nyeri AA"]).await;
    assert_eq!(r.code, 2);

    // A purchase's price fixed, then taken out
    let move_id = moves[0]["id"].as_str().unwrap().to_string();
    let r = roast(
        &url,
        t,
        &[
            "bean",
            "stock",
            "edit",
            "Kenya Nyeri AA",
            &move_id,
            "--price",
            "22",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["price"], 22.0);
    let r = roast(
        &url,
        t,
        &["bean", "stock", "rm", "Kenya Nyeri AA", &move_id],
    )
    .await;
    assert!(
        matches!(r.code, 0 | 3),
        "done, or held for approval: {:?}",
        r.json
    );

    // A bean's details
    let r = roast(
        &url,
        t,
        &[
            "bean",
            "edit",
            "Brazil Cerrado",
            "--process",
            "natural",
            "--clear",
            "lot",
            "--low-stock",
            "500g",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["process"], "natural");
    assert!(r.json.get("lot").is_none(), "cleared");
    assert_eq!(r.json["low_stock_g"], 500.0);
    let r = roast(
        &url,
        t,
        &["bean", "edit", "Brazil Cerrado", "--clear", "price"],
    )
    .await;
    assert_eq!(r.code, 2);

    // The plan: two roasts, the second moved first, one changed, one removed
    let mut ids = Vec::new();
    for w in ["100g", "120g"] {
        let r = roast(
            &url,
            t,
            &["plan", "add", "--bean", "Brazil Cerrado", "--weight", w],
        )
        .await;
        assert_eq!(r.code, 0, "{:?}", r.json);
        ids.push(r.json["id"].as_str().unwrap().to_string());
    }
    let r = roast(&url, t, &["plan", "move", &ids[1], "--to", "1"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json[0]["id"], ids[1].as_str());
    let r = roast(
        &url,
        t,
        &[
            "plan",
            "edit",
            &ids[0],
            "--weight",
            "90g",
            "--profile",
            "fluidbed-espresso",
            "--roaster",
            "virtual_roaster",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["green_weight_g"], 90.0);
    assert_eq!(r.json["device_id"], "virtual_roaster");
    let r = roast(&url, t, &["plan", "edit", &ids[0], "--roaster", "any"]).await;
    assert!(r.json.get("device_id").is_none(), "{:?}", r.json);
    let r = roast(&url, t, &["plan", "rm", &ids[0]]).await;
    assert!(matches!(r.code, 0 | 3), "{:?}", r.json);
    let r = roast(&url, t, &["plan", "move", "nope", "--to", "1"]).await;
    assert_eq!(r.code, 4);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_builds_cart_links_from_the_shopping_list() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let token = h.token("inventory").await;
    let t = Some(token.as_str());
    let mut ids = Vec::new();
    for (name, link, vid) in [
        (
            "Rwanda",
            "https://www.sweetmarias.com/products/rwanda-x",
            "900001",
        ),
        (
            "Sumatra",
            "https://burmancoffee.com/product/green-coffee-beans/sumatra-x/",
            "900003",
        ),
    ] {
        let w = h
            .owner(
                "POST",
                "/api/wanted",
                Some(
                    serde_json::json!({ "size": "1 lb", "draft": { "name": name, "url": link,
                    "details": {}, "sizes": [{ "label": "1 lb", "weight_g": 453.6, "price": 9.0,
                    "available": true, "variant_id": vid }] } }),
                ),
            )
            .await
            .json();
        ids.push(w["id"].as_str().unwrap().to_string());
    }
    let r = roast(&url, t, &["wanted", "qty", &ids[0], "2"]).await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert_eq!(r.json["quantity"], 2);
    // Offline: nothing read from the stores (the test has no network)
    let r = roast(&url, t, &["wanted", "cart", "--offline"]).await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    let links: Vec<&str> = r.json["carts"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["url"].as_str())
        .collect();
    assert!(
        links.contains(&"https://burmancoffee.com/checkout-link/?products=900003:1"),
        "{links:?}"
    );
    assert!(
        links.contains(&"https://www.sweetmarias.com/cart/900001:2"),
        "{links:?}"
    );
    // A read-only token may not make the server reach out to the stores
    let reader = h.token("read_only").await;
    let r = roast(&url, Some(&reader), &["wanted", "cart", "--offline"]).await;
    assert_eq!(r.code, 5, "{:?}", r.json);
}
