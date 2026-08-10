//! Filesystem locations for app data, templates, reports and model weights.
//!
//! macOS:   `~/Library/Application Support/ch.ajila.report-tool`
//! Linux:   `~/.local/share/report-tool`
//! Windows: `%APPDATA%\ajila\report-tool\data`
//!
//! The qualifier/org/app triple must stay in sync with the bundle identifier in
//! `app/Dioxus.toml`, or a bundled build and a `dx serve` build disagree about where
//! the user's reports live.

use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;

fn project_dirs() -> Result<ProjectDirs> {
    ProjectDirs::from("ch", "ajila", "report-tool")
        .context("could not determine a home/app-data directory for this platform")
}

/// Root data directory; created if missing.
///
/// `REPORT_DATA_DIR` overrides it. That makes a portable install possible — point it
/// at a USB stick or a synced folder and templates, reports and settings travel with
/// it — and it is what lets the store be tested against real files rather than only
/// at the serialisation layer.
pub fn data_dir() -> Result<PathBuf> {
    if let Ok(overridden) = std::env::var("REPORT_DATA_DIR") {
        let dir = PathBuf::from(overridden);
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("creating data dir {}", dir.display()))?;
        return Ok(dir);
    }
    let dir = project_dirs()?.data_dir().to_path_buf();
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating data dir {}", dir.display()))?;
    Ok(dir)
}

/// Persisted user settings (`settings.json` under the data dir).
pub fn settings_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("settings.json"))
}

/// Directory holding the user's templates, created if missing. One
/// `<uuid>.json` per template — plain files rather than a database, so they are
/// inspectable, diffable and trivially shareable between users.
pub fn templates_dir() -> Result<PathBuf> {
    ensure(data_dir()?.join("templates"))
}

/// Directory holding the user's reports, created if missing (`<uuid>.json` each).
pub fn reports_dir() -> Result<PathBuf> {
    ensure(data_dir()?.join("reports"))
}

/// Downloaded model weights (`<data_dir>/models`).
///
/// `REPORT_MODELS_DIR` overrides it when it points at an existing directory, which
/// is how a dev machine or a shared pre-seeded weights location gets used without
/// re-downloading several GB.
pub fn models_dir() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("REPORT_MODELS_DIR") {
        let p = PathBuf::from(p);
        if p.is_dir() {
            return Ok(p);
        }
    }
    ensure(data_dir()?.join("models"))
}

/// Per-model directory for a generation model (`<models>/llm/<id>`). Each model gets
/// its own directory so a GGUF and its sidecar files never collide across models.
pub fn llm_model_dir(id: &str) -> Result<PathBuf> {
    ensure(models_dir()?.join("llm").join(id))
}

/// Per-model directory for a transcription model (`<models>/stt/<id>`).
pub fn stt_model_dir(id: &str) -> Result<PathBuf> {
    ensure(models_dir()?.join("stt").join(id))
}

fn ensure(dir: PathBuf) -> Result<PathBuf> {
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    Ok(dir)
}
