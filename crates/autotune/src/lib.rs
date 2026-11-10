//! Relay autotuning for slow single-loop processes (docs/autotune-2.md).
//!
//! The crate knows nothing about coffee: it takes a process input (an actuator %, like a
//! heater) and a measured output (a temperature), runs a relay experiment on them, identifies
//! a model, designs PI gains and says how robust they are. Roastwarden uses it for the roaster
//! and the virtual roaster; it is meant to be reusable for ovens, kilns and other heaters.

pub mod analysis;
pub mod bench;
pub mod design;
pub mod fit;
pub mod harmonic;
pub mod model;
pub mod relay;
pub mod robust;

pub use model::{C64, Model};
pub use robust::{Pid, Robustness, evaluate};

/// Serde for figures that can be infinite (a gain margin when the phase never reaches −180°,
/// a profile interval with no upper end): JSON has no infinity, so they are written as `null`
/// and read back as infinite.
pub(crate) mod nonfinite {
    use serde::{Deserialize, Deserializer, Serializer};

    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's serialize_with passes the field by reference"
    )]
    pub fn serialize<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
        if v.is_finite() {
            s.serialize_f64(*v)
        } else {
            s.serialize_none()
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
        Ok(Option::<f64>::deserialize(d)?.unwrap_or(f64::INFINITY))
    }
}
