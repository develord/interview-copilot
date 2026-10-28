//! Keeps the provider stream at real-time pace and groups frames into ~80 ms messages.
//!
//! Loopback capture delivers *nothing* while the computer is silent. Without padding, a
//! provider would never hear the silence after a question and would wait (or time out) before
//! closing the turn. The pacer inserts digital silence whenever the stream falls behind the
//! wall clock.

use std::time::Duration;

use crate::audio::convert::TARGET_RATE;

/// 80 ms at 16 kHz — inside every provider's recommended 50–1000 ms window.
pub const CHUNK_SAMPLES: usize = 1_280;
/// Padding starts once the stream lags real time by more than this (100 ms).
pub const LAG_TOLERANCE_SAMPLES: u64 = 1_600;

/// Samples of silence to insert so the stream catches up with the wall clock.
pub fn padding_needed(elapsed: Duration, sent_samples: u64) -> u64 {
    let expected = (elapsed.as_secs_f64() * f64::from(TARGET_RATE)) as u64;
    if expected > sent_samples + LAG_TOLERANCE_SAMPLES {
        expected - sent_samples
    } else {
        0
    }
}

/// Accumulates PCM16 samples and yields fixed little-endian chunks.
#[derive(Default)]
pub struct Chunker {
    buffer: Vec<u8>,
}

impl Chunker {
    /// Appends `samples`; returns every completed chunk.
    pub fn push(&mut self, samples: &[i16]) -> Vec<Vec<u8>> {
        let mut chunks = Vec::new();
        for &s in samples {
            self.buffer.extend_from_slice(&s.to_le_bytes());
            if self.buffer.len() == CHUNK_SAMPLES * 2 {
                chunks.push(std::mem::replace(
                    &mut self.buffer,
                    Vec::with_capacity(CHUNK_SAMPLES * 2),
                ));
            }
        }
        chunks
    }

    /// Returns the partial chunk, if any (used before forcing an endpoint or closing).
    pub fn flush(&mut self) -> Option<Vec<u8>> {
        (!self.buffer.is_empty()).then(|| std::mem::take(&mut self.buffer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_padding_while_audio_keeps_up() {
        assert_eq!(padding_needed(Duration::from_secs(1), 16_000), 0);
        assert_eq!(
            padding_needed(Duration::from_secs(1), 14_500),
            0,
            "within tolerance"
        );
    }

    #[test]
    fn pads_up_to_real_time_when_the_device_goes_quiet() {
        assert_eq!(padding_needed(Duration::from_secs(2), 16_000), 16_000);
        assert_eq!(padding_needed(Duration::from_millis(500), 0), 8_000);
    }

    #[test]
    fn chunks_are_80_ms_of_little_endian_pcm16() {
        let mut chunker = Chunker::default();
        let mut samples = vec![0i16; 2_000];
        samples[0] = 0x0102;
        let chunks = chunker.push(&samples);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), CHUNK_SAMPLES * 2);
        assert_eq!(&chunks[0][..2], &[0x02, 0x01]);
        assert_eq!(
            chunker.flush().map(|c| c.len()),
            Some((2_000 - CHUNK_SAMPLES) * 2)
        );
        assert_eq!(chunker.flush(), None);
    }
}
