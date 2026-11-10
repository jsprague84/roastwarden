//! Modules (docs/lean-home.md, "Modules"): the optional parts of the dashboard that are on.
//!
//! One setting, `modules`, a JSON list of the six names below. Not set means every module is
//! on, so an install from before modules is unchanged. Modules shape the dashboard and what
//! the skill offers; they never close an API route (tokens still gate them).

use serde::Serialize;
use sqlx::SqlitePool;
use utoipa::ToSchema;

/// The settings key.
pub const SETTING: &str = "modules";

/// Every module, in the order they are shown.
pub const ALL: [&str; 6] = ["reports", "cupping", "tuning", "agents", "costs", "labels"];

/// The presets, each a list of modules.
pub const PRESETS: [(&str, &[&str]); 3] = [
    ("home", &[]),
    ("enthusiast", &["reports", "cupping", "tuning", "agents"]),
    ("roastery", &ALL),
];

/// A stored value as modules, in [`ALL`]'s order; empty (cleared) is every module.
pub fn parse(value: &str) -> Result<Vec<&'static str>, String> {
    if value.trim().is_empty() {
        return Ok(ALL.to_vec());
    }
    let names: Vec<String> = serde_json::from_str(value).map_err(|_| {
        format!(
            "modules is a JSON list of module names, any of {}",
            ALL.join(", ")
        )
    })?;
    if let Some(bad) = names.iter().find(|n| !ALL.contains(&n.as_str())) {
        return Err(format!(
            "{bad:?} is not a module: they are {}",
            ALL.join(", ")
        ));
    }
    Ok(ALL
        .into_iter()
        .filter(|m| names.iter().any(|n| n == m))
        .collect())
}

/// The preset a set of modules is, or `custom`.
pub fn preset_of(modules: &[&str]) -> &'static str {
    PRESETS
        .iter()
        .find(|(_, p)| p.len() == modules.len() && p.iter().all(|m| modules.contains(m)))
        .map_or("custom", |(name, _)| name)
}

/// What `GET /api/about` says about the modules.
#[derive(Serialize, ToSchema, Debug, PartialEq)]
pub struct Modules {
    /// The modules that are on (reports, cupping, tuning, agents, costs, labels); every one
    /// when the setting was never saved
    pub modules: Vec<&'static str>,
    /// The preset they match: home, enthusiast, roastery, or custom
    pub preset: &'static str,
    /// The setting was never saved and there is no roast and no bean yet: the dashboard
    /// opens its first-run page
    pub first_run: bool,
}

pub async fn read(db: &SqlitePool) -> Modules {
    let stored = crate::auth::settings::get(db, SETTING).await;
    // A value that no longer parses (edited by hand) counts as every module
    let modules = stored
        .as_deref()
        .and_then(|v| parse(v).ok())
        .unwrap_or_else(|| ALL.to_vec());
    let first_run = stored.is_none() && is_empty(db).await;
    Modules {
        preset: preset_of(&modules),
        modules,
        first_run,
    }
}

/// No roast and no bean.
async fn is_empty(db: &SqlitePool) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT NOT EXISTS (SELECT 1 FROM roast_sessions) AND NOT EXISTS (SELECT 1 FROM beans)",
    )
    .fetch_one(db)
    .await
    .is_ok_and(|v| v == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_is_a_list_of_known_modules() {
        assert_eq!(parse("[]").unwrap(), Vec::<&str>::new());
        assert_eq!(
            parse(r#"["labels","reports","reports"]"#).unwrap(),
            ["reports", "labels"]
        );
        assert_eq!(parse("").unwrap(), ALL);
        assert!(
            parse(r#"["reports","stats"]"#)
                .unwrap_err()
                .contains("not a module")
        );
        assert!(parse("reports").is_err());
        assert!(crate::auth::settings::check_value(SETTING, r#"["cupping"]"#).is_ok());
        assert!(crate::auth::settings::check_value(SETTING, r#"["cups"]"#).is_err());
    }

    #[test]
    fn presets_are_named_and_anything_else_is_custom() {
        assert_eq!(preset_of(&[]), "home");
        assert_eq!(
            preset_of(&["tuning", "reports", "agents", "cupping"]),
            "enthusiast"
        );
        assert_eq!(preset_of(&ALL), "roastery");
        assert_eq!(preset_of(&["reports", "cupping", "tuning"]), "custom");
    }

    #[tokio::test]
    async fn about_reports_modules_and_the_first_run() {
        use crate::test_support::Harness;
        use serde_json::json;
        let h = Harness::new().await;
        let about = || async { h.owner("GET", "/api/about", None).await.json() };
        // A new install: every module, first run
        let a = about().await;
        assert_eq!(a["modules"], json!(ALL), "{a}");
        assert_eq!(a["preset"], "roastery");
        assert_eq!(a["first_run"], true);

        // A roast in the database: not a first run, though the setting was never saved
        sqlx::query(
            "INSERT INTO roast_sessions (id, name, device_id, status, created_at) \
             VALUES ('s1', 'old', 'virtual_roaster', 'completed', CURRENT_TIMESTAMP)",
        )
        .execute(&h.state.db)
        .await
        .unwrap();
        assert_eq!(about().await["first_run"], false);
        sqlx::query("DELETE FROM roast_sessions")
            .execute(&h.state.db)
            .await
            .unwrap();
        assert_eq!(about().await["first_run"], true);

        // Saved: no longer the first run, whatever was chosen
        let put = h
            .owner(
                "PUT",
                "/api/settings/modules",
                Some(json!({"value": r#"["cupping","reports","tuning"]"#})),
            )
            .await;
        assert!(put.status.is_success());
        let a = about().await;
        assert_eq!(a["modules"], json!(["reports", "cupping", "tuning"]));
        assert_eq!(a["preset"], "custom");
        assert_eq!(a["first_run"], false);

        // A name that is not a module is refused
        let bad = h
            .owner(
                "PUT",
                "/api/settings/modules",
                Some(json!({"value": r#"["stats"]"#})),
            )
            .await;
        assert_eq!(bad.status.as_u16(), 400);
    }

    #[test]
    fn the_admin_token_may_change_modules_the_others_may_not() {
        use crate::auth::tokens::Preset;
        for (p, ok) in [
            (Preset::Admin, true),
            (Preset::Roast, false),
            (Preset::Inventory, false),
            (Preset::ReadOnly, false),
        ] {
            assert_eq!(p.allows("PUT", "/api/settings/modules"), ok, "{p:?}");
        }
    }
}
