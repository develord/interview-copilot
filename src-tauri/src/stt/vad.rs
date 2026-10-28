//! Lightweight energy VAD on 20 ms, 16 kHz frames. It does not decide what a question is (the
//! provider's semantic end-of-turn does that); it detects "speech, then N ms of silence" so the
//! app can force a turn to close quickly and measure time-to-final.

use std::collections::VecDeque;

use crate::audio::meter::to_dbfs;

/// Frames of consecutive speech needed to start a segment (60 ms).
const START_FRAMES: u32 = 3;
const FRAME_MS: u32 = 20;
/// Speech must exceed the noise floor by this margin…
const MARGIN_DB: f32 = 10.0;
/// …and this absolute level.
const MIN_SPEECH_DBFS: f32 = -55.0;
/// The noise floor is the quietest frame of the last 3 s: a steady hum becomes the floor, while
/// speech always contains quieter gaps between syllables and words.
const FLOOR_WINDOW_FRAMES: usize = 150;
/// Prior floor until real history fills the window, so speech at stream start is detected.
const INITIAL_FLOOR_DBFS: f32 = -70.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VadEvent {
    SpeechStart,
    /// Emitted once `silence_ms` of silence followed speech; carries that silence duration.
    SpeechEnd {
        silence_ms: u32,
    },
}

pub struct EnergyVad {
    end_silence_ms: u32,
    recent_levels: VecDeque<f32>,
    in_speech: bool,
    speech_run: u32,
    silence_ms: u32,
    last_frame_was_speech: bool,
}

impl EnergyVad {
    pub fn new(end_silence_ms: u32) -> Self {
        Self {
            end_silence_ms: end_silence_ms.max(FRAME_MS),
            recent_levels: std::iter::repeat_n(INITIAL_FLOOR_DBFS, FLOOR_WINDOW_FRAMES).collect(),
            in_speech: false,
            speech_run: 0,
            silence_ms: 0,
            last_frame_was_speech: false,
        }
    }

    /// Whether the most recent frame was above the speech threshold (for latency timing).
    pub fn last_frame_was_speech(&self) -> bool {
        self.last_frame_was_speech
    }

    pub fn push(&mut self, frame: &[i16]) -> Option<VadEvent> {
        let level = frame_dbfs(frame);
        self.recent_levels.pop_front();
        self.recent_levels.push_back(level);
        let floor = self
            .recent_levels
            .iter()
            .copied()
            .fold(f32::INFINITY, f32::min);
        let is_speech = level > (floor + MARGIN_DB).max(MIN_SPEECH_DBFS);
        self.last_frame_was_speech = is_speech;

        if self.in_speech {
            if is_speech {
                self.silence_ms = 0;
            } else {
                self.silence_ms += FRAME_MS;
                if self.silence_ms >= self.end_silence_ms {
                    self.in_speech = false;
                    self.speech_run = 0;
                    let silence_ms = self.silence_ms;
                    self.silence_ms = 0;
                    return Some(VadEvent::SpeechEnd { silence_ms });
                }
            }
            return None;
        }

        self.speech_run = if is_speech { self.speech_run + 1 } else { 0 };
        if self.speech_run >= START_FRAMES {
            self.in_speech = true;
            self.silence_ms = 0;
            return Some(VadEvent::SpeechStart);
        }
        None
    }
}

fn frame_dbfs(frame: &[i16]) -> f32 {
    if frame.is_empty() {
        return to_dbfs(0.0);
    }
    let sum: f64 = frame.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
    let rms = (sum / frame.len() as f64).sqrt() / f64::from(i16::MAX);
    to_dbfs(rms as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(amplitude: f32) -> Vec<i16> {
        (0..320)
            .map(|i| (amplitude * 32767.0 * (i as f32 * 0.3).sin()) as i16)
            .collect()
    }

    fn noise(amplitude: i16) -> Vec<i16> {
        (0..320)
            .map(|i| if i % 2 == 0 { amplitude } else { -amplitude })
            .collect()
    }

    fn run(vad: &mut EnergyVad, frame: &[i16], count: usize) -> Vec<VadEvent> {
        (0..count).filter_map(|_| vad.push(frame)).collect()
    }

    #[test]
    fn detects_speech_then_the_configured_silence() {
        let mut vad = EnergyVad::new(700);
        assert!(run(&mut vad, &[0; 320], 10).is_empty());
        assert_eq!(run(&mut vad, &tone(0.2), 25), vec![VadEvent::SpeechStart]);
        // 680 ms of silence: not yet.
        assert!(run(&mut vad, &[0; 320], 34).is_empty());
        assert_eq!(
            run(&mut vad, &[0; 320], 1),
            vec![VadEvent::SpeechEnd { silence_ms: 700 }]
        );
        assert!(run(&mut vad, &[0; 320], 50).is_empty(), "ends only once");
    }

    #[test]
    fn short_pauses_inside_a_question_do_not_end_it() {
        let mut vad = EnergyVad::new(700);
        run(&mut vad, &tone(0.2), 10);
        let events: Vec<_> = (0..5)
            .flat_map(|_| {
                let mut e = run(&mut vad, &[0; 320], 20); // 400 ms hesitation
                e.extend(run(&mut vad, &tone(0.2), 10));
                e
            })
            .collect();
        assert!(events.is_empty(), "{events:?}");
    }

    #[test]
    fn ignores_clicks_shorter_than_60_ms() {
        let mut vad = EnergyVad::new(700);
        assert!(run(&mut vad, &tone(0.3), 2).is_empty());
        assert!(run(&mut vad, &[0; 320], 5).is_empty());
    }

    #[test]
    fn detects_speech_from_the_very_first_frames() {
        // Regression: with no history, a voice starting at t=0 became its own noise floor.
        let mut vad = EnergyVad::new(700);
        assert_eq!(run(&mut vad, &tone(0.2), 5), vec![VadEvent::SpeechStart]);
    }

    #[test]
    fn adapts_to_a_steady_background_noise_within_the_floor_window() {
        let mut vad = EnergyVad::new(700);
        // A hum present from the start (~-36 dBFS) reads as speech until the 3 s window has
        // learned it, then ends once; after that it is the floor and never speech.
        let first_window = run(&mut vad, &noise(500), 200);
        assert_eq!(first_window.first(), Some(&VadEvent::SpeechStart));
        assert!(matches!(
            first_window.last(),
            Some(VadEvent::SpeechEnd { .. })
        ));
        assert!(run(&mut vad, &noise(500), 300).is_empty());
        // Speech clearly above the hum is still detected.
        assert_eq!(run(&mut vad, &tone(0.3), 5), vec![VadEvent::SpeechStart]);
    }
}
