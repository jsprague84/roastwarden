//! Notification channels (`docs/notifications.md`): every fired alarm and every system event
//! goes to Web Push and to each channel that is turned on: Gotify (and rstify), ntfy, a
//! Matrix room and a webhook.
//!
//! ntfy is published as JSON, so titles keep their "°C" (HTTP headers carry ASCII only) and
//! a public topic is not sent an empty bearer token. A notification fans out to every enabled
//! channel, not to one provider.
//!
//! System events are sent once when a problem starts and once when it clears; the state is kept in
//! `notify_state` so a restart neither repeats a problem nor forgets one.

pub(crate) mod home_assistant;
pub(crate) mod push;
pub(crate) mod watch;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Serialize;
use sqlx::SqlitePool;
use tokio::sync::{Mutex, broadcast};

use crate::analysis::alarms::AlarmEvent;
use crate::auth::settings;
use push::PushService;

/// How long one channel may take before the send counts as failed.
const SEND_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low,
    Default,
    High,
}

impl Priority {
    /// Gotify: 0–10; Android shows 8 and up as a heads-up notification.
    fn gotify(self) -> u8 {
        match self {
            Priority::Low => 2,
            Priority::Default => 5,
            Priority::High => 8,
        }
    }

    /// ntfy: 1 (min) to 5 (max).
    fn ntfy(self) -> u8 {
        match self {
            Priority::Low => 2,
            Priority::Default => 3,
            Priority::High => 5,
        }
    }
}

/// One notification, as every channel sends it.
#[derive(Debug, Clone)]
pub struct Note {
    pub title: String,
    pub body: String,
    /// Push notifications with the same tag replace each other.
    pub tag: String,
    /// Dashboard path a tap opens.
    pub url: String,
    pub priority: Priority,
    /// What the webhook receives.
    pub payload: serde_json::Value,
}

impl Note {
    pub fn from_alarm(a: &AlarmEvent) -> Self {
        let m = push::Message::from_alarm(a);
        Note {
            title: m.title,
            body: m.body,
            tag: m.tag,
            url: m.url,
            priority: Priority::High,
            payload: serde_json::json!({ "type": "roast_alarm", "alarm": a }),
        }
    }

