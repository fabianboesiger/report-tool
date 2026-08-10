//! The `contenteditable` bridge: inline spans ↔ a deliberately tiny HTML dialect.
//!
//! This is the *only* place the browser's DOM and the Rust document model meet, and
//! it is a trust boundary in one direction. What we write out is ours; what comes
//! back has been through a `contenteditable`, which means it may contain anything
//! the user pasted — Word's `<span style=…>` soup, whole `<table>`s, `<script>`.
//!
//! So [`html_to_spans`] is a *whitelist* parser rather than a general HTML parser.
//! It understands exactly the four mark tags we emit, treats every other element as
//! transparent (keeping its text, dropping the element), and never interprets
//! attributes. Anything it does not recognise becomes plain text, which is the
//! failure mode we want: paste always produces something reasonable, and a paste can
//! never introduce structure or markup the model cannot represent.

use crate::doc::{Marks, Span};

/// The HTML tag for a mark, in nesting order (outermost first).
fn tag(mark: Marks) -> &'static str {
    match mark {
        Marks::BOLD => "strong",
        Marks::ITALIC => "em",
        Marks::STRIKE => "s",
        Marks::CODE => "code",
        _ => unreachable!("tag() is only called with the single marks in Marks::ALL"),
    }
}

/// Render spans to the HTML that goes inside a `contenteditable`.
///
/// Marks nest in the fixed order of [`Marks::ALL`], which is what makes this
/// deterministic — and therefore what makes the round-trip test meaningful.
pub fn spans_to_html(spans: &[Span]) -> String {
    let mut out = String::new();
    for span in spans {
        if span.text.is_empty() {
            continue;
        }
        let marks: Vec<Marks> =
            Marks::ALL.into_iter().filter(|m| span.marks.contains(*m)).collect();
        for mark in &marks {
            out.push('<');
            out.push_str(tag(*mark));
            out.push('>');
        }
        escape_into(&span.text, &mut out);
        for mark in marks.iter().rev() {
            out.push_str("</");
            out.push_str(tag(*mark));
            out.push('>');
        }
    }
    out
}

/// Parse the HTML a `contenteditable` produced back into spans.
///
/// Unknown elements are transparent (text kept, element dropped) and attributes are
/// ignored entirely, so pasted markup degrades to plain text instead of being
/// rejected or smuggled through.
pub fn html_to_spans(html: &str) -> Vec<Span> {
    let mut sink = Sink::default();
    let mut parser = Parser { bytes: html.as_bytes(), pos: 0 };
    while let Some(event) = parser.next_event() {
        match event {
            Event::Text(t) => sink.text(&t),
            Event::Open(name) => match mark_for(&name) {
                Some(mark) => sink.open_mark(mark),
                None if is_break(&name) => sink.line_break(),
                None => {}
            },
            Event::Close(name) => match mark_for(&name) {
                Some(mark) => sink.close_mark(mark),
                None if is_break(&name) => sink.line_break(),
                None => {}
            },
            Event::SelfClosing(name) => {
                if is_break(&name) {
                    sink.line_break();
                }
            }
        }
    }
    sink.finish()
}

/// Accumulates spans while tracking which marks are open.
#[derive(Default)]
struct Sink {
    spans: Vec<Span>,
    text: String,
    /// Only marks we recognise are pushed, so unknown elements neither open nor
    /// close formatting — exactly the "transparent" behaviour we want.
    open: Vec<Marks>,
    /// A line break was seen but not yet written.
    ///
    /// Deferred rather than written immediately so a break at the very start or very
    /// end of the fragment contributes nothing. `<div>a</div><div>b</div>` — the
    /// shape a browser produces constantly — is four break events around two words,
    /// and writing each one eagerly would yield `" a  b "`.
    pending_break: bool,
}

impl Sink {
    fn text(&mut self, t: &str) {
        if t.is_empty() {
            return;
        }
        if self.pending_break {
            self.pending_break = false;
            // Nothing before it, or whitespace already either side: no space needed.
            if !self.is_empty() && !self.ends_with_space() && !t.starts_with(char::is_whitespace) {
                self.text.push(' ');
            }
        }
        self.text.push_str(t);
    }

    fn line_break(&mut self) {
        if !self.is_empty() {
            self.pending_break = true;
        }
    }

    fn open_mark(&mut self, mark: Marks) {
        self.flush();
        self.open.push(mark);
    }

