//! PDF reports: two roasts compared, a roast's report as PDF, page and data.

use roastwarden_server::test_support::Harness;

use crate::common::{error_code, finished_roast, roast};

/// `roast report compare`: two roasts as an A/B experiment, the newest two by default
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_roasts_compared_as_a_report() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let a = finished_roast(&h, "virtual_roaster", "batch 3").await;
    // One finished roast: nothing to compare it with yet
    let r = roast(&url, None, &["report", "compare", "--format", "json"]).await;
    assert_eq!((r.code, error_code(&r)), (4, "NOT_FOUND"), "{:?}", r.json);
    let b = finished_roast(&h, "virtual_roaster", "batch 4").await;
    // None named: the newest two, the older as A
    let r = roast(&url, None, &["report", "compare", "--format", "json"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["schema"], "roastwarden.report.compare/1");
    assert_eq!(r.json["roasts"][0]["id"], a.as_str());
    assert_eq!(r.json["roasts"][1]["id"], b.as_str());
    assert_eq!(r.json["summary"][0]["value"], "6:16 → 6:16");
    // One named: it is B, against the roast before it
    let r = roast(
        &url,
        None,
        &[
            "report",
            "compare",
            "latest",
            "--roaster",
            "virtual_roaster",
            "--format",
            "json",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["roasts"][0]["id"], a.as_str());
    // Two named, B first: as asked; a PDF written here
    let dir = h.db_path().parent().unwrap().join("compare");
    std::fs::create_dir_all(&dir).unwrap();
    let pdf = dir.join("c.pdf").to_str().unwrap().to_string();
    let r = roast(&url, None, &["report", "compare", &b, &a, "-o", &pdf]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));
    // The same roast twice
    let r = roast(&url, None, &["report", "compare", &a, &a]).await;
    assert_eq!((r.code, error_code(&r)), (6, "BAD_REQUEST"), "{:?}", r.json);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_roasts_report_is_a_pdf_a_page_and_its_data() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let id = finished_roast(&h, "virtual_roaster", "batch 3").await;
    let dir = h.db_path().parent().unwrap().join("reports");
    std::fs::create_dir_all(&dir).unwrap();
    let path = |n: &str| dir.join(n).to_str().unwrap().to_string();

    // A read: no token needed
    let pdf = path("r.pdf");
    let r = roast(
        &url,
        None,
        &[
            "report",
            "roast",
            "latest",
            "--roaster",
            "virtual_roaster",
            "-o",
            &pdf,
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["file"], pdf.as_str());
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));
    // One page to look at
    let png = path("p1.png");
    let r = roast(
        &url,
        None,
        &[
            "report", "roast", &id, "--format", "png", "--dpi", "50", "-o", &png,
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert!(std::fs::read(&png).unwrap().starts_with(b"\x89PNG"));
    // The data, printed
    let r = roast(
        &url,
        None,
        &["report", "roast", &id, "--format", "json", "--units", "F"],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["schema"], "roastwarden.report.roast/1");
    assert_eq!(r.json["meta"]["temp"], "°F");
    assert_eq!(r.json["roast"]["name"], "batch 3");
    // FC at 381 s, drop at 611 s, from charge at 5 s
    assert_eq!(r.json["numbers"]["fc_s"], 376.0);
    assert_eq!(r.json["numbers"]["total_s"], 606.0);
    let r = roast(&url, None, &["report", "list"]).await;
    assert_eq!(r.json[0]["id"], "roast", "{:?}", r.json);
    // No such roast; a paper it does not know
    let r = roast(&url, None, &["report", "roast", "no-such-roast"]).await;
    assert_eq!((r.code, error_code(&r)), (4, "NOT_FOUND"), "{:?}", r.json);
    let r = roast(&url, None, &["report", "roast", &id, "--paper", "legal"]).await;
    assert_eq!((r.code, error_code(&r)), (6, "BAD_REQUEST"), "{:?}", r.json);

    // The cupping sheet: a blank form for the latest roast and one sample of the owner's own
    let r = roast(
        &url,
        None,
        &[
            "report",
            "cupping",
            "latest",
            "--roaster",
            "virtual_roaster",
            "--blank",
            "--samples",
            "1",
            "--format",
            "json",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["mode"], "blank");
    assert_eq!(r.json["samples"].as_array().unwrap().len(), 2);
    let key = r.json["key"].as_array().unwrap();
    assert!(key.iter().any(|k| k["label"] == "#1 · batch 3"), "{key:?}");
    let sheet = path("cupping.pdf");
    let r = roast(
        &url,
        None,
        &[
            "report",
            "cupping",
            &id,
            "--blank",
            "--per-page",
            "6",
            "-o",
            &sheet,
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert!(std::fs::read(&sheet).unwrap().starts_with(b"%PDF"));
    let r = roast(
        &url,
        None,
        &["report", "cupping", &id, "--blank", "--per-page", "4"],
    )
    .await;
    assert_eq!(r.code, 2, "2 or 6 a page");
    // Scores: none yet, then one roast cupped
    let r = roast(&url, None, &["report", "cupping", &id, "--format", "json"]).await;
    assert_eq!((r.code, error_code(&r)), (6, "BAD_REQUEST"), "{:?}", r.json);
    let t = h.token("roast").await;
    let r = roast(&url, Some(&t), &["cup", &id, "acidity=8", "--fault", "1"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let r = roast(&url, None, &["report", "cupping", "--format", "json"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["mode"], "filled");
    assert_eq!(r.json["rows"][0]["label"], "#1 · batch 3");
    // 8 + 6 × 6 + 10 × 3 = 74, less a fault's 4
    assert_eq!(r.json["rows"][0]["final"], "70.00");

    // Green inventory, the stock-take sheet, the shopping list
    let r = roast(&url, None, &["report", "inventory", "--format", "json"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["mode"], "report");
    let take = path("count.pdf");
    let r = roast(&url, None, &["report", "inventory", "--count", "-o", &take]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert!(std::fs::read(&take).unwrap().starts_with(b"%PDF"));
    let r = roast(&url, None, &["report", "shopping", "--format", "json"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["schema"], "roastwarden.report.shopping/1");

    // A coffee's and a profile's report, by name
    h.owner(
        "POST",
        "/api/beans",
        Some(serde_json::json!({ "name": "Kenya Nyeri AA", "stock_g": 500 })),
    )
    .await;
    let r = roast(
        &url,
        None,
        &["report", "coffee", "Kenya Nyeri AA", "--format", "json"],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["coffee"]["name"], "Kenya Nyeri AA");
    assert_eq!(r.json["summary"][0]["value"], "500 g");
    let r = roast(
        &url,
        None,
        &["report", "profile", "fluidbed-espresso", "--format", "json"],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["schema"], "roastwarden.report.profile/1");
    let r = roast(&url, None, &["report", "coffee", "No such coffee"]).await;
    assert_eq!(r.code, 4, "{:?}", r.json);

    // A period, purchases, the roast-day plan
    let r = roast(&url, None, &["report", "period", "--format", "json"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["summary"][0]["value"], "1", "this month's roast");
    let r = roast(&url, None, &["report", "period", "--month", "October"]).await;
    assert_eq!((r.code, error_code(&r)), (6, "BAD_REQUEST"), "{:?}", r.json);
    let r = roast(&url, None, &["report", "purchases", "--format", "json"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["schema"], "roastwarden.report.purchases/1");
    let sheet = path("plan.pdf");
    let r = roast(
        &url,
        None,
        &[
            "report",
            "plan",
            "--roaster",
            "virtual_roaster",
            "-o",
            &sheet,
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert!(std::fs::read(&sheet).unwrap().starts_with(b"%PDF"));
}
