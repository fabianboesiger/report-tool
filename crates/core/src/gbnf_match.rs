//! A small GBNF parser and matcher, for tests only.
//!
//! [`crate::compile::Shape::to_gbnf`] emits the grammar that constrains local
//! generation, and nothing else in the build ever reads it back: llama.cpp parses it
//! at runtime, inside a process that also needs a multi-gigabyte model loaded. That
//! leaves the emitter's escaping and repetition arithmetic — exactly the fiddly
//! parts — checked only by eventual, expensive, manual testing.
//!
//! So the tests carry their own matcher. It understands the subset of GBNF the
//! emitter produces (literals, character classes, references, groups, `*`, `?`,
//! `+` and `{m,n}`) and answers one question: does this grammar accept this string?
//! Pointing it at `serde_json::to_string(&sample)` proves the emitted text really
//! does describe the same JSON the schema describes.
//!
//! Deliberately not a general GBNF implementation, and deliberately not a
//! dependency: the one crate that could do this vendors its own copy of llama.cpp,
//! which is a large build and a second copy of ggml for a test helper.

use std::collections::BTreeSet;
use std::collections::HashMap;

#[derive(Debug, Clone)]
enum Atom {
    Literal(Vec<char>),
    Class { negated: bool, items: Vec<ClassItem> },
    Reference(String),
    Group(Alternatives),
}

#[derive(Debug, Clone)]
enum ClassItem {
    Char(char),
    Range(char, char),
}

#[derive(Debug, Clone)]
struct Term {
    atom: Atom,
    min: u32,
    max: Option<u32>,
}

type Sequence = Vec<Term>;
type Alternatives = Vec<Sequence>;

pub struct Grammar {
    rules: HashMap<String, Alternatives>,
}

impl Grammar {
    /// Parse a grammar. Panics with a message on malformed input — this is test
    /// support, and a grammar our own emitter cannot produce is a bug, not a case to
    /// handle gracefully.
    pub fn parse(src: &str) -> Grammar {
        let mut rules = HashMap::new();
        // A rule body may wrap across lines, so join first and split on the `::=`
        // that starts each new rule.
        for (name, body) in split_rules(src) {
            let mut p = P { chars: body.chars().collect(), pos: 0 };
            let alts = p.alternatives();
            p.skip_ws();
            assert!(p.done(), "trailing input in rule {name}: {:?}", p.rest());
            rules.insert(name, alts);
        }
        assert!(rules.contains_key("root"), "grammar has no root rule");
        Grammar { rules }
    }

    /// Whether the grammar accepts `input` in its entirety.
    pub fn accepts(&self, input: &str) -> bool {
        let chars: Vec<char> = input.chars().collect();
        let root = &self.rules["root"];
        self.alts(root, &chars, 0).contains(&chars.len())
    }

    fn alts(&self, alts: &Alternatives, input: &[char], pos: usize) -> BTreeSet<usize> {
        let mut out = BTreeSet::new();
        for seq in alts {
            out.extend(self.seq(seq, input, pos));
        }
        out
    }

    fn seq(&self, seq: &Sequence, input: &[char], pos: usize) -> BTreeSet<usize> {
        let mut positions = BTreeSet::from([pos]);
        for term in seq {
            let mut next = BTreeSet::new();
            for p in &positions {
                next.extend(self.term(term, input, *p));
            }
            if next.is_empty() {
                return BTreeSet::new();
            }
            positions = next;
        }
        positions
    }

    fn term(&self, term: &Term, input: &[char], pos: usize) -> BTreeSet<usize> {
        let mut out = BTreeSet::new();
        if term.min == 0 {
            out.insert(pos);
        }
        let mut frontier = BTreeSet::from([pos]);
        // An unbounded repetition cannot usefully outrun the input.
        let ceiling = term.max.unwrap_or((input.len() - pos.min(input.len())) as u32 + 1);
        for n in 1..=ceiling {
            let mut next = BTreeSet::new();
            for p in &frontier {
                next.extend(self.atom(&term.atom, input, *p));
            }
            // A repetition whose body can match nothing would otherwise spin here.
            if next.is_empty() || next == frontier {
                if next == frontier && n >= term.min {
                    out.extend(next);
                }
                break;
            }
            if n >= term.min {
                out.extend(next.iter().copied());
            }
            frontier = next;
        }
        out
    }

