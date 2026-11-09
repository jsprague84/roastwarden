//! Pick lists for a bean's repeated values (`docs/green-coffee.md`, "Pick lists"): what was
//! entered before, then a seeded list. The seeds come from a survey of suppliers' listings;
//! they suggest and never restrict, so any value can still be typed.

use std::collections::BTreeSet;

use anyhow::Result;
use serde::Serialize;
use sqlx::AssertSqlSafe;
use sqlx::SqlitePool;

/// Coffee-producing countries a home roaster meets (Wikipedia's production list, by region).
pub const COUNTRIES: &[&str] = &[
    // Africa and Arabia
    "Burundi",
    "Cameroon",
    "DR Congo",
    "Ethiopia",
    "Kenya",
    "Malawi",
    "Rwanda",
    "Tanzania",
    "Uganda",
    "Yemen",
    "Zambia",
    "Zimbabwe",
    // The Americas
    "Bolivia",
    "Brazil",
    "Colombia",
    "Costa Rica",
    "Dominican Republic",
    "Ecuador",
    "El Salvador",
    "Guatemala",
    "Haiti",
    "Honduras",
    "Jamaica",
    "Mexico",
    "Nicaragua",
    "Panama",
    "Peru",
    "Puerto Rico",
    "USA (Hawaii)",
    "Venezuela",
    // Asia and the Pacific
    "China",
    "India",
    "Indonesia",
    "Laos",
    "Myanmar",
    "Nepal",
    "Papua New Guinea",
    "Philippines",
    "Taiwan",
    "Thailand",
    "Timor-Leste",
    "Vietnam",
];

pub const PROCESSES: &[&str] = &[
    "Washed",
    "Natural",
    "Honey",
    "White honey",
    "Yellow honey",
    "Red honey",
    "Black honey",
    "Pulped natural",
    "Anaerobic washed",
    "Anaerobic natural",
    "Carbonic maceration",
    "Wet-hulled",
    "Decaf (Swiss Water)",
    "Decaf (Mountain Water)",
    "Decaf (sugarcane EA)",
    "Decaf (CO2)",
];

pub const CERTIFICATIONS: &[&str] = &[
    "Organic",
    "Fairtrade",
    "Fair Trade USA",
    "Rainforest Alliance",
    "Bird Friendly",
    "4C",
    "C.A.F.E. Practices",
];

pub const VARIETIES: &[&str] = &[
    "Bourbon",
    "Caturra",
    "Catuai",
    "Castillo",
    "Gesha",
    "Heirloom",
    "Mundo Novo",
    "Pacamara",
    "Pacas",
    "Ruiru 11",
    "SL28",
    "SL34",
    "Typica",
    "Yellow Bourbon",
];

pub const DRYING: &[&str] = &["Raised beds", "Patio", "Mechanical", "Greenhouse"];

/// Home-roaster suppliers whose product pages `docs/green-coffee-sources.md` checked.
pub const SUPPLIERS: &[&str] = &[
    "Sweet Maria's",
    "Bodhi Leaf",
    "Theta Ridge",
    "Coffee Bean Corral",
    "Showroom Coffee",
    "Happy Mug",
    "Burman Coffee Traders",
];

/// Suggestions per field: what was entered before first (most used first), then the seeds.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PickLists {
    pub supplier: Vec<String>,
    pub origin: Vec<String>,
    pub region: Vec<String>,
    pub variety: Vec<String>,
    pub process: Vec<String>,
    pub drying: Vec<String>,
    pub certifications: Vec<String>,
}

/// Values used before (trimmed, distinct ignoring case, most used first), then the seeds
/// not already there.
fn merge(used: Vec<String>, seeds: &[&str]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for v in used
        .into_iter()
        .chain(seeds.iter().map(std::string::ToString::to_string))
    {
        let t = v.trim().to_string();
        if !t.is_empty() && seen.insert(t.to_lowercase()) {
            out.push(t);
        }
    }
    out
}

async fn used(db: &SqlitePool, sql: &str) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar::<_, String>(AssertSqlSafe(sql))
        .fetch_all(db)
        .await?)
}

/// `SELECT col FROM … GROUP BY col ORDER BY COUNT(*) DESC` over the beans (and, for the
/// supplier, the purchases).
fn by_use(col: &str) -> String {
    format!(
        "SELECT {col} FROM beans WHERE {col} IS NOT NULL AND TRIM({col}) <> '' \
         GROUP BY {col} COLLATE NOCASE ORDER BY COUNT(*) DESC, {col}"
    )
}

pub async fn pick_lists(db: &SqlitePool) -> Result<PickLists> {
    let suppliers = used(
        db,
        "SELECT supplier FROM (SELECT supplier FROM beans UNION ALL \
         SELECT supplier FROM bean_stock_moves) WHERE supplier IS NOT NULL AND TRIM(supplier) <> '' \
         GROUP BY supplier COLLATE NOCASE ORDER BY COUNT(*) DESC, supplier",
    )
    .await?;
    // Certifications are kept comma-separated: split them.
    let certs: Vec<String> = used(db, &by_use("certifications"))
        .await?
        .iter()
        .flat_map(|c| c.split(',').map(|s| s.trim().to_string()))
        .collect();
    Ok(PickLists {
        supplier: merge(suppliers, SUPPLIERS),
        origin: merge(used(db, &by_use("origin")).await?, COUNTRIES),
        region: merge(used(db, &by_use("region")).await?, &[]),
        variety: merge(used(db, &by_use("variety")).await?, VARIETIES),
        process: merge(used(db, &by_use("process")).await?, PROCESSES),
        drying: merge(used(db, &by_use("drying")).await?, DRYING),
        certifications: merge(certs, CERTIFICATIONS),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn used_values_come_first_without_case_duplicates() {
        let m = merge(
            vec!["natural".into(), " Washed ".into(), String::new()],
            &["Washed", "Natural", "Honey"],
        );
        assert_eq!(m, vec!["natural", "Washed", "Honey"]);
    }

    #[tokio::test]
    async fn pick_lists_mix_what_was_entered_with_the_seeds() {
        let db = crate::db::open_memory().await;
        for (name, origin, supplier, certs) in [
            ("A", "Ethiopia", "Sweet Maria's", "Organic, Fairtrade"),
            ("B", "ethiopia", "Local Importer", "Organic"),
            ("C", "Kenya", "Local Importer", ""),
        ] {
            sqlx::query(
                "INSERT INTO beans (id, name, origin, supplier, certifications, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?, '2026-10-25', '2026-10-25')",
            )
            .bind(name)
            .bind(name)
            .bind(origin)
            .bind(supplier)
            .bind(certs)
            .execute(&db)
            .await
            .unwrap();
        }
        let p = pick_lists(&db).await.unwrap();
        assert_eq!(p.origin[..2], ["Ethiopia", "Kenya"]);
        assert!(p.origin.contains(&"Brazil".to_string()));
        assert_eq!(
            p.origin
                .iter()
                .filter(|o| o.eq_ignore_ascii_case("ethiopia"))
                .count(),
            1
        );
        assert_eq!(p.supplier[0], "Local Importer");
        assert_eq!(p.certifications[..2], ["Organic", "Fairtrade"]);
        assert!(p.process.contains(&"Washed".to_string()));
    }
}
