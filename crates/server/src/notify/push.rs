//! Web Push (docs/push.md): alarms and system events (docs/notifications.md) reach a phone
//! even when the dashboard is closed or the phone is locked.
//!
//! A browser that turns notifications on registers its push subscription here (the push
//! service's endpoint URL and the browser's P-256 key and auth secret). For each notification
//! (`notify.rs` sends them), the server encrypts a short JSON message for each subscription
//! (RFC 8291, `aes128gcm`) and POSTs it to the endpoint with a VAPID signature (RFC 8292)
//! from the server's own key pair, made on first use and kept in the database. The
//! browser's service worker shows it.
//!
//! Written on RustCrypto rather than a Web Push crate: those bring OpenSSL, which the image
//! does not carry. The encryption is checked against RFC 8291's worked example byte for byte.

use std::fmt::Write as _;
use std::sync::Arc;
use std::time::Duration;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes128Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use hkdf::Hkdf;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use p256::elliptic_curve::sec1::ToSec1Point;
use p256::{PublicKey, SecretKey};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use crate::analysis::alarms::AlarmEvent;

/// Record size written in the header; a push message is one record well under it.
const RECORD_SIZE: u32 = 4096;
/// How long a push service keeps an undelivered alarm (seconds). An alarm is about now.
const TTL_SECS: u32 = 600;
/// VAPID tokens are valid this long (the spec allows at most 24 h).
const VAPID_SECS: i64 = 12 * 3600;
/// Push services accept 4096 bytes of payload; keep the message well under.
const MAX_PLAINTEXT: usize = 3000;

// ------------------------------------------------------------------ crypto --

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).expect("the OS random source failed");
    b
}

fn random_secret() -> SecretKey {
    loop {
        // A random 32-byte string is a valid scalar unless it is zero or above the order.
        if let Ok(k) = SecretKey::from_slice(&random_bytes::<32>()) {
            return k;
        }
    }
}

/// The uncompressed SEC1 form (65 bytes, 0x04 first), as browsers and the RFCs use it.
fn uncompressed(public: &PublicKey) -> Vec<u8> {
    public.to_sec1_point(false).as_bytes().to_vec()
}

fn hkdf(salt: &[u8], ikm: &[u8], info: &[u8], out: &mut [u8]) {
    Hkdf::<Sha256>::new(Some(salt), ikm)
        .expand(info, out)
        .expect("HKDF output length is valid");
}