    pub fn test() -> Self {
        Note {
            title: "Roastwarden test".into(),
            body: "Notifications reach this channel.".into(),
            tag: "test".into(),
            url: "/settings/notifications".into(),
            priority: Priority::Default,
            payload: serde_json::json!({
                "type": "test",
                "message": "Notifications reach this channel.",
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Push,
    Gotify,
    Ntfy,
    Matrix,
    Webhook,
}

impl Channel {
    pub const ALL: [Channel; 5] = [
        Channel::Push,
        Channel::Gotify,
        Channel::Ntfy,
        Channel::Matrix,
        Channel::Webhook,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Channel::Push => "push",
            Channel::Gotify => "gotify",
            Channel::Ntfy => "ntfy",
            Channel::Matrix => "matrix",
            Channel::Webhook => "webhook",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Channel::ALL.into_iter().find(|c| c.as_str() == s)
    }
}

/// The channels' settings (`notify_*`), read when a notification goes out.
#[derive(Debug, Clone, Default)]
pub struct Config {
    values: HashMap<String, String>,
    /// Stored secrets left out because a test named another server for them ([`Config::with`]).
    withheld: Vec<&'static str>,
}

/// Each stored secret and the setting that says where it goes. A test that changes where it
/// goes and does not send the secret again gets none: a stored token never leaves for a
/// server it was not saved for.
const SECRET_DESTINATIONS: &[(&str, &str)] = &[
    ("notify_gotify_token", "notify_gotify_url"),
    ("notify_ntfy_token", "notify_ntfy_url"),
    ("notify_matrix_token", "notify_matrix_homeserver"),
];

/// A server address as compared for [`SECRET_DESTINATIONS`]: trimmed, no trailing slash, ntfy's
/// default for empty.
fn destination(key: &str, v: &str) -> String {
    let v = v.trim().trim_end_matches('/');
    match (key, v) {
        ("notify_ntfy_url", "") => "https://ntfy.sh".into(),
        _ => v.to_ascii_lowercase(),
    }
}

impl Config {
    pub async fn load(db: &SqlitePool) -> Result<Self> {
        Ok(Config {
            values: settings::with_prefix(db, "notify_").await?,
            withheld: Vec::new(),
        })
    }

    /// Lay a form's unsaved values over the stored ones; an empty secret means "the stored
    /// one" (the form never sees it), unless the form changed the server it goes to.
    pub fn with(mut self, over: HashMap<String, String>) -> Self {
        for &(secret, dest) in SECRET_DESTINATIONS {
            let resent = over.get(secret).is_some_and(|v| !v.trim().is_empty());
            let moved = over
                .get(dest)
                .is_some_and(|v| destination(dest, v) != destination(dest, self.get(dest)));
            if moved && !resent && self.values.remove(secret).is_some_and(|v| !v.is_empty()) {
                self.withheld.push(secret);
            }
        }
        for (k, v) in over {
            if k.starts_with("notify_") && !(settings::is_secret(&k) && v.is_empty()) {
                self.values.insert(k, v);
            }
        }
        self
    }

    /// The error for a test whose stored secret was withheld ([`SECRET_DESTINATIONS`]).
    fn not_withheld(&self, secret: &str) -> Result<()> {
        if self.withheld.contains(&secret) {
            bail!(
                "The saved token is only sent to the server it was saved for: enter it again to test another server"
            );
        }
        Ok(())
    }

    fn get(&self, key: &str) -> &str {
        self.values.get(key).map_or("", |v| v.trim())
    }

    fn on(&self, key: &str) -> bool {
        self.get(key) == "true"
    }

    pub fn enabled(&self, ch: Channel) -> bool {
        match ch {
            Channel::Push => true,
            Channel::Gotify => self.on("notify_gotify_enabled"),
            Channel::Ntfy => self.on("notify_ntfy_enabled"),
            Channel::Matrix => self.on("notify_matrix_enabled"),
            Channel::Webhook => self.on("notify_webhook_enabled"),
        }
    }

    fn gotify(&self) -> Result<Gotify> {
        let (url, token) = (
            self.get("notify_gotify_url"),
            self.get("notify_gotify_token"),
        );
        self.not_withheld("notify_gotify_token")?;
        if url.is_empty() || token.is_empty() {
            bail!("Gotify needs the server URL and an application token");
        }
        Ok(Gotify {
            url: url.into(),
            token: token.into(),
        })
    }

    fn ntfy(&self) -> Result<Ntfy> {
        let url = match self.get("notify_ntfy_url") {
            "" => "https://ntfy.sh",
            u => u,
        };
        self.not_withheld("notify_ntfy_token")?;
        let topic = self.get("notify_ntfy_topic");
        if topic.is_empty() {
            bail!("ntfy needs a topic");
        }
        Ok(Ntfy {
            url: url.into(),
            topic: topic.into(),
            token: self.get("notify_ntfy_token").into(),
        })
    }

    fn matrix(&self) -> Result<MatrixRoom> {
        self.not_withheld("notify_matrix_token")?;
        let room = MatrixRoom {
            homeserver: self.get("notify_matrix_homeserver").into(),
            room_id: self.get("notify_matrix_room").into(),
            access_token: self.get("notify_matrix_token").into(),
        };
        if room.homeserver.is_empty() || room.room_id.is_empty() || room.access_token.is_empty() {
            bail!("Matrix needs the homeserver, the room id and an access token");
        }
        Ok(room)
    }

    fn webhook(&self) -> Result<String> {
        match self.get("notify_webhook_url") {
            "" => bail!("The webhook needs a URL"),
            u => Ok(u.into()),
        }
    }
}

// ------------------------------------------------------------------ senders --

/// Gotify, or rstify (same API).
struct Gotify {
    url: String,
    token: String,
}

impl Gotify {
    async fn send(&self, http: &reqwest::Client, n: &Note) -> Result<()> {
        http.post(format!("{}/message", self.url.trim_end_matches('/')))
            .header("X-Gotify-Key", &self.token)
            .json(&serde_json::json!({
                "title": n.title,
                "message": n.body,
                "priority": n.priority.gotify(),
            }))
            .send()
            .await
            .context("Could not reach Gotify")?
            .error_for_status()
            .context("Gotify refused the message")?;
        Ok(())
    }
}

struct Ntfy {
    url: String,
    topic: String,
    /// Empty for a public topic.
    token: String,
}

impl Ntfy {
    async fn send(&self, http: &reqwest::Client, n: &Note) -> Result<()> {
        let mut req = http
            .post(self.url.trim_end_matches('/'))
            .json(&serde_json::json!({
                "topic": self.topic,
                "title": n.title,
                "message": n.body,
                "priority": n.priority.ntfy(),
                "tags": ["coffee"],
            }));
        if !self.token.is_empty() {
            req = req.bearer_auth(&self.token);
        }
        req.send()
            .await
            .context("Could not reach ntfy")?
            .error_for_status()
            .context("ntfy refused the message")?;
        Ok(())
    }
}

/// POST the note's payload as JSON (an n8n workflow that forwards it, say).
async fn post_webhook(
    http: &reqwest::Client,
    url: &str,
    payload: &serde_json::Value,
) -> Result<()> {
    http.post(url)
        .json(payload)
        .send()
        .await
        .context("Could not call the webhook")?
        .error_for_status()
        .context("The webhook returned an error")?;
    Ok(())
}

/// A Matrix room, posted to with an account's access token (client-server API).
#[derive(Debug, Clone)]
pub struct MatrixRoom {
    /// `https://matrix.example.org`
    pub homeserver: String,
    /// `!abc123:example.org`
    pub room_id: String,
    pub access_token: String,
}

impl MatrixRoom {
    /// The `PUT …/rooms/{room}/send/m.room.message/{txn}` URL, segments percent-encoded.
    fn send_url(&self, txn_id: &str) -> Result<reqwest::Url> {
        let mut url = reqwest::Url::parse(self.homeserver.trim_end_matches('/'))
            .context("The Matrix homeserver is not a URL")?;
        url.path_segments_mut()
            .map_err(|()| anyhow::anyhow!("The Matrix homeserver URL cannot take a path"))?
            .pop_if_empty()
            .extend([
                "_matrix",
                "client",
                "v3",
                "rooms",
                &self.room_id,
                "send",
                "m.room.message",
                txn_id,
            ]);
        Ok(url)
    }

    /// Post a message: `text` for clients without formatting, `html` for the rest. `txn_id`
    /// makes a retry of the same message idempotent.
    async fn send(
        &self,
        http: &reqwest::Client,
        txn_id: &str,
        text: &str,
        html: &str,
    ) -> Result<()> {
        http.put(self.send_url(txn_id)?)
            .bearer_auth(&self.access_token)
            .json(&serde_json::json!({
                "msgtype": "m.text",
                "body": text,
                "format": "org.matrix.custom.html",
                "formatted_body": html,
            }))
            .send()
            .await
            .context("Could not reach the Matrix homeserver")?
            .error_for_status()
            .context("Matrix refused the message")?;
        Ok(())
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ------------------------------------------------------------------ service --

/// How a channel's last send went (kept in memory; a restart clears it).
#[derive(Debug, Clone, Default, Serialize, utoipa::ToSchema)]
pub struct ChannelStatus {
    pub last_ok_at: Option<String>,
    pub last_error: Option<String>,
    pub last_error_at: Option<String>,
}

/// A system problem that has not cleared.
#[derive(Debug, Clone, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Problem {
    pub key: String,
    pub title: String,
    pub body: String,
    pub changed_at: String,
}

/// What kind of system event, each with its own on/off setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Backup,
    Offline,
    Stock,
}

impl EventKind {
    fn setting(self) -> &'static str {
        match self {
            EventKind::Backup => "notify_event_backup",
            EventKind::Offline => "notify_event_offline",
            EventKind::Stock => "notify_event_stock",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            EventKind::Backup => "backup",
            EventKind::Offline => "offline",
            EventKind::Stock => "stock",
        }
    }
}

/// What a Web Push send to every device (or one) amounts to for the channel.
fn push_outcome(d: &[push::Delivery]) -> Result<String> {
    if d.is_empty() {
        bail!("No device has turned notifications on (Settings → This browser)");
    }
    let ok = d.iter().filter(|d| d.ok).count();
    if ok == 0 {
        bail!("No device took it: {}", d[0].detail);
    }
    Ok(format!("{ok} of {} devices", d.len()))
}

fn now_text() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[derive(Clone)]
pub struct Notifier {
    db: SqlitePool,
    http: reqwest::Client,
    push: PushService,
    status: Arc<Mutex<HashMap<Channel, ChannelStatus>>>,
}

impl Notifier {
    pub fn new(db: SqlitePool, push: PushService) -> Self {
        Self {
            db,
            http: reqwest::Client::builder()
                .timeout(SEND_TIMEOUT)
                .build()
                .expect("reqwest client"),
            push,
            status: Arc::default(),
        }
    }

    /// Each channel's last send. Web Push's last success also comes from its devices' rows,
    /// which a restart keeps and every send path (alarms, tests from This browser) updates.
    pub async fn statuses(&self) -> HashMap<Channel, ChannelStatus> {
        let mut st = self.status.lock().await.clone();
        let rows = self.push.list().await.unwrap_or_default();
        if let Some(last) = rows.iter().filter_map(|r| r.last_ok_at.clone()).max() {
            let s = st.entry(Channel::Push).or_default();
            if s.last_ok_at.as_ref().is_none_or(|m| *m < last) {
                s.last_ok_at = Some(last);
            }
        }
        st
    }

    /// A Web Push test from This browser (`/api/push/test`) counts as the channel's send.
    pub async fn record_push(&self, d: &[push::Delivery]) {
        self.record(Channel::Push, &push_outcome(d)).await;
    }

    /// Send through one channel; `Ok` carries a short line on what happened.
    async fn send_via(&self, ch: Channel, cfg: &Config, n: &Note) -> Result<String> {
        match ch {
            Channel::Push => {
                let msg = push::Message {
                    title: n.title.clone(),
                    body: n.body.clone(),
                    tag: n.tag.clone(),
                    url: n.url.clone(),
                };
                push_outcome(&self.push.send(&msg, None).await?)
            }
            Channel::Gotify => cfg
                .gotify()?
                .send(&self.http, n)
                .await
                .map(|()| "sent".into()),
            Channel::Ntfy => cfg
                .ntfy()?
                .send(&self.http, n)
                .await
                .map(|()| "sent".into()),
            Channel::Matrix => {
                let text = format!("{}\n{}", n.title, n.body);
                let html = format!(
                    "<b>{}</b><br>{}",
                    html_escape(&n.title),
                    html_escape(&n.body)
                );
                let txn = format!("rr-{}", uuid::Uuid::new_v4());
                cfg.matrix()?
                    .send(&self.http, &txn, &text, &html)
                    .await
                    .map(|()| "sent".into())
            }
            Channel::Webhook => post_webhook(&self.http, &cfg.webhook()?, &n.payload)
                .await
                .map(|()| "sent".into()),
        }
    }

    async fn record(&self, ch: Channel, r: &Result<String>) {
        let mut st = self.status.lock().await;
        let s = st.entry(ch).or_default();
        match r {
            Ok(_) => s.last_ok_at = Some(now_text()),
            Err(e) => {
                s.last_error = Some(format!("{e:#}"));
                s.last_error_at = Some(now_text());
            }
        }
    }

    /// Send to every enabled channel at once; one slow or failing channel holds up none.
    pub async fn send(&self, n: Note) {
        let cfg = match Config::load(&self.db).await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "could not read the notification settings");
                Config::default()
            }
        };
        let (cfg, n) = (Arc::new(cfg), Arc::new(n));
        let no_devices = self.push.list().await.map_or(true, |l| l.is_empty());
        for ch in Channel::ALL {
            if !cfg.enabled(ch) || (ch == Channel::Push && no_devices) {
                continue;
            }
            let (this, cfg, n) = (self.clone(), cfg.clone(), n.clone());
            tokio::spawn(async move {
                let r = this.send_via(ch, &cfg, &n).await;
                if let Err(e) = &r {
                    tracing::warn!(channel = ch.as_str(), error = %format!("{e:#}"), "notification not sent");
                }
                this.record(ch, &r).await;
            });
        }
    }

