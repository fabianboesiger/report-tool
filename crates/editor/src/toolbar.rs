//! Formatting controls for the surrounding [`Editor`](crate::Editor).
//!
//! Every button uses `onmousedown` with `prevent_default`, never `onclick`. A click
//! moves focus to the button, which blurs the block — and the blur is what releases
//! the focus guard and clears the selection the button was about to act on. Stopping
//! the default on mousedown means focus never leaves the text at all, so the caret is
//! still where the user left it when the command runs.

use dioxus::prelude::*;
use report_doc::{BlockKind, Marks};

use crate::editor::EditorState;

/// The toolbar's tooltips, supplied by the host.
///
/// A prop rather than a lookup, for the same reason [`crate::Editor`]'s `placeholder` is
/// one: this crate must not learn about the app, and the app's Fluent catalogue is the
/// app's. (Nor could it have its own — a Fluent bundle admits exactly one resource per
/// language, so there is no way for two crates to contribute keys to the same catalogue.)
///
/// The button faces themselves — `B`, `¶`, `1.`, `❝` — are glyphs and are not here;
/// they read the same in every language and translating them would only break the
/// toolbar's alignment.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolbarLabels {
    pub bold: String,
    pub italic: String,
    pub code: String,
    pub strike: String,
    pub paragraph: String,
    /// One per heading level, rendered whole rather than as a stem plus a number:
    /// "Heading 2" and "Titolo 2" happen to share their word order, and the next
    /// language need not.
    pub headings: [String; 3],
    pub bulleted: String,
    pub numbered: String,
    pub quote: String,
}

impl Default for ToolbarLabels {
    /// English, so that a host that says nothing still gets a usable toolbar rather than
    /// nine empty tooltips.
    fn default() -> Self {
        Self {
            bold: "Bold (Cmd/Ctrl+B)".into(),
            italic: "Italic (Cmd/Ctrl+I)".into(),
            code: "Code (Cmd/Ctrl+E)".into(),
            strike: "Strikethrough".into(),
            paragraph: "Paragraph".into(),
            headings: ["Heading 1".into(), "Heading 2".into(), "Heading 3".into()],
            bulleted: "Bulleted list".into(),
            numbered: "Numbered list".into(),
            quote: "Quote".into(),
        }
    }
}

#[component]
pub fn Toolbar(#[props(default)] labels: ToolbarLabels) -> Element {
    let state = use_context::<EditorState>();
    let current = state.current_kind();

    let is = |kind: &BlockKind| current.as_ref() == Some(kind);

    rsx! {
        div { class: "rt-toolbar", role: "toolbar",
            MarkButton { mark: Marks::BOLD, label: "B", title: labels.bold.clone(), class: "rt-bold" }
            MarkButton { mark: Marks::ITALIC, label: "I", title: labels.italic.clone(), class: "rt-italic" }
            MarkButton { mark: Marks::CODE, label: "‹›", title: labels.code.clone(), class: "rt-mono" }
            MarkButton { mark: Marks::STRIKE, label: "S", title: labels.strike.clone(), class: "rt-strike" }

            span { class: "rt-sep" }

            KindButton {
                kind: BlockKind::Paragraph, label: "¶", title: labels.paragraph.clone(),
                active: is(&BlockKind::Paragraph),
            }
            for level in 1..=3u8 {
                KindButton {
                    key: "h{level}",
                    kind: BlockKind::Heading { level },
                    label: "H{level}",
                    title: labels.headings[level as usize - 1].clone(),
                    active: is(&BlockKind::Heading { level }),
                }
            }

            span { class: "rt-sep" }

            KindButton {
                kind: BlockKind::BulletItem { indent: 0 }, label: "•", title: labels.bulleted.clone(),
                active: matches!(current, Some(BlockKind::BulletItem { .. })),
            }
            KindButton {
                kind: BlockKind::NumberedItem { indent: 0 }, label: "1.", title: labels.numbered.clone(),
                active: matches!(current, Some(BlockKind::NumberedItem { .. })),
            }
            KindButton {
                kind: BlockKind::Quote, label: "❝", title: labels.quote.clone(),
                active: is(&BlockKind::Quote),
            }
        }
    }
}

#[component]
fn MarkButton(mark: Marks, label: String, title: String, class: String) -> Element {
    let state = use_context::<EditorState>();
    // Nothing selected means nothing to format; showing the button as available
    // would promise an effect it cannot deliver.
    let enabled = state.selection.read().is_some_and(|s| !s.is_empty());

    rsx! {
        button {
            r#type: "button",
            class: "rt-tool {class}",
            title: "{title}",
            disabled: !enabled,
            onmousedown: move |event| {
                event.prevent_default();
                state.toggle_mark(mark);
            },
            "{label}"
        }
    }
}

#[component]
fn KindButton(kind: BlockKind, label: String, title: String, active: bool) -> Element {
    let state = use_context::<EditorState>();
    rsx! {
        button {
            r#type: "button",
            class: if active { "rt-tool rt-active" } else { "rt-tool" },
            title: "{title}",
            onmousedown: {
                let kind = kind.clone();
                move |event: Event<MouseData>| {
                    event.prevent_default();
                    state.set_kind(kind.clone());
                }
            },
            "{label}"
        }
    }
}
