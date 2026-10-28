//! Smoke tests against the real audio devices. Ignored by default (CI has no audio hardware);
//! run them on a workstation with:
//!
//! ```text
//! cargo test --manifest-path src-tauri/Cargo.toml hardware -- --ignored --nocapture --test-threads=1
//! ```
//!
//! `loopback_records_a_tone_played_on_the_default_output` plays a quiet 440 Hz tone for ~1.5 s.

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use super::backend::{CaptureBackend, CpalBackend};
use super::types::{SourceConfig, SourceDiagnostics, SourceKind, SourceState};
use super::worker::{self, FrameSink, RecoveryPolicy, WorkerHandle, WorkerOptions};

struct CollectingSink(Arc<Mutex<Vec<i16>>>);

impl FrameSink for CollectingSink {
    fn push(&mut self, _source: SourceKind, frame: &[i16], _index: u64) {
        self.0.lock().unwrap().extend_from_slice(frame);
    }
}

fn start(source: SourceKind, samples: Arc<Mutex<Vec<i16>>>) -> WorkerHandle {
    worker::spawn(
        Arc::new(CpalBackend),
        WorkerOptions {
            source,
            config: SourceConfig {
                enabled: true,
                device_id: None,
                muted: false,
            },
            policy: RecoveryPolicy::DEFAULT,
            sink: Box::new(CollectingSink(samples)),
        },
    )
}

fn wait_until(
    handle: &WorkerHandle,
    timeout: Duration,
    done: impl Fn(&SourceDiagnostics) -> bool,
) -> SourceDiagnostics {
    let deadline = Instant::now() + timeout;
    loop {
        let snapshot = handle.snapshot();
        if done(&snapshot) || Instant::now() >= deadline {
            return snapshot;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
#[ignore = "needs real audio devices"]
fn lists_real_devices() {
    let devices = CpalBackend
        .list_devices()
        .expect("device enumeration failed");
    println!(
        "outputs: {:#?}\ninputs: {:#?}",
        devices.outputs, devices.inputs
    );
    assert!(!devices.outputs.is_empty(), "no output device found");
    assert!(devices.outputs.iter().filter(|d| d.is_default).count() <= 1);
}

#[test]
#[ignore = "needs a microphone (opens it for ~2 s, records nothing to disk)"]
fn microphone_delivers_frames_or_reports_a_clear_error() {
    let samples = Arc::new(Mutex::new(Vec::new()));
    let handle = start(SourceKind::Microphone, samples);
    let d = wait_until(&handle, Duration::from_secs(3), |d| {
        d.captured_seconds > 1.0 || d.state == SourceState::Error
    });
    let d = handle.stop().max_by_captured(d);
    println!("{d:#?}");
    match d.state {
        SourceState::Error | SourceState::Stopped if d.error.is_some() => {
            println!("microphone unavailable: {:?}", d.error)
        }
        _ => assert!(
            d.captured_seconds > 1.0,
            "microphone produced no audio: {d:?}"
        ),
    }
}

#[test]
#[ignore = "plays a quiet 440 Hz tone on the default output for ~1.5 s"]
fn loopback_records_a_tone_played_on_the_default_output() {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("no default output device");
    let config = device.default_output_config().expect("no output config");
    assert_eq!(
        config.sample_format(),
        cpal::SampleFormat::F32,
        "test tone expects f32 output"
    );
    let rate = config.sample_rate() as f32;
    let channels = config.channels() as usize;

    let samples = Arc::new(Mutex::new(Vec::new()));
    let handle = start(SourceKind::System, samples.clone());
    wait_until(&handle, Duration::from_secs(3), |d| {
        d.state == SourceState::Capturing
    });

    let mut phase = 0.0f32;
    let tone = device
        .build_output_stream::<f32, _, _>(
            config.config(),
            move |data: &mut [f32], _| {
                for frame in data.chunks_mut(channels) {
                    frame.fill(0.05 * (phase * std::f32::consts::TAU).sin());
                    phase = (phase + 440.0 / rate) % 1.0;
                }
            },
            |error| eprintln!("tone stream error: {error}"),
            None,
        )
        .expect("cannot open output stream");
    tone.play().unwrap();
    thread::sleep(Duration::from_millis(1_500));
    drop(tone);
    let d = handle.stop();
    println!("{d:#?}");

    let captured: Vec<f32> = samples
        .lock()
        .unwrap()
        .iter()
        .map(|&s| f32::from(s) / 32767.0)
        .collect();
    assert!(
        captured.len() > 16_000,
        "only {} samples captured",
        captured.len()
    );
    let steady = &captured[4_800..captured.len() - 1_600];
    let rms = (steady.iter().map(|s| s * s).sum::<f32>() / steady.len() as f32).sqrt();
    let tone_amplitude = goertzel(steady, 440.0, 16_000.0);
    println!("rms {rms:.5}, 440 Hz amplitude {tone_amplitude:.5}");
    assert!(
        rms > 1e-4,
        "loopback captured silence (is the output muted?)"
    );
    // The tone must dominate what was captured (other playing audio would lower this ratio).
    assert!(
        tone_amplitude / (rms * 2f32.sqrt()) > 0.5,
        "440 Hz tone not found in the loopback signal"
    );
}

fn goertzel(samples: &[f32], freq: f32, rate: f32) -> f32 {
    let k = 2.0 * (std::f32::consts::TAU * freq / rate).cos();
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for &x in samples {
        let s0 = x + k * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    2.0 * (s1 * s1 + s2 * s2 - k * s1 * s2).max(0.0).sqrt() / samples.len() as f32
}

trait MaxByCaptured {
    fn max_by_captured(self, other: SourceDiagnostics) -> SourceDiagnostics;
}

impl MaxByCaptured for SourceDiagnostics {
    /// `stop()` resets levels; keep whichever snapshot saw more audio for reporting.
    fn max_by_captured(self, other: SourceDiagnostics) -> SourceDiagnostics {
        if other.captured_seconds > self.captured_seconds {
            other
        } else {
            self
        }
    }
}
