//! The WYSIWYG block editor.
//!
//! Two levels of API, which is what lets one library serve all three editing
//! surfaces in the app:
//!
//! - [`EditableText`] — the primitive. One `contenteditable` bound to a block id,
//!   carrying the focus guard that keeps the virtual DOM and the browser from
//!   fighting over the caret. Everything hard lives in [`editable`].
//! - [`Editor`] — the prose editor over a whole [`report_doc::RichDoc`], composed
//!   from `EditableText` plus the keymap. This is what notes and report editing use.
//!
//! The template builder in `app` uses `EditableText` directly for each node's
//! description and supplies its own container chrome, so it gets identical editing
//! behaviour and keybindings without pushing template types down into this crate.
//! **Nothing here may learn about `Template`, `Report` or any other domain type** —
//! that constraint is why this is a crate rather than a module in `app`.
//!
//! ## Mounting
//!
//! [`EditorRuntime`] must wrap anything that edits. It installs the browser-side
//! script and the stylesheet once, and provides the [`Bridge`] the editing surfaces
//! read their events from.
//!
//! ```ignore
//! rsx! {
//!     EditorRuntime {
//!         Editor { doc: notes }
//!     }
//! }
//! ```
//!
//! ## No JavaScript editor framework
//!
//! Deliberately hand-written. A ProseMirror or Quill would supply undo, selection and
//! paste handling, but the document would then live in JavaScript with Rust mirroring
//! it — and the template builder's containers would need custom node views written in
//! JavaScript too. Instead the structural semantics live in `report_doc::ops` as pure,
//! unit-tested transforms, the browser owns only the inline text inside one focused
//! block, and `assets/editor.js` stays a shim for the three things a webview will not
//! let Rust do: read and restore the caret, apply a mark to a selection, and
//! intercept a paste.

pub mod bridge;
pub mod editable;
pub mod editor;
pub mod keys;
pub mod styles;
pub mod toolbar;

pub use bridge::{use_bridge, Bridge, EditorRuntime, RawEvent};
pub use editable::{EditableText, Focus};
pub use editor::{doc_from_markdown, doc_to_markdown, Editor, EditorState, Selection};
pub use keys::{markdown_shortcut, Shortcut};
pub use toolbar::{Toolbar, ToolbarLabels};
