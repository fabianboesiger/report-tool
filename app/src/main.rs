//! Report tool — desktop entry point.

mod audio;
mod dictate;
mod generate;
mod models;
mod ui;

use dioxus::prelude::*;
use report_core::settings::Settings;
use report_core::template::Template;
use report_doc::RichDoc;
use report_editor::{doc_to_markdown, Editor, EditorRuntime};

use crate::dictate::use_dictation;
use crate::generate::{generate, Status};
use crate::models::{use_model_downloads, ModelBanner};
use crate::ui::{LibraryBar, SettingsPanel, TemplateBuilder, Workspace};

fn main() {
    tracing_subscriber::fmt()
        // **stderr, not stdout.** `fmt()` defaults to stdout, and in an inference
        // worker stdout *is* the protocol channel — every log line would land in the
        // middle of the JSON the parent is parsing.
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                // The C++ engines are pinned to `warn`: loading a model emits a line
                // per tensor at info, which buries everything else. `RUST_LOG` still
                // overrides this when a model will not load and you need to see why.
                // The C++ engines are pinned to `warn`: loading a model emits a line
                // per tensor at info, which buries everything else. `RUST_LOG` still
                // overrides this when a model will not load and you need to see why.
                //
                // `llama-cpp-2` is hyphenated because that is the literal target the
                // crate compiles into its log metadata — `llama_cpp_2` matches
                // nothing, which is exactly the mistake that let the noise through.
                .unwrap_or_else(|_| {
                    "info,report_core=debug,report_editor=debug,\
                     llama-cpp-2=warn,whisper_rs=warn"
                        .into()
                }),
        )
        .init();

    // Before anything else: if this process was started as an inference worker it
    // must serve the pipe and never open a window. `run_child` does not return.
    report_core::worker::take_over_if_worker();

    dioxus::launch(App);
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Template,
    Notes,
    Report,
    Settings,
}

#[component]
fn App() -> Element {
    // Resolved once at startup so the chosen backend appears in the log before any
    // model is loaded; a wrong choice here is otherwise only visible as slowness.
    use_hook(|| match report_core::gpu::select() {
        Ok(backend) => tracing::info!("compute backend: {}", backend.as_str()),
        Err(error) => tracing::error!("gpu: {error}"),
    });

    let mut tab = use_signal(|| Tab::Template);
    let settings = use_signal(Settings::load);
    let template = use_signal(starter_template);
    let notes = use_signal(|| {
        report_editor::doc_from_markdown(
            "north wall: hairline cracking below the second window, roughly 30 cm long\n\n\
             roof: two slipped tiles above the west gable\n\n\
             tenant mentioned damp after heavy rain; wants a follow-up before winter\n",
        )
    });

    // Starts on open and resumes an interrupted download from where it stopped.
    let downloads = use_model_downloads(settings);
    let dictation = use_dictation(notes, settings);
    let report = use_signal(RichDoc::empty_paragraph);
    let mut has_report = use_signal(|| false);
    let mut status = use_signal(|| Status::Idle);

    let workspace = Workspace {
        template,
        notes,
        generated: report,
        has_report,
        report_id: use_signal(|| None),
        report_name: use_signal(|| "Untitled report".to_string()),
    };

    let mut run = move || {
        let template = template.read().clone();
        let notes_markdown = doc_to_markdown(&notes.read());
        let settings = settings.read().clone();
        let mut report = report;

        status.set(Status::Running);
        spawn(async move {
            match generate(template, notes_markdown, settings).await {
                Ok(document) => {
                    report.set(document);
                    has_report.set(true);
                    status.set(Status::Done);
                    tab.set(Tab::Report);
                }
                // The whole chain, so the message names the cause rather than the
                // symptom — "Incorrect API key" instead of "request failed".
                Err(error) => status.set(Status::Failed(format!("{error:#}"))),
            }
        });
    };

    let running = status.read().is_running();
    let backend = settings.read().describe();

    rsx! {
        style { dangerous_inner_html: crate::ui::CSS }
        EditorRuntime {
            main { class: "app",
                header { class: "app-bar",
                    h1 { "Report tool" }
                    for (value, label) in [
                        (Tab::Template, "Template"),
                        (Tab::Notes, "Notes"),
                        (Tab::Report, "Report"),
                        (Tab::Settings, "Settings"),
                    ] {
                        button {
                            key: "{label}",
                            class: if tab() == value { "app-tab is-active" } else { "app-tab" },
                            onclick: move |_| tab.set(value),
                            "{label}"
                        }
                    }

                    span { class: "app-spacer" }
                    span { class: "app-backend", "{backend}" }
                    button {
                        class: "app-generate",
                        disabled: running,
                        onclick: move |_| run(),
                        if running { "Generating…" } else { "Generate report" }
                    }
                }

                LibraryBar { workspace }
                ModelBanner { statuses: downloads }

                if let Some(error) = status.read().message() {
                    div { class: "app-error",
                        strong { "Generation failed. " }
                        "{error}"
                    }
                }

                div { class: "app-panes",
                    section { class: "app-pane",
                        match tab() {
                            Tab::Template => rsx! {
                                h2 { class: "app-pane-title", "Template" }
                                div { class: "app-scroll", TemplateBuilder { template } }
                            },
                            Tab::Notes => rsx! {
                                div { class: "app-pane-head",
                                    h2 { class: "app-pane-title", "Notes" }
                                    button {
                                        class: if dictation.state.read().is_recording() {
                                            "app-record is-recording"
                                        } else {
                                            "app-record"
                                        },
                                        disabled: matches!(*dictation.state.read(), crate::dictate::Dictation::Transcribing),
                                        onclick: {
                                            let dictation = dictation.clone();
                                            move |_| dictation.toggle()
                                        },
                                        match &*dictation.state.read() {
                                            crate::dictate::Dictation::Recording => "■ Stop",
                                            crate::dictate::Dictation::Transcribing => "Transcribing…",
                                            _ => "● Dictate",
                                        }
                                    }
                                }
                                if let Some(error) = dictation.state.read().message() {
                                    p { class: "app-dictate-error", "{error}" }
                                }
                                Editor { doc: notes, placeholder: "Jot down what you saw…" }
                            },
                            Tab::Report => rsx! {
                                h2 { class: "app-pane-title", "Report" }
                                if has_report() {
                                    Editor { doc: report, placeholder: "" }
                                } else {
                                    p { class: "app-empty",
                                        "No report yet. Press "
                                        strong { "Generate report" }
                                        " to build one from the template and your notes."
                                    }
                                }
                            },
                            Tab::Settings => rsx! {
                                h2 { class: "app-pane-title", "Settings" }
                                div { class: "app-scroll", SettingsPanel { settings } }
                            },
                        }
                    }

                    section { class: "app-pane",
                        match tab() {
                            // The prompt the template produces, shown live because it
                            // is the only route a description has to a locally
                            // generated report: GBNF cannot carry descriptions, so
                            // anything missing here is invisible to the model.
                            Tab::Template => rsx! {
                                h2 { class: "app-pane-title", "System prompt" }
                                pre { class: "app-markdown",
                                    "{report_core::prompt::system(&template.read())}"
                                }
                            },
                            // The document model, not the DOM: this is what export
                            // writes and what the prompt sends, so watching it change
                            // is the fastest check that an edit landed in the
                            // document rather than only on screen.
                            Tab::Notes => rsx! {
                                h2 { class: "app-pane-title", "Markdown sent to the model" }
                                pre { class: "app-markdown", "{doc_to_markdown(&notes.read())}" }
                            },
                            Tab::Report => rsx! {
                                h2 { class: "app-pane-title", "Markdown" }
                                pre { class: "app-markdown", "{doc_to_markdown(&report.read())}" }
                            },
                            Tab::Settings => rsx! {
                                h2 { class: "app-pane-title", "JSON schema sent to the model" }
                                pre { class: "app-markdown", "{schema_preview(&template.read())}" }
                            },
                        }
                    }
                }
            }
        }
    }
}