/// Encrypt `plaintext` for one subscription (RFC 8291 with RFC 8188 `aes128gcm`): one
/// record, the header carrying the salt and the sender's public key. `sender` and `salt` are
/// fresh for every message; tests pass the RFC's.
pub fn encrypt(
    ua_public: &[u8],
    auth_secret: &[u8],
    plaintext: &[u8],
    sender: &SecretKey,
    salt: [u8; 16],
) -> anyhow::Result<Vec<u8>> {
    let ua = PublicKey::from_sec1_bytes(ua_public)
        .map_err(|_| anyhow::anyhow!("the subscription's p256dh key is not a P-256 point"))?;
    if auth_secret.len() != 16 {
        anyhow::bail!("the subscription's auth secret is not 16 bytes");
    }
    let as_public = uncompressed(&sender.public_key());
    let ua_bytes = uncompressed(&ua);
    let shared = p256::ecdh::diffie_hellman(sender.to_nonzero_scalar(), ua.as_affine());

    // IKM = HKDF(auth_secret, ecdh_secret, "WebPush: info" 0x00 ua_public as_public, 32)
    let mut key_info = b"WebPush: info\0".to_vec();
    key_info.extend_from_slice(&ua_bytes);
    key_info.extend_from_slice(&as_public);
    let mut ikm = [0u8; 32];
    hkdf(auth_secret, shared.raw_secret_bytes(), &key_info, &mut ikm);
    let mut cek = [0u8; 16];
    hkdf(&salt, &ikm, b"Content-Encoding: aes128gcm\0", &mut cek);
    let mut nonce = [0u8; 12];
    hkdf(&salt, &ikm, b"Content-Encoding: nonce\0", &mut nonce);

    // The last (only) record ends with the 0x02 delimiter and no padding.
    let mut record = plaintext.to_vec();
    record.push(2);
    let sealed = Aes128Gcm::new_from_slice(&cek)
        .expect("16-byte key")
        .encrypt(&Nonce::from(nonce), record.as_slice())
        .map_err(|_| anyhow::anyhow!("AES-GCM failed"))?;

    let mut out = Vec::with_capacity(86 + sealed.len());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&RECORD_SIZE.to_be_bytes());
    out.push(as_public.len() as u8);
    out.extend_from_slice(&as_public);
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// `Authorization: vapid t=<JWT>, k=<public key>` for a push to `endpoint` (RFC 8292). The
/// JWT's audience is the endpoint's origin; `subject` is a contact (`mailto:` or `https:`).
pub fn vapid_header(
    key: &SecretKey,
    endpoint: &str,
    subject: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<String> {
    let url = reqwest::Url::parse(endpoint)?;
    let aud = url.origin().ascii_serialization();
    let header = B64.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
    let claims = B64.encode(
        serde_json::json!({
            "aud": aud,
            "exp": now.timestamp() + VAPID_SECS,
            "sub": subject,
        })
        .to_string(),
    );
    let signing_input = format!("{header}.{claims}");
    let sig: Signature = SigningKey::from(key).sign(signing_input.as_bytes());
    let jwt = format!("{signing_input}.{}", B64.encode(sig.to_bytes()));
    Ok(format!(
        "vapid t={jwt}, k={}",
        B64.encode(uncompressed(&key.public_key()))
    ))
}

// ------------------------------------------------------------------- store --

/// A browser's push subscription as the dashboard sends it (`PushSubscription.toJSON()`).
#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct NewSubscription {
    pub endpoint: String,
    pub keys: SubscriptionKeys,
    /// What to call this device in the list, e.g. "Chrome on Android".
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct SubscriptionKeys {
    pub p256dh: String,
    pub auth: String,
}

/// A subscription as the API shows it: no endpoint or keys, which would let anyone push to
/// the device.
#[derive(Debug, Clone, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Subscription {
    pub id: i64,
    pub name: String,
    /// The push service's host (fcm.googleapis.com, web.push.apple.com, …).
    pub service: String,
    pub created_at: String,
    pub last_ok_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(sqlx::FromRow)]
struct Target {
    id: i64,
    endpoint: String,
    p256dh: String,
    auth: String,
    subject: String,
}

/// What a notification says; the service worker shows it.
#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub title: String,
    pub body: String,
    /// Notifications with the same tag replace each other.
    pub tag: String,
    /// Opened on a tap.
    pub url: String,
}

impl Message {
    pub fn from_alarm(a: &AlarmEvent) -> Self {
        let mm = (a.elapsed_seconds.max(0.0) / 60.0).floor() as i64;
        let ss = (a.elapsed_seconds.max(0.0) as i64) % 60;
        let mut body = format!("{} · {mm}:{ss:02} into {}", a.message, a.session_name);
        if !a.actions.is_empty() {
            let _ = write!(body, " · {}", a.actions.join(", "));
        }
        Message {
            title: format!("{}: {}", a.device_id, a.name),
            body,
            tag: format!("alarm-{}-{}", a.session_id, a.name),
            url: "/".into(),
        }
    }
}

/// How one delivery ended.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct Delivery {
    pub id: i64,
    pub name: String,
    pub ok: bool,
    /// The push service's status, or the error.
    pub detail: String,
    /// The push service said the subscription is gone; it was removed.
    pub removed: bool,
}

fn now_text() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[derive(Clone)]
pub struct PushService {
    db: SqlitePool,
    http: reqwest::Client,
    key: Arc<Mutex<Option<SecretKey>>>,
}

