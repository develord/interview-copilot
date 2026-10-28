//! Level metering for capture diagnostics.

pub const SILENCE_DBFS: f32 = -100.0;

pub fn to_dbfs(amplitude: f32) -> f32 {
    if amplitude <= 1e-5 {
        SILENCE_DBFS
    } else {
        (20.0 * amplitude.log10()).max(SILENCE_DBFS)
    }
}

/// Accumulates RMS and peak over a reporting window.
#[derive(Default)]
pub struct LevelMeter {
    sum_squares: f64,
    count: u64,
    peak: f32,
}

impl LevelMeter {
    pub fn add(&mut self, samples: &[f32]) {
        for &s in samples {
            self.sum_squares += f64::from(s) * f64::from(s);
            self.peak = self.peak.max(s.abs());
        }
        self.count += samples.len() as u64;
    }

    /// Returns `(rms_dbfs, peak_dbfs)` for the window and starts a new one.
    pub fn take(&mut self) -> (f32, f32) {
        let rms = if self.count == 0 {
            0.0
        } else {
            (self.sum_squares / self.count as f64).sqrt() as f32
        };
        let levels = (to_dbfs(rms), to_dbfs(self.peak));
        *self = Self::default();
        levels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(amplitude: f32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|i| amplitude * (i as f32 * 2.0 * std::f32::consts::PI * 1000.0 / 48_000.0).sin())
            .collect()
    }

    #[test]
    fn full_scale_sine_is_about_minus_3_dbfs_rms_and_0_dbfs_peak() {
        let mut meter = LevelMeter::default();
        meter.add(&sine(1.0, 48_000));
        let (rms, peak) = meter.take();
        assert!((rms + 3.01).abs() < 0.05, "rms {rms}");
        assert!(peak.abs() < 0.01, "peak {peak}");
    }

    #[test]
    fn halving_the_amplitude_drops_6_db() {
        let mut meter = LevelMeter::default();
        meter.add(&sine(0.5, 48_000));
        let (rms, peak) = meter.take();
        assert!((rms + 9.03).abs() < 0.05, "rms {rms}");
        assert!((peak + 6.02).abs() < 0.05, "peak {peak}");
    }

    #[test]
    fn silence_and_empty_windows_report_the_floor() {
        let mut meter = LevelMeter::default();
        assert_eq!(meter.take(), (SILENCE_DBFS, SILENCE_DBFS));
        meter.add(&[0.0; 100]);
        assert_eq!(meter.take(), (SILENCE_DBFS, SILENCE_DBFS));
    }

    #[test]
    fn take_resets_the_window() {
        let mut meter = LevelMeter::default();
        meter.add(&sine(1.0, 4_800));
        meter.take();
        assert_eq!(meter.take(), (SILENCE_DBFS, SILENCE_DBFS));
    }
}
