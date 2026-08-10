//! Markdown ↔ [`RichDoc`].
//!
//! This is the export format and the wire format for notes handed to the model, so
//! the two directions must agree: `from_markdown(to_markdown(doc)) == doc` for every
//! document the editor can produce. The round-trip tests at the bottom of this file
//! are what hold that.
//!
//! ## Reconstructing list nesting
//!
//! Blocks are flat, carrying an `indent` (see the crate docs), but CommonMark nests
//! lists by indentation — and not by a fixed step: a nested item must be indented to
//! the *content column* of its parent, which is 2 for `- ` and 3 for `1. `. Guessing
//! a constant (say four spaces) produces documents that re-parse as code blocks or
//! as flat lists. [`Writer`] therefore tracks the real marker widths in a stack and
//! indents to their running sum, so what we emit is what we read back.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::doc::{Block, BlockKind, Marks, RichDoc, Span};

/// Render a document to CommonMark (with strikethrough).
pub fn to_markdown(doc: &RichDoc) -> String {
    let mut w = Writer::default();
    for block in &doc.blocks {
        w.block(block);
    }
    w.out.trim_end_matches('\n').to_string() + "\n"
}

/// Parse CommonMark into a document.
///
/// Anything the model cannot represent — tables, images, nested block quotes,
/// arbitrary HTML — degrades to its text rather than being dropped or rejected. A
/// report's notes are user input; refusing to parse them is never the right answer.
pub fn from_markdown(src: &str) -> RichDoc {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    Reader::default().run(Parser::new_ext(src, opts))
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

/// One open list level.
struct Level {
    /// Content-column width of the last marker written at this level (2 for `- `,
    /// 3 for `1. `, 4 for `10. `). Nested items indent to the running sum of these.
    width: usize,
    /// Ordinal for the next item at this level.
    next: usize,
    ordered: bool,
}

#[derive(Default)]
struct Writer {
    out: String,
    /// Currently open list levels, outermost first. Empty means "not in a list",
    /// which is also what resets numbering.
    stack: Vec<Level>,
}

impl Writer {
    fn block(&mut self, block: &Block) {
        match &block.kind {
            BlockKind::Heading { level } => {
                self.leave_lists();
                let level = (*level).clamp(1, BlockKind::MAX_HEADING_LEVEL) as usize;
                self.out.push_str(&"#".repeat(level));
                self.out.push(' ');
                self.inline(block);
                self.out.push_str("\n\n");
            }
            BlockKind::Paragraph => {
                self.leave_lists();
                self.inline(block);
                self.out.push_str("\n\n");
            }
            BlockKind::Quote => {
                self.leave_lists();
                self.out.push_str("> ");
                self.inline(block);
                self.out.push_str("\n\n");
            }
            BlockKind::CodeBlock { lang } => {
                self.leave_lists();
                let text = block.text();
                // A fence must be longer than any backtick run inside it, or the
                // content closes its own block.
                let fence = "`".repeat(longest_backtick_run(&text).max(2) + 1);
                self.out.push_str(&fence);
                self.out.push_str(lang.as_deref().unwrap_or(""));
                self.out.push('\n');
                self.out.push_str(&text);
                if !text.ends_with('\n') {
                    self.out.push('\n');
                }
                self.out.push_str(&fence);
                self.out.push_str("\n\n");
            }
            BlockKind::BulletItem { indent } | BlockKind::NumberedItem { indent } => {
                let ordered = matches!(block.kind, BlockKind::NumberedItem { .. });
                self.list_item(*indent, ordered, block);
            }
        }
    }

    fn list_item(&mut self, indent: u8, ordered: bool, block: &Block) {
        // A gap in indentation (an item at depth 3 with no depth-2 parent) cannot be
        // expressed, so clamp to one deeper than what is actually open. Otherwise the
        // emitted indentation would re-parse as a code block.
        let depth = (indent as usize).min(self.stack.len());

        // Indent to the content column of the enclosing items, using the marker
        // widths actually written rather than a guessed constant.
        let prefix: usize = self.stack[..depth].iter().map(|l| l.width).sum();

        let n = if self.stack.len() > depth && self.stack[depth].ordered == ordered {
            // Continuing the list already open at this depth.
            self.stack.truncate(depth + 1);
            let level = &mut self.stack[depth];
            let n = level.next;
            level.next += 1;
            n
        } else {
            // A different marker type at this depth is a different list. It needs a
            // blank line, or CommonMark reads the switch as a lazy continuation of
            // the previous item rather than as a new list.
            if self.stack.len() > depth {
                self.out.push('\n');
            }
            self.stack.truncate(depth);
            self.stack.push(Level { width: 0, next: 2, ordered });
            1
        };

        // Every level restarts at 1. That is not cosmetic: an ordered list can only
        // interrupt a paragraph when it starts with 1, so a nested list numbered
        // from anything else would be swallowed as continuation text.
        let marker = if ordered { format!("{n}. ") } else { "- ".to_string() };
        self.out.push_str(&" ".repeat(prefix));
        self.out.push_str(&marker);
        self.stack[depth].width = marker.chars().count();

        self.inline(block);
        self.out.push('\n');
    }

    /// Close any open list. A blank line here is what stops the *next* paragraph
    /// being absorbed into the last list item as a lazy continuation.
    fn leave_lists(&mut self) {
        if !self.stack.is_empty() {
            self.out.push('\n');
            self.stack.clear();
        }
    }

    fn inline(&mut self, block: &Block) {
        // Code blocks hold literal text; escaping inside one would corrupt it.
        if !block.kind.allows_marks() {
            self.out.push_str(&block.text());
            return;
        }
        // Escaping state runs across the whole block, not per span: "1" in bold
        // followed by ". text" in plain would otherwise re-parse as a list item.
        let mut esc = Escape::at_block_start();
        for span in block.content.iter() {
            if span.text.is_empty() {
                continue;
            }
            if span.marks.contains(Marks::CODE) {
                // A code span is literal, so it takes no escaping — but it does need
                // a fence longer than any backtick run inside it.
                let fence = "`".repeat(longest_backtick_run(&span.text) + 1);
                let outer = span.marks.without(Marks::CODE);
                self.open_marks(outer);
                self.out.push_str(&fence);
                // A code span starting or ending with a backtick needs padding
                // spaces, which CommonMark strips back off on the way in.
                if span.text.starts_with('`') || span.text.ends_with('`') {
                    self.out.push(' ');
                    self.out.push_str(&span.text);
                    self.out.push(' ');
                } else {
                    self.out.push_str(&span.text);
                }
                self.out.push_str(&fence);
                self.close_marks(outer);
                esc.consume_opaque();
            } else {
                self.open_marks(span.marks);
                esc.write(&span.text, &mut self.out);
                self.close_marks(span.marks);
            }
        }
    }

    fn open_marks(&mut self, marks: Marks) {
        for mark in Marks::ALL {
            if mark != Marks::CODE && marks.contains(mark) {
                self.out.push_str(delimiter(mark));
            }
        }
    }

    fn close_marks(&mut self, marks: Marks) {
        for mark in Marks::ALL.into_iter().rev() {
            if mark != Marks::CODE && marks.contains(mark) {
                self.out.push_str(delimiter(mark));
            }
        }
    }
}

/// Emphasis uses `_` rather than `*` so that bold+italic is `**_x_**` rather than
/// the notoriously ambiguous `***x***`, which parsers disagree about.
fn delimiter(mark: Marks) -> &'static str {
    match mark {
        Marks::BOLD => "**",
        Marks::ITALIC => "_",
        Marks::STRIKE => "~~",
        _ => "",
    }
}

