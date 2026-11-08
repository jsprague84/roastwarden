use std::collections::BTreeMap;

use serde_bytes::ByteBuf;
use serde_json::{json, Value};

use super::designs::*;
use super::facts::*;
use super::notes::*;
use super::sheets::*;
use super::*;

fn facts() -> Facts {
    Facts {
        bean_id: None,
        name: "Costa Rica Palmichal".into(),
        origin: Some("Costa Rica".into()),
        region: Some("Tarrazú".into()),
        producer: None,
        variety: Some("Caturra, Catuaí".into()),
        process: Some("washed".into()),
        level: Some("light".into()),
        purpose: Some("espresso".into()),
        // 23:50 UTC on 25 Oct is 25 Oct in Chicago (UTC−5), 26 Oct in Berlin
        roasted_at: Some("2026-10-25T23:50:00Z".parse().unwrap()),
        batch: Some(5),
        roasted_g: Some(85.7),
        notes: Some("Red apple, panela, milk chocolate".into()),
        roaster: None,
        url: Some("https://roastwarden.example/history/abc".into()),
        short_name: None,
        brew: share::brew_for(Some("espresso"), Some("light"), 18.0, 15.0),
        lot: None,
    }
}

const REST: RestDays = RestDays {
    espresso_light: 10,
    espresso: 7,
    filter_light: 7,
    filter: 4,
};

#[test]
fn a_label_reads_dates_in_the_owners_time_and_rests_by_purpose() {
    let ask = Ask {
        zone: crate::ops::zone::Zone::minutes(-300),
        ..Ask::default()
    };
    let l = label_data(&facts(), &ask, &REST);
    assert_eq!(l["roasted"], "25 Oct 2026");
    assert_eq!(l["best_from"], "4 Nov 2026", "light espresso: 10 days");
    assert_eq!(l["roasted_iso"], "2026-10-25");
    assert_eq!(l["best_from_iso"], "2026-11-04");
    assert_eq!(l["rest_days"], 10);
    assert_eq!(l["origin"], "Tarrazú, Costa Rica");
    // Rounded down like the net weight: never more than is in the bag
    assert_eq!(l["weight"], "85 g · 3.0 oz");
    assert_eq!(l["net"], "Net wt 3 oz (85 g)");
    assert_eq!(l["level"], "Light");
    assert_eq!(l["purpose"], "Espresso");
    assert_eq!(l["brew"]["out_g"], 45.0, "light espresso: 1:2.5");
    assert_eq!(l["qr_to"], "app");
    // Whole bean by default, at its best for 35 days (light)
    assert_eq!(l["form"], "Whole bean");
    assert_eq!(l["ground"], false);
    assert_eq!(l["best_until_iso"], "2026-11-29");
    // Ground for espresso, packed on 1 Nov: 14 days from then, before the lot's 29 Nov
    let ground = Ask {
        zone: crate::ops::zone::Zone::minutes(-300),
        form: crate::coffee::roasted::Form::parse(None, Some("espresso")).unwrap(),
        packed_on: chrono::NaiveDate::from_ymd_opt(2026, 11, 1),
        ..Ask::default()
    };
    let g = label_data(&facts(), &ground, &REST);
    assert_eq!(g["form"], "Ground · espresso");
    assert_eq!(g["grind"], "espresso");
    assert_eq!(g["best_until"], "15 Nov 2026");
    assert_eq!(g["best_from"], "4 Nov 2026", "best from is unchanged");
    // Packed late, the lot's own date is the earlier
    let late = Ask {
        packed_on: chrono::NaiveDate::from_ymd_opt(2026, 11, 25),
        ..ground.clone()
    };
    assert_eq!(
        label_data(&facts(), &late, &REST)["best_until_iso"],
        "2026-11-29"
    );
    let berlin = Ask {
        zone: crate::ops::zone::Zone::minutes(60),
        ..Ask::default()
    };
    assert_eq!(
        label_data(&facts(), &berlin, &REST)["roasted"],
        "26 Oct 2026"
    );
    // By name (the `time_zone` setting): 23:50 UTC is 18:50 CDT
    let chicago = Ask {
        zone: crate::ops::zone::Zone::named("America/Chicago").unwrap(),
        ..Ask::default()
    };
    assert_eq!(
        label_data(&facts(), &chicago, &REST)["roasted"],
        "25 Oct 2026"
    );
    // No zone at all is UTC
    assert_eq!(
        label_data(&facts(), &Ask::default(), &REST)["roasted"],
        "25 Oct 2026"
    );
    assert_eq!(REST.for_roast(Some("filter"), Some("medium")), 4);
    assert_eq!(REST.for_roast(Some("omni"), Some("medium_light")), 7);
    assert_eq!(REST.for_roast(Some("espresso"), None), 10);
}