    fn close_mark(&mut self, mark: Marks) {
        self.flush();
        // Remove the innermost matching mark. A stray `</strong>` with no opener
        // finds nothing and is ignored, rather than corrupting the rest of the block.
        if let Some(i) = self.open.iter().rposition(|m| *m == mark) {
            self.open.remove(i);
        }
    }

    fn flush(&mut self) {
        if !self.text.is_empty() {
            let marks = self.open.iter().fold(Marks::NONE, |acc, m| acc.with(*m));
            self.spans.push(Span::new(std::mem::take(&mut self.text), marks));
        }
    }

    fn finish(mut self) -> Vec<Span> {
        self.flush();
        let mut block = crate::doc::Block::new(crate::doc::BlockKind::Paragraph, self.spans);
        block.normalize();
        block.content
    }

    fn is_empty(&self) -> bool {
        self.text.is_empty() && self.spans.is_empty()
    }

    fn ends_with_space(&self) -> bool {
        if !self.text.is_empty() {
            return self.text.ends_with(char::is_whitespace);
        }
        self.spans.last().is_some_and(|s| s.text.ends_with(char::is_whitespace))
    }
}

fn mark_for(name: &str) -> Option<Marks> {
    match name {
        // `b`/`i`/`strike` are what `document.execCommand` historically produced and
        // what a paste from another editor is likely to carry, so treat them as
        // synonyms rather than dropping the user's formatting on the floor.
        "strong" | "b" => Some(Marks::BOLD),
        "em" | "i" => Some(Marks::ITALIC),
        "code" | "tt" => Some(Marks::CODE),
        "s" | "strike" | "del" => Some(Marks::STRIKE),
        _ => None,
    }
}

