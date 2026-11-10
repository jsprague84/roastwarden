//! Bag labels: a sheet, a design checked and a one-off, versions and a coffee's own design, a gift run reprinted, a label for sale.

use roastwarden_server::test_support::{Harness, OWNER_KEY};

use crate::common::{error_code, finished_roast, roast};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_makes_bag_labels() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let inv = h.token("inventory").await;
    let t = Some(inv.as_str());
    let id = finished_roast(&h, "esp32_roaster_01", "batch 9").await;
    let dir = h.db_path().parent().unwrap().join("out");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("l.pdf");

    // Reads: an inventory token (or none) may make labels
    let r = roast(
        &url,
        t,
        &[
            "labels",
            "latest",
            "--roaster",
            "esp32_roaster_01",
            "--count",
            "2",
            "--start",
            "3",
            "--weight",
            "250g",
            "-o",
            file.to_str().unwrap(),
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert!(std::fs::read(&file).unwrap().starts_with(b"%PDF"));
    let svg = dir.join("p.svg");
    let r = roast(
        &url,
        None,
        &[
            "labels",
            &id,
            "--design",
            "gift",
            "--message",
            "for Sam",
            "--sheet",
            "5294",
            "--svg",
            "-o",
            svg.to_str().unwrap(),
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert!(std::fs::read_to_string(&svg).unwrap().contains("<svg"));
    let r = roast(&url, None, &["labels", "--list"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let sheets: Vec<&str> = r.json["sheets"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["id"].as_str())
        .collect();
    assert_eq!(sheets, ["5163", "6874", "5294"]);
    assert!(
        r.json["designs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["id"] == "gift")
    );
    let cal = dir.join("c.pdf");
    let r = roast(
        &url,
        None,
        &[
            "labels",
            "--calibration",
            "--sheet",
            "6874",
            "-o",
            cal.to_str().unwrap(),
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);

    // What the owner asks for that is not there
    let r = roast(&url, None, &["labels", &id, "--sheet", "9999"]).await;
    assert_eq!((r.code, error_code(&r)), (6, "BAD_REQUEST"), "{:?}", r.json);
    let r = roast(&url, None, &["labels", &id, "--hide", "price"]).await;
    assert_eq!(r.code, 6, "{:?}", r.json);
    let r = roast(&url, None, &["labels", &id, "--start", "11"]).await;
    assert_eq!(r.code, 6, "{:?}", r.json);
    let r = roast(&url, None, &["labels"]).await;
    assert_eq!(r.code, 2, "a roast, --list or --calibration");

    // The owner's offset moves the print; settings check it
    let admin = h.token("admin").await;
    let r = roast(
        &url,
        Some(&admin),
        &[
            "settings",
            "set",
            r#"label_offsets={"5163":{"x_mm":0.8,"y_mm":-1.2}}"#,
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let r = roast(&url, None, &["labels", "--list"]).await;
    assert_eq!(r.json["sheets"][0]["offset"]["x_mm"], 0.8);
    let r = roast(
        &url,
        Some(&admin),
        &["settings", "set", r#"label_offsets={"5163":{"x_mm":80}}"#],
    )
    .await;
    assert_eq!(r.code, 6, "{:?}", r.json);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_checks_a_label_design_and_makes_a_one_off() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let id = finished_roast(&h, "esp32_roaster_01", "batch 10").await;
    let ro = h.token("read_only").await;
    let t = Some(ro.as_str());
    let dir = h.db_path().parent().unwrap().join("studio");
    std::fs::create_dir_all(&dir).unwrap();

    // A draft with a parameter and ASCII art from a file
    let draft = dir.join("cup.typ");
    std::fs::write(
        &draft,
        r##"#import "roastwarden.typ": fit, ascii
#let design = (name: "Cup", params: (art: (default: "", help: "ASCII art"), ink: "#000000"))
#let label(l, w, h, round) = box(width: w, height: h, inset: 0.15in, {
  set text(font: "Atkinson Hyperlegible Next", fill: rgb(l.params.ink))
  grid(columns: (auto, 1fr), column-gutter: 0.12in,
    ascii(l.params.art, size: 7pt),
    stack(spacing: 4pt, fit(l.name, w - 1.4in, max: 15pt, weight: "bold"), text(size: 9pt, l.roasted)))
})
"##,
    )
    .unwrap();
    let art = dir.join("cup.txt");
    std::fs::write(&art, "  ( (\n   ) )\n ......\n |    |]\n \\    /\n  `--'").unwrap();
    let png = dir.join("cup.png");
    let set_art = format!("art=@{}", art.display());
    let r = roast(
        &url,
        t,
        &[
            "labels",
            "design",
            "check",
            draft.to_str().unwrap(),
            "--roast",
            "latest",
            "--roaster",
            "esp32_roaster_01",
            "--set",
            &set_art,
            "--png",
            png.to_str().unwrap(),
            "--dpi",
            "72",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert_eq!(r.json["ok"], true);
    assert_eq!(r.json["data"], id);
    assert_eq!(r.json["report"]["manifest"]["name"], "Cup");
    assert!(
        r.json["report"]["lines"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["font"] == "JetBrains Mono NL")
    );
    assert!(std::fs::read(&png).unwrap().starts_with(b"\x89PNG"));
    assert_eq!(r.json["png"], png.display().to_string());

    // Issues: exit 1 with the report; a design that does not compile: exit 6 with the line
    let r = roast(
        &url,
        t,
        &["labels", "design", "check", "plain", "--set", "colour=red"],
    )
    .await;
    assert_eq!(r.code, 1, "{:?}", r.json);
    assert_eq!(r.json["report"]["issues"][0]["kind"], "param");
    let broken = dir.join("broken.typ");
    std::fs::write(
        &broken,
        "#let label(l, w, h, round) = {\n  text(l.nmae)\n}\n",
    )
    .unwrap();
    let r = roast(
        &url,
        t,
        &["labels", "design", "check", broken.to_str().unwrap()],
    )
    .await;
    assert_eq!(r.code, 6, "{:?}", r.json);
    assert_eq!(r.json["errors"][0]["line"], 2);
    let r = roast(&url, t, &["labels", "design", "check", "nope.typ"]).await;
    assert_eq!((r.code, error_code(&r)), (4, "NO_FILE"), "{:?}", r.json);

    // A one-off sheet from the draft, not saved
    let pdf = dir.join("one-off.pdf");
    let r = roast(
        &url,
        t,
        &[
            "labels",
            &id,
            "--source",
            draft.to_str().unwrap(),
            "--set",
            &set_art,
            "--count",
            "3",
            "-o",
            pdf.to_str().unwrap(),
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_saves_versions_and_gives_a_coffee_its_label() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let inv = h.token("inventory").await;
    let t = Some(inv.as_str());
    let dir = h.db_path().parent().unwrap().join("saved");
    std::fs::create_dir_all(&dir).unwrap();
    // A coffee and a roast of it
    let bean = h
        .owner(
            "POST",
            "/api/beans",
            Some(serde_json::json!({ "name": "Ethiopia Guji", "origin": "Ethiopia" })),
        )
        .await
        .json();
    let bean_id = bean["id"].as_str().unwrap().to_string();
    let roast_id = finished_roast(&h, "virtual_roaster", "guji 1").await;
    h.owner(
        "PUT",
        &format!("/api/sessions/{roast_id}"),
        Some(serde_json::json!({ "bean_id": bean_id })),
    )
    .await;

    // An asset, a brand, and a design that uses both
    let logo = dir.join("logo.svg");
    std::fs::write(&logo, r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40"><circle cx="20" cy="20" r="18" fill="none" stroke="black" stroke-width="3"/></svg>"#).unwrap();
    let r = roast(
        &url,
        t,
        &[
            "labels",
            "asset",
            "add",
            logo.to_str().unwrap(),
            "--note",
            "ours",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["kind"], "image");
    let r = roast(
        &url,
        t,
        &[
            "labels",
            "asset",
            "add",
            dir.join("nope.gif").to_str().unwrap(),
        ],
    )
    .await;
    assert_eq!(r.code, 4, "a file that is not there: {:?}", r.json);
    let r = roast(
        &url,
        t,
        &[
            "labels",
            "brand",
            "set",
            "home",
            "--name",
            "Hill Street Roasting",
            "--logo",
            "logo.svg",
            "--ink",
            "#2b2118",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let r = roast(
        &url,
        t,
        &["labels", "brand", "set", "home", "--tagline", "small batch"],
    )
    .await;
    assert_eq!(
        (r.json["name"].as_str(), r.json["tagline"].as_str()),
        (Some("Hill Street Roasting"), Some("small batch"))
    );
    let draft = dir.join("sam.typ");
    let v1 = r#"#import "roastwarden.typ": fit
#let design = (name: "Sam's gift", params: (to: (default: "Sam", help: "who it is for")))
#let label(l, w, h, round) = box(width: w, height: h, inset: 0.15in, stack(spacing: 4pt,
  if l.brand.logo != none { image(l.brand.logo, width: 0.4in) },
  fit("for " + l.params.to, w - 0.3in, max: 14pt),
  fit(l.name, w - 0.3in, max: 12pt),
  if l.brand.name != none { text(size: 8pt, fill: if l.brand.ink != none { rgb(l.brand.ink) } else { black }, l.brand.name) }))
"#;
    std::fs::write(&draft, v1).unwrap();
    let r = roast(
        &url,
        t,
        &[
            "labels",
            "design",
            "add",
            "sam-gift",
            draft.to_str().unwrap(),
            "--note",
            "first",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert_eq!(r.json["design"]["version"], 1);
    assert_eq!(r.json["design"]["name"], "Sam's gift", "from the manifest");
    assert_eq!(
        r.json["design"]["versions"][0]["created_by"],
        "test inventory"
    );
    // Not over a built-in, not twice, not when it does not compile
    let r = roast(
        &url,
        t,
        &["labels", "design", "add", "plain", draft.to_str().unwrap()],
    )
    .await;
    assert_eq!((r.code, error_code(&r)), (6, "CONFLICT"), "{:?}", r.json);
    let r = roast(
        &url,
        t,
        &[
            "labels",
            "design",
            "add",
            "sam-gift",
            draft.to_str().unwrap(),
        ],
    )
    .await;
    assert_eq!(r.code, 6, "{:?}", r.json);
    let broken = dir.join("broken.typ");
    std::fs::write(&broken, "#let label(l, w, h, round) = text(l.nmae)").unwrap();
    let r = roast(
        &url,
        t,
        &[
            "labels",
            "design",
            "edit",
            "sam-gift",
            broken.to_str().unwrap(),
        ],
    )
    .await;
    assert_eq!(r.code, 6, "{:?}", r.json);
    // Version 2, then back to 1 (as version 3)
    std::fs::write(&draft, v1.replace("max: 14pt", "max: 18pt")).unwrap();
    let r = roast(
        &url,
        t,
        &[
            "labels",
            "design",
            "edit",
            "sam-gift",
            draft.to_str().unwrap(),
            "--note",
            "bigger",
        ],
    )
    .await;
    assert_eq!(r.json["design"]["version"], 2, "{:?}", r.json);
    let r = roast(&url, t, &["labels", "design", "rollback", "sam-gift", "1"]).await;
    assert_eq!(r.json["design"]["version"], 3, "{:?}", r.json);
    let r = roast(&url, None, &["labels", "design", "show", "sam-gift@2"]).await;
    assert!(r.json["source"].as_str().unwrap().contains("max: 18pt"));
    let out = dir.join("back.typ");
    let r = roast(
        &url,
        None,
        &[
            "labels",
            "design",
            "show",
            "sam-gift",
            "-o",
            out.to_str().unwrap(),
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert!(std::fs::read_to_string(&out).unwrap().contains("max: 14pt"));
    let r = roast(&url, None, &["labels", "design", "list"]).await;
    let ids: Vec<&str> = r
        .json
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["id"].as_str())
        .collect();
    assert!(
        ids.contains(&"sam-gift") && ids.contains(&"cherry"),
        "{ids:?}"
    );

    // The coffee's own label: the roast's labels use it, with the brand
    let r = roast(
        &url,
        t,
        &[
            "labels",
            "design",
            "use",
            "sam-gift@2",
            "--bean",
            "Ethiopia Guji",
            "--set",
            "to=Alex",
            "--brand",
            "home",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let r = roast(
        &url,
        t,
        &[
            "labels", "design", "check", "sam-gift", "--roast", &roast_id,
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let text: String = r.json["report"]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|l| l["text"].as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(
        text.contains("Hill Street Roasting") && text.contains("Ethiopia Guji"),
        "{text}"
    );
    assert!(
        r.json["report"]["images"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["kind"] == "svg"),
        "the logo"
    );
    let pdf = dir.join("guji.pdf");
    let r = roast(&url, t, &["labels", &roast_id, "-o", pdf.to_str().unwrap()]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let bean = h
        .owner("GET", &format!("/api/beans/{bean_id}"), None)
        .await
        .json();
    assert_eq!(bean["label_design"], "sam-gift@2");
    assert_eq!(bean["label_params"]["to"], "Alex");

    // Deletes wait for the owner; a design a coffee uses cannot go
    let r = roast(&url, t, &["labels", "design", "rm", "sam-gift"]).await;
    assert_eq!(r.code, 3, "held for approval: {:?}", r.json);
    let r = h.owner("DELETE", "/api/label-designs/sam-gift", None).await;
    assert_eq!(r.status.as_u16(), 409, "{:?}", r.json());
    let r = roast(
        &url,
        t,
        &[
            "labels",
            "design",
            "use",
            "--bean",
            "Ethiopia Guji",
            "--clear",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let r = h.owner("DELETE", "/api/label-designs/sam-gift", None).await;
    assert_eq!(r.status.as_u16(), 204, "{:?}", r.json());
    // A read-only token reads but does not save
    let ro = h.token("read_only").await;
    let r = roast(
        &url,
        Some(&ro),
        &["labels", "design", "add", "x", draft.to_str().unwrap()],
    )
    .await;
    assert_eq!(r.code, 5, "{:?}", r.json);
    let r = roast(&url, Some(&ro), &["labels", "asset", "list"]).await;
    assert_eq!(r.json[0]["name"], "logo.svg");
}

/// Print runs from the CLI (docs/labels.md, "Print runs"): a gift run of two roasts on one
/// sheet with a message, written as a PDF; listed; part of it reprinted from another label;
/// marked printed by hand, which moves the sheet's next free label on.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_makes_a_gift_run_and_reprints_part_of_it() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let inv = h.token("inventory").await;
    let t = Some(inv.as_str());
    let a = finished_roast(&h, "esp32_roaster_01", "Guji").await;
    let b = finished_roast(&h, "esp32_roaster_01", "Palmichal").await;
    let dir = h.db_path().parent().unwrap().join("runs");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("gift.pdf");

    let items = format!(
        r#"[{{"roast":"{a}","count":2,"design":"gift","message":"Happy holidays"}},{{"roast":"{b}"}}]"#
    );
    let r = roast(
        &url,
        t,
        &[
            "labels",
            "run",
            "--items",
            &items,
            "--note",
            "gift baskets",
            "-o",
            file.to_str().unwrap(),
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert!(std::fs::read(&file).unwrap().starts_with(b"%PDF"));
    assert_eq!(r.json["labels"], 3);
    assert_eq!(r.json["start"], 1);
    let first = r.json["run"].as_str().unwrap().to_string();

    // The same with --roast: one design and message for each
    let r = roast(
        &url,
        t,
        &[
            "labels",
            "run",
            "--roast",
            &a,
            "--roast",
            &b,
            "--message",
            "for Sam",
            "--count",
            "2",
            "--start",
            "4",
            "-o",
            dir.join("two.pdf").to_str().unwrap(),
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert_eq!(r.json["labels"], 4);
    assert_eq!(r.json["start"], 4);
    let two = r.json["run"].clone();

    // The same again before it is printed: that run, not a second copy
    let again_pdf = dir.join("two-again.pdf");
    let again = [
        "labels",
        "run",
        "--roast",
        &a,
        "--roast",
        &b,
        "--message",
        "for Sam",
        "--count",
        "2",
        "--start",
        "4",
        "-o",
        again_pdf.to_str().unwrap(),
    ];
    let r = roast(&url, t, &again).await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert_eq!(r.json["run"], two);
    assert_eq!(r.json["reused"], true);

    let r = roast(&url, t, &["labels", "runs"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json.as_array().unwrap().len(), 2);
    let r = roast(&url, t, &["labels", "runs", "show", &first]).await;
    assert_eq!(r.json["items"][0]["design"], "gift");
    assert_eq!(r.json["items"][0]["data"]["message"], "Happy holidays");
    assert_eq!(r.json["note"], "gift baskets");

    // Labels 2 and 3 of the first run again, from label 7
    let r = roast(
        &url,
        t,
        &[
            "labels",
            "reprint",
            &first,
            "--labels",
            "2,3",
            "--start",
            "7",
            "-o",
            dir.join("again.pdf").to_str().unwrap(),
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert_eq!(r.json["labels"], 2);
    assert_eq!(r.json["start"], 7);

    // Printed by hand: the sheet's next free label is 9
    let r = roast(&url, t, &["labels", "runs", "done", "latest"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["state"], "printed");
    let r = roast(&url, t, &["labels", "sheets"]).await;
    let s5163 = r
        .json
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["sheet"] == "5163")
        .unwrap()
        .clone();
    assert_eq!(s5163["next_label"], 9);
    let r = roast(&url, t, &["labels", "sheets", "--set", "5163=1"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);

    // No printer: --print is refused and says why (a conflict, exit 6)
    let r = roast(&url, t, &["labels", "runs", "print", &first]).await;
    assert_eq!(
        r.code, 3,
        "a token's print waits for the owner first: {:?}",
        r.json
    );
    let r = roast(&url, Some(OWNER_KEY), &["labels", "runs", "print", &first]).await;
    assert_eq!(r.code, 6, "{:?}", r.json);
    assert!(
        r.json.to_string().contains("no printer is set"),
        "{:?}",
        r.json
    );
}

/// The retail check from the CLI (docs/labels.md, "The retail check"): the shop design with
/// the roastery's brand passes for a 12 oz pouch; the plain design does not, and says what
/// to fix; `--diff` says what changed against another design.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_checks_a_label_for_sale() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let inv = h.token("inventory").await;
    let t = Some(inv.as_str());
    let r = roast(
        &url,
        t,
        &[
            "labels",
            "brand",
            "set",
            "hill",
            "--name",
            "Hill Street Roasting",
            "--place",
            "Portland, OR 97201",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let r = roast(
        &url,
        t,
        &[
            "labels",
            "design",
            "check",
            "shop",
            "--sheet",
            "6874",
            "--brand",
            "hill",
            "--retail",
            "pouch-12oz",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json["retail"]);
    assert_eq!(r.json["retail"]["ok"], true);
    assert_eq!(r.json["retail"]["min_net_mm"], 4.8);
    assert_ne!(
        r.json["retail"]["cannot_check"]
            .as_array()
            .unwrap()
            .as_slice(),
        &[] as &[serde_json::Value]
    );

    let r = roast(
        &url,
        t,
        &[
            "labels",
            "design",
            "check",
            "plain",
            "--brand",
            "hill",
            "--retail",
            "pouch-12oz",
            "--diff",
            "shop",
        ],
    )
    .await;
    assert_eq!(r.code, 1, "issues to fix: {:?}", r.json);
    let kinds: Vec<&str> = r.json["report"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i["kind"].as_str())
        .collect();
    assert!(kinds.contains(&"retail_identity"), "{kinds:?}");
    assert!(kinds.contains(&"retail_net"), "{kinds:?}");
    assert_eq!(r.json["diff"]["against"], "shop");
    assert!(
        !r.json["diff"]["lines_gone"].as_array().unwrap().is_empty(),
        "shop's lines are not in plain: {:?}",
        r.json["diff"]
    );
}
