//! The contenteditable primitive, and the focus guard that makes it work.
//!
//! ## The problem
//!
//! A virtual DOM and a `contenteditable` both believe they own an element's contents.
//! Left alone they fight: the user types, Rust re-renders the block from its model,
//! the browser's DOM node is replaced, and the caret jumps to the start — or the
//! character is dropped, or an input method's composition is aborted mid-word. This
//! is the long-standing difficulty behind [dioxus#611].
//!
//! ## The guard
//!
//! Ownership is split by focus, and only one block can be focused at a time:
//!
//! - **Not focused** — Rust owns it. The element's HTML is rendered from the model,
//!   as any other component would be.
//! - **Focused** — the browser owns it. Rust renders the *frozen* HTML captured at
//!   the moment focus arrived, so the attribute it hands the VDOM never changes, the
//!   diff finds nothing to do, and the DOM node is left strictly alone. Keystrokes
//!   still flow into the model through `input` events; they simply do not flow back.
//!
//! The DOM and the model are therefore never simultaneously authoritative, and the
//! caret is never touched by a render. The only way back in while focused is
//! [`Bridge::sync`](crate::Bridge::sync), used deliberately when Rust itself changes
//! a focused block.
//!
//! Stable [`BlockId`] keys are the other half: without them Dioxus may recreate an
//! untouched element, which destroys the caret just as surely as rewriting it.
//!
//! [dioxus#611]: https://github.com/DioxusLabs/dioxus/issues/611

use dioxus::prelude::*;
use report_doc::{html, BlockId, Span};

/// Which side currently owns a block's DOM subtree.
///
/// Held by the surrounding editing surface rather than by each block, because focus
/// is singular: one signal describes the whole document.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Focus {
    pub block: Option<BlockId>,
    /// The HTML the DOM held when focus arrived. Re-rendered verbatim for as long as
    /// the block stays focused, which is what stops the VDOM touching it.
    frozen: String,
}

impl Focus {
    /// Record that `id` gained focus, freezing what the browser currently has.
    ///
    /// `spans` must be the block's content *as last rendered*, since that is what the
    /// DOM was built from.
    pub fn enter(&mut self, id: BlockId, spans: &[Span]) {
        self.block = Some(id);
        self.frozen = html::spans_to_html(spans);
    }

    /// Record that `id` lost focus. Ignored if focus has already moved on: `focusout`
    /// for the old block can arrive after `focusin` for the new one, and honouring it
    /// blindly would unfreeze the block the user is now typing in.
    pub fn leave(&mut self, id: BlockId) {
        if self.block == Some(id) {
            self.block = None;
            self.frozen.clear();
        }
    }

    /// Replace the frozen HTML after Rust has deliberately changed a focused block.
    /// Pair with [`Bridge::sync`](crate::Bridge::sync), which performs the matching
    /// DOM write.
    pub fn refreeze(&mut self, html: String) {
        self.frozen = html;
    }

    pub fn has(&self, id: BlockId) -> bool {
        self.block == Some(id)
    }

    /// The HTML to render for a block: frozen while it is focused, from the model
    /// otherwise.
    pub fn html_for(&self, id: BlockId, spans: &[Span]) -> String {
        if self.has(id) {
            self.frozen.clone()
        } else {
            html::spans_to_html(spans)
        }
    }
}

