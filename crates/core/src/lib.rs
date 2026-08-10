//! Report templates, the constrained-generation compilers, and the inference
//! backends.
//!
//! UI-free by construction: this crate never depends on `report-editor` or Dioxus,
//! so `cargo test -p report-core` runs in seconds and the template compilers can be
//! exercised without a webview.
//!
//! ## The shape of report generation
//!
//! A template is compiled into a JSON *shape*, the model is constrained to produce
//! exactly that shape, and the markdown is then rendered from the result by
//! [`render`] — deterministically, in Rust. Headings, their levels, ordering and
//! formatting therefore come from the template and never from the model, which only
//! ever supplies the text that goes inside them.
//!
//! Constraining requires two emitters from one traversal, because the two backends
//! speak different dialects:
//!
//! - remote: JSON Schema, via OpenAI's `response_format: {"type": "json_schema"}`
//! - local:  GBNF, via `LlamaSampler::grammar` — `llama-cpp-2` exposes no
//!   schema-based constraint
//!
//! Both must describe the same shape. Their agreement is enforced by tests in
//! [`compile`], and it is the property the whole design rests on.

/// Whisper's fixed input rate.
///
/// Deliberately *not* behind the `inference` feature: the capture and resampling
/// pipeline has to target it in every build, including one with no engine compiled
/// in. Gating it would make a stub build fail to compile for no reason.
pub const STT_SAMPLE_RATE: u32 = 16_000;

pub mod backend;
/// The models the app fetches for itself.
pub mod catalog;
pub mod compile;

/// The database behind `store` and `Settings`: opening it, migrating it, and the
/// one-time import of the JSON files that used to be the storage layer.
pub mod db;
/// Resumable downloads. Only `download::fetch` needs an HTTP client; the rest of
/// the module compiles everywhere so the UI can describe a download in any build.
pub mod download;
pub mod edit;

pub mod gpu;

/// Local generation through llama.cpp. Needs an engine, hence the gate.
#[cfg(feature = "inference")]
pub mod llm;

/// Speech to text through whisper.cpp. Needs an engine, hence the gate.
#[cfg(feature = "inference")]
pub mod stt;

/// The local backend, which talks to the inference worker.
#[cfg(all(feature = "inference", feature = "worker"))]
pub mod local;

pub mod paths;

/// The OpenAI-compatible connector. Needs `reqwest`, hence the feature gate.
#[cfg(feature = "remote")]
pub mod openai;
pub mod prompt;
pub mod render;
pub mod settings;
pub mod store;
pub mod template;

/// Out-of-process inference workers. See the module docs for why there are two.
pub mod worker;

/// Test-only helpers for the parts of the crate that read process-global state.
#[cfg(test)]
pub(crate) mod testenv {
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, MutexGuard};

    static LOCK: Mutex<()> = Mutex::new(());

    /// Points `REPORT_DATA_DIR` at a private directory for as long as it is held.
    ///
    /// The variable is process-global and Rust runs tests in parallel, so every test
    /// that touches it must take the same lock. Without this they overwrite each
    /// other's value and fail depending on how they happen to interleave — which is
    /// exactly the sort of failure that looks like flakiness and gets re-run rather
    /// than fixed.
    pub struct DataDir {
        _guard: MutexGuard<'static, ()>,
        path: PathBuf,
    }

    impl DataDir {
        pub fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for DataDir {
        fn drop(&mut self) {
            std::env::remove_var("REPORT_DATA_DIR");
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// Claim the lock and a fresh directory.
    pub fn data_dir(name: &str) -> DataDir {
        // Poisoning only means another test panicked while holding it; the directory
        // is fresh either way, so there is nothing to recover.
        let guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let path =
            std::env::temp_dir().join(format!("report-tool-{name}-{}", uuid::Uuid::new_v4()));
        std::env::set_var("REPORT_DATA_DIR", &path);
        DataDir { _guard: guard, path }
    }
}

/// A GBNF matcher used to check the emitted grammar in tests. See the module docs
/// for why the tests carry their own rather than taking a dependency.
#[cfg(test)]
mod gbnf_match;

pub use backend::{JsonRequest, LlmBackend, StubBackend};
pub use compile::{Shape, ShapeError};
pub use settings::{Provider, Settings};
pub use store::Report;
pub use template::{NodeKind, Template, TemplateNode};
