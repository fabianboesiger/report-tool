//! The prose editor: a whole [`RichDoc`], edited in place.
//!
//! Composed from [`EditableText`] plus the keymap. Structural edits are delegated to
//! `report_doc::ops`, which is where their boundary cases are tested; this module is
//! the wiring between a browser event and the right call.

use dioxus::prelude::*;
use report_doc::{html, ops, Block, BlockId, BlockKind, Marks, RichDoc, Span};

use crate::bridge::{use_bridge, Bridge, RawEvent};
use crate::editable::{EditableText, Focus};
use crate::keys;
use crate::toolbar::Toolbar;

/// A selected range within one block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Selection {
    pub block: BlockId,
    pub start: usize,
    pub end: usize,
}

impl Selection {
    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }
}

/// The shared state of one editing surface, published so a toolbar can act on it.
#[derive(Clone, Copy)]
pub struct EditorState {
    pub doc: Signal<RichDoc>,
    pub focus: Signal<Focus>,
    pub selection: Signal<Option<Selection>>,
    pub bridge: Bridge,
}

impl EditorState {
    /// Toggle an inline mark over the current selection.
    ///
    /// Writes straight into the DOM afterwards, because the block is focused and the
    /// focus guard has therefore stopped rendering it — see [`crate::editable`].
    pub fn toggle_mark(&self, mark: Marks) {
        let Some(range) = *self.selection.read() else { return };
        if range.is_empty() {
            return;
        }
        let mut doc = self.doc;
        let mut d = doc.write();
        ops::toggle_mark_in_range(&mut d, range.block, range.start, range.end, mark);
        let updated = d.block(range.block).map(|b| html::spans_to_html(&b.content));
        drop(d);

        let Some(updated) = updated else { return };
        if self.focus.read().has(range.block) {
            self.focus.clone().write().refreeze(updated.clone());
            self.bridge.sync(range.block, &updated, range.start, range.end);
        }
    }

    /// Change the kind of the focused block.
    ///
    /// Needs no DOM write: only the element's class changes, and the text inside it
    /// is untouched, so the caret stays where the user left it.
    pub fn set_kind(&self, kind: BlockKind) {
        let Some(id) = self.current_block() else { return };
        let mut doc = self.doc;
        ops::set_kind(&mut doc.write(), id, kind);
    }

    /// The block the user is working in: the focused one, or whatever the last
    /// selection touched if focus has since moved to a toolbar button.
    pub fn current_block(&self) -> Option<BlockId> {
        self.focus.read().block.or_else(|| self.selection.read().as_ref().map(|s| s.block))
    }

    pub fn current_kind(&self) -> Option<BlockKind> {
        let id = self.current_block()?;
        self.doc.read().block(id).map(|b| b.kind.clone())
    }
}

/// A WYSIWYG editor over `doc`.
#[component]
pub fn Editor(
    doc: Signal<RichDoc>,
    #[props(default = true)] toolbar: bool,
    #[props(default)] class: String,
    #[props(default = "Start typing…".to_string())] placeholder: String,
) -> Element {
    let bridge = use_bridge();
    let focus = use_signal(Focus::default);
    let selection = use_signal(|| None::<Selection>);
    // Where to put the caret once the pending render has produced the element. It
    // cannot be done inline: a block created by splitting does not exist in the DOM
    // until the render commits, and focusing it any earlier silently does nothing.
    let mut pending = use_signal(|| None::<(BlockId, usize)>);
    let mut seen = use_signal(|| 0u64);

    let state = EditorState { doc, focus, selection, bridge };
    use_context_provider(|| state);

    use_effect(move || {
        let Some(delivery) = bridge.latest() else { return };
        // `peek` rather than a read: subscribing to the counter we are about to
        // write would make this effect re-trigger itself forever.
        if delivery.seq <= *seen.peek() {
            return;
        }
        seen.set(delivery.seq);
        handle(delivery.event, state, pending);
    });

    use_effect(move || {
        let Some((id, offset)) = *pending.read() else { return };
        pending.set(None);
        bridge.focus(id, offset);
    });

    // Cloned rather than borrowed across the loop: holding the read guard while
    // building child components risks a borrow panic if any of them touch the doc.
    let blocks = doc.read().blocks.clone();
    let markers = list_markers(&blocks);
    let guard = focus.read();

    rsx! {
        div { class: "rt-editor {class}",
            if toolbar {
                Toolbar {}
            }
            div { class: "rt-blocks",
                for (index, block) in blocks.iter().enumerate() {
                    EditableText {
                        key: "{block.id}",
                        id: block.id,
                        html: guard.html_for(block.id, &block.content),
                        class: block_class(&block.kind),
                        marker: markers[index].clone(),
                        placeholder: if index == 0 { placeholder.clone() } else { String::new() },
                    }
                }
            }
        }
    }
}