fn schema_preview(template: &Template) -> String {
    let schema = report_core::compile::Shape::compile(template).to_json_schema();
    serde_json::to_string_pretty(&schema).unwrap_or_default()
}

/// Something to open onto, exercising every node kind.
fn starter_template() -> Template {
    use report_core::template::{NodeKind, TemplateNode};

    let mut template = Template::new("Site inspection");
    template.description = "A record of a building inspection visit.".into();
    template.nodes = vec![
        TemplateNode::new(
            "Summary",
            NodeKind::Paragraph {
                description: "Two or three sentences summarising the visit.".into(),
            },
        ),
        TemplateNode::new(
            "Findings",
            NodeKind::Section {
                heading_description: "A heading naming the inspected area.".into(),
                children: vec![
                    TemplateNode::new(
                        "Overview",
                        NodeKind::Paragraph { description: "What was observed overall.".into() },
                    ),
                    TemplateNode::new(
                        "Defects",
                        NodeKind::Repeat {
                            description: "One group per defect mentioned in the notes.".into(),
                            item_label: "defect".into(),
                            min: Some(1),
                            max: None,
                            children: vec![
                                TemplateNode::new(
                                    "Location",
                                    NodeKind::Section {
                                        heading_description: "The defect's location.".into(),
                                        children: vec![TemplateNode::new(
                                            "Detail",
                                            NodeKind::Paragraph {
                                                description: "What is wrong and how severe it is."
                                                    .into(),
                                            },
                                        )],
                                    },
                                ),
                                TemplateNode::new(
                                    "Actions",
                                    NodeKind::List {
                                        description: "Recommended remedial actions.".into(),
                                        ordered: true,
                                        min_items: Some(1),
                                        max_items: Some(5),
                                    },
                                ),
                            ],
                        },
                    ),
                ],
            },
        ),
        TemplateNode::new(
            "Follow-up",
            NodeKind::Optional {
                description: "Include only if a follow-up visit is needed.".into(),
                children: vec![TemplateNode::new(
                    "Next steps",
                    NodeKind::List {
                        description: "What must happen before the next visit.".into(),
                        ordered: false,
                        min_items: None,
                        max_items: None,
                    },
                )],
            },
        ),
    ];
    template
}
