//! One capture worker per source. The worker thread owns the device stream (created, played,
//! paused and dropped on that thread), drains the ring buffer filled by the device callback,
//! converts to 16 kHz mono, meters levels, forwards 20 ms frames, and rebuilds the stream with
//! bounded exponential backoff when the device is lost.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rtrb::{Consumer, Producer, RingBuffer};

use super::backend::{CaptureBackend, CaptureStream};
use super::convert::{FrameAssembler, MonoResampler, TARGET_RATE};
use super::meter::{LevelMeter, SILENCE_DBFS};
use super::types::{
    AudioError, AudioErrorCode, SourceConfig, SourceDiagnostics, SourceKind, SourceState,
};

/// ~2 s of 48 kHz stereo (more channels or higher rates get proportionally less headroom).
const RING_CAPACITY: usize = 192_000;
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const LEVEL_WINDOW: Duration = Duration::from_millis(100);
/// Loopback delivers no packets while nothing plays; after this long we report "no signal".
const RECEIVING_TIMEOUT: Duration = Duration::from_millis(500);

/// Downstream consumer of 20 ms, 16 kHz mono PCM16 frames (speech-to-text from Phase 3).
pub trait FrameSink: Send {
    fn push(&mut self, source: SourceKind, frame: &[i16], index: u64);
}

/// Phase 2 sink: frames are metered, then dropped. Nothing is retained (PRD §11).
pub struct DiscardSink;

impl FrameSink for DiscardSink {
    fn push(&mut self, _source: SourceKind, _frame: &[i16], _index: u64) {}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecoveryPolicy {
    pub initial_delay: Duration,
    pub max_attempts: u32,
}

impl RecoveryPolicy {
    pub const DEFAULT: Self = Self {
        initial_delay: Duration::from_millis(500),
        max_attempts: 6,
    };

    /// Delay before retry `attempt` (1-based), doubling each time; `None` once exhausted.
    pub fn delay(&self, attempt: u32) -> Option<Duration> {
        (1..=self.max_attempts)
            .contains(&attempt)
            .then(|| self.initial_delay * 2u32.pow(attempt - 1))
    }
}

pub enum WorkerCommand {
    Pause,
    Resume,
    SetMuted(bool),
    Stop,
}

pub struct WorkerOptions {
    pub source: SourceKind,
    pub config: SourceConfig,
    pub policy: RecoveryPolicy,
    pub sink: Box<dyn FrameSink>,
}

pub struct WorkerHandle {
    pub source: SourceKind,
    commands: Sender<WorkerCommand>,
    diagnostics: Arc<Mutex<SourceDiagnostics>>,
    thread: Option<JoinHandle<()>>,
}

impl WorkerHandle {
    pub fn send(&self, command: WorkerCommand) {
        let _ = self.commands.send(command);
    }

