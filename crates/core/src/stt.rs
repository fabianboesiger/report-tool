//! Speech to text: whisper.cpp over 16 kHz mono audio.
//!
//! Runs inside the **STT worker**, never alongside the language model — see
//! [`crate::worker`] for the ggml-duplication reason that separation exists.
//!
//! Synchronous, like [`crate::llm`], and for the same reason: the worker does one
//! thing at a time and has no UI to keep responsive.

use std::path::Path;

use anyhow::{Context, Result};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Whisper's fixed input rate. Audio arrives already resampled — see
/// `app/src/audio.rs`, where the conversion and its anti-aliasing live.
pub use crate::STT_SAMPLE_RATE as SAMPLE_RATE;

pub struct Stt {
    context: WhisperContext,
}

impl Stt {
    pub fn load(model: &Path) -> Result<Self> {
        // Checked first: initialising the ggml backend takes seconds, and a mistyped
        // path should not cost that before saying so.
        anyhow::ensure!(model.exists(), "no transcription model at {}", model.display());

        let device = crate::gpu::select()?;
        let mut params = WhisperContextParameters::default();
        params.use_gpu(device.use_gpu());

        let context = WhisperContext::new_with_params(model, params)
            .with_context(|| format!("loading the transcription model {}", model.display()))?;

        tracing::info!("stt: loaded {} on {}", model.display(), device.as_str());
        Ok(Self { context })
    }

    /// Transcribe mono `f32` samples at [`SAMPLE_RATE`].
    ///
    /// `language` is an ISO code, or `None` to let whisper detect it — worth
    /// defaulting to detection, since these notes are as likely to be German as
    /// English and a wrong forced language produces confident nonsense rather than a
    /// visible error.
    pub fn transcribe(&mut self, pcm: &[f32], language: Option<&str>) -> Result<String> {
        anyhow::ensure!(!pcm.is_empty(), "no audio was recorded");
        // Whisper works on 30-second windows; below about a second it tends to
        // return nothing at all, which reads as a broken button.
        anyhow::ensure!(
            pcm.len() >= SAMPLE_RATE as usize / 4,
            "the recording is too short to transcribe"
        );

        let mut state = self.context.create_state().context("creating a whisper state")?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(language);
        params.set_translate(false);
        // whisper.cpp prints its own progress to stdout, which in a worker process is
        // the protocol channel — anything it wrote there would be parsed as an event.
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_n_threads(threads());

        state.full(params, pcm).context("running transcription")?;

        let mut out = String::new();
        for index in 0..state.full_n_segments() {
            let Some(segment) = state.get_segment(index) else { continue };
            let text = segment.to_str_lossy().context("decoding a transcript segment")?;
            out.push_str(text.trim());
            out.push(' ');
        }
        Ok(out.trim().to_string())
    }
}

fn threads() -> std::ffi::c_int {
    // Leave a core for the UI process; whisper saturates whatever it is given.
    let available = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    available.saturating_sub(1).clamp(1, 8) as std::ffi::c_int
}
