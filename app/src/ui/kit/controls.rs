//! Buttons, and the rail's navigation links.
//!
//! These four replace eight separate button styles — `.app-tab`, `.app-generate`,
//! `.lib-btn`, `.sp-save`, `.tb-btn`, `.tb-danger`, `.tb-add-btn` and the rail links —
//! each of which used to be defined once per module and drift from the others.

use dioxus::prelude::*;

use super::icon::{Glyph, Icon};

/// How much weight a button carries.
///
/// There are exactly three, because Aperture has exactly one accent — ink — and spending
/// it more than once per screen would stop it meaning "this is the thing to press".
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Variant {
    /// A hairline border on the surface. The workhorse.
    #[default]
    Normal,
    /// Ink filled. One per screen.
    Primary,
    /// No border, muted text: an action that must be reachable but not noticed.
    Quiet,
    /// Dashed, for adding something — it reads as a slot waiting to be filled rather
    /// than as an action already available.
    Ghost,
}

impl Variant {
    fn class(self) -> &'static str {
        match self {
            Variant::Normal => "btn",
            Variant::Primary => "btn btn-primary",
            Variant::Quiet => "btn btn-quiet",
            Variant::Ghost => "btn btn-ghost",
        }
    }
}

#[component]
pub fn Button(
    label: String,
    /// Drawn before the label, never after. Fixed here rather than left to a children
    /// slot precisely so the icon-then-text rhythm cannot vary by call site.
    #[props(default)]
    icon: Option<Icon>,
    #[props(default)] variant: Variant,
    #[props(default)] disabled: bool,
    #[props(default)] title: String,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: variant.class(),
            disabled,
            title: if title.is_empty() { None } else { Some(title.clone()) },
            onclick: move |event| onclick.call(event),
            if let Some(icon) = icon {
                Glyph { icon }
            }
            "{label}"
        }
    }
}

/// A 26px square with an icon in it.
///
/// `title` is required rather than optional: these appear on hover with no label, so one
/// without a title is simply unreadable.
#[component]
pub fn IconButton(
    icon: Icon,
    title: String,
    #[props(default)] disabled: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "icon-btn",
            disabled,
            title: "{title}",
            "aria-label": "{title}",
            onclick: move |event| onclick.call(event),
            Glyph { icon }
        }
    }
}

/// One entry in the rail.
///
/// Takes `active: bool` rather than a `Screen`, which is what keeps this file free of the
/// app's vocabulary; the rail does the comparison.
#[component]
pub fn NavLink(
    icon: Icon,
    label: String,
    active: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: if active { "is-active" } else { "" },
            "aria-current": if active { Some("page") } else { None },
            onclick: move |event| onclick.call(event),
            Glyph { icon }
            "{label}"
        }
    }
}