/// The bean's label notes win, a supplier's prose is cut at a sentence, and
/// tasting words are suggested from it; a short name prints as the name
#[test]
fn a_bag_of_several_roasts_prints_its_batches_and_days() {
    let mut f = facts();
    f.lot = Some(LotFacts {
        batches: "16+17".into(),
        first_at: "2026-10-30T23:50:00Z".parse().unwrap(),
        last_at: "2026-11-02T02:10:00Z".parse().unwrap(),
        roasted_g: 194.0,
        name: None,
        coffees: vec![],
        parts: vec![],
    });
    let l = label_data(&f, &Ask::default(), &REST);
    assert_eq!(l["batch"], "16+17");
    assert_eq!(l["roasted"], "30 Oct – 2 Nov 2026");
    assert_eq!(l["roasted_iso"], "2026-10-30");
    assert_eq!(l["roasted_last_iso"], "2026-11-02");
    // Ready when the newest is (light espresso, 10 days); at its best until the oldest's 35
    assert_eq!(l["best_from"], "12 Nov 2026");
    assert_eq!(l["best_until"], "4 Dec 2026");
    assert_eq!(
        range_text(
            chrono::NaiveDate::from_ymd_opt(2026, 11, 6).unwrap(),
            chrono::NaiveDate::from_ymd_opt(2026, 11, 7).unwrap()
        ),
        "6–7 Nov 2026"
    );
    // One roast: the number as before
    assert_eq!(label_data(&facts(), &Ask::default(), &REST)["batch"], 5);
}

#[test]
fn label_notes_short_names_and_the_suppliers_prose() {
    let prose = "Impressive Limu natural that's quite versatile. Attractive sweetness plays \
            off modest fruit and berry tones, with an aromatic finish. Body and bittersweet \
            notes build as it cools, with cocoa and a hint of dried fig in the cup.";
    assert_eq!(
        notes_for(Some("Cocoa · fig"), Some("bright"), Some(prose)).as_deref(),
        Some("Cocoa · fig")
    );
    assert_eq!(
        notes_for(None, Some("bright"), Some(prose)).as_deref(),
        Some("bright")
    );
    assert_eq!(
        notes_for(Some(" "), None, Some(prose)).as_deref(),
        Some(
            "Impressive Limu natural that's quite versatile. Attractive sweetness plays off \
                 modest fruit and berry tones, with an aromatic finish."
        )
    );
    // One long sentence: cut after a clause, never mid-word
    let one = "Sweet and juicy with layered notes of red grape, black tea, cocoa nib, brown \
            sugar and a dried apricot finish that lingers, a heavy body, and a soft acidity like \
            red apple through the cup as it cools down";
    let cut = prose_notes(one);
    assert!(cut.ends_with('…') && cut.chars().count() <= 141, "{cut}");
    assert!(!cut.contains(", …"), "{cut}");
    assert_eq!(prose_notes("Short and sweet."), "Short and sweet.");

    assert_eq!(
        suggest_notes(prose).as_deref(),
        Some("Berry · cocoa · dried fig")
    );
    assert_eq!(
        suggest_notes("Milk chocolate, red apple and panela; a jasmine finish").as_deref(),
        Some("Milk chocolate · red apple · panela · jasmine")
    );
    assert_eq!(suggest_notes("A nice coffee from a good farm"), None);

    let mut f = facts();
    f.short_name = Some("Palmichal".into());
    let l = label_data(&f, &Ask::default(), &REST);
    assert_eq!(l["name"], "Palmichal");
    assert_eq!(l["full_name"], "Costa Rica Palmichal");
}

/// One brew recipe for the share page and the labels (`l.brew`)
#[test]
fn the_brew_recipe_follows_purpose_level_and_the_owners_doses() {
    let b = share::brew_for(Some("espresso"), Some("light"), 18.0, 15.0).unwrap();
    assert_eq!((b.dose_g, b.out_g, b.ratio.as_str()), (18.0, 45.0, "1:2.5"));
    assert_eq!(b.temp_c, 94);
    assert!(
        b.line.starts_with("18 g in, about 45 g out (1:2.5)"),
        "{}",
        b.line
    );
    let m = share::brew_for(Some("espresso"), Some("medium"), 18.0, 15.0).unwrap();
    assert_eq!((m.out_g, m.ratio.as_str()), (40.0, "1:2.2"));
    let d = share::brew_for(Some("espresso"), Some("medium_dark"), 20.0, 15.0).unwrap();
    assert_eq!((d.out_g, d.ratio.as_str()), (40.0, "1:2"));
    let f = share::brew_for(Some("filter"), Some("light"), 18.0, 15.0).unwrap();
    assert_eq!((f.dose_g, f.out_g, f.temp_c), (15.0, 240.0, 96));
    assert_eq!(f.method, "Filter");
    assert!(share::brew_for(Some("omni"), None, 18.0, 15.0).is_none());
}

