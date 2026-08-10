//! Page structure: the head band, the scrolling body, a pane, a library listing, and the
//! two things that sit between the head and the body.

use dioxus::prelude::*;

use super::icon::{Glyph, Icon};

/// The `.head` band: a title, one line of context, and right-aligned actions.
///
/// Drawn by all four screens, which is why the "12 reports · last written 2 hours ago"
/// line is a plain string rather than a slot — every screen has exactly one sentence to
/// say about itself, and a slot would invite a second.
#[component]
pub fn PageHead(
    title: String,
    #[props(default)] subtitle: String,
    /// When set, the title becomes an editable field and every keystroke is reported.
    ///
    /// Only the editor screen passes this. It is an `input` rather than the mockup's
    /// `contenteditable h1`: a single-line name gets undo, select-all and input-method
    /// handling from the platform for free, and routing it through the editor bridge
    /// would put a report's name into the same event stream as its prose for no gain.
    #[props(default)]
    on_title: Option<EventHandler<String>>,
    #[props(default)] actions: Option<Element>,
) -> Element {
    rsx! {
        div { class: "head",
            div { style: "min-width:0;flex:0 1 auto",
                if let Some(on_title) = on_title {
                    input {
                        class: "head-title",
                        value: "{title}",
                        placeholder: "Untitled report",
                        "aria-label": "Report name",
                        oninput: move |event| on_title.call(event.value()),
                    }
                } else {
                    h1 { "{title}" }
                }
                if !subtitle.is_empty() {
                    p { "{subtitle}" }
                }
            }
            span { class: "grow" }
            if let Some(actions) = actions {
                div { class: "head-actions", {actions} }
            }
        }
    }
}

/// The scrolling area below the head.
///
/// `flush` turns off the padding and the scroll for a screen that fills the space with
/// its own layout instead — the editor split, which manages two scrollers of its own.
#[component]
pub fn PageBody(#[props(default)] flush: bool, children: Element) -> Element {
    rsx! {
        div { class: if flush { "body is-flush" } else { "body" }, {children} }
    }
}

/// One half of the editor split: a quiet uppercase label, optional controls, and a
/// scrolling body.
#[component]
pub fn Pane(
    label: String,
    #[props(default)] actions: Option<Element>,
    /// Added to the editor inside, `notes` or `doc` in practice. See the "editor, dressed
    /// for each pane" section of `app.css` for why the two panes are styled from out here
    /// rather than by the editor crate.
    #[props(default)]
    body_class: String,
    children: Element,
) -> Element {
    rsx! {
        section { class: "pane",
            div { class: "pane-head",
                h2 { "{label}" }
                span { class: "grow" }
                if let Some(actions) = actions {
                    {actions}
                }
            }
            div { class: "pane-body {body_class}", {children} }
        }
    }
}

/// A strip between the head and the body.
///
/// Not a modal and not dismissible: everything it says is something the user can keep
/// working through, which is the whole reason it is a strip.
#[component]
pub fn Banner(#[props(default)] warn: bool, children: Element) -> Element {
    rsx! {
        div { class: if warn { "banner is-warn" } else { "banner" }, {children} }
    }
}

/// A progress track.
///
/// `None` means the total is unknown, drawn as a full faint bar rather than an empty one —
/// an empty bar reads as stuck.
#[component]
pub fn Bar(fraction: Option<f32>) -> Element {
    rsx! {
        div { class: "bar",
            match fraction {
                Some(fraction) => rsx! {
                    i { style: "width: {(fraction * 100.0).clamp(0.0, 100.0):.1}%" }
                },
                None => rsx! { i { class: "is-unknown" } },
            }
        }
    }
}

/// What a screen shows instead of an empty list.
///
/// Carries its own call to action, because "no reports yet" without a way to make one is
/// a dead end dressed as an explanation.
#[component]
pub fn EmptyState(
    #[props(default)] icon: Option<Icon>,
    title: String,
    #[props(default)] hint: String,
    #[props(default)] action: Option<Element>,
) -> Element {
    rsx! {
        div { class: "empty",
            if let Some(icon) = icon {
                Glyph { icon }
            }
            h2 { "{title}" }
            if !hint.is_empty() {
                p { "{hint}" }
            }
            if let Some(action) = action {
                {action}
            }
        }
    }
}

/// A small inline message.
///
/// Retires five one-off styles — `.lib-status`, `.sp-ok`, `.sp-error`,
/// `.app-dictate-error` and `.dl-detail` — and the hardcoded hexes they carried, none of
/// which had a dark-mode variant.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Ok,
    Error,
}

#[component]
pub fn Notice(kind: NoticeKind, message: String) -> Element {
    let class = match kind {
        NoticeKind::Ok => "notice is-ok",
        NoticeKind::Error => "notice is-error",
    };
    rsx! {
        p { class: "{class}", "{message}" }
    }
}

/// A library listing.
#[component]
pub fn List(children: Element) -> Element {
    rsx! {
        div { class: "list", {children} }
    }
}

/// One row of a library listing.
///
/// Drawn by both the Reports and the Templates screen. That second caller is the only
/// reason this is in the kit at all — with a header picker for templates instead of a
/// list it would have had one, and belonged in `reports.rs`.
#[component]
pub fn Row(
    name: String,
    /// The second column: where the thing came from.
    #[props(default)]
    from: String,
    /// A label, and whether it marks the unfinished state — which colours it.
    #[props(default)]
    tag: Option<(String, bool)>,
    /// Right-aligned, already humanised by `report_core::store::relative_time`.
    #[props(default)]
    when: String,
    onopen: EventHandler<()>,
    /// Revealed on hover, like the node actions in the builder.
    #[props(default)]
    ondelete: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        div { class: "row", role: "button", tabindex: "0",
            onclick: move |_| onopen.call(()),
            span { class: "name", "{name}" }
            if !from.is_empty() {
                span { class: "from", "{from}" }
            }
            if let Some((label, draft)) = tag {
                span { class: if draft { "tag is-draft" } else { "tag" }, "{label}" }
            }
            span { class: "when", "{when}" }
            if let Some(ondelete) = ondelete {
                span { class: "row-acts",
                    super::controls::IconButton {
                        icon: Icon::Trash,
                        title: "Delete".to_string(),
                        onclick: move |event: MouseEvent| {
                            // Without this the row's own handler fires too and the
                            // report opens on its way to being deleted.
                            event.stop_propagation();
                            ondelete.call(());
                        },
                    }
                }
            }
        }
    }
}
