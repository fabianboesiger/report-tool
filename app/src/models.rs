//! Fetching the models the app needs, on first open.
//!
//! Starts by itself when the window opens and continues where it left off if the app
//! was quit mid-download. Sequential rather than parallel: the two downloads would
//! only compete for the same connection, and the ordering in
//! [`report_core::catalog::ALL`] is what makes the app useful during the long one —
//! dictation lands in about a minute, so notes can be taken while the report model is
//! still arriving.

use dioxus::prelude::*;
use report_core::catalog::{self, ModelSpec};

use crate::i18n::t;
use crate::ui::kit::{Banner, Bar};

#[derive(Debug, Clone, PartialEq)]
pub enum Stage {
    /// Queued behind another download.
    Waiting,
    Fetching {
        downloaded: u64,
        total: Option<u64>,
    },
    Ready,
    Failed(String),
    /// A path was set in Settings, so there is nothing to fetch.
    Configured,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelStatus {
    pub name: &'static str,
    pub stage: Stage,
}

impl ModelStatus {
    /// Whether this is worth showing. A model that is present, or one the user
    /// pointed at themselves, is not news.
    pub fn is_noteworthy(&self) -> bool {
        !matches!(self.stage, Stage::Ready | Stage::Configured)
    }

    /// Progress from 0 to 1, when it is known.
    pub fn fraction(&self) -> Option<f32> {
        match self.stage {
            Stage::Fetching { downloaded, total: Some(total) } if total > 0 => {
                Some((downloaded as f64 / total as f64) as f32)
            }
            _ => None,
        }
    }

    /// The byte counts a fetch has to show, already humanised.
    ///
    /// Split out from [`ModelStatus::detail`] so the arithmetic stays testable: `detail`
    /// translates, and a translating function can only be exercised inside a running app.
    fn bytes(&self) -> Option<(String, Option<String>)> {
        match self.stage {
            Stage::Fetching { downloaded, total } => Some((
                report_core::download::human_bytes(downloaded),
                total.map(report_core::download::human_bytes),
            )),
            _ => None,
        }
    }

