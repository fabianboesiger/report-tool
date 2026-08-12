//! The left rail: where you are, and where the writing happens.

use dioxus::prelude::*;
use report_core::settings::{Provider, Settings};

use crate::i18n::t;
use crate::ui::kit::{Glyph, Icon, NavLink};
use crate::ui::Screen;

#[component]
pub fn Rail(screen: Signal<Screen>, settings: Signal<Settings>) -> Element {
    let provider = settings.read().provider;
    let (where_title, where_detail) = where_line(provider, &settings.read());

    rsx! {
        aside { class: "rail",
            div { class: "brand",
                Glyph { icon: Icon::Brand }
                // The product's name, not a word about it: left untranslated for the same
                // reason the window title is.
                "Report tool"
            }

            nav { class: "nav",
                NavLink {
                    icon: Icon::Document,
                    label: t!("rail-nav-reports"),
                    active: screen() == Screen::Reports,
                    onclick: move |_| screen.set(Screen::Reports),
                }
                NavLink {
                    icon: Icon::Pencil,
                    label: t!("rail-nav-current"),
                    active: screen() == Screen::Editor,
                    onclick: move |_| screen.set(Screen::Editor),
                }
                div { class: "nav-label", {t!("rail-nav-setup")} }
                NavLink {
                    icon: Icon::Layout,
                    label: t!("rail-nav-templates"),
                    active: screen() == Screen::Templates,
                    onclick: move |_| screen.set(Screen::Templates),
                }
                NavLink {
                    icon: Icon::Cog,
                    label: t!("rail-nav-settings"),
                    active: screen() == Screen::Settings,
                    onclick: move |_| screen.set(Screen::Settings),
                }
            }

            div { class: "where",
                b { "{where_title}" }
                "{where_detail}"
            }

        }
    }
}

/// The privacy line, in the only wording that answers the question it exists to answer.
///
/// Live rather than hardcoded: the whole point of this blurb is that it tells the user
/// whether their notes leave the machine, and a fixed string would be a promise the
/// settings could quietly break. "This computer" rather than the mockup's "this Mac",
/// because the same binary ships to Windows and Linux.
fn where_line(provider: Provider, settings: &Settings) -> (String, String) {
    match provider {
        Provider::Local => (t!("rail-where-local-title"), t!("rail-where-local-detail")),
        Provider::Remote => {
            // Naming the host is the point, so an address nobody has set has to say so:
            // "Your notes are sent to ." would read as a bug, and worse, as reassurance.
            let host =
                host_of(&settings.openai.base_url).unwrap_or_else(|| t!("rail-where-unset-server"));
            (t!("rail-where-remote-title"), t!("rail-where-remote-detail", host: host))
        }
        Provider::Stub => (t!("rail-where-stub-title"), t!("rail-where-stub-detail")),
    }
}

/// The host part of a URL, so the line names somewhere recognisable rather than a path.
///
/// `None` for an address that has not been set. Kept separate from the wording around it
/// precisely so it stays testable: what is worth asserting here is the URL handling, and a
/// function that also translates could only be exercised inside a running app.
fn host_of(url: &str) -> Option<String> {
    let host = url
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("")
        .trim();
    (!host.is_empty()).then(|| host.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_privacy_line_names_a_place_rather_than_a_path() {
        assert_eq!(host_of("https://gateway.ajila.com/v1").as_deref(), Some("gateway.ajila.com"));
        assert_eq!(host_of("http://localhost:11434/v1").as_deref(), Some("localhost:11434"));
        assert_eq!(host_of("gateway.ajila.com").as_deref(), Some("gateway.ajila.com"));
    }

    #[test]
    fn an_unset_server_is_absent_rather_than_empty() {
        // The caller substitutes "a server you have not set yet"; what must not happen is an
        // empty string reaching the sentence, which would read as reassurance.
        assert_eq!(host_of(""), None);
        assert_eq!(host_of("   "), None);
        assert_eq!(host_of("https://"), None);
    }
}