    /// A test through one channel, with the form's unsaved values, whether or not the
    /// channel is turned on.
    pub async fn test(&self, ch: Channel, over: HashMap<String, String>) -> Result<String> {
        let cfg = Config::load(&self.db).await?.with(over);
        let r = self.send_via(ch, &cfg, &Note::test()).await;
        self.record(ch, &r).await;
        r
    }

    /// Every fired alarm, to every channel.
    pub async fn run(self, mut alarms: broadcast::Receiver<AlarmEvent>) {
        loop {
            match alarms.recv().await {
                Ok(a) => self.send(Note::from_alarm(&a)).await,
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "notifications fell behind the alarms");
                }
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    }

    // ---- system events: once on failure, once on recovery ----

    pub async fn problems(&self) -> Result<Vec<Problem>> {
        Ok(sqlx::query_as::<_, Problem>(
            "SELECT key, title, body, changed_at FROM notify_state WHERE failing = 1 ORDER BY changed_at",
        )
        .fetch_all(&self.db)
        .await?)
    }

    /// `key` is failing now. Notifies only when it was not already.
    pub async fn problem(&self, kind: EventKind, key: &str, title: &str, body: &str) {
        let r = sqlx::query(
            "INSERT INTO notify_state (key, failing, title, body, changed_at) VALUES (?, 1, ?, ?, ?)
             ON CONFLICT(key) DO NOTHING",
        )
        .bind(key)
        .bind(title)
        .bind(body)
        .bind(now_text())
        .execute(&self.db)
        .await;
        match r {
            Ok(done) if done.rows_affected() == 1 => {
                tracing::warn!(key, title, body, "system problem");
                self.system_note(kind, key, "failing", title, body, Priority::Default)
                    .await;
            }
            Ok(_) => {}
            Err(e) => tracing::error!(error = %e, key, "could not record a system problem"),
        }
    }

