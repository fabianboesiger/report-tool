//! The shared document model: `RichDoc`, and the conversions and edits around it.
//!
//! One model serves three surfaces — the notes pane, the generated report, and (via
//! the same inline primitive) the template builder's description fields. Keeping it
//! here, below both the editor and the domain crate, is what lets `report-core`
//! render a generated report without ever linking Dioxus.
//!
//! ## Why the block list is flat
//!
//! `Block` carries an `indent` on list items rather than nesting lists inside one
//! another. A nested tree is the obvious model and the wrong one for an editor:
//! every structural edit (split a block, merge into the previous one, outdent the
//! first item of a sublist) becomes a tree surgery with special cases at every
//! boundary, whereas on a flat list they are `Vec` operations. Nesting is a
//! *rendering* concern, reconstructed in `markdown::to_markdown`.

pub mod doc;
pub mod html;
pub mod markdown;
pub mod ops;

pub use doc::{Block, BlockId, BlockKind, Marks, RichDoc, Span};
