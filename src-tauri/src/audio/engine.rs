//! Capture session: starts one worker per enabled source, relays pause/resume/mute, and reports
//! diagnostics plus the derived session status to the UI every 100 ms.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::backend::CaptureBackend;
use super::types::{
    AudioConfig, AudioDevices, AudioError, AudioErrorCode, SourceKind, SourceState,
};
use super::worker::{self, FrameSink, RecoveryPolicy, WorkerCommand, WorkerHandle, WorkerOptions};
use crate::events::{PipelineEvent, SessionStatus};

const REPORT_INTERVAL: Duration = Duration::from_millis(100);

pub type Emit = Arc<dyn Fn(PipelineEvent) + Send + Sync>;

/// Builds the downstream consumer of each source's 16 kHz frames (discard, or speech-to-text).
pub type SinkFactory<'a> = &'a dyn Fn(SourceKind) -> Box<dyn FrameSink>;

/// Session status as seen by the user, derived from the sources' states.
pub fn derive_status(paused: bool, states: &[SourceState]) -> SessionStatus {
    if states.is_empty() {
        SessionStatus::Idle
    } else if states.iter().all(|s| *s == SourceState::Error) {
        SessionStatus::Error
    } else if paused {
        SessionStatus::Paused
    } else if states.contains(&SourceState::Recovering) {
        SessionStatus::Reconnecting
    } else {
        SessionStatus::Listening
    }
}

struct Session {
    workers: Arc<Vec<WorkerHandle>>,
    paused: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    reporter: Option<JoinHandle<()>>,
    emit: Emit,
}

pub struct AudioEngine {
    backend: Arc<dyn CaptureBackend>,
    policy: RecoveryPolicy,
    session: Mutex<Option<Session>>,
}

impl AudioEngine {
    pub fn new(backend: Arc<dyn CaptureBackend>, policy: RecoveryPolicy) -> Self {
        Self {
            backend,
            policy,
            session: Mutex::new(None),
        }
    }

    pub fn list_devices(&self) -> Result<AudioDevices, AudioError> {
        self.backend.list_devices()
    }

    #[cfg(test)]
    pub fn is_running(&self) -> bool {
        self.session.lock().is_ok_and(|s| s.is_some())
    }

    /// Starts capture for every enabled source. Only ever called from an explicit user action.
    pub fn start(
        &self,
        config: AudioConfig,
        emit: Emit,
        sinks: SinkFactory<'_>,
    ) -> Result<(), AudioError> {
        let mut guard = self.lock()?;
        if guard.is_some() {
            return Err(AudioError::new(
                AudioErrorCode::Backend,
                "capture is already running",
            ));
        }
        let enabled: Vec<_> = config
            .sources()
            .into_iter()
            .filter(|(_, source)| source.enabled)
            .collect();
        if enabled.is_empty() {
            return Err(AudioError::new(
                AudioErrorCode::NoSource,
                "no audio source is enabled",
            ));
        }

        let workers: Vec<WorkerHandle> = enabled
            .into_iter()
            .map(|(source, config)| {
                worker::spawn(
                    self.backend.clone(),
                    WorkerOptions {
                        source,
                        config: config.clone(),
                        policy: self.policy,
                        sink: sinks(source),
                    },
                )
            })
            .collect();

        let workers = Arc::new(workers);
        let paused = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        emit(PipelineEvent::SessionStatus {
            status: SessionStatus::Listening,
            message: None,
        });
        let reporter = spawn_reporter(workers.clone(), paused.clone(), stop.clone(), emit.clone());
        *guard = Some(Session {
            workers,
            paused,
            stop,
            reporter: Some(reporter),
            emit,
        });
        Ok(())
    }

    pub fn pause(&self) -> Result<(), AudioError> {
        self.with_session(|session| {
            session.paused.store(true, Ordering::Relaxed);
            session
                .workers
                .iter()
                .for_each(|w| w.send(WorkerCommand::Pause));
        })
    }

    pub fn resume(&self) -> Result<(), AudioError> {
        self.with_session(|session| {
            session.paused.store(false, Ordering::Relaxed);
            session
                .workers
                .iter()
                .for_each(|w| w.send(WorkerCommand::Resume));
        })
    }

    pub fn set_muted(&self, source: SourceKind, muted: bool) -> Result<(), AudioError> {
        self.with_session(|session| {
            session
                .workers
                .iter()
                .filter(|w| w.source == source)
                .for_each(|w| w.send(WorkerCommand::SetMuted(muted)));
        })
    }