impl PushService {
    pub fn new(db: SqlitePool) -> Self {
        Self {
            db,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("HTTP client"),
            key: Arc::new(Mutex::new(None)),
        }
    }

    /// The server's VAPID key, made and stored on first use.
    async fn key(&self) -> anyhow::Result<SecretKey> {
        let mut cached = self.key.lock().await;
        if let Some(k) = cached.as_ref() {
            return Ok(k.clone());
        }
        let stored: Option<String> =
            sqlx::query_scalar("SELECT private_key FROM push_vapid WHERE id = 1")
                .fetch_optional(&self.db)
                .await?;
        let key = match stored {
            Some(s) => SecretKey::from_slice(&B64.decode(s)?)
                .map_err(|_| anyhow::anyhow!("the stored VAPID key is not valid"))?,
            None => {
                let k = random_secret();
                // INSERT OR IGNORE, then read back: two first uses at once agree on one key.
                sqlx::query(
                    "INSERT OR IGNORE INTO push_vapid (id, private_key, public_key, created_at)
                     VALUES (1, ?, ?, ?)",
                )
                .bind(B64.encode(k.to_bytes()))
                .bind(B64.encode(uncompressed(&k.public_key())))
                .bind(now_text())
                .execute(&self.db)
                .await?;
                let s: String =
                    sqlx::query_scalar("SELECT private_key FROM push_vapid WHERE id = 1")
                        .fetch_one(&self.db)
                        .await?;
                SecretKey::from_slice(&B64.decode(s)?)
                    .map_err(|_| anyhow::anyhow!("the stored VAPID key is not valid"))?
            }
        };
        *cached = Some(key.clone());
        Ok(key)
    }

    /// The public key a browser subscribes with (`applicationServerKey`), base64url.
    pub async fn public_key(&self) -> anyhow::Result<String> {
        Ok(B64.encode(uncompressed(&self.key().await?.public_key())))
    }

    pub async fn list(&self) -> anyhow::Result<Vec<Subscription>> {
        Ok(sqlx::query_as::<_, Subscription>(
            "SELECT id, name, service, created_at, last_ok_at, last_error
             FROM push_subscriptions ORDER BY created_at",
        )
        .fetch_all(&self.db)
        .await?)
    }

    /// Store a subscription (again: the same endpoint updates its keys and name). `origin` is
    /// the dashboard's address, the contact in the VAPID token.
    pub async fn subscribe(
        &self,
        s: NewSubscription,
        origin: Option<String>,
    ) -> Result<Subscription, String> {
        let url = reqwest::Url::parse(&s.endpoint)
            .map_err(|_| "the endpoint is not a URL".to_string())?;
        if url.scheme() != "https" {
            return Err("the endpoint must be https".into());
        }
        let host = url.host_str().unwrap_or_default().to_string();
        let p256dh = B64
            .decode(s.keys.p256dh.trim_end_matches('='))
            .map_err(|_| "keys.p256dh is not base64url".to_string())?;
        PublicKey::from_sec1_bytes(&p256dh)
            .map_err(|_| "keys.p256dh is not a P-256 public key".to_string())?;
        let auth = B64
            .decode(s.keys.auth.trim_end_matches('='))
            .map_err(|_| "keys.auth is not base64url".to_string())?;
        if auth.len() != 16 {
            return Err("keys.auth must be 16 bytes".into());
        }
        let name = s
            .name
            .map(|n| n.trim().chars().take(80).collect::<String>())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| host.clone());
        // Apple wants an https: or mailto: subject; the dashboard's own https origin is one.
        let subject = origin
            .filter(|o| o.starts_with("https://"))
            .unwrap_or_else(|| "mailto:roastwarden@users.noreply.invalid".into());
        let mut rows = sqlx::query_as::<_, Subscription>(
            "INSERT INTO push_subscriptions
               (endpoint, p256dh, auth, name, service, subject, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(endpoint) DO UPDATE SET
               p256dh = excluded.p256dh, auth = excluded.auth, name = excluded.name,
               subject = excluded.subject, last_error = NULL
             RETURNING id, name, service, created_at, last_ok_at, last_error",
        )
        .bind(&s.endpoint)
        .bind(B64.encode(&p256dh))
        .bind(B64.encode(&auth))
        .bind(&name)
        .bind(&host)
        .bind(&subject)
        .bind(now_text())
        .fetch_all(&self.db)
        .await
        .map_err(|e| e.to_string())?;
        rows.pop()
            .ok_or_else(|| "the insert returned no row".into())
    }

