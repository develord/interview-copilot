//! Capture backend abstraction (PRD §7.3 "audio capture provider") and its cpal/WASAPI
//! implementation. Workers only talk to `CaptureBackend`, so device loss, permission errors and
//! odd formats can be simulated in tests without hardware.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ErrorKind, FromSample, SampleFormat, SizedSample};

use super::types::{AudioDevices, AudioError, AudioErrorCode, DeviceInfo, SourceKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

/// Device stream handle. Created and dropped on the worker thread that owns it.
pub trait CaptureStream {
    fn play(&self) -> Result<(), AudioError>;
    fn pause(&self) -> Result<(), AudioError>;
}

pub struct OpenedStream {
    pub stream: Box<dyn CaptureStream>,
    pub format: StreamFormat,
    pub device_name: String,
}

/// Receives interleaved f32 samples on the device's real-time thread. Must not block.
pub type DataCallback = Box<dyn FnMut(&[f32]) + Send + 'static>;
pub type ErrorCallback = Box<dyn FnMut(AudioError) + Send + 'static>;

pub trait CaptureBackend: Send + Sync + 'static {
    fn list_devices(&self) -> Result<AudioDevices, AudioError>;

    /// Opens a paused stream. `device_id: None` uses the current default device.
    fn open(
        &self,
        source: SourceKind,
        device_id: Option<&str>,
        on_data: DataCallback,
        on_error: ErrorCallback,
    ) -> Result<OpenedStream, AudioError>;
}

/// WASAPI through cpal. Opening an *output* device as an input enables loopback capture.
pub struct CpalBackend;

/// `E_ACCESSDENIED` as reported in cpal's error text (`os error -2147024891`). cpal maps it to a
/// generic backend error with a localized message, so the numeric code is the stable signal.
const E_ACCESSDENIED_OS_ERROR: &str = "-2147024891";

/// Maps cpal errors to stable codes. Returns `None` for informational stream events.
pub fn map_cpal_error(error: &cpal::Error) -> Option<AudioError> {
    let detail = error.to_string();
    let code = match error.kind() {
        // Notices, not failures: the stream keeps delivering data. `Xrun` is WASAPI's
        // DATA_DISCONTINUITY flag, common on loopback when playback starts after silence.
        ErrorKind::DeviceChanged | ErrorKind::RealtimeDenied | ErrorKind::Xrun => return None,
        ErrorKind::PermissionDenied => AudioErrorCode::PermissionDenied,
        ErrorKind::DeviceNotAvailable | ErrorKind::StreamInvalidated => AudioErrorCode::DeviceLost,
        ErrorKind::DeviceBusy => AudioErrorCode::DeviceBusy,
        ErrorKind::UnsupportedConfig => AudioErrorCode::UnsupportedFormat,
        _ if detail.contains(E_ACCESSDENIED_OS_ERROR) => AudioErrorCode::PermissionDenied,
        _ => AudioErrorCode::Backend,
    };
    Some(AudioError::new(code, detail))
}

fn backend_error(error: cpal::Error) -> AudioError {
    map_cpal_error(&error)
        .unwrap_or_else(|| AudioError::new(AudioErrorCode::Backend, error.to_string()))
}

fn device_name(device: &cpal::Device) -> String {
    device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| device.to_string())
}

fn device_infos(
    devices: impl Iterator<Item = cpal::Device>,
    default_id: Option<String>,
) -> Vec<DeviceInfo> {
    devices
        .filter_map(|device| {
            let id = device.id().ok()?.to_string();
            Some(DeviceInfo {
                is_default: default_id.as_deref() == Some(id.as_str()),
                name: device_name(&device),
                id,
            })
        })
        .collect()
}

struct CpalStream(cpal::Stream);

impl CaptureStream for CpalStream {
    fn play(&self) -> Result<(), AudioError> {
        self.0.play().map_err(backend_error)
    }
    fn pause(&self) -> Result<(), AudioError> {
        self.0.pause().map_err(backend_error)
    }
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut on_data: DataCallback,
    mut on_error: ErrorCallback,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let mut scratch: Vec<f32> = Vec::with_capacity(16_384);
    device
        .build_input_stream::<T, _, _>(
            *config,
            move |data: &[T], _info| {
                scratch.clear();
                scratch.extend(data.iter().map(|&s| f32::from_sample_(s)));
                on_data(&scratch);
            },
            move |error| {
                if let Some(error) = map_cpal_error(&error) {
                    on_error(error);
                }
            },
            None,
        )
        .map_err(backend_error)
}

