use std::collections::BTreeMap;

use chrono::Utc;
use sqlx::SqlitePool;

use crate::roasts::types::{CreateSessionRequest, UpdateSessionRequest};

use super::types::*;
use super::*;

async fn setup() -> (InventoryService, SqlitePool) {
    let db = crate::db::open_memory().await;
    (InventoryService::new(db.clone()), db)
}

fn bean_req(name: &str, stock: f32) -> BeanRequest {
    BeanRequest {
        name: Some(name.into()),
        origin: Some(Some("Brazil".into())),
        variety: Some(Some("Catuai".into())),
        moisture_pct: Some(Some(10.5)),
        stock_g: Some(stock),
        ..Default::default()
    }
}

fn roast(device: &str) -> CreateSessionRequest {
    CreateSessionRequest {
        name: "Roast 1".into(),
        device_id: device.into(),
        ..Default::default()
    }
}

#[tokio::test]
async fn a_roast_draws_its_green_weight_and_cancel_gives_it_back() {
    let (inv, db) = setup().await;
    let bean = inv
        .create_bean(bean_req("Brazil Cerrado", 1000.0))
        .await
        .unwrap();
    assert_eq!(bean.stock_g, 1000.0);
    assert!(!bean.low);

    let s = crate::roasts::sessions::create_session(
        &db,
        CreateSessionRequest {
            bean_id: Some(bean.id.clone()),
            green_weight: Some(100.0),
            ..roast("esp32_roaster_01")
        },
    )
    .await
    .unwrap();
    // The bean fills origin, variety and moisture.
    assert_eq!(s.bean_origin.as_deref(), Some("Brazil"));
    assert_eq!(s.bean_variety.as_deref(), Some("Catuai"));
    assert_eq!(s.moisture_green, Some(10.5));
    assert_eq!(inv.bean(&bean.id).await.unwrap().unwrap().stock_g, 900.0);

    // Editing the weight afterwards moves the stock with it.
    crate::roasts::sessions::update_session(
        &db,
        &s.id,
        UpdateSessionRequest {
            green_weight: Some(120.0),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let b = inv.bean(&bean.id).await.unwrap().unwrap();
    assert_eq!(b.stock_g, 880.0);
    assert_eq!(b.roast_count, 1);

    // Moving the roast to no bean returns the beans.
    crate::roasts::sessions::update_session(
        &db,
        &s.id,
        UpdateSessionRequest {
            bean_id: Some(None),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(inv.bean(&bean.id).await.unwrap().unwrap().stock_g, 1000.0);
    crate::roasts::sessions::update_session(
        &db,
        &s.id,
        UpdateSessionRequest {
            bean_id: Some(Some(bean.id.clone())),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(inv.bean(&bean.id).await.unwrap().unwrap().stock_g, 880.0);

    // Cancel deletes the roast: its move goes with it.
    assert!(crate::roasts::sessions::delete_session(&db, &s.id)
        .await
        .unwrap());
    assert_eq!(inv.bean(&bean.id).await.unwrap().unwrap().stock_g, 1000.0);
}

#[tokio::test]
async fn batch_numbers_count_per_roaster() {
    let (_inv, db) = setup().await;
    let a1 = crate::roasts::sessions::create_session(&db, roast("a"))
        .await
        .unwrap();
    let a2 = crate::roasts::sessions::create_session(&db, roast("a"))
        .await
        .unwrap();
    let b1 = crate::roasts::sessions::create_session(&db, roast("b"))
        .await
        .unwrap();
    assert_eq!(
        (a1.batch_number, a2.batch_number, b1.batch_number),
        (Some(1), Some(2), Some(1))
    );
    // A cancelled last roast frees its number.
    crate::roasts::sessions::delete_session(&db, &a2.id)
        .await
        .unwrap();
    let a3 = crate::roasts::sessions::create_session(&db, roast("a"))
        .await
        .unwrap();
    assert_eq!(a3.batch_number, Some(2));
    // Editable; 0 or less clears.
    let e = crate::roasts::sessions::update_session(
        &db,
        &a3.id,
        UpdateSessionRequest {
            batch_number: Some(Some(41)),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(e.batch_number, Some(41));
    assert_eq!(
        crate::roasts::sessions::create_session(&db, roast("a"))
            .await
            .unwrap()
            .batch_number,
        Some(42)
    );
}

#[tokio::test]
async fn a_planned_roast_fills_the_roast_and_returns_to_the_queue_on_cancel() {
    let (inv, db) = setup().await;
    let bean = inv.create_bean(bean_req("Kenya AA", 400.0)).await.unwrap();
    // 400 g is below the default 500 g: low.
    assert!(bean.low);
    let p1 = inv
        .create_plan(PlanRequest {
            device_id: Some(Some("a".into())),
            bean_id: Some(Some(bean.id.clone())),
            green_weight_g: Some(Some(100.0)),
            name: Some(Some("Kenya for filter".into())),
            ..Default::default()
        })
        .await
        .unwrap();
    let p2 = inv.create_plan(PlanRequest::default()).await.unwrap();
    // A profile or bean that does not exist is refused.
    for req in [
        PlanRequest {
            profile_id: Some(Some("no-such-profile".into())),
            ..Default::default()
        },
        PlanRequest {
            bean_id: Some(Some("no-such-bean".into())),
            ..Default::default()
        },
    ] {
        let e = inv.create_plan(req).await.unwrap_err();
        assert!(e.downcast_ref::<InventoryInputError>().is_some(), "{e}");
    }
    let other = inv
        .create_plan(PlanRequest {
            device_id: Some(Some("b".into())),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(p2.position > p1.position);
    assert_eq!(p1.bean_name.as_deref(), Some("Kenya AA"));
    assert_eq!(p1.bean_stock_g, Some(400.0));
    // Roaster a sees its plan and the any-roaster one, not b's.
    let ids: Vec<_> = inv
        .list_plans(Some("a"), false)
        .await
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(ids, vec![p1.id.clone(), p2.id.clone()]);

    let s = crate::roasts::sessions::create_session(
        &db,
        CreateSessionRequest {
            planned_roast_id: Some(p1.id.clone()),
            name: "Roast 10/24/2026, 7:00 AM".into(),
            ..roast("a")
        },
    )
    .await
    .unwrap();
    assert_eq!(s.name, "Kenya for filter");
    assert_eq!(s.green_weight, Some(100.0));
    assert_eq!(s.bean_id.as_deref(), Some(bean.id.as_str()));
    assert_eq!(s.planned_roast_id.as_deref(), Some(p1.id.as_str()));
    assert_eq!(inv.bean(&bean.id).await.unwrap().unwrap().stock_g, 300.0);
    let queue = inv.list_plans(Some("a"), false).await.unwrap();
    assert_eq!(queue.len(), 1);
    let done = inv.plan(&p1.id).await.unwrap().unwrap();
    assert_eq!(done.session_id.as_deref(), Some(s.id.as_str()));
    assert_eq!(done.batch_number, Some(1));

    // A plan that already ran does not fill a second roast.
    let again = crate::roasts::sessions::create_session(
        &db,
        CreateSessionRequest {
            planned_roast_id: Some(p1.id.clone()),
            ..roast("a")
        },
    )
    .await
    .unwrap();
    assert_eq!(again.planned_roast_id, None);
    assert_eq!(again.green_weight, None);

    // Cancel: the plan is back at the front of the queue.
    crate::roasts::sessions::delete_session(&db, &s.id)
        .await
        .unwrap();
    let ids: Vec<_> = inv
        .list_plans(Some("a"), false)
        .await
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(ids, vec![p1.id.clone(), p2.id.clone()]);

    // Reorder.
    inv.reorder_plans(&[p2.id.clone(), other.id.clone(), p1.id.clone()])
        .await
        .unwrap();
    let ids: Vec<_> = inv
        .list_plans(None, false)
        .await
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(ids, vec![p2.id, other.id, p1.id]);
}

#[tokio::test]
async fn stock_purchases_adjustments_and_deleting_beans() {
    let (inv, db) = setup().await;
    let bean = inv.create_bean(bean_req("Ethiopia", 0.0)).await.unwrap();
    assert_eq!(bean.stock_g, 0.0);
    let b = inv
        .add_stock(
            &bean.id,
            StockRequest {
                delta_g: Some(2000.0),
                note: Some("bag".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(b.stock_g, 2000.0);
    let b = inv
        .add_stock(
            &bean.id,
            StockRequest {
                set_g: Some(1850.0),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(b.stock_g, 1850.0);
    let d = inv.bean_detail(&bean.id).await.unwrap().unwrap();
    let kinds: Vec<_> = d
        .moves
        .iter()
        .map(|m| (m.kind.as_str(), m.delta_g))
        .collect();
    assert_eq!(kinds, vec![("adjust", -150.0), ("purchase", 2000.0)]);
    assert!(inv
        .add_stock(&bean.id, StockRequest::default())
        .await
        .unwrap_err()
        .downcast_ref::<InventoryInputError>()
        .is_some());
    assert!(inv
        .create_bean(BeanRequest::default())
        .await
        .unwrap_err()
        .downcast_ref::<InventoryInputError>()
        .is_some());

    // Unused: deleted. Used: archived, and hidden from the default list.
    let spare = inv.create_bean(bean_req("Spare", 0.0)).await.unwrap();
    assert_eq!(
        inv.delete_bean(&spare.id).await.unwrap(),
        BeanDeleted::Deleted
    );
    crate::roasts::sessions::create_session(
        &db,
        CreateSessionRequest {
            bean_id: Some(bean.id.clone()),
            green_weight: Some(100.0),
            ..roast("a")
        },
    )
    .await
    .unwrap();
    assert_eq!(
        inv.delete_bean(&bean.id).await.unwrap(),
        BeanDeleted::Archived
    );
    assert!(inv.list_beans(false).await.unwrap().is_empty());
    assert_eq!(inv.list_beans(true).await.unwrap().len(), 1);
    assert_eq!(
        inv.delete_bean("nope").await.unwrap(),
        BeanDeleted::NotFound
    );
}

fn buy(delta_g: f32, price: f64, at: &str) -> StockRequest {
    StockRequest {
        delta_g: Some(delta_g),
        purchase: PurchaseFields {
            price: Some(price),
            at: Some(at.into()),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// 2 kg for 40, a 100 g roast with 84 g out, then an order of two beans with 12 shipping
/// split by weight: costs follow the ledger, and editing a purchase reprices the roast.
#[tokio::test]
async fn purchases_price_the_stock_and_the_roasts() {
    let (inv, db) = setup().await;
    let guji = inv.create_bean(bean_req("Guji", 0.0)).await.unwrap();
    let kenya = inv.create_bean(bean_req("Kenya", 0.0)).await.unwrap();
    let b = inv
        .add_stock(&guji.id, buy(2000.0, 40.0, "2026-09-01"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!((b.cost_per_kg, b.stock_value), (Some(20.0), Some(40.0)));
    assert_eq!(b.currency.as_deref(), Some("USD"));

    let s = crate::roasts::sessions::create_session(
        &db,
        CreateSessionRequest {
            bean_id: Some(guji.id.clone()),
            green_weight: Some(100.0),
            ..roast("a")
        },
    )
    .await
    .unwrap();
    crate::roasts::sessions::update_session(
        &db,
        &s.id,
        UpdateSessionRequest {
            roasted_weight: Some(84.0),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let c = inv.roast_cost(&s.id).await.unwrap().unwrap();
    assert_eq!((c.green_cost, c.roasted_per_kg), (2.0, Some(23.81)));

    let order = inv
        .record_order(OrderRequest {
            supplier: Some("Sweet Maria's".into()),
            shipping: Some(12.0),
            // Now: after the roast.
            lines: vec![
                OrderLine {
                    bean_id: guji.id.clone(),
                    weight_g: 1000.0,
                    price: Some(25.0),
                    crop_year: Some("2026".into()),
                    ..Default::default()
                },
                OrderLine {
                    bean_id: kenya.id.clone(),
                    weight_g: 2000.0,
                    price: Some(60.0),
                    ..Default::default()
                },
            ],
            ..Default::default()
        })
        .await
        .unwrap();
    let shares: Vec<_> = order.moves.iter().map(|m| m.shipping).collect();
    assert_eq!(shares, vec![Some(4.0), Some(8.0)]);
    assert_eq!(order.moves[0].supplier.as_deref(), Some("Sweet Maria's"));
    // Guji: 1.9 kg at 20 = 38, plus 1 kg for 29: 67 / 2.9 kg.
    let g = inv.bean(&guji.id).await.unwrap().unwrap();
    assert_eq!(g.cost_per_kg, Some(23.1));
    assert_eq!(
        inv.bean(&kenya.id).await.unwrap().unwrap().cost_per_kg,
        Some(34.0)
    );

    // The first purchase was really 50: the roast before the order costs 2.50 now.
    let d = inv.bean_detail(&guji.id).await.unwrap().unwrap();
    let first = d.moves.iter().find(|m| m.price == Some(40.0)).unwrap();
    let edited = inv
        .update_move(
            &guji.id,
            &first.id,
            MoveRequest {
                price: Some(Some(50.0)),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(edited.cost, Some(50.0));
    assert_eq!(
        inv.roast_cost(&s.id).await.unwrap().unwrap().green_cost,
        2.5
    );
    // A roast's move is the roast's.
    let roast_move = d.moves.iter().find(|m| m.kind == "roast").unwrap();
    assert!(inv
        .delete_move(&guji.id, &roast_move.id)
        .await
        .unwrap_err()
        .downcast_ref::<InventoryInputError>()
        .is_some());
    // A price goes with a purchase only, and the currency is a code.
    for req in [
        StockRequest {
            set_g: Some(10.0),
            purchase: PurchaseFields {
                price: Some(1.0),
                ..Default::default()
            },
            ..Default::default()
        },
        StockRequest {
            delta_g: Some(10.0),
            purchase: PurchaseFields {
                currency: Some("dollars".into()),
                ..Default::default()
            },
            ..Default::default()
        },
    ] {
        assert!(inv.add_stock(&guji.id, req).await.is_err());
    }

    let r = inv.cost_report(12).await.unwrap();
    assert_eq!(r.currency, "USD");
    let this_month = Utc::now().format("%Y-%m").to_string();
    let now = r.months.iter().find(|m| m.month == this_month).unwrap();
    assert_eq!((now.spent, now.bought_g), (97.0, 3000.0));
    let sep = r.months.iter().find(|m| m.month == "2026-09").unwrap();
    assert_eq!((sep.spent, sep.bought_g), (50.0, 2000.0));
    let roast_month = r
        .months
        .iter()
        .find(|m| m.roasts == 1)
        .expect("the roast's month");
    assert_eq!(
        (roast_month.roast_cost, roast_month.costed_roasts),
        (2.5, 1)
    );
    assert_eq!(roast_month.roasted_g, 84.0);

    assert!(inv.delete_move(&guji.id, &first.id).await.unwrap());
    assert!(!inv.delete_move(&guji.id, &first.id).await.unwrap());
}

#[tokio::test]
async fn supplier_details_are_kept_checked_and_cleared() {
    let (inv, _) = setup().await;
    let mut details = BTreeMap::new();
    details.insert("Packaging".to_string(), "GrainPro".to_string());
    let b = inv
        .create_bean(BeanRequest {
            region: Some(Some("Kercha, Guji Zone".into())),
            altitude: Some(Some("1900–2200 masl".into())),
            certifications: Some(Some("Organic, Fairtrade".into())),
            score: Some(Some(92.6)),
            url: Some(Some("https://www.sweetmarias.com/products/x".into())),
            details: Some(Some(details)),
            ..bean_req("Kercha", 0.0)
        })
        .await
        .unwrap();
    assert_eq!(b.region.as_deref(), Some("Kercha, Guji Zone"));
    assert_eq!(b.score, Some(92.6));
    assert_eq!(b.details.as_ref().unwrap().0["Packaging"], "GrainPro");
    let b = inv
        .update_bean(
            &b.id,
            BeanRequest {
                region: Some(None),
                details: Some(None),
                grade: Some(Some("Grade 1".into())),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!((b.region, b.details.is_none()), (None, true));
    assert_eq!(b.grade.as_deref(), Some("Grade 1"));
    assert_eq!(b.altitude.as_deref(), Some("1900–2200 masl"));
    for bad_req in [
        BeanRequest {
            score: Some(Some(101.0)),
            ..Default::default()
        },
        BeanRequest {
            url: Some(Some("javascript:alert(1)".into())),
            ..Default::default()
        },
    ] {
        assert!(inv.update_bean(&b.id, bad_req).await.is_err());
    }
    let p = crate::coffee::picklists::pick_lists(&inv.db).await.unwrap();
    assert_eq!(p.certifications[..2], ["Organic", "Fairtrade"]);
}

/// Moves are f32: 5000 + 907.2 (2 lb) and a correction to 3200 summed to 3199.9998.
#[tokio::test]
async fn stock_is_whole_tenths_of_a_gram() {
    let (inv, _) = setup().await;
    let bean = inv.create_bean(bean_req("Guji", 5000.0)).await.unwrap();
    let add = |delta_g: Option<f32>, set_g: Option<f32>| StockRequest {
        delta_g,
        set_g,
        ..Default::default()
    };
    let b = inv
        .add_stock(&bean.id, add(Some(907.2), None))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(b.stock_g, 5907.2);
    let b = inv
        .add_stock(&bean.id, add(None, Some(3200.0)))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(b.stock_g, 3200.0);
}
