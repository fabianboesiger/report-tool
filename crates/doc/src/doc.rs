//! The document types.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable identity for a block.
///
/// Load-bearing for the editor, not just bookkeeping: it is the Dioxus key, so an
/// untouched block keeps its DOM node (and therefore the caret inside it) across a
/// re-render, and it is how the focus guard recognises "the block the user is
/// currently typing in" — see `report_editor::editable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BlockId(pub Uuid);

impl BlockId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for BlockId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for BlockId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Inline formatting, as a bitset so a run can carry several at once.
///
/// A bitset rather than nested inline nodes for the same reason blocks are flat:
/// `<strong><em>x</em></strong>` and `<em><strong>x</strong></em>` are the same
/// document, and a set has no opinion about which one it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Marks(pub u8);

impl Marks {
    pub const NONE: Marks = Marks(0);
    pub const BOLD: Marks = Marks(1 << 0);
    pub const ITALIC: Marks = Marks(1 << 1);
    pub const CODE: Marks = Marks(1 << 2);
    pub const STRIKE: Marks = Marks(1 << 3);

    /// Every mark, in the order they nest when written out as HTML or markdown.
    /// Fixing the order is what makes `spans_to_html` deterministic, and therefore
    /// what makes the HTML round-trip test meaningful.
    pub const ALL: [Marks; 4] = [Marks::BOLD, Marks::ITALIC, Marks::STRIKE, Marks::CODE];

    pub fn contains(self, other: Marks) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub fn with(self, other: Marks) -> Self {
        Marks(self.0 | other.0)
    }

    #[must_use]
    pub fn without(self, other: Marks) -> Self {
        Marks(self.0 & !other.0)
    }

    #[must_use]
    pub fn toggled(self, other: Marks) -> Self {
        Marks(self.0 ^ other.0)
    }
}

impl std::ops::BitOr for Marks {
    type Output = Marks;
    fn bitor(self, rhs: Marks) -> Marks {
        Marks(self.0 | rhs.0)
    }
}

/// A run of text sharing one set of marks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub text: String,
    #[serde(default)]
    pub marks: Marks,
}

impl Span {
    pub fn plain(text: impl Into<String>) -> Self {
        Self { text: text.into(), marks: Marks::NONE }
    }

    pub fn new(text: impl Into<String>, marks: Marks) -> Self {
        Self { text: text.into(), marks }
    }
}

/// What kind of block this is. `indent` lives on the list variants rather than on
/// `Block` because it is meaningless for a paragraph or a heading, and an
/// unrepresentable state is better than a field everyone has to remember to ignore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BlockKind {
    Paragraph,
    Heading { level: u8 },
    BulletItem { indent: u8 },
    NumberedItem { indent: u8 },
    Quote,
    CodeBlock { lang: Option<String> },
}

impl BlockKind {
    /// Markdown allows six heading levels; deeper nesting in a template clamps here
    /// rather than emitting `#######`, which is not a heading at all.
    pub const MAX_HEADING_LEVEL: u8 = 6;

    /// Nesting depth for list items, `0` for everything else.
    pub fn indent(&self) -> u8 {
        match self {
            BlockKind::BulletItem { indent } | BlockKind::NumberedItem { indent } => *indent,
            _ => 0,
        }
    }

    /// The same kind at a different depth. Non-list kinds are returned unchanged,
    /// so callers can apply this blindly.
    #[must_use]
    pub fn with_indent(&self, indent: u8) -> Self {
        match self {
            BlockKind::BulletItem { .. } => BlockKind::BulletItem { indent },
            BlockKind::NumberedItem { .. } => BlockKind::NumberedItem { indent },
            other => other.clone(),
        }
    }

    pub fn is_list_item(&self) -> bool {
        matches!(self, BlockKind::BulletItem { .. } | BlockKind::NumberedItem { .. })
    }

    /// Whether the inline content is formattable. Code blocks hold literal text, so
    /// the editor must not offer marks inside one and the markdown writer must not
    /// escape anything within one.
    pub fn allows_marks(&self) -> bool {
        !matches!(self, BlockKind::CodeBlock { .. })
    }
}

/// One block: a paragraph, heading, list item, quote or code block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    #[serde(default)]
    pub id: BlockId,
    #[serde(flatten)]
    pub kind: BlockKind,
    #[serde(default)]
    pub content: Vec<Span>,
}

impl Block {
    pub fn new(kind: BlockKind, content: Vec<Span>) -> Self {
        Self { id: BlockId::new(), kind, content }
    }

    pub fn paragraph(text: impl Into<String>) -> Self {
        Self::new(BlockKind::Paragraph, vec![Span::plain(text)])
    }

    pub fn heading(level: u8, text: impl Into<String>) -> Self {
        Self::new(
            BlockKind::Heading { level: level.clamp(1, BlockKind::MAX_HEADING_LEVEL) },
            vec![Span::plain(text)],
        )
    }

