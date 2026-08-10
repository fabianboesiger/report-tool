//! User settings, and the choice of generation backend.
//!
//! ## The API key is stored in plain text
//!
//! `settings.json` sits in the app's data directory with the user's key in it,
//! readable by anything running as that user. A platform keychain would be better and
//! is a larger piece of work — the honest position for now is that this is stated
//! rather than hidden, and that the local backend needs no key at all.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::backend::{LlmBackend, StubBackend};

/// How to reach an OpenAI-compatible server.
///
/// Defined here rather than beside the connector because it is plain serde data with
/// no HTTP in it, and a build without the `remote` feature still has to read and
/// write a settings file that contains it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenAiConfig {
    /// The API root, with or without a trailing slash — e.g.
    /// `https://api.openai.com/v1`, or `http://localhost:11434/v1` for Ollama.
    pub base_url: String,
    /// Sent as `Authorization: Bearer`. Left empty for local servers that want none.
    #[serde(default)]
    pub api_key: String,
    pub model: String,
    /// Seconds. Generous by default: a long report on a small self-hosted model can
    /// take minutes, and a timeout that fires mid-generation looks like a crash.
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_timeout() -> u64 {
    300
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: "gpt-4o-mini".to_string(),
            timeout_secs: default_timeout(),
        }
    }
}

impl OpenAiConfig {
    /// The chat completions endpoint for this base URL.
    pub fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }
}

/// Where the local model lives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalConfig {
    /// Path to a GGUF file. There is no downloader yet, so this is a path the user
    /// supplies.
    #[serde(default)]
    pub model_path: String,
    /// Context window in tokens. Larger costs memory and prefill time; too small and
    /// a long template plus long notes will not fit.
    #[serde(default = "default_context_tokens")]
    pub context_tokens: usize,
}

fn default_context_tokens() -> usize {
    8192
}

impl Default for LocalConfig {
    fn default() -> Self {
        Self { model_path: String::new(), context_tokens: default_context_tokens() }
    }
}

/// Where the transcription model lives.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SttConfig {
    /// Path to a whisper.cpp `ggml-*.bin` model.
    #[serde(default)]
    pub model_path: String,
    /// ISO language code, or empty to let whisper detect it.
    ///
    /// Detection is the better default here: these notes are as likely to be German
    /// as English, and a wrongly forced language produces confident nonsense rather
    /// than a visible error.
    #[serde(default)]
    pub language: String,
}

impl SttConfig {
    pub fn language(&self) -> Option<String> {
        let language = self.language.trim();
        (!language.is_empty()).then(|| language.to_string())
    }
}

/// Where reports are generated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    /// An OpenAI-compatible server.
    Remote,
    /// The in-process model.
    Local,
    /// Canned output. Useful without a model, and the only thing a
    /// `--no-default-features` build can offer.
    Stub,
}

