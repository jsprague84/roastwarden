//! Each roaster's setup: what kind of roaster it is and what Roastwarden may send it
//! (`docs/roaster-setup.md`: outputs, the fan floor, kinds' defaults).
//!
//! The control level is the gate in [`crate::transports::transport::TransportRegistry::send`]: every
//! command from every feature passes it. A roaster with no row is [`Control::Off`], so one
//! added or discovered after migration 045 logs only until the owner switches control on;
//! the roasters that existed then are [`Control::Full`]. `virtual_roaster` is always full.

use std::collections::HashMap;
use std::sync::Arc;

use roastwarden_core::commands::DeviceCommand;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::RwLock;
use utoipa::ToSchema;

use crate::roasters::channels::{self, Channel};
use crate::sim::virtual_roaster;

/// What Roastwarden may send a roaster.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Control {
    /// Logs only: nothing is sent, not even emergency stop.
    Off,
    /// Heater %, fan %, drum %, damper %, heater on/off and emergency stop.
    Manual,
    /// Everything: setpoint, mode, PID gains, feedforward, lease, RoR limit too.
    Full,
}

impl Control {
    pub fn as_str(self) -> &'static str {
        match self {
            Control::Off => "off",
            Control::Manual => "manual",
            Control::Full => "full",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "off" => Some(Control::Off),
            "manual" => Some(Control::Manual),
            "full" => Some(Control::Full),
            _ => None,
        }
    }

    /// Whether this level lets `cmd` through.
    pub fn allows(self, cmd: &DeviceCommand) -> bool {
        match self {
            Control::Off => false,
            Control::Manual => matches!(
                cmd,
                DeviceCommand::Heater { .. }
                    | DeviceCommand::Fan { .. }
                    | DeviceCommand::Drum { .. }
                    | DeviceCommand::Damper { .. }
                    | DeviceCommand::HeaterEnable { .. }
                    | DeviceCommand::EmergencyStop
            ),
            Control::Full => true,
        }
    }
}

/// What kind of roaster it is. Choosing one writes its defaults: outputs, the fan floor, the
/// roast sequence and fan assist (`docs/roaster-setup.md`, "Kinds' defaults").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    FluidBedElectric,
    DrumElectric,
    DrumGas,
    Logger,
    Other,
}

impl Kind {
    /// Every kind, in the order the setup lists them.
    pub const ALL: [Kind; 5] = [
        Kind::FluidBedElectric,
        Kind::DrumElectric,
        Kind::DrumGas,
        Kind::Logger,
        Kind::Other,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::FluidBedElectric => "fluid_bed_electric",
            Kind::DrumElectric => "drum_electric",
            Kind::DrumGas => "drum_gas",
            Kind::Logger => "logger",
            Kind::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Kind {
        match s {
            "fluid_bed_electric" => Kind::FluidBedElectric,
            "drum_electric" => Kind::DrumElectric,
            "drum_gas" => Kind::DrumGas,
            "logger" => Kind::Logger,
            _ => Kind::Other,
        }
    }

    /// The kind's outputs.
    pub fn outputs(self) -> Outputs {
        match self {
            Kind::FluidBedElectric | Kind::DrumElectric | Kind::Other => Outputs {
                heater: Some(Output::percent("Heater")),
                fan: Some(Output::percent("Fan")),
                ..Outputs::default()
            },
            Kind::DrumGas => Outputs {
                heater: Some(Output::percent("Burner")),
                fan: Some(Output::percent("Air")),
                ..Outputs::default()
            },
            Kind::Logger => Outputs::default(),
        }
    }

    /// The kind's fan floor while the heater is on, %: the fluid bed's firmware cuts the heater
    /// below 40 % fan, and the server keeps a margin above it. 0: none.
    pub fn fan_floor_pct(self) -> f64 {
        match self {
            Kind::FluidBedElectric => FLUID_BED_FAN_FLOOR_PCT,
            _ => 0.0,
        }
    }

    /// Whether the kind's roast sequence acts at CHARGE and DROP (the fluid bed's charges cold
    /// and drives; a drum is preheated and keeps running, so its sequence only records).
    pub fn sequence_acts(self) -> bool {
        self == Kind::FluidBedElectric
    }
}

/// The fluid bed's fan floor: a margin above its firmware's 40 % heater interlock.
pub const FLUID_BED_FAN_FLOOR_PCT: f64 = 45.0;

/// One output a roaster accepts: heater (or burner), fan (or air), drum speed or damper, in %.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Output {
    /// What the dashboard calls it ("Heater", "Burner", "Fan", "Air").
    pub label: String,
    /// Lowest value when on, %. 0 always passes as off.
    pub min: f64,
    /// Highest value, %.
    pub max: f64,
    /// Values round to a multiple of this from `min`, %; 0: any value.
    #[serde(default)]
    pub step: f64,
}

impl Output {
    pub fn percent(label: &str) -> Self {
        Self {
            label: label.to_string(),
            min: 0.0,
            max: 100.0,
            step: 0.0,
        }
    }

    /// The value the roaster gets for `v`: 0 (or less, or not a number) is off; anything
    /// else is clamped to `min..max` and rounded to the step.
    pub fn shape(&self, v: f64) -> f64 {
        if v.is_nan() || v <= 0.0 {
            return 0.0;
        }
        let mut x = v.clamp(self.min, self.max);
        if self.step > 0.0 {
            x = self.min + ((x - self.min) / self.step).round() * self.step;
            x = x.clamp(self.min, self.max);
        }
        x
    }

    /// The lowest value the output takes at or above `v` (the fan floor with a step).
    pub fn at_least(&self, v: f64) -> f64 {
        let mut x = v.clamp(self.min, self.max);
        if self.step > 0.0 {
            x = (self.min + ((x - self.min) / self.step - 1e-9).ceil() * self.step).min(self.max);
        }
        x
    }

    fn validate(&self, name: &str) -> Result<(), String> {
        let l = self.label.trim().chars().count();
        if !(1..=24).contains(&l) {
            return Err(format!("{name}.label must be 1..24 characters"));
        }
        if !(self.min.is_finite() && self.max.is_finite() && self.step.is_finite()) {
            return Err(format!("{name}: min, max and step must be numbers"));
        }
        if !(0.0 <= self.min && self.min < self.max && self.max <= 100.0) {
            return Err(format!("{name}: 0 <= min < max <= 100"));
        }
        if !(0.0 <= self.step && self.step <= self.max - self.min) {
            return Err(format!("{name}: step must be 0..(max - min)"));
        }
        Ok(())
    }
}

/// The outputs a roaster accepts; `None` is not declared (its commands are refused).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Outputs {
    pub heater: Option<Output>,
    pub fan: Option<Output>,
    /// Drum speed (docs/ingest-mapping.md, "Drum and damper"); no kind declares it.
    #[serde(default)]
    pub drum: Option<Output>,
    /// Damper position; no kind declares it.
    #[serde(default)]
    pub damper: Option<Output>,
}

