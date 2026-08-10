//! The app's own UI: everything that knows what a template and a report are.
//!
//! [`kit`] is the exception and the reason the rest reads cleanly — it holds the shared
//! Aperture pieces and is forbidden from knowing any of those types.

pub mod editor;
pub mod kit;
pub mod rail;
pub mod reports;
pub mod settings_panel;
pub mod template_builder;
pub mod template_picker;
pub mod templates;
pub mod workspace;

pub use editor::EditorScreen;
pub use rail::Rail;
pub use reports::ReportsScreen;
pub use settings_panel::SettingsPanel;
pub use template_builder::TemplateBuilder;
pub use templates::{starter_template, TemplatesScreen};
pub use workspace::{use_autosave, Workspace};

/// Which screen the main column shows.
///
/// A signal rather than `dioxus-router`, even though the router feature is already pulled
/// in. A router's model is URLs and history, and there is neither here; what it would add
/// is a `Routable` derive, and what it would cost is control over where the workspace
/// signals live — a route change remounts its subtree, so the notes and the report would
/// have to be hoisted out of it to survive navigation anyway. Once they are hoisted, the
/// router is doing nothing a `Copy` enum in a signal was not already doing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    /// The library. What the app opens on, because the question at launch is nearly always
    /// "carry on with yesterday's", not "start something".
    Reports,
    /// Notes and report, side by side. The screen the app exists for.
    Editor,
    Templates,
    Settings,
}

/// App chrome and template-outline styling.
///
/// The editor's own stylesheet is injected separately by
/// `report_editor::EditorRuntime`; this only covers what the app draws around it. The two
/// share one palette — the tokens declared here are the ones the editor's `--rt-*`
/// properties resolve against.
pub const CSS: &str = include_str!("../assets/app.css");

#[cfg(test)]
mod tests {
    use report_core::settings::Theme;

    #[test]
    fn every_theme_has_a_rule_in_the_stylesheet() {
        // The enum and the stylesheet are two halves of one mechanism, and nothing else
        // would notice a `data-theme` value the CSS has no selector for — the app would
        // simply render light and nobody would know why.
        for theme in [Theme::System, Theme::Light, Theme::Dark] {
            let selector = format!("[data-theme=\"{}\"]", theme.attribute());
            assert!(super::CSS.contains(&selector), "no rule for {selector} in app.css");
        }
    }

    #[test]
    fn the_stylesheet_carries_no_leftover_class_prefixes() {
        // The old per-module prefixes. Each one that survives is a rule with no markup
        // left to style, and the redesign is only finished when none do.
        for stale in [
            ".app-bar",
            ".app-tab",
            ".app-pane",
            ".app-markdown",
            ".tb-node",
            ".sp-group",
            ".lib-btn",
            ".dl-row",
        ] {
            assert!(!super::CSS.contains(stale), "{stale} outlived the markup it styled");
        }
    }
}
