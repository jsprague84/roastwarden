//! Roasted coffee: what was used and given, two roasts in one bag.

use roastwarden_server::test_support::Harness;
use serde_json::Value;

use crate::common::{error_code, finished_roast, roast};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_reads_roasted_coffee_and_records_what_was_used_and_given() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let t = h.token("inventory").await;
    let t = Some(t.as_str());
    let ro = h.token("read_only").await;

    let a = finished_roast(&h, "virtual_roaster", "Kenya for espresso").await;
    let b = finished_roast(&h, "virtual_roaster", "Ethiopia for filter").await;
    for (id, purpose, g) in [(&a, "espresso", 86.0), (&b, "filter", 85.0)] {
        h.owner(
            "PUT",
            &format!("/api/sessions/{id}"),
            Some(
                serde_json::json!({ "roasted_weight": g, "roast_purpose": purpose,
                                     "target_roast_level": "light" }),
            ),
        )
        .await;
    }
    // An unweighed roast on the simulator is not coffee
    finished_roast(&h, "virtual_roaster", "test only").await;

    // Fresh roasts rest: nothing ready yet; espresso alone with --for
    let r = roast(&url, t, &["roasted"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let lots = r.json["lots"].as_array().unwrap();
    assert_eq!(lots.len(), 2, "{lots:?}");
    assert!(lots.iter().all(|l| l["state"] == "resting"));
    // Resting, the sooner ready first: a light filter rests 7 days, a light espresso 10
    assert_eq!(lots[0]["session_id"], b.as_str());
    assert_eq!(
        (
            lots[0]["ready_in_days"].as_i64(),
            lots[1]["ready_in_days"].as_i64()
        ),
        (Some(7), Some(10))
    );
    let r = roast(&url, t, &["roasted", "--for", "espresso"]).await;
    let lots = r.json["lots"].as_array().unwrap();
    assert_eq!(lots.len(), 1);
    assert_eq!(lots[0]["session_id"], a.as_str());
    assert_eq!(r.json["groups"][0]["resting_g"], 86.0);
    let r = roast(&url, t, &["roasted", "--ready"]).await;
    assert_eq!(r.json["lots"], Value::Array(vec![]));

    // Two shots (18 g each), a gift by name, a count, a take-back
    let r = roast(&url, t, &["roasted", "use", &a, "2", "shots"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["left_g"], 50.0);
    // Bags packed, ground for espresso (docs/roasted.md, "Packed bags"): off what is left, not
    // use; more than is left refused; the trace lists them
    let r = roast(
        &url,
        t,
        &["roasted", "pack", &a, "2x10g", "--ground", "espresso"],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(
        (r.json["left_g"].as_f64(), r.json["packed_g"].as_f64()),
        (Some(30.0), Some(20.0))
    );
    let m = r.json["moves"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(
        (m["bags"].as_i64(), m["form"].as_str(), m["grind"].as_str()),
        (Some(2), Some("ground"), Some("espresso"))
    );
    let r = roast(&url, t, &["roasted", "pack", &a, "1x500g"]).await;
    assert_eq!((r.code, error_code(&r)), (6, "BAD_REQUEST"), "{:?}", r.json);
    let r = roast(&url, t, &["trace", &a]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["subject"], "roast");
    assert_eq!(r.json["roasts"][0]["packed_g"], 20.0, "{:?}", r.json);
    let r = roast(
        &url,
        t,
        &[
            "roasted", "give", "ethiopia", "50g", "--to", "Sam", "--note", "birthday",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["session_id"], b.as_str());
    assert_eq!(
        (r.json["left_g"].as_f64(), r.json["given_g"].as_f64()),
        (Some(35.0), Some(50.0))
    );
    assert_eq!(r.json["moves"][0]["recipient"], "Sam");
    let r = roast(&url, t, &["roasted", "use", &b, "100g"]).await;
    assert_eq!((r.code, error_code(&r)), (6, "BAD_REQUEST"), "{:?}", r.json);
    let r = roast(&url, t, &["roasted", "count", &b, "30g"]).await;
    assert_eq!(r.json["left_g"], 30.0);
    let r = roast(&url, t, &["roasted", "undo", &b]).await;
    assert_eq!(r.json["left_g"], 35.0);
    let r = roast(&url, t, &["roasted", "toss", &b]).await;
    assert_eq!(r.json["state"], "finished");
    let r = roast(&url, t, &["roasted", "show", &b]).await;
    let kinds: Vec<&str> = r.json["moves"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["given", "discarded"]);
    // Finished coffee drops off the list unless asked for
    let r = roast(&url, t, &["roasted"]).await;
    assert_eq!(r.json["lots"].as_array().unwrap().len(), 1);
    let r = roast(&url, t, &["roasted", "--all"]).await;
    assert_eq!(r.json["lots"].as_array().unwrap().len(), 2);

    // A read-only token reads but records nothing
    let r = roast(&url, Some(ro.as_str()), &["roasted"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let r = roast(&url, Some(ro.as_str()), &["roasted", "use", &a, "all"]).await;
    assert_eq!((r.code, error_code(&r)), (5, "TOKEN_SCOPE"), "{:?}", r.json);
}

/// Roasts in one bag (docs/roasted.md, "Roasts in one bag"): two roasts of one coffee combined,
/// a use on either split across them, the list showing the bag, undo, the bag's label data,
/// split apart; then a blend of two coffees, its shares and its share page.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_puts_two_roasts_in_one_bag() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let t = h.token("inventory").await;
    let t = Some(t.as_str());
    let bean = |name: &str, origin: &str| serde_json::json!({ "name": name, "origin": origin });
    let eth = h
        .owner("POST", "/api/beans", Some(bean("Senna Kata", "Ethiopia")))
        .await
        .json();
    let other = h
        .owner("POST", "/api/beans", Some(bean("Nyeri AA", "Kenya")))
        .await
        .json();
    let mut ids = Vec::new();
    for (name, g, b) in [("A", 97.0, &eth), ("B", 98.0, &eth), ("C", 90.0, &other)] {
        let id = finished_roast(&h, "virtual_roaster", name).await;
        h.owner(
            "PUT",
            &format!("/api/sessions/{id}"),
            Some(
                serde_json::json!({ "roasted_weight": g, "roast_purpose": "filter",
                                     "target_roast_level": "light", "bean_id": b["id"] }),
            ),
        )
        .await;
        ids.push(id);
    }
    let (a, b, c) = (&ids[0], &ids[1], &ids[2]);

    let r = roast(
        &url,
        t,
        &["roasted", "combine", a, b, "--name", "Senna jar"],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["name"], "Senna jar");
    assert_eq!(r.json["left_g"], 195.0);
    assert_eq!(r.json["members"].as_array().unwrap().len(), 2);

    // 30 g from the bag, recorded on either roast: split 97:98
    let r = roast(&url, t, &["roasted", "use", b, "30g"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["combined"]["left_g"], 165.0, "{:?}", r.json);
    let r = roast(&url, t, &["roasted", "combine", a]).await;
    assert_eq!(r.code, 0, "the bag: {:?}", r.json);
    let split: Vec<f64> = r.json["moves"][0]["split"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["delta_g"].as_f64().unwrap())
        .collect();
    assert_eq!(split, vec![-14.9, -15.1]);
    // The list: both roasts carry the bag, side by side
    let r = roast(&url, t, &["roasted"]).await;
    let lots = r.json["lots"].as_array().unwrap();
    let in_bag: Vec<&Value> = lots.iter().filter(|l| l["combined"].is_object()).collect();
    assert_eq!(in_bag.len(), 2, "{lots:?}");
    assert_eq!(in_bag[0]["combined"]["roasts"][0], a.as_str());
    // Its share page (what a bag's QR code opens) names both batches
    let link = h
        .owner(
            "POST",
            &format!("/api/sessions/{a}/share"),
            Some(serde_json::json!({})),
        )
        .await
        .json();
    let code = link["code"].as_str().expect("a share code").to_string();
    let page = h.text(&format!("/R/{code}")).await;
    assert!(
        page.contains("#1+2"),
        "the page names the bag's batches: {page}"
    );
    // Undo on the other roast takes the whole move back
    let r = roast(&url, t, &["roasted", "undo", a]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["combined"]["left_g"], 195.0, "{:?}", r.json);
    // The trace names the bag
    let r = roast(&url, t, &["trace", a]).await;
    assert_eq!(
        r.json["roasts"][0]["combined"]["name"], "Senna jar",
        "{:?}",
        r.json
    );
    // Split apart: separate again
    let r = roast(&url, t, &["roasted", "uncombine", b]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["roasts"].as_array().unwrap().len(), 2);
    let r = roast(&url, t, &["roasted", "show", a]).await;
    assert!(r.json["combined"].is_null(), "{:?}", r.json);

    // Another coffee in the bag: a blend, its coffees by share (97 g of the Senna, 90 g of the
    // Nyeri), the text naming them, the share page listing them
    let r = roast(&url, t, &["roasted", "combine", a, c]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["blend"], true, "{:?}", r.json);
    assert_eq!(r.json["name"], "Senna Kata + Nyeri AA #1+3");
    assert!(r.json["bean_id"].is_null());
    let shares: Vec<i64> = r.json["coffees"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["share_pct"].as_i64().unwrap())
        .collect();
    assert_eq!(shares, vec![52, 48]);
    let r = roast(&url, t, &["roasted", "show", c, "--text"]).await;
    let text = r.json.as_str().unwrap_or_default();
    assert!(
        text.contains("Blend Senna Kata + Nyeri AA #1+3 (52 % Senna Kata · 48 % Nyeri AA)"),
        "{text}"
    );
    let page = h.text(&format!("/R/{code}")).await;
    assert!(
        page.contains("52 % Ethiopia · 48 % Kenya") && page.contains("In this blend"),
        "the page names each origin: {page}"
    );
}
