//! The icon set: fourteen glyphs vendored from [Lucide] v1.31.0 (ISC).
//!
//! Each is stored as the *inner* markup of its `<svg>`, taken verbatim from
//! `lucide-static`. Lucide draws on a 24-unit grid with `fill="none"`,
//! `stroke="currentColor"`, round caps and round joins — exactly what [`Glyph`] puts on
//! the wrapper — so the paths drop in without adjustment.
//!
//! ## These used to be traced by hand, and one of them was wrong
//!
//! The previous set was copied out of a mockup by hand. The gear was visibly broken: its
//! path ran `V21` straight into `v-.1`, `H3` into `h.1`, and closed an arc at `13 4.6`
//! where the tooth belongs near the top centre. It was still *syntactically* valid path
//! data, so nothing caught it — a malformed icon renders as a smudge, not as an error.
//! That is the argument for vendoring upstream geometry rather than transcribing it, and
//! for recording below exactly where it came from.
//!
//! ## Refreshing them
//!
//! Each arm is commented with its upstream name. To re-fetch one:
//!
//! ```sh
//! curl -s https://unpkg.com/lucide-static@1.31.0/icons/settings.svg
//! ```
//!
//! Strip the `<svg>` wrapper and paste the inside. Bump the version in this comment when
//! you do, so the next person knows which release the set corresponds to.
//!
//! ## Two structural choices
//!
//! - **An enum, not a module of components or a string-keyed lookup.** The set is closed
//!   and small, and a `match` the compiler insists is exhaustive means an icon can never
//!   be referenced by a name that does not exist — the failure mode of every sprite
//!   sheet, and one that shows up as a blank space rather than as an error.
//! - **`dangerous_inner_html`, not rsx children.** Transcribing each path as
//!   `path { d: "…" }` would triple the line count and re-copy coordinates that are
//!   already correct, and every re-copy is a chance to get one wrong — which is precisely
//!   how the old gear broke. The markup is a `&'static str` from this file, so there is no
//!   untrusted input for the "dangerous" in the name to apply to.
//!
//! The `<svg>` itself comes from the rsx prelude rather than being part of the string,
//! because that is what carries the SVG namespace: an `svg` element created in the HTML
//! namespace parses without complaint and draws nothing.
//!
//! [Lucide]: https://lucide.dev

use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    /// The brand mark. Lucide `notebook-pen`.
    ///
    /// The same glyph the app icon is built from — see `tools/make-icons.py`. It replaced
    /// `aperture`, a camera diaphragm that said nothing about writing and collapsed into a
    /// grey disc at 16px.
    Brand,
    /// Lucide `file-text`.
    Document,
    /// Lucide `pencil`.
    Pencil,
    /// A framed panel; templates. Lucide `panels-top-left`.
    Layout,
    /// Settings. Lucide `settings`.
    Cog,
    /// Lucide `moon`.
    Moon,
    /// Lucide `search`.
    Search,
    /// Lucide `plus`.
    Plus,
    /// Lucide `download`.
    Download,
    /// Generate. Lucide `sparkles`.
    Sparkle,
    /// Lucide `chevron-up`.
    ChevronUp,
    /// Lucide `chevron-down`.
    ChevronDown,
    /// Lucide `x`.
    Close,
    /// Lucide `trash-2`.
    Trash,
}

