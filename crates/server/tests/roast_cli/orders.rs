//! A supplier order from its receipt: ordered, shipped, received.

use roastwarden_server::test_support::Harness;

use crate::common::roast;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_records_an_order_from_its_receipt() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let token = h.token("inventory").await;
    let t = Some(token.as_str());
    let dir = std::env::temp_dir().join(format!("roastwarden-order-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let order = dir.join("order.json");
    std::fs::write(
        &order,
        r#"{ "supplier": "Sweet Maria's", "order_number": "000412345", "placed_at": "2024-12-31",
             "currency": "USD", "subtotal": 21.65, "shipping": 8.99, "tax": 0.21, "total": 30.85,
             "lines": [
               { "name": "Kenya Nyeri Gatugi AA", "sku": "GCX-8087-001", "bag": "1 LB", "quantity": 1, "price": 9.15 },
               { "name": "Ethiopia Guji Hambela", "bag": "1 LB", "quantity": 1, "price": 9.50 },
               { "name": "Roasted Coffee Color Card", "price": 3.00, "kind": "other", "taxed": true } ] }"#,
    )
    .unwrap();
    let eml = dir.join("confirmation.eml");
    std::fs::write(
        &eml,
        "From: orders@example.com\r\nMessage-ID: <412345@example.com>\r\nSubject: Order 000412345\r\n\r\nThanks",
    )
    .unwrap();
    let (order, eml) = (order.to_str().unwrap(), eml.to_str().unwrap());

    let r = roast(
        &url,
        t,
        &[
            "bean",
            "add",
            "--name",
            "Kenya Nyeri Gatugi AA",
            "--supplier",
            "Sweet Maria's",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{}", r.stderr);

    // The dry run: Kenya by name, the Guji a new bean, the card not coffee; nothing written
    let r = roast(
        &url,
        t,
        &["order", "import", order, "--receipt", eml, "--dry-run"],
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert_eq!(r.json["check"]["ok"], true, "{}", r.json);
    assert_eq!(r.json["check"]["totals"]["adds_up"], true);
    let results: Vec<&str> = r.json["check"]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["result"].as_str().unwrap())
        .collect();
    assert_eq!(results, ["matched", "new", "other"]);
    let r = roast(&url, t, &["orders", "--all"]).await;
    assert_eq!(r.json.as_array().map(Vec::len), Some(0));

    // Recorded with its receipt: on order
    let r = roast(&url, t, &["order", "import", order, "--receipt", eml]).await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert_eq!(r.json["order"]["status"], "ordered");
    assert_eq!(
        r.json["order"]["receipts"][0]["message_id"],
        "<412345@example.com>"
    );
    let r = roast(&url, t, &["beans"]).await;
    let kenya = r
        .json
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["name"] == "Kenya Nyeri Gatugi AA")
        .unwrap()
        .clone();
    assert_eq!(
        (kenya["stock_g"].as_f64(), kenya["on_order_g"].as_f64()),
        (Some(0.0), Some(453.6))
    );

    // The same again (or a dry run of it) names the order there
    let r = roast(&url, t, &["order", "import", order]).await;
    assert_eq!(r.code, 6, "{:?}", r.json);
    let r = roast(
        &url,
        t,
        &["order", "import", "-", "--dry-run", "--receipt", eml],
    )
    .await;
    assert_ne!(r.code, 0);

    // It came, the Guji a little light
    let r = roast(
        &url,
        t,
        &[
            "order",
            "received",
            "000412345",
            "--weight",
            "2=440g",
            "--date",
            "2025-01-06",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert_eq!(r.json["status"], "received");
    let r = roast(&url, t, &["bean", "show", "Kenya Nyeri Gatugi AA"]).await;
    assert_eq!(r.json["stock_g"], 453.6);
    assert_eq!(r.json["moves"][0]["price"], 9.15);

    // The receipt: listed, saved by an inventory token, not for a read-only one or anyone
    let r = roast(&url, t, &["order", "receipt", "000412345"]).await;
    assert_eq!(r.json.as_array().map(Vec::len), Some(1), "{}", r.json);
    let saved = dir.join("saved.eml");
    let r = roast(
        &url,
        t,
        &[
            "order",
            "receipt",
            "000412345",
            "1",
            "--out",
            saved.to_str().unwrap(),
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert_eq!(std::fs::read(&saved).unwrap(), std::fs::read(eml).unwrap());
    let reader = h.token("read_only").await;
    let r = roast(
        &url,
        Some(&reader),
        &[
            "order",
            "receipt",
            "000412345",
            "1",
            "--out",
            saved.to_str().unwrap(),
        ],
    )
    .await;
    assert_eq!(r.code, 5, "{:?}", r.json);
    let r = roast(
        &url,
        None,
        &[
            "order",
            "receipt",
            "000412345",
            "1",
            "--out",
            saved.to_str().unwrap(),
        ],
    )
    .await;
    assert_eq!(r.code, 5, "{:?}", r.json);
    // The order itself is a read
    let r = roast(&url, Some(&reader), &["order", "show", "000412345"]).await;
    assert_eq!(r.code, 0);
    std::fs::remove_dir_all(&dir).ok();
}