impl Outputs {
    /// The declared output by name (`heater`, `fan`, `drum`, `damper`).
    pub fn get(&self, name: &str) -> Option<&Output> {
        match name {
            "heater" => self.heater.as_ref(),
            "fan" => self.fan.as_ref(),
            "drum" => self.drum.as_ref(),
            "damper" => self.damper.as_ref(),
            _ => None,
        }
    }
}

/// Check outputs and a fan floor together (the `PUT`'s 400).
pub fn validate(outputs: &Outputs, fan_floor_pct: f64) -> Result<(), String> {
    if let Some(h) = &outputs.heater {
        h.validate("heater")?;
    }
    if let Some(f) = &outputs.fan {
        f.validate("fan")?;
    }
    if let Some(d) = &outputs.drum {
        d.validate("drum")?;
    }
    if let Some(d) = &outputs.damper {
        d.validate("damper")?;
    }
    if !(fan_floor_pct.is_finite() && (0.0..=100.0).contains(&fan_floor_pct)) {
        return Err("fan_floor_pct must be 0..100".into());
    }
    if fan_floor_pct > 0.0 {
        match (&outputs.heater, &outputs.fan) {
            (Some(_), Some(f)) if fan_floor_pct <= f.max => {}
            (Some(_), Some(_)) => {
                return Err("fan_floor_pct must be at or below the fan's max".into());
            }
            _ => {
                return Err("a fan floor needs both a heater and a fan output".into());
            }
        }
    }
    Ok(())
}

/// Which declared output a command needs; `None` for emergency stop and the lease.
pub fn output_of(cmd: &DeviceCommand) -> Option<&'static str> {
    match cmd {
        DeviceCommand::Fan { .. } => Some("fan"),
        DeviceCommand::Drum { .. } => Some("drum"),
        DeviceCommand::Damper { .. } => Some("damper"),
        DeviceCommand::EmergencyStop | DeviceCommand::Lease { .. } => None,
        // Heater %, on/off and everything the PID drives (setpoint, mode, gains, feedforward,
        // RoR limit) act on the heater. Listed, not `_`: a new command must say its output.
        DeviceCommand::Heater { .. }
        | DeviceCommand::HeaterEnable { .. }
        | DeviceCommand::Setpoint { .. }
        | DeviceCommand::Mode { .. }
        | DeviceCommand::Pid { .. }
        | DeviceCommand::Feedforward { .. }
        | DeviceCommand::RorLimit { .. } => Some("heater"),
    }
}

/// What the gate knows about a roaster: its control level, outputs and fan floor.
#[derive(Debug, Clone)]
pub struct Limits {
    pub kind: Kind,
    pub control: Control,
    pub outputs: Outputs,
    pub fan_floor_pct: f64,
}

/// Why the gate refused a command.
#[derive(Debug, Clone, PartialEq)]
pub enum Refusal {
    Control(Control),
    NotDeclared(&'static str),
}

impl Limits {
    /// The command the roaster gets, or why it gets none (`docs/roaster-setup.md`, "The
    /// gate", "Outputs"): the control level, then the output, then its range, then the fan
    /// floor while the heater is on.
    pub fn shape(&self, cmd: &DeviceCommand, heater_on: bool) -> Result<DeviceCommand, Refusal> {
        if !self.control.allows(cmd) {
            return Err(Refusal::Control(self.control));
        }
        let out = match output_of(cmd) {
            None => return Ok(cmd.clone()),
            Some(name) => self.outputs.get(name).ok_or(Refusal::NotDeclared(name))?,
        };
        Ok(match cmd {
            DeviceCommand::Heater { percent } => DeviceCommand::Heater {
                percent: out.shape(*percent),
            },
            DeviceCommand::Fan { percent } => {
                let mut p = out.shape(*percent);
                if heater_on && self.fan_floor_pct > 0.0 && p < self.fan_floor_pct {
                    p = out.at_least(self.fan_floor_pct);
                }
                DeviceCommand::Fan { percent: p }
            }
            // Drum and damper: the range only, no floor.
            DeviceCommand::Drum { percent } => DeviceCommand::Drum {
                percent: out.shape(*percent),
            },
            DeviceCommand::Damper { percent } => DeviceCommand::Damper {
                percent: out.shape(*percent),
            },
            other => other.clone(),
        })
    }

    /// Whether fan assist can run: both outputs declared.
    pub fn has_heater_and_fan(&self) -> bool {
        self.outputs.heater.is_some() && self.outputs.fan.is_some()
    }
}

/// A roaster's setup as the API shows it.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RoasterSetup {
    pub device_id: String,
    pub kind: Kind,
    pub control: Control,
    /// The outputs it accepts; a null output's commands are refused and its controls hidden.
    pub outputs: Outputs,
    /// While the heater is on, the fan never goes below this, %; 0: none.
    pub fan_floor_pct: f64,
    /// The extra values kept with a roast, drawn and usable in alarms (docs/channels.md)
    pub channels: Vec<Channel>,
    /// The virtual roaster's setup cannot change: it is always full.
    pub fixed: bool,
    /// When the owner (or migration 045) last set it; null for a roaster with no setup yet.
    pub updated_at: Option<String>,
}

/// A change to a setup (the owner's `PUT`). Outputs and floor left out keep theirs, or take the
/// new kind's defaults when the kind changes.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct SetupChange {
    pub kind: Kind,
    pub control: Control,
    #[serde(default)]
    pub outputs: Option<Outputs>,
    #[serde(default)]
    pub fan_floor_pct: Option<f64>,
    /// The declared channels (docs/channels.md); left out keeps the saved list.
    #[serde(default)]
    pub channels: Option<Vec<Channel>>,
}

#[derive(Debug)]
pub enum SetupError {
    /// The virtual roaster's setup is fixed, or the values are wrong: a 400.
    Invalid(String),
    Db(sqlx::Error),
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SetupError::Invalid(m) => f.write_str(m),
            SetupError::Db(e) => write!(f, "database error: {e}"),
        }
    }
}

#[derive(Clone)]
struct Entry {
    kind: Kind,
    control: Control,
    outputs: Outputs,
    fan_floor_pct: f64,
    channels: Vec<Channel>,
}

impl Entry {
    fn none() -> Self {
        Self::of_kind(Kind::Other, Control::Off)
    }

    fn of_kind(kind: Kind, control: Control) -> Self {
        Self {
            kind,
            control,
            outputs: kind.outputs(),
            fan_floor_pct: kind.fan_floor_pct(),
            channels: Vec::new(),
        }
    }

    fn virtual_roaster() -> Self {
        Self::of_kind(Kind::FluidBedElectric, Control::Full)
    }

    fn limits(&self) -> Limits {
        Limits {
            kind: self.kind,
            control: self.control,
            outputs: self.outputs.clone(),
            fan_floor_pct: self.fan_floor_pct,
        }
    }
}