#[test]
fn the_net_weight_never_claims_more_than_the_bag_holds() {
    assert_eq!(net_weight(250.0), "Net wt 8.8 oz (250 g)");
    assert_eq!(
        net_weight(340.0),
        "Net wt 11.9 oz (340 g)",
        "11.99 oz is not 12"
    );
    assert_eq!(net_weight(113.4), "Net wt 4 oz (113 g)");
    assert_eq!(net_weight(453.6), "Net wt 16 oz (1 lb) 453 g");
    assert_eq!(net_weight(680.4), "Net wt 24 oz (1 lb 8 oz) 680 g");
    assert_eq!(net_weight(2268.0), "Net wt 5 lb (2.26 kg)");
    let hidden = label_data(
        &facts(),
        &Ask {
            weight_g: Some(250.0),
            hide: vec!["weight".into()],
            ..Ask::default()
        },
        &REST,
    );
    assert!(hidden["weight"].is_null() && hidden["net"].is_null() && hidden["weight_g"].is_null());
}

#[test]
fn fields_are_hidden_and_the_owner_overrides_weight_and_notes() {
    let ask = Ask {
        weight_g: Some(250.0),
        notes: Some("Bright".into()),
        message: Some("for Sam".into()),
        hide: vec!["batch".into(), "qr".into()],
        zone: crate::ops::zone::Zone::default(),
        ..Ask::default()
    };
    let l = label_data(&facts(), &ask, &REST);
    assert_eq!(l["weight"], "250 g · 8.8 oz");
    assert_eq!(l["notes"], "Bright");
    assert_eq!(l["message"], "for Sam");
    assert_eq!(l["batch"], Value::Null);
    assert_eq!(l["qr"], false);
}

fn sheet(id: &str) -> Sheet {
    builtin_sheets().into_iter().find(|s| s.id == id).unwrap()
}

fn job(design: Option<&str>, sheet_id: &str, count: usize, start: usize) -> Job {
    let l = label_data(&facts(), &Ask::default(), &REST);
    let mut j = Job::new(
        sheet(sheet_id),
        Offset {
            x_mm: 0.5,
            y_mm: -0.5,
        },
        design.map(|d| builtin_design(d).unwrap()),
    );
    j.slots = slots(l, count, start);
    j.url = facts().url;
    j
}

fn bare() -> Facts {
    Facts {
        bean_id: None,
        name: "X".into(),
        origin: None,
        region: None,
        producer: None,
        variety: None,
        process: None,
        level: None,
        purpose: None,
        roasted_at: None,
        batch: None,
        roasted_g: None,
        notes: None,
        roaster: None,
        url: None,
        short_name: None,
        brew: None,
        lot: None,
    }
}

/// Every built-in design on every built-in sheet, with every field and with none
#[tokio::test]
async fn every_design_renders_on_every_sheet() {
    let r = Renderer::in_process();
    for d in BUILTIN_DESIGNS.map(|d| d.0) {
        for s in ["5163", "6874", "5294"] {
            let pdf = render(&r, job(Some(d), s, 3, 2), Output::Pdf)
                .await
                .unwrap_or_else(|e| panic!("{d} on {s}: {e}"));
            assert!(pdf.starts_with(b"%PDF"), "{d} on {s}");
            // RR_LABELS_OUT=dir writes them, to look at
            if let Ok(dir) = std::env::var("RR_LABELS_OUT") {
                std::fs::write(format!("{dir}/{d}-{s}.pdf"), &pdf).unwrap();
                let mut gift = job(Some(d), s, 2, 1);
                let mut l = gift.slots[0].clone().unwrap();
                l["message"] = json!("for Sam, with love");
                gift.slots = slots(l, 2, 1);
                gift.outline = true;
                let svg = render(&r, gift, Output::Svg).await.unwrap();
                std::fs::write(format!("{dir}/{d}-{s}-message.svg"), &svg).unwrap();
            }
            let mut b = job(Some(d), s, 1, 1);
            b.slots = slots(label_data(&bare(), &Ask::default(), &REST), 1, 1);
            b.url = None;
            render(&r, b, Output::Svg)
                .await
                .unwrap_or_else(|e| panic!("{d} on {s}, nothing known: {e}"));
        }
    }
}

