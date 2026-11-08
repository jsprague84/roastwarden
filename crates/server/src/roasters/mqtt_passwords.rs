//! Roasters' MQTT passwords (docs/add-roaster.md, "MQTT passwords"): who may sign in to the
//! built-in broker (broker.rs) as a roaster. Two sources: `MQTT_DEVICES` in the environment,
//! read at start and fixed there, and the ones added from the dashboard or `roast roaster
//! password`, kept in the database (migration 060). Those take effect at once: the broker's
//! sign-in check reads this list on every connect, and a changed or removed password signs that
//! roaster out.
//!
//! The passwords are kept as written, as `MQTT_DEVICES` keeps them: the firmware download
//! (firmware.rs) fills a roaster's into its sketch. The API never lists them; adding one or
//! giving a new one answers with it once.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::{Arc, OnceLock, RwLock};

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use utoipa::ToSchema;

/// Where a roaster's password comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PasswordSource {
    /// `MQTT_DEVICES` in the server's environment (`.env`): changed there, with a restart
    Env,
    /// Added from the dashboard or `roast roaster password`: changed there, at once
    Dashboard,
}

/// A roaster that may sign in to the built-in broker. Never its password.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MqttPassword {
    pub roaster_id: String,
    pub source: PasswordSource,
    /// When it was added (a dashboard one)
    pub created_at: Option<String>,
    /// When its password last changed (a dashboard one)
    pub changed_at: Option<String>,
    /// Connected to the broker now
    pub signed_in: bool,
    /// Set up under Roasters already
    pub added: bool,
}

/// The roasters that may sign in, and whether this server can add one.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MqttPasswordList {
    /// The server runs its own MQTT broker (`ROASTWARDEN_BROKER_ADDR`). Without it a roaster's
    /// user and password belong to the broker the server uses, and none can be added here.
    pub built_in_broker: bool,
    pub passwords: Vec<MqttPassword>,
}

/// A roaster's password, answered once: when it is added or changed.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MqttPasswordGiven {
    pub roaster_id: String,
    pub password: String,
}

/// Add a roaster's password.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct NewMqttPassword {
    /// Its id: its MQTT user and its topics (`roaster/<id>/…`). Letters, digits, `_` and `-`.
    pub roaster_id: String,
    /// Left out: a new random one (32 characters)
    #[serde(default)]
    pub password: Option<String>,
}

/// A roaster's new password.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
pub struct ChangeMqttPassword {
    /// Left out: a new random one (32 characters)
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug)]
pub enum PasswordError {
    NoBuiltInBroker,
    Invalid(String),
    Exists(String),
    FromEnv(String),
    NotFound(String),
    Db(sqlx::Error),
}

impl std::fmt::Display for PasswordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoBuiltInBroker => write!(
                f,
                "this server uses an MQTT broker of its own (MQTT_BROKER_HOST): add the \
                 roaster's user and password there"
            ),
            Self::Invalid(m) => write!(f, "{m}"),
            Self::Exists(id) => write!(f, "{id} has a password already: give it a new one instead"),
            Self::FromEnv(id) => write!(
                f,
                "{id}'s password is in MQTT_DEVICES (the server's .env): change it there and restart"
            ),
            Self::NotFound(id) => write!(f, "{id} has no password here"),
            Self::Db(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for PasswordError {}

impl From<sqlx::Error> for PasswordError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e)
    }
}

#[derive(Debug, Clone)]
struct Stored {
    password: String,
    created_at: String,
    changed_at: String,
}

/// The roasters' passwords, shared by the broker's sign-in check and the API.
#[derive(Clone)]
pub struct MqttPasswords {
    inner: Arc<Inner>,
}

struct Inner {
    db: SqlitePool,
    built_in: bool,
    /// `MQTT_DEVICES`, as read at start
    env: Vec<(String, String)>,
    /// The database's, by roaster id
    stored: RwLock<HashMap<String, Stored>>,
    /// The running broker, to see who is signed in and sign a roaster out
    broker: OnceLock<crate::ops::broker::Handle>,
}

/// The longest roaster id (a topic level, the sketch's `ROASTER_ID`)
const MAX_ID: usize = 48;