    pub fn bullet(indent: u8, text: impl Into<String>) -> Self {
        Self::new(BlockKind::BulletItem { indent }, vec![Span::plain(text)])
    }

    pub fn numbered(indent: u8, text: impl Into<String>) -> Self {
        Self::new(BlockKind::NumberedItem { indent }, vec![Span::plain(text)])
    }

    /// An empty block of the same kind — what Enter at the end of a block produces.
    pub fn empty_like(&self) -> Self {
        Self::new(self.kind.clone(), Vec::new())
    }

    /// The block's text with all formatting dropped.
    pub fn text(&self) -> String {
        self.content.iter().map(|s| s.text.as_str()).collect()
    }

    pub fn char_len(&self) -> usize {
        self.content.iter().map(|s| s.text.chars().count()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.content.iter().all(|s| s.text.is_empty())
    }

    /// Merge adjacent runs sharing marks and drop empty ones.
    ///
    /// Every edit funnels through here so that two documents which *read* the same
    /// are also `==`. Without it, typing a character would split one span into two
    /// identical-marked halves and the round-trip tests would fail on documents that
    /// are indistinguishable to the user.
    pub fn normalize(&mut self) {
        let mut out: Vec<Span> = Vec::with_capacity(self.content.len());
        for span in self.content.drain(..) {
            if span.text.is_empty() {
                continue;
            }
            match out.last_mut() {
                Some(prev) if prev.marks == span.marks => prev.text.push_str(&span.text),
                _ => out.push(span),
            }
        }
        self.content = out;
    }
}

/// A whole document: an ordered list of blocks.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RichDoc {
    pub blocks: Vec<Block>,
}

impl RichDoc {
    pub fn new() -> Self {
        Self::default()
    }

    /// A document holding a single empty paragraph.
    ///
    /// The editor's resting state: a truly empty `blocks` vec would render nothing
    /// for the user to click into, so there is nowhere to start typing.
    pub fn empty_paragraph() -> Self {
        Self { blocks: vec![Block::new(BlockKind::Paragraph, Vec::new())] }
    }

    pub fn from_blocks(blocks: Vec<Block>) -> Self {
        Self { blocks }
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.iter().all(Block::is_empty)
    }

    pub fn index_of(&self, id: BlockId) -> Option<usize> {
        self.blocks.iter().position(|b| b.id == id)
    }

    pub fn block(&self, id: BlockId) -> Option<&Block> {
        self.blocks.iter().find(|b| b.id == id)
    }

    pub fn block_mut(&mut self, id: BlockId) -> Option<&mut Block> {
        self.blocks.iter_mut().find(|b| b.id == id)
    }

    pub fn normalize(&mut self) {
        for block in &mut self.blocks {
            block.normalize();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_merges_runs_with_equal_marks() {
        let mut block = Block::new(
            BlockKind::Paragraph,
            vec![
                Span::plain("Hello, "),
                Span::plain(""),
                Span::new("wor", Marks::BOLD),
                Span::new("ld", Marks::BOLD),
                Span::plain("!"),
            ],
        );
        block.normalize();
        assert_eq!(
            block.content,
            vec![Span::plain("Hello, "), Span::new("world", Marks::BOLD), Span::plain("!")]
        );
    }

    #[test]
    fn normalize_of_all_empty_spans_yields_an_empty_block() {
        let mut block = Block::new(BlockKind::Paragraph, vec![Span::plain(""), Span::plain("")]);
        block.normalize();
        assert!(block.content.is_empty());
        assert!(block.is_empty());
    }

    #[test]
    fn marks_are_a_set_not_a_nesting() {
        let bold_italic = Marks::BOLD.with(Marks::ITALIC);
        assert!(bold_italic.contains(Marks::BOLD));
        assert!(bold_italic.contains(Marks::ITALIC));
        assert!(!bold_italic.contains(Marks::CODE));
        assert_eq!(bold_italic.toggled(Marks::BOLD), Marks::ITALIC);
        assert!(bold_italic.without(Marks::BOLD).without(Marks::ITALIC).is_empty());
    }

    #[test]
    fn with_indent_leaves_non_list_kinds_alone() {
        assert_eq!(BlockKind::Paragraph.with_indent(3), BlockKind::Paragraph);
        assert_eq!(
            BlockKind::BulletItem { indent: 0 }.with_indent(2),
            BlockKind::BulletItem { indent: 2 }
        );
    }

    #[test]
    fn heading_constructor_clamps_to_the_markdown_maximum() {
        assert_eq!(Block::heading(9, "x").kind, BlockKind::Heading { level: 6 });
        assert_eq!(Block::heading(0, "x").kind, BlockKind::Heading { level: 1 });
    }
}
