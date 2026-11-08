//! A test bench: a simulated process with a real sensor's faults, to try the experiment and
//! the tuning offline. The tests use it, and so can anyone reusing the crate to check a
//! configuration before running it on hardware.
//!
//! The process is a [`Model`] around an operating point: `y = offset + model(u)`, with optional
//! drift (the process changing during the test, like beans drying), white noise, quantisation
//! (a MAX6675 reads in 0.25 °C steps), oversampling (several reads averaged per sample), and an
//! actuator limited to 0..100.

use crate::model::Model;

#[derive(Debug, Clone)]
pub struct BenchConfig {
    pub model: Model,
    /// Output at zero input, e.g. ambient temperature.
    pub offset: f64,
    /// Sensor noise per read, standard deviation.
    pub noise: f64,
    /// Sensor step (0 for none).
    pub quantum: f64,
    /// Reads averaged into each sample.
    pub oversample: u32,
    /// Seconds between samples.
    pub dt: f64,
    /// The process gain drifts by this fraction per hour (beans drying: negative).
    pub gain_drift_per_h: f64,
    /// The output drifts by this much per hour on its own.
    pub offset_drift_per_h: f64,
    pub seed: u64,
}

impl BenchConfig {
    pub fn new(model: Model, offset: f64) -> BenchConfig {
        BenchConfig {
            model,
            offset,
            noise: 0.0,
            quantum: 0.0,
            oversample: 1,
            dt: 1.0,
            gain_drift_per_h: 0.0,
            offset_drift_per_h: 0.0,
            seed: 1,
        }
    }

    /// A process like the roaster's, read like the MAX6675 is today.
    pub fn roaster_like(model: Model) -> BenchConfig {
        BenchConfig {
            noise: 0.25,
            quantum: 0.25,
            ..BenchConfig::new(model, 25.0)
        }
    }
}

/// The running bench. `step` applies an input for one sample period and returns the next
/// reading.
pub struct Bench {
    pub cfg: BenchConfig,
    pub time: f64,
    x1: f64,
    x2: f64,
    /// Inputs applied, newest last, one per integration step (for the dead time).
    history: std::collections::VecDeque<f64>,
    h: f64,
    rng: u64,
}

impl Bench {
    /// Start at steady state with input `u0`.
    pub fn new(cfg: BenchConfig, u0: f64) -> Bench {
        let h = (cfg.dt / 20.0).min(0.05);
        let y0 = cfg.model.k * u0;
        let delay_steps = (cfg.model.l / h).round() as usize;
        let rng = cfg.seed.max(1).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        Bench {
            time: 0.0,
            x1: y0,
            x2: y0,
            history: std::iter::repeat_n(u0, delay_steps).collect(),
            h,
            rng,
            cfg,
        }
    }

    /// The process output now, without the sensor.
    pub fn truth(&self) -> f64 {
        let m = &self.cfg.model;
        let y = if m.t2 > 0.0 { self.x2 } else { self.x1 };
        y + self.offset_now()
    }

    fn offset_now(&self) -> f64 {
        self.cfg.offset + self.cfg.offset_drift_per_h * self.time / 3600.0
    }

    fn gain_now(&self) -> f64 {
        self.cfg.model.k * (1.0 + self.cfg.gain_drift_per_h * self.time / 3600.0)
    }

    fn gauss(&mut self) -> f64 {
        // Box–Muller on xorshift64*.
        let mut uni = || {
            self.rng ^= self.rng >> 12;
            self.rng ^= self.rng << 25;
            self.rng ^= self.rng >> 27;
            let v = self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D);
            ((v >> 11) as f64 + 0.5) / (1u64 << 53) as f64
        };
        let (a, b) = (uni(), uni());
        (-2.0 * a.ln()).sqrt() * (std::f64::consts::TAU * b).cos()
    }

    fn read(&mut self) -> f64 {
        let mut v = self.truth() + self.cfg.noise * self.gauss();
        if self.cfg.quantum > 0.0 {
            v = (v / self.cfg.quantum).round() * self.cfg.quantum;
        }
        v
    }

    /// Apply `u` (clamped to 0..100) for one sample period; return the sample at its end.
    pub fn step(&mut self, u: f64) -> f64 {
        let u = u.clamp(0.0, 100.0);
        let m = self.cfg.model;
        let steps = (self.cfg.dt / self.h).round().max(1.0) as usize;
        let reads = self.cfg.oversample.max(1) as usize;
        let read_every = (steps / reads).max(1);
        let mut sum = 0.0;
        let mut count = 0;
        for i in 0..steps {
            self.history.push_back(u);
            let delayed = self.history.pop_front().unwrap_or(u);
            let k = self.gain_now();
            let e1 = (-self.h / m.t1).exp();
            let x1_new = self.x1 * e1 + (1.0 - e1) * k * delayed;
            if m.t2 > 0.0 {
                let e2 = (-self.h / m.t2).exp();
                self.x2 = self.x2 * e2 + (1.0 - e2) * 0.5 * (self.x1 + x1_new);
            }
            self.x1 = x1_new;
            self.time += self.h;
            // Reads spread over the period, the last at its end.
            if (steps - 1 - i).is_multiple_of(read_every) && count < reads {
                sum += self.read();
                count += 1;
            }
        }
        sum / count.max(1) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steady_state_and_step() {
        let mut b = Bench::new(BenchConfig::new(Model::fotd(2.0, 30.0, 3.0), 25.0), 50.0);
        assert!((b.step(50.0) - 125.0).abs() < 1e-9);
        for _ in 0..3 {
            b.step(60.0);
        }
        // The dead time has passed; the lag is only starting.
        let y = b.step(60.0);
        assert!(y > 125.0 && y < 127.0, "{y}");
        for _ in 0..400 {
            b.step(60.0);
        }
        assert!((b.step(60.0) - 145.0).abs() < 1e-3);
    }

    #[test]
    fn oversampling_shrinks_the_noise() {
        let spread = |n: u32| {
            let mut cfg = BenchConfig::roaster_like(Model::fotd(2.0, 30.0, 3.0));
            cfg.oversample = n;
            let mut b = Bench::new(cfg, 50.0);
            let v: Vec<f64> = (0..2000).map(|_| b.step(50.0)).collect();
            let m = v.iter().sum::<f64>() / v.len() as f64;
            (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / v.len() as f64).sqrt()
        };
        let (one, four) = (spread(1), spread(4));
        assert!(four < one * 0.6, "{one} {four}");
    }
}
