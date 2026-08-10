//! The editor's structural edits, as pure functions on the document.
//!
//! These live here rather than in `report-editor` on purpose. Enter-splits-a-block,
//! Backspace-merges-into-the-previous-one and Tab-indents are the semantics users
//! actually judge an editor by, and they are full of boundary cases: splitting
//! exactly on a mark boundary, merging a heading into a list item, outdenting the
//! first item of a sublist. Keeping them as `&mut RichDoc` transforms means every
//! one of those cases is a unit test that runs in milliseconds without a webview.
//!
//! Offsets are in **characters**, not bytes — they come from the browser's
//! `Selection` API, which counts UTF-16 code units within a text node but is
//! converted to character offsets by the JS shim before it crosses into Rust. Using
//! byte offsets here would panic on any non-ASCII document.

use crate::doc::{Block, BlockId, BlockKind, Marks, RichDoc, Span};

/// Where the caret should land after an edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caret {
    pub block: BlockId,
    /// Character offset from the start of the block's text.
    pub offset: usize,
}

impl Caret {
    pub fn new(block: BlockId, offset: usize) -> Self {
        Self { block, offset }
    }
}

/// Split `block` at `offset`, moving the remainder into a new block after it.
///
/// The new block's kind follows the old one, except that splitting a heading
/// produces a paragraph — pressing Enter at the end of a title starts body text, not
/// a second title, which is what every editor does and what users expect.
///
/// Returns where the caret goes: the start of the new block.
pub fn split_block(doc: &mut RichDoc, id: BlockId, offset: usize) -> Option<Caret> {
    let index = doc.index_of(id)?;
    let kind = doc.blocks[index].kind.clone();

    // Enter on an empty list item leaves the list rather than adding another empty
    // one — the standard way out of a list without reaching for the mouse. A nested
    // item steps out one level at a time.
    if kind.is_list_item() && doc.blocks[index].is_empty() {
        doc.blocks[index].kind = match kind.indent() {
            0 => BlockKind::Paragraph,
            n => kind.with_indent(n - 1),
        };
        return Some(Caret::new(id, 0));
    }

    let (left, right) = split_spans(&doc.blocks[index].content, offset);
    doc.blocks[index].content = left;
    doc.blocks[index].normalize();

    // A heading continues as body text: pressing Enter at the end of a title starts
    // a paragraph, not a second title.
    let new_kind = match kind {
        BlockKind::Heading { .. } => BlockKind::Paragraph,
        other => other,
    };
    let mut new = Block::new(new_kind, right);
    new.normalize();
    let new_id = new.id;
    doc.blocks.insert(index + 1, new);
    Some(Caret::new(new_id, 0))
}

/// Merge `block` into its predecessor — what Backspace at offset 0 does.
///
/// Two things happen before a merge is even considered, because they are what the
/// user means far more often:
///
/// 1. A list item with indent > 0 outdents instead.
/// 2. A non-paragraph block at indent 0 becomes a paragraph instead. Backspace at
///    the start of a heading should clear the heading, not silently glue its text
///    onto the end of the paragraph above.
///
/// Only a paragraph at the very start actually merges. Returns the caret position,
/// which is the join point.
pub fn merge_with_previous(doc: &mut RichDoc, id: BlockId) -> Option<Caret> {
    let index = doc.index_of(id)?;
    let kind = doc.blocks[index].kind.clone();

    if kind.is_list_item() && kind.indent() > 0 {
        doc.blocks[index].kind = kind.with_indent(kind.indent() - 1);
        return Some(Caret::new(id, 0));
    }
    if !matches!(kind, BlockKind::Paragraph) {
        doc.blocks[index].kind = BlockKind::Paragraph;
        return Some(Caret::new(id, 0));
    }
    // Nothing above to merge into.
    if index == 0 {
        return Some(Caret::new(id, 0));
    }

    let moved = doc.blocks.remove(index);
    let prev = &mut doc.blocks[index - 1];
    // A code block holds literal text; appending a paragraph's spans into it would
    // silently turn prose into code.
    if matches!(prev.kind, BlockKind::CodeBlock { .. }) {
        doc.blocks.insert(index, moved);
        return Some(Caret::new(id, 0));
    }
    let offset = prev.char_len();
    prev.content.extend(moved.content);
    prev.normalize();
    Some(Caret::new(prev.id, offset))
}