fn longest_backtick_run(s: &str) -> usize {
    let mut best = 0;
    let mut cur = 0;
    for ch in s.chars() {
        if ch == '`' {
            cur += 1;
            best = best.max(cur);
        } else {
            cur = 0;
        }
    }
    best
}

/// Escapes text so it re-parses as itself, carrying the little bit of state that
/// block-level markers need.
///
/// Inline metacharacters are escaped everywhere. Block-level ones (`#`, `>`, `-`,
/// `1.`) only matter at the start of a block, and escaping them mid-sentence would
/// litter exported reports with backslashes in ordinary prose — dates, measurements
/// and numbered references all contain them.
struct Escape {
    /// Still at the very first character of the block.
    at_start: bool,
    /// Everything so far in this block has been an ASCII digit.
    digits_only: bool,
    saw_digit: bool,
}

impl Escape {
    fn at_block_start() -> Self {
        Self { at_start: true, digits_only: true, saw_digit: false }
    }

    /// Account for content written verbatim (a code span): it is neither the start
    /// of the block nor part of a leading digit run.
    fn consume_opaque(&mut self) {
        self.at_start = false;
        self.digits_only = false;
    }

    fn write(&mut self, text: &str, out: &mut String) {
        for ch in text.chars() {
            let needs_escape = match ch {
                '\\' | '`' | '*' | '_' | '[' | ']' | '~' | '<' => true,
                '#' | '>' | '-' | '+' if self.at_start => true,
                // "1." at the start of a block would become an ordered list. It is
                // the separator that must be escaped, never the digit: CommonMark
                // backslash escapes apply only to ASCII punctuation, so `\1` is a
                // literal backslash followed by a one.
                '.' | ')' if self.digits_only && self.saw_digit => true,
                _ => false,
            };
            if needs_escape {
                out.push('\\');
            }
            out.push(ch);
            if ch.is_ascii_digit() {
                self.saw_digit = true;
            } else {
                self.digits_only = false;
            }
            self.at_start = false;
        }
    }
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Reader {
    doc: RichDoc,
    /// Marks currently open, as a stack, so `<strong><em>` closes in the right order.
    marks: Vec<Marks>,
    /// Ordered-ness of each open list level; its length is the current indent.
    lists: Vec<bool>,
    /// The block being accumulated, if any.
    current: Option<Block>,
    /// The kind imposed by an enclosing container (a list item or a block quote).
    ///
    /// CommonMark wraps the contents of both in paragraphs, and in a *loose* list
    /// that wrapping is explicit — so without this, every list item and every quote
    /// would come back as a plain paragraph. Any further paragraphs inside the same
    /// container take the container's kind too, which is the closest this flat model
    /// can get to a multi-paragraph quote.
    container: Option<BlockKind>,
}

impl Reader {
    fn run(mut self, parser: Parser<'_>) -> RichDoc {
        for event in parser {
            self.event(event);
        }
        self.flush();
        if self.doc.blocks.is_empty() {
            return RichDoc::empty_paragraph();
        }
        self.doc.normalize();
        self.doc
    }

    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                self.flush();
                self.current = Some(Block::new(
                    BlockKind::Heading { level: heading_level(level) },
                    Vec::new(),
                ));
            }
            Event::Start(Tag::Paragraph) => match &self.container {
                // The container already opened the block this paragraph fills.
                Some(_) if self.current.is_some() => {}
                Some(kind) => self.current = Some(Block::new(kind.clone(), Vec::new())),
                None => {
                    self.flush();
                    self.current = Some(Block::new(BlockKind::Paragraph, Vec::new()));
                }
            },
            Event::Start(Tag::BlockQuote(_)) => {
                self.flush();
                self.container = Some(BlockKind::Quote);
                self.current = Some(Block::new(BlockKind::Quote, Vec::new()));
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                self.flush();
                self.container = None;
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                self.flush();
                let lang = match kind {
                    CodeBlockKind::Fenced(l) if !l.is_empty() => Some(l.to_string()),
                    _ => None,
                };
                self.current = Some(Block::new(BlockKind::CodeBlock { lang }, Vec::new()));
            }
            Event::Start(Tag::List(first)) => {
                self.flush();
                self.lists.push(first.is_some());
            }
            Event::End(TagEnd::List(_)) => {
                self.flush();
                self.lists.pop();
            }
            Event::Start(Tag::Item) => {
                self.flush();
                let indent = self.lists.len().saturating_sub(1) as u8;
                let ordered = *self.lists.last().unwrap_or(&false);
                let kind = if ordered {
                    BlockKind::NumberedItem { indent }
                } else {
                    BlockKind::BulletItem { indent }
                };
                self.current = Some(Block::new(kind.clone(), Vec::new()));
                self.container = Some(kind);
            }
            Event::End(TagEnd::Item) => {
                self.flush();
                self.container = None;
            }
            Event::End(TagEnd::Heading(_) | TagEnd::Paragraph | TagEnd::CodeBlock) => self.flush(),