/// One editable region: a `contenteditable` element bound to a block id.
///
/// Deliberately presentational. It renders the HTML it is given and nothing else —
/// no event handlers, no state — because every keystroke arrives through the
/// delegated listeners in `assets/editor.js` instead. The surrounding surface decides
/// what an edit means; this only has to not fight the browser.
#[component]
pub fn EditableText(
    /// Identifies the block to the browser shim. Also the Dioxus key, so an untouched
    /// element survives a re-render with its caret intact.
    id: BlockId,
    /// Already resolved through [`Focus::html_for`].
    html: String,
    #[props(default)] class: String,
    /// Shown while the block is empty, via CSS.
    #[props(default)]
    placeholder: String,
    /// A list bullet or ordinal, drawn by CSS in the element's padding.
    ///
    /// Kept out of the element's content on purpose: anything inside the editable box
    /// would become part of the text the browser reports back on every input event,
    /// and the marker would end up in the document.
    #[props(default)]
    marker: String,
) -> Element {
    rsx! {
        div {
            key: "{id}",
            class: "rt-editable {class}",
            "data-block-id": "{id}",
            "data-placeholder": "{placeholder}",
            "data-marker": "{marker}",
            contenteditable: "true",
            spellcheck: "true",
            // The one place this is warranted: the content is produced by
            // `report_doc::html::spans_to_html`, which emits only the four mark tags
            // and escapes everything else. Anything a user pastes has already been
            // through `html_to_spans`, which drops every element it does not model.
            dangerous_inner_html: "{html}",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use report_doc::Marks;

    fn spans(text: &str) -> Vec<Span> {
        vec![Span::plain(text)]
    }

    #[test]
    fn an_unfocused_block_renders_from_the_model() {
        let focus = Focus::default();
        let id = BlockId::new();
        assert_eq!(focus.html_for(id, &spans("hello")), "hello");
    }

    #[test]
    fn a_focused_block_renders_frozen_html_however_the_model_changes() {
        // The whole point: while the user is typing, what Rust hands the VDOM must
        // not move, or the diff will rewrite the element and take the caret with it.
        let id = BlockId::new();
        let mut focus = Focus::default();
        focus.enter(id, &spans("hello"));

        // The model has since advanced — this is what an `input` event produces.
        let updated = spans("hello world");
        assert_eq!(focus.html_for(id, &updated), "hello");
        assert_eq!(focus.html_for(id, &spans("anything at all")), "hello");
    }

    #[test]
    fn other_blocks_keep_rendering_from_the_model_while_one_is_focused() {
        let focused = BlockId::new();
        let other = BlockId::new();
        let mut focus = Focus::default();
        focus.enter(focused, &spans("typing here"));
        assert_eq!(focus.html_for(other, &spans("elsewhere")), "elsewhere");
    }

    #[test]
    fn leaving_focus_hands_the_block_back_to_the_model() {
        let id = BlockId::new();
        let mut focus = Focus::default();
        focus.enter(id, &spans("old"));
        focus.leave(id);
        assert_eq!(focus.html_for(id, &spans("new")), "new");
    }

    #[test]
    fn a_late_blur_for_the_previous_block_does_not_unfreeze_the_new_one() {
        // Clicking straight from one block into another delivers focusin for the new
        // block before focusout for the old. Acting on that blur would unfreeze the
        // block the user is now typing in, and the next render would eat the caret.
        let first = BlockId::new();
        let second = BlockId::new();
        let mut focus = Focus::default();
        focus.enter(first, &spans("first"));
        focus.enter(second, &spans("second"));
        focus.leave(first);

        assert!(focus.has(second), "focus must still be on the second block");
        assert_eq!(focus.html_for(second, &spans("changed")), "second");
    }

    #[test]
    fn refreezing_matches_a_deliberate_write_into_a_focused_block() {
        let id = BlockId::new();
        let mut focus = Focus::default();
        focus.enter(id, &spans("plain"));

        // What a toolbar bold does: the model changes and the DOM is written
        // directly, so the frozen copy has to move with it or the next render would
        // revert the user's formatting.
        let bolded = vec![Span::new("plain", Marks::BOLD)];
        let updated = html::spans_to_html(&bolded);
        focus.refreeze(updated.clone());
        assert_eq!(focus.html_for(id, &bolded), updated);
        assert_eq!(focus.html_for(id, &bolded), "<strong>plain</strong>");
    }

    #[test]
    fn frozen_html_matches_what_the_dom_was_built_from() {
        // `enter` must be given the spans the element was last *rendered* from; if it
        // captured something else the guard would freeze the wrong thing and the
        // block would visibly revert on blur.
        let id = BlockId::new();
        let content = vec![Span::plain("a "), Span::new("b", Marks::ITALIC)];
        let rendered = html::spans_to_html(&content);
        let mut focus = Focus::default();
        focus.enter(id, &content);
        assert_eq!(focus.html_for(id, &content), rendered);
    }
}
