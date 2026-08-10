//! The editor stylesheet, injected once by
//! [`EditorRuntime`](crate::EditorRuntime).
//!
//! Inlined with `include_str!` rather than shipped through the asset pipeline: a
//! library crate's assets have to be re-declared by every binary that uses it, and a
//! stylesheet this small is not worth that coupling.

pub const CSS: &str = include_str!("../assets/editor.css");
