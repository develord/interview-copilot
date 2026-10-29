//! Tauri commands for audio capture. Commands that touch devices are `async` so they run off the
//! main (UI) thread; results stream back as `PipelineEvent`s, not as return values.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, State};

use super::engine::{AudioEngine, Emit};
use super::types::{AudioConfig, AudioDevices, AudioError, AudioErrorCode, SourceKind};
use super::worker::{DiscardSink, FrameSink, RecoveryPolicy};
use crate::answer::engine::AnswerEngine;
use crate::events::PIPELINE_EVENT;
use crate::intent::{
    classifier::Classifier,
    claude::ClaudeClassifier,
    types::{ClassifyError, ClassifyErrorCode, QuestionDetectionConfig},
};
use crate::pipeline::{self, Detection};
use crate::stt::{self, credentials, session};

#[tauri::command]
pub async fn audio_list_devices(
    engine: State<'_, AudioEngine>,
) -> Result<AudioDevices, AudioError> {
    engine.list_devices()
}

#[tauri::command]
pub async fn audio_start(
    app: AppHandle,
    engine: State<'_, AudioEngine>,
    answers: State<'_, Arc<AnswerEngine>>,
    config: AudioConfig,
) -> Result<(), AudioError> {
    let emit: Emit = Arc::new(move |event| {
        let _ = app.emit(PIPELINE_EVENT, event);
    });

    let Some(transcription) = config.transcription.clone() else {
        return engine.start(config, emit, &|_| {
            Box::new(DiscardSink) as Box<dyn FrameSink>
        });
    };
    // The key is read here, in Rust, and handed to the provider; the webview never sees it.
    let api_key = credentials::get(transcription.provider)
        .map_err(|e| AudioError::new(AudioErrorCode::Backend, e.detail))?
        .ok_or_else(|| {
            AudioError::new(
                AudioErrorCode::TranscriptionKeyMissing,
                format!("no API key stored for {}", transcription.provider.id()),
            )
        })?;
    let provider = stt::build_provider(&transcription, api_key, None);
    let runtime = tokio::runtime::Handle::current();
    let session_id = format!(
        "session-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis())
    );
    let sink_emit = pipeline::session_emit(
        &runtime,
        session_id.clone(),
        config.questions.as_ref().map(detection),
        emit.clone(),
        answers.inner().clone(),
    );
    let sinks = move |source: SourceKind| -> Box<dyn FrameSink> {
        Box::new(session::spawn(
            &runtime,
            provider.clone(),
            session::SessionOptions {
                source,
                session_id: session_id.clone(),
                force_endpoint_ms: transcription.force_endpoint_ms,
                policy: RecoveryPolicy::DEFAULT,
            },
            sink_emit.clone(),
        ))
    };
    engine.start(config, emit, &sinks)
}

/// Question detection for the session. A missing or unreadable Claude key does not block the
/// session: detection runs on the rules alone and `classifier.status` says why.
fn detection(questions: &QuestionDetectionConfig) -> Detection {
    let (classifier, unavailable): (Option<Arc<dyn Classifier>>, _) =
        match crate::llm::credentials::get() {
            Ok(Some(key)) => match ClaudeClassifier::new(key, questions.model, None) {
                Ok(claude) => (Some(Arc::new(claude)), None),
                Err(error) => (None, Some(error)),
            },
            Ok(None) => (
                None,
                Some(ClassifyError::new(
                    ClassifyErrorCode::MissingKey,
                    "no Claude API key stored",
                )),
            ),
            Err(error) => (None, Some(error)),
        };
    Detection {
        include_microphone: questions.include_microphone,
        technical_only: questions.technical_only,
        classifier,
        unavailable,
    }
}

#[tauri::command]
pub async fn audio_pause(engine: State<'_, AudioEngine>) -> Result<(), AudioError> {
    engine.pause()
}

#[tauri::command]
pub async fn audio_resume(engine: State<'_, AudioEngine>) -> Result<(), AudioError> {
    engine.resume()
}

#[tauri::command]
pub async fn audio_set_muted(
    engine: State<'_, AudioEngine>,
    source: SourceKind,
    muted: bool,
) -> Result<(), AudioError> {
    engine.set_muted(source, muted)
}

#[tauri::command]
pub async fn audio_stop(engine: State<'_, AudioEngine>) -> Result<(), AudioError> {
    engine.stop()
}

/// Desktop apps cannot prompt for microphone access on Windows; the user grants it in Settings.
#[tauri::command]
pub fn open_microphone_privacy_settings() -> Result<(), AudioError> {
    std::process::Command::new("explorer.exe")
        .arg("ms-settings:privacy-microphone")
        .spawn()
        .map(|_| ())
        .map_err(|e| AudioError::new(AudioErrorCode::Backend, e.to_string()))
}