    pub fn detail(&self) -> String {
        match &self.stage {
            Stage::Waiting => t!("models-stage-waiting"),
            Stage::Fetching { .. } => match self.bytes() {
                Some((done, Some(total))) => {
                    t!("models-stage-fetching", done: done.as_str(), total: total.as_str())
                }
                // An unknown total: the count alone, rather than a sentence with a hole in it.
                Some((done, None)) => done,
                None => String::new(),
            },
            Stage::Ready => t!("models-stage-ready"),
            // `report-core`'s own words, still English: a download failure is a diagnostic.
            Stage::Failed(error) => error.clone(),
            Stage::Configured => t!("models-stage-configured"),
        }
    }
}

/// Start the downloads and report on them.
///
/// Takes settings by value at mount: a path configured later should not cancel a
/// download already under way, and one configured earlier is read here.
pub fn use_model_downloads(settings: Signal<report_core::Settings>) -> Signal<Vec<ModelStatus>> {
    let statuses = use_signal(|| {
        catalog::ALL
            .iter()
            .map(|spec| ModelStatus {
                name: spec.name,
                stage: initial_stage(spec, &settings.read()),
            })
            .collect::<Vec<_>>()
    });

    use_hook(move || {
        let mut statuses = statuses;
        spawn(async move {
            for (index, spec) in catalog::ALL.iter().enumerate() {
                // Re-read rather than trusting the snapshot: an earlier download in
                // this same loop may have taken minutes.
                if !matches!(statuses.read()[index].stage, Stage::Waiting) {
                    continue;
                }
                fetch(index, spec, &mut statuses).await;
            }
        });
    });

    statuses
}

fn initial_stage(spec: &ModelSpec, settings: &report_core::Settings) -> Stage {
    let configured = match spec.kind {
        catalog::ModelKind::Report => !settings.local.model_path.trim().is_empty(),
        catalog::ModelKind::Dictation => !settings.stt.model_path.trim().is_empty(),
    };
    if configured {
        Stage::Configured
    } else if spec.is_present() {
        // A finished model is never handed to the downloader, so this is the only place
        // that ever looks beside it. An interrupted attempt can leave a `.part` the size
        // of the model itself, and without this nothing would ever reclaim it.
        if let Ok(path) = spec.path() {
            let reclaimed = report_core::download::discard_partial(&path);
            if reclaimed > 0 {
                tracing::info!(
                    "models: reclaimed {} from an abandoned {} download",
                    report_core::download::human_bytes(reclaimed),
                    spec.name
                );
            }
        }
        Stage::Ready
    } else {
        Stage::Waiting
    }
}

async fn fetch(index: usize, spec: &ModelSpec, statuses: &mut Signal<Vec<ModelStatus>>) {
    let path = match spec.path() {
        Ok(path) => path,
        Err(error) => {
            set(statuses, index, Stage::Failed(format!("{error:#}")));
            return;
        }
    };

    // Seeded from the partial file so a resumed download opens at the right place
    // rather than jumping from zero.
    let resumed = report_core::download::resume_offset(&path);
    set(statuses, index, Stage::Fetching { downloaded: resumed, total: Some(spec.approx_bytes) });

    tracing::info!("models: fetching {} ({})", spec.name, spec.url);
    let statuses_for_progress = *statuses;
    let result = transfer(spec, &path, index, statuses_for_progress).await;

    match result {
        Ok(()) => {
            tracing::info!("models: {} is ready", spec.name);
            set(statuses, index, Stage::Ready);
        }
        Err(error) => {
            // The partial file survives, so this is "try again next time" rather than
            // a permanent failure — worth saying, because the alternative reading is
            // that the app is broken.
            tracing::error!("models: {} failed: {error:#}", spec.name);
            set(statuses, index, Stage::Failed(format!("{error:#}")));
        }
    }
}

/// Perform the transfer, reporting progress.
#[cfg(feature = "inference")]
async fn transfer(
    spec: &ModelSpec,
    path: &std::path::Path,
    index: usize,
    mut statuses: Signal<Vec<ModelStatus>>,
) -> anyhow::Result<()> {
    report_core::download::fetch(spec.url, path, move |progress| {
        // Written on every chunk. Dioxus re-renders only when the value differs, and
        // a chunk is tens of kilobytes, so this is a handful of updates a second
        // rather than one per byte.
        statuses.with_mut(|all| {
            if let Some(status) = all.get_mut(index) {
                status.stage =
                    Stage::Fetching { downloaded: progress.downloaded, total: progress.total };
            }
        });
    })
    .await
}

/// A build with no engine has nothing to run the model with, so fetching gigabytes
/// would be pure waste. Said plainly rather than failing silently.
#[cfg(not(feature = "inference"))]
async fn transfer(
    _spec: &ModelSpec,
    _path: &std::path::Path,
    _index: usize,
    _statuses: Signal<Vec<ModelStatus>>,
) -> anyhow::Result<()> {
    anyhow::bail!("this build has no inference engine, so models are not downloaded")
}

fn set(statuses: &mut Signal<Vec<ModelStatus>>, index: usize, stage: Stage) {
    statuses.with_mut(|all| {
        if let Some(status) = all.get_mut(index) {
            status.stage = stage;
        }
    });
}

/// The download banner. Renders nothing once everything is in place.
#[component]
pub fn ModelBanner(statuses: Signal<Vec<ModelStatus>>) -> Element {
    let showing: Vec<ModelStatus> =
        statuses.read().iter().filter(|s| s.is_noteworthy()).cloned().collect();
    if showing.is_empty() {
        return rsx! {};
    }

    rsx! {
        for status in showing.iter() {
            Banner {
                key: "{status.name}",
                warn: matches!(status.stage, Stage::Failed(_)),
                span { {t!("models-preparing", name: status.name, detail: status.detail())} }
                // An unknown total still shows movement rather than an empty bar that
                // looks stuck.
                Bar { fraction: status.fraction() }
                span { class: "banner-tail",
                    // Which is the point of fetching the small one first: dictation lands
                    // in about a minute, so notes can be taken while the report model is
                    // still arriving.
                    {t!("models-keep-taking-notes")}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(stage: Stage) -> ModelStatus {
        ModelStatus { name: "test", stage }
    }

    #[test]
    fn only_unfinished_downloads_are_worth_showing() {
        assert!(status(Stage::Waiting).is_noteworthy());
        assert!(status(Stage::Fetching { downloaded: 1, total: Some(2) }).is_noteworthy());
        assert!(status(Stage::Failed("boom".into())).is_noteworthy());
        // A model that is present, or one the user pointed at, is not news.
        assert!(!status(Stage::Ready).is_noteworthy());
        assert!(!status(Stage::Configured).is_noteworthy());
    }

    #[test]
    fn progress_is_only_reported_when_the_total_is_known() {
        assert_eq!(
            status(Stage::Fetching { downloaded: 50, total: Some(200) }).fraction(),
            Some(0.25)
        );
        assert_eq!(status(Stage::Fetching { downloaded: 50, total: None }).fraction(), None);
        // Would otherwise divide by zero.
        assert_eq!(status(Stage::Fetching { downloaded: 0, total: Some(0) }).fraction(), None);
        assert_eq!(status(Stage::Ready).fraction(), None);
    }

    #[test]
    fn the_detail_line_reads_as_a_download_should() {
        // The counts, not the sentence around them: `detail` translates, and the wording is
        // pinned by the catalogue tests in `crate::i18n` instead.
        assert_eq!(
            status(Stage::Fetching { downloaded: 2_100_000_000, total: Some(5_150_000_000) })
                .bytes(),
            Some(("2.1 GB".to_string(), Some("5.2 GB".to_string())))
        );
        // An unknown total has to stay unknown rather than becoming a zero to divide by.
        assert_eq!(
            status(Stage::Fetching { downloaded: 574_000_000, total: None }).bytes(),
            Some(("574 MB".to_string(), None))
        );
        // Nothing to show for a stage that is not a transfer.
        assert_eq!(status(Stage::Ready).bytes(), None);
    }

    #[test]
    fn a_configured_path_means_nothing_is_fetched() {
        // Downloading five gigabytes the user does not need would be a poor way to
        // greet someone who already pointed the app at their own model.
        let mut settings = report_core::Settings::default();
        settings.local.model_path = "/my/own.gguf".into();
        assert_eq!(initial_stage(&catalog::REPORT_MODEL, &settings), Stage::Configured);
        // The other model is unaffected by that choice.
        assert_ne!(initial_stage(&catalog::DICTATION_MODEL, &settings), Stage::Configured);
    }
}
