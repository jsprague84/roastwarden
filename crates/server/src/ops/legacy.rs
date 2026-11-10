//! The names from before the rename to Roastwarden (0.25.0), carried
//! over once so an install made as rustRoast starts as it was: its `RUSTROAST_*` variables, its
//! database file `rustroast.db` and its backups `rustroast-<time>.db`. The MQTT presence topic
//! and the mDNS service name of older firmware are in lib.rs and discovery.rs.

use std::path::{Path, PathBuf};

const OLD_ENV: &str = "RUSTROAST_";
const NEW_ENV: &str = "ROASTWARDEN_";
const OLD_DB: &str = "rustroast.db";
const NEW_DB: &str = "roastwarden.db";
const OLD_BACKUP: &str = "rustroast-";
const NEW_BACKUP: &str = "roastwarden-";

/// Every `RUSTROAST_<name>` set without its `ROASTWARDEN_<name>` is read under the new name.
/// Returns the old names it read, for a warning once logging is up.
///
/// # Safety
///
/// It writes the process's environment, which nothing else may read or write meanwhile: call
/// it first in `main`, while the process has one thread (before the async runtime is built).
pub unsafe fn adopt_env() -> Vec<String> {
    let mut adopted = Vec::new();
    for (key, value) in std::env::vars_os() {
        let Some(key) = key.to_str() else { continue };
        let Some(rest) = key.strip_prefix(OLD_ENV) else {
            continue;
        };
        let new = format!("{NEW_ENV}{rest}");
        if std::env::var_os(&new).is_none() {
            // SAFETY: the caller's promise, no other thread exists yet
            unsafe { std::env::set_var(&new, value) };
            adopted.push(key.to_string());
        }
    }
    adopted.sort();
    adopted
}

/// The warning for [`adopt_env`]'s names, or None when there were none.
pub fn env_warning(adopted: &[String]) -> Option<String> {
    (!adopted.is_empty()).then(|| {
        format!(
            "{} read under their new names ({NEW_ENV}…): rename them, the old names stop working in a later release",
            adopted.join(", ")
        )
    })
}

/// Before the database opens: when `db` is `…/roastwarden.db` and missing while `rustroast.db`
/// is beside it, the old file (with its `-wal`, `-shm` and a staged `.restore`) becomes the new
/// one; then every `rustroast-<time>.db` in `backups` is renamed `roastwarden-<time>.db`.
/// Returns what was moved, for the log.
pub fn adopt_files(db: &Path, backups: &Path) -> std::io::Result<Vec<String>> {
    let mut moved = Vec::new();
    if db.file_name().and_then(|n| n.to_str()) == Some(NEW_DB) && !db.exists() {
        let dir = db.parent().unwrap_or(Path::new("."));
        if dir.join(OLD_DB).is_file() {
            for suffix in ["", "-wal", "-shm", ".restore"] {
                let from = dir.join(format!("{OLD_DB}{suffix}"));
                if from.exists() {
                    let to = dir.join(format!("{NEW_DB}{suffix}"));
                    std::fs::rename(&from, &to)?;
                    moved.push(format!("{} → {}", from.display(), to.display()));
                }
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir(backups) {
        let old: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(OLD_BACKUP) && n.ends_with(".db"))
            })
            .collect();
        for from in old {
            let name = from
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            let to = from.with_file_name(name.replacen(OLD_BACKUP, NEW_BACKUP, 1));
            if !to.exists() {
                std::fs::rename(&from, &to)?;
                moved.push(format!("{} → {}", from.display(), to.display()));
            }
        }
    }
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_install_s_files_take_the_new_names_once() {
        let dir = std::env::temp_dir().join(format!("roastwarden-legacy-{}", uuid::Uuid::new_v4()));
        let backups = dir.join("backups");
        std::fs::create_dir_all(&backups).unwrap();
        for f in ["rustroast.db", "rustroast.db-wal", "rustroast.db-shm"] {
            std::fs::write(dir.join(f), f).unwrap();
        }
        for f in [
            "rustroast-20261101-030000Z.db",
            "notes.txt",
            "roastwarden-20261102-030000Z.db",
        ] {
            std::fs::write(backups.join(f), f).unwrap();
        }
        let db = dir.join("roastwarden.db");
        let moved = adopt_files(&db, &backups).unwrap();
        assert_eq!(moved.len(), 4, "{moved:?}");
        assert_eq!(std::fs::read_to_string(&db).unwrap(), "rustroast.db");
        assert_eq!(
            std::fs::read_to_string(dir.join("roastwarden.db-wal")).unwrap(),
            "rustroast.db-wal"
        );
        assert!(!dir.join("rustroast.db").exists());
        assert!(backups.join("roastwarden-20261101-030000Z.db").is_file());
        assert!(backups.join("notes.txt").is_file());

        // A second start finds nothing to move; a new database is never replaced
        std::fs::write(dir.join("rustroast.db"), "stray").unwrap();
        assert_eq!(
            adopt_files(&db, &backups).unwrap(),
            [] as [std::string::String; 0]
        );
        assert_eq!(std::fs::read_to_string(&db).unwrap(), "rustroast.db");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_database_named_by_hand_is_left_alone() {
        let dir = std::env::temp_dir().join(format!("roastwarden-legacy-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("rustroast.db"), "old").unwrap();
        let moved = adopt_files(&dir.join("mine.db"), &dir.join("backups")).unwrap();
        assert_eq!(moved, [] as [std::string::String; 0]);
        assert!(dir.join("rustroast.db").is_file());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_warning_names_the_old_variables() {
        assert_eq!(env_warning(&[]), None);
        let w = env_warning(&["RUSTROAST_API_KEY".into()]).unwrap();
        assert!(w.starts_with("RUSTROAST_API_KEY read under their new names"));
    }
}
