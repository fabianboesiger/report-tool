//! GPU backend discovery and selection for the local engines.
//!
//! ggml resolves backends at *runtime* from its device registry, so one binary can
//! serve NVIDIA, AMD, Intel, Apple and CPU-only machines, and a machine with no
//! usable GPU degrades to CPU instead of failing to start.
//!
//! `REPORT_BACKEND` overrides the choice:
//!   - `auto` (default) — offload to the GPU if one is present
//!   - `cpu`            — force CPU, for triaging a suspected GPU-driver bug
//!
//! An explicit request that cannot be honoured is a hard error rather than a silent
//! fallback: a silent 100x slowdown is the worst failure mode we can ship.
//!
//! Adapted from the `privstory` project's `gpu.rs`, which is where the runtime
//! backend selection was worked out.

use anyhow::{bail, Result};

/// How many transformer layers to place on the GPU.
///
/// llama.cpp treats any value >= the model's layer count as "all of them", so a
/// large constant means full offload without having to read the layer count first.
const ALL_LAYERS: u32 = 1_000_000;

/// What the process resolved to, for logging and for the Settings display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// Offload everything to a GPU (Metal / CUDA / Vulkan, whichever was compiled
    /// in and is present at runtime).
    Gpu,
    /// CPU only: either no GPU backend is available, or the user asked for CPU.
    Cpu,
}

impl Backend {
    /// `n_gpu_layers` to hand to llama.cpp for this choice.
    pub fn n_gpu_layers(self) -> u32 {
        match self {
            Backend::Gpu => ALL_LAYERS,
            Backend::Cpu => 0,
        }
    }

    /// Whether whisper.cpp should use the GPU. whisper-rs takes a bool rather than
    /// a layer count, since it offloads the whole encoder or none of it.
    pub fn use_gpu(self) -> bool {
        self == Backend::Gpu
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Backend::Gpu => "gpu",
            Backend::Cpu => "cpu",
        }
    }
}

/// Whether this build can actually offload anything.
///
/// The `inference` half matters: `app` enables `metal` unconditionally on macOS
/// (Cargo unions features, and the flag is inert without an engine behind it), so a
/// stub build would otherwise report that it is offloading to the GPU while carrying
/// no inference engine at all.
///
/// Runtime *availability* is a separate question that ggml answers when the model is
/// loaded; if the backend is compiled in but the device is missing, llama.cpp falls
/// back to CPU on its own rather than failing.
const fn gpu_compiled_in() -> bool {
    cfg!(all(feature = "inference", any(feature = "metal", feature = "cuda", feature = "vulkan")))
}

/// Which GPU backend this build carries, for the log line.
const fn compiled_backends() -> &'static str {
    match (cfg!(feature = "metal"), cfg!(feature = "cuda"), cfg!(feature = "vulkan")) {
        (true, _, _) => "metal",
        (_, true, _) => "cuda",
        (_, _, true) => "vulkan",
        _ => "none",
    }
}

/// Load ggml's runtime-selectable backend modules, if this build has them.
///
/// Idempotent and infallible by design: modules that fail to load (no driver, wrong
/// CPU ISA) are skipped by the registry. That is exactly what makes a single
/// installer safe to ship — a hard-linked CUDA build would instead fail in the
/// dynamic loader before `main` ever ran.
///
/// Search order, first hit wins:
///   1. `$REPORT_BACKENDS_DIR` — explicit override
///   2. `<exe dir>/backends` — the layout the installers ship
///   3. `<exe dir>/../Resources/backends` — inside a macOS .app bundle
///   4. the compile-time directory — development builds
#[cfg(all(feature = "inference", feature = "runtime-backends"))]
pub fn load_backends() {
    use std::path::PathBuf;

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(dir) = std::env::var("REPORT_BACKENDS_DIR") {
        candidates.push(PathBuf::from(dir));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            candidates.push(d.join("backends"));
            candidates.push(d.join("../Resources/backends"));
        }
    }

    for dir in &candidates {
        if dir.is_dir() {
            tracing::info!("gpu: loading ggml backends from {}", dir.display());
            llama_cpp_2::llama_backend::load_backends_from_path(dir);
            return;
        }
    }

    tracing::info!("gpu: no shipped backends dir found, using the compile-time default");
    llama_cpp_2::llama_backend::load_backends();
}

/// No-op when the backends are statically linked into the binary.
#[cfg(not(all(feature = "inference", feature = "runtime-backends")))]
pub fn load_backends() {}

/// Resolve the backend to use, honouring `REPORT_BACKEND`.
pub fn select() -> Result<Backend> {
    let requested = std::env::var("REPORT_BACKEND").unwrap_or_else(|_| "auto".into());
    let backend = match requested.trim().to_ascii_lowercase().as_str() {
        "cpu" => Backend::Cpu,
        "auto" | "" => {
            if gpu_compiled_in() {
                Backend::Gpu
            } else {
                Backend::Cpu
            }
        }
        // An explicit GPU request against a build with no GPU backend is a
        // configuration mistake, and silently running 100x slower would hide it.
        "gpu" | "metal" | "cuda" | "vulkan" => {
            if !gpu_compiled_in() {
                bail!(
                    "REPORT_BACKEND={requested} but this build has no GPU backend compiled in \
                     (build with --features metal|cuda|vulkan)"
                );
            }
            Backend::Gpu
        }
        other => bail!("unknown REPORT_BACKEND `{other}` (expected auto, gpu or cpu)"),
    };

    if !cfg!(feature = "inference") {
        // A stub build has no engine to offload, so neither of the messages below
        // would be true. Saying so plainly beats a warning about GPU backends in a
        // build that was never going to run a model.
        tracing::info!("gpu: stub build, local inference is not compiled in");
    } else if backend == Backend::Cpu && gpu_compiled_in() {
        tracing::warn!("gpu: running on CPU by request (REPORT_BACKEND={requested})");
    } else if backend == Backend::Cpu {
        tracing::warn!("gpu: no GPU backend compiled in, running on CPU (generation will be slow)");
    } else {
        tracing::info!("gpu: offloading to GPU (compiled backend: {})", compiled_backends());
    }
    Ok(backend)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `REPORT_BACKEND` is process-global, so these cases share one test rather than
    /// racing each other across threads.
    #[test]
    fn backend_selection_honours_the_env_override() {
        std::env::set_var("REPORT_BACKEND", "cpu");
        assert_eq!(select().unwrap(), Backend::Cpu);

        std::env::set_var("REPORT_BACKEND", "nonsense");
        assert!(select().is_err());

        // An explicit GPU request must fail loudly on a build with no GPU backend,
        // rather than quietly running 100x slower.
        std::env::set_var("REPORT_BACKEND", "gpu");
        assert_eq!(select().is_ok(), gpu_compiled_in());

        std::env::remove_var("REPORT_BACKEND");
        assert_eq!(select().unwrap(), if gpu_compiled_in() { Backend::Gpu } else { Backend::Cpu });
    }

    #[test]
    fn cpu_means_no_offload() {
        assert_eq!(Backend::Cpu.n_gpu_layers(), 0);
        assert!(!Backend::Cpu.use_gpu());
        assert!(Backend::Gpu.n_gpu_layers() > 0);
        assert!(Backend::Gpu.use_gpu());
    }
}
