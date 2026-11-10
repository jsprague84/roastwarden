//! A public demo's visitors, each with a simulated roaster of their own (docs/DEPLOY.md, "A
//! simulated roaster per visitor").
//!
//! A visitor is a browser with the `rw_visitor` cookie, set by `POST /api/demo/roaster`. Their
//! roaster's id comes from the cookie's hash, so the same browser finds the same roaster after
//! the demo starts over, and the id (seen by others in History) does not give the cookie away.
//! At most [`MAX_ROASTERS`] at once and [`PER_ADDRESS`] per address; one idle for [`IDLE`]
//! goes. Its simulation is in `virtual_roaster.rs`, beside the shared `virtual_roaster`.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::http::{HeaderMap, HeaderValue, header};
use sha2::{Digest, Sha256};

use crate::app::state::AppState;
use crate::roasters::types::{
    CreateConnectionRequest, CreateDeviceRequest, DeviceStatus, Protocol,
};

pub const COOKIE: &str = "rw_visitor";
/// The cookie's life: a day, as the Privacy page says
const COOKIE_MAX_AGE_S: u32 = 24 * 3600;
/// Visitors' roasters at once; past it a visitor shares `virtual_roaster`
pub const MAX_ROASTERS: usize = 20;
/// Roasters for one address (a household, or a script asking again without the cookie)
pub const PER_ADDRESS: usize = 3;
/// A roaster with no request from its visitor for this long goes
pub const IDLE: Duration = Duration::from_secs(3600);
const PREFIX: &str = "virtual_";

/// The visitor's cookie, when the browser brought a well-formed one (32 hex digits).
pub fn presented(headers: &HeaderMap) -> Option<&str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|pair| {
            let (k, v) = pair.trim().split_once('=')?;
            (k == COOKIE && v.len() == 32 && v.bytes().all(|b| b.is_ascii_hexdigit())).then_some(v)
        })
}

/// A new visitor's cookie: 128 random bits.
pub fn new_cookie() -> String {
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).expect("the OS random source failed");
    buf.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// The `Set-Cookie` for a visitor (HttpOnly, SameSite=Lax, Secure behind HTTPS).
pub fn set_cookie(value: &str, https: bool) -> HeaderValue {
    let secure = if https { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{COOKIE}={value}; Path=/; Max-Age={COOKIE_MAX_AGE_S}; HttpOnly; SameSite=Lax{secure}"
    ))
    .expect("a cookie of hex digits is a header value")
}