#[tokio::test]
async fn one_sheet_holds_several_designs_each_with_its_own_qr_code() {
    let r = Renderer::in_process();
    let mut j = job(Some("plain"), "5163", 1, 1);
    let mut a = j.slots[0].clone().unwrap();
    a["name"] = json!("Alpha Natural");
    let mut b = a.clone();
    b["name"] = json!("Beta Washed");
    b["message"] = json!("for Sam");
    b["group"] = json!(1);
    j.url = Some("https://example.test/history/alpha".into());
    j.more = vec![Group {
        design: builtin_design("gift").unwrap(),
        params: serde_json::Map::new(),
        brand: store::Brand::value(None),
        url: Some("https://example.test/history/beta".into()),
    }];
    // Label 1 blank, then Alpha, Beta, Beta
    j.slots = vec![None, Some(a), Some(b.clone()), Some(b)];
    let reply = r
        .render(
            j.request(engine::Kind::Sheet, engine::Output::Report)
                .unwrap(),
        )
        .await;
    let engine::Reply::Done {
        report: Some(rep), ..
    } = reply
    else {
        panic!("{reply:?}")
    };
    let text: Vec<&str> = rep.lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(
        text.iter().filter(|t| t.contains("Alpha Natural")).count(),
        1,
        "{text:?}"
    );
    assert_eq!(
        text.iter().filter(|t| t.contains("Beta Washed")).count(),
        2,
        "{text:?}"
    );
    assert!(
        text.iter().any(|t| t.contains("for Sam")),
        "gift's message: {text:?}"
    );
    // Plain draws Alpha's QR code; gift draws none on a 4 × 2 label
    let qr: Vec<_> = rep.images.iter().filter(|i| i.kind == "qr").collect();
    assert_eq!(qr.len(), 1, "{:?}", rep.images);
    // Beta's own QR code reaches its design: plain in group 1 draws it
    let mut k = job(Some("gift"), "5163", 1, 1);
    let mut c = k.slots[0].clone().unwrap();
    c["group"] = json!(1);
    k.more = vec![Group {
        design: builtin_design("plain").unwrap(),
        params: serde_json::Map::new(),
        brand: store::Brand::value(None),
        url: Some("https://example.test/history/gamma".into()),
    }];
    k.url = None;
    k.slots = vec![Some(c)];
    let pdf = render(&r, k, Output::Pdf).await.unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}

#[tokio::test]
async fn copies_past_a_sheet_make_more_pages_and_the_calibration_page_renders() {
    let r = Renderer::in_process();
    let one = render(&r, job(Some("plain"), "5163", 10, 1), Output::Pdf)
        .await
        .unwrap();
    let two = render(&r, job(Some("plain"), "5163", 10, 4), Output::Pdf)
        .await
        .unwrap();
    assert!(two.len() > one.len());
    let cal = render(&r, job(None, "5294", 0, 1), Output::Svg)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&cal).contains("<svg"));
    // RR_LABELS_OUT=dir writes each sheet's calibration page, to look at
    if let Ok(dir) = std::env::var("RR_LABELS_OUT") {
        for s in ["5163", "6874", "5294"] {
            let svg = render(&r, job(None, s, 0, 1), Output::Svg).await.unwrap();
            std::fs::write(format!("{dir}/calibration-{s}.svg"), &svg).unwrap();
        }
    }
}

