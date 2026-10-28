//! Converts device audio (any rate, any channel count) into the format downstream stages use:
//! 16 kHz mono, grouped into 20 ms PCM16 frames.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

pub const TARGET_RATE: u32 = 16_000;
/// 20 ms at 16 kHz — the usual frame size for VAD and streaming speech-to-text.
pub const FRAME_SAMPLES: usize = 320;
const RESAMPLER_CHUNK: usize = 1024;

/// Streaming downmix + band-limited resampling to 16 kHz mono. Keeps state between calls, so
/// device callbacks of any size can be fed in as they arrive.
pub struct MonoResampler {
    channels: usize,
    resampler: Option<Fft<f32>>,
    pending: Vec<f32>,
    output: Vec<f32>,
}

impl MonoResampler {
    pub fn new(input_rate: u32, channels: u16) -> Result<Self, String> {
        if input_rate == 0 || channels == 0 {
            return Err(format!(
                "invalid input format: {input_rate} Hz, {channels} channels"
            ));
        }
        let resampler = if input_rate == TARGET_RATE {
            None
        } else {
            let fft = Fft::<f32>::new(
                input_rate as usize,
                TARGET_RATE as usize,
                RESAMPLER_CHUNK,
                1,
                FixedSync::Input,
            )
            .map_err(|e| format!("cannot resample {input_rate} Hz: {e}"))?;
            Some(fft)
        };
        let output = vec![0.0; resampler.as_ref().map_or(0, |r| r.output_frames_max())];
        Ok(Self {
            channels: channels as usize,
            resampler,
            pending: Vec::with_capacity(RESAMPLER_CHUNK * 2),
            output,
        })
    }

    /// Appends the 16 kHz mono result for `interleaved` to `out`.
    pub fn process(&mut self, interleaved: &[f32], out: &mut Vec<f32>) {
        downmix_into(interleaved, self.channels, &mut self.pending);
        let Some(resampler) = self.resampler.as_mut() else {
            out.append(&mut self.pending);
            return;
        };

        let mut consumed = 0;
        loop {
            let needed = resampler.input_frames_next();
            if self.pending.len() - consumed < needed {
                break;
            }
            let chunk = &self.pending[consumed..consumed + needed];
            let capacity = self.output.len();
            let result = match (
                InterleavedSlice::new(chunk, 1, needed),
                InterleavedSlice::new_mut(&mut self.output, 1, capacity),
            ) {
                (Ok(input), Ok(mut output)) => resampler
                    .process_into_buffer(&input, &mut output, None)
                    .ok(),
                _ => None,
            };
            // `None` is only possible with mis-sized buffers, which the constructor rules out.
            let Some((used, written)) = result else { break };
            out.extend_from_slice(&self.output[..written]);
            consumed += used;
        }
        self.pending.drain(..consumed);
    }
}

/// Averages interleaved channels into mono samples appended to `out`.
pub fn downmix_into(interleaved: &[f32], channels: usize, out: &mut Vec<f32>) {
    if channels == 1 {
        out.extend_from_slice(interleaved);
        return;
    }
    let scale = 1.0 / channels as f32;
    out.extend(
        interleaved
            .chunks_exact(channels)
            .map(|frame| frame.iter().sum::<f32>() * scale),
    );
}

pub fn to_pcm16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16
}

/// Groups 16 kHz mono samples into fixed 20 ms PCM16 frames.
#[derive(Default)]
pub struct FrameAssembler {
    buffer: Vec<i16>,
    frames_emitted: u64,
}

impl FrameAssembler {
    /// Calls `emit(frame, index)` for each complete frame; the remainder waits for more input.
    pub fn push(&mut self, samples: &[f32], mut emit: impl FnMut(&[i16], u64)) {
        for &s in samples {
            self.buffer.push(to_pcm16(s));
            if self.buffer.len() == FRAME_SAMPLES {
                emit(&self.buffer, self.frames_emitted);
                self.frames_emitted += 1;
                self.buffer.clear();
            }
        }
    }