/// The setups, held in memory so the gate costs no SQL; the table is the record.
pub struct Setups {
    db: SqlitePool,
    map: RwLock<HashMap<String, (Entry, String)>>,
}

type Row = (
    String,
    String,
    String,
    String,
    Option<String>,
    Option<f64>,
    Option<String>,
);

impl Setups {
    /// Load every row. Called once at start.
    pub async fn load(db: SqlitePool) -> anyhow::Result<Arc<Self>> {
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT device_id, kind, control, updated_at, outputs, fan_floor_pct, channels FROM roaster_setup",
        )
        .fetch_all(&db)
        .await?;
        let mut map = HashMap::new();
        for (device_id, kind, control, updated_at, outputs, floor, chans) in rows {
            let kind = Kind::parse(&kind);
            let control = Control::parse(&control).unwrap_or(Control::Off);
            // NULL (and anything unreadable) is the kind's default (migration 046).
            let outputs = outputs
                .and_then(|o| serde_json::from_str::<Outputs>(&o).ok())
                .unwrap_or_else(|| kind.outputs());
            let fan_floor_pct = floor
                .filter(|f| f.is_finite())
                .unwrap_or_else(|| kind.fan_floor_pct());
            let entry = Entry {
                kind,
                control,
                outputs,
                fan_floor_pct,
                channels: channels::parse(chans.as_deref()),
            };
            map.insert(device_id, (entry, updated_at));
        }
        Ok(Arc::new(Self {
            db,
            map: RwLock::new(map),
        }))
    }

    async fn entry(&self, device_id: &str) -> (Entry, Option<String>) {
        if virtual_roaster::is_simulated(device_id) {
            return (Entry::virtual_roaster(), None);
        }
        match self.map.read().await.get(device_id) {
            Some((e, at)) => (e.clone(), Some(at.clone())),
            None => (Entry::none(), None),
        }
    }

    /// Everything the gate checks.
    pub async fn limits(&self, device_id: &str) -> Limits {
        self.entry(device_id).await.0.limits()
    }

    /// The roaster's declared channels (docs/channels.md): what `process_telemetry` keeps and
    /// the alarms read, without SQL.
    pub async fn channels(&self, device_id: &str) -> Vec<Channel> {
        if virtual_roaster::is_simulated(device_id) {
            return Vec::new();
        }
        self.map
            .read()
            .await
            .get(device_id)
            .map(|(e, _)| e.channels.clone())
            .unwrap_or_default()
    }

    /// The keys of its declared °C channels: a TC4 column naming one follows the device's
    /// unit (docs/ingest-mapping.md, "Serial TC4").
    pub async fn temp_channels(&self, device_id: &str) -> Vec<String> {
        let all = self.channels(device_id).await;
        all.into_iter()
            .filter(super::channels::Channel::is_temp)
            .map(|c| c.key)
            .collect()
    }

    pub async fn get(&self, device_id: &str) -> RoasterSetup {
        let (e, at) = self.entry(device_id).await;
        RoasterSetup {
            device_id: device_id.to_string(),
            kind: e.kind,
            control: e.control,
            outputs: e.outputs,
            fan_floor_pct: e.fan_floor_pct,
            channels: e.channels,
            fixed: virtual_roaster::is_simulated(device_id),
            updated_at: at,
        }
    }

    /// Store a roaster's setup (the owner's `PUT`). The virtual roaster's is fixed. Answers the
    /// setup and whether the kind changed (the caller then resets the sequence and fan assist).
    pub async fn set(
        &self,
        device_id: &str,
        change: SetupChange,
    ) -> Result<(RoasterSetup, bool), SetupError> {
        if virtual_roaster::is_simulated(device_id) {
            return Err(SetupError::Invalid(
                "the virtual roaster always has full control".into(),
            ));
        }
        let at = crate::auth::tokens::now_text();
        // Hold the map's lock across the write so the table and the map change together.
        let mut map = self.map.write().await;
        let old = map.get(device_id).map(|(e, _)| e.clone());
        let kind_changed = old.as_ref().map(|e| e.kind) != Some(change.kind);
        let base = match &old {
            Some(e) if !kind_changed => e.clone(),
            _ => Entry::of_kind(change.kind, change.control),
        };
        let outputs = change.outputs.unwrap_or(base.outputs);
        let mut fan_floor_pct = change.fan_floor_pct.unwrap_or(base.fan_floor_pct);
        // Outputs given without a floor: a floor that no longer fits (no fan, or above its
        // max) is dropped rather than refused.
        if change.fan_floor_pct.is_none() && validate(&outputs, fan_floor_pct).is_err() {
            fan_floor_pct = 0.0;
        }
        validate(&outputs, fan_floor_pct).map_err(SetupError::Invalid)?;
        // Channels are the roaster's, not the kind's: a kind change keeps them.
        let mut chans = change
            .channels
            .unwrap_or_else(|| old.map(|e| e.channels).unwrap_or_default());
        channels::validate(&mut chans).map_err(SetupError::Invalid)?;
        let outputs_json = serde_json::to_string(&outputs).unwrap_or_else(|_| "{}".into());
        let channels_json = (!chans.is_empty())
            .then(|| serde_json::to_string(&chans).unwrap_or_else(|_| "[]".into()));
        sqlx::query(
            "INSERT INTO roaster_setup (device_id, kind, control, updated_at, outputs, fan_floor_pct, channels)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(device_id) DO UPDATE SET
               kind = excluded.kind, control = excluded.control, updated_at = excluded.updated_at,
               outputs = excluded.outputs, fan_floor_pct = excluded.fan_floor_pct,
               channels = excluded.channels",
        )
        .bind(device_id)
        .bind(change.kind.as_str())
        .bind(change.control.as_str())
        .bind(&at)
        .bind(&outputs_json)
        .bind(fan_floor_pct)
        .bind(channels_json)
        .execute(&self.db)
        .await
        .map_err(SetupError::Db)?;
        let entry = Entry {
            kind: change.kind,
            control: change.control,
            outputs,
            fan_floor_pct,
            channels: chans,
        };
        map.insert(device_id.to_string(), (entry, at));
        drop(map);
        tracing::info!(
            device_id,
            kind = change.kind.as_str(),
            control = change.control.as_str(),
            kind_changed,
            "roaster setup changed"
        );
        Ok((self.get(device_id).await, kind_changed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use roastwarden_core::commands::ControlMode;

    fn change(kind: Kind, control: Control) -> SetupChange {
        SetupChange {
            kind,
            control,
            outputs: None,
            fan_floor_pct: None,
            channels: None,
        }
    }

    fn every_command() -> Vec<DeviceCommand> {
        vec![
            DeviceCommand::Setpoint { value: 200.0 },
            DeviceCommand::Fan { percent: 80.0 },
            DeviceCommand::Heater { percent: 50.0 },
            DeviceCommand::Drum { percent: 40.0 },
            DeviceCommand::Damper { percent: 30.0 },
            DeviceCommand::Mode {
                mode: ControlMode::Manual,
            },
            DeviceCommand::HeaterEnable { enabled: true },
            DeviceCommand::HeaterEnable { enabled: false },
            DeviceCommand::Pid {
                kp: 1.0,
                ki: 0.1,
                kd: 0.0,
                persist: false,
            },
            DeviceCommand::EmergencyStop,
            DeviceCommand::Feedforward { value: None },
            DeviceCommand::Lease { ms: 0 },
            DeviceCommand::RorLimit {
                max: 0.0,
                min_temp: 0.0,
            },
        ]
    }

    #[test]
    fn off_refuses_everything_even_stop() {
        for c in every_command() {
            assert!(!Control::Off.allows(&c), "{} passed off", c.name());
        }
    }

    #[test]
    fn manual_passes_heater_fan_heater_enable_and_stop_only() {
        let passed: Vec<&str> = every_command()
            .iter()
            .filter(|c| Control::Manual.allows(c))
            .map(roastwarden_core::DeviceCommand::name)
            .collect();
        let want: Vec<&str> = every_command()
            .iter()
            .filter(|c| {
                matches!(
                    c,
                    DeviceCommand::Fan { .. }
                        | DeviceCommand::Heater { .. }
                        | DeviceCommand::Drum { .. }
                        | DeviceCommand::Damper { .. }
                        | DeviceCommand::HeaterEnable { .. }
                        | DeviceCommand::EmergencyStop
                )
            })
            .map(roastwarden_core::DeviceCommand::name)
            .collect();
        assert_eq!(passed, want);
        assert_eq!(passed.len(), 7);
    }

    #[test]
    fn full_passes_everything() {
        for c in every_command() {
            assert!(Control::Full.allows(&c));
        }
    }

    async fn db() -> SqlitePool {
        let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
        crate::db::run_migrations(&db).await.unwrap();
        db
    }

    #[tokio::test]
    async fn no_row_is_off_and_the_virtual_roaster_is_always_full() {
        let s = Setups::load(db().await).await.unwrap();
        assert_eq!(s.limits("esp32_new").await.control, Control::Off);
        let g = s.get("esp32_new").await;
        assert_eq!(
            (g.kind, g.control, g.fixed),
            (Kind::Other, Control::Off, false)
        );
        assert!(g.updated_at.is_none());
        assert_eq!(
            s.limits(virtual_roaster::DEVICE_ID).await.control,
            Control::Full
        );
        assert!(s.get(virtual_roaster::DEVICE_ID).await.fixed);
        assert!(s
            .set(
                virtual_roaster::DEVICE_ID,
                change(Kind::Logger, Control::Off)
            )
            .await
            .is_err());
        assert_eq!(
            s.limits(virtual_roaster::DEVICE_ID).await.control,
            Control::Full
        );
    }

    #[tokio::test]
    async fn a_setup_is_stored_and_survives_a_reload() {
        let pool = db().await;
        let s = Setups::load(pool.clone()).await.unwrap();
        let r = s
            .set("tc4_drum", change(Kind::DrumGas, Control::Manual))
            .await
            .unwrap()
            .0;
        assert_eq!((r.kind, r.control), (Kind::DrumGas, Control::Manual));
        assert_eq!(s.limits("tc4_drum").await.control, Control::Manual);
        let again = Setups::load(pool).await.unwrap();
        assert_eq!(again.limits("tc4_drum").await.control, Control::Manual);
        assert_eq!(again.get("tc4_drum").await.kind, Kind::DrumGas);
    }

    /// Migration 045 keeps every roaster that existed: full, as the fluid bed.
    #[tokio::test]
    async fn the_migration_gives_existing_roasters_full_control() {
        // Every migration, then 045, 046 and 048 undone, two roasters added, and both again.
        let pool = db().await;
        sqlx::query("DROP TABLE roaster_setup")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM schema_migrations WHERE name IN ('045_roaster_setup', '046_roaster_outputs', '048_roaster_channels')")
            .execute(&pool)
            .await
            .unwrap();
        for id in ["esp32_roaster_01", "virtual_roaster"] {
            sqlx::query(
                "INSERT INTO devices (id, name, device_id, status) VALUES (?, ?, ?, 'active')",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(id)
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        }
        crate::db::run_migrations(&pool).await.unwrap();
        let s = Setups::load(pool.clone()).await.unwrap();
        let esp = s.get("esp32_roaster_01").await;
        assert_eq!(
            (esp.kind, esp.control),
            (Kind::FluidBedElectric, Control::Full)
        );
        assert!(esp.updated_at.is_some());
        // A roaster discovered after the migration has no row: off.
        sqlx::query("INSERT INTO devices (id, name, device_id, status) VALUES ('x', 'n', 'esp32_later', 'pending')")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(s.limits("esp32_later").await.control, Control::Off);
    }

    /// The whole server: a roaster nobody set up gets nothing; manual lets the heater through
    /// but not a setpoint; only the owner changes a setup; the virtual roaster is unchanged.
    #[tokio::test]
    async fn the_gate_through_the_api() {
        use crate::test_support::{call, Harness};
        use axum::http::StatusCode;
        use serde_json::json;

        let h = Harness::new().await;
        let heater = |id: &str| format!("/api/roaster/{id}/control/heater");
        let setpoint = |id: &str| format!("/api/roaster/{id}/control/setpoint");

        let r = h
            .owner("POST", &heater("esp32_new"), Some(json!({ "percent": 30 })))
            .await;
        assert_eq!(r.status, StatusCode::CONFLICT, "{:?}", r.json());
        assert_eq!(r.json()["error"]["code"], "CONTROL_OFF");
        assert!(r.json()["error"]["message"]
            .as_str()
            .unwrap()
            .contains("control is off for esp32_new"));
        let stop = h
            .owner(
                "POST",
                "/api/roaster/esp32_new/control/emergency_stop",
                None,
            )
            .await;
        assert_eq!(stop.status, StatusCode::CONFLICT);

        let got = h.owner("GET", "/api/roaster/esp32_new/setup", None).await;
        assert_eq!(got.json()["control"], "off");
        assert_eq!(got.json()["kind"], "other");

        // An admin token reads it; switching control on waits for the owner's approval.
        let admin = h.token("admin").await;
        let body = json!({ "kind": "drum_gas", "control": "manual" });
        let t = call(
            &h.app,
            "PUT",
            "/api/roaster/esp32_new/setup",
            Some(&admin),
            Some(body.clone()),
        )
        .await;
        assert_eq!(t.status, StatusCode::ACCEPTED);
        let t = call(
            &h.app,
            "GET",
            "/api/roaster/esp32_new/setup",
            Some(&admin),
            None,
        )
        .await;
        assert_eq!(t.status, StatusCode::OK);

        let put = h
            .owner("PUT", "/api/roaster/esp32_new/setup", Some(body))
            .await;
        assert_eq!(put.status, StatusCode::OK, "{:?}", put.json());
        assert_eq!(put.json()["control"], "manual");
        let r = h
            .owner("POST", &heater("esp32_new"), Some(json!({ "percent": 30 })))
            .await;
        assert!(r.status.is_success(), "{:?}", r.json());
        let r = h
            .owner(
                "POST",
                &setpoint("esp32_new"),
                Some(json!({ "value": 200 })),
            )
            .await;
        assert_eq!(r.status, StatusCode::CONFLICT);
        assert!(r.json()["error"]["message"]
            .as_str()
            .unwrap()
            .contains("setpoint needs full control"));

        // The virtual roaster: full, fixed, its control unchanged.
        let vr = virtual_roaster::DEVICE_ID;
        let v = h
            .owner("GET", &format!("/api/roaster/{vr}/setup"), None)
            .await;
        assert_eq!(
            (v.json()["control"].clone(), v.json()["fixed"].clone()),
            (json!("full"), json!(true))
        );
        let no = h
            .owner(
                "PUT",
                &format!("/api/roaster/{vr}/setup"),
                Some(json!({ "kind": "logger", "control": "off" })),
            )
            .await;
        assert_eq!(no.status, StatusCode::BAD_REQUEST);
        let r = h
            .owner("POST", &setpoint(vr), Some(json!({ "value": 200 })))
            .await;
        assert!(r.status.is_success(), "{:?}", r.json());
    }

    fn limits(kind: Kind, control: Control) -> Limits {
        Entry::of_kind(kind, control).limits()
    }

    fn fan(l: &Limits, pct: f64, heater_on: bool) -> f64 {
        match l.shape(&DeviceCommand::Fan { percent: pct }, heater_on) {
            Ok(DeviceCommand::Fan { percent }) => percent,
            other => panic!("{other:?}"),
        }
    }

    fn heater(l: &Limits, pct: f64) -> f64 {
        match l.shape(&DeviceCommand::Heater { percent: pct }, false) {
            Ok(DeviceCommand::Heater { percent }) => percent,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_undeclared_output_refuses_its_commands() {
        let logger = limits(Kind::Logger, Control::Full);
        for c in every_command() {
            let r = logger.shape(&c, false);
            match c {
                DeviceCommand::EmergencyStop | DeviceCommand::Lease { .. } => {
                    assert_eq!(r, Ok(c.clone()));
                }
                DeviceCommand::Fan { .. } => assert_eq!(r, Err(Refusal::NotDeclared("fan"))),
                DeviceCommand::Drum { .. } => assert_eq!(r, Err(Refusal::NotDeclared("drum"))),
                DeviceCommand::Damper { .. } => {
                    assert_eq!(r, Err(Refusal::NotDeclared("damper")));
                }
                _ => assert_eq!(r, Err(Refusal::NotDeclared("heater")), "{}", c.name()),
            }
        }
        // The control level comes first.
        assert_eq!(
            limits(Kind::Logger, Control::Off).shape(&DeviceCommand::EmergencyStop, false),
            Err(Refusal::Control(Control::Off))
        );
    }

    #[test]
    fn ranges_clamp_steps_round_and_zero_is_off() {
        let mut l = limits(Kind::DrumGas, Control::Manual);
        l.outputs.heater = Some(Output {
            label: "Burner".into(),
            min: 20.0,
            max: 80.0,
            step: 10.0,
        });
        assert_eq!(heater(&l, 95.0), 80.0);
        assert_eq!(heater(&l, 5.0), 20.0);
        assert_eq!(heater(&l, 44.0), 40.0);
        assert_eq!(heater(&l, 46.0), 50.0);
        assert_eq!(heater(&l, 0.0), 0.0);
        assert_eq!(heater(&l, -3.0), 0.0);
        assert_eq!(heater(&l, f64::NAN), 0.0);
        // The default outputs pass any 0..100 unchanged.
        let fb = limits(Kind::FluidBedElectric, Control::Full);
        assert_eq!(heater(&fb, 37.5), 37.5);
        assert_eq!(fan(&fb, 100.0, false), 100.0);
    }

    /// Drum and damper (docs/ingest-mapping.md): no kind declares them; declared, they are
    /// shaped to their range with no floor, and manual control lets them through.
    #[test]
    fn drum_and_damper_are_declared_by_the_owner_and_shaped_to_their_range() {
        for k in [
            Kind::FluidBedElectric,
            Kind::DrumElectric,
            Kind::DrumGas,
            Kind::Logger,
            Kind::Other,
        ] {
            let o = k.outputs();
            assert!(o.drum.is_none() && o.damper.is_none(), "{}", k.as_str());
        }
        let mut l = limits(Kind::DrumGas, Control::Manual);
        assert_eq!(
            l.shape(&DeviceCommand::Drum { percent: 50.0 }, false),
            Err(Refusal::NotDeclared("drum"))
        );
        l.outputs.drum = Some(Output {
            label: "Drum".into(),
            min: 30.0,
            max: 70.0,
            step: 5.0,
        });
        l.outputs.damper = Some(Output::percent("Damper"));
        let drum = |l: &Limits, p: f64| match l.shape(&DeviceCommand::Drum { percent: p }, true) {
            Ok(DeviceCommand::Drum { percent }) => percent,
            other => panic!("{other:?}"),
        };
        assert_eq!(drum(&l, 90.0), 70.0);
        assert_eq!(drum(&l, 10.0), 30.0);
        assert_eq!(drum(&l, 52.0), 50.0);
        assert_eq!(drum(&l, 0.0), 0.0, "0 is off");
        assert_eq!(
            l.shape(&DeviceCommand::Damper { percent: 12.5 }, true),
            Ok(DeviceCommand::Damper { percent: 12.5 })
        );
        // Off refuses them as everything else.
        l.control = Control::Off;
        assert_eq!(
            l.shape(&DeviceCommand::Damper { percent: 12.5 }, false),
            Err(Refusal::Control(Control::Off))
        );
        // Their ranges are checked as the heater's and fan's.
        let mut o = Kind::DrumGas.outputs();
        o.drum = Some(Output {
            label: "Drum".into(),
            min: 80.0,
            max: 20.0,
            step: 0.0,
        });
        assert!(validate(&o, 0.0).unwrap_err().starts_with("drum"));
        // A setup stored before drum and damper existed still reads.
        let old: Outputs = serde_json::from_str(
            r#"{"heater":{"label":"Heater","min":0,"max":100,"step":0},"fan":null}"#,
        )
        .unwrap();
        assert!(old.drum.is_none() && old.damper.is_none() && old.heater.is_some());
    }

    #[test]
    fn the_fan_floor_holds_only_while_the_heater_is_on() {
        let fb = limits(Kind::FluidBedElectric, Control::Manual);
        assert_eq!(fb.fan_floor_pct, 45.0);
        assert_eq!(fan(&fb, 30.0, true), 45.0);
        assert_eq!(fan(&fb, 0.0, true), 45.0, "fan off while heating is raised");
        assert_eq!(fan(&fb, 60.0, true), 60.0);
        assert_eq!(fan(&fb, 30.0, false), 30.0);
        assert_eq!(fan(&fb, 0.0, false), 0.0, "COOL END switches the fan off");
        // A drum has no floor.
        let drum = limits(Kind::DrumElectric, Control::Manual);
        assert_eq!(fan(&drum, 10.0, true), 10.0);
        // With a step the floor rounds up, never below it.
        let mut stepped = fb.clone();
        stepped.outputs.fan = Some(Output {
            label: "Fan".into(),
            min: 0.0,
            max: 100.0,
            step: 20.0,
        });
        assert_eq!(fan(&stepped, 10.0, true), 60.0);
    }

    #[test]
    fn heater_on_reads_the_flag_or_the_output() {
        use crate::transports::transport::heater_on_in;
        let mut s = roastwarden_core::Sample::default();
        assert!(!heater_on_in(&s));
        s.heater = Some(30.0);
        assert!(heater_on_in(&s), "no flag: heater output above 0");
        s.heater_enable = Some(false);
        assert!(!heater_on_in(&s), "the flag wins");
        s.heater = Some(0.0);
        s.heater_enable = Some(true);
        assert!(heater_on_in(&s));
    }

    #[test]
    fn each_kind_has_its_defaults() {
        let fb = Kind::FluidBedElectric;
        assert_eq!(fb.outputs().heater.unwrap().label, "Heater");
        assert_eq!(fb.fan_floor_pct(), 45.0);
        assert!(fb.sequence_acts());
        let gas = Kind::DrumGas.outputs();
        assert_eq!(
            (gas.heater.unwrap().label, gas.fan.unwrap().label),
            ("Burner".to_string(), "Air".to_string())
        );
        assert_eq!(Kind::Logger.outputs(), Outputs::default());
        for k in [Kind::DrumElectric, Kind::DrumGas, Kind::Logger, Kind::Other] {
            assert_eq!(k.fan_floor_pct(), 0.0);
            assert!(!k.sequence_acts());
            assert!(validate(&k.outputs(), k.fan_floor_pct()).is_ok());
        }
        assert!(validate(&fb.outputs(), fb.fan_floor_pct()).is_ok());
    }

    #[test]
    fn bad_outputs_are_refused() {
        let o = |min: f64, max: f64, step: f64| Outputs {
            heater: Some(Output {
                label: "Heater".into(),
                min,
                max,
                step,
            }),
            fan: Some(Output::percent("Fan")),
            ..Outputs::default()
        };
        assert!(validate(&o(0.0, 100.0, 0.0), 45.0).is_ok());
        assert!(validate(&o(50.0, 50.0, 0.0), 0.0).is_err());
        assert!(validate(&o(0.0, 120.0, 0.0), 0.0).is_err());
        assert!(validate(&o(-1.0, 100.0, 0.0), 0.0).is_err());
        assert!(validate(&o(0.0, 100.0, 101.0), 0.0).is_err());
        assert!(validate(&o(0.0, 100.0, 0.0), 101.0).is_err());
        let mut no_fan = o(0.0, 100.0, 0.0);
        no_fan.fan = None;
        assert!(validate(&no_fan, 45.0).is_err(), "a floor needs a fan");
        assert!(validate(&no_fan, 0.0).is_ok());
        let mut blank = o(0.0, 100.0, 0.0);
        blank.heater.as_mut().unwrap().label = " ".into();
        assert!(validate(&blank, 0.0).is_err());
    }

    #[tokio::test]
    async fn a_kind_change_takes_its_defaults_and_a_same_kind_put_keeps_the_outputs() {
        let s = Setups::load(db().await).await.unwrap();
        let (r, changed) = s
            .set("r1", change(Kind::FluidBedElectric, Control::Full))
            .await
            .unwrap();
        assert!(changed);
        assert_eq!(r.fan_floor_pct, 45.0);
        // The owner narrows the heater; a PUT of the same kind without outputs keeps it.
        let mut narrow = Kind::FluidBedElectric.outputs();
        narrow.heater.as_mut().unwrap().max = 90.0;
        let (r, changed) = s
            .set(
                "r1",
                SetupChange {
                    outputs: Some(narrow.clone()),
                    ..change(Kind::FluidBedElectric, Control::Full)
                },
            )
            .await
            .unwrap();
        assert!(!changed);
        assert_eq!(r.outputs, narrow);
        let (r, _) = s
            .set("r1", change(Kind::FluidBedElectric, Control::Manual))
            .await
            .unwrap();
        assert_eq!((r.outputs.clone(), r.control), (narrow, Control::Manual));
        // A drum: its outputs, no floor.
        let (r, changed) = s
            .set("r1", change(Kind::DrumGas, Control::Manual))
            .await
            .unwrap();
        assert!(changed);
        assert_eq!(r.outputs, Kind::DrumGas.outputs());
        assert_eq!(r.fan_floor_pct, 0.0);
        // Outputs with no fan drop a floor that no longer fits rather than refuse.
        let (r, _) = s
            .set(
                "r1",
                SetupChange {
                    outputs: Some(Outputs {
                        fan: None,
                        ..Kind::FluidBedElectric.outputs()
                    }),
                    ..change(Kind::FluidBedElectric, Control::Full)
                },
            )
            .await
            .unwrap();
        assert_eq!(r.fan_floor_pct, 0.0);
        assert!(matches!(
            s.set(
                "r1",
                SetupChange {
                    fan_floor_pct: Some(45.0),
                    ..change(Kind::FluidBedElectric, Control::Full)
                }
            )
            .await,
            Err(SetupError::Invalid(_))
        ));
    }

    /// Migration 046 keeps every setup's behaviour: NULL is the kind's defaults, so the
    /// fluid bed keeps heater and fan 0..100 and its 45 % floor.
    #[tokio::test]
    async fn migration_046_gives_each_row_its_kinds_defaults() {
        let pool = db().await;
        sqlx::query(
            "INSERT INTO roaster_setup (device_id, kind, control, updated_at) VALUES
             ('esp32_roaster_01', 'fluid_bed_electric', 'full', '2026-10-31T00:00:00Z'),
             ('drum', 'drum_gas', 'manual', '2026-10-31T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let s = Setups::load(pool).await.unwrap();
        let esp = s.get("esp32_roaster_01").await;
        assert_eq!(esp.outputs, Kind::FluidBedElectric.outputs());
        assert_eq!(esp.fan_floor_pct, 45.0);
        let l = s.limits("esp32_roaster_01").await;
        assert_eq!(fan(&l, 100.0, true), 100.0);
        assert_eq!(heater(&l, 35.0), 35.0);
        let drum = s.get("drum").await;
        assert_eq!(drum.outputs.heater.unwrap().label, "Burner");
        assert_eq!(drum.fan_floor_pct, 0.0);
        // The virtual roaster is the fluid bed, fixed.
        let v = s.get(virtual_roaster::DEVICE_ID).await;
        assert_eq!(
            (v.outputs, v.fan_floor_pct),
            (Kind::FluidBedElectric.outputs(), 45.0)
        );
    }

    /// The drum and damper routes (docs/ingest-mapping.md, "Drum and damper"): off refuses,
    /// undeclared is `OUTPUT_NOT_DECLARED`, declared goes out on control/drum shaped to range.
    #[tokio::test]
    async fn drum_and_damper_through_the_api() {
        use crate::test_support::Harness;
        use axum::http::StatusCode;
        use serde_json::json;

        let h = Harness::new().await;
        let drum = |p: f64| json!({ "percent": p });
        let r = h
            .owner("POST", "/api/roaster/gas1/control/drum", Some(drum(50.0)))
            .await;
        assert_eq!(r.status, StatusCode::CONFLICT);
        assert_eq!(r.json()["error"]["code"], "CONTROL_OFF");

        let mut outputs = serde_json::to_value(Kind::DrumGas.outputs()).unwrap();
        outputs["drum"] = json!({ "label": "Drum", "min": 30, "max": 70, "step": 0 });
        let r = h
            .owner(
                "PUT",
                "/api/roaster/gas1/setup",
                Some(json!({ "kind": "drum_gas", "control": "manual", "outputs": outputs })),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert_eq!(r.json()["outputs"]["drum"]["label"], "Drum");
        assert!(r.json()["outputs"]["damper"].is_null());

        let r = h
            .owner("POST", "/api/roaster/gas1/control/damper", Some(drum(20.0)))
            .await;
        assert_eq!(r.status, StatusCode::CONFLICT);
        assert_eq!(r.json()["error"]["code"], "OUTPUT_NOT_DECLARED");
        assert!(r.json()["error"]["message"]
            .as_str()
            .unwrap()
            .contains("gas1 has no damper output"));
        let bad = h
            .owner("POST", "/api/roaster/gas1/control/drum", Some(drum(120.0)))
            .await;
        assert_eq!(bad.status, StatusCode::BAD_REQUEST);

        // Declared: on MQTT, roaster/gas1/control/drum, clamped to the drum's 70 %.
        h.state
            .mqtt
            .subscribe("roaster/gas1/control/#", rumqttc::QoS::AtMostOnce)
            .await
            .unwrap();
        let mut rx = h.state.mqtt.events();
        let r = h
            .owner("POST", "/api/roaster/gas1/control/drum", Some(drum(90.0)))
            .await;
        assert_eq!(r.status, StatusCode::NO_CONTENT, "{:?}", r.json());
        let got = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if let Ok(roastwarden_mqtt::MqttEvent::Publish { topic, payload }) = rx.recv().await
                    && topic.ends_with("/drum") {
                        return (topic, String::from_utf8(payload).unwrap());
                    }
            }
        })
        .await
        .unwrap();
        assert_eq!(
            got,
            ("roaster/gas1/control/drum".to_string(), "70".to_string())
        );
    }

    /// The setup PUT from a token (docs/roaster-setup.md, "Tokens and the setup"): an admin
    /// token sets up a roaster at control off at once, on a roaster tokens may not control;
    /// leaving control on is held for the owner's approval (202) and applied when approved;
    /// turning it off runs at once; a read-only token never.
    #[tokio::test]
    async fn an_admin_token_switches_control_on_only_with_the_owners_approval() {
        use crate::test_support::{call, Harness};
        use axum::http::StatusCode;
        use serde_json::json;
        let h = Harness::new().await;
        let admin = h.token("admin").await;
        let ro = h.token("read_only").await;
        let put = |token: String, id: &'static str, body: serde_json::Value| {
            let app = h.app.clone();
            async move {
                call(
                    &app,
                    "PUT",
                    &format!("/api/roaster/{id}/setup"),
                    Some(&token),
                    Some(body),
                )
                .await
            }
        };
        let logger = json!({ "kind": "logger", "control": "off" });
        // No setup yet (off): off → off passes, with outputs and channels
        let r = put(
            admin.clone(),
            "tc4_popper",
            json!({ "kind": "logger", "control": "off",
                "channels": [{ "key": "t3", "label": "Exhaust", "unit": "C" }] }),
        )
        .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert_eq!(r.json()["kind"], "logger");
        assert_eq!(r.json()["channels"][0]["key"], "t3");
        let url = "/api/roaster/tc4_popper/setup";
        // Raising control waits for the owner: nothing changes until they approve
        let raise = json!({ "kind": "logger", "control": "manual" });
        let r = put(admin.clone(), "tc4_popper", raise.clone()).await;
        assert_eq!(r.status, StatusCode::ACCEPTED, "{:?}", r.json());
        let held = r.json();
        assert_eq!(held["approval"]["action"], "roaster.control");
        assert_eq!(held["approval"]["device_id"], "tc4_popper");
        assert_eq!(
            held["approval"]["summary"],
            "Set tc4_popper's control to manual"
        );
        assert_eq!(h.owner("GET", url, None).await.json()["control"], "off");
        // The same call again is the same request
        let again = put(admin.clone(), "tc4_popper", raise).await;
        assert_eq!(again.json()["approval"]["id"], held["approval"]["id"]);
        // The owner approves: the call is made as the token, and control is on
        let id = held["approval"]["id"].as_str().unwrap();
        let d = h
            .owner("POST", &format!("/api/approvals/{id}/approve"), None)
            .await;
        assert_eq!(d.status, StatusCode::OK, "{:?}", d.json());
        assert_eq!(d.json()["status"], "done", "{:?}", d.json());
        assert_eq!(d.json()["result"]["control"], "manual");
        assert_eq!(h.owner("GET", url, None).await.json()["control"], "manual");
        // A change that keeps control on is held too; a denied one changes nothing
        let r = put(
            admin.clone(),
            "tc4_popper",
            json!({ "kind": "logger", "control": "full" }),
        )
        .await;
        assert_eq!(r.status, StatusCode::ACCEPTED);
        let id = r.json()["approval"]["id"].as_str().unwrap().to_string();
        let d = h
            .owner("POST", &format!("/api/approvals/{id}/deny"), None)
            .await;
        assert_eq!(d.json()["status"], "denied", "{:?}", d.json());
        assert_eq!(h.owner("GET", url, None).await.json()["control"], "manual");
        // Turning control off runs at once
        let r = put(admin.clone(), "tc4_popper", logger.clone()).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert_eq!(h.owner("GET", url, None).await.json()["control"], "off");
        // A read-only token may not at all
        let r = put(ro.clone(), "tc4_popper", logger.clone()).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);
        assert_eq!(r.json()["error"]["code"], "TOKEN_SCOPE");
        let r = put(
            ro,
            "tc4_popper",
            json!({ "kind": "logger", "control": "full" }),
        )
        .await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);
    }

    /// Channels (docs/channels.md): stored, read back after a reload, kept by a PUT without
    /// them and by a kind change, refused when wrong; the virtual roaster has none.
    #[tokio::test]
    async fn channels_are_stored_kept_and_checked() {
        use crate::test_support::Harness;
        use axum::http::StatusCode;
        use serde_json::json;

        let h = Harness::new().await;
        let url = "/api/roaster/tc4/setup";
        let r = h.owner("GET", url, None).await;
        assert_eq!(r.json()["channels"], json!([]));
        let r = h
            .owner(
                "PUT",
                url,
                Some(json!({ "kind": "drum_gas", "control": "off", "channels": [
                    { "key": "t3", "label": "Exhaust", "unit": "°C" },
                    { "key": "burner_kpa", "label": "Gas", "unit": "kPa", "max": 4 }
                ] })),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert_eq!(r.json()["channels"][0]["unit"], "C");
        assert_eq!(r.json()["channels"][1]["max"], 4.0);
        // A PUT without channels keeps them, a kind change too.
        let r = h
            .owner(
                "PUT",
                url,
                Some(json!({ "kind": "drum_electric", "control": "manual" })),
            )
            .await;
        assert_eq!(r.json()["channels"].as_array().unwrap().len(), 2);
        let s = h.state.transports.setups();
        assert_eq!(s.channels("tc4").await.len(), 2);
        let again = Setups::load(h.state.db.clone()).await.unwrap();
        assert_eq!(again.channels("tc4").await[1].label, "Gas");
        // A typed field is not a channel; nothing changes on a 400.
        let bad = h
            .owner(
                "PUT",
                url,
                Some(json!({ "kind": "drum_electric", "control": "manual",
                    "channels": [{ "key": "beanTemp", "label": "BT", "unit": "C" }] })),
            )
            .await;
        assert_eq!(bad.status, StatusCode::BAD_REQUEST);
        assert_eq!(s.channels("tc4").await.len(), 2);
        // An empty list clears them (stored as NULL).
        let r = h
            .owner(
                "PUT",
                url,
                Some(json!({ "kind": "drum_electric", "control": "manual", "channels": [] })),
            )
            .await;
        assert_eq!(r.json()["channels"], json!([]));
        let raw: Option<String> =
            sqlx::query_scalar("SELECT channels FROM roaster_setup WHERE device_id = 'tc4'")
                .fetch_one(&h.state.db)
                .await
                .unwrap();
        assert!(raw.is_none());
        let v = h
            .owner("GET", "/api/roaster/virtual_roaster/setup", None)
            .await;
        assert_eq!(v.json()["channels"], json!([]));
    }

    /// Through the API: a logger refuses the heater with `OUTPUT_NOT_DECLARED`; a drum's
    /// sequence records only; a kind change resets the sequence; the floor checks the
    /// sequence's fans.
    #[tokio::test]
    async fn outputs_and_kinds_through_the_api() {
        use crate::test_support::Harness;
        use axum::http::StatusCode;
        use serde_json::json;

        let h = Harness::new().await;
        let put = |kind: &str, control: &str| json!({ "kind": kind, "control": control });

        let r = h
            .owner(
                "PUT",
                "/api/roaster/tc4/setup",
                Some(put("logger", "manual")),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert_eq!(
            r.json()["outputs"],
            json!({ "heater": null, "fan": null, "drum": null, "damper": null })
        );
        let r = h
            .owner(
                "POST",
                "/api/roaster/tc4/control/heater",
                Some(json!({ "percent": 30 })),
            )
            .await;
        assert_eq!(r.status, StatusCode::CONFLICT);
        assert_eq!(r.json()["error"]["code"], "OUTPUT_NOT_DECLARED");
        assert!(r.json()["error"]["message"]
            .as_str()
            .unwrap()
            .contains("tc4 has no heater output"));

        // A gas drum with full control: its sequence records only, and takes a low fan.
        let r = h
            .owner(
                "PUT",
                "/api/roaster/tc4/setup",
                Some(put("drum_gas", "full")),
            )
            .await;
        assert_eq!(r.json()["outputs"]["heater"]["label"], "Burner");
        let seq = h.owner("GET", "/api/roaster/tc4/sequence", None).await;
        assert_eq!(seq.json()["config"]["charge_enabled"], false);
        assert_eq!(seq.json()["config"]["drop_enabled"], false);
        let mut cfg = seq.json()["config"].clone();
        cfg["charge_fan_pct"] = json!(20);
        let ok = h
            .owner("PUT", "/api/roaster/tc4/sequence", Some(cfg.clone()))
            .await;
        assert_eq!(ok.status, StatusCode::OK, "{:?}", ok.json());

        // Now a fluid bed: the sequence resets to the fluid bed's, and 20 % is below its floor.
        let r = h
            .owner(
                "PUT",
                "/api/roaster/tc4/setup",
                Some(put("fluid_bed_electric", "full")),
            )
            .await;
        assert_eq!(r.json()["fan_floor_pct"], 45.0);
        let seq = h.owner("GET", "/api/roaster/tc4/sequence", None).await;
        assert_eq!(seq.json()["config"]["charge_enabled"], true);
        assert_eq!(seq.json()["config"]["charge_fan_pct"], 100.0);
        let low = h.owner("PUT", "/api/roaster/tc4/sequence", Some(cfg)).await;
        assert_eq!(low.status, StatusCode::BAD_REQUEST);

        // A same-kind PUT leaves the sequence alone.
        let mut keep = h
            .owner("GET", "/api/roaster/tc4/sequence", None)
            .await
            .json()["config"]
            .clone();
        keep["charge_heater_pct"] = json!(35);
        h.owner("PUT", "/api/roaster/tc4/sequence", Some(keep))
            .await;
        h.owner(
            "PUT",
            "/api/roaster/tc4/setup",
            Some(put("fluid_bed_electric", "manual")),
        )
        .await;
        let seq = h.owner("GET", "/api/roaster/tc4/sequence", None).await;
        assert_eq!(seq.json()["config"]["charge_heater_pct"], 35.0);

        // A bad range is a 400.
        let bad = h
            .owner(
                "PUT",
                "/api/roaster/tc4/setup",
                Some(json!({ "kind": "fluid_bed_electric", "control": "full",
                    "outputs": { "heater": { "label": "Heater", "min": 80, "max": 20 }, "fan": null } })),
            )
            .await;
        assert_eq!(bad.status, StatusCode::BAD_REQUEST, "{:?}", bad.json());

        // A roaster nobody set up: its sequence is `other`'s, which records only.
        let seq = h
            .owner("GET", "/api/roaster/never_seen/sequence", None)
            .await;
        assert_eq!(seq.json()["config"]["charge_enabled"], false);
    }
}
