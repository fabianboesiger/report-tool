//! The editor stylesheet, injected once by
//! [`EditorRuntime`](crate::EditorRuntime).
//!
//! Inlined with `include_str!` rather than shipped through the asset pipeline: a
//! library crate's assets have to be re-declared by every binary that uses it, and a
//! stylesheet this small is not worth that coupling.
//!
//! ## Palette
//!
//! Every colour resolves from a custom property the host may define, falling back to the
//! editor's own light and dark defaults when it does not:
//!
//! | Property | Used for |
//! |---|---|
//! | `--text` | body text |
//! | `--muted` | list markers, quotes, placeholders, a level-six heading |
//! | `--line` | the toolbar rule, a quote's left border, separators |
//! | `--accent` | the active toolbar button's fill |
//! | `--on-accent` | that button's label, so an ink accent stays legible |
//! | `--sunk` | code blocks and inline code |
//!
//! Defining them is how an application dresses the editor in its own palette **without
//! this crate learning what that application is** — a custom property is a string in a
//! stylesheet, not a type, so nothing about the dependency direction changes.
//!
//! Type is inherited rather than set: `.rt-editor` declares `font-family: inherit`, so a
//! host that wants its report in a serif can simply say so on any ancestor.
//!
//! ## Restyling from the host
//!
//! [`Editor`](crate::Editor)'s `class` prop lands on `.rt-editor` itself, so a host rule
//! like `.rt-editor.doc .rt-h1` is a descendant selector at specificity (0,3,0) and wins
//! against anything in here regardless of which stylesheet is injected first.

pub const CSS: &str = include_str!("../assets/editor.css");
