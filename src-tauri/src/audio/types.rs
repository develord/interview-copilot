//! Serializable audio contracts shared with the frontend (`src/domain/audio.ts`).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    /// Everything the computer plays (other participants' voices), via WASAPI loopback.
    System,
    Microphone,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceConfig {
    pub enabled: bool,
    /// `None` follows the system default device.
    pub device_id: Option<String>,
    pub muted: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioConfig {
    pub system: SourceConfig,
    pub microphone: SourceConfig,
    /// Streams captured audio to a speech-to-text provider when set (Phase 3).
    #[serde(default)]
    pub transcription: Option<crate::stt::types::TranscriptionConfig>,
    /// Detects questions in the transcripts when set (Phase 4). Needs transcription.
    #[serde(default)]
    pub questions: Option<crate::intent::types::QuestionDetectionConfig>,
}

impl AudioConfig {
    pub fn sources(&self) -> [(SourceKind, &SourceConfig); 2] {
        [
            (SourceKind::System, &self.system),
            (SourceKind::Microphone, &self.microphone),
        ]
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevices {
    /// Render devices; capturing one records what it plays (loopback).
    pub outputs: Vec<DeviceInfo>,
    pub inputs: Vec<DeviceInfo>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceState {
    Starting,
    Capturing,
    Paused,
    Muted,
    Recovering,
    Error,
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AudioErrorCode {
    PermissionDenied,
    DeviceNotFound,
    DeviceLost,
    DeviceBusy,
    UnsupportedFormat,
    /// Start was requested with every source disabled.
    NoSource,
    /// Transcription is enabled but no API key is stored for the selected provider.
    TranscriptionKeyMissing,
    Backend,
}

impl AudioErrorCode {
    /// Transient conditions worth retrying (unplugged/replugged device, device in use).
    pub fn is_recoverable(self) -> bool {
        matches!(
            self,
            Self::DeviceNotFound | Self::DeviceLost | Self::DeviceBusy
        )
    }
}

/// `detail` is technical (English, from the OS/backend); the UI shows a localized label per code.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AudioError {
    pub code: AudioErrorCode,
    pub detail: String,
}

impl AudioError {
    pub fn new(code: AudioErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.detail)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDiagnostics {
    pub source: SourceKind,
    pub state: SourceState,
    pub device_name: Option<String>,
    pub follows_default: bool,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    /// RMS level over the last reporting window, in dBFS (floor: -100).
    pub level_dbfs: f32,
    pub peak_dbfs: f32,
    /// Audio data arrived recently. Loopback delivers nothing while the system is silent.
    pub receiving: bool,
    /// Seconds of 16 kHz mono audio produced for downstream stages.
    pub captured_seconds: f64,
    pub dropped_frames: u64,
    pub recoveries: u32,
    pub error: Option<AudioError>,
}

impl SourceDiagnostics {
    pub fn new(source: SourceKind, follows_default: bool) -> Self {
        Self {
            source,
            state: SourceState::Starting,
            device_name: None,
            follows_default,
            sample_rate: None,
            channels: None,
            level_dbfs: super::meter::SILENCE_DBFS,
            peak_dbfs: super::meter::SILENCE_DBFS,
            receiving: false,
            captured_seconds: 0.0,
            dropped_frames: 0,
            recoveries: 0,
            error: None,
        }
    }
}
