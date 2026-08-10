//! The seam between the browser's DOM and the Rust document model.
//!
//! [`EditorRuntime`] installs `assets/editor.js` once, pumps the events it sends into
//! a signal, and provides that signal through context. Editing surfaces react to it
//! with `use_effect`.
//!
//! ## Why events land in a signal instead of per-block callbacks
//!
//! The obvious design is a registry: each editable block registers a callback keyed
//! by its id, and the pump dispatches to it. That does not survive contact with
//! Dioxus. A callback captured in `use_hook` closes over the props of the render that
//! created it, so after the first re-render it is holding stale handlers — and the
//! symptom is edits that silently apply to an older version of the document, which is
//! close to the worst bug this component could have.
//!
//! Signals do not have that problem: a `Signal` is a `Copy` handle that stays valid
//! across renders, so an effect closing over signals only ever reads current state.
//! Hence one signal carrying the latest event, and consumers that own their state as
//! signals. Every surface's effect sees every event and ignores the ids it does not
//! own, which costs a comparison per keystroke and buys correctness that the registry
//! design cannot offer.

use dioxus::prelude::*;
use report_doc::BlockId;
use serde::Deserialize;

/// An event from the browser.
///
/// Offsets are **code point** counts, matching `str::chars()` on this side; the shim
/// converts from the browser's UTF-16 indices.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RawEvent {
    /// The user typed; `html` is the block's new content.
    Input {
        id: BlockId,
        html: String,
        caret: usize,
    },
    /// A key the browser was stopped from handling, because it means a structural
    /// change: Enter, Tab, or Backspace at offset 0.
    Key {
        id: BlockId,
        key: String,
        shift: bool,
        caret: usize,
        length: usize,
        html: String,
    },
    /// A formatting shortcut (Cmd/Ctrl + B, I, E).
    Mark {
        id: BlockId,
        mark: String,
        start: usize,
        end: usize,
    },
    Focus {
        id: BlockId,
    },
    /// `html` rides along so an edit dismissed by clicking elsewhere is not lost
    /// between the last `input` and the blur.
    Blur {
        id: BlockId,
        html: String,
    },
    Selection {
        id: BlockId,
        start: usize,
        end: usize,
    },
}

impl RawEvent {
    pub fn block(&self) -> BlockId {
        match self {
            RawEvent::Input { id, .. }
            | RawEvent::Key { id, .. }
            | RawEvent::Mark { id, .. }
            | RawEvent::Focus { id }
            | RawEvent::Blur { id, .. }
            | RawEvent::Selection { id, .. } => *id,
        }
    }
}

/// One delivered event.
///
/// The sequence number matters: two identical events in a row (pressing Enter twice
/// on an empty block) would otherwise leave the signal unchanged and the second one
/// would never wake an effect.
#[derive(Debug, Clone, PartialEq)]
pub struct Delivery {
    pub seq: u64,
    pub event: RawEvent,
}

/// Handle to the browser bridge, obtained with [`use_bridge`].
#[derive(Clone, Copy)]
pub struct Bridge {
    latest: Signal<Option<Delivery>>,
}

impl Bridge {
    /// The most recent event, if any.
    pub fn latest(&self) -> Option<Delivery> {
        self.latest.read().clone()
    }

    /// Focus a block and place the caret at a character offset.
    ///
    /// Call this from an effect rather than straight after mutating the document: the
    /// element for a block that was just created does not exist until the render
    /// commits, and focusing it any earlier is a silent no-op.
    pub fn focus(&self, id: BlockId, offset: usize) {
        command(format!("window.__reportEditor.focus(\"{id}\", {offset});"));
    }

    /// Select a range within a block.
    pub fn select(&self, id: BlockId, start: usize, end: usize) {
        command(format!("window.__reportEditor.select(\"{id}\", {start}, {end});"));
    }

    /// Push new HTML into a focused block.
    ///
    /// Needed only because of the focus guard: while a block is focused, Rust stops
    /// rewriting its HTML so the browser can own the caret, which also means an edit
    /// Rust makes itself (a toolbar mark) has no other way in.
    pub fn sync(&self, id: BlockId, html: &str, start: usize, end: usize) {
        let escaped = escape_js(html);
        command(format!("window.__reportEditor.sync(\"{id}\", \"{escaped}\", {start}, {end});"));
    }
}

