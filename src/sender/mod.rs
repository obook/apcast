//! sender/
//! The receiver protocol, step by step (ROADMAP.md). Done: the receiver's info (info.rs).
//! Until the stream is written, a cast logs that info, runs the capture and drops its frames,
//! so the menu's stats (bitrate, lost frames, CPU) can be checked on the PC alone.
//!
//! Author: O. Booklage
//! Date: October 2026
//! Licence: MIT

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::Context;
use gstreamer as gst;
use gstreamer::prelude::*;

use crate::capture::{Capture, ScreenCast};
use crate::menu::{Settings, Status};
use crate::session::Out;

pub mod info;
pub mod rtsp;

pub const NOT_WRITTEN: &str = "the receiver protocol is not written yet: frames are captured, then dropped";

/// Capture `screen` until `stop`, as a real cast to `receiver` would.
pub fn cast(receiver: SocketAddr, name: &str, s: &Settings, screen: &ScreenCast, stop: &AtomicBool, out: &Out) -> anyhow::Result<()> {
    match rtsp::Rtsp::connect(receiver).and_then(|mut rtsp| info::fetch(&mut rtsp)) {
        Ok((info, _)) => tracing::info!(%receiver, ?info, access = ?info.access(), "receiver info"),
        Err(e) => tracing::warn!(%receiver, "no receiver info: {e:#}"),
    }
    tracing::warn!(%receiver, name, NOT_WRITTEN);
    out.say(&format!("{name}: {NOT_WRITTEN}"));
    let capture = Capture::start(screen, &s.stream, s.audio_delay)?;
    let sink = capture.element("sink").context("no video appsink in the pipeline")?;
    let bytes = Arc::new(AtomicU64::new(0));
    let counted = Arc::clone(&bytes);
    out.send(Status::Served(Box::new(move || counted.load(Ordering::Relaxed))));
    out.send(Status::Dropped(Arc::clone(&capture.dropped)));
    while !stop.load(Ordering::SeqCst) {
        // Timed pull: the stop flag is checked at least every 100 ms.
        if let Some(sample) = sink.emit_by_name::<Option<gst::Sample>>("try-pull-sample", &[&100_000_000u64]) {
            bytes.fetch_add(sample.buffer().map_or(0, |b| b.size() as u64), Ordering::Relaxed);
        }
    }
    Ok(())
}