    /// Stops capture, releases every device and reports the final state. Idempotent.
    pub fn stop(&self) -> Result<(), AudioError> {
        let Some(mut session) = self.lock()?.take() else {
            return Ok(());
        };
        session.stop.store(true, Ordering::Relaxed);
        if let Some(reporter) = session.reporter.take() {
            let _ = reporter.join();
        }
        let sources = match Arc::try_unwrap(session.workers) {
            Ok(workers) => workers.into_iter().map(WorkerHandle::stop).collect(),
            Err(shared) => shared.iter().map(WorkerHandle::snapshot).collect(),
        };
        (session.emit)(PipelineEvent::AudioDiagnostics { sources });
        (session.emit)(PipelineEvent::SessionStatus {
            status: SessionStatus::Idle,
            message: None,
        });
        Ok(())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Option<Session>>, AudioError> {
        self.session
            .lock()
            .map_err(|_| AudioError::new(AudioErrorCode::Backend, "audio engine state is poisoned"))
    }

    fn with_session(&self, action: impl FnOnce(&Session)) -> Result<(), AudioError> {
        match self.lock()?.as_ref() {
            Some(session) => {
                action(session);
                Ok(())
            }
            None => Err(AudioError::new(
                AudioErrorCode::Backend,
                "capture is not running",
            )),
        }
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn spawn_reporter(
    workers: Arc<Vec<WorkerHandle>>,
    paused: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    emit: Emit,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("audio-reporter".into())
        .spawn(move || {
            let mut last_status = SessionStatus::Listening;
            while !stop.load(Ordering::Relaxed) {
                let sources: Vec<_> = workers.iter().map(WorkerHandle::snapshot).collect();
                let states: Vec<_> = sources.iter().map(|s| s.state).collect();
                let status = derive_status(paused.load(Ordering::Relaxed), &states);
                emit(PipelineEvent::AudioDiagnostics { sources });
                if status != last_status {
                    emit(PipelineEvent::SessionStatus {
                        status,
                        message: None,
                    });
                    last_status = status;
                }
                thread::sleep(REPORT_INTERVAL);
            }
        })
        .expect("failed to spawn audio reporter thread")
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::super::types::SourceConfig;
    use super::super::worker::DiscardSink;
    use super::super::worker::fake::FakeBackend;
    use super::*;

    fn engine(backend: &FakeBackend) -> AudioEngine {
        AudioEngine::new(
            Arc::new(backend.clone()),
            RecoveryPolicy {
                initial_delay: Duration::from_millis(5),
                max_attempts: 2,
            },
        )
    }

    fn recorder() -> (Emit, Arc<Mutex<Vec<PipelineEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        (Arc::new(move |e| sink.lock().unwrap().push(e)), events)
    }

    fn system_only() -> AudioConfig {
        AudioConfig {
            system: SourceConfig {
                enabled: true,
                device_id: None,
                muted: false,
            },
            microphone: SourceConfig::default(),
            transcription: None,
        }
    }

    fn discard(_source: SourceKind) -> Box<dyn FrameSink> {
        Box::new(DiscardSink)
    }

    fn statuses(events: &Mutex<Vec<PipelineEvent>>) -> Vec<SessionStatus> {
        events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                PipelineEvent::SessionStatus { status, .. } => Some(*status),
                _ => None,
            })
            .collect()
    }

    fn wait_for_status(events: &Mutex<Vec<PipelineEvent>>, wanted: SessionStatus) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while statuses(events).last() != Some(&wanted) {
            assert!(
                Instant::now() < deadline,
                "never reached {wanted:?}: {:?}",
                statuses(events)
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn derives_the_session_status_from_sources() {
        use SourceState::*;
        assert_eq!(derive_status(false, &[]), SessionStatus::Idle);
        assert_eq!(
            derive_status(false, &[Capturing, Muted]),
            SessionStatus::Listening
        );
        assert_eq!(
            derive_status(true, &[Paused, Paused]),
            SessionStatus::Paused
        );
        assert_eq!(
            derive_status(false, &[Capturing, Recovering]),
            SessionStatus::Reconnecting
        );
        // One working source keeps the session alive; its sibling's error is shown per source.
        assert_eq!(
            derive_status(false, &[Capturing, Error]),
            SessionStatus::Listening
        );
        assert_eq!(derive_status(true, &[Error, Error]), SessionStatus::Error);
    }

    #[test]
    fn refuses_to_start_without_an_enabled_source() {
        let backend = FakeBackend::default();
        let (emit, events) = recorder();
        let error = engine(&backend)
            .start(AudioConfig::default(), emit, &discard)
            .unwrap_err();
        assert_eq!(error.code, AudioErrorCode::NoSource);
        assert!(events.lock().unwrap().is_empty());
        assert_eq!(backend.opens(), 0);
    }

    #[test]
    fn runs_a_full_session_lifecycle_and_reports_each_status() {
        let backend = FakeBackend::default();
        let engine = engine(&backend);
        let (emit, events) = recorder();

        engine.start(system_only(), emit.clone(), &discard).unwrap();
        assert!(
            engine.start(system_only(), emit, &discard).is_err(),
            "double start must be refused"
        );
        wait_for_status(&events, SessionStatus::Listening);

        engine.pause().unwrap();
        wait_for_status(&events, SessionStatus::Paused);
        engine.resume().unwrap();
        wait_for_status(&events, SessionStatus::Listening);

        engine.stop().unwrap();
        assert_eq!(statuses(&events).last(), Some(&SessionStatus::Idle));
        assert!(!engine.is_running());
        assert_eq!(backend.live_streams(), 0, "device must be released");
        engine.stop().unwrap(); // idempotent

        let last_diagnostics = events.lock().unwrap().iter().rev().find_map(|e| match e {
            PipelineEvent::AudioDiagnostics { sources } => Some(sources.clone()),
            _ => None,
        });
        assert_eq!(last_diagnostics.unwrap()[0].state, SourceState::Stopped);
    }

    #[test]
    fn reports_reconnecting_while_a_lost_device_is_retried() {
        let missing = || Err(AudioError::new(AudioErrorCode::DeviceLost, "gone"));
        let backend = FakeBackend::with_script(vec![missing(), missing(), missing()]);
        let engine = AudioEngine::new(
            Arc::new(backend.clone()),
            RecoveryPolicy {
                initial_delay: Duration::from_millis(150),
                max_attempts: 2,
            },
        );
        let (emit, events) = recorder();
        engine.start(system_only(), emit, &discard).unwrap();

        wait_for_status(&events, SessionStatus::Reconnecting);
        wait_for_status(&events, SessionStatus::Error);
        engine.stop().unwrap();
    }

    #[test]
    fn commands_without_a_session_are_rejected() {
        let backend = FakeBackend::default();
        let engine = engine(&backend);
        assert!(engine.pause().is_err());
        assert!(engine.set_muted(SourceKind::Microphone, true).is_err());
        assert!(engine.stop().is_ok());
    }
}
