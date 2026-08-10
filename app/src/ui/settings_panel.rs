//! Where the generation backend is configured.

use dioxus::prelude::*;
use report_core::settings::{Provider, Settings};

#[component]
pub fn SettingsPanel(settings: Signal<Settings>) -> Element {
    // Saving is explicit. Writing the file on every keystroke would put a partial
    // API key on disk dozens of times while it is being typed.
    let mut saved = use_signal(|| None::<Result<(), String>>);
    let provider = settings.read().provider;

    let mut save = move || {
        let result = settings.read().save().map_err(|e| format!("{e:#}"));
        saved.set(Some(result));
    };

    rsx! {
        div { class: "sp",
            fieldset { class: "sp-group",
                legend { "Generate with" }
                for (value, label, hint) in provider_choices() {
                    label { key: "{label}", class: "sp-radio",
                        input {
                            r#type: "radio",
                            name: "provider",
                            checked: provider == value,
                            onchange: move |_| settings.write().provider = value,
                        }
                        span { class: "sp-radio-label", "{label}" }
                        span { class: "sp-hint", "{hint}" }
                    }
                }
            }

            if provider == Provider::Remote {
                fieldset { class: "sp-group",
                    legend { "OpenAI-compatible server" }

                    Field {
                        label: "Base URL".to_string(),
                        hint: "Anything speaking the OpenAI API — api.openai.com/v1, \
                               localhost:11434/v1 for Ollama, or your own gateway.".to_string(),
                        value: settings.read().openai.base_url.clone(),
                        on_input: move |v| settings.write().openai.base_url = v,
                    }
                    Field {
                        label: "Model".to_string(),
                        hint: "The model id the server expects.".to_string(),
                        value: settings.read().openai.model.clone(),
                        on_input: move |v| settings.write().openai.model = v,
                    }
                    Field {
                        label: "API key".to_string(),
                        // Said plainly rather than hidden: it is the user's call
                        // whether that is acceptable on this machine.
                        hint: "Stored in plain text in the app's data directory. \
                               Leave empty for local servers.".to_string(),
                        value: settings.read().openai.api_key.clone(),
                        secret: true,
                        on_input: move |v| settings.write().openai.api_key = v,
                    }
                    Field {
                        label: "Timeout (s)".to_string(),
                        hint: "A long report on a small local model can take minutes."
                            .to_string(),
                        value: settings.read().openai.timeout_secs.to_string(),
                        on_input: move |v: String| {
                            if let Ok(secs) = v.trim().parse::<u64>() {
                                settings.write().openai.timeout_secs = secs.max(1);
                            }
                        },
                    }
                }
            }

            if provider == Provider::Local {
                fieldset { class: "sp-group",
                    legend { "Local model" }

                    Field {
                        label: "GGUF file".to_string(),
                        hint: "Full path to a quantized model file. There is no downloader \
                               yet — fetch one from Hugging Face and point at it here.".to_string(),
                        value: settings.read().local.model_path.clone(),
                        on_input: move |v| settings.write().local.model_path = v,
                    }
                    Field {
                        label: "Context tokens".to_string(),
                        hint: "The template's instructions and the notes must both fit. \
                               Larger costs memory and prefill time.".to_string(),
                        value: settings.read().local.context_tokens.to_string(),
                        on_input: move |v: String| {
                            if let Ok(tokens) = v.trim().parse::<usize>() {
                                settings.write().local.context_tokens = tokens.max(512);
                            }
                        },
                    }
                    p { class: "sp-hint",
                        "The model runs in a separate process, so a load or a crash never \
                         takes the app down with it. The first generation pays the load \
                         cost; later ones reuse the resident model and its cache."
                    }
                }
            }

            fieldset { class: "sp-group",
                legend { "Dictation" }
                Field {
                    label: "Whisper model".to_string(),
                    hint: "Path to a whisper.cpp ggml model, e.g. ggml-base.bin. \
                           Runs in its own process, separate from the report model."
                        .to_string(),
                    value: settings.read().stt.model_path.clone(),
                    on_input: move |v| settings.write().stt.model_path = v,
                }
                Field {
                    label: "Language".to_string(),
                    hint: "ISO code such as de or en. Leave empty to detect it — a \
                           wrongly forced language produces confident nonsense rather \
                           than a visible error.".to_string(),
                    value: settings.read().stt.language.clone(),
                    on_input: move |v| settings.write().stt.language = v,
                }
            }

            div { class: "sp-actions",
                button { class: "sp-save", onclick: move |_| save(), "Save settings" }
                match &*saved.read() {
                    Some(Ok(())) => rsx! { span { class: "sp-ok", "Saved" } },
                    Some(Err(error)) => rsx! { span { class: "sp-error", "{error}" } },
                    None => rsx! {},
                }
            }
        }
    }
}

/// The providers this build can actually offer.
fn provider_choices() -> Vec<(Provider, &'static str, &'static str)> {
    let mut choices = vec![(
        Provider::Remote,
        "Remote",
        if cfg!(feature = "remote") {
            "Any OpenAI-compatible server."
        } else {
            "Not in this build (compiled without `remote`)."
        },
    )];
    choices.push((
        Provider::Local,
        "Local",
        if cfg!(feature = "inference") {
            "A GGUF on this machine, in a separate process. Nothing leaves the device."
        } else {
            "Not in this build (compiled without `inference`)."
        },
    ));
    choices.push((Provider::Stub, "Stub", "Placeholder text. Exercises the flow without a model."));
    choices
}

#[component]
fn Field(
    label: String,
    hint: String,
    value: String,
    #[props(default)] secret: bool,
    on_input: EventHandler<String>,
) -> Element {
    rsx! {
        label { class: "sp-field",
            span { class: "sp-field-label", "{label}" }
            input {
                r#type: if secret { "password" } else { "text" },
                value: "{value}",
                oninput: move |event| on_input.call(event.value()),
            }
            span { class: "sp-hint", "{hint}" }
        }
    }
}