/// A roaster id: letters, digits, `_` and `-`, not the server's or Home Assistant's user.
pub fn check_id(id: &str) -> Result<(), PasswordError> {
    if id.is_empty()
        || id.len() > MAX_ID
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(PasswordError::Invalid(format!(
            "roaster_id: letters, digits, _ and - only, at most {MAX_ID}"
        )));
    }
    if id == crate::ops::broker::SERVER_USER || id == crate::notify::home_assistant::HA_USER {
        return Err(PasswordError::Invalid(format!(
            "roaster_id: \"{id}\" is taken by the broker itself"
        )));
    }
    Ok(())
}

/// A password: 8 to 128 characters, printable ASCII without spaces, quotes or backslashes,
/// so it goes into a sketch, `.env` and a shell unchanged.
pub fn check_password(pw: &str) -> Result<(), PasswordError> {
    if !(8..=128).contains(&pw.len())
        || !pw
            .bytes()
            .all(|b| (0x21..=0x7e).contains(&b) && !matches!(b, b'"' | b'\'' | b'\\' | b'`'))
    {
        return Err(PasswordError::Invalid(
            "password: 8 to 128 characters, no spaces, quotes or backslashes".into(),
        ));
    }
    Ok(())
}

/// A new random password: 32 hex characters (128 bits)
pub fn random_password() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("random bytes");
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

