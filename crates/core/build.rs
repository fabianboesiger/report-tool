//! Let the linker keep going when both ggml copies define the same symbol.
//!
//! `llama-cpp-sys-2` and `whisper-rs-sys` each vendor ggml, so anything linking both
//! ends up with two `gguf.cpp.o` in the link. Apple's ld keeps one definition and
//! says nothing; Rust's default Linux linker, `rust-lld`, stops with `duplicate
//! symbol: gguf_type_size` and ~40 more.
//!
//! This script covers *this crate's* own targets — the `--features inference` tests
//! and the `llm_smoke` example, which link both engines exactly as the app does. The
//! app binary is handled by the identical arm in `app/build.rs`; a build script's
//! link args reach only the package it belongs to, so both need it.
//!
//! Why this is safe here, and why it must be a link arg rather than a `rustflags`
//! entry, is written out in `app/build.rs` and in the module docs of
//! [`crate::worker`](src/worker.rs).

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_INFERENCE");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if std::env::var_os("CARGO_FEATURE_INFERENCE").is_some() && target_os == "linux" {
        println!("cargo:rustc-link-arg=-Wl,--allow-multiple-definition");
    }
}
