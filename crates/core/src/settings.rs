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

/// What the microphone is expected to hear.
///
/// Three states rather than the free-text ISO code this replaced, because "the same as
/// the app" and "work it out yourself" are different answers and an empty string cannot
/// say both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Spoken {
    /// Follow [`Settings::language`]. The default: someone running the app in German is
    /// overwhelmingly likely to dictate in German, and saying so beats making whisper
    /// guess from the first seconds of audio.
    #[default]
    App,
    /// Let whisper detect the language per recording.
    Detect,
    /// A language named outright, for notes taken in one language with the app in
    /// another.
    Fixed(Locale),
}

/// Where the transcription model lives.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SttConfig {
    /// Path to a whisper.cpp `ggml-*.bin` model.
    #[serde(default)]
    pub model_path: String,
    #[serde(default)]
    pub spoken: Spoken,
    /// The free-text ISO code that [`Spoken`] replaced.
    ///
    /// Read once by [`SttConfig::whisper_language`] while `spoken` is still at its
    /// default, so that upgrading does not silently discard a language the user had
    /// forced. `skip_serializing` rather than a conditional skip: it is never written
    /// back at all, so the field leaves the stored blob the first time settings are
    /// saved and this can be deleted a version later. The old empty-means-detect default
    /// carries no information and is ignored.
    #[serde(default, skip_serializing)]
    pub language: String,
}

impl SttConfig {
    /// The code to hand `params.set_language`, or `None` for whisper's own detection.
    ///
    /// A wrongly forced language produces confident nonsense rather than a visible
    /// error, which is why [`Spoken::Detect`] stays on offer — but it is no longer what
    /// an unconfigured install gets, since the app now knows which language its user
    /// works in.
    pub fn whisper_language(&self, app: Locale) -> Option<&'static str> {
        match self.spoken {
            Spoken::Detect => None,
            Spoken::Fixed(locale) => Some(locale.tag()),
            // Only consulted here: a legacy code is a language the user chose once, and
            // dropping it on upgrade would change what dictation does without telling
            // anyone.
            Spoken::App => match Locale::from_tag(&self.language) {
                Some(legacy) => Some(legacy.tag()),
                None => Some(app.tag()),
            },
        }
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

/// Which language the app works in.
///
/// One setting for three things — the interface, the language reports are written in, and
/// what dictation expects to hear — because they are one decision. Someone working in
/// French wants all three in French, and three separate controls would only ever be set
/// to the same value while offering a dozen ways to get it wrong.
///
/// `System` is the default for the same reason [`Theme::System`] is: it is the only value
/// that is right without having been chosen, and unlike a language resolved once at first
/// launch it keeps following the operating system when the user changes it there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    System,
    German,
    English,
    French,
    Italian,
}

impl Language {
    /// The language to actually use.
    ///
    /// Never `System`, which is the point of the second type: everything downstream — the
    /// Fluent bundle, the prompt, whisper — needs a language it can name, and threading a
    /// "whatever the OS says" variant through all of them would mean each one resolving
    /// it again and getting a different answer if the environment changed in between.
    pub fn resolve(self) -> Locale {
        match self {
            Language::System => detect(),
            Language::German => Locale::German,
            Language::English => Locale::English,
            Language::French => Locale::French,
            Language::Italian => Locale::Italian,
        }
    }
}

/// A language the app actually ships.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Locale {
    German,
    English,
    French,
    Italian,
}

impl Locale {
    /// Every language, in the order the settings picker offers them: the three Swiss
    /// official languages this tool is used in, then English.
    pub const ALL: [Locale; 4] = [Locale::German, Locale::English, Locale::French, Locale::Italian];