impl CaptureBackend for CpalBackend {
    fn list_devices(&self) -> Result<AudioDevices, AudioError> {
        let host = cpal::default_host();
        let default_output = host
            .default_output_device()
            .and_then(|d| d.id().ok())
            .map(|id| id.to_string());
        let default_input = host
            .default_input_device()
            .and_then(|d| d.id().ok())
            .map(|id| id.to_string());
        Ok(AudioDevices {
            outputs: device_infos(
                host.output_devices().map_err(backend_error)?,
                default_output,
            ),
            inputs: device_infos(host.input_devices().map_err(backend_error)?, default_input),
        })
    }

    fn open(
        &self,
        source: SourceKind,
        device_id: Option<&str>,
        on_data: DataCallback,
        on_error: ErrorCallback,
    ) -> Result<OpenedStream, AudioError> {
        let host = cpal::default_host();
        let not_found =
            |what: &str| AudioError::new(AudioErrorCode::DeviceNotFound, what.to_string());

        let device = match (source, device_id) {
            (_, Some(id)) => {
                let parsed = id
                    .parse::<cpal::DeviceId>()
                    .map_err(|_| not_found("invalid device id"))?;
                host.device_by_id(&parsed)
                    .ok_or_else(|| not_found("selected device is not connected"))?
            }
            // A default-device stream keeps following the system default where WASAPI allows it.
            (SourceKind::System, None) => host
                .default_output_device()
                .ok_or_else(|| not_found("no output device"))?,
            (SourceKind::Microphone, None) => host
                .default_input_device()
                .ok_or_else(|| not_found("no input device"))?,
        };

        let supported = if device.supports_output() && !device.supports_input() {
            device.default_output_config()
        } else {
            device.default_input_config()
        }
        .map_err(backend_error)?;
        let config = supported.config();

        let stream = match supported.sample_format() {
            SampleFormat::F32 => build_stream::<f32>(&device, &config, on_data, on_error),
            SampleFormat::I16 => build_stream::<i16>(&device, &config, on_data, on_error),
            SampleFormat::I32 => build_stream::<i32>(&device, &config, on_data, on_error),
            SampleFormat::U16 => build_stream::<u16>(&device, &config, on_data, on_error),
            SampleFormat::U8 => build_stream::<u8>(&device, &config, on_data, on_error),
            SampleFormat::F64 => build_stream::<f64>(&device, &config, on_data, on_error),
            other => Err(AudioError::new(
                AudioErrorCode::UnsupportedFormat,
                format!("sample format {other:?}"),
            )),
        }?;
        stream.pause().map_err(backend_error)?;

        Ok(OpenedStream {
            stream: Box::new(CpalStream(stream)),
            format: StreamFormat {
                sample_rate: config.sample_rate,
                channels: config.channels,
            },
            device_name: device_name(&device),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_access_denied_to_permission_denied_on_any_windows_language() {
        let french = cpal::Error::with_message(
            ErrorKind::BackendError,
            "Accès refusé. (os error -2147024891)",
        );
        assert_eq!(
            map_cpal_error(&french).map(|e| e.code),
            Some(AudioErrorCode::PermissionDenied)
        );
    }

    #[test]
    fn maps_device_loss_and_contention_to_recoverable_codes() {
        let lost = map_cpal_error(&ErrorKind::DeviceNotAvailable.into()).unwrap();
        let invalidated = map_cpal_error(&ErrorKind::StreamInvalidated.into()).unwrap();
        let busy = map_cpal_error(&ErrorKind::DeviceBusy.into()).unwrap();
        assert_eq!(lost.code, AudioErrorCode::DeviceLost);
        assert_eq!(invalidated.code, AudioErrorCode::DeviceLost);
        assert_eq!(busy.code, AudioErrorCode::DeviceBusy);
        assert!(lost.code.is_recoverable() && busy.code.is_recoverable());
    }

    #[test]
    fn ignores_informational_stream_events() {
        assert_eq!(map_cpal_error(&ErrorKind::DeviceChanged.into()), None);
        assert_eq!(map_cpal_error(&ErrorKind::RealtimeDenied.into()), None);
    }

    // Regression: a loopback discontinuity notice was treated as fatal and killed the stream.
    #[test]
    fn a_buffer_discontinuity_does_not_stop_capture() {
        assert_eq!(map_cpal_error(&ErrorKind::Xrun.into()), None);
    }

    #[test]
    fn unknown_backend_errors_are_not_retried() {
        let other =
            map_cpal_error(&cpal::Error::with_message(ErrorKind::BackendError, "boom")).unwrap();
        assert_eq!(other.code, AudioErrorCode::Backend);
        assert!(!other.code.is_recoverable());
        assert!(!AudioErrorCode::PermissionDenied.is_recoverable());
    }
}
