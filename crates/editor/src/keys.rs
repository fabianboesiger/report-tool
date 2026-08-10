//! Markdown shortcuts: typing `# ` at the start of a paragraph makes it a heading.
//!
//! Pure logic, kept apart from the component so the boundary cases are unit tests
//! rather than things to try by hand. Most of them are about *not* firing: the moment
//! this triggers on text a user meant literally, they lose their words and have no
//! obvious way to get them back.

use report_doc::BlockKind;

/// A block conversion triggered by typed text.
#[derive(Debug, Clone, PartialEq)]
pub struct Shortcut {
    pub kind: BlockKind,
    /// Characters of marker text to remove from the front of the block.
    pub strip: usize,
}

/// Decide whether the text just typed should convert the block.
///
/// Fires only when the caret sits **exactly** at the end of the marker, which is the
/// condition that keeps it honest: it means the user has just completed the marker
/// and nothing follows it yet. Pasting a document that happens to start with `- `,
/// or editing back into the start of an existing line, both leave the caret
/// elsewhere and are left alone.
pub fn markdown_shortcut(kind: &BlockKind, text: &str, caret: usize) -> Option<Shortcut> {
    // Only plain paragraphs convert. A heading or list item that already has a kind
    // was chosen deliberately, and re-converting it would fight the user.
    if !matches!(kind, BlockKind::Paragraph) {
        return None;
    }

    let chars: Vec<char> = text.chars().collect();
    // The marker must be the *whole* block: the user has just completed it and
    // nothing follows. Requiring only that the text starts with a marker would fire
    // when someone edits back into the beginning of an existing line, deleting the
    // words they already wrote — a loss with no obvious way to undo it.
    if caret == 0 || caret != chars.len() {
        return None;
    }
    let marker: String = chars.iter().collect();

    let shortcut = |kind, strip| Some(Shortcut { kind, strip });

    // Headings: one to six `#` followed by a space.
    if let Some(hashes) = marker.strip_suffix(' ') {
        if !hashes.is_empty() && hashes.chars().all(|c| c == '#') {
            let level = hashes.len() as u8;
            if level <= BlockKind::MAX_HEADING_LEVEL {
                return shortcut(BlockKind::Heading { level }, caret);
            }
            return None;
        }
    }

    match marker.as_str() {
        "- " | "* " | "+ " => shortcut(BlockKind::BulletItem { indent: 0 }, caret),
        "> " => shortcut(BlockKind::Quote, caret),
        "``` " | "```" => shortcut(BlockKind::CodeBlock { lang: None }, caret),
        _ => {
            // An ordered list: any number, then `.` or `)`, then a space.
            let digits: String = marker.chars().take_while(char::is_ascii_digit).collect();
            let rest = &marker[digits.len()..];
            if !digits.is_empty() && (rest == ". " || rest == ") ") {
                return shortcut(BlockKind::NumberedItem { indent: 0 }, caret);
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at_end(text: &str) -> Option<Shortcut> {
        markdown_shortcut(&BlockKind::Paragraph, text, text.chars().count())
    }

    #[test]
    fn heading_markers_convert_at_every_level() {
        for level in 1..=6u8 {
            let marker = format!("{} ", "#".repeat(level as usize));
            assert_eq!(
                at_end(&marker),
                Some(Shortcut { kind: BlockKind::Heading { level }, strip: level as usize + 1 })
            );
        }
    }

    #[test]
    fn seven_hashes_is_not_a_heading() {
        // Markdown has six levels; converting would produce a heading the document
        // model cannot hold.
        assert_eq!(at_end("####### "), None);
    }

    #[test]
    fn list_quote_and_code_markers_convert() {
        assert_eq!(at_end("- ").unwrap().kind, BlockKind::BulletItem { indent: 0 });
        assert_eq!(at_end("* ").unwrap().kind, BlockKind::BulletItem { indent: 0 });
        assert_eq!(at_end("1. ").unwrap().kind, BlockKind::NumberedItem { indent: 0 });
        assert_eq!(at_end("12) ").unwrap().kind, BlockKind::NumberedItem { indent: 0 });
        assert_eq!(at_end("> ").unwrap().kind, BlockKind::Quote);
        assert_eq!(at_end("```").unwrap().kind, BlockKind::CodeBlock { lang: None });
    }

    #[test]
    fn the_marker_length_is_what_gets_stripped() {
        assert_eq!(at_end("## ").unwrap().strip, 3);
        assert_eq!(at_end("1. ").unwrap().strip, 3);
    }

    #[test]
    fn nothing_fires_when_the_caret_is_not_at_the_end_of_the_marker() {
        // Editing back into the start of an existing line: the text matches, but the
        // user is not completing a marker, and converting would delete their words.
        assert_eq!(markdown_shortcut(&BlockKind::Paragraph, "- already a sentence", 2), None);
        assert_eq!(markdown_shortcut(&BlockKind::Paragraph, "# heading text", 2), None);
    }

    #[test]
    fn nothing_fires_mid_sentence() {
        assert_eq!(at_end("see item 1. "), None);
        assert_eq!(at_end("a - b"), None);
        assert_eq!(at_end("2 + 2 "), None);
    }

    #[test]
    fn a_marker_without_its_space_does_not_fire() {
        // Otherwise a hyphen could never be typed at the start of a line.
        assert_eq!(at_end("-"), None);
        assert_eq!(at_end("#"), None);
        assert_eq!(at_end("1."), None);
    }

    #[test]
    fn only_plain_paragraphs_convert() {
        // A block whose kind the user already chose must not be reinterpreted.
        assert_eq!(markdown_shortcut(&BlockKind::Heading { level: 2 }, "- ", 2), None);
        assert_eq!(markdown_shortcut(&BlockKind::BulletItem { indent: 0 }, "# ", 2), None);
        assert_eq!(markdown_shortcut(&BlockKind::CodeBlock { lang: None }, "# ", 2), None);
    }

    #[test]
    fn an_out_of_range_caret_is_ignored_rather_than_panicking() {
        // The caret arrives from the browser and can lag the text by an event.
        assert_eq!(markdown_shortcut(&BlockKind::Paragraph, "- ", 99), None);
        assert_eq!(markdown_shortcut(&BlockKind::Paragraph, "", 0), None);
    }

    #[test]
    fn multibyte_text_does_not_panic_on_the_marker_slice() {
        assert_eq!(at_end("Grüezi "), None);
        assert_eq!(at_end("— "), None);
    }
}