    fn atom(&self, atom: &Atom, input: &[char], pos: usize) -> BTreeSet<usize> {
        match atom {
            Atom::Literal(lit) => {
                if input[pos.min(input.len())..].starts_with(lit.as_slice()) {
                    BTreeSet::from([pos + lit.len()])
                } else {
                    BTreeSet::new()
                }
            }
            Atom::Class { negated, items } => {
                let Some(ch) = input.get(pos) else { return BTreeSet::new() };
                let hit = items.iter().any(|item| match item {
                    ClassItem::Char(c) => c == ch,
                    ClassItem::Range(a, b) => a <= ch && ch <= b,
                });
                if hit != *negated {
                    BTreeSet::from([pos + 1])
                } else {
                    BTreeSet::new()
                }
            }
            Atom::Reference(name) => {
                let rule = self.rules.get(name).unwrap_or_else(|| panic!("undefined rule {name}"));
                self.alts(rule, input, pos)
            }
            Atom::Group(alts) => self.alts(alts, input, pos),
        }
    }
}

/// Split the source into `(name, body)` pairs.
///
/// Bodies may span lines, so a new rule is recognised by an identifier followed by
/// `::=` at the start of a line rather than by the line break alone.
fn split_rules(src: &str) -> Vec<(String, String)> {
    let mut rules: Vec<(String, String)> = Vec::new();
    for line in src.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        match trimmed.split_once("::=") {
            Some((name, body)) if is_identifier(name.trim()) => {
                rules.push((name.trim().to_string(), body.trim().to_string()));
            }
            // A continuation of the previous rule.
            _ => {
                let last = rules.last_mut().expect("input starts with a rule body");
                last.1.push(' ');
                last.1.push_str(trimmed);
            }
        }
    }
    rules
}