    /// `key` is fine now. Notifies only when it was failing.
    pub async fn resolved(&self, kind: EventKind, key: &str, title: &str, body: &str) {
        let r = sqlx::query("DELETE FROM notify_state WHERE key = ?")
            .bind(key)
            .execute(&self.db)
            .await;
        match r {
            Ok(done) if done.rows_affected() == 1 => {
                tracing::info!(key, title, "system problem cleared");
                self.system_note(kind, key, "recovered", title, body, Priority::Low)
                    .await;
            }
            Ok(_) => {}
            Err(e) => tracing::error!(error = %e, key, "could not clear a system problem"),
        }
    }

    async fn system_note(
        &self,
        kind: EventKind,
        key: &str,
        state: &str,
        title: &str,
        body: &str,
        priority: Priority,
    ) {
        if !settings::flag(&self.db, kind.setting(), true).await {
            return;
        }
        self.send(Note {
            title: title.into(),
            body: body.into(),
            tag: format!("system-{key}"),
            url: match kind {
                EventKind::Backup => "/settings/backups".into(),
                EventKind::Offline => "/roasters".into(),
                EventKind::Stock => "/beans".into(),
            },
            priority,
            payload: serde_json::json!({
                "type": "system_event",
                "kind": kind.as_str(),
                "key": key,
                "state": state,
                "title": title,
                "message": body,
                "at": now_text(),
            }),
        })
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Bytes, extract::State, http::HeaderMap, routing::any};