    pub async fn remove(&self, id: i64) -> anyhow::Result<bool> {
        let r = sqlx::query("DELETE FROM push_subscriptions WHERE id = ?")
            .bind(id)
            .execute(&self.db)
            .await?;
        Ok(r.rows_affected() > 0)
    }

    /// Remove the subscription with this endpoint (a browser turning notifications off).
    pub async fn remove_endpoint(&self, endpoint: &str) -> anyhow::Result<bool> {
        let r = sqlx::query("DELETE FROM push_subscriptions WHERE endpoint = ?")
            .bind(endpoint)
            .execute(&self.db)
            .await?;
        Ok(r.rows_affected() > 0)
    }

    /// Send `msg` to every subscription (or the one with `only`), at once.
    pub async fn send(&self, msg: &Message, only: Option<i64>) -> anyhow::Result<Vec<Delivery>> {
        let key = self.key().await?;
        let targets = sqlx::query_as::<_, Target>(
            "SELECT id, endpoint, p256dh, auth, subject FROM push_subscriptions
             WHERE ?1 IS NULL OR id = ?1",
        )
        .bind(only)
        .fetch_all(&self.db)
        .await?;
        let mut body = serde_json::to_vec(msg)?;
        if body.len() > MAX_PLAINTEXT {
            let mut short = msg.clone();
            short.body = short.body.chars().take(500).collect();
            body = serde_json::to_vec(&short)?;
        }
        let names: std::collections::HashMap<i64, String> = self
            .list()
            .await?
            .into_iter()
            .map(|s| (s.id, s.name))
            .collect();
        let sends = targets.into_iter().map(|t| {
            let (key, body, this) = (key.clone(), body.clone(), self.clone());
            let name = names.get(&t.id).cloned().unwrap_or_default();
            async move {
                let d = this.deliver(&key, &t, &body, &msg.tag).await;
                this.record(&t, &d).await;
                Delivery { name, ..d }
            }
        });
        Ok(futures_util::future::join_all(sends).await)
    }

    async fn deliver(&self, key: &SecretKey, t: &Target, body: &[u8], tag: &str) -> Delivery {
        let fail = |detail: String, removed: bool| Delivery {
            id: t.id,
            name: String::new(),
            ok: false,
            detail,
            removed,
        };
        let (Ok(p256dh), Ok(auth)) = (B64.decode(&t.p256dh), B64.decode(&t.auth)) else {
            return fail("stored keys are not base64url".into(), false);
        };
        let payload = match encrypt(&p256dh, &auth, body, &random_secret(), random_bytes()) {
            Ok(p) => p,
            Err(e) => return fail(e.to_string(), false),
        };
        let vapid = match vapid_header(key, &t.endpoint, &t.subject, chrono::Utc::now()) {
            Ok(v) => v,
            Err(e) => return fail(e.to_string(), false),
        };
        // Topic lets the push service replace an undelivered message with the same one.
        let topic: String = tag
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .take(32)
            .collect();
        let res = self
            .http
            .post(&t.endpoint)
            .header("TTL", TTL_SECS.to_string())
            .header("Urgency", "high")
            .header("Topic", topic)
            .header("Content-Encoding", "aes128gcm")
            .header("Content-Type", "application/octet-stream")
            .header("Authorization", vapid)
            .body(payload)
            .send()
            .await;
        match res {
            Ok(r) if r.status().is_success() => Delivery {
                id: t.id,
                name: String::new(),
                ok: true,
                detail: r.status().to_string(),
                removed: false,
            },
            Ok(r) => {
                let status = r.status();
                let text = r.text().await.unwrap_or_default();
                let gone = matches!(status.as_u16(), 404 | 410);
                fail(
                    format!("{status} {}", text.chars().take(200).collect::<String>())
                        .trim()
                        .to_string(),
                    gone,
                )
            }
            Err(e) => fail(e.to_string(), false),
        }
    }