    /// The primary subtag, which is all four of its uses want.
    ///
    /// It names the Fluent catalogue, it is the `lang` attribute on the shell — which is
    /// what the webview's spellchecker reads inside the editor's `contenteditable` — and
    /// it is the code whisper takes. One function because a second spelling of the same
    /// value is a second thing to keep in step.
    pub fn tag(self) -> &'static str {
        match self {
            Locale::German => "de",
            Locale::English => "en",
            Locale::French => "fr",
            Locale::Italian => "it",
        }
    }

    /// The language's name in itself.
    ///
    /// Endonyms, not translations, because the language picker is the one control you
    /// have to be able to read *before* the app is in a language you understand.
    pub fn endonym(self) -> &'static str {
        match self {
            Locale::German => "Deutsch",
            Locale::English => "English",
            Locale::French => "Français",
            Locale::Italian => "Italiano",
        }
    }

    /// The name to use when telling the model which language to write in.
    ///
    /// English, deliberately: the rest of the system prompt is English, and an
    /// instruction that stays in one language is the one that reliably lands. See
    /// [`crate::prompt::system`].
    pub fn in_english(self) -> &'static str {
        match self {
            Locale::German => "German",
            Locale::English => "English",
            Locale::French => "French",
            Locale::Italian => "Italian",
        }
    }

    /// The language a BCP-47 tag asks for, if it is one we ship.
    ///
    /// Only the primary subtag is considered: `de-CH`, `de-DE` and `de` are one catalogue
    /// here, and there is nothing to gain from distinguishing them. Underscores are
    /// accepted because that is what a POSIX `LANG` looks like (`de_CH.UTF-8`).
    pub fn from_tag(tag: &str) -> Option<Locale> {
        let primary = tag.trim().split(['-', '_', '.']).next()?.to_ascii_lowercase();
        Locale::ALL.into_iter().find(|locale| locale.tag() == primary)
    }
}

/// The operating system's language, or English.
///
/// English rather than an error for anything outside the four: a Japanese or Portuguese
/// system is not a misconfiguration, and the honest response to a language we do not have
/// is the one language nearly every user of this tool also reads.
///
/// ## Testing this by hand
///
/// Each platform is asked in its own way, and only one of them is the obvious one:
///
/// ```text
/// macOS    ./report-tool -AppleLanguages "(fr-CH)"    # CFLocaleCopyPreferredLanguages
/// Linux    LANG=fr_CH.UTF-8 ./report-tool             # LC_ALL / LC_MESSAGES / LANG
/// Windows                                             # GetUserDefaultLocaleName
/// ```
///
/// Worth writing down because **`LANG` does nothing on macOS** — `sys-locale` reads
/// CoreFoundation there, so exporting it changes the app not at all and reads as detection
/// being broken when it is working exactly as intended.
fn detect() -> Locale {
    sys_locale::get_locale().as_deref().and_then(Locale::from_tag).unwrap_or(Locale::English)
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
    pub language: Language,
    #[serde(default)]
    pub appearance: Theme,
}

impl Settings {
    /// The language to use: the chosen one, or the operating system's.
    ///
    /// Everything that needs a language goes through here rather than reading
    /// [`Settings::language`] directly, so `System` is resolved in exactly one place.
    pub fn locale(&self) -> Locale {
        self.language.resolve()
    }

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
            stt: SttConfig {
                model_path: "/models/ggml-base.bin".into(),
                spoken: Spoken::Fixed(Locale::German),
                language: String::new(),
            },
            language: Language::French,
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
    fn a_language_resolves_to_one_the_app_actually_ships() {
        // The whole reason `resolve` returns a second type: nothing downstream should ever
        // have to handle a "whatever the OS says" variant.
        for language in [
            Language::System,
            Language::German,
            Language::English,
            Language::French,
            Language::Italian,
        ] {
            assert!(Locale::ALL.contains(&language.resolve()), "{language:?}");
        }
        assert_eq!(Language::German.resolve(), Locale::German);
        // `System` reads the environment, so the value is not assertable — only that it is
        // one of the four, which the loop above already covers.
    }

    #[test]
    fn a_tag_is_matched_on_its_primary_subtag_only() {
        // Regions do not get their own catalogue: `de-CH` and `de-DE` are one language
        // here, and a POSIX `LANG` brings its own punctuation.
        assert_eq!(Locale::from_tag("de"), Some(Locale::German));
        assert_eq!(Locale::from_tag("de-CH"), Some(Locale::German));
        assert_eq!(Locale::from_tag("de_CH.UTF-8"), Some(Locale::German));
        assert_eq!(Locale::from_tag("FR-ch"), Some(Locale::French), "case must not matter");
        assert_eq!(Locale::from_tag(" it "), Some(Locale::Italian));

        // A language we do not ship is not a misconfiguration; the caller falls back.
        assert_eq!(Locale::from_tag("ja"), None);
        assert_eq!(Locale::from_tag("pt-BR"), None);
        assert_eq!(Locale::from_tag(""), None);
        assert_eq!(Locale::from_tag("not a language tag"), None);
        // `C` and `POSIX` are what a stripped-down container reports.
        assert_eq!(Locale::from_tag("C"), None);
        assert_eq!(Locale::from_tag("POSIX"), None);
    }

