//! Is a roaster in the middle of something a server restart would cut? (`docs/DEPLOY.md`,
//! "Deploys wait for the roaster"). A restart stops a profile drive and the server's autotune,
//! drops the cooling after DROP (its fan-off never comes) and, if it takes longer than the
//! firmware's server-offline timeout, cuts the heater mid-roast. The deploy script asks
//! `GET /api/busy` and waits while a real roaster is busy; the virtual roaster is listed but
//! never holds a deploy.

use serde::Serialize;
use utoipa::ToSchema;

use crate::app::state::AppState;

/// A roaster this hot with a roast open counts as roasting even with the heater off.
pub const HOT_C: f64 = 50.0;
/// A sample older than this says nothing about the heater or the beans now.
const STALE_MS: u64 = 60_000;

/// What a roaster is doing that a restart would cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// A profile drive runs it
    Drive,
    /// The server's autotune runs it
    Autotune,
    /// The fan cools the beans after DROP; the server ends it
    Cooling,
    /// Its heater is on (applied output above 0)
    Heating,
    /// A roast is open and the beans are hot (at least 50 °C)
    Roasting,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RoasterBusy {
    pub device_id: String,
    /// A simulated roaster (`virtual_roaster`, `virtual_drum`): listed, never holds a deploy
    pub r#virtual: bool,
    pub reasons: Vec<Reason>,
    /// Bean temperature in the latest fresh sample, °C
    pub bt_c: Option<f64>,
    /// The open roast, if any
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Busy {
    /// A real roaster is busy: a deploy or restart should wait
    pub busy: bool,
    /// Every roaster doing something (an idle one is left out)
    pub roasters: Vec<RoasterBusy>,
}

/// The reasons from a roaster's state. Pure, so the rules have their own tests.
pub fn reasons(
    drive: bool,
    autotune: bool,
    cooling: bool,
    heater_applied: Option<f64>,
    bt_c: Option<f64>,
    roast_open: bool,
) -> Vec<Reason> {
    let mut r = Vec::new();
    if drive {
        r.push(Reason::Drive);
    }
    if autotune {
        r.push(Reason::Autotune);
    }
    if cooling {
        r.push(Reason::Cooling);
    }
    if heater_applied.is_some_and(|h| h > 0.0) {
        r.push(Reason::Heating);
    }
    if roast_open && bt_c.is_some_and(|t| t >= HOT_C) {
        r.push(Reason::Roasting);
    }
    r
}

/// Every roaster heard since the server started, checked.
pub async fn check(state: &AppState) -> Busy {
    let now_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
    let mut roasters = Vec::new();
    for info in state.telemetry_service.ring_summary() {
        let id = info.device_id;
        let fresh = state
            .telemetry_service
            .latest_sample(&id)
            .filter(|s| now_ms.saturating_sub(s.ts_ms) < STALE_MS);
        let bt_c = fresh.as_ref().and_then(|s| s.bt);
        // `heaterApplied` (after the enable flag and the fan interlock) when reported, else
        // the command
        let heater = fresh.as_ref().and_then(|s| {
            s.extras
                .get("heaterApplied")
                .copied()
                .or(s.heater)
                .filter(|h| h.is_finite())
        });
        let session = crate::roasts::sessions::get_active_session(&state.db, &id)
            .await
            .ok()
            .flatten();
        let reasons = reasons(
            state.replay.drives_device(&id).await,
            state.tune.active(&id).await,
            state.sequence.cooling(&id).await,
            heater,
            bt_c,
            session.is_some(),
        );
        if reasons.is_empty() {
            continue;
        }
        roasters.push(RoasterBusy {
            r#virtual: crate::sim::virtual_roaster::is_simulated(&id)
                || id == crate::sim::virtual_drum::DEVICE_ID,
            device_id: id,
            reasons,
            bt_c,
            session_id: session.map(|s| s.id),
        });
    }
    roasters.sort_by(|a, b| a.device_id.cmp(&b.device_id));
    Busy {
        busy: roasters.iter().any(|r| !r.r#virtual),
        roasters,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_counts_as_busy() {
        let none: Vec<Reason> = vec![];
        assert_eq!(
            reasons(false, false, false, Some(0.0), Some(24.0), false),
            none
        );
        assert_eq!(
            reasons(false, false, false, Some(0.0), Some(24.0), true),
            none,
            "a roast opened on a cold roaster is not busy yet"
        );
        assert_eq!(
            reasons(false, false, false, Some(0.0), Some(180.0), true),
            vec![Reason::Roasting]
        );
        assert_eq!(
            reasons(false, false, false, Some(0.0), Some(180.0), false),
            none,
            "hot with no roast: cooling down on its own"
        );
        assert_eq!(
            reasons(false, false, false, Some(35.0), None, false),
            vec![Reason::Heating],
            "a heater on by hand"
        );
        assert_eq!(
            reasons(true, true, true, Some(60.0), Some(200.0), true),
            vec![
                Reason::Drive,
                Reason::Autotune,
                Reason::Cooling,
                Reason::Heating,
                Reason::Roasting
            ]
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_hot_real_roaster_holds_a_deploy_the_virtual_one_never_does() {
        use crate::test_support::{Harness, eventually};
        use serde_json::json;
        let h = Harness::new().await;
        let busy = || async { h.owner("GET", "/api/busy", None).await.json() };
        assert_eq!(busy().await, json!({"busy": false, "roasters": []}));

        // A real roaster with its heater on
        let emit = |heater: f64| {
            h.owner(
                "POST",
                "/api/test/emit-telemetry/esp32_test",
                Some(
                    json!({"beanTemp": 120.0, "envTemp": 200.0, "heater": heater,
                    "heaterApplied": heater, "fan": 80}),
                ),
            )
        };
        assert!(emit(40.0).await.status.is_success());
        assert!(eventually(|| async { busy().await["busy"] == json!(true) }).await);
        let b = busy().await;
        assert_eq!(b["roasters"][0]["device_id"], "esp32_test", "{b}");
        assert_eq!(b["roasters"][0]["reasons"], json!(["heating"]), "{b}");
        assert_eq!(b["roasters"][0]["virtual"], false, "{b}");

        // Heater off, no roast open: idle again
        assert!(emit(0.0).await.status.is_success());
        assert!(eventually(|| async { busy().await["busy"] == json!(false) }).await);

        // The virtual roaster heating is listed, and holds nothing
        h.owner(
            "POST",
            "/api/test/emit-telemetry/virtual_roaster",
            Some(json!({"beanTemp": 120.0, "heater": 50, "heaterApplied": 50})),
        )
        .await;
        assert!(
            eventually(|| async {
                let b = busy().await;
                b["roasters"]
                    .as_array()
                    .is_some_and(|r| r.iter().any(|x| x["device_id"] == "virtual_roaster"))
            })
            .await
        );
        let b = busy().await;
        assert_eq!(b["busy"], false, "{b}");
    }
}