#[tokio::test]
async fn a_broken_design_says_where() {
    let dir = std::env::temp_dir().join(format!("rr-labels-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(dir.join("designs")).unwrap();
    std::fs::write(
        dir.join("designs/mine.typ"),
        "#let label(l, w, h, round) = {\n  text(l.nmae)\n}\n",
    )
    .unwrap();
    let folder = Folder(dir.clone());
    assert!(folder.list().iter().any(|d| d.id == "mine" && !d.builtin));
    let mut j = job(Some("plain"), "5163", 1, 1);
    j.design = folder.design("mine");
    match render(&Renderer::in_process(), j, Output::Pdf).await {
        Err(LabelError::Design(m)) => assert!(m.contains("design.typ:2"), "{m}"),
        other => panic!("{:?}", other.map(|b| b.len())),
    }
    assert!(folder.design("../etc").is_none());
    std::fs::remove_dir_all(dir).ok();
}

fn one_off(src: &str, sheet_id: &str) -> Job {
    let mut j = job(Some("plain"), sheet_id, 1, 1);
    j.design = Some(DesignSource {
        id: "test".into(),
        version: None,
        source: src.into(),
        files: BTreeMap::new(),
    });
    j
}

/// The built-in designs pass their own check on the sheets they are for
#[tokio::test]
async fn the_built_in_designs_check_clean() {
    let r = Renderer::in_process();
    for d in BUILTIN_DESIGNS.map(|d| d.0) {
        for s in ["5163", "6874", "5294"] {
            for message in [None, Some("for Sam, with love")] {
                let mut j = job(Some(d), s, 1, 1);
                if let Some(m) = message {
                    let mut l = j.slots[0].clone().unwrap();
                    l["message"] = json!(m);
                    j.slots = slots(l, 1, 1);
                }
                let c = check(&r, j, None).await.unwrap();
                let rep = c
                    .report
                    .unwrap_or_else(|| panic!("{d} on {s}: {:?}", c.errors));
                // Shop with no brand and no roaster name says so (a soft hint)
                let shown: Vec<_> = rep
                    .issues
                    .iter()
                    .filter(|i| {
                        !(d == "shop"
                            && (i.kind == "hint" || (message.is_some() && i.kind == "dropped")))
                    })
                    .collect();
                assert!(
                    shown.is_empty() && rep.ok,
                    "{d} on {s} ({message:?}): {:?}",
                    rep.issues
                );
                if d == "shop" {
                    assert!(rep.issues.iter().any(|i| i.kind == "hint"), "{s}");
                }
                assert!(rep.fields.shown.contains(&"name".to_string()), "{d} on {s}");
            }
        }
    }
}

/// A ground bag: plain prints its form line with its best-until, shop says
/// "Ground coffee"; both check clean on a rectangle, a big label and a round one
#[tokio::test]
async fn a_ground_bag_prints_its_form_and_checks_clean() {
    let r = Renderer::in_process();
    for d in ["plain", "shop"] {
        for s in ["5163", "6874", "5294"] {
            let mut j = job(Some(d), s, 1, 1);
            let mut l = j.slots[0].clone().unwrap();
            l["form"] = json!("Ground · French press");
            l["ground"] = json!(true);
            l["grind"] = json!("French press");
            l["best_until"] = json!("8 Nov 2026");
            j.slots = slots(l, 1, 1);
            let c = check(&r, j, None).await.unwrap();
            let rep = c
                .report
                .unwrap_or_else(|| panic!("{d} on {s}: {:?}", c.errors));
            let shown: Vec<_> = rep
                .issues
                .iter()
                .filter(|i| !(d == "shop" && i.kind == "hint"))
                .collect();
            assert!(shown.is_empty() && rep.ok, "{d} on {s}: {:?}", rep.issues);
            let text = rep
                .lines
                .iter()
                .map(|l| l.text.as_str())
                .collect::<Vec<_>>()
                .join(" | ");
            assert!(text.contains("best until 8 Nov 2026"), "{d} on {s}: {text}");
            assert!(text.contains("Ground · French press"), "{d} on {s}: {text}");
            if d == "shop" {
                assert!(text.contains("Ground coffee"), "{s}: {text}");
            }
        }
    }
}

/// Long names, a 52-character gift message and long notes: every
/// built-in design keeps them on the label, shrinking and wrapping the name and the
/// message, and fills the room left with notes. `RR_LABELS_OUT=dir` writes the PNGs.
#[tokio::test]
async fn long_names_and_messages_wrap_inside_every_design() {
    let r = Renderer::in_process();
    let out = std::env::var("RR_LABELS_OUT").ok();
    let long = [
        (
            "Ethiopia Dry Process Senna Kata Mountain",
            Some("For Sam & Alex, happy housewarming! Enjoy it slowly."),
        ),
        ("Kenya Nyeri Gatugi AA", None),
    ];
    let notes = "Impressive Limu natural that's quite versatile. Attractive sweetness plays off \
            modest fruit and berry tones, with an aromatic finish. Body and bittersweet notes \
            build as it cools, with cocoa and a hint of dried fig in the cup.";
    let mut failed = Vec::new();
    for d in BUILTIN_DESIGNS.map(|d| d.0) {
        for s in ["5163", "6874", "5294"] {
            for (i, (name, message)) in long.iter().enumerate() {
                let mut j = job(Some(d), s, 1, 1);
                let mut l = j.slots[0].clone().unwrap();
                l["name"] = json!(name);
                l["origin"] = json!("Limmu Kossa, Jimma, Ethiopia · Hambela Estate");
                l["notes"] = json!(notes);
                l["message"] = json!(message);
                j.slots = slots(l, 1, 1);
                let c = check(&r, j, out.as_ref().map(|_| 100)).await.unwrap();
                let rep = c
                    .report
                    .unwrap_or_else(|| panic!("{d} on {s}: {:?}", c.errors));
                if let (Some(dir), Some(png)) = (&out, &c.png) {
                    std::fs::write(format!("{dir}/long-{d}-{s}-{i}.png"), png).unwrap();
                }
                let bad: Vec<_> = rep
                    .issues
                    .iter()
                    .filter(|i| {
                        !(i.kind == "cut"
                            || i.kind == "hint"
                            || (d == "shop" && i.kind == "dropped"))
                    })
                    .collect();
                if !bad.is_empty() {
                    failed.push(format!("{d} on {s} ({name}): {bad:?}"));
                }
                for f in ["name", "message"] {
                    if rep.fields.missing.contains(&f.to_string()) {
                        failed.push(format!("{d} on {s}: {f} missing"));
                    }
                }
            }
        }
    }
    assert!(
        failed.is_empty(),
        "{}",
        failed.join(
            "
"
        )
    );
}

/// A design that prints the fields its own way: a slugged name, the day and
/// month only, an ISO date, one part of the weight. The check finds them all.
#[tokio::test]
async fn the_check_finds_fields_printed_their_own_way() {
    let src = r#"
#let label(l, w, h, round) = { set text(size: 8pt); stack(dir: ttb, spacing: 3pt,
  lower(l.name.replace(" ", "-")), upper(l.roasted.slice(0, 6)), l.best_from_iso,
  l.weight.split(" · ").at(1), str(l.rest_days) + " days") }
"#;
    let r = Renderer::in_process();
    let rep = check(&r, one_off(src, "5163"), None)
        .await
        .unwrap()
        .report
        .unwrap();
    for f in ["name", "roasted", "best_from", "weight"] {
        assert!(
            rep.fields.shown.contains(&f.to_string()),
            "{f}: {:?}",
            rep.fields
        );
    }
    assert!(rep.fields.missing.contains(&"notes".to_string()));
    assert!(rep.fields.missing.contains(&"batch".to_string()));
}