    #[test]
    fn matrix_send_url_encodes_the_room_and_keeps_a_base_path() {
        let room = MatrixRoom {
            homeserver: "https://matrix.example.org/".into(),
            room_id: "!abc123:example.org".into(),
            access_token: "t".into(),
        };
        assert_eq!(
            room.send_url("rr-1").unwrap().as_str(),
            "https://matrix.example.org/_matrix/client/v3/rooms/!abc123:example.org/send/m.room.message/rr-1"
        );
        let behind_a_path = MatrixRoom {
            homeserver: "https://example.org/matrix".into(),
            ..room
        };
        assert!(
            behind_a_path
                .send_url("x")
                .unwrap()
                .as_str()
                .starts_with("https://example.org/matrix/_matrix/client/v3/rooms/")
        );
        // a room version 12 id (no server part) goes into the path as it is
        let v12 = MatrixRoom {
            homeserver: "https://matrix.example.org".into(),
            room_id: "!31hneApxJ_1o-63DmFrpeqnkFfWppnzWso1JvH3ogLM".into(),
            access_token: "t".into(),
        };
        assert_eq!(
            v12.send_url("rr-2").unwrap().as_str(),
            "https://matrix.example.org/_matrix/client/v3/rooms/!31hneApxJ_1o-63DmFrpeqnkFfWppnzWso1JvH3ogLM/send/m.room.message/rr-2"
        );
    }