    #[test]
    fn every_locale_has_a_distinct_tag_and_endonym() {
        // Two locales sharing a tag would mean one silently rendering the other's
        // catalogue, and a duplicated endonym would leave the picker with two
        // indistinguishable rows.
        for (index, locale) in Locale::ALL.into_iter().enumerate() {
            for other in Locale::ALL.into_iter().skip(index + 1) {
                assert_ne!(locale.tag(), other.tag(), "{locale:?} and {other:?}");
                assert_ne!(locale.endonym(), other.endonym(), "{locale:?} and {other:?}");
                assert_ne!(locale.in_english(), other.in_english(), "{locale:?} and {other:?}");
            }
            // The tag is the Fluent langid, the `lang` attribute and whisper's code all at
            // once, so it has to stay a bare primary subtag.
            assert_eq!(locale.tag().len(), 2, "{locale:?}");
            assert_eq!(Locale::from_tag(locale.tag()), Some(locale), "{locale:?} must round-trip");
        }
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
        // Same for `language`, which every existing install predates: following the system
        // is what a settings blob with nothing to say about language must mean.
        assert_eq!(settings.language, Language::System);
        assert_eq!(settings.stt.spoken, Spoken::App);
    }

    #[test]
    fn the_language_is_left_out_of_a_blob_that_never_set_it() {
        // `spoken` and `language` both round-trip, but the legacy free-text code must not
        // be written back — it exists only to be read once on upgrade.
        let settings = Settings {
            stt: SttConfig { spoken: Spoken::Detect, language: "de".into(), ..Default::default() },
            language: Language::Italian,
            ..Default::default()
        };
        let text = serde_json::to_string(&settings).unwrap();
        assert!(text.contains(r#""language":"italian""#), "{text}");
        assert!(text.contains(r#""spoken":"detect""#), "{text}");
        assert!(!text.contains(r#""language":"de""#), "the legacy code must not be re-saved");
    }

    // Only meaningful where the connector exists; without it the backend refuses
    // for a different and equally clear reason, covered below.
    #[cfg(feature = "remote")]
    #[test]
    fn an_unconfigured_remote_backend_says_what_is_missing() {
        // `Remote` named outright rather than taken from `Settings::default()`, which is
        // `Local` in a build with the engine — the test then built the *local* backend and
        // passed or failed depending on whether a model happened to be downloaded on the
        // machine running it. Green in CI, red on any developer's laptop that had used the
        // app once.
        let mut settings = Settings { provider: Provider::Remote, ..Default::default() };
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
            stt: SttConfig { model_path: " /my/whisper.bin ".into(), ..Default::default() },
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
    fn an_unconfigured_install_dictates_in_the_app_language() {
        // The behaviour change from the free-text field this replaced: whisper used to
        // detect by default, and now it is told what to expect, because the app knows.
        let config = SttConfig::default();
        assert_eq!(config.whisper_language(Locale::French), Some("fr"));
        assert_eq!(config.whisper_language(Locale::German), Some("de"));
    }

    #[test]
    fn detection_is_still_reachable_and_still_means_no_code() {
        // Passing "" through to whisper would force an unnamed language rather than
        // letting it detect one, so the absence has to be an `Option`.
        let config = SttConfig { spoken: Spoken::Detect, ..Default::default() };
        assert_eq!(config.whisper_language(Locale::German), None);
    }

    #[test]
    fn a_named_language_overrides_the_app() {
        // The case the setting exists for: notes dictated in one language with the
        // interface in another.
        let config = SttConfig { spoken: Spoken::Fixed(Locale::Italian), ..Default::default() };
        assert_eq!(config.whisper_language(Locale::German), Some("it"));
    }

    #[test]
    fn a_legacy_iso_code_survives_the_upgrade() {
        // A code in the old field is a language the user chose once. Dropping it would
        // change what dictation does without telling anyone — the one thing an upgrade
        // must not do.
        let config = SttConfig { language: "de-CH".into(), ..Default::default() };
        assert_eq!(config.whisper_language(Locale::English), Some("de"));

        // But it loses to an explicit choice, since that is the newer decision.
        let config =
            SttConfig { spoken: Spoken::Detect, language: "de".into(), ..Default::default() };
        assert_eq!(config.whisper_language(Locale::English), None);

        // And a code we cannot make sense of falls through to the app language rather
        // than being forwarded to whisper as-is.
        let config = SttConfig { language: "klingon".into(), ..Default::default() };
        assert_eq!(config.whisper_language(Locale::French), Some("fr"));
    }
}