fn command(script: String) {
    // Fire-and-forget: these are one-shot calls into the API the main script put on
    // `window`, so there is no reply to wait for and nothing to keep alive.
    document::eval(&script);
}

fn escape_js(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            // A literal `</script>` inside a string would end the block early in
            // some embedding contexts; splitting the slash makes that impossible.
            '<' => out.push_str("\\x3C"),
            _ => out.push(ch),
        }
    }
    out
}

/// The bridge provided by the nearest [`EditorRuntime`].
///
/// Panics if no runtime is mounted, which is a wiring mistake rather than a runtime
/// condition — an editable field outside the runtime would silently never receive a
/// keystroke, and a panic at mount is far easier to diagnose than that.
pub fn use_bridge() -> Bridge {
    use_context::<Bridge>()
}

/// Installs the browser-side script and the editor stylesheet, and provides the
/// [`Bridge`] to everything beneath it. Mount once, near the root of the app.
#[component]
pub fn EditorRuntime(children: Element) -> Element {
    let latest = use_signal(|| None::<Delivery>);
    let bridge = Bridge { latest };
    use_context_provider(|| bridge);

    use_hook(move || {
        let mut latest = latest;
        spawn(async move {
            let mut eval = document::eval(include_str!("../assets/editor.js"));
            let mut seq = 0u64;
            loop {
                // Received as a `Value` first so a message this build does not
                // understand is skipped rather than tearing down the channel — after
                // which every later keystroke would be lost with no visible cause.
                match eval.recv::<serde_json::Value>().await {
                    Ok(value) => match serde_json::from_value::<RawEvent>(value.clone()) {
                        Ok(event) => {
                            seq += 1;
                            latest.set(Some(Delivery { seq, event }));
                        }
                        Err(error) => {
                            tracing::warn!("editor: unrecognised event {value}: {error}");
                        }
                    },
                    Err(error) => {
                        tracing::error!("editor: bridge closed: {error}");
                        break;
                    }
                }
            }
        });
    });

    rsx! {
        style { dangerous_inner_html: crate::styles::CSS }
        {children}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_deserialize_from_what_the_shim_sends() {
        let raw = r#"{"kind":"input","id":"1ca1d2e6-0f1e-4f4b-9c4e-3a0b5d6e7f80",
                      "html":"<strong>a</strong>","caret":1}"#;
        let event: RawEvent = serde_json::from_str(raw).unwrap();
        match event {
            RawEvent::Input { html, caret, .. } => {
                assert_eq!(html, "<strong>a</strong>");
                assert_eq!(caret, 1);
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn key_events_carry_everything_needed_to_decide_the_edit() {
        let raw = r#"{"kind":"key","id":"1ca1d2e6-0f1e-4f4b-9c4e-3a0b5d6e7f80",
                      "key":"Enter","shift":false,"caret":3,"length":9,"html":"some text"}"#;
        let RawEvent::Key { key, caret, length, .. } = serde_json::from_str(raw).unwrap() else {
            panic!("wrong variant");
        };
        assert_eq!((key.as_str(), caret, length), ("Enter", 3, 9));
    }

    /// Whether a quote appears that would close the surrounding string literal.
    fn has_unescaped_quote(s: &str) -> bool {
        let chars: Vec<char> = s.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            match chars[i] {
                '\\' => i += 2,
                '"' => return true,
                _ => i += 1,
            }
        }
        false
    }

    #[test]
    fn javascript_strings_are_escaped_so_content_cannot_break_out() {
        // Block content is user text, and it reaches the browser inside a string
        // literal in a generated script. Unescaped, a quote would close that literal
        // and everything after it would be evaluated as code.
        let hostile = r#"a" ); alert(1); //"#;
        let escaped = escape_js(hostile);
        assert!(!has_unescaped_quote(&escaped), "{escaped}");
        assert_eq!(escaped, r#"a\" ); alert(1); //"#);

        // A trailing backslash must not escape the closing quote either.
        assert_eq!(escape_js("a\\"), "a\\\\");
        assert!(!has_unescaped_quote(&escape_js("ends with a backslash \\")));

        assert_eq!(escape_js("line\nbreak"), "line\\nbreak");
        assert_eq!(escape_js("</script>"), "\\x3C/script>");
    }
}