fn handle(event: RawEvent, state: EditorState, mut pending: Signal<Option<(BlockId, usize)>>) {
    let mut doc = state.doc;
    let mut focus = state.focus;
    let mut selection = state.selection;

    match event {
        RawEvent::Input { id, html: source, caret } => {
            let spans = html::html_to_spans(&source);
            let mut d = doc.write();
            let Some(index) = d.index_of(id) else { return };

            let text: String = spans.iter().map(|s| s.text.as_str()).collect();
            match keys::markdown_shortcut(&d.blocks[index].kind, &text, caret) {
                Some(shortcut) => {
                    // Drop the marker the user typed and keep the formatting of
                    // whatever followed it.
                    let (_, rest) = ops::split_spans(&spans, shortcut.strip);
                    d.blocks[index].kind = shortcut.kind;
                    d.blocks[index].content = rest;
                    d.blocks[index].normalize();
                    let updated = html::spans_to_html(&d.blocks[index].content);
                    drop(d);

                    // The block is focused, so the guard has stopped rendering it;
                    // removing the marker has to be written to the DOM directly.
                    focus.write().refreeze(updated.clone());
                    state.bridge.sync(id, &updated, 0, 0);
                }
                None => {
                    d.blocks[index].content = spans;
                    d.blocks[index].normalize();
                }
            }
        }

        RawEvent::Key { id, key, shift, caret, html: source, .. } => {
            // The event carries the block's HTML because the last `input` may not
            // have been processed yet when a key is pressed quickly after typing;
            // taking it from here means the final character is never lost.
            apply_html(doc, id, &source);

            let mut d = doc.write();
            let caret = match key.as_str() {
                "Enter" => ops::split_block(&mut d, id, caret),
                "Backspace" => ops::merge_with_previous(&mut d, id),
                "Tab" => {
                    if shift {
                        ops::outdent(&mut d, id);
                    } else {
                        ops::indent(&mut d, id);
                    }
                    // Only the element's class changes, so the caret is untouched
                    // and must not be moved.
                    None
                }
                _ => None,
            };
            drop(d);

            if let Some(caret) = caret {
                pending.set(Some((caret.block, caret.offset)));
            }
        }

        RawEvent::Mark { id, mark, start, end } => {
            let Some(mark) = mark_named(&mark) else { return };
            selection.set(Some(Selection { block: id, start, end }));
            state.toggle_mark(mark);
        }

        RawEvent::Focus { id } => {
            // Freeze what the DOM already holds, which is what the model last
            // rendered for this block.
            let spans = doc.read().block(id).map(|b| b.content.clone());
            if let Some(spans) = spans {
                focus.write().enter(id, &spans);
            }
        }

        RawEvent::Blur { id, html: source } => {
            apply_html(doc, id, &source);
            focus.write().leave(id);
        }

        RawEvent::Selection { id, start, end } => {
            selection.set(Some(Selection { block: id, start, end }));
        }
    }
}

fn apply_html(mut doc: Signal<RichDoc>, id: BlockId, source: &str) {
    let spans = html::html_to_spans(source);
    let mut d = doc.write();
    if let Some(block) = d.block_mut(id) {
        block.content = spans;
        block.normalize();
    }
}

fn mark_named(name: &str) -> Option<Marks> {
    match name {
        "bold" => Some(Marks::BOLD),
        "italic" => Some(Marks::ITALIC),
        "code" => Some(Marks::CODE),
        "strike" => Some(Marks::STRIKE),
        _ => None,
    }
}

fn block_class(kind: &BlockKind) -> String {
    match kind {
        BlockKind::Paragraph => "rt-p".into(),
        BlockKind::Heading { level } => format!("rt-h rt-h{level}"),
        BlockKind::BulletItem { indent } => format!("rt-li rt-indent-{indent}"),
        BlockKind::NumberedItem { indent } => format!("rt-li rt-indent-{indent}"),
        BlockKind::Quote => "rt-quote".into(),
        BlockKind::CodeBlock { .. } => "rt-code".into(),
    }
}

