//! Link-time setup for the `runtime-backends` build.
//!
//! With that feature, ggml is built as **shared** libraries so each GPU backend can
//! be a separately loadable module (one installer, every GPU — see
//! `report_core::gpu`). The binary then references `@rpath/libggml-base.0.dylib` and
//! friends, but nothing adds an `LC_RPATH`, so it dies in the dynamic loader before
//! `main`:
//!
//! ```text
//! dyld: Library not loaded: @rpath/libggml-base.0.dylib
//!   Reason: no LC_RPATH's found
//! ```
//!
//! This script adds the search paths for the layout the installers ship (the ggml
//! libraries sitting next to the executable, or in `Resources/` inside a macOS
//! `.app`), plus the build directory for `cargo run` during development.
//!
//! Without `runtime-backends` everything is statically linked and this is a no-op.
//!
//! Adapted from the `privstory` project, where the rpath layout was worked out.

use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_RUNTIME_BACKENDS");

    if std::env::var_os("CARGO_FEATURE_RUNTIME_BACKENDS").is_none() {
        return;
    }

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    match target_os.as_str() {
        "macos" => {
            // Next to the binary, and the `Resources/` dir of a bundled .app.
            rpath("@executable_path");
            rpath("@executable_path/../Resources");
            // `@loader_path` covers one ggml library resolving another.
            rpath("@loader_path");
        }
        "linux" => {
            rpath("$ORIGIN");
            rpath("$ORIGIN/lib");
        }
        // Windows has no rpath: the loader already searches the executable's own
        // directory, which is where the installer puts the DLLs.
        _ => {}
    }

    // Development only. `cargo run` leaves the binary in `target/<profile>/` while
    // the ggml libraries stay under `target/<profile>/build/<crate>-<hash>/out/lib`,
    // which no relative rpath can reach. Release builds deliberately skip this:
    // baking an absolute path would leak the builder's home directory into the
    // shipped binary, which is exactly what the `--remap-path-prefix` flags in
    // `.cargo/config.toml` exist to prevent.
    if std::env::var("PROFILE").as_deref() != Ok("release") {
        for dir in sys_lib_dirs() {
            rpath(&dir.to_string_lossy());
        }
    }
}

fn rpath(path: &str) {
    // `rustc-link-arg-bins` rather than `rustc-link-arg`: only the app binary needs
    // this, and applying it to build scripts or tests would be noise.
    println!("cargo:rustc-link-arg-bins=-Wl,-rpath,{path}");
}

/// Locate the `out/lib` directories of the `-sys` crates that emit shared libraries.
///
/// `OUT_DIR` is `target/<profile>/build/report-tool-<hash>/out`, so the sibling build
/// directories are two levels up. There is no cleaner route: `links`-key metadata
/// reaches only *direct* dependents of the sys crate, and this package depends on it
/// transitively through `report-core`.
fn sys_lib_dirs() -> Vec<PathBuf> {
    let Ok(out_dir) = std::env::var("OUT_DIR") else {
        return Vec::new();
    };
    let Some(build_root) = Path::new(&out_dir).parent().and_then(|p| p.parent()) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(build_root) else {
        return Vec::new();
    };

    let mut dirs = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("llama-cpp-sys-2-") && !name.starts_with("whisper-rs-sys-") {
            continue;
        }
        let lib = entry.path().join("out").join("lib");
        if lib.is_dir() {
            dirs.push(lib);
        }
    }
    dirs
}