            Event::Start(Tag::Strong) => self.marks.push(Marks::BOLD),
            Event::Start(Tag::Emphasis) => self.marks.push(Marks::ITALIC),
            Event::Start(Tag::Strikethrough) => self.marks.push(Marks::STRIKE),
            Event::End(TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough) => {
                self.marks.pop();
            }

            Event::Text(t) => self.push_text(&t, Marks::NONE),
            Event::Code(t) => self.push_text(&t, Marks::CODE),
            // A soft break is a wrapped source line, not a new block.
            Event::SoftBreak | Event::HardBreak => self.push_text(" ", Marks::NONE),
            // Raw HTML the model cannot represent: keep the text, drop the markup —
            // the same whitelist stance as `html::html_to_spans`.
            Event::Html(t) | Event::InlineHtml(t) => {
                let spans = crate::html::html_to_spans(&t);
                for span in spans {
                    self.push_text(&span.text, span.marks);
                }
            }
            _ => {}
        }
    }

    fn push_text(&mut self, text: &str, extra: Marks) {
        if text.is_empty() {
            return;
        }
        // Text outside any block (a table cell, say) still belongs somewhere, so
        // open a paragraph rather than dropping it.
        let block =
            self.current.get_or_insert_with(|| Block::new(BlockKind::Paragraph, Vec::new()));
        let marks = self.marks.iter().fold(extra, |acc, m| acc.with(*m));
        block.content.push(Span::new(text, marks));
    }

    fn flush(&mut self) {
        if let Some(mut block) = self.current.take() {
            block.normalize();
            // Keep empty list items and code blocks — an empty bullet is a real
            // thing the user typed — but drop the empty paragraphs that block
            // structure naturally produces.
            let keep = !block.content.is_empty()
                || block.kind.is_list_item()
                || matches!(block.kind, BlockKind::CodeBlock { .. });
            if keep {
                self.doc.blocks.push(block);
            }
        }
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(d: &RichDoc) -> Vec<(BlockKind, Vec<Span>)> {
        d.blocks.iter().map(|b| (b.kind.clone(), b.content.clone())).collect()
    }

    /// Round-trip through markdown, ignoring block ids (which are regenerated).
    ///
    /// The input is normalized first: a document straight from a constructor may
    /// hold adjacent spans with equal marks, which reading back necessarily merges.
    /// Comparing against the un-normalized form would fail on documents that are
    /// identical to the user.
    fn round_trip(blocks: Vec<Block>) {
        let mut doc = RichDoc::from_blocks(blocks);
        doc.normalize();
        let md = to_markdown(&doc);
        let back = from_markdown(&md);
        assert_eq!(strip(&back), strip(&doc), "round-trip failed for:\n{md}");
    }

    #[test]
    fn every_block_kind_round_trips() {
        round_trip(vec![
            Block::heading(1, "Site Inspection"),
            Block::paragraph("A short summary of the visit."),
            Block::heading(2, "Findings"),
            Block::bullet(0, "north wall"),
            Block::bullet(0, "south wall"),
            Block::numbered(0, "first"),
            Block::numbered(0, "second"),
            Block::new(BlockKind::Quote, vec![Span::plain("quoted remark")]),
            Block::new(
                BlockKind::CodeBlock { lang: Some("rust".into()) },
                vec![Span::plain("fn main() {}\n")],
            ),
        ]);
    }

    #[test]
    fn every_heading_level_round_trips() {
        round_trip((1..=BlockKind::MAX_HEADING_LEVEL).map(|l| Block::heading(l, "h")).collect());
    }

    #[test]
    fn every_mark_combination_round_trips() {
        // All 16 subsets of the four marks, each as its own paragraph so an escape
        // bug in one cannot be masked by its neighbour.
        for bits in 0..16u8 {
            let marks = Marks(bits);
            round_trip(vec![Block::new(
                BlockKind::Paragraph,
                vec![Span::plain("a "), Span::new("marked", marks), Span::plain(" b")],
            )]);
        }
    }

    #[test]
    fn nested_lists_round_trip_at_every_depth() {
        round_trip(vec![
            Block::bullet(0, "top"),
            Block::bullet(1, "nested"),
            Block::bullet(2, "deeper"),
            Block::bullet(1, "back up"),
            Block::bullet(0, "top again"),
        ]);
    }

    #[test]
    fn nested_ordered_lists_round_trip() {
        // The marker is three characters wide, so a constant indent step would
        // re-parse this as a flat list or as code.
        round_trip(vec![
            Block::numbered(0, "one"),
            Block::numbered(1, "one point one"),
            Block::numbered(1, "one point two"),
            Block::numbered(0, "two"),
        ]);
    }

    #[test]
    fn ordered_lists_are_numbered_consecutively() {
        let md = to_markdown(&RichDoc::from_blocks(vec![
            Block::numbered(0, "a"),
            Block::numbered(0, "b"),
            Block::numbered(0, "c"),
        ]));
        assert!(md.contains("1. a"), "{md}");
        assert!(md.contains("2. b"), "{md}");
        assert!(md.contains("3. c"), "{md}");
    }

    #[test]
    fn a_paragraph_after_a_list_is_not_absorbed_into_the_last_item() {
        round_trip(vec![
            Block::bullet(0, "item"),
            Block::paragraph("A following paragraph, not a lazy continuation."),
        ]);
    }

    #[test]
    fn markdown_metacharacters_in_prose_round_trip() {
        round_trip(vec![
            Block::paragraph("a * b _ c ` d [e] ~f~ <g> \\h"),
            Block::paragraph("# not a heading"),
            Block::paragraph("- not a bullet"),
            Block::paragraph("1. not a numbered item"),
            Block::paragraph("> not a quote"),
        ]);
    }

    #[test]
    fn numbers_in_prose_are_not_over_escaped() {
        // Escaping every digit would litter exported reports with backslashes.
        let md = to_markdown(&RichDoc::from_blocks(vec![Block::paragraph(
            "Measured 12.5 m across on 3 March, within the 2.4 m tolerance.",
        )]));
        assert!(!md.contains('\\'), "unexpected escaping in: {md}");
    }

    #[test]
    fn code_spans_containing_backticks_round_trip() {
        round_trip(vec![Block::new(
            BlockKind::Paragraph,
            vec![Span::plain("use "), Span::new("a ` b", Marks::CODE), Span::plain(" here")],
        )]);
    }

    #[test]
    fn code_blocks_containing_fences_round_trip() {
        round_trip(vec![Block::new(
            BlockKind::CodeBlock { lang: None },
            vec![Span::plain("```\nnested\n```\n")],
        )]);
    }

    #[test]
    fn code_block_content_is_never_escaped() {
        let md = to_markdown(&RichDoc::from_blocks(vec![Block::new(
            BlockKind::CodeBlock { lang: Some("rust".into()) },
            vec![Span::plain("let x = *y_z[0];\n")],
        )]));
        assert!(md.contains("let x = *y_z[0];"), "{md}");
    }

    #[test]
    fn unrepresentable_markdown_degrades_to_text_rather_than_being_dropped() {
        // Tables have no place in the model, but the words in them are the user's
        // notes and must survive into the prompt.
        let doc = from_markdown("| a | b |\n|---|---|\n| 1 | 2 |\n");
        let text: String = doc.blocks.iter().map(|b| b.text()).collect();
        for expected in ["a", "b", "1", "2"] {
            assert!(text.contains(expected), "lost {expected:?} in {text:?}");
        }
    }

    #[test]
    fn an_empty_document_parses_to_an_editable_resting_state() {
        // Zero blocks would render nothing for the user to click into.
        assert_eq!(strip(&from_markdown("")), strip(&RichDoc::empty_paragraph()));
    }

    #[test]
    fn soft_breaks_join_rather_than_splitting_a_paragraph() {
        let doc = from_markdown("one\ntwo\n");
        assert_eq!(doc.blocks.len(), 1);
        assert_eq!(doc.blocks[0].text(), "one two");
    }
}