    pub fn snapshot(&self) -> SourceDiagnostics {
        match self.diagnostics.lock() {
            Ok(diagnostics) => diagnostics.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// Stops the worker and waits for its thread, which releases the device.
    pub fn stop(mut self) -> SourceDiagnostics {
        self.shutdown();
        self.snapshot()
    }

    fn shutdown(&mut self) {
        let _ = self.commands.send(WorkerCommand::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for WorkerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn spawn(backend: Arc<dyn CaptureBackend>, options: WorkerOptions) -> WorkerHandle {
    let (commands, receiver) = mpsc::channel();
    let source = options.source;
    let diagnostics = Arc::new(Mutex::new(SourceDiagnostics::new(
        source,
        options.config.device_id.is_none(),
    )));
    let shared = diagnostics.clone();
    let thread = thread::Builder::new()
        .name(format!("audio-{source:?}").to_lowercase())
        .spawn(move || Worker::new(backend, options, shared).run(receiver))
        .expect("failed to spawn audio worker thread");
    WorkerHandle {
        source,
        commands,
        diagnostics,
        thread: Some(thread),
    }
}

/// Device-callback side: pushes a whole callback buffer or drops it whole, so the ring never
/// holds a partial frame (which would swap channels for the rest of the stream).
pub fn push_or_drop(producer: &mut Producer<f32>, data: &[f32], dropped_samples: &AtomicU64) {
    if producer.slots() < data.len() || producer.push_entire_slice(data).is_err() {
        dropped_samples.fetch_add(data.len() as u64, Ordering::Relaxed);
    }
}

struct ActiveStream {
    stream: Box<dyn CaptureStream>,
    consumer: Consumer<f32>,
    errors: Receiver<AudioError>,
    converter: MonoResampler,
    channels: usize,
}

struct Worker {
    source: SourceKind,
    device_id: Option<String>,
    backend: Arc<dyn CaptureBackend>,
    sink: Box<dyn FrameSink>,
    policy: RecoveryPolicy,
    shared: Arc<Mutex<SourceDiagnostics>>,
    diagnostics: SourceDiagnostics,
    dropped_samples: Arc<AtomicU64>,
    paused: bool,
    muted: bool,
    active: Option<ActiveStream>,
    /// Pending reconnection: (attempt number, due time).
    retry: Option<(u32, Instant)>,
    failed: bool,
    meter: LevelMeter,
    level_window_start: Instant,
    last_data: Option<Instant>,
    assembler: FrameAssembler,
    output_samples: u64,
    input_buffer: Vec<f32>,
    output_buffer: Vec<f32>,
}

impl Worker {
    fn new(
        backend: Arc<dyn CaptureBackend>,
        options: WorkerOptions,
        shared: Arc<Mutex<SourceDiagnostics>>,
    ) -> Self {
        let diagnostics =
            SourceDiagnostics::new(options.source, options.config.device_id.is_none());
        Self {
            source: options.source,
            device_id: options.config.device_id,
            backend,
            sink: options.sink,
            policy: options.policy,
            shared,
            diagnostics,
            dropped_samples: Arc::new(AtomicU64::new(0)),
            paused: false,
            muted: options.config.muted,
            active: None,
            retry: None,
            failed: false,
            meter: LevelMeter::default(),
            level_window_start: Instant::now(),
            last_data: None,
            assembler: FrameAssembler::default(),
            output_samples: 0,
            input_buffer: vec![0.0; 16_384],
            output_buffer: Vec::with_capacity(8_192),
        }
    }

    fn run(mut self, commands: Receiver<WorkerCommand>) {
        if let Err(error) = self.open() {
            self.on_failure(error, 1);
        }
        loop {
            match commands.recv_timeout(POLL_INTERVAL) {
                Ok(WorkerCommand::Stop) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(command) => self.apply(command),
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.drain();
            self.check_stream_errors();
            self.retry_if_due();
            self.publish();
        }
        self.active = None;
        self.retry = None;
        self.diagnostics.state = SourceState::Stopped;
        self.diagnostics.receiving = false;
        self.diagnostics.level_dbfs = SILENCE_DBFS;
        self.diagnostics.peak_dbfs = SILENCE_DBFS;
        self.write_shared();
    }

    fn open(&mut self) -> Result<(), AudioError> {
        let (mut producer, consumer) = RingBuffer::<f32>::new(RING_CAPACITY);
        let (error_tx, errors) = mpsc::channel();
        let dropped = self.dropped_samples.clone();

        let opened = self.backend.open(
            self.source,
            self.device_id.as_deref(),
            Box::new(move |data| push_or_drop(&mut producer, data, &dropped)),
            Box::new(move |error| {
                let _ = error_tx.send(error);
            }),
        )?;
        let converter = MonoResampler::new(opened.format.sample_rate, opened.format.channels)
            .map_err(|detail| AudioError::new(AudioErrorCode::UnsupportedFormat, detail))?;
        if !self.paused && !self.muted {
            opened.stream.play()?;
        }

        self.diagnostics.device_name = Some(opened.device_name);
        self.diagnostics.sample_rate = Some(opened.format.sample_rate);
        self.diagnostics.channels = Some(opened.format.channels);
        self.diagnostics.error = None;
        self.active = Some(ActiveStream {
            stream: opened.stream,
            consumer,
            errors,
            converter,
            channels: opened.format.channels as usize,
        });
        Ok(())
    }

    fn apply(&mut self, command: WorkerCommand) {
        match command {
            WorkerCommand::Pause => self.paused = true,
            WorkerCommand::Resume => self.paused = false,
            WorkerCommand::SetMuted(muted) => self.muted = muted,
            WorkerCommand::Stop => return,
        }
        let should_play = !self.paused && !self.muted;
        let result = self.active.as_ref().map(|active| {
            if should_play {
                active.stream.play()
            } else {
                active.stream.pause()
            }
        });
        if !should_play {
            self.reset_levels();
        }
        if let Some(Err(error)) = result {
            self.on_failure(error, 1);
        }
    }

    fn drain(&mut self) {
        let Some(active) = self.active.as_mut() else {
            return;
        };
        let forwarding = !self.paused && !self.muted;
        loop {
            let available = active.consumer.slots().min(self.input_buffer.len());
            let count = available - available % active.channels;
            if count == 0 {
                break;
            }
            let (filled, _) = active
                .consumer
                .pop_partial_slice(&mut self.input_buffer[..count]);
            let filled = filled.len();
            // Tail data that arrives while paused/muted is discarded, never forwarded.
            if !forwarding {
                continue;
            }
            active
                .converter
                .process(&self.input_buffer[..filled], &mut self.output_buffer);
            self.meter.add(&self.output_buffer);
            let (sink, source) = (&mut self.sink, self.source);
            self.assembler.push(&self.output_buffer, |frame, index| {
                sink.push(source, frame, index)
            });
            self.output_samples += self.output_buffer.len() as u64;
            self.output_buffer.clear();
            self.last_data = Some(Instant::now());
        }
    }

    fn check_stream_errors(&mut self) {
        let error = self
            .active
            .as_ref()
            .and_then(|active| active.errors.try_recv().ok());
        if let Some(error) = error {
            self.on_failure(error, 1);
        }
    }

    /// Drops the stream, then schedules a retry for recoverable errors or fails permanently.
    fn on_failure(&mut self, error: AudioError, attempt: u32) {
        self.active = None;
        self.reset_levels();
        let delay = if error.code.is_recoverable() {
            self.policy.delay(attempt)
        } else {
            None
        };
        match delay {
            Some(delay) => self.retry = Some((attempt, Instant::now() + delay)),
            None => {
                self.retry = None;
                self.failed = true;
            }
        }
        self.diagnostics.error = Some(error);
    }

    fn retry_if_due(&mut self) {
        let Some((attempt, due)) = self.retry else {
            return;
        };
        if Instant::now() < due {
            return;
        }
        self.retry = None;
        match self.open() {
            Ok(()) => self.diagnostics.recoveries += 1,
            Err(error) => self.on_failure(error, attempt + 1),
        }
    }

    fn reset_levels(&mut self) {
        self.meter = LevelMeter::default();
        self.last_data = None;
    }

    fn state(&self) -> SourceState {
        if self.failed {
            SourceState::Error
        } else if self.retry.is_some() {
            SourceState::Recovering
        } else if self.active.is_none() {
            SourceState::Starting
        } else if self.paused {
            SourceState::Paused
        } else if self.muted {
            SourceState::Muted
        } else {
            SourceState::Capturing
        }
    }

    fn publish(&mut self) {
        let receiving = self
            .last_data
            .is_some_and(|at| at.elapsed() < RECEIVING_TIMEOUT);
        if self.level_window_start.elapsed() >= LEVEL_WINDOW {
            let (rms, peak) = self.meter.take();
            self.diagnostics.level_dbfs = if receiving { rms } else { SILENCE_DBFS };
            self.diagnostics.peak_dbfs = if receiving { peak } else { SILENCE_DBFS };
            self.level_window_start = Instant::now();
        }
        let channels = self.diagnostics.channels.unwrap_or(1).max(1);
        self.diagnostics.state = self.state();
        self.diagnostics.receiving = receiving;
        self.diagnostics.captured_seconds = self.output_samples as f64 / f64::from(TARGET_RATE);
        self.diagnostics.dropped_frames =
            self.dropped_samples.load(Ordering::Relaxed) / u64::from(channels);
        self.write_shared();
    }

    fn write_shared(&self) {
        if let Ok(mut shared) = self.shared.lock() {
            *shared = self.diagnostics.clone();
        }
    }
}

#[cfg(test)]
pub mod fake {
    //! Scriptable backend used by worker and engine tests.

    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use super::super::backend::{
        CaptureBackend, CaptureStream, DataCallback, ErrorCallback, OpenedStream, StreamFormat,
    };
    use super::super::types::{AudioDevices, AudioError, SourceKind};

    pub const STEREO_48K: StreamFormat = StreamFormat {
        sample_rate: 48_000,
        channels: 2,
    };

    #[derive(Default)]
    pub struct FakeState {
        pub script: VecDeque<Result<StreamFormat, AudioError>>,
        pub opens: u32,
        pub playing: bool,
        /// Streams opened and not yet dropped (a dropped stream releases the device).
        pub live_streams: u32,
        callbacks: Option<(DataCallback, ErrorCallback)>,
    }

    #[derive(Clone, Default)]
    pub struct FakeBackend(pub Arc<Mutex<FakeState>>);

    impl FakeBackend {
        pub fn with_script(script: Vec<Result<StreamFormat, AudioError>>) -> Self {
            let backend = Self::default();
            backend.0.lock().unwrap().script = script.into();
            backend
        }

        /// Delivers samples as the device callback would (dropped if no stream is open).
        pub fn feed(&self, samples: &[f32]) {
            if let Some((on_data, _)) = self.0.lock().unwrap().callbacks.as_mut() {
                on_data(samples);
            }
        }

        pub fn fail(&self, error: AudioError) {
            if let Some((_, on_error)) = self.0.lock().unwrap().callbacks.as_mut() {
                on_error(error);
            }
        }

        pub fn opens(&self) -> u32 {
            self.0.lock().unwrap().opens
        }

        pub fn playing(&self) -> bool {
            self.0.lock().unwrap().playing
        }

        pub fn live_streams(&self) -> u32 {
            self.0.lock().unwrap().live_streams
        }
    }

    struct FakeStream(Arc<Mutex<FakeState>>);

    impl Drop for FakeStream {
        fn drop(&mut self) {
            let mut state = self.0.lock().unwrap();
            state.live_streams -= 1;
            state.playing = false;
        }
    }

    impl CaptureStream for FakeStream {
        fn play(&self) -> Result<(), AudioError> {
            self.0.lock().unwrap().playing = true;
            Ok(())
        }
        fn pause(&self) -> Result<(), AudioError> {
            self.0.lock().unwrap().playing = false;
            Ok(())
        }
    }

    impl CaptureBackend for FakeBackend {
        fn list_devices(&self) -> Result<AudioDevices, AudioError> {
            Ok(AudioDevices::default())
        }

        fn open(
            &self,
            _source: SourceKind,
            _device_id: Option<&str>,
            on_data: DataCallback,
            on_error: ErrorCallback,
        ) -> Result<OpenedStream, AudioError> {
            let mut state = self.0.lock().unwrap();
            state.opens += 1;
            let format = state.script.pop_front().unwrap_or(Ok(STEREO_48K))?;
            state.callbacks = Some((on_data, on_error));
            state.playing = false;
            state.live_streams += 1;
            Ok(OpenedStream {
                stream: Box::new(FakeStream(self.0.clone())),
                format,
                device_name: "Fake device".into(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeBackend;
    use super::*;

    const FAST: RecoveryPolicy = RecoveryPolicy {
        initial_delay: Duration::from_millis(5),
        max_attempts: 3,
    };

    struct CountingSink(Arc<AtomicU64>);
    impl FrameSink for CountingSink {
        fn push(&mut self, _source: SourceKind, frame: &[i16], _index: u64) {
            assert_eq!(frame.len(), super::super::convert::FRAME_SAMPLES);
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn start(backend: &FakeBackend, muted: bool) -> (WorkerHandle, Arc<AtomicU64>) {
        let frames = Arc::new(AtomicU64::new(0));
        let handle = spawn(
            Arc::new(backend.clone()),
            WorkerOptions {
                source: SourceKind::System,
                config: SourceConfig {
                    enabled: true,
                    device_id: None,
                    muted,
                },
                policy: FAST,
                sink: Box::new(CountingSink(frames.clone())),
            },
        );
        (handle, frames)
    }

    fn wait_for(
        handle: &WorkerHandle,
        what: &str,
        predicate: impl Fn(&SourceDiagnostics) -> bool,
    ) -> SourceDiagnostics {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let snapshot = handle.snapshot();
            if predicate(&snapshot) {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}: {snapshot:?}"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    /// 10 ms of a 1 kHz stereo tone at 48 kHz, as a device callback would deliver it.
    fn callback_buffer(amplitude: f32, offset: usize) -> Vec<f32> {
        (0..480)
            .flat_map(|i| {
                let t = (offset + i) as f32 / 48_000.0;
                let v = amplitude * (2.0 * std::f32::consts::PI * 1_000.0 * t).sin();
                [v, v]
            })
            .collect()
    }

    fn feed_seconds(backend: &FakeBackend, seconds: f32) {
        let buffers = (seconds * 100.0) as usize;
        for b in 0..buffers {
            backend.feed(&callback_buffer(0.5, b * 480));
        }
    }

    #[test]
    fn captures_converts_and_reports_the_stream() {
        let backend = FakeBackend::default();
        let (handle, frames) = start(&backend, false);
        wait_for(&handle, "capturing", |d| d.state == SourceState::Capturing);
        assert!(backend.playing());

        feed_seconds(&backend, 0.5);
        let d = wait_for(&handle, "0.45 s of output", |d| d.captured_seconds >= 0.45);
        assert_eq!(d.device_name.as_deref(), Some("Fake device"));
        assert_eq!((d.sample_rate, d.channels), (Some(48_000), Some(2)));
        assert!(d.follows_default);
        assert!(d.receiving);
        assert!(frames.load(Ordering::Relaxed) >= 22, "frames: {frames:?}");

        let d = wait_for(&handle, "level", |d| d.level_dbfs > -100.0);
        assert!(
            (d.level_dbfs - (-9.0)).abs() < 1.5,
            "level {}",
            d.level_dbfs
        );
        assert_eq!(d.dropped_frames, 0);
    }

    #[test]
    fn pause_and_mute_stop_the_device_and_forward_nothing() {
        let backend = FakeBackend::default();
        let (handle, _frames) = start(&backend, false);
        wait_for(&handle, "capturing", |d| d.state == SourceState::Capturing);

        handle.send(WorkerCommand::Pause);
        wait_for(&handle, "paused", |d| d.state == SourceState::Paused);
        assert!(!backend.playing());

        handle.send(WorkerCommand::Resume);
        wait_for(&handle, "capturing again", |d| {
            d.state == SourceState::Capturing
        });
        assert!(backend.playing());

        handle.send(WorkerCommand::SetMuted(true));
        let before = wait_for(&handle, "muted", |d| d.state == SourceState::Muted);
        assert!(!backend.playing());
        feed_seconds(&backend, 0.2); // tail data delivered after the mute
        thread::sleep(Duration::from_millis(60));
        assert_eq!(handle.snapshot().captured_seconds, before.captured_seconds);
    }

    #[test]
    fn starting_muted_never_plays_the_device() {
        let backend = FakeBackend::default();
        let (handle, _frames) = start(&backend, true);
        wait_for(&handle, "muted", |d| d.state == SourceState::Muted);
        assert!(!backend.playing());
    }

    #[test]
    fn rebuilds_the_stream_after_device_loss() {
        let backend = FakeBackend::default();
        let (handle, _frames) = start(&backend, false);
        wait_for(&handle, "capturing", |d| d.state == SourceState::Capturing);

        backend.fail(AudioError::new(AudioErrorCode::DeviceLost, "unplugged"));
        let d = wait_for(&handle, "recovered", |d| d.recoveries == 1);
        assert_eq!(d.state, SourceState::Capturing);
        assert_eq!(d.error, None);
        assert_eq!(backend.opens(), 2);
        assert_eq!(backend.live_streams(), 1, "the lost stream must be dropped");

        feed_seconds(&backend, 0.2);
        wait_for(&handle, "audio after recovery", |d| {
            d.captured_seconds > 0.1
        });
    }

    #[test]
    fn permission_denied_is_reported_and_never_retried() {
        let backend = FakeBackend::with_script(vec![Err(AudioError::new(
            AudioErrorCode::PermissionDenied,
            "denied",
        ))]);
        let (handle, _frames) = start(&backend, false);

        let d = wait_for(&handle, "error", |d| d.state == SourceState::Error);
        assert_eq!(
            d.error.map(|e| e.code),
            Some(AudioErrorCode::PermissionDenied)
        );
        thread::sleep(Duration::from_millis(50));
        assert_eq!(backend.opens(), 1);
    }

    #[test]
    fn gives_up_after_the_retry_budget() {
        let missing = || Err(AudioError::new(AudioErrorCode::DeviceNotFound, "no device"));
        let backend = FakeBackend::with_script(vec![missing(), missing(), missing(), missing()]);
        let (handle, _frames) = start(&backend, false);

        wait_for(&handle, "recovering", |d| {
            d.state == SourceState::Recovering
        });
        let d = wait_for(&handle, "error", |d| d.state == SourceState::Error);
        assert_eq!(
            d.error.map(|e| e.code),
            Some(AudioErrorCode::DeviceNotFound)
        );
        assert_eq!(backend.opens(), 1 + FAST.max_attempts);
    }

    #[test]
    fn stop_releases_the_device_and_reports_stopped() {
        let backend = FakeBackend::default();
        let (handle, _frames) = start(&backend, false);
        wait_for(&handle, "capturing", |d| d.state == SourceState::Capturing);
        let last = handle.stop();
        assert_eq!(last.state, SourceState::Stopped);
        assert!(!last.receiving);
        assert_eq!(backend.live_streams(), 0);
    }

    #[test]
    fn ring_buffer_drops_whole_callback_buffers_when_full() {
        let (mut producer, mut consumer) = RingBuffer::<f32>::new(4);
        let dropped = AtomicU64::new(0);
        push_or_drop(&mut producer, &[1.0, 2.0, 3.0], &dropped);
        push_or_drop(&mut producer, &[4.0, 5.0], &dropped);
        assert_eq!(dropped.load(Ordering::Relaxed), 2);
        assert_eq!(consumer.slots(), 3);
        assert_eq!(consumer.pop(), Ok(1.0));
    }

    #[test]
    fn recovery_delays_double_and_are_bounded() {
        let policy = RecoveryPolicy::DEFAULT;
        let delays: Vec<_> = (1..=7).map(|n| policy.delay(n)).collect();
        assert_eq!(
            delays,
            vec![
                Some(Duration::from_millis(500)),
                Some(Duration::from_secs(1)),
                Some(Duration::from_secs(2)),
                Some(Duration::from_secs(4)),
                Some(Duration::from_secs(8)),
                Some(Duration::from_secs(16)),
                None,
            ]
        );
        assert_eq!(policy.delay(0), None);
    }
}