#[tokio::test]
async fn the_shop_design_checks_clean_with_a_whole_brand() {
    let r = Renderer::in_process();
    let brand = store::Brand {
        name: "Hill Street Roasting".into(),
        tagline: Some("Small batches, roasted at home".into()),
        place: Some("Portland, OR".into()),
        logo: Some("logo.svg".into()),
        ink: Some("#2b2118".into()),
        accent: Some("#8a3b12".into()),
        font_display: Some("Cormorant Garamond".into()),
        ..store::Brand::default()
    };
    let logo = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><circle cx="5" cy="5" r="4"/></svg>"#;
    for (s, message) in ["5163", "6874", "5294"]
        .into_iter()
        .flat_map(|s| [(s, None), (s, Some("for Sam, with love"))])
    {
        let mut j = job(Some("shop"), s, 1, 1);
        j.brand = store::Brand::value(Some(&brand));
        if let Some(m) = message {
            let mut l = j.slots[0].clone().unwrap();
            l["message"] = json!(m);
            j.slots = slots(l, 1, 1);
        }
        if let Some(d) = j.design.as_mut() {
            d.files
                .insert("logo.svg".into(), ByteBuf::from(logo.to_vec()));
        }
        let c = check(&r, j, None).await.unwrap();
        let rep = c
            .report
            .unwrap_or_else(|| panic!("shop on {s}: {:?}", c.errors));
        // On a short label a message takes the dates' room (they give way on purpose)
        assert!(
            rep.issues
                .iter()
                .all(|i| message.is_some() && i.kind == "dropped"),
            "shop on {s} ({message:?}): {:?}",
            rep.issues
        );
        if message.is_some() {
            assert!(
                rep.fields.shown.contains(&"message".to_string()),
                "a gift message is kept before the details on {s}: {:?}",
                rep.fields
            );
        }
        let text: String = rep
            .lines
            .iter()
            .map(|l| l.text.clone())
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(
            text.to_lowercase().contains("hill street roasting"),
            "{s}: {text}"
        );
        assert!(
            text.contains("Whole bean coffee") && text.contains("Net wt"),
            "{s}: {text}"
        );
        if let Ok(dir) = std::env::var("RR_LABELS_OUT") {
            let mut j = job(Some("shop"), s, 1, 1);
            j.brand = store::Brand::value(Some(&brand));
            if let Some(d) = j.design.as_mut() {
                d.files
                    .insert("logo.svg".into(), ByteBuf::from(logo.to_vec()));
            }
            let png = check(&r, j, Some(150)).await.unwrap().png.unwrap();
            std::fs::write(format!("{dir}/shop-{s}-brand.png"), png).unwrap();
        }
    }
}

