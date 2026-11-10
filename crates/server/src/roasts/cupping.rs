//! The classic SCA cupping form (2004 protocol, `docs/cupping.md`): ten attributes, defects,
//! the total and the final score.
//!
//! Seven attributes score quality on 0–10 in quarter points (the form's scale runs 6–9.75).
//! Uniformity, clean cup and sweetness give 2 points a cup over five cups (0, 2, …, 10), and
//! are normally 10. Defects take 2 points for each tainted cup and 4 for each faulty one.
//! Final = the ten's total − defects. Scores are never converted to or from the 2024 CVA form.

use crate::profiles::validate::ProfileInputError;

/// Cups a sample is cupped in (the protocol's five).
pub const CUPS: u8 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// 0–10 in quarter points
    Quality,
    /// 2 points a cup: 0, 2, 4, 6, 8, 10
    Cups,
}

/// The form's attributes in the protocol's order: (key, label, kind, other names accepted).
pub const ATTRIBUTES: [(&str, &str, Kind, &[&str]); 10] = [
    (
        "fragrance_aroma",
        "Fragrance/Aroma",
        Kind::Quality,
        &["aroma", "fragrance", "fragrance/aroma"],
    ),
    ("flavor", "Flavor", Kind::Quality, &["flavour"]),
    ("aftertaste", "Aftertaste", Kind::Quality, &[]),
    ("acidity", "Acidity", Kind::Quality, &[]),
    ("body", "Body", Kind::Quality, &[]),
    ("balance", "Balance", Kind::Quality, &[]),
    ("uniformity", "Uniformity", Kind::Cups, &[]),
    ("clean_cup", "Clean Cup", Kind::Cups, &["clean", "cleancup"]),
    ("sweetness", "Sweetness", Kind::Cups, &[]),
    ("overall", "Overall", Kind::Quality, &[]),
];

/// The key for a name or one of its other names (`Aroma`, `clean-cup`), if the form has it.
pub fn key(name: &str) -> Option<&'static str> {
    let n = name.trim().to_ascii_lowercase().replace(['-', ' '], "_");
    ATTRIBUTES
        .iter()
        .find(|(k, _, _, aliases)| *k == n || aliases.contains(&n.as_str()))
        .map(|(k, ..)| *k)
}

pub fn kind(key: &str) -> Option<Kind> {
    ATTRIBUTES
        .iter()
        .find(|(k, ..)| *k == key)
        .map(|(_, _, kind, _)| *kind)
}

/// A score the form allows for that attribute, or why not.
pub fn check_score(key: &str, score: f32) -> Result<(), String> {
    match kind(key) {
        Some(Kind::Quality) => {
            if !(0.0..=10.0).contains(&score) || (score * 4.0).fract() != 0.0 {
                return Err(format!(
                    "{key} {score}: quality attributes take 0 to 10 in quarter points (the form's scale is 6–9.75)"
                ));
            }
        }
        Some(Kind::Cups) => {
            if !(0.0..=10.0).contains(&score) || (score / 2.0).fract() != 0.0 {
                return Err(format!(
                    "{key} {score}: 2 points a cup over five cups, so 0, 2, 4, 6, 8 or 10 (10 when every cup passes)"
                ));
            }
        }
        None => return Err(unknown(key)),
    }
    Ok(())
}

fn unknown(name: &str) -> String {
    let keys: Vec<&str> = ATTRIBUTES.iter().map(|(k, ..)| *k).collect();
    format!("unknown attribute '{name}': {}", keys.join(", "))
}

/// Points off for defects: 2 per tainted cup, 4 per faulty cup.
pub fn defects(taint_cups: i64, fault_cups: i64) -> i64 {
    2 * taint_cups + 4 * fault_cups
}

/// The ten attributes' sum, once all ten are scored.
pub fn total<'a>(scores: impl IntoIterator<Item = (&'a str, f32)>) -> Option<f32> {
    let mut seen = 0usize;
    let mut sum = 0.0;
    for (k, s) in scores {
        if key(k).is_some() {
            seen += 1;
            sum += s;
        }
    }
    (seen == ATTRIBUTES.len()).then_some(sum)
}

/// The protocol's band for a final score.
pub fn band(final_score: f32) -> &'static str {
    if final_score >= 90.0 {
        "outstanding"
    } else if final_score >= 85.0 {
        "excellent"
    } else if final_score >= 80.0 {
        "very good"
    } else {
        "below specialty"
    }
}