/// The visible marker for each block: a bullet, an ordinal, or nothing.
///
/// Computed here rather than left to CSS counters because the blocks are siblings
/// rather than a real `<ol>`, and because the numbering has to agree with what
/// `report_doc::markdown` writes on export — a list shown as 1, 2, 3 that exports as
/// 1, 1, 1 would be a bug the user only discovers after sending the report.
fn list_markers(blocks: &[Block]) -> Vec<String> {
    let mut out = Vec::with_capacity(blocks.len());
    // Next ordinal for each open list level, outermost first.
    let mut stack: Vec<usize> = Vec::new();

    for block in blocks {
        match &block.kind {
            BlockKind::NumberedItem { indent } => {
                // A level deeper than what is open is clamped, exactly as the
                // markdown writer clamps it.
                let depth = (*indent as usize).min(stack.len());
                if stack.len() > depth {
                    stack.truncate(depth + 1);
                } else {
                    stack.push(1);
                }
                let n = stack[depth];
                stack[depth] += 1;
                out.push(format!("{n}."));
            }
            BlockKind::BulletItem { indent } => {
                let depth = (*indent as usize).min(stack.len());
                stack.truncate(depth);
                // Occupy the level so a numbered item following a bullet run
                // restarts at one rather than continuing an earlier count.
                stack.push(1);
                out.push("•".into());
            }
            _ => {
                stack.clear();
                out.push(String::new());
            }
        }
    }
    out
}

/// Convenience for callers holding markdown rather than a document.
pub fn doc_from_markdown(source: &str) -> RichDoc {
    report_doc::markdown::from_markdown(source)
}

/// The document as markdown — what export and the model prompt both consume.
pub fn doc_to_markdown(doc: &RichDoc) -> String {
    report_doc::markdown::to_markdown(doc)
}

#[allow(dead_code)]
fn assert_span_is_used(_: &Span) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocks(spec: &[(&str, u8)]) -> Vec<Block> {
        spec.iter()
            .map(|(kind, indent)| match *kind {
                "bullet" => Block::bullet(*indent, "x"),
                "number" => Block::numbered(*indent, "x"),
                _ => Block::paragraph("x"),
            })
            .collect()
    }

    #[test]
    fn ordered_lists_are_numbered_consecutively() {
        let markers = list_markers(&blocks(&[("number", 0), ("number", 0), ("number", 0)]));
        assert_eq!(markers, ["1.", "2.", "3."]);
    }

    #[test]
    fn each_nesting_level_numbers_independently() {
        let markers =
            list_markers(&blocks(&[("number", 0), ("number", 1), ("number", 1), ("number", 0)]));
        assert_eq!(markers, ["1.", "1.", "2.", "2."]);
    }

    #[test]
    fn a_paragraph_between_lists_restarts_the_numbering() {
        let markers = list_markers(&blocks(&[("number", 0), ("para", 0), ("number", 0)]));
        assert_eq!(markers, ["1.", "", "1."]);
    }

    #[test]
    fn a_bullet_run_does_not_carry_a_number_across_it() {
        let markers = list_markers(&blocks(&[("number", 0), ("bullet", 0), ("number", 0)]));
        assert_eq!(markers, ["1.", "•", "1."]);
    }

    #[test]
    fn markers_agree_with_what_the_markdown_writer_emits() {
        // A list shown as 1, 2, 3 that exports as 1, 1, 1 is a bug the user would
        // only find after sending the report.
        let doc = RichDoc::from_blocks(blocks(&[
            ("number", 0),
            ("number", 1),
            ("number", 1),
            ("number", 0),
        ]));
        let markers = list_markers(&doc.blocks);
        let exported = report_doc::markdown::to_markdown(&doc);
        for marker in &markers {
            assert!(exported.contains(marker.as_str()), "{marker} missing from:\n{exported}");
        }
        assert_eq!(exported.matches("1. ").count(), 2, "{exported}");
        assert_eq!(exported.matches("2. ").count(), 2, "{exported}");
    }

    #[test]
    fn an_indent_gap_is_clamped_rather_than_panicking() {
        // `ops::indent` prevents this, but a document loaded from disk could hold it.
        let markers = list_markers(&blocks(&[("number", 3)]));
        assert_eq!(markers, ["1."]);
    }

    #[test]
    fn mark_names_from_the_shim_map_onto_the_model() {
        assert_eq!(mark_named("bold"), Some(Marks::BOLD));
        assert_eq!(mark_named("italic"), Some(Marks::ITALIC));
        assert_eq!(mark_named("code"), Some(Marks::CODE));
        assert_eq!(mark_named("strike"), Some(Marks::STRIKE));
        assert_eq!(mark_named("blink"), None);
    }

    #[test]
    fn block_classes_distinguish_every_kind_and_depth() {
        assert_eq!(block_class(&BlockKind::Paragraph), "rt-p");
        assert_eq!(block_class(&BlockKind::Heading { level: 3 }), "rt-h rt-h3");
        assert_eq!(block_class(&BlockKind::BulletItem { indent: 2 }), "rt-li rt-indent-2");
        assert_eq!(block_class(&BlockKind::CodeBlock { lang: None }), "rt-code");
    }
}