fn is_identifier(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

struct P {
    chars: Vec<char>,
    pos: usize,
}

impl P {
    fn done(&self) -> bool {
        self.pos >= self.chars.len()
    }

    fn rest(&self) -> String {
        self.chars[self.pos.min(self.chars.len())..].iter().collect()
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.pos += 1;
        }
    }

    fn alternatives(&mut self) -> Alternatives {
        let mut alts = vec![self.sequence()];
        loop {
            self.skip_ws();
            if self.peek() == Some('|') {
                self.pos += 1;
                alts.push(self.sequence());
            } else {
                return alts;
            }
        }
    }

    fn sequence(&mut self) -> Sequence {
        let mut terms = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                None | Some('|') | Some(')') => return terms,
                _ => terms.push(self.term()),
            }
        }
    }

    fn term(&mut self) -> Term {
        let atom = self.atom();
        let (min, max) = match self.peek() {
            Some('*') => {
                self.pos += 1;
                (0, None)
            }
            Some('+') => {
                self.pos += 1;
                (1, None)
            }
            Some('?') => {
                self.pos += 1;
                (0, Some(1))
            }
            Some('{') => {
                self.pos += 1;
                let min = self.number();
                let max = if self.peek() == Some(',') {
                    self.pos += 1;
                    if self.peek() == Some('}') {
                        None
                    } else {
                        Some(self.number())
                    }
                } else {
                    Some(min)
                };
                assert_eq!(self.bump(), Some('}'), "unterminated repetition");
                (min, max)
            }
            _ => (1, Some(1)),
        };
        Term { atom, min, max }
    }

    fn number(&mut self) -> u32 {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.pos += 1;
        }
        self.chars[start..self.pos].iter().collect::<String>().parse().expect("a number")
    }

    fn atom(&mut self) -> Atom {
        match self.peek() {
            Some('"') => {
                self.pos += 1;
                let mut lit = Vec::new();
                loop {
                    match self.bump().expect("unterminated literal") {
                        '"' => return Atom::Literal(lit),
                        '\\' => lit.push(self.escape()),
                        ch => lit.push(ch),
                    }
                }
            }
            Some('[') => {
                self.pos += 1;
                let negated = if self.peek() == Some('^') {
                    self.pos += 1;
                    true
                } else {
                    false
                };
                let mut items = Vec::new();
                loop {
                    let ch = match self.bump().expect("unterminated character class") {
                        ']' => return Atom::Class { negated, items },
                        '\\' => self.escape(),
                        ch => ch,
                    };
                    // A `-` before the closing bracket is a literal dash.
                    if self.peek() == Some('-') && self.chars.get(self.pos + 1) != Some(&']') {
                        self.pos += 1;
                        let end = match self.bump().expect("unterminated range") {
                            '\\' => self.escape(),
                            c => c,
                        };
                        items.push(ClassItem::Range(ch, end));
                    } else {
                        items.push(ClassItem::Char(ch));
                    }
                }
            }
            Some('(') => {
                self.pos += 1;
                let alts = self.alternatives();
                self.skip_ws();
                assert_eq!(self.bump(), Some(')'), "unterminated group");
                Atom::Group(alts)
            }
            Some(c) if c.is_ascii_alphanumeric() => {
                let start = self.pos;
                while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == '-' || c == '_')
                {
                    self.pos += 1;
                }
                Atom::Reference(self.chars[start..self.pos].iter().collect())
            }
            other => panic!("unexpected {other:?} at {:?}", self.rest()),
        }
    }

    fn escape(&mut self) -> char {
        match self.bump().expect("dangling escape") {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            'x' => self.hex(2),
            'u' => self.hex(4),
            ch => ch,
        }
    }

    fn hex(&mut self, digits: usize) -> char {
        let start = self.pos;
        self.pos += digits;
        let text: String = self.chars[start..self.pos].iter().collect();
        char::from_u32(u32::from_str_radix(&text, 16).expect("hex escape")).expect("a character")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The matcher is itself test infrastructure, so it gets tested: a false
    /// "accepts everything" would silently disarm every grammar test that uses it.
    #[test]
    fn literals_alternation_and_groups() {
        let g = Grammar::parse("root ::= \"a\" ( \"b\" | \"c\" ) \"d\"\n");
        assert!(g.accepts("abd"));
        assert!(g.accepts("acd"));
        assert!(!g.accepts("ad"));
        assert!(!g.accepts("abcd"));
        assert!(!g.accepts("ab"));
    }

    #[test]
    fn repetition_bounds_are_honoured() {
        let g = Grammar::parse("root ::= \"a\"{2,4}\n");
        assert!(!g.accepts("a"));
        assert!(g.accepts("aa"));
        assert!(g.accepts("aaaa"));
        assert!(!g.accepts("aaaaa"));

        let g = Grammar::parse("root ::= \"a\"{2,}\n");
        assert!(!g.accepts("a"));
        assert!(g.accepts("aaaaaaa"));

        let g = Grammar::parse("root ::= \"a\"* \"b\"\n");
        assert!(g.accepts("b"));
        assert!(g.accepts("aaab"));

        let g = Grammar::parse("root ::= \"a\"? \"b\"\n");
        assert!(g.accepts("b"));
        assert!(g.accepts("ab"));
        assert!(!g.accepts("aab"));
    }

    #[test]
    fn character_classes_including_negation_and_escapes() {
        let g = Grammar::parse("root ::= [a-c0-9]+\n");
        assert!(g.accepts("abc012"));
        assert!(!g.accepts("d"));

        let g = Grammar::parse("root ::= [^\"\\\\]+\n");
        assert!(g.accepts("hello"));
        assert!(!g.accepts("say \"hi\""));

        let g = Grammar::parse("root ::= [ \\t\\n]{0,3}\n");
        assert!(g.accepts(""));
        assert!(g.accepts(" \t\n"));
        assert!(!g.accepts("    "));
    }

    #[test]
    fn rule_references_resolve() {
        let g = Grammar::parse("root ::= item (\",\" item)*\nitem ::= [a-z]+\n");
        assert!(g.accepts("ab,cd,ef"));
        assert!(!g.accepts("ab,,cd"));
    }

    #[test]
    fn a_rule_body_may_wrap_across_lines() {
        let g = Grammar::parse("root ::= \"a\"\n  | \"b\"\n");
        assert!(g.accepts("a"));
        assert!(g.accepts("b"));
    }

    #[test]
    fn a_nullable_repetition_terminates_instead_of_spinning() {
        // `ws` matches empty, so a naive matcher loops forever here.
        let g = Grammar::parse("root ::= ws* \"a\"\nws ::= [ ]{0,2}\n");
        assert!(g.accepts("a"));
        assert!(g.accepts("  a"));
    }
}
