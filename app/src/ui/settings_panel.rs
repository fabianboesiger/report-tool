//! Settings, phrased as the question the user actually has.
//!
//! The old panel offered `Remote` / `Local` / `Stub` and put `context_tokens`,
//! `timeout_secs` and two model paths on the surface. None of that is the decision anyone
//! is making. The decision is **do my notes leave this computer**, so that is what the
//! three choices say.
//!
//! ## Every section is the same shape
//!
//! One `Group` per section: a heading, a sentence saying what the section is for, then its
//! fields. Nothing collapses. Half of this used to sit behind `Server details` and
//! `Advanced` disclosures, which made a short page look shorter while hiding the fields
//! most likely to be wrong when something fails — a timeout, a model path, an address. A
//! settings page of eight fields does not need progressive disclosure; it needs the fields
//! grouped by which decision they belong to.

use dioxus::prelude::*;
use report_core::settings::{Language, Locale, Provider, Settings, Spoken};

use crate::i18n::t;
use crate::ui::kit::{
    Button, ChoiceCard, Group, Icon, Notice, NoticeKind, PageBody, PageHead, Select, TextField,
    Variant,
};

#[component]
pub fn SettingsPanel(settings: Signal<Settings>) -> Element {
    // Saving is explicit. Writing the file on every keystroke would put a partial API key
    // on disk dozens of times while it is being typed. (Appearance and language are the
    // exceptions, and each says why where it saves itself.)
    let mut saved = use_signal(|| None::<Result<(), String>>);
    let provider = settings.read().provider;
    let locale = settings.read().locale();

    let mut save = move || {
        let result = settings.read().save().map_err(|error| format!("{error:#}"));
        saved.set(Some(result));
    };

    rsx! {
        PageHead {
            title: t!("settings-title"),
            subtitle: t!("settings-subtitle"),
            actions: rsx! {
                Button {
                    label: t!("settings-save"),
                    variant: Variant::Primary,
                    onclick: move |_| save(),
                }
            },
        }

        PageBody {
            div { class: "set",
                match &*saved.read() {
                    Some(Ok(())) => rsx! { Notice { kind: NoticeKind::Ok, message: t!("settings-saved") } },
                    Some(Err(error)) => rsx! { Notice { kind: NoticeKind::Error, message: error.clone() } },
                    None => rsx! {},
                }

                // First on the page, because it is the one setting that changes everything
                // else on it — including the words the other groups are written in.
                Group {
                    title: t!("settings-language-title"),
                    sub: t!("settings-language-sub"),
                    Select {
                        label: t!("settings-language-label"),
                        options: language_options(),
                        value: language_token(settings.read().language).to_string(),
                        onchange: move |token: String| {
                            let Some(language) = language_from_token(&token) else { return };
                            settings.write().language = language;
                            // Saved on the spot, for the same reason appearance is: the whole
                            // window is in the new language the instant this changes, and
                            // leaving that unsaved would mean the app reads as changed and is
                            // not. Picking a language is also a complete decision the moment
                            // it is made, which a half-typed API key is not.
                            if let Err(error) = settings.read().save() {
                                tracing::warn!("settings: could not persist language: {error:#}");
                            }
                        },
                    }
                }

                Group {
                    title: t!("settings-provider-title"),
                    sub: t!("settings-provider-sub"),

                    for choice in provider_choices() {
                        ChoiceCard {
                            key: "{choice.key}",
                            group: "provider".to_string(),
                            title: choice.title,
                            hint: choice.hint,
                            on: provider == choice.provider,
                            disabled: !choice.available,
                            onselect: move |_| settings.write().provider = choice.provider,
                        }
                    }
                }

                Group {
                    title: t!("settings-local-title"),
                    sub: t!("settings-local-sub"),
                    TextField {
                        label: t!("settings-local-model"),
                        placeholder: t!("settings-local-managed"),
                        hint: t!("settings-local-model-hint"),
                        value: settings.read().local.model_path.clone(),
                        oninput: move |value| settings.write().local.model_path = value,
                    }
                    TextField {
                        label: t!("settings-local-context"),
                        hint: t!("settings-local-context-hint"),
                        value: settings.read().local.context_tokens.to_string(),
                        oninput: move |value: String| {
                            if let Ok(tokens) = value.trim().parse::<usize>() {
                                settings.write().local.context_tokens = tokens.max(512);
                            }
                        },
                    }
                }

                Group {
                    title: t!("settings-server-title"),
                    sub: t!("settings-server-sub"),
                    TextField {
                        label: t!("settings-server-address"),
                        value: settings.read().openai.base_url.clone(),
                        oninput: move |value| settings.write().openai.base_url = value,
                    }
                    TextField {
                        label: t!("settings-server-model"),
                        hint: t!("settings-server-model-hint"),
                        value: settings.read().openai.model.clone(),
                        oninput: move |value| settings.write().openai.model = value,
                    }
                    TextField {
                        label: t!("settings-server-key"),
                        // Said plainly rather than hidden: it is the user's call whether
                        // that is acceptable on this machine.
                        hint: t!("settings-server-key-hint"),
                        value: settings.read().openai.api_key.clone(),
                        secret: true,
                        oninput: move |value| settings.write().openai.api_key = value,
                    }
                    TextField {
                        label: t!("settings-server-timeout"),
                        hint: t!("settings-server-timeout-hint"),
                        value: settings.read().openai.timeout_secs.to_string(),
                        oninput: move |value: String| {
                            if let Ok(seconds) = value.trim().parse::<u64>() {
                                settings.write().openai.timeout_secs = seconds.max(1);
                            }
                        },
                    }
                }

                Group {
                    title: t!("settings-dictation-title"),
                    sub: t!("settings-dictation-sub"),
                    Select {
                        label: t!("settings-spoken-label"),
                        hint: t!("settings-spoken-hint"),
                        options: spoken_options(locale),
                        value: spoken_token(settings.read().stt.spoken),
                        onchange: move |token: String| {
                            if let Some(spoken) = spoken_from_token(&token) {
                                settings.write().stt.spoken = spoken;
                            }
                        },
                    }
                    TextField {
                        label: t!("settings-dictation-model"),
                        placeholder: t!("settings-local-managed"),
                        hint: t!("settings-dictation-model-hint"),
                        value: settings.read().stt.model_path.clone(),
                        oninput: move |value| settings.write().stt.model_path = value,
                    }
                }

                Group {
                    title: t!("settings-appearance-title"),
                    sub: t!("settings-appearance-sub"),
                    Button {
                        label: appearance_label(settings.read().appearance),
                        icon: Icon::Moon,
                        onclick: move |_| {
                            let next = settings.read().appearance.next();
                            settings.write().appearance = next;
                            // Saved on the spot, unlike everything else here. The window
                            // repaints the instant this is clicked, so leaving it unsaved
                            // until the button at the top would mean the app looks changed
                            // and is not — and picking a theme is a complete decision the
                            // moment it is made, which a half-typed API key is not.
                            if let Err(error) = settings.read().save() {
                                tracing::warn!("settings: could not persist appearance: {error:#}");
                            }
                        },
                    }
                }

                Group {
                    title: t!("settings-data-title"),
                    sub: t!("settings-data-sub"),
                    Button {
                        label: reveal_label(),
                        onclick: move |_| {
                            if let Err(error) = reveal_data_dir() {
                                saved.set(Some(Err(format!("{error:#}"))));
                            }
                        },
                    }
                    p { class: "hint", {t!("settings-data-hint")} }
                }

                if cfg!(debug_assertions) {
                    DeveloperGroup { settings }
                }
            }
        }
    }
}

