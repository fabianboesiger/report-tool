//! Let the linker keep going when both ggml copies define the same symbol.
//!
//! `llama-cpp-sys-2` and `whisper-rs-sys` each vendor ggml, so anything linking both
//! ends up with two copies of the same objects. Apple's ld keeps one definition and
//! says nothing; `rust-lld` stops with `duplicate symbol: gguf_type_size` and link.exe
//! with `LNK2005: ggml_abort already defined`.
//!
//! This script covers *this crate's* own targets — the `--features inference` tests
//! and the `llm_smoke` example, which link both engines exactly as the app does. The
//! app binary is handled by the identical arm in `app/build.rs`; a build script's link
//! args reach only the package it belongs to, so both need it.
//!
//! Why this is safe here, and why it must be a link arg rather than a `rustflags`
//! entry, is written out in `app/build.rs` and in the module docs of `crate::worker`.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_INFERENCE");

    if std::env::var_os("CARGO_FEATURE_INFERENCE").is_none() {
        return;
    }

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg=/FORCE:MULTIPLE");
    } else if target_os == "linux" || target_os == "windows" {
        println!("cargo:rustc-link-arg=-Wl,--allow-multiple-definition");
    }
}