/// Tags that imply a line break inside what must stay a single block.
fn is_break(name: &str) -> bool {
    matches!(name, "br" | "p" | "div" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
}

enum Event {
    Text(String),
    Open(String),
    Close(String),
    SelfClosing(String),
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn next_event(&mut self) -> Option<Event> {
        if self.pos >= self.bytes.len() {
            return None;
        }
        if self.bytes[self.pos] == b'<' {
            // A `<` with no closing `>` is not markup; treat the remainder as text
            // so malformed input can never swallow the rest of the block.
            match self.find(b'>') {
                Some(end) => {
                    let raw = &self.bytes[self.pos + 1..end];
                    self.pos = end + 1;
                    return Some(classify(raw));
                }
                None => {
                    let raw = &self.bytes[self.pos..];
                    self.pos = self.bytes.len();
                    return Some(Event::Text(unescape(&String::from_utf8_lossy(raw))));
                }
            }
        }
        let end = self.find(b'<').unwrap_or(self.bytes.len());
        let raw = &self.bytes[self.pos..end];
        self.pos = end;
        Some(Event::Text(unescape(&String::from_utf8_lossy(raw))))
    }

    fn find(&self, needle: u8) -> Option<usize> {
        self.bytes[self.pos..].iter().position(|b| *b == needle).map(|i| i + self.pos)
    }
}

/// Turn the inside of a `<...>` into an event, discarding attributes.
fn classify(raw: &[u8]) -> Event {
    let s = String::from_utf8_lossy(raw);
    let s = s.trim();
    // Comments and doctypes carry no text worth keeping.
    if s.starts_with('!') || s.starts_with('?') {
        return Event::Text(String::new());
    }
    if let Some(rest) = s.strip_prefix('/') {
        return Event::Close(tag_name(rest));
    }
    if let Some(rest) = s.strip_suffix('/') {
        return Event::SelfClosing(tag_name(rest));
    }
    Event::Open(tag_name(s))
}

/// The lowercased element name, with any attributes dropped.
fn tag_name(s: &str) -> String {
    s.trim()
        .split(|c: char| c.is_whitespace() || c == '/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn escape_into(text: &str, out: &mut String) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
}

fn unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        // Entities are short; a `&` that starts no entity within this window is
        // literal text, which is the common case in prose ("R&D", "Tom & Jerry").
        let end = rest[..rest.len().min(12)].find(';');
        match end.map(|e| (&rest[1..e], e)) {
            Some(("amp", e)) => {
                out.push('&');
                rest = &rest[e + 1..];
            }
            Some(("lt", e)) => {
                out.push('<');
                rest = &rest[e + 1..];
            }
            Some(("gt", e)) => {
                out.push('>');
                rest = &rest[e + 1..];
            }
            Some(("quot", e)) => {
                out.push('"');
                rest = &rest[e + 1..];
            }
            Some(("#39", e)) | Some(("apos", e)) => {
                out.push('\'');
                rest = &rest[e + 1..];
            }
            Some(("nbsp", e)) => {
                // A contenteditable inserts these constantly to keep trailing spaces
                // visible. They must become ordinary spaces or every edit slowly
                // fills the document with U+00A0.
                out.push(' ');
                rest = &rest[e + 1..];
            }
            _ => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(spans: Vec<Span>) {
        let html = spans_to_html(&spans);
        assert_eq!(html_to_spans(&html), spans, "round-trip failed for {html}");
    }

    #[test]
    fn plain_text_round_trips() {
        round_trip(vec![Span::plain("hello world")]);
    }

    #[test]
    fn every_single_mark_round_trips() {
        for mark in Marks::ALL {
            round_trip(vec![Span::new("x", mark)]);
        }
    }

    #[test]
    fn combined_marks_round_trip() {
        round_trip(vec![
            Span::plain("a "),
            Span::new("b", Marks::BOLD.with(Marks::ITALIC)),
            Span::plain(" c "),
            Span::new("d", Marks::BOLD.with(Marks::ITALIC).with(Marks::STRIKE).with(Marks::CODE)),
        ]);
    }

    #[test]
    fn html_metacharacters_survive_escaping() {
        round_trip(vec![Span::plain("a < b && c > d \"quoted\"")]);
    }

    #[test]
    fn unknown_elements_are_transparent_not_propagated() {
        // The paste case: markup we do not model must lose the element and keep the
        // text, never smuggle a tag or an attribute into the document.
        let spans = html_to_spans(
            r#"<span style="color:red" class="x">red</span> and <script>alert(1)</script>"#,
        );
        assert_eq!(spans, vec![Span::plain("red and alert(1)")]);
    }

    #[test]
    fn legacy_and_synonym_tags_map_onto_our_marks() {
        assert_eq!(
            html_to_spans("<b>a</b><i>b</i><strike>c</strike>"),
            vec![
                Span::new("a", Marks::BOLD),
                Span::new("b", Marks::ITALIC),
                Span::new("c", Marks::STRIKE),
            ]
        );
    }

    #[test]
    fn contenteditable_nbsp_becomes_an_ordinary_space() {
        // Left unhandled, every edit would slowly fill the document with U+00A0.
        assert_eq!(html_to_spans("a&nbsp;b"), vec![Span::plain("a b")]);
    }

    #[test]
    fn line_breaks_inside_a_block_collapse_to_a_space() {
        assert_eq!(html_to_spans("a<br>b"), vec![Span::plain("a b")]);
        // The shape a browser actually produces: breaks at the edges must not leave
        // stray leading or trailing spaces, and the pair between the words is one.
        assert_eq!(html_to_spans("<div>a</div><div>b</div>"), vec![Span::plain("a b")]);
        assert_eq!(html_to_spans("<br>a<br>"), vec![Span::plain("a")]);
        // An existing space is not doubled.
        assert_eq!(html_to_spans("a <br> b"), vec![Span::plain("a  b")]);
    }

    #[test]
    fn malformed_markup_degrades_to_text_instead_of_swallowing_the_block() {
        // An unclosed `<` must not eat the rest of the line.
        assert_eq!(html_to_spans("a < b"), vec![Span::plain("a < b")]);
        // A stray close tag finds no opener and is simply ignored.
        assert_eq!(html_to_spans("a</strong>b"), vec![Span::plain("ab")]);
        // An unclosed open tag still applies to what follows.
        assert_eq!(html_to_spans("<strong>a"), vec![Span::new("a", Marks::BOLD)]);
    }

    #[test]
    fn adjacent_equal_marks_are_normalized_so_equality_matches_appearance() {
        assert_eq!(
            html_to_spans("<strong>a</strong><strong>b</strong>"),
            vec![Span::new("ab", Marks::BOLD)]
        );
    }

    #[test]
    fn ampersands_in_prose_are_not_mistaken_for_entities() {
        assert_eq!(html_to_spans("R&D and Tom & Jerry"), vec![Span::plain("R&D and Tom & Jerry")]);
    }
}
