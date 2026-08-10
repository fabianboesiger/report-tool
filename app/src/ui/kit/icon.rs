//! The icon set.
//!
//! Fourteen line icons, each stored as the *inner* markup of its `<svg>`, copied out of
//! `design/1-aperture.html`. Two choices worth defending:
//!
//! - **An enum, not a module of components or a string-keyed lookup.** The set is closed
//!   and small, and a `match` the compiler insists is exhaustive means an icon can never
//!   be referenced by a name that does not exist — the failure mode of every sprite
//!   sheet, and one that shows up as a blank space rather than as an error.
//! - **`dangerous_inner_html`, not rsx children.** Transcribing each path as
//!   `path { d: "…" }` would triple the line count and re-copy coordinates that are
//!   already correct, and every re-copy is a chance to get one wrong. The markup is a
//!   `&'static str` from this file, so there is no untrusted input for the "dangerous" in
//!   the name to apply to — the same reasoning that already puts both stylesheets in
//!   `style { dangerous_inner_html: … }`.
//!
//! The `<svg>` itself comes from the rsx prelude rather than being part of the string,
//! because that is what carries the SVG namespace: an `svg` element created in the HTML
//! namespace parses without complaint and draws nothing.

use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    /// The brand mark: two concentric circles, an aperture.
    Aperture,
    Document,
    Pencil,
    Layout,
    Cog,
    Moon,
    Search,
    Plus,
    Download,
    Sparkle,
    ChevronUp,
    ChevronDown,
    Close,
    Trash,
}

impl Icon {
    /// Every icon, so a test can check the set is whole without this file growing a
    /// second list to forget to update. Only the test reads it; the app reaches for
    /// variants by name.
    #[cfg(test)]
    pub const ALL: [Icon; 14] = [
        Icon::Aperture,
        Icon::Document,
        Icon::Pencil,
        Icon::Layout,
        Icon::Cog,
        Icon::Moon,
        Icon::Search,
        Icon::Plus,
        Icon::Download,
        Icon::Sparkle,
        Icon::ChevronUp,
        Icon::ChevronDown,
        Icon::Close,
        Icon::Trash,
    ];

    fn body(self) -> &'static str {
        match self {
            Icon::Aperture => r#"<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="4.2"/>"#,
            Icon::Document => {
                r#"<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/>"#
            }
            Icon::Pencil => {
                r#"<path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z"/>"#
            }
            Icon::Layout => {
                r#"<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M3 9h18M9 9v12"/>"#
            }
            Icon::Cog => {
                r#"<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-2.9 1.2V21a2 2 0 1 1-4 0v-.1A1.7 1.7 0 0 0 7 19.4a1.7 1.7 0 0 0-1.9.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1A1.7 1.7 0 0 0 3 15H3a2 2 0 1 1 0-4h.1A1.7 1.7 0 0 0 4.6 9a1.7 1.7 0 0 0-.3-1.9l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1A1.7 1.7 0 0 0 9 4.6h.1A2 2 0 1 1 13 4.6V4.7A1.7 1.7 0 0 0 15 4.6a1.7 1.7 0 0 0 1.9.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1A1.7 1.7 0 0 0 19.4 9V9a2 2 0 1 1 0 4z"/>"#
            }
            Icon::Moon => r#"<path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z"/>"#,
            Icon::Search => r#"<circle cx="11" cy="11" r="7"/><path d="m21 21-4.3-4.3"/>"#,
            Icon::Plus => r#"<path d="M12 5v14M5 12h14"/>"#,
            Icon::Download => {
                r#"<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="M7 10l5 5 5-5M12 15V3"/>"#
            }
            Icon::Sparkle => {
                r#"<path d="M12 3v3M12 18v3M5.6 5.6l2.1 2.1M16.3 16.3l2.1 2.1M3 12h3M18 12h3M5.6 18.4l2.1-2.1M16.3 7.7l2.1-2.1"/>"#
            }
            Icon::ChevronUp => r#"<path d="m18 15-6-6-6 6"/>"#,
            Icon::ChevronDown => r#"<path d="m6 9 6 6 6-6"/>"#,
            Icon::Close => r#"<path d="M18 6 6 18M6 6l12 12"/>"#,
            Icon::Trash => {
                r#"<path d="M3 6h18M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2"/><path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/>"#
            }
        }
    }

    /// Stroke weight, per icon rather than one value on the `<svg>`.
    ///
    /// A plus or a chevron drawn at 14px with a 1.6 stroke reads as a smudge, and a 22px
    /// brand mark drawn at 2 reads as heavy. The mockup already varies it; this is where
    /// that variation is kept in one place instead of at every call site.
    fn stroke(self) -> &'static str {
        match self {
            Icon::Plus | Icon::ChevronUp | Icon::ChevronDown | Icon::Close => "2",
            Icon::Search | Icon::Sparkle => "1.8",
            _ => "1.6",
        }
    }
}

/// One icon, coloured by `currentColor` and sized by whatever contains it.
///
/// Deliberately has no size prop: the stylesheet already sizes icons per context
/// (`.btn svg`, `.nav svg`, `.icon-btn svg`, `.brand svg`), and a size passed in here
/// would be a second opinion that silently disagrees with the first.
#[component]
pub fn Glyph(icon: Icon) -> Element {
    rsx! {
        svg {
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: icon.stroke(),
            stroke_linecap: "round",
            stroke_linejoin: "round",
            // Decorative: every icon in this app sits beside a label or inside a button
            // that carries a `title`, so announcing it twice is worse than not at all.
            "aria-hidden": "true",
            dangerous_inner_html: icon.body(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_carries_markup_and_nothing_but_markup() {
        for icon in Icon::ALL {
            let body = icon.body();
            assert!(!body.trim().is_empty(), "{icon:?} draws nothing");
            // The wrapper is drawn once by `Glyph`. A nested one would inherit neither
            // the stroke width nor the size, and would silently render at 0×0.
            assert!(!body.contains("<svg"), "{icon:?} carries its own <svg>");
            assert_eq!(
                body.matches('"').count() % 2,
                0,
                "{icon:?} has an unclosed attribute, which swallows the rest of the markup"
            );
        }
    }
}