impl MqttPasswords {
    /// The stored passwords and `env` (`MQTT_DEVICES`). `built_in`: the server runs its own
    /// broker; without it nothing can be added.
    pub async fn load(
        db: SqlitePool,
        env: Vec<(String, String)>,
        built_in: bool,
    ) -> Result<Self, sqlx::Error> {
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT roaster_id, password, created_at, changed_at FROM mqtt_passwords",
        )
        .fetch_all(&db)
        .await?;
        let mut stored = HashMap::new();
        // Without the built-in broker the stored ones sign nobody in: the broker in use has
        // its own users, so neither the list nor the firmware offers them
        for (id, password, created_at, changed_at) in rows.into_iter().filter(|_| built_in) {
            if env.iter().any(|(e, _)| *e == id) {
                tracing::warn!(
                    roaster = %id,
                    "MQTT_DEVICES names this roaster too: its password there is the one used"
                );
            }
            stored.insert(
                id,
                Stored {
                    password,
                    created_at,
                    changed_at,
                },
            );
        }
        Ok(Self {
            inner: Arc::new(Inner {
                db,
                built_in,
                env,
                stored: RwLock::new(stored),
                broker: OnceLock::new(),
            }),
        })
    }

    /// The broker is running: who is signed in, and signing a roaster out
    pub fn attach(&self, broker: crate::ops::broker::Handle) {
        let _ = self.inner.broker.set(broker);
    }

    pub fn built_in(&self) -> bool {
        self.inner.built_in
    }

    /// A sign-in as `user`: `None` when it is not a roaster's (the server's, Home Assistant's,
    /// a stranger's: the broker's own rules decide), else whether the password is right.
    pub fn check(&self, user: &str, password: &[u8]) -> Option<bool> {
        let want = self.password(user)?;
        Some(constant_eq(want.as_bytes(), password))
    }

    /// A roaster's password: `MQTT_DEVICES`', else a stored one
    pub fn password(&self, id: &str) -> Option<String> {
        if let Some((_, p)) = self.inner.env.iter().find(|(d, _)| d == id) {
            return Some(p.clone());
        }
        self.inner
            .stored
            .read()
            .expect("the MQTT passwords lock is poisoned")
            .get(id)
            .map(|s| s.password.clone())
    }

    /// Every roaster with a password, `MQTT_DEVICES`' first, then by id
    pub fn ids(&self) -> Vec<(String, PasswordSource)> {
        let mut out: Vec<(String, PasswordSource)> = self
            .inner
            .env
            .iter()
            .map(|(d, _)| (d.clone(), PasswordSource::Env))
            .collect();
        let stored = self
            .inner
            .stored
            .read()
            .expect("the MQTT passwords lock is poisoned");
        let mut more: Vec<&String> = stored
            .keys()
            .filter(|id| !out.iter().any(|(d, _)| d == *id))
            .collect();
        more.sort();
        out.extend(
            more.into_iter()
                .map(|id| (id.clone(), PasswordSource::Dashboard)),
        );
        out
    }

    /// The list for the API; `added`: the roasters set up under Roasters
    pub async fn list(&self, added: &[String]) -> MqttPasswordList {
        let signed_in = match self.inner.broker.get() {
            Some(b) => b.signed_in().await,
            None => std::collections::HashSet::new(),
        };
        let stored = self
            .inner
            .stored
            .read()
            .expect("the MQTT passwords lock is poisoned")
            .clone();
        MqttPasswordList {
            built_in_broker: self.inner.built_in,
            passwords: self
                .ids()
                .into_iter()
                .map(|(id, source)| {
                    let s = (source == PasswordSource::Dashboard)
                        .then(|| stored.get(&id))
                        .flatten();
                    MqttPassword {
                        created_at: s.map(|s| s.created_at.clone()),
                        changed_at: s.map(|s| s.changed_at.clone()),
                        signed_in: signed_in.contains(&id),
                        added: added.contains(&id),
                        roaster_id: id,
                        source,
                    }
                })
                .collect(),
        }
    }

    fn writable(&self, id: &str) -> Result<(), PasswordError> {
        if !self.inner.built_in {
            return Err(PasswordError::NoBuiltInBroker);
        }
        check_id(id)?;
        if self.inner.env.iter().any(|(d, _)| d == id) {
            return Err(PasswordError::FromEnv(id.to_string()));
        }
        Ok(())
    }

    fn chosen(password: Option<String>) -> Result<String, PasswordError> {
        match password.filter(|p| !p.is_empty()) {
            Some(p) => {
                check_password(&p)?;
                Ok(p)
            }
            None => Ok(random_password()),
        }
    }

    /// A new roaster's password (`None`: a random one); it may sign in at once.
    pub async fn add(&self, id: &str, password: Option<String>) -> Result<String, PasswordError> {
        self.writable(id)?;
        let pw = Self::chosen(password)?;
        if self
            .inner
            .stored
            .read()
            .expect("the MQTT passwords lock is poisoned")
            .contains_key(id)
        {
            return Err(PasswordError::Exists(id.to_string()));
        }
        let t = now();
        let r = sqlx::query(
            "INSERT INTO mqtt_passwords (roaster_id, password, created_at, changed_at) \
             VALUES (?, ?, ?, ?) ON CONFLICT (roaster_id) DO NOTHING",
        )
        .bind(id)
        .bind(&pw)
        .bind(&t)
        .bind(&t)
        .execute(&self.inner.db)
        .await?;
        if r.rows_affected() == 0 {
            return Err(PasswordError::Exists(id.to_string()));
        }
        self.inner
            .stored
            .write()
            .expect("the MQTT passwords lock is poisoned")
            .insert(
                id.to_string(),
                Stored {
                    password: pw.clone(),
                    created_at: t.clone(),
                    changed_at: t,
                },
            );
        tracing::info!(roaster = %id, "MQTT password added");
        Ok(pw)
    }

    /// A roaster's new password (`None`: a random one). Signed in with the old one, it is
    /// signed out and must come back with the new one.
    pub async fn change(
        &self,
        id: &str,
        password: Option<String>,
    ) -> Result<String, PasswordError> {
        self.writable(id)?;
        let pw = Self::chosen(password)?;
        let t = now();
        let r = sqlx::query(
            "UPDATE mqtt_passwords SET password = ?, changed_at = ? WHERE roaster_id = ?",
        )
        .bind(&pw)
        .bind(&t)
        .bind(id)
        .execute(&self.inner.db)
        .await?;
        if r.rows_affected() == 0 {
            return Err(PasswordError::NotFound(id.to_string()));
        }
        if let Some(s) = self
            .inner
            .stored
            .write()
            .expect("the MQTT passwords lock is poisoned")
            .get_mut(id)
        {
            s.password.clone_from(&pw);
            s.changed_at = t;
        }
        self.sign_out(id).await;
        tracing::info!(roaster = %id, "MQTT password changed");
        Ok(pw)
    }

    /// A roaster's password removed: it can no longer sign in, and is signed out now.
    pub async fn remove(&self, id: &str) -> Result<(), PasswordError> {
        self.writable(id)?;
        let r = sqlx::query("DELETE FROM mqtt_passwords WHERE roaster_id = ?")
            .bind(id)
            .execute(&self.inner.db)
            .await?;
        if r.rows_affected() == 0 {
            return Err(PasswordError::NotFound(id.to_string()));
        }
        self.inner
            .stored
            .write()
            .expect("the MQTT passwords lock is poisoned")
            .remove(id);
        self.sign_out(id).await;
        tracing::info!(roaster = %id, "MQTT password removed");
        Ok(())
    }

    async fn sign_out(&self, id: &str) {
        if let Some(b) = self.inner.broker.get() {
            let n = b.sign_out(id).await;
            if n > 0 {
                tracing::info!(roaster = %id, clients = n, "signed out of the MQTT broker");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn db() -> (SqlitePool, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("rw-mqttpw-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = crate::db::open(&dir.join("t.db").to_string_lossy())
            .await
            .unwrap();
        (db, dir)
    }

    #[test]
    fn ids_and_passwords_are_checked() {
        for ok in ["esp32_roaster_01", "a", "drum-2"] {
            assert!(check_id(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "a/b",
            "a+b",
            "a#",
            "with space",
            "roastwarden",
            "homeassistant",
        ] {
            assert!(check_id(bad).is_err(), "{bad}");
        }
        assert!(check_id(&"x".repeat(49)).is_err());
        assert!(check_password("abcdefgh").is_ok());
        assert!(check_password("S3cret!#%&*+=?.,:;").is_ok());
        for bad in [
            "short",
            "has space1",
            "quo\"te12",
            "back\\slash",
            "tick'ed12",
        ] {
            assert!(check_password(bad).is_err(), "{bad}");
        }
        assert_eq!(random_password().len(), 32);
        assert_ne!(random_password(), random_password());
    }

    #[tokio::test]
    async fn added_changed_and_removed_at_once_and_kept() {
        let (db, dir) = db().await;
        let env = vec![("env_roaster".to_string(), "env-pw-123".to_string())];
        let p = MqttPasswords::load(db.clone(), env.clone(), true)
            .await
            .unwrap();
        assert_eq!(p.check("env_roaster", b"env-pw-123"), Some(true));
        assert_eq!(p.check("env_roaster", b"nope"), Some(false));
        assert_eq!(
            p.check("roastwarden", b"x"),
            None,
            "the server's own: the ACL decides"
        );

        let pw = p.add("new_one", None).await.unwrap();
        assert_eq!(pw.len(), 32);
        assert_eq!(p.check("new_one", pw.as_bytes()), Some(true));
        assert!(matches!(
            p.add("new_one", None).await,
            Err(PasswordError::Exists(_))
        ));
        assert!(matches!(
            p.add("env_roaster", None).await,
            Err(PasswordError::FromEnv(_))
        ));
        assert!(matches!(
            p.add("x", Some("short".into())).await,
            Err(PasswordError::Invalid(_))
        ));
        let given = p.add("given", Some("my-own-pass".into())).await.unwrap();
        assert_eq!(given, "my-own-pass");

        let new = p.change("new_one", None).await.unwrap();
        assert_ne!(new, pw);
        assert_eq!(p.check("new_one", pw.as_bytes()), Some(false));
        assert_eq!(p.check("new_one", new.as_bytes()), Some(true));
        assert!(matches!(
            p.change("env_roaster", None).await,
            Err(PasswordError::FromEnv(_))
        ));
        assert!(matches!(
            p.change("nobody", None).await,
            Err(PasswordError::NotFound(_))
        ));

        // Kept across a restart
        let again = MqttPasswords::load(db.clone(), env.clone(), true)
            .await
            .unwrap();
        assert_eq!(again.check("new_one", new.as_bytes()), Some(true));
        let ids: Vec<_> = again.ids();
        assert_eq!(
            ids,
            vec![
                ("env_roaster".to_string(), PasswordSource::Env),
                ("given".to_string(), PasswordSource::Dashboard),
                ("new_one".to_string(), PasswordSource::Dashboard),
            ]
        );
        let list = again.list(&["given".to_string()]).await;
        assert!(list.built_in_broker);
        assert!(list.passwords[1].added && !list.passwords[2].added);
        assert!(list.passwords[0].created_at.is_none() && list.passwords[1].created_at.is_some());

        again.remove("new_one").await.unwrap();
        assert_eq!(again.check("new_one", new.as_bytes()), None);
        assert!(matches!(
            again.remove("new_one").await,
            Err(PasswordError::NotFound(_))
        ));

        // Without the built-in broker nothing is added, the environment's still read
        let outside = MqttPasswords::load(db, env, false).await.unwrap();
        assert!(matches!(
            outside.add("other", None).await,
            Err(PasswordError::NoBuiltInBroker)
        ));
        assert_eq!(
            outside.password("env_roaster").as_deref(),
            Some("env-pw-123")
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