/// A provider, said as a place rather than as a protocol.
struct Choice {
    provider: Provider,
    /// A stable key for the rsx loop. Not the title: that is now translated, and keying a
    /// list on text that changes with the language would remount all three cards on every
    /// language switch.
    key: &'static str,
    title: String,
    hint: String,
    available: bool,
}

/// The three answers to "where does this get written", in the order of least to most
/// exposure — which is also the order someone worried about it would want to read them.
fn provider_choices() -> Vec<Choice> {
    vec![
        Choice {
            provider: Provider::Local,
            key: "local",
            title: t!("settings-provider-local-title"),
            hint: if cfg!(feature = "inference") {
                t!("settings-provider-local-hint")
            } else {
                t!("settings-provider-local-absent")
            },
            available: cfg!(feature = "inference"),
        },
        Choice {
            provider: Provider::Remote,
            key: "remote",
            title: t!("settings-provider-remote-title"),
            hint: if cfg!(feature = "remote") {
                t!("settings-provider-remote-hint")
            } else {
                t!("settings-provider-remote-absent")
            },
            available: cfg!(feature = "remote"),
        },
        Choice {
            provider: Provider::Stub,
            key: "stub",
            title: t!("settings-provider-stub-title"),
            hint: t!("settings-provider-stub-hint"),
            available: true,
        },
    ]
}

/// The appearance button's label, which doubles as its current value.
///
/// One cycling button rather than three radios: appearance is not worth a settings group of
/// its own, cycling means the label always states what you have, and pressing three times
/// returns you to where you started.
fn appearance_label(theme: report_core::settings::Theme) -> String {
    use report_core::settings::Theme;
    match theme {
        Theme::System => t!("settings-appearance-system"),
        Theme::Light => t!("settings-appearance-light"),
        Theme::Dark => t!("settings-appearance-dark"),
    }
}