impl Default for Provider {
    /// Remote where there is a connector to use, otherwise the stub.
    ///
    /// A `--no-default-features` build has no way to reach a server, and defaulting
    /// to something it cannot do would greet the user with an error on the first
    /// click rather than with a working app.
    fn default() -> Self {
        if cfg!(feature = "remote") {
            Provider::Remote
        } else {
            Provider::Stub
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub provider: Provider,
    #[serde(default)]
    pub openai: OpenAiConfig,
    #[serde(default)]
    pub local: LocalConfig,
    #[serde(default)]
    pub stt: SttConfig,
}

impl Settings {
    /// Load from disk, falling back to defaults.
    ///
    /// Never fails. A settings file written by a future version, or corrupted by a
    /// half-finished write, must not stop the app from starting — the user can always
    /// re-enter a base URL, but cannot fix a program that refuses to open.
    pub fn load() -> Settings {
        let Ok(path) = crate::paths::settings_path() else {
            return Settings::default();
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Settings::default();
        };
        match serde_json::from_str(&text) {
            Ok(settings) => settings,
            Err(error) => {
                tracing::warn!(
                    "settings: {} is unreadable ({error}), using defaults",
                    path.display()
                );
                Settings::default()
            }
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = crate::paths::settings_path()?;
        let text = serde_json::to_string_pretty(self).context("serialising settings")?;
        // Written through a temporary file: a crash midway through a direct write
        // would leave a truncated file, and the next start would silently discard
        // every setting including the API key.
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, text)
            .with_context(|| format!("writing {}", temporary.display()))?;
        std::fs::rename(&temporary, &path)
            .with_context(|| format!("replacing {}", path.display()))?;
        Ok(())
    }

    /// Build the backend this configuration asks for.
    pub fn backend(&self) -> Result<Box<dyn LlmBackend>> {
        match self.provider {
            Provider::Stub => Ok(Box::new(StubBackend)),

            #[cfg(feature = "remote")]
            Provider::Remote => {
                anyhow::ensure!(
                    !self.openai.base_url.trim().is_empty(),
                    "no server URL configured — set one in Settings"
                );
                anyhow::ensure!(
                    !self.openai.model.trim().is_empty(),
                    "no model configured — set one in Settings"
                );
                Ok(Box::new(crate::openai::OpenAiBackend::new(self.openai.clone())?))
            }
            #[cfg(not(feature = "remote"))]
            Provider::Remote => {
                anyhow::bail!("this build has no remote connector (built without `remote`)")
            }

            #[cfg(all(feature = "inference", feature = "worker"))]
            Provider::Local => {
                let path = self.report_model_path().ok_or_else(|| {
                    anyhow::anyhow!(
                        "the report model is not ready yet — it is still downloading, or set \
                         the path to a GGUF file in Settings"
                    )
                })?;
                Ok(Box::new(crate::local::LocalBackend::new(
                    path,
                    self.local.context_tokens.max(512),
                )))
            }
            // Named explicitly rather than silently falling back to the stub, which
            // would look like a real model producing placeholder text.
            #[cfg(not(all(feature = "inference", feature = "worker")))]
            Provider::Local => {
                anyhow::bail!("this build has no local engine (built without `inference`)")
            }
        }
    }

    /// The GGUF to generate with: the configured path, or the managed download.
    ///
    /// Falling back rather than pre-filling the setting keeps the two meanings apart —
    /// an empty field means "use whatever the app manages", so a user who never opens
    /// Settings gets a working app, and one who sets a path is never overridden by a
    /// later download.
    pub fn report_model_path(&self) -> Option<std::path::PathBuf> {
        let configured = self.local.model_path.trim();
        if !configured.is_empty() {
            return Some(std::path::PathBuf::from(configured));
        }
        crate::catalog::REPORT_MODEL.path().ok().filter(|p| p.is_file())
    }

    /// The whisper model to dictate with, on the same terms.
    pub fn dictation_model_path(&self) -> Option<std::path::PathBuf> {
        let configured = self.stt.model_path.trim();
        if !configured.is_empty() {
            return Some(std::path::PathBuf::from(configured));
        }
        crate::catalog::DICTATION_MODEL.path().ok().filter(|p| p.is_file())
    }

    /// A short description of the configured backend, for the UI.
    pub fn describe(&self) -> String {
        match self.provider {
            Provider::Stub => "stub".to_string(),
            Provider::Local => {
                let path = self
                    .report_model_path()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                let path = path.trim();
                if path.is_empty() {
                    "local (model not ready)".to_string()
                } else {
                    let name = std::path::Path::new(path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.to_string());
                    format!("local · {name}")
                }
            }
            Provider::Remote => {
                if self.openai.model.trim().is_empty() {
                    "remote (not configured)".to_string()
                } else {
                    format!("{} · {}", self.openai.model, host_of(&self.openai.base_url))
                }
            }
        }
    }
}

/// The host part of a URL, for a compact status line.
fn host_of(url: &str) -> String {
    url.trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(url)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_endpoint_tolerates_a_trailing_slash() {
        let config =
            OpenAiConfig { base_url: "http://localhost:11434/v1/".into(), ..Default::default() };
        assert_eq!(config.endpoint(), "http://localhost:11434/v1/chat/completions");
        let config =
            OpenAiConfig { base_url: "https://api.openai.com/v1".into(), ..Default::default() };
        assert_eq!(config.endpoint(), "https://api.openai.com/v1/chat/completions");
    }

    #[test]
    fn settings_round_trip_through_json() {
        let settings = Settings {
            provider: Provider::Local,
            openai: OpenAiConfig {
                base_url: "http://localhost:11434/v1".into(),
                api_key: "secret".into(),
                model: "qwen3:8b".into(),
                timeout_secs: 120,
            },
            local: LocalConfig { model_path: "/models/x.gguf".into(), context_tokens: 4096 },
            stt: SttConfig { model_path: "/models/ggml-base.bin".into(), language: "de".into() },
        };
        let text = serde_json::to_string(&settings).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&text).unwrap(), settings);
    }

    #[test]
    fn a_settings_file_from_another_version_still_loads() {
        // Every field defaulted, so a file written before a field existed — or after
        // one was removed — opens rather than resetting the user's configuration.
        let settings: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings, Settings::default());

