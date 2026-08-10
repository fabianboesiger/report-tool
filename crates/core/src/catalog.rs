//! The models the app fetches for itself.
//!
//! Two, chosen for this tool's particular constraints rather than for leaderboard
//! position. Both are Apache-2.0, which a commercial deployment needs and which was
//! not true of either family until recently.
//!
//! ## Why not a reasoning model
//!
//! The generated JSON is grammar-constrained from the very first token, so a model
//! that wants to open with `<think>` **cannot**: the grammar admits only `{`. That
//! rules out the otherwise obvious picks. Qwen3.5 reasons by default and needs
//! `enable_thinking=false` passed as a chat-template argument — which
//! `LlamaModel::apply_chat_template` gives no way to send. Gemma 4 inverts it:
//! thinking is opt-*in*, enabled by putting a `<|think|>` token in the system prompt,
//! so simply not adding one leaves the model in the mode we need. Nothing to
//! configure and nothing to get wrong.
//!
//! ## Why the QAT build
//!
//! We ship 4-bit out of necessity — a 16-bit model is beyond a laptop. Google's QAT
//! releases are *trained* quantized, so their 4-bit quality sits far closer to the
//! full-precision model than a quantization applied after the fact. Same download
//! size, better output.

use std::path::PathBuf;

use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    /// Writes the report.
    Report,
    /// Transcribes dictation.
    Dictation,
}

#[derive(Debug, Clone, Copy)]
pub struct ModelSpec {
    /// Directory name under the models folder. Stable — changing it strands an
    /// already-downloaded file.
    pub id: &'static str,
    pub name: &'static str,
    pub kind: ModelKind,
    pub url: &'static str,
    pub file_name: &'static str,
    /// For the progress bar before the server has answered. The real total comes
    /// from `content-length`.
    pub approx_bytes: u64,
    pub license: &'static str,
    /// Shown in Settings, so the choice is explicable to whoever inherits this.
    pub why: &'static str,
}

impl ModelSpec {
    /// Where this model lives once downloaded.
    pub fn path(&self) -> Result<PathBuf> {
        let dir = match self.kind {
            ModelKind::Report => crate::paths::llm_model_dir(self.id)?,
            ModelKind::Dictation => crate::paths::stt_model_dir(self.id)?,
        };
        Ok(dir.join(self.file_name))
    }

    /// Whether the file is fully downloaded.
    ///
    /// A plain existence check, and that is sound because the downloader only ever
    /// renames a file into place once it is whole — a partial download lives beside
    /// it under `.part`. Deliberately not routed through `crate::download`, so the
    /// catalog stays readable in a build with no HTTP client compiled in.
    pub fn is_present(&self) -> bool {
        self.path().map(|p| p.is_file()).unwrap_or(false)
    }
}

/// Gemma 4 E4B, quantization-aware trained to 4-bit.
///
/// Thinking is off unless asked for, German is among the 35+ languages supported out
/// of the box, the context window is far larger than a template plus notes will ever
/// need, and the GGUF is published by Google rather than re-quantized by a third
/// party.
pub const REPORT_MODEL: ModelSpec = ModelSpec {
    id: "gemma-4-e4b-it-qat-q4-0",
    name: "Gemma 4 E4B (QAT, 4-bit)",
    kind: ModelKind::Report,
    url: "https://huggingface.co/google/gemma-4-E4B-it-qat-q4_0-gguf/resolve/main/gemma-4-E4B_q4_0-it.gguf",
    file_name: "gemma-4-E4B_q4_0-it.gguf",
    approx_bytes: 5_150_000_000,
    license: "Apache-2.0",
    why: "Thinking is opt-in, so it works under a grammar that forbids it. \
          Quantization-aware trained, so 4-bit costs little quality. German included.",
};

/// Whisper large-v3-turbo, 5-bit.
///
/// Turbo trades a fraction of a percent of word error rate for roughly six times the
/// speed, and at this quantization it is a third the size of `medium` while being
/// more accurate. Deliberately the multilingual build, not `.en`: these notes are as
/// likely to be German as English.
pub const DICTATION_MODEL: ModelSpec = ModelSpec {
    id: "whisper-large-v3-turbo-q5-0",
    name: "Whisper large-v3-turbo (5-bit)",
    kind: ModelKind::Dictation,
    url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
    file_name: "ggml-large-v3-turbo-q5_0.bin",
    approx_bytes: 574_000_000,
    license: "MIT (whisper.cpp) / Apache-2.0 (OpenAI Whisper weights)",
    why: "Multilingual, so German dictation works. Six times faster than large-v3 for \
          a fraction of a percent of accuracy, and smaller than medium.",
};

/// Everything the app fetches, **smallest first**.
///
/// The order is the download order and it is deliberate: dictation is usable within a
/// minute on a normal connection, so notes can be taken while the report model is
/// still arriving. Fetching the five-gigabyte one first would leave the app doing
/// nothing useful for the whole download.
pub const ALL: [ModelSpec; 2] = [DICTATION_MODEL, REPORT_MODEL];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictation_downloads_before_the_report_model() {
        // The app is useful during the long download only if the short one lands
        // first; reordering this would quietly undo that.
        assert_eq!(ALL[0].kind, ModelKind::Dictation);
        assert!(ALL[0].approx_bytes < ALL[1].approx_bytes);
    }

    #[test]
    fn each_url_ends_in_the_file_it_claims_to_fetch() {
        // A mismatch would download the right bytes to the wrong name, and the model
        // would look absent on the next start — downloading it again, forever.
        for spec in ALL {
            assert!(
                spec.url.ends_with(spec.file_name),
                "{}: url {} does not end with {}",
                spec.id,
                spec.url,
                spec.file_name
            );
        }
    }

    #[test]
    fn model_directories_are_distinct_and_id_shaped() {
        assert_ne!(REPORT_MODEL.id, DICTATION_MODEL.id);
        for spec in ALL {
            assert!(
                spec.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{} is not a safe directory name",
                spec.id
            );
        }
    }

    #[test]
    fn the_multilingual_whisper_build_is_used_not_the_english_only_one() {
        // `.en` models silently transcribe German as garbled English, which reads as
        // a bad microphone rather than as the wrong model.
        assert!(!DICTATION_MODEL.file_name.contains(".en"));
    }

    #[test]
    fn report_and_dictation_models_land_in_separate_directories() {
        // They are loaded by different worker processes; sharing a directory would
        // make it harder to reason about which one owns what.
        let _dir = crate::testenv::data_dir("catalog");
        let report = REPORT_MODEL.path().unwrap();
        let dictation = DICTATION_MODEL.path().unwrap();
        assert_ne!(report.parent(), dictation.parent());
        assert!(report.ends_with(REPORT_MODEL.file_name));
    }
}