/// Checks a request and gives back its attributes under their keys, in the form's order.
pub fn normalize(
    framework: Option<&str>,
    attributes: &[(String, f32)],
    taint_cups: i64,
    fault_cups: i64,
) -> Result<Vec<(&'static str, f32)>, ProfileInputError> {
    let bad = |m: String| ProfileInputError(m);
    if let Some(f) = framework
        && !f.eq_ignore_ascii_case("sca") {
            return Err(bad(format!(
                "scoring framework '{f}': only the classic SCA form (sca) is recorded; CVA scores are not converted"
            )));
        }
    let mut out: Vec<(&'static str, f32)> = Vec::with_capacity(attributes.len());
    for (name, score) in attributes {
        let k = key(name).ok_or_else(|| bad(unknown(name)))?;
        if out.iter().any(|(o, _)| *o == k) {
            return Err(bad(format!("{k} is scored twice")));
        }
        check_score(k, *score).map_err(bad)?;
        out.push((k, *score));
    }
    for (what, n) in [("taint_cups", taint_cups), ("fault_cups", fault_cups)] {
        if !(0..=CUPS as i64).contains(&n) {
            return Err(bad(format!("{what} {n}: 0 to {CUPS} cups")));
        }
    }
    if taint_cups + fault_cups > CUPS as i64 {
        return Err(bad(format!(
            "{} cups with defects: a sample has {CUPS} cups, each tainted or faulty at most",
            taint_cups + fault_cups
        )));
    }
    out.sort_by_key(|(k, _)| ATTRIBUTES.iter().position(|(a, ..)| a == k));
    Ok(out)
}

use anyhow::Result;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::roasts::types::{
    CreateCuppingRequest, CuppingAttribute, CuppingScore, CuppingWithAttributes,
};

// ---- Cupping Notes CRUD (AP-012) ----

/// Scores a roast on the classic SCA form, replacing an earlier score (`docs/cupping.md`).
/// A score the form does not allow is a [`ProfileInputError`]; `None` when there is no
/// such roast.
pub async fn create_cupping(
    db: &SqlitePool,
    session_id: &str,
    req: CreateCuppingRequest,
) -> Result<Option<CuppingWithAttributes>> {
    let attrs: Vec<(String, f32)> = req
        .attributes
        .into_iter()
        .map(|a| (a.name, a.score))
        .collect();
    let attrs = crate::roasts::cupping::normalize(
        req.scoring_framework.as_deref(),
        &attrs,
        req.taint_cups,
        req.fault_cups,
    )?;
    if crate::roasts::sessions::get_session(db, session_id)
        .await?
        .is_none()
    {
        return Ok(None);
    }
    let final_score = crate::roasts::cupping::total(attrs.iter().map(|(k, s)| (*k, *s)))
        .map(|t| t - crate::roasts::cupping::defects(req.taint_cups, req.fault_cups) as f32);

    let mut tx = db.begin().await?;
    // One cupping a roast: a new score replaces the old (its attributes go with it)
    sqlx::query("DELETE FROM cupping_scores WHERE session_id = ?")
        .bind(session_id)
        .execute(&mut *tx)
        .await?;
    let cupping_id = Uuid::new_v4().to_string();
    sqlx::query(
        r"INSERT INTO cupping_scores
                 (id, session_id, scoring_framework, overall_score, notes, taint_cups, fault_cups)
               VALUES (?, ?, 'sca', ?, ?, ?, ?)",
    )
    .bind(&cupping_id)
    .bind(session_id)
    .bind(final_score)
    .bind(&req.notes)
    .bind(req.taint_cups)
    .bind(req.fault_cups)
    .execute(&mut *tx)
    .await?;
    for (name, score) in &attrs {
        sqlx::query(
                "INSERT INTO cupping_attributes (id, cupping_id, attribute_name, score) VALUES (?, ?, ?, ?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(&cupping_id)
            .bind(name)
            .bind(score)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    get_cupping(db, session_id).await
}

pub async fn get_cupping(
    db: &SqlitePool,
    session_id: &str,
) -> Result<Option<CuppingWithAttributes>> {
    let cupping = sqlx::query_as::<_, CuppingScore>(
        "SELECT * FROM cupping_scores WHERE session_id = ? ORDER BY created_at DESC LIMIT 1",
    )
    .bind(session_id)
    .fetch_optional(db)
    .await?;

    let Some(cupping) = cupping else {
        return Ok(None);
    };

    let mut attributes = sqlx::query_as::<_, CuppingAttribute>(
        "SELECT * FROM cupping_attributes WHERE cupping_id = ?",
    )
    .bind(&cupping.id)
    .fetch_all(db)
    .await?;
    let order = |name: &str| {
        crate::roasts::cupping::ATTRIBUTES
            .iter()
            .position(|(k, ..)| *k == name)
            .unwrap_or(usize::MAX)
    };
    attributes.sort_by_key(|a| order(&a.attribute_name));
    let total_score = crate::roasts::cupping::total(
        attributes
            .iter()
            .map(|a| (a.attribute_name.as_str(), a.score)),
    );
    let defects = crate::roasts::cupping::defects(cupping.taint_cups, cupping.fault_cups);
    let band = cupping
        .final_score
        .map(|f| crate::roasts::cupping::band(f).to_string());

    Ok(Some(CuppingWithAttributes {
        cupping,
        total_score,
        defects,
        band,
        attributes,
    }))
}

pub async fn delete_cupping(db: &SqlitePool, session_id: &str) -> Result<()> {
    // CASCADE will delete cupping_attributes
    sqlx::query("DELETE FROM cupping_scores WHERE session_id = ?")
        .bind(session_id)
        .execute(db)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roasts::types::{CreateCuppingAttributeRequest, CreateSessionRequest};

    fn all(quality: f32, cups: f32) -> Vec<(String, f32)> {
        ATTRIBUTES
            .iter()
            .map(|(k, _, kind, _)| {
                (
                    k.to_string(),
                    if *kind == Kind::Cups { cups } else { quality },
                )
            })
            .collect()
    }

    #[test]
    fn a_full_form_totals_and_bands() {
        // 7.5 × 7 + 10 × 3 = 82.5: very good; one fault takes 4 → 78.5, below specialty
        let a = normalize(Some("SCA"), &all(7.5, 10.0), 0, 0).unwrap();
        let t = total(a.iter().map(|(k, s)| (*k, *s))).unwrap();
        assert_eq!(t, 82.5);
        assert_eq!(band(t), "very good");
        assert_eq!(t - defects(0, 1) as f32, 78.5);
        assert_eq!(band(78.5), "below specialty");
        assert_eq!(band(85.0), "excellent");
        assert_eq!(band(90.0), "outstanding");
        assert_eq!(defects(2, 1), 8);
    }

    #[test]
    fn a_partial_form_has_no_total() {
        let a = normalize(None, &[("acidity".into(), 7.5)], 0, 0).unwrap();
        assert_eq!(total(a.iter().map(|(k, s)| (*k, *s))), None);
    }

    #[test]
    fn other_names_map_to_keys_in_the_forms_order() {
        let a = normalize(
            None,
            &[
                ("Overall".into(), 8.0),
                ("aroma".into(), 8.25),
                ("clean-cup".into(), 10.0),
            ],
            0,
            0,
        )
        .unwrap();
        assert_eq!(
            a,
            vec![
                ("fragrance_aroma", 8.25),
                ("clean_cup", 10.0),
                ("overall", 8.0)
            ]
        );
    }

    #[test]
    fn what_the_form_does_not_allow_is_refused() {
        let no = |attrs: &[(&str, f32)], t: i64, f: i64| {
            let attrs: Vec<(String, f32)> =
                attrs.iter().map(|(k, s)| (k.to_string(), *s)).collect();
            normalize(None, &attrs, t, f).unwrap_err().0
        };
        assert!(no(&[("sweetness", 6.5)], 0, 0).contains("0, 2, 4, 6, 8 or 10"));
        assert!(no(&[("uniformity", 7.0)], 0, 0).contains("2 points a cup"));
        assert!(no(&[("acidity", 7.3)], 0, 0).contains("quarter points"));
        assert!(no(&[("acidity", 10.5)], 0, 0).contains("quarter points"));
        assert!(no(&[("crema", 7.0)], 0, 0).contains("unknown attribute 'crema'"));
        assert!(no(&[("aroma", 7.0), ("fragrance", 7.0)], 0, 0).contains("twice"));
        assert!(no(&[], 6, 0).contains("taint_cups 6"));
        assert!(no(&[], 3, 3).contains("6 cups with defects"));
        assert!(normalize(Some("cva"), &[], 0, 0)
            .unwrap_err()
            .0
            .contains("not converted"));
        // The extremes the form allows pass
        assert!(normalize(None, &all(0.0, 0.0), 5, 0).is_ok());
        assert!(normalize(None, &all(10.0, 10.0), 0, 5).is_ok());
    }

    #[tokio::test]
    async fn test_cupping_crud() {
        let db = crate::db::open_memory().await;

        // Create a session
        let session = crate::roasts::sessions::create_session(
            &db,
            CreateSessionRequest {
                bean_id: None,
                planned_roast_id: None,
                roast_purpose: None,
                name: "Cupping Test".to_string(),
                device_id: "esp32-001".to_string(),
                profile_id: None,
                bean_origin: Some("Ethiopia".to_string()),
                bean_variety: None,
                green_weight: None,
                target_roast_level: None,
                notes: None,
                ambient_temp: None,
                humidity: None,
            },
        )
        .await
        .unwrap();

        // No cupping initially
        let cupping = crate::roasts::cupping::get_cupping(&db, &session.id)
            .await
            .unwrap();
        assert!(cupping.is_none());

        // A partial form: no total yet; other names map to the keys
        let attr = |name: &str, score: f32| CreateCuppingAttributeRequest {
            name: name.to_string(),
            score,
        };
        let result = crate::roasts::cupping::create_cupping(
            &db,
            &session.id,
            CreateCuppingRequest {
                scoring_framework: Some("sca".to_string()),
                notes: Some("Floral, bright".to_string()),
                attributes: vec![attr("aroma", 8.0), attr("acidity", 7.5), attr("body", 7.0)],
                taint_cups: 0,
                fault_cups: 0,
            },
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(result.cupping.session_id, session.id);
        assert_eq!(result.cupping.scoring_framework, "sca");
        assert_eq!(result.cupping.notes, Some("Floral, bright".to_string()));
        assert_eq!(result.cupping.final_score, None);
        assert_eq!(result.total_score, None);
        assert_eq!(result.attributes.len(), 3);
        assert_eq!(result.attributes[0].attribute_name, "fragrance_aroma");

        // A full form with one tainted and one faulty cup: 7.5 × 7 + 10 + 8 + 10 = 80.5,
        // less 2 + 4 = 74.5
        let full: Vec<_> = crate::roasts::cupping::ATTRIBUTES
            .iter()
            .map(|(k, ..)| match *k {
                "uniformity" | "sweetness" => attr(k, 10.0),
                "clean_cup" => attr(k, 8.0),
                _ => attr(k, 7.5),
            })
            .collect();
        let whole = crate::roasts::cupping::create_cupping(
            &db,
            &session.id,
            CreateCuppingRequest {
                scoring_framework: None,
                notes: None,
                attributes: full,
                taint_cups: 1,
                fault_cups: 1,
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(whole.total_score, Some(80.5));
        assert_eq!(whole.defects, 6);
        assert_eq!(whole.cupping.final_score, Some(74.5));
        assert_eq!(whole.band.as_deref(), Some("below specialty"));
        assert_eq!(whole.attributes.last().unwrap().attribute_name, "overall");

        // What the form does not allow, and an unknown roast
        let err = crate::roasts::cupping::create_cupping(
            &db,
            &session.id,
            CreateCuppingRequest {
                scoring_framework: None,
                notes: None,
                attributes: vec![attr("sweetness", 6.5)],
                taint_cups: 0,
                fault_cups: 0,
            },
        )
        .await
        .unwrap_err();
        assert!(err.downcast_ref::<ProfileInputError>().is_some(), "{err}");
        let none = crate::roasts::cupping::create_cupping(
            &db,
            "no-such-roast",
            CreateCuppingRequest {
                scoring_framework: None,
                notes: None,
                attributes: vec![],
                taint_cups: 0,
                fault_cups: 0,
            },
        )
        .await
        .unwrap();
        assert!(none.is_none());
        // The partial one again, for the checks below
        let result = crate::roasts::cupping::create_cupping(
            &db,
            &session.id,
            CreateCuppingRequest {
                scoring_framework: None,
                notes: None,
                attributes: vec![attr("aroma", 8.0), attr("acidity", 7.5), attr("body", 7.0)],
                taint_cups: 0,
                fault_cups: 0,
            },
        )
        .await
        .unwrap()
        .unwrap();

        // Fetch cupping
        let fetched = crate::roasts::cupping::get_cupping(&db, &session.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched.cupping.id, result.cupping.id);
        assert_eq!(fetched.attributes.len(), 3);

        // Cupping included in session with telemetry
        let swt = crate::roasts::sessions::get_session_with_telemetry(&db, &session.id)
            .await
            .unwrap()
            .unwrap();
        assert!(swt.cupping.is_some());
        assert_eq!(swt.cupping.unwrap().attributes.len(), 3);

        // Delete cupping
        crate::roasts::cupping::delete_cupping(&db, &session.id)
            .await
            .unwrap();
        let gone = crate::roasts::cupping::get_cupping(&db, &session.id)
            .await
            .unwrap();
        assert!(gone.is_none());
    }
}