/// Nest a list item one level deeper (Tab).
///
/// An item can only ever be one level deeper than the item above it: a bullet with
/// no parent at the shallower depth is not expressible in markdown, so the clamp
/// here is what keeps [`crate::markdown`] from having to invent a parent later.
/// Non-list blocks are left alone, and so is the first item in a list.
pub fn indent(doc: &mut RichDoc, id: BlockId) -> bool {
    let Some(index) = doc.index_of(id) else { return false };
    if !doc.blocks[index].kind.is_list_item() {
        return false;
    }
    let current = doc.blocks[index].kind.indent();
    let max = match index.checked_sub(1).map(|i| &doc.blocks[i]) {
        Some(prev) if prev.kind.is_list_item() => prev.kind.indent() + 1,
        // First item of a list: there is nothing to nest under.
        _ => 0,
    };
    if current >= max {
        return false;
    }
    doc.blocks[index].kind = doc.blocks[index].kind.with_indent(current + 1);
    true
}

/// Move a list item one level out (Shift-Tab).
///
/// Any children of the moved item must follow it, or they would be left orphaned at
/// a depth with no parent — the exact state [`indent`] refuses to create.
pub fn outdent(doc: &mut RichDoc, id: BlockId) -> bool {
    let Some(index) = doc.index_of(id) else { return false };
    if !doc.blocks[index].kind.is_list_item() {
        return false;
    }
    let current = doc.blocks[index].kind.indent();
    if current == 0 {
        return false;
    }
    doc.blocks[index].kind = doc.blocks[index].kind.with_indent(current - 1);

    for block in doc.blocks[index + 1..].iter_mut() {
        if !block.kind.is_list_item() || block.kind.indent() <= current {
            break;
        }
        block.kind = block.kind.with_indent(block.kind.indent() - 1);
    }
    true
}

/// Change a block's kind, preserving its text (the markdown shortcuts and the
/// toolbar's heading/list buttons).
pub fn set_kind(doc: &mut RichDoc, id: BlockId, kind: BlockKind) -> bool {
    let Some(block) = doc.block_mut(id) else { return false };
    // Text that was literal inside a code block becomes formattable prose; the
    // spans carry no marks either way, so only the kind changes.
    block.kind = kind;
    true
}

/// Toggle `mark` over the character range `[start, end)` of a block.
///
/// Follows the convention every editor uses: if the whole selection already has the
/// mark, remove it; otherwise apply it to all of it. Toggling per-span instead would
/// make a partly-bold selection flip to its inverse, which reads as a bug.
///
/// Returns whether the mark ended up applied.
pub fn toggle_mark_in_range(
    doc: &mut RichDoc,
    id: BlockId,
    start: usize,
    end: usize,
    mark: Marks,
) -> bool {
    let Some(block) = doc.block_mut(id) else { return false };
    if !block.kind.allows_marks() || start >= end {
        return false;
    }

    let (before, rest) = split_spans(&block.content, start);
    let (middle, after) = split_spans(&rest, end.saturating_sub(start));
    let all_marked = !middle.is_empty() && middle.iter().all(|s| s.marks.contains(mark));

    let middle: Vec<Span> = middle
        .into_iter()
        .map(|s| {
            Span::new(s.text, if all_marked { s.marks.without(mark) } else { s.marks.with(mark) })
        })
        .collect();

    block.content = before.into_iter().chain(middle).chain(after).collect();
    block.normalize();
    !all_marked
}

