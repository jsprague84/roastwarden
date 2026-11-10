use chrono::{DateTime, Duration, NaiveDate, Utc};
use sqlx::SqlitePool;

use crate::labels::RestDays;
use crate::ops::zone::Zone;

use super::lots::*;
use super::moves::*;
use super::types::*;
use super::windows::*;
use super::*;

fn windows() -> Windows {
    Windows {
        rest: RestDays {
            espresso_light: 10,
            espresso: 7,
            filter_light: 7,
            filter: 4,
        },
        best: BestDays::default(),
        low_g: 200.0,
    }
}

fn row(id: &str, purpose: &str, level: &str, roasted: &str, g: Option<f32>, moved: f32) -> LotRow {
    LotRow {
        id: id.into(),
        name: id.into(),
        device_id: "esp32_roaster_01".into(),
        batch_number: None,
        bean_id: None,
        bean_name: None,
        roast_purpose: Some(purpose.into()).filter(|p: &String| !p.is_empty()),
        target_roast_level: Some(level.into()).filter(|l: &String| !l.is_empty()),
        roasted_at: DateTime::parse_from_rfc3339(roasted)
            .unwrap()
            .with_timezone(&Utc),
        roasted_weight: g,
        moved_g: moved,
        used_g: -moved,
        given_g: 0.0,
        packed_g: 0.0,
        move_count: i64::from(moved != 0.0),
    }
}

