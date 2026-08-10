//! The left rail: where you are, and where the writing happens.

use dioxus::prelude::*;
use report_core::settings::{Provider, Settings};

use crate::ui::kit::{Glyph, Icon, NavLink};
use crate::ui::Screen;

#[component]
pub fn Rail(screen: Signal<Screen>, settings: Signal<Settings>) -> Element {
    let provider = settings.read().provider;
    let (where_title, where_detail) = where_line(provider, &settings.read());
    let appearance = settings.read().appearance;

    rsx! {
        aside { class: "rail",
            div { class: "brand",
                Glyph { icon: Icon::Aperture }
                "Report tool"
            }

            nav { class: "nav",
                NavLink {
                    icon: Icon::Document,
                    label: "Reports".to_string(),
                    active: screen() == Screen::Reports,
                    onclick: move |_| screen.set(Screen::Reports),
                }
                NavLink {
                    icon: Icon::Pencil,
                    label: "Current report".to_string(),
                    active: screen() == Screen::Editor,
                    onclick: move |_| screen.set(Screen::Editor),
                }
                div { class: "nav-label", "Set up" }
                NavLink {
                    icon: Icon::Layout,
                    label: "Templates".to_string(),
                    active: screen() == Screen::Templates,
                    onclick: move |_| screen.set(Screen::Templates),
                }
                NavLink {
                    icon: Icon::Cog,
                    label: "Settings".to_string(),
                    active: screen() == Screen::Settings,
                    onclick: move |_| screen.set(Screen::Settings),
                }
            }

            div { class: "where",
                b { "{where_title}" }
                "{where_detail}"
            }

            div { class: "rail-foot",
                button {
                    r#type: "button",
                    onclick: move |_| {
                        let next = settings.read().appearance.next();
                        settings.write().appearance = next;
                        // Written straight away, unlike everything in the settings panel.
                        // Picking a theme is a complete decision the moment it is made; a
                        // half-typed API key is not.
                        if let Err(error) = settings.read().save() {
                            tracing::warn!("settings: could not persist appearance: {error:#}");
                        }
                    },
                    Glyph { icon: Icon::Moon }
                    "{appearance.label()}"
                }
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
fn where_line(provider: Provider, settings: &Settings) -> (&'static str, String) {
    match provider {
        Provider::Local => ("Writing on this computer", "Nothing leaves the device.".to_string()),
        Provider::Remote => (
            "Writing on a server",
            format!("Your notes are sent to {}.", host_of(&settings.openai.base_url)),
        ),
        Provider::Stub => ("Example text only", "No model is being used yet.".to_string()),
    }
}

/// The host part of a URL, so the line names somewhere recognisable rather than a path.
fn host_of(url: &str) -> String {
    let host = url
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("")
        .trim();
    if host.is_empty() {
        "a server you have not set yet".to_string()
    } else {
        host.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_privacy_line_names_where_the_notes_go() {
        let mut settings = Settings::default();

        settings.provider = Provider::Local;
        assert_eq!(where_line(Provider::Local, &settings).0, "Writing on this computer");

        settings.provider = Provider::Remote;
        settings.openai.base_url = "https://gateway.ajila.com/v1".into();
        let (_, detail) = where_line(Provider::Remote, &settings);
        assert!(detail.contains("gateway.ajila.com"), "{detail}");
        assert!(!detail.contains("/v1"), "a path is not a place: {detail}");
    }

    #[test]
    fn an_unset_server_says_so_rather_than_naming_nothing() {
        // "Your notes are sent to ." would read as a bug, and worse, as reassurance.
        let mut settings = Settings::default();
        settings.openai.base_url = String::new();
        let (_, detail) = where_line(Provider::Remote, &settings);
        assert!(detail.contains("not set"), "{detail}");
    }
}
