//! The local backend: the same trait, served by the inference worker.
//!
//! Thin on purpose. All it does is translate a [`JsonRequest`] into a worker request
//! and the reply back into JSON — the model, the grammar sampler and the KV cache all
//! live in the child process (see [`crate::worker`] for why).
//!
//! Note what is *absent* compared to [`crate::openai`]: no fallback ladder, no
//! tolerance for a server that ignores the schema. The grammar makes a
//! shape-violating token unsamplable, so there is nothing to degrade to and nothing
//! to recover from.

use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::backend::{extract_json, JsonRequest, LlmBackend};
use crate::worker::{encode_pcm, llm_worker, stt_worker, Event, Request};

pub struct LocalBackend {
    model_path: PathBuf,
    context_tokens: usize,
}

impl LocalBackend {
    pub fn new(model_path: PathBuf, context_tokens: usize) -> Self {
        Self { model_path, context_tokens }
    }
}

#[async_trait]
impl LlmBackend for LocalBackend {
    async fn complete_json(&self, request: JsonRequest) -> Result<Value> {
        let worker_request = Request::GenerateJson {
            model_path: self.model_path.clone(),
            context_tokens: self.context_tokens,
            system: request.system,
            user: request.user,
            grammar: request.grammar,
            temperature: request.temperature,
        };

        let text = llm_worker()
            .request(worker_request, |event| match event {
                // Logged at info: a first-run load reads several gigabytes, and a
                // silent multi-minute pause is indistinguishable from a hang.
                Event::Loading { model_path } => {
                    tracing::info!("local: loading {}", model_path.display());
                }
                Event::Progress { tokens } => tracing::debug!("local: {tokens} tokens"),
                _ => {}
            })
            .await?;

        extract_json(&text)
    }

    fn describe(&self) -> String {
        let name = self
            .model_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.model_path.display().to_string());
        format!("local · {name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_description_names_the_model_file_not_its_whole_path() {
        let backend = LocalBackend::new(
            PathBuf::from("/Users/someone/Library/Application Support/models/llm/qwen.gguf"),
            8192,
        );
        assert_eq!(backend.describe(), "local · qwen.gguf");
    }
}

/// Transcribe 16 kHz mono audio through the STT worker.
///
/// A free function rather than a trait implementation: transcription has one
/// backend, and inventing an abstraction for a single implementation would be
/// scaffolding without a second case to justify it.
pub async fn transcribe(
    model_path: PathBuf,
    pcm_16k_mono: &[f32],
    language: Option<String>,
) -> Result<String> {
    anyhow::ensure!(!pcm_16k_mono.is_empty(), "no audio was recorded");

    let request =
        Request::Transcribe { model_path, pcm_base64: encode_pcm(pcm_16k_mono), language };

    // A different worker from the one that generates reports, so dictating while a
    // report is being written works — and, more importantly, so the two ggml builds
    // never share a process. See `crate::worker`.
    stt_worker()
        .request(request, |event| {
            if let Event::Loading { model_path } = event {
                tracing::info!("stt: loading {}", model_path.display());
            }
        })
        .await
}