    async fn record(&self, t: &Target, d: &Delivery) {
        let q = if d.removed {
            sqlx::query("DELETE FROM push_subscriptions WHERE id = ?").bind(t.id)
        } else if d.ok {
            sqlx::query(
                "UPDATE push_subscriptions SET last_ok_at = ?, last_error = NULL WHERE id = ?",
            )
            .bind(now_text())
            .bind(t.id)
        } else {
            sqlx::query("UPDATE push_subscriptions SET last_error = ? WHERE id = ?")
                .bind(&d.detail)
                .bind(t.id)
        };
        if let Err(e) = q.execute(&self.db).await {
            tracing::warn!(error = %e, "could not record a push delivery");
        }
        if !d.ok {
            tracing::warn!(subscription = t.id, detail = %d.detail, removed = d.removed, "push not delivered");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b64(s: &str) -> Vec<u8> {
        B64.decode(s.replace([' ', '\n'], "")).unwrap()
    }

    /// RFC 8291 section 5 and appendix A, byte for byte.
    #[test]
    fn encryption_matches_the_rfc_example() {
        let sender =
            SecretKey::from_slice(&b64("yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw")).unwrap();
        assert_eq!(
            uncompressed(&sender.public_key()),
            b64(
                "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8"
            )
        );
        let ua_public = b64(
            "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4",
        );
        let auth = b64("BTBZMqHH6r4Tts7J_aSIgg");
        let salt: [u8; 16] = b64("DGv6ra1nlYgDCS1FRnbzlw").try_into().unwrap();
        let out = encrypt(
            &ua_public,
            &auth,
            b"When I grow up, I want to be a watermelon",
            &sender,
            salt,
        )
        .unwrap();
        let expected = b64(
            "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27ml
             mlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPT
             pK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN",
        );
        // 86-byte header + 41 + 1 delimiter + 16 tag. (The example's "Content-Length: 145" is
        // one off; its body and appendix A both give 144.)
        assert_eq!(out.len(), 144);
        assert_eq!(B64.encode(&out), B64.encode(&expected));
        assert_eq!(
            B64.encode(&out[86..]),
            "8pfeW0KbunFT06SuDKoJH9Ql87S1QUrdirN6GcG7sFz1y1sqLgVi1VhjVkHsUoEsbI_0LpXMuGvnzQ"
        );
    }

    #[test]
    fn bad_subscription_keys_are_refused() {
        let k = random_secret();
        assert!(encrypt(&[4; 65], &[0; 16], b"x", &k, [0; 16]).is_err());
        let ua = uncompressed(&random_secret().public_key());
        assert!(encrypt(&ua, &[0; 15], b"x", &k, [0; 16]).is_err());
        assert!(encrypt(&ua, &[0; 16], b"x", &k, [0; 16]).is_ok());
    }

    #[test]
    fn vapid_tokens_are_signed_for_the_endpoint_origin() {
        use p256::ecdsa::signature::Verifier;
        let key = random_secret();
        let now = chrono::DateTime::parse_from_rfc3339("2026-10-24T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let h = vapid_header(
            &key,
            "https://fcm.googleapis.com/fcm/send/abc:def",
            "https://roastwarden.example",
            now,
        )
        .unwrap();
        let rest = h.strip_prefix("vapid t=").unwrap();
        let (jwt, k) = rest.split_once(", k=").unwrap();
        assert_eq!(b64(k), uncompressed(&key.public_key()));
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
        let claims: serde_json::Value = serde_json::from_slice(&b64(parts[1])).unwrap();
        assert_eq!(claims["aud"], "https://fcm.googleapis.com");
        assert_eq!(claims["sub"], "https://roastwarden.example");
        assert_eq!(claims["exp"], now.timestamp() + VAPID_SECS);
        let sig = Signature::from_slice(&b64(parts[2])).unwrap();
        p256::ecdsa::VerifyingKey::from(key.public_key())
            .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &sig)
            .unwrap();
    }

    #[test]
    fn alarm_messages_read_well() {
        let a = AlarmEvent {
            device_id: "esp32_roaster_01".into(),
            session_id: "s1".into(),
            session_name: "Batch 3".into(),
            name: "BT high".into(),
            condition_type: "temp_above".into(),
            threshold: 230.0,
            message: "BT above 230 °C".into(),
            value: Some(231.0),
            elapsed_seconds: 605.4,
            fired_at: chrono::Utc::now(),
            actions: vec!["heater off".into()],
        };
        let m = Message::from_alarm(&a);
        assert_eq!(m.title, "esp32_roaster_01: BT high");
        assert_eq!(m.body, "BT above 230 °C · 10:05 into Batch 3 · heater off");
        assert_eq!(m.tag, "alarm-s1-BT high");
    }

    /// The receiving side of RFC 8291, as a browser does it.
    fn decrypt(ua: &SecretKey, auth: &[u8], body: &[u8]) -> Vec<u8> {
        let salt = &body[..16];
        assert_eq!(
            u32::from_be_bytes(body[16..20].try_into().unwrap()),
            RECORD_SIZE
        );
        assert_eq!(body[20], 65);
        let as_public = PublicKey::from_sec1_bytes(&body[21..86]).unwrap();
        let shared = p256::ecdh::diffie_hellman(ua.to_nonzero_scalar(), as_public.as_affine());
        let mut info = b"WebPush: info\0".to_vec();
        info.extend_from_slice(&uncompressed(&ua.public_key()));
        info.extend_from_slice(&body[21..86]);
        let mut ikm = [0u8; 32];
        hkdf(auth, shared.raw_secret_bytes(), &info, &mut ikm);
        let (mut cek, mut nonce) = ([0u8; 16], [0u8; 12]);
        hkdf(salt, &ikm, b"Content-Encoding: aes128gcm\0", &mut cek);
        hkdf(salt, &ikm, b"Content-Encoding: nonce\0", &mut nonce);
        let mut plain = Aes128Gcm::new_from_slice(&cek)
            .unwrap()
            .decrypt(&Nonce::from(nonce), &body[86..])
            .unwrap();
        assert_eq!(plain.pop(), Some(2), "last-record delimiter");
        plain
    }

    async fn test_db() -> (SqlitePool, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("roastwarden-push-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = crate::db::open(dir.join("t.db").to_str().unwrap())
            .await
            .unwrap();
        (db, dir)
    }

    /// An alarm goes out to a (local) push service, which a browser could decrypt; a
    /// subscription the service says is gone is removed.
    #[tokio::test]
    async fn alarms_are_delivered_encrypted_and_dead_subscriptions_pruned() {
        use axum::{Router, body::Bytes, extract::State, http::HeaderMap, routing::post};
        type Seen = Arc<Mutex<Vec<(HeaderMap, Bytes)>>>;
        let seen: Seen = Arc::default();
        let app = Router::new()
            .route(
                "/push/live",
                post(|State(s): State<Seen>, h: HeaderMap, b: Bytes| async move {
                    s.lock().await.push((h, b));
                    axum::http::StatusCode::CREATED
                }),
            )
            .route(
                "/push/gone",
                post(|| async { axum::http::StatusCode::GONE }),
            )
            .with_state(seen.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let (db, dir) = test_db().await;
        let push = PushService::new(db.clone());
        let ua = random_secret();
        let auth = [9u8; 16];
        for (path, name) in [("live", "phone"), ("gone", "old laptop")] {
            sqlx::query(
                "INSERT INTO push_subscriptions
                   (endpoint, p256dh, auth, name, service, subject, created_at)
                 VALUES (?, ?, ?, ?, '127.0.0.1', 'https://rr.example', '2026-10-24')",
            )
            .bind(format!("http://{addr}/push/{path}"))
            .bind(B64.encode(uncompressed(&ua.public_key())))
            .bind(B64.encode(auth))
            .bind(name)
            .execute(&db)
            .await
            .unwrap();
        }
        let msg = Message {
            title: "virtual_roaster: BT high".into(),
            body: "BT above 230 °C".into(),
            tag: "alarm-s1-BT high".into(),
            url: "/".into(),
        };
        let mut d = push.send(&msg, None).await.unwrap();
        d.sort_by_key(|x| x.id);
        assert!(d[0].ok && d[0].name == "phone", "{d:?}");
        assert!(!d[1].ok && d[1].removed, "{d:?}");

        let seen = seen.lock().await;
        let (h, body) = &seen[0];
        assert_eq!(h["content-encoding"], "aes128gcm");
        assert_eq!(h["ttl"], TTL_SECS.to_string());
        assert_eq!(h["topic"], "alarm-s1-BThigh");
        let vapid = h["authorization"].to_str().unwrap();
        assert!(vapid.starts_with("vapid t="), "{vapid}");
        let got: serde_json::Value = serde_json::from_slice(&decrypt(&ua, &auth, body)).unwrap();
        assert_eq!(got["title"], "virtual_roaster: BT high");
        assert_eq!(got["body"], "BT above 230 °C");

        let left = push.list().await.unwrap();
        assert_eq!(left.len(), 1, "the gone one is removed");
        assert!(left[0].last_ok_at.is_some());

        // The Notifications page shows the delivery, also after a restart (a new notifier),
        // and a test from This browser that no device took is the channel's error.
        let n = crate::notify::Notifier::new(db.clone(), push.clone());
        let st = n.statuses().await;
        assert_eq!(
            st[&crate::notify::Channel::Push].last_ok_at,
            left[0].last_ok_at
        );
        n.record_push(&[]).await;
        let st = n.statuses().await;
        assert!(
            st[&crate::notify::Channel::Push]
                .last_error
                .as_deref()
                .is_some_and(|e| e.starts_with("No device"))
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn subscriptions_are_checked_stored_and_replaced_by_endpoint() {
        let (db, dir) = test_db().await;
        let push = PushService::new(db);
        let key = push.public_key().await.unwrap();
        assert_eq!(key, push.public_key().await.unwrap(), "one key, kept");
        assert_eq!(b64(&key).len(), 65);

        let ua = B64.encode(uncompressed(&random_secret().public_key()));
        let sub = |endpoint: &str, p256dh: &str, auth: &str| NewSubscription {
            endpoint: endpoint.into(),
            keys: SubscriptionKeys {
                p256dh: p256dh.into(),
                auth: auth.into(),
            },
            name: Some("Firefox on Android".into()),
        };
        let auth = B64.encode([7u8; 16]);
        assert!(
            push.subscribe(sub("http://push.example/x", &ua, &auth), None)
                .await
                .is_err()
        );
        assert!(
            push.subscribe(sub("https://push.example/x", "AAAA", &auth), None)
                .await
                .is_err()
        );
        assert!(
            push.subscribe(sub("https://push.example/x", &ua, "AAAA"), None)
                .await
                .is_err()
        );
        let a = push
            .subscribe(
                sub("https://push.example/x", &ua, &auth),
                Some("https://rr.example".into()),
            )
            .await
            .unwrap();
        assert_eq!(a.service, "push.example");
        let b = push
            .subscribe(sub("https://push.example/x", &ua, &auth), None)
            .await
            .unwrap();
        assert_eq!(a.id, b.id, "same endpoint, same row");
        assert_eq!(push.list().await.unwrap().len(), 1);
        assert!(
            push.remove_endpoint("https://push.example/x")
                .await
                .unwrap()
        );
        assert!(push.list().await.unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
