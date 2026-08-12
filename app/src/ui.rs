//! The app's own UI: everything that knows what a template and a report are.
//!
//! [`kit`] is the exception and the reason the rest reads cleanly — it holds the shared
//! Aperture pieces and is forbidden from knowing any of those types.

pub mod confirm;
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
mod render_tests {
    //! Every screen, drawn in every language.
    //!
    //! A real [`VirtualDom`] rather than a snapshot: building one runs each component's body
    //! for real, which means every `t!` in it is actually resolved. That is the whole point —
    //! `t!` panics on a key the catalogue does not have, and Fluent's own failures (an
    //! argument a message never declares, a selector with no matching variant) come back as
    //! errors that `t!` turns into a panic too.
    //!
    //! ## Why this reads the log rather than expecting a panic
    //!
    //! `dioxus-core` wraps every component render in `catch_unwind` — see `any_props.rs` —
    //! and converts a panic into `Element::Err` plus a `tracing::error!("Panic while
    //! rendering component …")`. So a broken translation does *not* unwind out of
    //! `rebuild_in_place`, and a test that merely called it would pass no matter what: the
    //! first draft of this one did, with a key that existed in no catalogue at all.
    //!
    //! The rebuild therefore runs under a subscriber that captures `ERROR`, and the assertion
    //! is that nothing was logged. `with_default` scopes the subscriber to this thread, so
    //! tests still run in parallel.
    //!
    //! Nothing here asserts on the markup. What the words *are* is the catalogue's business
    //! and is pinned by the tests in [`crate::i18n`]; what this covers is that every screen
    //! asks only for words that exist, in all four languages.

    use dioxus::prelude::*;
    use report_core::settings::{Language, Locale, Provider, Settings, Spoken, Theme};
    use report_core::store::Summary;
    use report_doc::RichDoc;
    use report_editor::EditorRuntime;

    use crate::dictate::use_dictation;
    use crate::generate::Status;
    use crate::models::ModelStatus;
    use crate::ui::kit::{List, Row};
    use crate::ui::template_picker::TemplatePicker;
    use crate::ui::{
        starter_template, EditorScreen, Rail, ReportsScreen, Screen, SettingsPanel,
        TemplateBuilder, Workspace,
    };

    /// Draw everything, in the language `settings` asks for.
    ///
    /// One component rather than one per screen, because the screens are cheap to build and a
    /// single tree means a single `VirtualDom` per language.
    #[component]
    fn EveryScreen(initial: Settings, has_report: bool) -> Element {
        // The signal is created here rather than handed in as a prop: a `Signal` may only be
        // made inside a running Dioxus runtime, and the test harness is outside one.
        let settings = use_signal(move || initial.clone());
        let locale = crate::i18n::use_app_i18n(settings);

        let template = use_signal(starter_template);
        let workspace = Workspace {
            template,
            notes: use_signal(RichDoc::empty_paragraph),
            generated: use_signal(RichDoc::empty_paragraph),
            has_report: use_signal(move || has_report),
            report_id: use_signal(|| None),
            report_name: use_signal(|| "March visit".to_string()),
            // A timestamp old enough to render as a date, which is the `time-month-*` path.
            written_at: use_signal(|| Some(1_700_000_000)),
            revision: use_signal(|| 0),
        };
        let screen = use_signal(|| Screen::Reports);

        // The developer group in Settings reads the report's template through context.
        use_context_provider(|| template);

        let dictation = use_dictation(workspace.notes, settings);
        let downloads = use_signal(|| {
            // Every stage, so each `models-stage-*` key is asked for.
            vec![
                ModelStatus { name: "Whisper", stage: crate::models::Stage::Waiting },
                ModelStatus {
                    name: "Gemma",
                    stage: crate::models::Stage::Fetching {
                        downloaded: 2_100_000_000,
                        total: Some(5_150_000_000),
                    },
                },
                ModelStatus {
                    name: "Unknown total",
                    stage: crate::models::Stage::Fetching { downloaded: 1_000, total: None },
                },
            ]
        });

        let summaries = vec![Summary {
            id: uuid::Uuid::new_v4(),
            name: "March visit".to_string(),
            updated: 1_700_000_000,
            template_name: "Site inspection".to_string(),
            generated: true,
            fields: 3,
        }];

        rsx! {
            // `lang` is what the webview's spellchecker reads; asserted only in as much as it
            // has to be a value `Locale::tag` produced.
            div { lang: locale.tag(),
                EditorRuntime {
                    Rail { screen, settings }
                    ReportsScreen { workspace, screen, starter: starter_template() }
                    EditorScreen {
                        workspace,
                        dictation: dictation.clone(),
                        downloads,
                        status: use_signal(|| Status::Failed("boom".to_string())),
                        save_state: use_signal(|| crate::ui::workspace::SaveState::Saved),
                        on_generate: move |_| {},
                    }
                    TemplateBuilder { template }
                    SettingsPanel { settings }
                    TemplatePicker {
                        templates: summaries.clone(),
                        title: "t".to_string(),
                        subtitle: "s".to_string(),
                        allow_none: true,
                        on_pick: move |_| {},
                        on_cancel: move |_| {},
                    }
                    // The rows the library draws, which the screens above cannot reach without
                    // a database: both tags, and a humanised timestamp.
                    List {
                        for summary in summaries.iter() {
                            Row {
                                key: "{summary.id}",
                                name: summary.name.clone(),
                                from: summary.template_name.clone(),
                                tag: Some(("Final".to_string(), false)),
                                when: crate::i18n::relative_time(summary.updated),
                                onopen: move |_| {},
                                ondelete: move |_| {},
                            }
                        }
                    }
                }
            }
        }
    }