/// The roaster that belongs to this cookie: `virtual_` and 8 hex digits of its SHA-256.
pub fn roaster_for(cookie: &str) -> String {
    let hash = Sha256::digest(cookie.to_ascii_lowercase().as_bytes());
    hash[..4].iter().fold(PREFIX.to_string(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// The visitor's own roaster, from the request's cookie.
pub fn own_roaster(headers: &HeaderMap) -> Option<String> {
    presented(headers).map(roaster_for)
}

/// A visitor's roaster: `virtual_` and 8 lowercase hex digits (never `virtual_roaster` or
/// `virtual_drum`).
pub fn is_visitor_roaster(id: &str) -> bool {
    id.strip_prefix(PREFIX).is_some_and(|h| {
        h.len() == 8
            && h.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// What a visitor asking for a roaster gets.
#[derive(Debug, PartialEq, Eq)]
pub enum Claim {
    /// Their own: `new` when it is made now
    Own { new: bool },
    /// No room: they share `virtual_roaster`
    Full,
}

#[derive(Debug)]
struct Visitor {
    last_seen: Instant,
    address: String,
}

/// The visitors' roasters and when each was last used: the auth layer keeps them used, the
/// sweep ([`run`]) drops the idle ones.
#[derive(Clone, Default)]
pub struct Visitors {
    inner: Arc<Mutex<HashMap<String, Visitor>>>,
}

impl Visitors {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Visitor>> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// A request from this roaster's visitor: it stays.
    pub fn touch(&self, roaster: &str, now: Instant) {
        if let Some(v) = self.lock().get_mut(roaster) {
            v.last_seen = now;
        }
    }

    /// The visitor's roaster, made when there is room.
    pub fn claim(&self, roaster: &str, address: &str, now: Instant) -> Claim {
        let mut m = self.lock();
        if let Some(v) = m.get_mut(roaster) {
            v.last_seen = now;
            return Claim::Own { new: false };
        }
        let same_address = m.values().filter(|v| v.address == address).count();
        if m.len() >= MAX_ROASTERS || same_address >= PER_ADDRESS {
            return Claim::Full;
        }
        m.insert(
            roaster.to_string(),
            Visitor {
                last_seen: now,
                address: address.to_string(),
            },
        );
        Claim::Own { new: true }
    }

    /// A claim that could not be made after all (the roaster's row failed): room again.
    pub fn release(&self, roaster: &str) {
        self.lock().remove(roaster);
    }

    /// The roasters idle for [`IDLE`], taken out.
    pub fn take_idle(&self, now: Instant) -> Vec<String> {
        let mut m = self.lock();
        let idle: Vec<String> = m
            .iter()
            .filter(|(_, v)| now.duration_since(v.last_seen) >= IDLE)
            .map(|(k, _)| k.clone())
            .collect();
        for k in &idle {
            m.remove(k);
        }
        idle
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.lock().len()
    }
}

/// A visitor's roaster in the database and in the simulation: a `devices` row (active, with
/// its MQTT connection) and its simulated device.
pub async fn provision(state: &AppState, roaster: &str) -> anyhow::Result<()> {
    if crate::roasters::registry::get_device_by_device_id(&state.db, roaster)
        .await?
        .is_none()
    {
        let short = &roaster[PREFIX.len()..PREFIX.len() + 4];
        let dev = crate::roasters::registry::create_device(
            &state.db,
            CreateDeviceRequest {
                name: format!("Simulated roaster {short}"),
                device_id: roaster.to_string(),
                profile_id: None,
                description: Some("A demo visitor's own simulated roaster".into()),
                location: None,
            },
        )
        .await?;
        crate::roasters::registry::update_device_status(&state.db, &dev.id, DeviceStatus::Active)
            .await?;
        crate::roasters::registry::add_connection(&state.db,
                &dev.id,
                CreateConnectionRequest {
                    protocol: Protocol::Mqtt,
                    enabled: Some(true),
                    priority: Some(0),
                    config: serde_json::json!({ "topic_prefix": format!("roaster/{roaster}"), "qos": 0 }),
                },
            )
            .await?;
        state.pollers.sync_device(&dev.id).await;
    }
    state.virtual_roaster.add_visitor(roaster).await;
    Ok(())
}

/// A roaster whose visitor left: its roast ended (as DROP would end it), its drive and
/// autotune stopped, its simulation and its row gone. Its roasts stay in History.
pub async fn retire(state: &AppState, roaster: &str) {
    state
        .replay
        .stop_for_device(roaster, "the demo visitor left")
        .await;
    state.tune.stop(roaster, "the demo visitor left").await;
    if let Ok(Some(s)) = crate::roasts::sessions::get_active_session(&state.db, roaster).await
        && let Err(e) = crate::roasts::sessions::complete_session(&state.db, &s.id).await
    {
        tracing::warn!(error = %e, %roaster, "demo: could not end an idle visitor's roast");
    }
    state.virtual_roaster.remove_visitor(roaster).await;
    match crate::roasters::registry::get_device_by_device_id(&state.db, roaster).await {
        Ok(Some(d)) => {
            if let Err(e) = crate::roasters::registry::delete_device(&state.db, &d.device.id).await
            {
                tracing::warn!(error = %e, %roaster, "demo: could not delete an idle roaster");
            }
            state.pollers.sync_device(&d.device.id).await;
        }
        Ok(None) => {}
        Err(e) => tracing::warn!(error = %e, %roaster, "demo: could not look up an idle roaster"),
    }
    tracing::info!(%roaster, "demo: an idle visitor's roaster went");
}

/// A demo's `/robots.txt` (docs/DEPLOY.md, "A public demo"): search engines may index the
/// front page and fetch what it loads (the app's files and icons); the views behind it hold
/// made-up records and the API makes roasters, so both stay out. The site map is the project
/// site's. Only a demo serves it; elsewhere `/robots.txt` is the SPA's as before.
pub const ROBOTS: &str = "\
# A public demo of Roastwarden: a simulated coffee roaster to try.
# Its front page may be indexed; the views behind it hold made-up records.
User-agent: *
Allow: /$
Allow: /_app/
Allow: /icons/
Allow: /favicon.ico
Allow: /favicon.svg
Allow: /manifest.webmanifest
Disallow: /

Sitemap: https://roastwarden.com/sitemap.xml
";

/// `GET /robots.txt` on a demo: [`ROBOTS`] as plain text, cached for a day.
pub async fn robots() -> impl axum::response::IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        ROBOTS,
    )
}

/// Every minute, the roasters idle for an hour go (only on a demo).
pub async fn run(state: AppState) {
    let mut tick = tokio::time::interval(Duration::from_secs(60));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        for roaster in state.auth.visitors().take_idle(Instant::now()) {
            retire(&state, &roaster).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cookie_names_one_roaster_and_only_its_own() {
        let c = new_cookie();
        assert_eq!(c.len(), 32);
        let r = roaster_for(&c);
        assert!(is_visitor_roaster(&r), "{r}");
        assert_eq!(r, roaster_for(&c.to_ascii_uppercase()), "stable");
        assert_ne!(r, roaster_for(&new_cookie()));
        for id in [
            "virtual_roaster",
            "virtual_drum",
            "virtual_3FA9C1D2",
            "virtual_3fa9c1d",
            "esp32_roaster_01",
        ] {
            assert!(!is_visitor_roaster(id), "{id}");
        }
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            format!("rw_login=x; {COOKIE}={c}").parse().unwrap(),
        );
        assert_eq!(own_roaster(&h), Some(r));
        h.insert(
            header::COOKIE,
            format!("{COOKIE}=../../etc").parse().unwrap(),
        );
        assert_eq!(presented(&h), None, "not 32 hex digits");
        let set = set_cookie(&c, true);
        let set = set.to_str().unwrap();
        assert!(set.contains("HttpOnly") && set.contains("SameSite=Lax") && set.contains("Secure"));
        assert!(set.contains("Max-Age=86400"));
        assert!(!set_cookie(&c, false).to_str().unwrap().contains("Secure"));
    }

    #[test]
    fn room_for_twenty_three_an_address_and_an_idle_hour() {
        let v = Visitors::default();
        let t0 = Instant::now();
        assert_eq!(
            v.claim("virtual_00000001", "a", t0),
            Claim::Own { new: true }
        );
        assert_eq!(
            v.claim("virtual_00000001", "a", t0),
            Claim::Own { new: false }
        );
        assert_eq!(
            v.claim("virtual_00000002", "a", t0),
            Claim::Own { new: true }
        );
        assert_eq!(
            v.claim("virtual_00000003", "a", t0),
            Claim::Own { new: true }
        );
        assert_eq!(
            v.claim("virtual_00000004", "a", t0),
            Claim::Full,
            "three an address"
        );
        for i in 4..=MAX_ROASTERS + 1 {
            v.claim(&format!("virtual_{i:08x}"), &format!("b{i}"), t0);
        }
        assert_eq!(v.len(), MAX_ROASTERS);
        assert_eq!(v.claim("virtual_ffffffff", "c", t0), Claim::Full, "the cap");
        // Kept by use, dropped when idle
        let later = (t0 + IDLE).checked_sub(Duration::from_secs(1)).unwrap();
        v.touch("virtual_00000001", later);
        let idle = v.take_idle(t0 + IDLE);
        assert_eq!(idle.len(), MAX_ROASTERS - 1);
        assert!(!idle.contains(&"virtual_00000001".to_string()));
        assert_eq!(v.len(), 1);
        v.release("virtual_00000001");
        assert_eq!(v.len(), 0);
    }

    #[tokio::test]
    async fn a_visitor_who_left_has_their_roast_ended_and_roaster_gone() {
        use crate::test_support::Harness;
        let h = Harness::demo().await;
        let r = h.anon("POST", "/api/demo/roaster", None).await;
        let id = r.json()["device_id"].as_str().unwrap().to_string();
        let cookie = r.set_cookie[0].split(';').next().unwrap().to_string();
        let s = h
            .with_cookie(
                &cookie,
                "198.51.100.9",
                "POST",
                "/api/sessions",
                Some(serde_json::json!({ "name": "Left", "device_id": id })),
            )
            .await
            .json();
        let sid = s["id"].as_str().unwrap().to_string();
        let started = h
            .with_cookie(
                &cookie,
                "198.51.100.9",
                "POST",
                &format!("/api/sessions/{sid}/start"),
                None,
            )
            .await;
        assert!(started.status.is_success(), "{:?}", started.json());

        // An hour later nothing came from them
        let idle = h.state.auth.visitors().take_idle(Instant::now() + IDLE);
        assert_eq!(idle, vec![id.clone()]);
        retire(&h.state, &id).await;

        let s = h
            .owner("GET", &format!("/api/sessions/{sid}"), None)
            .await
            .json();
        assert_eq!(
            s["session"]["status"].as_str().or(s["status"].as_str()),
            Some("completed"),
            "{s}"
        );
        assert!(h.state.virtual_roaster.state_of(&id).await.is_none());
        assert!(
            crate::roasters::registry::get_device_by_device_id(&h.state.db, &id)
                .await
                .unwrap()
                .is_none()
        );
        // Coming back makes it again, the same roaster
        let again = h
            .with_cookie(&cookie, "198.51.100.9", "POST", "/api/demo/roaster", None)
            .await;
        assert_eq!(again.json()["device_id"], serde_json::json!(id));
        assert!(h.state.virtual_roaster.state_of(&id).await.is_some());
    }
}