/// Split a span list at a character offset.
///
/// The single primitive under split, merge and mark toggling — which is why the
/// mark-boundary cases only have to be right once. Public because the editor's
/// markdown shortcuts need it too, to drop a `# ` marker the user just typed while
/// preserving the formatting of everything after it.
pub fn split_spans(spans: &[Span], offset: usize) -> (Vec<Span>, Vec<Span>) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut seen = 0usize;

    for span in spans {
        let len = span.text.chars().count();
        if seen >= offset {
            right.push(span.clone());
        } else if seen + len <= offset {
            left.push(span.clone());
        } else {
            // The split lands inside this span. Slice by character index, since a
            // byte index would panic on any multi-byte character.
            let cut = offset - seen;
            let byte = span.text.char_indices().nth(cut).map(|(i, _)| i).unwrap_or(span.text.len());
            left.push(Span::new(&span.text[..byte], span.marks));
            right.push(Span::new(&span.text[byte..], span.marks));
        }
        seen += len;
    }
    (left, right)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(blocks: Vec<Block>) -> RichDoc {
        RichDoc::from_blocks(blocks)
    }

    fn texts(doc: &RichDoc) -> Vec<String> {
        doc.blocks.iter().map(Block::text).collect()
    }

    fn kinds(doc: &RichDoc) -> Vec<BlockKind> {
        doc.blocks.iter().map(|b| b.kind.clone()).collect()
    }

    #[test]
    fn split_divides_text_and_places_the_caret_in_the_new_block() {
        let mut d = doc(vec![Block::paragraph("hello world")]);
        let id = d.blocks[0].id;
        let caret = split_block(&mut d, id, 5).unwrap();
        assert_eq!(texts(&d), ["hello", " world"]);
        assert_eq!(caret, Caret::new(d.blocks[1].id, 0));
    }

    #[test]
    fn split_at_the_ends_produces_an_empty_half() {
        let mut d = doc(vec![Block::paragraph("abc")]);
        let id = d.blocks[0].id;
        split_block(&mut d, id, 0).unwrap();
        assert_eq!(texts(&d), ["", "abc"]);

        let mut d = doc(vec![Block::paragraph("abc")]);
        let id = d.blocks[0].id;
        split_block(&mut d, id, 3).unwrap();
        assert_eq!(texts(&d), ["abc", ""]);
    }

    #[test]
    fn split_preserves_marks_on_both_sides_of_the_cut() {
        let mut d =
            doc(vec![Block::new(BlockKind::Paragraph, vec![Span::new("bolded", Marks::BOLD)])]);
        let id = d.blocks[0].id;
        split_block(&mut d, id, 3).unwrap();
        assert_eq!(d.blocks[0].content, vec![Span::new("bol", Marks::BOLD)]);
        assert_eq!(d.blocks[1].content, vec![Span::new("ded", Marks::BOLD)]);
    }

    #[test]
    fn split_on_a_multibyte_character_boundary_does_not_panic() {
        let mut d = doc(vec![Block::paragraph("Grüezi wohl")]);
        let id = d.blocks[0].id;
        split_block(&mut d, id, 3).unwrap();
        assert_eq!(texts(&d), ["Grü", "ezi wohl"]);
    }

    #[test]
    fn enter_at_the_end_of_a_heading_starts_body_text() {
        let mut d = doc(vec![Block::heading(2, "Title")]);
        let id = d.blocks[0].id;
        split_block(&mut d, id, 5).unwrap();
        assert_eq!(kinds(&d), [BlockKind::Heading { level: 2 }, BlockKind::Paragraph]);
    }

    #[test]
    fn enter_on_an_empty_bullet_leaves_the_list() {
        let mut d = doc(vec![Block::bullet(0, "item"), Block::bullet(0, "")]);
        let id = d.blocks[1].id;
        split_block(&mut d, id, 0).unwrap();
        assert_eq!(kinds(&d)[1], BlockKind::Paragraph);
        assert_eq!(d.blocks.len(), 2, "no extra empty block should appear");
    }

    #[test]
    fn enter_on_an_empty_nested_bullet_steps_out_one_level_first() {
        let mut d = doc(vec![Block::bullet(0, "top"), Block::bullet(1, "")]);
        let id = d.blocks[1].id;
        split_block(&mut d, id, 0).unwrap();
        assert_eq!(kinds(&d)[1], BlockKind::BulletItem { indent: 0 });
        assert_eq!(d.blocks.len(), 2);
    }

    #[test]
    fn merge_joins_a_paragraph_onto_the_previous_one_at_the_join_point() {
        let mut d = doc(vec![Block::paragraph("hello"), Block::paragraph(" world")]);
        let second = d.blocks[1].id;
        let caret = merge_with_previous(&mut d, second).unwrap();
        assert_eq!(texts(&d), ["hello world"]);
        assert_eq!(caret, Caret::new(d.blocks[0].id, 5));
    }

    #[test]
    fn backspace_at_the_start_of_a_heading_clears_the_heading_rather_than_merging() {
        let mut d = doc(vec![Block::paragraph("intro"), Block::heading(2, "Title")]);
        let id = d.blocks[1].id;
        merge_with_previous(&mut d, id).unwrap();
        assert_eq!(texts(&d), ["intro", "Title"], "the text must not have moved");
        assert_eq!(kinds(&d)[1], BlockKind::Paragraph);
    }

    #[test]
    fn backspace_in_a_nested_bullet_outdents_before_it_merges() {
        let mut d = doc(vec![Block::bullet(0, "top"), Block::bullet(1, "nested")]);
        let id = d.blocks[1].id;
        merge_with_previous(&mut d, id).unwrap();
        assert_eq!(kinds(&d)[1], BlockKind::BulletItem { indent: 0 });
        assert_eq!(d.blocks.len(), 2);
    }

    #[test]
    fn merge_into_a_code_block_is_refused_so_prose_never_becomes_code() {
        let mut d = doc(vec![
            Block::new(BlockKind::CodeBlock { lang: None }, vec![Span::plain("fn main() {}")]),
            Block::paragraph("prose"),
        ]);
        let id = d.blocks[1].id;
        merge_with_previous(&mut d, id).unwrap();
        assert_eq!(texts(&d), ["fn main() {}", "prose"]);
    }

    #[test]
    fn merge_at_the_top_of_the_document_is_a_no_op() {
        let mut d = doc(vec![Block::paragraph("only")]);
        let id = d.blocks[0].id;
        merge_with_previous(&mut d, id).unwrap();
        assert_eq!(texts(&d), ["only"]);
    }

    #[test]
    fn indent_is_clamped_to_one_level_below_the_item_above() {
        let mut d = doc(vec![Block::bullet(0, "top"), Block::bullet(0, "second")]);
        let id = d.blocks[1].id;
        assert!(indent(&mut d, id));
        assert_eq!(d.blocks[1].kind.indent(), 1);
        // A second Tab would create a depth-2 item under a depth-0 parent, which
        // markdown cannot express.
        assert!(!indent(&mut d, id));
        assert_eq!(d.blocks[1].kind.indent(), 1);
    }

    #[test]
    fn the_first_item_of_a_list_cannot_be_indented() {
        let mut d = doc(vec![Block::paragraph("intro"), Block::bullet(0, "first")]);
        let id = d.blocks[1].id;
        assert!(!indent(&mut d, id));
        assert_eq!(d.blocks[1].kind.indent(), 0);
    }

    #[test]
    fn indent_ignores_blocks_that_are_not_list_items() {
        let mut d = doc(vec![Block::bullet(0, "a"), Block::paragraph("b")]);
        let id = d.blocks[1].id;
        assert!(!indent(&mut d, id));
        assert!(!outdent(&mut d, id));
    }

    #[test]
    fn outdent_carries_children_along_so_none_are_orphaned() {
        let mut d = doc(vec![
            Block::bullet(0, "top"),
            Block::bullet(1, "middle"),
            Block::bullet(2, "child"),
            Block::bullet(0, "unrelated"),
        ]);
        let id = d.blocks[1].id;
        assert!(outdent(&mut d, id));
        assert_eq!(
            d.blocks.iter().map(|b| b.kind.indent()).collect::<Vec<_>>(),
            [0, 0, 1, 0],
            "the child must follow its parent out"
        );
    }

    #[test]
    fn outdent_at_the_top_level_is_a_no_op() {
        let mut d = doc(vec![Block::bullet(0, "top")]);
        let id = d.blocks[0].id;
        assert!(!outdent(&mut d, id));
    }

    #[test]
    fn toggling_a_mark_applies_it_to_exactly_the_selected_range() {
        let mut d = doc(vec![Block::paragraph("hello world")]);
        let id = d.blocks[0].id;
        assert!(toggle_mark_in_range(&mut d, id, 6, 11, Marks::BOLD));
        assert_eq!(
            d.blocks[0].content,
            vec![Span::plain("hello "), Span::new("world", Marks::BOLD),]
        );
    }

    #[test]
    fn toggling_a_fully_marked_range_removes_the_mark() {
        let mut d =
            doc(vec![Block::new(BlockKind::Paragraph, vec![Span::new("hello", Marks::BOLD)])]);
        let id = d.blocks[0].id;
        assert!(!toggle_mark_in_range(&mut d, id, 0, 5, Marks::BOLD));
        assert_eq!(d.blocks[0].content, vec![Span::plain("hello")]);
    }

    #[test]
    fn toggling_a_partly_marked_range_applies_rather_than_inverting() {
        // Inverting would read as a bug: the user asked for bold, and half the
        // selection would come back unbolded.
        let mut d = doc(vec![Block::new(
            BlockKind::Paragraph,
            vec![Span::new("bold", Marks::BOLD), Span::plain("plain")],
        )]);
        let id = d.blocks[0].id;
        assert!(toggle_mark_in_range(&mut d, id, 0, 9, Marks::BOLD));
        assert_eq!(d.blocks[0].content, vec![Span::new("boldplain", Marks::BOLD)]);
    }

    #[test]
    fn toggling_across_a_span_boundary_preserves_the_other_marks() {
        let mut d = doc(vec![Block::new(
            BlockKind::Paragraph,
            vec![Span::new("ab", Marks::ITALIC), Span::plain("cd")],
        )]);
        let id = d.blocks[0].id;
        toggle_mark_in_range(&mut d, id, 1, 3, Marks::BOLD);
        assert_eq!(
            d.blocks[0].content,
            vec![
                Span::new("a", Marks::ITALIC),
                Span::new("b", Marks::ITALIC.with(Marks::BOLD)),
                Span::new("c", Marks::BOLD),
                Span::plain("d"),
            ]
        );
    }

    #[test]
    fn marks_are_refused_inside_a_code_block() {
        let mut d = doc(vec![Block::new(
            BlockKind::CodeBlock { lang: None },
            vec![Span::plain("literal")],
        )]);
        let id = d.blocks[0].id;
        assert!(!toggle_mark_in_range(&mut d, id, 0, 7, Marks::BOLD));
        assert_eq!(d.blocks[0].content, vec![Span::plain("literal")]);
    }

    #[test]
    fn an_empty_range_changes_nothing() {
        let mut d = doc(vec![Block::paragraph("abc")]);
        let id = d.blocks[0].id;
        assert!(!toggle_mark_in_range(&mut d, id, 2, 2, Marks::BOLD));
        assert_eq!(d.blocks[0].content, vec![Span::plain("abc")]);
    }
}