    fn stored_gotify() -> Config {
        Config {
            values: HashMap::from([
                ("notify_gotify_url".into(), "https://old".into()),
                ("notify_gotify_token".into(), "stored".into()),
            ]),
            ..Config::default()
        }
    }

    #[test]
    fn unsaved_values_override_but_an_empty_secret_keeps_the_stored_one() {
        let c = stored_gotify().with(HashMap::from([
            ("notify_gotify_url".into(), "https://OLD/".into()),
            ("notify_gotify_token".into(), String::new()),
            ("notify_gotify_enabled".into(), "true".into()),
            ("roast_alarms".into(), "[]".into()),
        ]));
        let g = c.gotify().unwrap();
        assert_eq!(g.token, "stored");
        assert!(c.enabled(Channel::Gotify));
        assert!(!c.values.contains_key("roast_alarms"));
    }

    #[test]
    fn a_stored_secret_never_goes_to_a_server_the_test_changed() {
        // The test's own token goes anywhere it is sent.
        let c = stored_gotify().with(HashMap::from([
            ("notify_gotify_url".into(), "https://new".into()),
            ("notify_gotify_token".into(), "typed".into()),
        ]));
        let g = c.gotify().unwrap();
        assert_eq!((g.url.as_str(), g.token.as_str()), ("https://new", "typed"));
        // The stored one does not.
        let c = stored_gotify().with(HashMap::from([
            (
                "notify_gotify_url".into(),
                "https://attacker.example".into(),
            ),
            ("notify_gotify_token".into(), String::new()),
        ]));
        let e = c.gotify().err().unwrap().to_string();
        assert!(e.contains("enter it again"), "{e}");
        assert!(!c.values.contains_key("notify_gotify_token"));
        // The same for ntfy (its empty URL is ntfy.sh) and Matrix.
        let ntfy = Config {
            values: HashMap::from([
                ("notify_ntfy_topic".into(), "t".into()),
                ("notify_ntfy_token".into(), "stored".into()),
            ]),
            ..Config::default()
        };
        let same = ntfy.clone().with(HashMap::from([(
            "notify_ntfy_url".into(),
            "https://ntfy.sh/".into(),
        )]));
        assert_eq!(same.ntfy().unwrap().token, "stored");
        let moved = ntfy.with(HashMap::from([(
            "notify_ntfy_url".into(),
            "https://elsewhere".into(),
        )]));
        assert!(moved.ntfy().is_err());
        let matrix = Config {
            values: HashMap::from([
                (
                    "notify_matrix_homeserver".into(),
                    "https://m.example".into(),
                ),
                ("notify_matrix_room".into(), "!r:m.example".into()),
                ("notify_matrix_token".into(), "stored".into()),
            ]),
            ..Config::default()
        }
        .with(HashMap::from([(
            "notify_matrix_homeserver".into(),
            "https://evil".into(),
        )]));
        assert!(matrix.matrix().is_err());
    }