#[tokio::test]
async fn the_shop_design_meets_the_retail_check_for_a_12_oz_pouch() {
    let r = Renderer::in_process();
    let brand = store::Brand {
        name: "Hill Street Roasting".into(),
        place: Some("Portland, OR 97201".into()),
        ..store::Brand::default()
    };
    let pouch = retail::builtin_bags()
        .into_iter()
        .find(|b| b.id == "pouch-12oz")
        .unwrap();
    for s in ["6874", "5163"] {
        let mut j = job(Some("shop"), s, 1, 1);
        let brand_value = store::Brand::value(Some(&brand));
        j.brand = brand_value.clone();
        let mut l = j.slots[0].clone().unwrap();
        l["net"] = json!(net_weight(340.0));
        l["retail"] = json!({ "bag": pouch.name, "min_net_mm": 4.8, "min_mm": 1.6 });
        j.slots = slots(l, 1, 1);
        let c = check(&r, j, Some(150)).await.unwrap();
        let rep = c.report.unwrap_or_else(|| panic!("{s}: {:?}", c.errors));
        let found = retail::check(&rep, &pouch, &brand_value);
        if let Ok(dir) = std::env::var("RR_LABELS_OUT") {
            std::fs::write(format!("{dir}/shop-{s}-retail.png"), c.png.unwrap()).unwrap();
            eprintln!("{s}: layout {:?}\nretail {:?}", rep.issues, found.issues);
        }
        assert!(rep.issues.is_empty(), "{s} layout: {:?}", rep.issues);
        assert!(found.ok, "{s}: {:?}", found.issues);
    }
}

/// Every complete Typst example in the agents' design guide compiles and checks clean
#[tokio::test]
async fn the_design_guides_examples_check_clean() {
    const GUIDE: &str = include_str!("../../../../skills/roastwarden/label-design.md");
    let r = Renderer::in_process();
    let mut n = 0;
    for block in GUIDE.split("```typst\n").skip(1) {
        let src = block.split("```").next().unwrap();
        if src.contains("{ ... }") || !src.contains("#let label(") {
            continue; // an outline, not a design
        }
        let src = format!("#import \"roastwarden.typ\": fit, clamp, join, with, ascii\n{src}");
        // Round labels only for an example that draws them
        let sheets: &[&str] = if src.contains("if round") {
            &["5163", "5294"]
        } else {
            &["5163"]
        };
        for sheet in sheets {
            let rep = check(&r, one_off(&src, sheet), None).await.unwrap();
            let report = rep
                .report
                .unwrap_or_else(|| panic!("{src}\n{:?}", rep.errors));
            assert!(report.ok, "{src}\non {sheet}: {:?}", report.issues);
        }
        n += 1;
    }
    assert!(n >= 2, "only {n} examples");
}

#[tokio::test]
async fn a_check_reports_overflow_overlap_tiny_text_and_a_small_qr_code() {
    let src = r#"
#import "roastwarden.typ": fit
#let design = (name: "bad", params: (size: (default: 40, help: "pt")))
#let label(l, w, h, round) = {
  set text(font: "Atkinson Hyperlegible Next")
  place(top + left, dx: 2mm, dy: 2mm, box(width: 200mm, text(size: l.params.size * 1pt, l.name)))
  place(top + left, dx: 2mm, dy: 4mm, text(size: 12pt)[Overlapping words])
  place(bottom + left, dx: 2mm, dy: -2mm, text(size: 4pt)[tiny print])
  place(bottom + right, dx: -3mm, dy: -3mm, image("qr.svg", width: 8mm))
  place(top + left, dx: 2mm, dy: 25mm, text(font: "No Such Font")[x])
}
"#;
    let c = check(&Renderer::in_process(), one_off(src, "5163"), Some(72))
        .await
        .unwrap();
    assert!(c.errors.is_empty(), "{:?}", c.errors);
    let rep = c.report.unwrap();
    let kinds: Vec<&str> = rep.issues.iter().map(|i| i.kind.as_str()).collect();
    for k in ["overflow", "overlap", "tiny_text", "qr_small", "typst"] {
        assert!(kinds.contains(&k), "{k} in {:?}", rep.issues);
    }
    assert!(!rep.ok);
    assert_eq!(rep.params.as_ref().unwrap()["size"], 40);
    assert_eq!(rep.manifest.as_ref().unwrap()["name"], "bad");
    assert!(rep.fields.missing.contains(&"notes".to_string()));
    let png = c.png.unwrap();
    assert!(png.starts_with(b"\x89PNG"));
    // A parameter given as text takes the default's type; an unknown one is reported
    let mut j = one_off(src, "5163");
    j.params.insert("size".into(), json!("10"));
    j.params.insert("colour".into(), json!("red"));
    let rep = check(&Renderer::in_process(), j, None)
        .await
        .unwrap()
        .report
        .unwrap();
    assert_eq!(rep.params.unwrap()["size"], 10);
    assert!(rep.issues.iter().any(|i| i.kind == "param"));
}

