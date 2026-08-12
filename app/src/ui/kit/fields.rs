//! Inputs, and the containers that group them.

use dioxus::prelude::*;

use super::icon::{Glyph, Icon};

/// A labelled text input with an optional line of help under it.
#[component]
pub fn TextField(
    label: String,
    value: String,
    #[props(default)] hint: String,
    #[props(default)] placeholder: String,
    #[props(default)] secret: bool,
    oninput: EventHandler<String>,
) -> Element {
    rsx! {
        label { class: "field",
            span { "{label}" }
            input {
                r#type: if secret { "password" } else { "text" },
                value: "{value}",
                placeholder: "{placeholder}",
                oninput: move |event| oninput.call(event.value()),
            }
            if !hint.is_empty() {
                span { class: "hint", "{hint}" }
            }
        }
    }
}

/// A labelled dropdown, shaped like [`TextField`] so the two stack cleanly in a `Group`.
///
/// A real `<select>` under the app's own paint, for the same reason [`ChoiceCard`] wraps a
/// real radio: the element is what supplies keyboard navigation, type-to-select and an
/// accessible name, none of which a styled `div` and an `onclick` can imitate.
///
/// Radio cards would be the house style, but the language list is five options nobody
/// revisits, and five cards is more vertical space than the decision is worth.
///
/// `options` is `(value, label)`. The value is an opaque token matched back by the caller —
/// never a translated string, since Fluent wraps interpolations in invisible isolates and a
/// round-tripped label would stop comparing equal.
#[component]
pub fn Select(
    label: String,
    #[props(default)] hint: String,
    options: Vec<(String, String)>,
    value: String,
    onchange: EventHandler<String>,
) -> Element {
    rsx! {
        label { class: "field",
            span { "{label}" }
            // The caret is a sibling glyph inside a positioned wrapper rather than a
            // pseudo-element on the label, so it needs neither `:has()` — which this ships
            // against whatever webkit2gtk a distro carries — nor a chevron written out once
            // per theme as a data URI the palette cannot reach. Same shape as
            // [`ChoiceCard`], which paints its own dot over a real radio.
            span { class: "select-box",
                select {
                    class: "select",
                    value: "{value}",
                    onchange: move |event| onchange.call(event.value()),
                    for (token, text) in options.iter() {
                        option { key: "{token}", value: "{token}", selected: *token == value, "{text}" }
                    }
                }
                Glyph { icon: Icon::ChevronDown }
            }
            if !hint.is_empty() {
                span { class: "hint", "{hint}" }
            }
        }
    }
}

/// An optional count, inline in a row of options.
///
/// Empty means unbounded, which is the common case and therefore the default; a `0` would
/// mean something quite different. (This is the old `Bound`, moved here so the template
/// builder's four call sites and any later one share it.)
#[component]
pub fn NumberField(
    label: String,
    value: Option<u32>,
    #[props(default = "—".to_string())] placeholder: String,
    on_change: EventHandler<Option<u32>>,
) -> Element {
    let shown = value.map(|value| value.to_string()).unwrap_or_default();
    rsx! {
        label {
            "{label} "
            input {
                r#type: "number",
                min: "0",
                value: "{shown}",
                placeholder: "{placeholder}",
                oninput: move |event| {
                    let text = event.value();
                    on_change.call(if text.trim().is_empty() {
                        None
                    } else {
                        text.trim().parse().ok()
                    });
                },
            }
        }
    }
}

/// A radio card.
///
/// Wraps a real `input[type=radio]` hidden under the painted dot rather than replacing it
/// with a `div` and an `onclick`: the real input is what gives the group arrow-key
/// navigation and a name the accessibility layer can read, and the dot is only paint over
/// the top of it.
#[component]
pub fn ChoiceCard(
    /// Shared by every card in one question, so the browser treats them as a group.
    group: String,
    title: String,
    hint: String,
    on: bool,
    #[props(default)] disabled: bool,
    onselect: EventHandler<()>,
) -> Element {
    let mut class = String::from("choice");
    if on {
        class.push_str(" is-on");
    }
    if disabled {
        class.push_str(" is-disabled");
    }
    rsx! {
        label { class: "{class}", style: "position:relative",
            input {
                r#type: "radio",
                name: "{group}",
                checked: on,
                disabled,
                onchange: move |_| onselect.call(()),
            }
            span { class: "r" }
            span {
                b { "{title}" }
                span { class: "hint", "{hint}" }
            }
        }
    }
}

/// A settings block: heading, one sentence of why, then content.
///
/// Separated from its neighbour by a hairline rather than boxed, so the page reads as one
/// column instead of a stack of cards.
#[component]
pub fn Group(title: String, #[props(default)] sub: String, children: Element) -> Element {
    rsx! {
        div { class: "group",
            h3 { "{title}" }
            if !sub.is_empty() {
                p { class: "sub", "{sub}" }
            }
            {children}
        }
    }
}
