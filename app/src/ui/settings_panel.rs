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
use report_core::settings::{Provider, Settings};

use crate::ui::kit::{
    Button, ChoiceCard, Group, Icon, Notice, NoticeKind, PageBody, PageHead, TextField, Variant,
};

#[component]
pub fn SettingsPanel(settings: Signal<Settings>) -> Element {
    // Saving is explicit. Writing the file on every keystroke would put a partial API key
    // on disk dozens of times while it is being typed. (Appearance is the exception, and
    // saves itself from the rail — see `ui::rail`.)
    let mut saved = use_signal(|| None::<Result<(), String>>);
    let provider = settings.read().provider;

    let mut save = move || {
        let result = settings.read().save().map_err(|error| format!("{error:#}"));
        saved.set(Some(result));
    };

    rsx! {
        PageHead {
            title: "Settings".to_string(),
            subtitle: "Where reports get written, and how dictation behaves".to_string(),
            actions: rsx! {
                Button {
                    label: "Save settings".to_string(),
                    variant: Variant::Primary,
                    onclick: move |_| save(),
                }
            },
        }

        PageBody {
            div { class: "set",
                match &*saved.read() {
                    Some(Ok(())) => rsx! { Notice { kind: NoticeKind::Ok, message: "Saved".to_string() } },
                    Some(Err(error)) => rsx! { Notice { kind: NoticeKind::Error, message: error.clone() } },
                    None => rsx! {},
                }

                Group {
                    title: "Where reports are written".to_string(),
                    sub: "This decides whether your notes ever leave this computer.".to_string(),

                    for choice in provider_choices() {
                        ChoiceCard {
                            key: "{choice.title}",
                            group: "provider".to_string(),
                            title: choice.title.to_string(),
                            hint: choice.hint.to_string(),
                            on: provider == choice.provider,
                            disabled: !choice.available,
                            onselect: move |_| settings.write().provider = choice.provider,
                        }
                    }
                }

                Group {
                    title: "The model on this computer".to_string(),
                    sub: "Used when you choose On this computer. Both files are managed for \
                          you — set a path only to use one you already have."
                        .to_string(),
                    TextField {
                        label: "Report model".to_string(),
                        placeholder: "Managed by the app".to_string(),
                        hint: "Full path to a GGUF. Setting it suppresses the download."
                            .to_string(),
                        value: settings.read().local.model_path.clone(),
                        oninput: move |value| settings.write().local.model_path = value,
                    }
                    TextField {
                        label: "Context tokens".to_string(),
                        hint: "The template's instructions and the notes must both fit. Larger \
                               costs memory and prefill time.".to_string(),
                        value: settings.read().local.context_tokens.to_string(),
                        oninput: move |value: String| {
                            if let Ok(tokens) = value.trim().parse::<usize>() {
                                settings.write().local.context_tokens = tokens.max(512);
                            }
                        },
                    }
                }

                Group {
                    title: "The server".to_string(),
                    sub: "Used when you choose Company server. Anything speaking the OpenAI \
                          API — api.openai.com/v1, localhost:11434/v1 for Ollama, or your own \
                          gateway.".to_string(),
                    TextField {
                        label: "Address".to_string(),
                        value: settings.read().openai.base_url.clone(),
                        oninput: move |value| settings.write().openai.base_url = value,
                    }
                    TextField {
                        label: "Model".to_string(),
                        hint: "The model id the server expects.".to_string(),
                        value: settings.read().openai.model.clone(),
                        oninput: move |value| settings.write().openai.model = value,
                    }
                    TextField {
                        label: "Key".to_string(),
                        // Said plainly rather than hidden: it is the user's call whether
                        // that is acceptable on this machine.
                        hint: "Stored in plain text alongside your reports. Leave empty for a \
                               server that wants none.".to_string(),
                        value: settings.read().openai.api_key.clone(),
                        secret: true,
                        oninput: move |value| settings.write().openai.api_key = value,
                    }
                    TextField {
                        label: "Request timeout (seconds)".to_string(),
                        hint: "A long report on a small model can take minutes.".to_string(),
                        value: settings.read().openai.timeout_secs.to_string(),
                        oninput: move |value: String| {
                            if let Ok(seconds) = value.trim().parse::<u64>() {
                                settings.write().openai.timeout_secs = seconds.max(1);
                            }
                        },
                    }
                }

                Group {
                    title: "Dictation".to_string(),
                    sub: "Speech is turned into text on this computer. Recordings are never \
                          uploaded.".to_string(),
                    TextField {
                        label: "Spoken language".to_string(),
                        placeholder: "Detect automatically".to_string(),
                        hint: "An ISO code such as de or en. Leave empty to detect it — a \
                               wrongly forced language produces confident nonsense rather \
                               than a visible error.".to_string(),
                        value: settings.read().stt.language.clone(),
                        oninput: move |value| settings.write().stt.language = value,
                    }
                    TextField {
                        label: "Dictation model".to_string(),
                        placeholder: "Managed by the app".to_string(),
                        hint: "Full path to a whisper.cpp ggml model. Same rule as the report \
                               model: empty means the managed download.".to_string(),
                        value: settings.read().stt.model_path.clone(),
                        oninput: move |value| settings.write().stt.model_path = value,
                    }
                }

                Group {
                    title: "Appearance".to_string(),
                    sub: "The window follows your system unless you choose otherwise. This \
                          one saves itself.".to_string(),
                    Button {
                        label: settings.read().appearance.label().to_string(),
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
                    title: "Your data".to_string(),
                    sub: "Everything you write — reports, templates and these settings — is \
                          one small database file. The models sitting beside it are the large \
                          part, several gigabytes each.".to_string(),
                    Button {
                        label: reveal_label().to_string(),
                        onclick: move |_| {
                            if let Err(error) = reveal_data_dir() {
                                saved.set(Some(Err(format!("{error:#}"))));
                            }
                        },
                    }
                    p { class: "hint",
                        "Two reasons to open it: copying that one file backs up every report \
                         and template, and the models are where the disk space goes if you \
                         need it back."
                    }
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
    title: &'static str,
    hint: &'static str,
    available: bool,
}

/// The three answers to "where does this get written", in the order of least to most
/// exposure — which is also the order someone worried about it would want to read them.
fn provider_choices() -> Vec<Choice> {
    vec![
        Choice {
            provider: Provider::Local,
            title: "On this computer",
            hint: if cfg!(feature = "inference") {
                "Nothing leaves the device. Slower on long reports."
            } else {
                "Not in this build (compiled without `inference`)."
            },
            available: cfg!(feature = "inference"),
        },
        Choice {
            provider: Provider::Remote,
            title: "Company server",
            hint: if cfg!(feature = "remote") {
                "Faster. Your notes are sent to the address under The server, below."
            } else {
                "Not in this build (compiled without `remote`)."
            },
            available: cfg!(feature = "remote"),
        },
        Choice {
            provider: Provider::Stub,
            title: "Example text",
            hint: "Fills the template with placeholder text. For trying things out.",
            available: true,
        },
    ]
}

fn reveal_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Show in Finder"
    } else if cfg!(target_os = "windows") {
        "Show in Explorer"
    } else {
        "Show files"
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
            title: "Developer · backend".to_string(),
            sub: "Only in a debug build. What the model is actually sent.".to_string(),
            pre { class: "dev-dump", "{settings.read().describe()}" }
        }
        Group {
            title: "Developer · system prompt".to_string(),
            sub: "Built from the open report's template. The only route a field description \
                  has to a locally generated report."
                .to_string(),
            pre { class: "dev-dump", "{report_core::prompt::system(&template.read())}" }
        }
        Group {
            title: "Developer · JSON schema".to_string(),
            sub: "What a remote server is constrained by. The local grammar is compiled from \
                  the same traversal."
                .to_string(),
            pre { class: "dev-dump", "{schema_preview(&template.read())}" }
        }
    }
}

fn schema_preview(template: &report_core::Template) -> String {
    let schema = report_core::compile::Shape::compile(template).to_json_schema();
    serde_json::to_string_pretty(&schema).unwrap_or_default()
}