    #[test]
    fn channels_say_what_they_miss() {
        let c = Config::default();
        assert!(c.gotify().is_err());
        assert!(c.matrix().is_err());
        assert!(c.webhook().is_err());
        let e = c.ntfy().err().unwrap().to_string();
        assert!(e.contains("topic"), "{e}");
        let c = c.with(HashMap::from([("notify_ntfy_topic".into(), "t".into())]));
        assert_eq!(c.ntfy().unwrap().url, "https://ntfy.sh");
        assert!(c.enabled(Channel::Push));
        assert!(!c.enabled(Channel::Ntfy));
    }

    type Seen = Arc<std::sync::Mutex<Vec<(String, String, HeaderMap, Bytes)>>>;

    /// A local server that records every request.
    async fn recorder() -> (String, Seen) {
        let seen: Seen = Arc::default();
        let app = Router::new()
            .fallback(any(
                |State(seen): State<Seen>,
                 method: axum::http::Method,
                 uri: axum::http::Uri,
                 headers: HeaderMap,
                 body: Bytes| async move {
                    seen.lock().unwrap().push((
                        method.to_string(),
                        uri.path().to_string(),
                        headers,
                        body,
                    ));
                    "{}"
                },
            ))
            .with_state(seen.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{addr}"), seen)
    }

    async fn pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        pool
    }