    /// A `MakeWriter` that collects into a buffer this thread can read afterwards.
    #[derive(Clone, Default)]
    struct Captured(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for Captured {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("only this thread writes").extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
        type Writer = Self;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    /// Build the whole tree once, and return whatever was logged at `ERROR` while doing it.
    fn draw(language: Language, provider: Provider, has_report: bool) -> String {
        let settings = Settings {
            language,
            provider,
            appearance: Theme::Dark,
            // Named rather than default, so the dictation picker's `Fixed` arm is drawn too.
            stt: report_core::settings::SttConfig {
                spoken: Spoken::Fixed(Locale::Italian),
                ..Default::default()
            },
            ..Default::default()
        };

        let captured = Captured::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(captured.clone())
            .with_max_level(tracing::Level::ERROR)
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            let mut dom = VirtualDom::new_with_props(
                EveryScreen,
                EveryScreenProps { initial: settings, has_report },
            );
            dom.rebuild_in_place();
        });

        let logged = captured.0.lock().expect("the rebuild is finished").clone();
        String::from_utf8_lossy(&logged).to_string()
    }

    #[test]
    fn every_screen_draws_in_every_language() {
        for language in [
            Language::System,
            Language::German,
            Language::English,
            Language::French,
            Language::Italian,
        ] {
            // Every privacy line in the rail, and both halves of the report pane — each arm
            // asks for a different set of keys.
            for provider in [Provider::Local, Provider::Remote, Provider::Stub] {
                for has_report in [false, true] {
                    let logged = draw(language, provider, has_report);
                    assert!(
                        logged.is_empty(),
                        "drawing {language:?} / {provider:?} / has_report={has_report} logged:\n\
                         {logged}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_broken_translation_would_be_noticed() {
        // Guards the guard. `dioxus-core` swallows a panic in a component, so without this
        // there is nothing to say the test above is checking anything at all — and in its
        // first draft it was not.
        //
        // `Template::new` is used rather than a screen, because what has to be provoked is a
        // failing `t!` inside a component render, and the shortest one is a key that no
        // catalogue defines.
        #[component]
        fn AsksForNothing() -> Element {
            let settings = use_signal(Settings::default);
            crate::i18n::use_app_i18n(settings);
            // Assembled rather than written as a literal, so that
            // `i18n::tests::every_key_the_code_asks_for_exists` — which scans the source for
            // `t!("…")` — does not find this one and report it as a real missing key.
            let key = format!("no-catalogue-{}", "defines-this");
            let missing = crate::i18n::t!(&key);
            rsx! { "{missing}" }
        }

        let captured = Captured::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(captured.clone())
            .with_max_level(tracing::Level::ERROR)
            .finish();
        // The default hook would print the caught panic's backtrace to stderr and make a
        // passing test look like a failing one.
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        tracing::subscriber::with_default(subscriber, || {
            VirtualDom::new(AsksForNothing).rebuild_in_place();
        });
        std::panic::set_hook(hook);

        let logged = String::from_utf8_lossy(&captured.0.lock().unwrap().clone()).to_string();
        // The message and not the key: dioxus logs the payload as `{err:?}` on a
        // `Box<dyn Any>`, which prints `Any { .. }` and never the panic's text. Which is also
        // why the test above asserts an *empty* log rather than looking for a key by name.
        assert!(
            logged.contains("Panic while rendering component"),
            "a missing key has to reach the log, or the test above proves nothing: {logged:?}"
        );
    }
}

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
