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

/// The database holding templates, reports and settings.
///
/// One file, so backing the library up is copying it. Not created here — opening it is
/// [`crate::db::open`]'s job, which also has to bring the schema up to date.
pub fn db_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("report-tool.db"))
}

// --- legacy locations ------------------------------------------------------
//
// Templates, reports and settings used to be one JSON file each. Everything below
// exists **only** so `crate::db::import_legacy` can bring that data into the database
// on first launch; nothing else should reach for them. They are not `ensure`d any more
// either — creating a directory in order to find it empty is how the importer used to
// end up with an empty `templates/` beside the database it had just migrated into.

/// Where settings used to live.
pub fn legacy_settings_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("settings.json"))
}

/// Where templates used to live, one `<uuid>.json` each.
pub fn legacy_templates_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("templates"))
}

/// Where reports used to live, one `<uuid>.json` each.
pub fn legacy_reports_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("reports"))
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
