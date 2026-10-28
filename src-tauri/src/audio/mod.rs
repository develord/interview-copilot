//! Native audio capture (PRD §3.2, Phase 2).
//!
//! Each enabled source (system loopback, microphone) runs on its own worker thread that owns
//! the device stream. Device callbacks only copy samples into a lock-free ring buffer; the
//! worker downmixes, resamples to 16 kHz mono, meters levels, hands 20 ms frames to a
//! `FrameSink`, and rebuilds the stream when the device is lost. Raw audio is never stored.

pub mod backend;
pub mod commands;
pub mod convert;
pub mod engine;
pub mod meter;
pub mod types;
pub mod worker;

#[cfg(test)]
mod hardware_tests;