    #[cfg(test)]
    pub fn frames_emitted(&self) -> u64 {
        self.frames_emitted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freq: f32, rate: u32, channels: u16, seconds: f32, amplitude: f32) -> Vec<f32> {
        let frames = (rate as f32 * seconds) as usize;
        let mut out = Vec::with_capacity(frames * channels as usize);
        for i in 0..frames {
            let v = amplitude * (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin();
            out.extend(std::iter::repeat_n(v, channels as usize));
        }
        out
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    /// Goertzel magnitude of `freq` in `samples` (normalized to amplitude).
    fn magnitude_at(samples: &[f32], freq: f32, rate: u32) -> f32 {
        let k = 2.0 * (2.0 * std::f32::consts::PI * freq / rate as f32).cos();
        let (mut s1, mut s2) = (0.0f32, 0.0f32);
        for &x in samples {
            let s0 = x + k * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        let power = s1 * s1 + s2 * s2 - k * s1 * s2;
        2.0 * power.max(0.0).sqrt() / samples.len() as f32
    }

    fn convert(input: &[f32], rate: u32, channels: u16, chunk: usize) -> Vec<f32> {
        let mut resampler = MonoResampler::new(rate, channels).unwrap();
        let mut out = Vec::new();
        for piece in input.chunks(chunk * channels as usize) {
            resampler.process(piece, &mut out);
        }
        out
    }

    #[test]
    fn resamples_48k_stereo_speech_band_tone_to_16k_mono() {
        let out = convert(&tone(1_000.0, 48_000, 2, 1.0, 0.5), 48_000, 2, 480);
        // Rate: ~1 s of output (minus the resampler's start-up delay and unfilled last chunk).
        assert!(
            (15_000..=16_000).contains(&out.len()),
            "got {} samples",
            out.len()
        );
        let steady = &out[2_000..out.len() - 500];
        assert!(
            (rms(steady) - 0.5 / 2f32.sqrt()).abs() < 0.01,
            "rms {}",
            rms(steady)
        );
        assert!(magnitude_at(steady, 1_000.0, TARGET_RATE) > 0.45);
    }

    #[test]
    fn filters_content_above_the_new_nyquist_instead_of_aliasing_it() {
        // 10 kHz cannot exist at 16 kHz; without filtering it would alias to 6 kHz.
        let out = convert(&tone(10_000.0, 48_000, 1, 1.0, 0.5), 48_000, 1, 512);
        let steady = &out[2_000..out.len() - 500];
        assert!(rms(steady) < 0.005, "residual rms {}", rms(steady));
        assert!(magnitude_at(steady, 6_000.0, TARGET_RATE) < 0.005);
    }

    #[test]
    fn handles_44_1k_input() {
        let out = convert(&tone(440.0, 44_100, 2, 1.0, 0.5), 44_100, 2, 441);
        assert!(
            (15_000..=16_000).contains(&out.len()),
            "got {} samples",
            out.len()
        );
        assert!(magnitude_at(&out[2_000..out.len() - 500], 440.0, TARGET_RATE) > 0.45);
    }

    #[test]
    fn streaming_in_odd_chunk_sizes_matches_one_big_call() {
        let input = tone(700.0, 48_000, 2, 0.5, 0.3);
        let whole = convert(&input, 48_000, 2, input.len());
        let pieces = convert(&input, 48_000, 2, 137);
        assert_eq!(whole.len(), pieces.len());
        for (a, b) in whole.iter().zip(&pieces) {
            assert!((a - b).abs() < 1e-5);
        }
    }

    #[test]
    fn passes_16k_mono_through_unchanged() {
        let input = tone(300.0, 16_000, 1, 0.1, 0.4);
        assert_eq!(convert(&input, 16_000, 1, 100), input);
    }

    #[test]
    fn rejects_an_invalid_format() {
        assert!(MonoResampler::new(0, 2).is_err());
        assert!(MonoResampler::new(48_000, 0).is_err());
    }

    #[test]
    fn downmix_averages_channels() {
        let mut out = Vec::new();
        downmix_into(&[1.0, 0.0, 0.5, 0.5, -1.0, 1.0], 2, &mut out);
        assert_eq!(out, vec![0.5, 0.5, 0.0]);
    }

    #[test]
    fn pcm16_conversion_clamps() {
        assert_eq!(to_pcm16(0.0), 0);
        assert_eq!(to_pcm16(1.0), i16::MAX);
        assert_eq!(to_pcm16(2.0), i16::MAX);
        assert_eq!(to_pcm16(-2.0), -i16::MAX);
    }

    #[test]
    fn assembles_fixed_20ms_frames_and_keeps_the_remainder() {
        let mut assembler = FrameAssembler::default();
        let mut frames = Vec::new();
        assembler.push(&vec![0.25; 700], |frame, index| {
            frames.push((frame.len(), index))
        });
        assert_eq!(frames, vec![(FRAME_SAMPLES, 0), (FRAME_SAMPLES, 1)]);
        assembler.push(&vec![0.25; 260], |frame, index| {
            frames.push((frame.len(), index))
        });
        assert_eq!(frames.last(), Some(&(FRAME_SAMPLES, 2)));
        assert_eq!(assembler.frames_emitted(), 3);
    }
}
