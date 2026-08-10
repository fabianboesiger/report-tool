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

#[component]
pub fn Toolbar() -> Element {
    let state = use_context::<EditorState>();
    let current = state.current_kind();

    let is = |kind: &BlockKind| current.as_ref() == Some(kind);

    rsx! {
        div { class: "rt-toolbar", role: "toolbar",
            MarkButton { mark: Marks::BOLD, label: "B", title: "Bold (Cmd/Ctrl+B)", class: "rt-bold" }
            MarkButton { mark: Marks::ITALIC, label: "I", title: "Italic (Cmd/Ctrl+I)", class: "rt-italic" }
            MarkButton { mark: Marks::CODE, label: "‹›", title: "Code (Cmd/Ctrl+E)", class: "rt-mono" }
            MarkButton { mark: Marks::STRIKE, label: "S", title: "Strikethrough", class: "rt-strike" }

            span { class: "rt-sep" }

            KindButton {
                kind: BlockKind::Paragraph, label: "¶", title: "Paragraph",
                active: is(&BlockKind::Paragraph),
            }
            for level in 1..=3u8 {
                KindButton {
                    key: "h{level}",
                    kind: BlockKind::Heading { level },
                    label: "H{level}",
                    title: "Heading {level}",
                    active: is(&BlockKind::Heading { level }),
                }
            }

            span { class: "rt-sep" }

            KindButton {
                kind: BlockKind::BulletItem { indent: 0 }, label: "•", title: "Bulleted list",
                active: matches!(current, Some(BlockKind::BulletItem { .. })),
            }
            KindButton {
                kind: BlockKind::NumberedItem { indent: 0 }, label: "1.", title: "Numbered list",
                active: matches!(current, Some(BlockKind::NumberedItem { .. })),
            }
            KindButton {
                kind: BlockKind::Quote, label: "❝", title: "Quote",
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
