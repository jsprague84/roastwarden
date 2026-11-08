//! The commands that run next to the database, without the network: `backup` (the nightly copy,
//! now) and `restore` (check a copy and stage it for the next start). Meant for `docker exec
//! roastwarden roast …`.

use serde_json::json;
use std::path::{Path, PathBuf};

use roastwarden_server::backups;

use crate::out::{Failure, Out};

/// The same copy the server makes every night, recorded in the run history as `cli`.
pub async fn backup(out: &Out, db: &str) -> Result<(), Failure> {
    let created = backups::back_up_file(db, "cli")
        .await
        .map_err(|e| Failure::new("FAILED", format!("{e:#}")))?;
    let answer = json!({
        "backup": created.file.path,
        "name": created.file.name,
        "bytes": created.file.bytes,
        "removed": created.removed,
    });
    out.emit(&answer, |_| {
        format!(
            "wrote {} ({} bytes){}",
            created.file.path.display(),
            created.file.bytes,
            if created.removed > 0 {
                format!(", removed {} older", created.removed)
            } else {
                String::new()
            }
        )
    });
    Ok(())
}

/// Check a copy and, unless `check_only`, stage it for the next start. The running server is
/// not touched: it swaps the copy in when it restarts.
pub async fn restore(out: &Out, db: &str, file: &Path, check_only: bool) -> Result<(), Failure> {
    let file = resolve(db, file)?;
    let found = backups::inspect(&file)
        .await
        .map_err(|e| Failure::new("INVALID", format!("{e:#}")))?;
    if check_only {
        let answer = json!({
            "checked": file,
            "restorable": true,
            "schema": found.schema,
            "roasts": found.roasts,
            "profiles": found.profiles,
            "beans": found.beans,
        });
        out.emit(&answer, |_| {
            format!(
                "{} can be restored: schema {}, {} roasts, {} profiles, {} beans",
                file.display(),
                found.schema,
                found.roasts,
                found.profiles,
                found.beans
            )
        });
        return Ok(());
    }
    let db_path = Path::new(db);
    if !db_path.is_file() {
        return Err(Failure::new(
            "NO_FILE",
            format!("no database at {db} (set ROASTWARDEN_DB_PATH or pass --db)"),
        ));
    }
    let replaced = backups::staged_path(db_path).exists();
    let staged = backups::stage_restore(db_path, &file)
        .map_err(|e| Failure::new("FAILED", format!("{e:#}")))?;
    let next = "restart the server (docker restart roastwarden): it moves the current database \
                into before-restore-<time>/ beside it and starts on the copy";
    let answer = json!({
        "staged": staged,
        "from": file,
        "replaced_staged": replaced,
        "schema": found.schema,
        "roasts": found.roasts,
        "profiles": found.profiles,
        "beans": found.beans,
        "next": next,
    });
    out.emit(&answer, |_| {
        format!(
            "staged {} ({} roasts, {} profiles, {} beans); next: {next}",
            file.display(),
            found.roasts,
            found.profiles,
            found.beans
        )
    });
    Ok(())
}

/// A path as given, or else a copy's name in the backup folder.
fn resolve(db: &str, file: &Path) -> Result<PathBuf, Failure> {
    if file.is_file() {
        return Ok(file.to_path_buf());
    }
    let in_dir = backups::dir_for(db).join(file);
    if file.components().count() == 1 && in_dir.is_file() {
        return Ok(in_dir);
    }
    Err(Failure::new(
        "NO_FILE",
        format!(
            "no file {} (nor in {})",
            file.display(),
            backups::dir_for(db).display()
        ),
    ))
}