        let settings: Settings =
            serde_json::from_str(r#"{"provider":"local","openai":{"base_url":"x","model":"y"}}"#)
                .unwrap();
        assert_eq!(settings.provider, Provider::Local);
        assert_eq!(settings.openai.api_key, "");
        assert!(settings.openai.timeout_secs > 0, "a missing timeout must not become zero");
    }

    // Only meaningful where the connector exists; without it the backend refuses
    // for a different and equally clear reason, covered below.
    #[cfg(feature = "remote")]
    #[test]
    fn an_unconfigured_remote_backend_says_what_is_missing() {
        let mut settings = Settings::default();
        settings.openai.model.clear();
        // Mapped to a string so the Ok side is Debug; a trait object is not.
        let error = settings.backend().map(|b| b.describe()).unwrap_err().to_string();
        assert!(error.contains("model"), "{error}");
        assert!(error.contains("Settings"), "the message must say where to fix it: {error}");
    }

    #[test]
    fn a_configured_path_wins_over_the_managed_download() {
        // A user who set a path must never have it silently replaced by whatever the
        // app downloaded.
        let settings = Settings {
            local: LocalConfig { model_path: "/my/own.gguf".into(), context_tokens: 8192 },
            ..Default::default()
        };
        assert_eq!(settings.report_model_path(), Some(std::path::PathBuf::from("/my/own.gguf")));

        let settings = Settings {
            stt: SttConfig { model_path: " /my/whisper.bin ".into(), language: String::new() },
            ..Default::default()
        };
        assert_eq!(
            settings.dictation_model_path(),
            Some(std::path::PathBuf::from("/my/whisper.bin")),
            "the path must be trimmed"
        );
    }

    #[test]
    fn an_absent_managed_model_reads_as_not_ready_rather_than_as_a_path() {
        let _dir = crate::testenv::data_dir("settings");
        let settings = Settings { provider: Provider::Local, ..Default::default() };
        assert_eq!(settings.report_model_path(), None);
        assert_eq!(settings.describe(), "local (model not ready)");
    }

    #[test]
    fn an_unconfigured_local_backend_says_what_is_missing() {
        // Isolated, or the result depends on whether a model happens to be
        // downloaded on the machine running the tests.
        let _dir = crate::testenv::data_dir("settings-unconfigured");

        // Not a silent fall back to the stub, which would look like a real model
        // producing placeholder text.
        let settings = Settings { provider: Provider::Local, ..Default::default() };
        // Mapped to a string so the Ok side is Debug; a trait object is not.
        let error = settings.backend().map(|b| b.describe()).unwrap_err().to_string();
        if cfg!(all(feature = "inference", feature = "worker")) {
            assert!(error.contains("downloading") || error.contains("GGUF"), "{error}");
        } else {
            assert!(error.contains("no local engine"), "{error}");
        }
    }

    #[test]
    fn the_local_description_names_the_model_file() {
        let settings = Settings {
            provider: Provider::Local,
            local: LocalConfig {
                model_path: "/models/qwen3-8b-q4.gguf".into(),
                context_tokens: 8192,
            },
            ..Default::default()
        };
        assert_eq!(settings.describe(), "local · qwen3-8b-q4.gguf");
    }

    #[cfg(not(feature = "remote"))]
    #[test]
    fn a_build_without_the_connector_says_so_rather_than_failing_obscurely() {
        let settings = Settings { provider: Provider::Remote, ..Default::default() };
        let error = settings.backend().map(|b| b.describe()).unwrap_err().to_string();
        assert!(error.contains("no remote connector"), "{error}");
    }

    #[test]
    fn the_stub_backend_is_always_available() {
        let settings = Settings { provider: Provider::Stub, ..Default::default() };
        assert!(settings.backend().is_ok());
    }

    #[test]
    fn the_description_is_compact_and_names_the_host() {
        let settings = Settings { provider: Provider::Remote, ..Default::default() };
        assert_eq!(settings.describe(), "gpt-4o-mini · api.openai.com");
        assert_eq!(host_of("http://localhost:11434/v1"), "localhost:11434");
    }
}

#[cfg(test)]
mod stt_tests {
    use super::*;

    #[test]
    fn an_empty_language_means_detect_rather_than_an_empty_code() {
        // Passing "" through to whisper would force an unnamed language rather than
        // letting it detect one.
        let config = SttConfig { model_path: "x".into(), language: "  ".into() };
        assert_eq!(config.language(), None);
        let config = SttConfig { model_path: "x".into(), language: " de ".into() };
        assert_eq!(config.language(), Some("de".to_string()));
    }
}