fn day(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

#[test]
fn a_light_espresso_rests_ten_days_and_is_at_its_best_for_five_weeks() {
    let w = windows();
    // #12 dropped 04:03 UTC on the 28th: the owner's 27th at UTC−7
    let r = row(
        "k",
        "espresso",
        "light",
        "2026-10-28T04:03:00Z",
        Some(86.0),
        0.0,
    );
    let l = lot(r.clone(), &w, day("2026-10-29"), Zone::minutes(-420));
    assert_eq!(l.roasted_on, day("2026-10-27"));
    assert_eq!(l.ready_from, day("2026-11-06"));
    assert_eq!(l.best_until, day("2026-12-01"));
    assert_eq!(l.state, LotState::Resting);
    assert_eq!(l.ready_in_days, Some(8));
    assert_eq!(l.for_, "espresso");
    assert_eq!(
        lot(r.clone(), &w, day("2026-11-06"), Zone::minutes(-420)).state,
        LotState::Ready
    );
    assert_eq!(
        lot(r.clone(), &w, day("2026-12-01"), Zone::minutes(-420)).state,
        LotState::Ready
    );
    assert_eq!(
        lot(r.clone(), &w, day("2026-12-02"), Zone::minutes(-420)).state,
        LotState::PastBest
    );
    // Untouched two months past its best: gone
    assert_eq!(
        lot(r, &w, day("2027-02-01"), Zone::minutes(-420)).state,
        LotState::Finished
    );
}

#[test]
fn purpose_and_level_pick_the_rest_as_the_label_does() {
    let w = windows();
    let at = "2026-10-01T12:00:00Z";
    let rest = |p, lv| {
        lot(
            row("x", p, lv, at, Some(100.0), 0.0),
            &w,
            day("2026-10-02"),
            Zone::default(),
        )
        .rest_days
    };
    assert_eq!(rest("espresso", "medium"), 7);
    assert_eq!(rest("filter", "light"), 7);
    assert_eq!(rest("filter", "medium_dark"), 4);
    assert_eq!(rest("omni", ""), 7);
    let any = lot(
        row("x", "", "", at, Some(100.0), 0.0),
        &w,
        day("2026-10-02"),
        Zone::default(),
    );
    assert_eq!(any.for_, "any");
    let dark = lot(
        row("x", "filter", "dark", at, Some(100.0), 0.0),
        &w,
        day("2026-10-02"),
        Zone::default(),
    );
    assert_eq!(dark.best_until, day("2026-10-29"));
}

#[test]
fn what_is_left_and_finished() {
    let w = windows();
    let at = "2026-10-01T12:00:00Z";
    let l = lot(
        row("x", "espresso", "light", at, Some(86.0), -36.0),
        &w,
        day("2026-10-20"),
        Zone::default(),
    );
    assert_eq!(l.left_g, Some(50.0));
    assert_eq!(l.state, LotState::Ready);
    let done = lot(
        row("x", "espresso", "light", at, Some(86.0), -86.0),
        &w,
        day("2026-10-20"),
        Zone::default(),
    );
    assert_eq!(done.state, LotState::Finished);
    assert_eq!(done.left_g, Some(0.0));
    let unweighed = lot(
        row("x", "espresso", "light", at, None, 0.0),
        &w,
        day("2026-10-02"),
        Zone::default(),
    );
    assert_eq!(unweighed.state, LotState::Unweighed);
    assert_eq!(unweighed.left_g, None);
}

#[test]
fn a_group_reads_ready_resting_and_when_to_roast_again() {
    let w = windows();
    let today = day("2026-11-10");
    let lots = vec![
        // ready: roasted 10-25, ready 11-04
        lot(
            row(
                "a",
                "espresso",
                "light",
                "2026-10-25T12:00:00Z",
                Some(86.0),
                -20.0,
            ),
            &w,
            today,
            Zone::default(),
        ),
        // resting: roasted 11-05, ready 11-15
        lot(
            row(
                "b",
                "espresso",
                "light",
                "2026-11-05T12:00:00Z",
                Some(86.0),
                0.0,
            ),
            &w,
            today,
            Zone::default(),
        ),
        lot(
            row(
                "c",
                "espresso",
                "light",
                "2026-11-05T13:00:00Z",
                Some(85.0),
                0.0,
            ),
            &w,
            today,
            Zone::default(),
        ),
        lot(
            row(
                "f",
                "filter",
                "light",
                "2026-11-01T12:00:00Z",
                Some(90.0),
                0.0,
            ),
            &w,
            today,
            Zone::default(),
        ),
    ];
    // 18 g a day for the last 14 days
    let uses: Vec<Use> = (0..14).map(|i| (today - Duration::days(i), 18.0)).collect();
    let g = group("espresso", &lots, &uses, &w, today);
    assert_eq!(g.ready_g, 66.0);
    assert_eq!(g.resting_g, 171.0);
    assert_eq!(g.left_g, 237.0);
    assert_eq!(g.lots, 3);
    assert_eq!(g.next_ready, Some(day("2026-11-15")));
    assert_eq!(g.next_ready_g, Some(171.0));
    assert_eq!(g.per_day_g, Some(18.0));
    // 237 / 18 = 13.2 days; a light espresso rests 10: roast by 3 days from now
    assert_eq!(g.lasts_days, Some(13.2));
    assert_eq!(g.roast_by, Some(day("2026-11-13")));
    assert!(!g.low);
    // No use recorded: no rate, low only by the threshold
    let f = group("filter", &lots, &[], &w, today);
    assert_eq!(f.ready_g, 90.0);
    assert_eq!(f.per_day_g, None);
    assert_eq!(f.roast_by, None);
    assert!(f.low, "90 g is below 200 g");
    // Nothing past its best: 0, not -0
    assert!(f.past_best_g.is_sign_positive());
}

#[test]
fn one_mornings_use_is_read_over_a_week() {
    let w = windows();
    let today = day("2026-11-10");
    let lots = vec![lot(
        row(
            "a",
            "espresso",
            "light",
            "2026-10-25T12:00:00Z",
            Some(500.0),
            -36.0,
        ),
        &w,
        today,
        Zone::default(),
    )];
    let g = group("espresso", &lots, &[(today, 36.0)], &w, today);
    assert_eq!(
        g.per_day_g,
        Some(36.0 / 7.0 * 10.0).map(|v: f32| v.round() / 10.0)
    );
    // Use older than four weeks does not count
    let g = group(
        "espresso",
        &lots,
        &[(today - Duration::days(40), 36.0)],
        &w,
        today,
    );
    assert_eq!(g.per_day_g, None);
}

#[test]
fn a_roast_due_now_is_low() {
    let w = Windows {
        low_g: 0.0,
        ..windows()
    };
    let today = day("2026-11-10");
    let lots = vec![lot(
        row(
            "a",
            "espresso",
            "light",
            "2026-10-25T12:00:00Z",
            Some(86.0),
            0.0,
        ),
        &w,
        today,
        Zone::default(),
    )];
    let uses: Vec<Use> = (0..7).map(|i| (today - Duration::days(i), 30.0)).collect();
    let g = group("espresso", &lots, &uses, &w, today);
    assert_eq!(g.roast_by, Some(today));
    assert!(g.low);
}

async fn roast(db: &SqlitePool, id: &str, device: &str, end: &str, weight: Option<f32>) {
    sqlx::query(
        "INSERT INTO roast_sessions (id, name, device_id, status, created_at, updated_at, \
             end_time, roast_purpose, target_roast_level, roasted_weight) \
             VALUES (?, ?, ?, 'completed', ?, ?, ?, 'espresso', 'light', ?)",
    )
    .bind(id)
    .bind(format!("Roast {id}"))
    .bind(device)
    .bind(end)
    .bind(end)
    .bind(end)
    .bind(weight)
    .execute(db)
    .await
    .unwrap();
}

fn req(kind: &str) -> RoastedMoveRequest {
    RoastedMoveRequest {
        kind: kind.into(),
        ..Default::default()
    }
}

#[tokio::test]
async fn moves_take_coffee_and_a_deleted_roast_takes_its_moves() {
    let db = crate::db::open_memory().await;
    let now = DateTime::parse_from_rfc3339("2026-11-10T18:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    roast(
        &db,
        "a",
        "esp32_roaster_01",
        "2026-10-25T03:00:00Z",
        Some(86.0),
    )
    .await;
    roast(&db, "b", "esp32_roaster_01", "2026-11-08T03:00:00Z", None).await;
    // A test roast on the simulator that was never weighed is not coffee
    roast(
        &db,
        "v",
        crate::sim::virtual_roaster::DEVICE_ID,
        "2026-11-08T03:00:00Z",
        None,
    )
    .await;
    let svc = RoastedService::new(db.clone());

    // Two espresso doses (18 g each by default) and a gift
    let l = svc
        .add_move(
            "a",
            RoastedMoveRequest {
                doses: Some(2.0),
                ..req("used")
            },
            Zone::default(),
            now,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(l.left_g, Some(50.0));
    assert_eq!(l.used_g, 36.0);
    let l = svc
        .add_move(
            "a",
            RoastedMoveRequest {
                grams: Some(20.0),
                recipient: Some("Sam".into()),
                ..req("given")
            },
            Zone::default(),
            now,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(l.left_g, Some(30.0));
    assert_eq!(l.given_g, 20.0);
    assert_eq!(
        l.moves.as_ref().unwrap()[1].recipient.as_deref(),
        Some("Sam")
    );

    // More than is left, a recipient on use, two amounts, an unweighed roast: refused
    let too_much = RoastedMoveRequest {
        grams: Some(40.0),
        ..req("used")
    };
    assert!(
        svc.add_move("a", too_much, Zone::default(), now)
            .await
            .is_err()
    );
    let to = RoastedMoveRequest {
        grams: Some(1.0),
        recipient: Some("x".into()),
        ..req("used")
    };
    assert!(svc.add_move("a", to, Zone::default(), now).await.is_err());
    let two = RoastedMoveRequest {
        grams: Some(1.0),
        all: true,
        ..req("used")
    };
    assert!(svc.add_move("a", two, Zone::default(), now).await.is_err());
    assert!(
        svc.add_move(
            "b",
            RoastedMoveRequest {
                all: true,
                ..req("used")
            },
            Zone::default(),
            now
        )
        .await
        .is_err()
    );
    assert!(
        svc.add_move(
            "nope",
            RoastedMoveRequest {
                all: true,
                ..req("used")
            },
            Zone::default(),
            now
        )
        .await
        .unwrap()
        .is_none()
    );

    // A count of what is left, then everything used
    let l = svc
        .add_move(
            "a",
            RoastedMoveRequest {
                set_g: Some(25.0),
                ..req("adjust")
            },
            Zone::default(),
            now,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(l.left_g, Some(25.0));
    let l = svc
        .add_move(
            "a",
            RoastedMoveRequest {
                all: true,
                ..req("used")
            },
            Zone::default(),
            now,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(l.state, LotState::Finished);

    // The list leaves out what is finished, and lists the unweighed real roast
    let s = svc.stock(Zone::default(), false, now).await.unwrap();
    let ids: Vec<&str> = s.lots.iter().map(|l| l.session_id.as_str()).collect();
    assert_eq!(ids, ["b"]);
    assert_eq!(s.lots[0].state, LotState::Unweighed);
    assert_eq!(s.groups[0].unweighed, 1);
    let s = svc.stock(Zone::default(), true, now).await.unwrap();
    assert_eq!(s.lots.len(), 2);
    // 36 + 25 g used today, read over a week (a gift and a count are not use)
    assert_eq!(s.groups[0].per_day_g, Some(8.7));

    // Taking the last move back gives the coffee back
    let last = l.moves.unwrap().pop().unwrap();
    let l = svc
        .delete_move("a", &last.id, Zone::default(), now)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(l.left_g, Some(25.0));
    assert!(
        svc.delete_move("a", "nope", Zone::default(), now)
            .await
            .unwrap()
            .is_none()
    );

    // Deleting the roast deletes its moves
    sqlx::query("DELETE FROM roast_sessions WHERE id = 'a'")
        .execute(&db)
        .await
        .unwrap();
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM roasted_moves")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn packed_bags_take_coffee_off_but_are_not_use() {
    let db = crate::db::open_memory().await;
    let now = DateTime::parse_from_rfc3339("2026-11-10T18:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    roast(
        &db,
        "a",
        "esp32_roaster_01",
        "2026-11-01T03:00:00Z",
        Some(500.0),
    )
    .await;
    let svc = RoastedService::new(db.clone());
    let pack = |bags: f32, g: f32, grind: Option<&str>| RoastedMoveRequest {
        bags: Some(bags),
        bag_g: Some(g),
        grind: grind.map(str::to_string),
        ..req("packed")
    };
    // 2 × 100 g ground for espresso
    let l = svc
        .add_move(
            "a",
            pack(2.0, 100.0, Some("espresso")),
            Zone::default(),
            now,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(l.left_g, Some(300.0));
    assert_eq!(l.packed_g, 200.0);
    assert_eq!(l.used_g, 0.0);
    let m = &l.moves.as_ref().unwrap()[0];
    assert_eq!(
        (m.bags, m.bag_g, m.form.as_deref(), m.grind.as_deref()),
        (Some(2), Some(100.0), Some("ground"), Some("espresso"))
    );
    // Ground on the 10th: 14 days, before the lot's 6 Dec (35 days from 1 Nov)
    assert_eq!(l.best_until, day("2026-12-06"));
    assert_eq!(m.best_until, Some(day("2026-11-24")));
    // Whole bean keeps the lot's
    let l = svc
        .add_move("a", pack(1.0, 250.0, None), Zone::default(), now)
        .await
        .unwrap()
        .unwrap();
    let m = &l.moves.as_ref().unwrap()[1];
    assert_eq!(m.form.as_deref(), Some("whole"));
    assert_eq!(m.best_until, Some(l.best_until));
    assert_eq!(l.left_g, Some(50.0));
    // More than is left, part bags, grams, a grind with whole, a grind on use: refused
    for bad in [
        pack(1.0, 100.0, None),
        pack(0.5, 10.0, None),
        RoastedMoveRequest {
            grams: Some(10.0),
            ..req("packed")
        },
        RoastedMoveRequest {
            form: Some("whole".into()),
            ..pack(1.0, 10.0, Some("filter"))
        },
        RoastedMoveRequest {
            grams: Some(10.0),
            grind: Some("filter".into()),
            ..req("used")
        },
    ] {
        assert!(svc.add_move("a", bad, Zone::default(), now).await.is_err());
    }
    // Packing is not use: no rate
    let s = svc.stock(Zone::default(), false, now).await.unwrap();
    assert_eq!(s.groups[0].per_day_g, None);
    assert_eq!(s.best_until_days_ground, 14);
}

#[test]
fn a_form_reads_as_its_label_says() {
    let f = Form::parse(None, Some(" espresso ")).unwrap();
    assert!(f.ground);
    assert_eq!(f.text(), "Ground · espresso");
    assert_eq!(Form::parse(Some("ground"), None).unwrap().text(), "Ground");
    assert_eq!(Form::parse(None, None).unwrap().text(), "Whole bean");
    assert!(Form::parse(Some("whole"), Some("moka")).is_err());
    assert!(Form::parse(Some("powder"), None).is_err());
}

#[test]
fn a_move_splits_by_what_each_roast_has_left() {
    assert_eq!(
        split(-40.0, &[50.0, 30.0], &[86.0, 86.0]),
        vec![-25.0, -15.0]
    );
    // Thirds round to a tenth; the last takes the rounding
    let s = split(-10.0, &[30.0, 30.0, 30.0], &[1.0, 1.0, 1.0]);
    assert_eq!(s, vec![-3.3, -3.3, -3.4]);
    assert!((s.iter().sum::<f32>() + 10.0).abs() < 1e-4);
    // Everything: each gives all it has
    assert_eq!(
        split(-80.0, &[50.0, 30.0], &[86.0, 86.0]),
        vec![-50.0, -30.0]
    );
    // A count up from nothing: by roasted weight
    assert_eq!(split(20.0, &[0.0, 0.0], &[100.0, 60.0]), vec![12.5, 7.5]);
}

async fn bean_roast(db: &SqlitePool, id: &str, batch: i32, bean: &str, end: &str, g: Option<f32>) {
    sqlx::query(
            "INSERT INTO beans (id, name, created_at, updated_at) VALUES (?, ?, ?, ?) ON CONFLICT DO NOTHING",
        )
        .bind(bean)
        .bind(format!("Bean {bean}"))
        .bind(end)
        .bind(end)
        .execute(db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO roast_sessions (id, name, device_id, status, created_at, updated_at, \
             end_time, roast_purpose, target_roast_level, roasted_weight, bean_id, batch_number) \
             VALUES (?, ?, 'esp32_roaster_01', 'completed', ?, ?, ?, 'filter', 'light', ?, ?, ?)",
    )
    .bind(id)
    .bind(format!("Roast {id}"))
    .bind(end)
    .bind(end)
    .bind(end)
    .bind(g)
    .bind(bean)
    .bind(batch)
    .execute(db)
    .await
    .unwrap();
}

#[tokio::test]
async fn roasts_in_one_bag_share_their_moves() {
    let db = crate::db::open_memory().await;
    let now = DateTime::parse_from_rfc3339("2026-11-10T18:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let z = Zone::default();
    bean_roast(&db, "a", 16, "eth", "2026-11-06T02:00:00Z", Some(97.0)).await;
    bean_roast(&db, "b", 17, "eth", "2026-11-07T02:00:00Z", Some(97.0)).await;
    bean_roast(&db, "c", 18, "eth", "2026-11-08T02:00:00Z", Some(100.0)).await;
    bean_roast(&db, "k", 19, "kenya", "2026-11-08T02:00:00Z", Some(86.0)).await;
    bean_roast(&db, "u", 20, "eth", "2026-11-09T02:00:00Z", None).await;
    let svc = RoastedService::new(db.clone());
    let combine = |roasts: &[&str]| CombineRequest {
        roasts: roasts
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
        ..Default::default()
    };
    // a used a little before it went in the bag
    svc.add_move(
        "a",
        RoastedMoveRequest {
            grams: Some(17.0),
            ..req("used")
        },
        z,
        now,
    )
    .await
    .unwrap();

    // One roast, an unweighed roast, no such roast: refused (another coffee is a blend:
    // `a_blend_of_coffees_in_one_bag`)
    for bad in [&["a"][..], &["a", "u"], &["a", "nope"]] {
        assert!(svc.combine(combine(bad), z, now).await.is_err(), "{bad:?}");
    }
    let c = svc.combine(combine(&["b", "a"]), z, now).await.unwrap();
    assert_eq!(c.lot.batches, "16+17");
    assert_eq!(c.lot.name, "Bean eth #16+17");
    assert!(!c.lot.blend);
    assert_eq!(c.bean_id.as_deref(), Some("eth"));
    assert_eq!(c.lot.coffees.len(), 1);
    assert_eq!(c.lot.coffees[0].share_pct, 100);
    assert_eq!(c.lot.left_g, 177.0);
    assert_eq!(c.lot.roasts, ["a", "b"]);
    // Ready when the newest is (light filter: 7 days from the 7th), best until the oldest's
    assert_eq!(c.lot.ready_from, day("2026-11-14"));
    assert_eq!(c.lot.best_until, day("2026-12-11"));
    assert_eq!(c.lot.state, LotState::Resting);

    // A move on either roast is the lot's: 40 g split 80:97
    let l = svc
        .add_move(
            "b",
            RoastedMoveRequest {
                grams: Some(40.0),
                ..req("used")
            },
            z,
            now,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(l.combined.as_ref().unwrap().left_g, 137.0);
    let c = svc.combined("a", z, now).await.unwrap().unwrap();
    assert_eq!(c.moves.len(), 1);
    assert_eq!(c.moves[0].delta_g, -40.0);
    assert_eq!(c.moves[0].split.len(), 2);
    let parts: Vec<f32> = c.members.iter().map(|m| m.left_g.unwrap()).collect();
    assert_eq!(parts, vec![61.9, 75.1]);
    // More than the lot holds: refused
    assert!(
        svc.add_move(
            "a",
            RoastedMoveRequest {
                grams: Some(140.0),
                ..req("used")
            },
            z,
            now
        )
        .await
        .is_err()
    );

    // The list: both roasts, side by side, counted as one lot, resting as the lot is
    let s = svc.stock(z, false, now).await.unwrap();
    let ids: Vec<&str> = s.lots.iter().map(|l| l.session_id.as_str()).collect();
    let at = ids.iter().position(|i| *i == "a").unwrap();
    assert_eq!(ids[at + 1], "b");
    let filter = s.groups.iter().find(|g| g.for_ == "filter").unwrap();
    assert_eq!(filter.lots, 3, "the bag counts once, then c and k");
    assert_eq!(
        filter.resting_g, 323.0,
        "137 g in the bag, c's 100 g and k's 86 g"
    );

    // Undo from one roast takes the whole move back
    let share = c.moves[0].split[1].clone();
    svc.delete_move(&share.session_id, &share.move_id, z, now)
        .await
        .unwrap()
        .unwrap();
    let c = svc.combined(&c.lot.id, z, now).await.unwrap().unwrap();
    assert!(c.moves.is_empty());
    assert_eq!(c.lot.left_g, 177.0);

    // A third roast joins; another lot's roast cannot
    let c = svc.combine(combine(&["c", "a"]), z, now).await.unwrap();
    assert_eq!(c.lot.batches, "16+17+18");
    assert_eq!(c.lot.ready_from, day("2026-11-15"));

    // Packing two bags from it, then everything used
    svc.add_move(
        "c",
        RoastedMoveRequest {
            bags: Some(2.0),
            bag_g: Some(100.0),
            ..req("packed")
        },
        z,
        now,
    )
    .await
    .unwrap();
    let c = svc.combined("c", z, now).await.unwrap().unwrap();
    assert_eq!(c.packed_g, 200.0);
    assert_eq!(c.moves[0].bags, Some(2));
    svc.add_move(
        "a",
        RoastedMoveRequest {
            all: true,
            ..req("used")
        },
        z,
        now,
    )
    .await
    .unwrap();
    let c = svc.combined("a", z, now).await.unwrap().unwrap();
    assert_eq!(c.lot.state, LotState::Finished);
    assert!(c.members.iter().all(|m| m.left_g == Some(0.0)));

    // Split apart: each keeps its moves; a deleted roast leaves a lot of one, which goes
    let gone = svc.uncombine("b").await.unwrap().unwrap();
    assert_eq!(gone.len(), 3);
    assert!(svc.combined("a", z, now).await.unwrap().is_none());
    assert_eq!(
        svc.lot("a", z, now).await.unwrap().unwrap().left_g,
        Some(0.0)
    );
    bean_roast(&db, "d", 21, "eth", "2026-11-09T03:00:00Z", Some(90.0)).await;
    bean_roast(&db, "e", 22, "eth", "2026-11-09T04:00:00Z", Some(90.0)).await;
    svc.combine(combine(&["d", "e"]), z, now).await.unwrap();
    sqlx::query("DELETE FROM roast_sessions WHERE id = 'e'")
        .execute(&db)
        .await
        .unwrap();
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM roasted_combines")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(n, 0);
    assert!(
        svc.lot("d", z, now)
            .await
            .unwrap()
            .unwrap()
            .combined
            .is_none()
    );
}

#[test]
fn shares_add_up_to_a_hundred() {
    assert_eq!(percents(&[97.0, 98.0]), vec![50, 50]);
    assert_eq!(percents(&[1.0, 1.0, 1.0]), vec![34, 33, 33]);
    assert_eq!(percents(&[115.0, 85.0]), vec![58, 42]);
    assert_eq!(percents(&[0.0, 0.0]), vec![0, 0]);
}

/// Different coffees mixed in one bag, a blend: its coffees and their
/// shares, a name from them, moves split as for one coffee, the label naming each origin
#[tokio::test]
async fn a_blend_of_coffees_in_one_bag() {
    let db = crate::db::open_memory().await;
    let now = DateTime::parse_from_rfc3339("2026-11-10T18:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let z = Zone::default();
    bean_roast(&db, "e", 16, "eth", "2026-11-06T02:00:00Z", Some(120.0)).await;
    bean_roast(&db, "k", 19, "kenya", "2026-11-08T02:00:00Z", Some(80.0)).await;
    roast(
        &db,
        "x",
        "esp32_roaster_01",
        "2026-11-08T03:00:00Z",
        Some(90.0),
    )
    .await;
    sqlx::query(
        "UPDATE beans SET origin = 'Ethiopia', region = 'Guji', process = 'natural', \
             short_name = 'Guji' WHERE id = 'eth'",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE beans SET origin = 'Kenya', process = 'washed', variety = 'SL28' \
             WHERE id = 'kenya'",
    )
    .execute(&db)
    .await
    .unwrap();
    let svc = RoastedService::new(db.clone());
    let combine = |roasts: &[&str]| CombineRequest {
        roasts: roasts
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
        ..Default::default()
    };
    // A roast with no coffee recorded cannot go in: its label could not name it
    let e = svc.combine(combine(&["e", "x"]), z, now).await.unwrap_err();
    assert!(e.to_string().contains("no coffee"), "{e}");

    let c = svc.combine(combine(&["k", "e"]), z, now).await.unwrap();
    assert!(c.lot.blend);
    assert_eq!(c.bean_id, None);
    assert_eq!(c.lot.name, "Bean eth + Bean kenya #16+19");
    let shares: Vec<(&str, u32)> = c
        .lot
        .coffees
        .iter()
        .map(|c| (c.bean_id.as_str(), c.share_pct))
        .collect();
    assert_eq!(shares, [("eth", 60), ("kenya", 40)]);
    // Ready when the newer is: light filter, 7 days from the 8th
    assert_eq!(c.lot.ready_from, day("2026-11-15"));

    // 50 g used: 30 + 20, the shares stay
    svc.add_move(
        "k",
        RoastedMoveRequest {
            grams: Some(50.0),
            ..req("used")
        },
        z,
        now,
    )
    .await
    .unwrap();
    let c = svc.combined("e", z, now).await.unwrap().unwrap();
    let left: Vec<f32> = c.members.iter().map(|m| m.left_g.unwrap()).collect();
    assert_eq!(left, vec![90.0, 60.0]);
    assert_eq!(c.lot.coffees[0].share_pct, 60);

    // The label: the bag's coffees, each origin with its share
    let lot = label_lot(&db, "k").await.unwrap().unwrap();
    assert_eq!(lot.batches, "16+19");
    assert_eq!(
        lot.coffees,
        vec![("eth".to_string(), 60), ("kenya".to_string(), 40)]
    );
    let f = crate::labels::facts(&db, "k", "https://r.example")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(f.name, "Guji + Bean kenya");
    let l = crate::labels::label_data(&f, &crate::labels::Ask::default(), &windows().rest);
    assert_eq!(l["origin"], "60 % Ethiopia · 40 % Kenya");
    assert_eq!(l["process"], "natural / washed");
    assert_eq!(l["variety"], "SL28");
    assert_eq!(l["blend"], true);
    assert_eq!(l["components"][1]["name"], "Bean kenya");
    assert_eq!(l["components"][0]["region"], "Guji");
    assert_eq!(l["batch"], "16+19");
    assert_eq!(
        l["notes"],
        serde_json::Value::Null,
        "a blend's notes are the owner's"
    );
    // Hiding the origin hides the coffees too
    let hidden = crate::labels::label_data(
        &f,
        &crate::labels::Ask {
            hide: vec!["origin".into()],
            ..Default::default()
        },
        &windows().rest,
    );
    assert_eq!(hidden["components"], serde_json::json!([]));

    // Its own name wins
    svc.combine(
        CombineRequest {
            roasts: vec!["e".into()],
            name: Some("House filter".into()),
            ..Default::default()
        },
        z,
        now,
    )
    .await
    .unwrap();
    let f = crate::labels::facts(&db, "e", "").await.unwrap().unwrap();
    assert_eq!(f.name, "House filter");
}

#[test]
fn ready_comes_first_oldest_first() {
    let w = windows();
    let today = day("2026-11-10");
    let mut lots = vec![
        lot(
            row(
                "resting",
                "espresso",
                "light",
                "2026-11-05T12:00:00Z",
                Some(86.0),
                0.0,
            ),
            &w,
            today,
            Zone::default(),
        ),
        lot(
            row(
                "ready-new",
                "espresso",
                "light",
                "2026-10-30T12:00:00Z",
                Some(86.0),
                0.0,
            ),
            &w,
            today,
            Zone::default(),
        ),
        lot(
            row(
                "ready-old",
                "espresso",
                "light",
                "2026-10-25T12:00:00Z",
                Some(86.0),
                0.0,
            ),
            &w,
            today,
            Zone::default(),
        ),
    ];
    sort(&mut lots);
    let ids: Vec<&str> = lots.iter().map(|l| l.session_id.as_str()).collect();
    assert_eq!(ids, ["ready-old", "ready-new", "resting"]);
}