    async fn set(pool: &SqlitePool, k: &str, v: &str) {
        sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)")
            .bind(k)
            .bind(v)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn wait_for(seen: &Seen, n: usize) {
        for _ in 0..200 {
            if seen.lock().unwrap().len() >= n {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("expected {n} requests, got {}", seen.lock().unwrap().len());
    }

    fn json(b: &Bytes) -> serde_json::Value {
        serde_json::from_slice(b).unwrap()
    }

    #[tokio::test]
    async fn an_alarm_goes_to_every_enabled_channel_in_its_own_format() {
        let (base, seen) = recorder().await;
        let db = pool().await;
        for (k, v) in [
            ("notify_gotify_enabled", "true"),
            ("notify_gotify_url", &format!("{base}/gotify/")),
            ("notify_gotify_token", "gtok"),
            ("notify_ntfy_enabled", "true"),
            ("notify_ntfy_url", &format!("{base}/ntfy")),
            ("notify_ntfy_topic", "roasts"),
            ("notify_matrix_enabled", "true"),
            ("notify_matrix_homeserver", &format!("{base}/mx")),
            ("notify_matrix_room", "!r:example.org"),
            ("notify_matrix_token", "mtok"),
            ("notify_webhook_enabled", "true"),
            ("notify_webhook_url", &format!("{base}/hook")),
        ] {
            set(&db, k, v).await;
        }
        let n = Notifier::new(db.clone(), PushService::new(db.clone()));
        let alarm = AlarmEvent {
            device_id: "virtual_roaster".into(),
            session_id: "s1".into(),
            session_name: "Batch 3".into(),
            name: "BT high".into(),
            condition_type: "bt_above".into(),
            threshold: 230.0,
            message: "BT above 230 °C <b>".into(),
            value: Some(231.0),
            elapsed_seconds: 605.0,
            fired_at: chrono::Utc::now(),
            actions: vec![],
        };
        n.send(Note::from_alarm(&alarm)).await;
        wait_for(&seen, 4).await;
        let seen = seen.lock().unwrap().clone();
        let by = |p: &str| seen.iter().find(|r| r.1.starts_with(p)).unwrap();

        let g = by("/gotify");
        assert_eq!(g.1, "/gotify/message");
        assert_eq!(g.2["x-gotify-key"], "gtok");
        assert_eq!(json(&g.3)["priority"], 8);
        assert_eq!(json(&g.3)["title"], "virtual_roaster: BT high");

        let t = by("/ntfy");
        let body = json(&t.3);
        assert_eq!(body["topic"], "roasts");
        assert_eq!(body["priority"], 5);
        assert!(body["message"].as_str().unwrap().contains("230 °C"));
        assert!(t.2.get("authorization").is_none(), "public topic, no token");

        let m = by("/mx");
        assert_eq!(m.0, "PUT");
        assert!(
            m.1.starts_with("/mx/_matrix/client/v3/rooms/!r:example.org/send/m.room.message/rr-")
        );
        assert_eq!(m.2["authorization"], "Bearer mtok");
        assert!(
            json(&m.3)["formatted_body"]
                .as_str()
                .unwrap()
                .contains("&lt;b&gt;")
        );

        let w = by("/hook");
        assert_eq!(json(&w.3)["type"], "roast_alarm");
        assert_eq!(json(&w.3)["alarm"]["name"], "BT high");

        // The four channels each record their send; Web Push, with no devices, is skipped.
        for _ in 0..200 {
            if n.statuses().await.len() == 4 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let st = n.statuses().await;
        assert!(st[&Channel::Webhook].last_ok_at.is_some());
        assert!(st[&Channel::Matrix].last_error.is_none());
        assert!(!st.contains_key(&Channel::Push));
    }

    #[tokio::test]
    async fn a_problem_is_sent_once_and_its_recovery_once() {
        let (base, seen) = recorder().await;
        let db = pool().await;
        set(&db, "notify_webhook_enabled", "true").await;
        set(&db, "notify_webhook_url", &format!("{base}/hook")).await;
        let n = Notifier::new(db.clone(), PushService::new(db.clone()));

        n.resolved(EventKind::Backup, "backup", "fine", "").await; // was never failing
        n.problem(
            EventKind::Backup,
            "backup",
            "Nightly backup failed",
            "disk full",
        )
        .await;
        n.problem(
            EventKind::Backup,
            "backup",
            "Nightly backup failed",
            "disk full",
        )
        .await;
        wait_for(&seen, 1).await;
        assert_eq!(n.problems().await.unwrap().len(), 1);
        // A restart keeps the state: a new notifier does not repeat it.
        let n2 = Notifier::new(db.clone(), PushService::new(db.clone()));
        n2.problem(
            EventKind::Backup,
            "backup",
            "Nightly backup failed",
            "disk full",
        )
        .await;
        n2.resolved(
            EventKind::Backup,
            "backup",
            "Nightly backups work again",
            "ok",
        )
        .await;
        n2.resolved(
            EventKind::Backup,
            "backup",
            "Nightly backups work again",
            "ok",
        )
        .await;
        wait_for(&seen, 2).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        let seen_now = seen.lock().unwrap().clone();
        assert_eq!(seen_now.len(), 2);
        assert_eq!(json(&seen_now[0].3)["state"], "failing");
        assert_eq!(json(&seen_now[1].3)["state"], "recovered");
        assert_eq!(json(&seen_now[1].3)["kind"], "backup");
        assert!(n2.problems().await.unwrap().is_empty());

        // Turned off: the state is still kept, nothing is sent.
        set(&db, "notify_event_stock", "false").await;
        n2.problem(EventKind::Stock, "stock:b1", "Low stock", "100 g")
            .await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(seen.lock().unwrap().len(), 2);
        assert_eq!(n2.problems().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_test_uses_the_unsaved_values() {
        let (base, seen) = recorder().await;
        let db = pool().await;
        let n = Notifier::new(db.clone(), PushService::new(db.clone()));
        let e = n.test(Channel::Webhook, HashMap::new()).await.unwrap_err();
        assert!(e.to_string().contains("URL"), "{e}");
        let ok = n
            .test(
                Channel::Webhook,
                HashMap::from([("notify_webhook_url".into(), format!("{base}/hook"))]),
            )
            .await
            .unwrap();
        assert_eq!(ok, "sent");
        assert_eq!(json(&seen.lock().unwrap()[0].3)["type"], "test");
    }
}
