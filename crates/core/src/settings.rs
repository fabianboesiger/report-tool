//! User settings, and the choice of generation backend.
//!
//! ## The API key is stored in plain text
//!
//! It is a column in `report-tool.db` in the app's data directory, readable by anything
//! running as that user. Moving to `sqlite` changed *where* it lives and nothing about
//! how exposed it is — a platform keychain would be better and is a larger piece of
//! work. The honest position is that this is stated rather than hidden, and that the
//! local backend needs no key at all.

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
    /// **On this computer**, wherever the engine is compiled in.
    ///
    /// This is a privacy default, and it is the one the app should have shipped with.
    /// Notes taken on a site visit are somebody's building, tenant or defect; sending
    /// them to a third party should be a decision the user makes, not one they discover
    /// they already made. It also needs no account and no API key, so the app works out
    /// of the box.
    ///
    /// The cost is that a fresh install cannot generate until the model download
    /// finishes. That is a visible, self-explaining wait — the progress bar is on
    /// screen and [`Settings::backend`] says so in as many words — whereas defaulting
    /// to a server asks for an address and a key before anything works at all.
    ///
    /// Falls back in order of what the build can actually do, because defaulting to
    /// something a build cannot perform would greet the user with an error rather than
    /// a working app.
    fn default() -> Self {
        if cfg!(feature = "inference") {
            Provider::Local
        } else if cfg!(feature = "remote") {
            Provider::Remote
        } else {
            Provider::Stub
        }
    }
}

/// Which palette the window uses.
///
/// Lives here rather than in the app for the same reason [`OpenAiConfig`] does: it is
/// plain serde data with no UI in it, and putting it in `Settings` gets it the atomic
/// write and the never-fails-to-load guarantee for free rather than a second file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    /// Follow the operating system. The default, because it is the only setting that is
    /// right without having been chosen.
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    /// The `data-theme` value the stylesheet selects on.
    ///
    /// Named after the attribute so that grepping either one finds the other — the
    /// stylesheet and this function have to agree, and nothing else would notice a
    /// value the CSS has no rule for: the app would simply render light and no one
    /// would know why. There is a test in the app that checks they agree.
    pub fn attribute(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }

    /// What the rail's Appearance button does.
    ///
    /// One cycling button rather than three radios: appearance is not worth a settings
    /// group, cycling means the label always states what you have, and pressing three
    /// times returns you to where you started.
    pub fn next(self) -> Theme {
        match self {
            Theme::System => Theme::Light,
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::System,
        }
    }

    /// The button's label, which doubles as its current value.
    pub fn label(self) -> &'static str {
        match self {
            Theme::System => "Appearance: system",
            Theme::Light => "Appearance: light",
            Theme::Dark => "Appearance: dark",
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
    #[serde(default)]
    pub appearance: Theme,
}

impl Settings {
    /// Load the settings, falling back to defaults.
    ///
    /// Never fails, and that is deliberate. A database written by a future version, a
    /// row that will not deserialise, a data directory that cannot be created — none of
    /// those should stop the app from starting. A user can always re-enter a base URL;
    /// they cannot fix a program that refuses to open.
    pub fn load() -> Settings {
        match Self::try_load() {
            Ok(Some(settings)) => settings,
            // No row yet: a fresh install, which is not worth a warning.
            Ok(None) => Settings::default(),
            Err(error) => {
                tracing::warn!("settings: unreadable ({error:#}), using defaults");
                Settings::default()
            }
        }
    }

    fn try_load() -> Result<Option<Settings>> {
        use rusqlite::OptionalExtension;

        let connection = crate::db::open()?;
        let body: Option<String> = connection
            .query_row("SELECT body FROM settings WHERE id = 1", [], |row| row.get(0))
            .optional()
            .context("reading the settings row")?;

        match body {
            // Deserialised leniently, as before: every field carries `#[serde(default)]`,
            // so a blob written before a field existed still opens.
            Some(body) => Ok(Some(serde_json::from_str(&body).context("parsing the settings")?)),
            None => Ok(None),
        }
    }

    /// Write the settings.
    ///
    /// A single upsert of one row, which is atomic — the temporary-file-and-rename dance
    /// this used to need is now the database's problem rather than ours.
    pub fn save(&self) -> Result<()> {
        crate::db::write_settings(&crate::db::open()?, self)
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
            appearance: Theme::Dark,
        };
        let text = serde_json::to_string(&settings).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&text).unwrap(), settings);
    }

    #[test]
    fn the_appearance_cycle_returns_to_where_it_started() {
        // Three presses and you are back, which is what makes one button acceptable
        // where three radios would otherwise be needed.
        let mut theme = Theme::System;
        for _ in 0..3 {
            theme = theme.next();
        }
        assert_eq!(theme, Theme::System);
        // Every state must be reachable, or the button silently has two positions.
        assert_eq!(Theme::System.next(), Theme::Light);
        assert_eq!(Theme::Light.next(), Theme::Dark);
    }

    #[test]
    fn a_theme_serialises_as_the_attribute_the_stylesheet_expects() {
        // `attribute` is what the CSS selects on and the serde name is what lands in
        // settings.json; they are allowed to differ, so both are pinned here.
        assert_eq!(Theme::System.attribute(), "system");
        assert_eq!(Theme::Dark.attribute(), "dark");
        assert_eq!(serde_json::to_string(&Theme::Dark).unwrap(), r#""dark""#);
        assert_eq!(serde_json::from_str::<Theme>(r#""light""#).unwrap(), Theme::Light);
    }

    #[test]
    fn a_fresh_install_writes_reports_on_this_computer() {
        // A product decision, pinned so it cannot drift back with a refactor. Notes from
        // a site visit are somebody's building; sending them to a third party has to be
        // something the user chose, not something they discover they defaulted into.
        //
        // Asserted per build rather than unconditionally, because the fallback order is
        // part of the same decision: default to whatever this build can actually do, or
        // the first click is an error instead of a report.
        let provider = Settings::default().provider;
        if cfg!(feature = "inference") {
            assert_eq!(provider, Provider::Local, "the engine is here, so use it");
        } else if cfg!(feature = "remote") {
            assert_eq!(provider, Provider::Remote, "no local engine; a server is the next best");
        } else {
            assert_eq!(provider, Provider::Stub, "nothing else in this build works");
        }
    }

    #[cfg(all(feature = "inference", feature = "worker"))]
    #[test]
    fn a_fresh_install_explains_the_wait_rather_than_failing_blankly() {
        // The cost of defaulting to Local: until the 5 GB download lands there is no
        // model, and pressing Generate has to say why in terms the user can act on.
        let _dir = crate::testenv::data_dir("settings-fresh");
        let error = Settings::default().backend().map(|b| b.describe()).unwrap_err().to_string();
        assert!(error.contains("still downloading"), "{error}");
        assert!(error.contains("Settings"), "and how to bypass it: {error}");
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
        // Written before `appearance` existed, so it must default rather than fail the
        // whole file and reset the user's provider and key along with it.
        assert_eq!(settings.appearance, Theme::System);
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