#[tokio::test]
async fn a_check_answers_typst_errors_with_lines_and_cut_text() {
    let r = Renderer::in_process();
    let c = check(
        &r,
        one_off(
            "#let label(l, w, h, round) = {\n  text(l.nmae)\n}\n",
            "5163",
        ),
        None,
    )
    .await
    .unwrap();
    assert!(c.report.is_none());
    assert_eq!(c.errors[0].file.as_deref(), Some("design.typ"));
    assert_eq!(c.errors[0].line, Some(2));
    let src = r#"
#import "roastwarden.typ": clamp
#let label(l, w, h, round) = box(width: w, height: h, inset: 4mm,
  clamp("one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen", 30mm, lines: 1))
"#;
    let rep = check(&r, one_off(src, "5163"), None)
        .await
        .unwrap()
        .report
        .unwrap();
    let cut = rep.issues.iter().find(|i| i.kind == "cut");
    // Cut on one line with most of the label empty: the check says how much
    assert!(
        cut.is_some_and(|i| i.message.contains("mm of the label's height is free")),
        "{:?}",
        rep.issues
    );
    assert!(rep.free.as_ref().is_some_and(|f| f.bottom_mm > 30.0));
    // It prints none of the name and the dates: soft issues, still ok but for the cut
    let dropped: Vec<_> = rep.issues.iter().filter(|i| i.kind == "dropped").collect();
    assert_eq!(dropped.len(), 3, "{:?}", rep.issues);
    assert_eq!(rep.issues.last().unwrap().kind, "dropped");
    assert!(!rep.ok);
    // A cut keeps whole sentences when that keeps at least half
    let src = r#"
#import "roastwarden.typ": clamp
#let label(l, w, h, round) = box(width: w, height: h, inset: 4mm, { l.name; linebreak(); l.roasted; linebreak(); l.best_from; linebreak()
  clamp("Sweet and clean. Juicy red fruit with a long cocoa finish and more words after it here", 38mm, lines: 1) })
"#;
    let rep = check(&r, one_off(src, "5163"), None)
        .await
        .unwrap()
        .report
        .unwrap();
    assert!(
        rep.lines.iter().any(|l| l.text == "Sweet and clean."),
        "{:?}",
        rep.lines
    );
    assert!(rep.notes.iter().any(|n| n["at"] == "sentence"));
    assert!(
        rep.issues.iter().all(|i| i.kind == "cut"),
        "{:?}",
        rep.issues
    );
    // On a round label, text at the box's corner is outside the circle
    let src = r"#let label(l, w, h, round) = place(top + left, dx: 1mm, dy: 1mm, text(size: 10pt)[Corner])";
    let rep = check(&r, one_off(src, "5294"), None)
        .await
        .unwrap()
        .report
        .unwrap();
    assert!(rep.round);
    assert_eq!(rep.issues[0].kind, "overflow", "{:?}", rep.issues);
    assert!(rep.issues[0].message.contains("circle"));
}

#[test]
fn sheets_and_offsets_are_checked() {
    assert!(check_sheets("").is_ok());
    assert!(check_sheets(r#"[{"id":"OL125","name":"OL125","page_w":8.5,"page_h":11,"w":4,"h":2,"cols":2,"rows":5,"left":0.16,"top":0.5,"pitch_x":4.14,"pitch_y":2}]"#).is_ok());
    assert!(check_sheets(r#"[{"id":"x","name":"x","page_w":8.5,"page_h":11,"w":0,"h":2,"cols":2,"rows":5,"left":0,"top":0,"pitch_x":4,"pitch_y":2}]"#).is_err());
    assert!(check_offsets(r#"{"5163": {"x_mm": 0.5, "y_mm": -1}}"#).is_ok());
    assert!(check_offsets(r#"{"5163": {"x_mm": 50}}"#).is_err());
    assert!(check_offsets("nope").is_err());
}