impl Icon {
    /// Every icon, so a test can check the set is whole without this file growing a
    /// second list to forget to update. Only the test reads it; the app reaches for
    /// variants by name.
    #[cfg(test)]
    pub const ALL: [Icon; 14] = [
        Icon::Brand,
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

    /// The inner markup, verbatim from `lucide-static`.
    ///
    /// `r##"…"##` rather than `r#"…"#`: Lucide's markup contains `"` but no `"#`, and the
    /// extra hash leaves room for a future path that does.
    fn body(self) -> &'static str {
        match self {
            // lucide `notebook-pen`
            Icon::Brand => {
                r##"<path d="M13.4 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-7.4"/> <path d="M2 6h4"/> <path d="M2 10h4"/> <path d="M2 14h4"/> <path d="M2 18h4"/> <path d="M21.378 5.626a1 1 0 1 0-3.004-3.004l-5.01 5.012a2 2 0 0 0-.506.854l-.837 2.87a.5.5 0 0 0 .62.62l2.87-.837a2 2 0 0 0 .854-.506z"/>"##
            }
            // lucide `file-text`
            Icon::Document => {
                r##"<path d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z"/> <path d="M14 2v5a1 1 0 0 0 1 1h5"/> <path d="M10 9H8"/> <path d="M16 13H8"/> <path d="M16 17H8"/>"##
            }
            // lucide `pencil`
            Icon::Pencil => {
                r##"<path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z"/> <path d="m15 5 4 4"/>"##
            }
            // lucide `panels-top-left`
            Icon::Layout => {
                r##"<rect width="18" height="18" x="3" y="3" rx="2"/> <path d="M3 9h18"/> <path d="M9 21V9"/>"##
            }
            // lucide `settings`
            Icon::Cog => {
                r##"<path d="M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915"/> <circle cx="12" cy="12" r="3"/>"##
            }
            // lucide `moon`
            Icon::Moon => {
                r##"<path d="M20.985 12.486a9 9 0 1 1-9.473-9.472c.405-.022.617.46.402.803a6 6 0 0 0 8.268 8.268c.344-.215.825-.004.803.401"/>"##
            }
            // lucide `search`
            Icon::Search => r##"<path d="m21 21-4.34-4.34"/> <circle cx="11" cy="11" r="8"/>"##,
            // lucide `plus`
            Icon::Plus => r##"<path d="M5 12h14"/> <path d="M12 5v14"/>"##,
            // lucide `download`
            Icon::Download => {
                r##"<path d="M12 15V3"/> <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/> <path d="m7 10 5 5 5-5"/>"##
            }
            // lucide `sparkles`
            Icon::Sparkle => {
                r##"<path d="M11.017 2.814a1 1 0 0 1 1.966 0l1.051 5.558a2 2 0 0 0 1.594 1.594l5.558 1.051a1 1 0 0 1 0 1.966l-5.558 1.051a2 2 0 0 0-1.594 1.594l-1.051 5.558a1 1 0 0 1-1.966 0l-1.051-5.558a2 2 0 0 0-1.594-1.594l-5.558-1.051a1 1 0 0 1 0-1.966l5.558-1.051a2 2 0 0 0 1.594-1.594z"/> <path d="M20 2v4"/> <path d="M22 4h-4"/> <circle cx="4" cy="20" r="2"/>"##
            }
            // lucide `chevron-up`
            Icon::ChevronUp => r##"<path d="m18 15-6-6-6 6"/>"##,
            // lucide `chevron-down`
            Icon::ChevronDown => r##"<path d="m6 9 6 6 6-6"/>"##,
            // lucide `x`
            Icon::Close => r##"<path d="M18 6 6 18"/> <path d="m6 6 12 12"/>"##,
            // lucide `trash-2`
            Icon::Trash => {
                r##"<path d="M10 11v6"/> <path d="M14 11v6"/> <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/> <path d="M3 6h18"/> <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>"##
            }
        }
    }

    /// Stroke weight, per icon rather than one value on the `<svg>`.
    ///
    /// Lucide is designed at 2, and most of these deliberately sit below it: at the sizes
    /// the stylesheet renders them, a 2-weight chevron in a 14px slot reads as a smudge
    /// while a 22px brand mark at 2 reads as heavy. The shapes are upstream's; the weight
    /// is this app's, and keeping the choice here is what stops it being re-decided at
    /// every call site.
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
            // the stroke width nor the size, and would silently render at 0x0.
            assert!(!body.contains("<svg"), "{icon:?} carries its own <svg>");
            assert_eq!(
                body.matches('"').count() % 2,
                0,
                "{icon:?} has an unclosed attribute, which swallows the rest of the markup"
            );
        }
    }

    /// Guards the two ways vendored markup goes wrong without anyone noticing.
    #[test]
    fn every_icon_is_a_closed_set_of_drawable_shapes() {
        const DRAWABLE: [&str; 5] = ["<path", "<circle", "<rect", "<line", "<polyline"];

        for icon in Icon::ALL {
            let body = icon.body();
            assert!(
                DRAWABLE.iter().any(|tag| body.contains(tag)),
                "{icon:?} contains no drawable element"
            );
            // Every tag self-closes, because these are pasted into an existing `<svg>`
            // rather than parsed as a document: an unclosed `<path>` would swallow its
            // siblings and the icon would lose half its strokes.
            assert_eq!(
                body.matches('<').count(),
                body.matches("/>").count(),
                "{icon:?} has a tag that does not self-close"
            );
            // Lucide's own attributes belong on the wrapper, not on the shapes. One that
            // arrived inline would override `Glyph` and ignore the stroke weight above.
            for attribute in ["stroke-width", "stroke=", "fill="] {
                assert!(
                    !body.contains(attribute),
                    "{icon:?} carries `{attribute}` inline, which overrides the wrapper"
                );
            }
        }
    }

    #[test]
    fn the_gear_is_the_real_one() {
        // The icon that prompted the change. The hand-traced version closed a tooth at
        // `13 4.6` and ran `V21` into `v-.1`; Lucide's is six arcs of radius 2.34 around
        // a centred circle.
        let gear = Icon::Cog.body();
        assert!(gear.contains(r#"<circle cx="12" cy="12" r="3"/>"#), "{gear}");
        // Twelve arc segments — six teeth and the six arcs joining them. Counted by the
        // radius pair rather than by the `a` command, of which there are only two: SVG
        // lets a repeated command drop its letter, so ten of the twelve arcs are implicit.
        assert_eq!(gear.matches("2.34 2.34").count(), 12, "six teeth and six joins");
        assert!(!gear.contains("V21"), "the malformed vertical from the traced version");
    }
}