/// The language picker's options.
///
/// Endonyms, so the row you are looking for is legible even when the interface is in a
/// language you cannot read — which is exactly the situation someone opening this control is
/// most likely to be in. Swiss order: the three official languages, then English.
fn language_options() -> Vec<(String, String)> {
    let mut options = vec![(
        SYSTEM_TOKEN.to_string(),
        t!("settings-language-system", endonym: Language::System.resolve().endonym()),
    )];
    options.extend(Locale::ALL.into_iter().map(|locale| {
        // The endonym is its own label: translating "German" into four languages would give
        // four strings for one row, none of which a German speaker needs.
        (locale.tag().to_string(), locale.endonym().to_string())
    }));
    options
}

/// The token that stands for "follow the system".
///
/// Not a language tag, so it cannot collide with one, and not a translated string — Fluent
/// wraps interpolations in invisible isolates, so a round-tripped label would stop comparing
/// equal to the value that produced it.
const SYSTEM_TOKEN: &str = "system";

fn language_token(language: Language) -> &'static str {
    match language {
        Language::System => SYSTEM_TOKEN,
        Language::German => "de",
        Language::English => "en",
        Language::French => "fr",
        Language::Italian => "it",
    }
}

fn language_from_token(token: &str) -> Option<Language> {
    match token {
        SYSTEM_TOKEN => Some(Language::System),
        "de" => Some(Language::German),
        "en" => Some(Language::English),
        "fr" => Some(Language::French),
        "it" => Some(Language::Italian),
        // A value the `<select>` cannot have produced. Ignored rather than defaulted, so a
        // future option added to the markup and forgotten here does nothing visible instead
        // of silently resetting the language.
        _ => None,
    }
}

/// The dictation picker's options: the app's language, whisper's own detection, then each
/// language by name.
fn spoken_options(app: Locale) -> Vec<(String, String)> {
    let mut options = vec![
        (APP_TOKEN.to_string(), t!("settings-spoken-app", endonym: app.endonym())),
        (DETECT_TOKEN.to_string(), t!("settings-spoken-detect")),
    ];
    options.extend(
        Locale::ALL
            .into_iter()
            .map(|locale| (locale.tag().to_string(), locale.endonym().to_string())),
    );
    options
}

const APP_TOKEN: &str = "app";
const DETECT_TOKEN: &str = "detect";

fn spoken_token(spoken: Spoken) -> String {
    match spoken {
        Spoken::App => APP_TOKEN.to_string(),
        Spoken::Detect => DETECT_TOKEN.to_string(),
        Spoken::Fixed(locale) => locale.tag().to_string(),
    }
}

fn spoken_from_token(token: &str) -> Option<Spoken> {
    match token {
        APP_TOKEN => Some(Spoken::App),
        DETECT_TOKEN => Some(Spoken::Detect),
        tag => Locale::from_tag(tag).map(Spoken::Fixed),
    }
}

fn reveal_label() -> String {
    if cfg!(target_os = "macos") {
        t!("settings-reveal-finder")
    } else if cfg!(target_os = "windows") {
        t!("settings-reveal-explorer")
    } else {
        t!("settings-reveal-files")
    }
}

/// Open the data directory in the platform's file manager.
///
/// `rfd` has no reveal call, so this is the one place the app shells out.
fn reveal_data_dir() -> anyhow::Result<()> {
    let dir = report_core::paths::data_dir()?;
    let command = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    // Spawned, not waited on: `open` returns immediately but `xdg-open` may not, and
    // blocking the UI thread on a file manager would freeze the window.
    std::process::Command::new(command).arg(&dir).spawn()?;
    Ok(())
}

/// The prompt and schema views, which used to occupy half the window.
///
/// Kept rather than deleted, and given the developer flag they always wanted: a template
/// description reaches a locally generated report by no other route — GBNF cannot carry
/// descriptions — so anything missing from the prompt is otherwise invisible. A release
/// build never renders this.
#[component]
fn DeveloperGroup(settings: Signal<Settings>) -> Element {
    let template = use_context::<Signal<report_core::Template>>();

    rsx! {
        // Three sections rather than three collapsibles inside one. Each dump already
        // scrolls at 300px, so flattening them costs no more page than the summaries did.
        Group {
            title: t!("settings-dev-backend-title"),
            sub: t!("settings-dev-backend-sub"),
            pre { class: "dev-dump", "{settings.read().describe()}" }
        }
        Group {
            title: t!("settings-dev-prompt-title"),
            sub: t!("settings-dev-prompt-sub"),
            // The locale too, so the dump shows the language line the model actually gets
            // rather than a prompt nobody sends.
            pre { class: "dev-dump",
                "{report_core::prompt::system(&template.read(), settings.read().locale())}"
            }
        }
        Group {
            title: t!("settings-dev-schema-title"),
            sub: t!("settings-dev-schema-sub"),
            pre { class: "dev-dump", "{schema_preview(&template.read())}" }
        }
    }
}

fn schema_preview(template: &report_core::Template) -> String {
    let schema = report_core::compile::Shape::compile(template).to_json_schema();
    serde_json::to_string_pretty(&schema).unwrap_or_default()
}
