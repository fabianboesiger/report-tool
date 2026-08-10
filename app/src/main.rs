//! Report tool — desktop entry point.

mod audio;
mod dictate;
mod generate;
mod models;
mod ui;

use dioxus::prelude::*;
use report_core::settings::Settings;
use report_doc::RichDoc;
use report_editor::{doc_to_markdown, EditorRuntime};

use crate::dictate::use_dictation;
use crate::generate::{generate, Status};
use crate::models::use_model_downloads;
use crate::ui::{
    starter_template, use_autosave, EditorScreen, Rail, ReportsScreen, Screen, SettingsPanel,
    TemplatesScreen, Workspace,
};

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

#[component]
fn App() -> Element {
    // Resolved once at startup so the chosen backend appears in the log before any
    // model is loaded; a wrong choice here is otherwise only visible as slowness.
    use_hook(|| match report_core::gpu::select() {
        Ok(backend) => tracing::info!("compute backend: {}", backend.as_str()),
        Err(error) => tracing::error!("gpu: {error}"),
    });

    let screen = use_signal(|| Screen::Reports);
    let settings = use_signal(Settings::load);
    let template = use_signal(starter_template);

    // Owned here rather than inside a screen, so leaving the editor and coming back finds
    // the notes intact. Only the editor's focus guard resets, which is what leaving a text
    // field should do anyway.
    let workspace = Workspace {
        template,
        notes: use_signal(RichDoc::empty_paragraph),
        generated: use_signal(RichDoc::empty_paragraph),
        has_report: use_signal(|| false),
        report_id: use_signal(|| None),
        report_name: use_signal(|| "Untitled report".to_string()),
        written_at: use_signal(|| None),
        revision: use_signal(|| 0),
    };

    // Starts on open and resumes an interrupted download from where it stopped.
    let downloads = use_model_downloads(settings);
    let dictation = use_dictation(workspace.notes, settings);
    let save_state = use_autosave(workspace);
    let mut status = use_signal(|| Status::Idle);

    // The developer group in Settings reads the working template through context rather
    // than a prop, so nothing on the path down to it has to carry a parameter that only
    // exists in a debug build.
    use_context_provider(|| template);

    // A callback rather than a closure so it stays `Copy` across the rsx tree.
    let run = use_callback(move |_: ()| {
        let template = workspace.template.read().clone();
        let notes_markdown = doc_to_markdown(&workspace.notes.read());
        let settings = settings.read().clone();
        let mut generated = workspace.generated;
        let mut has_report = workspace.has_report;
        let mut written_at = workspace.written_at;

        status.set(Status::Running);
        spawn(async move {
            match generate(template, notes_markdown, settings).await {
                Ok(document) => {
                    generated.set(document);
                    has_report.set(true);
                    written_at.set(Some(report_core::store::now()));
                    status.set(Status::Done);
                }
                // The whole chain, so the message names the cause rather than the
                // symptom — "Incorrect API key" instead of "request failed".
                Err(error) => status.set(Status::Failed(format!("{error:#}"))),
            }
        });
    });

    let theme = settings.read().appearance;

    rsx! {
        style { dangerous_inner_html: crate::ui::CSS }
        EditorRuntime {
            // `data-theme` lives here rather than on `<html>`, which rsx does not render.
            // Setting it through `document::eval` would queue behind the editor bridge's
            // own channel and paint one frame in the wrong theme first — a white flash on
            // every launch for anyone who chose Dark. See the token block in `app.css`.
            div { class: "shell", "data-theme": theme.attribute(),
                Rail { screen, settings }

                main { class: "main",
                    match screen() {
                        Screen::Reports => rsx! {
                            ReportsScreen {
                                workspace,
                                screen,
                                starter: starter_template(),
                            }
                        },
                        Screen::Editor => rsx! {
                            EditorScreen {
                                workspace,
                                dictation: dictation.clone(),
                                downloads,
                                status,
                                save_state,
                                on_generate: move |_| run.call(()),
                            }
                        },
                        Screen::Templates => rsx! {
                            TemplatesScreen { template }
                        },
                        Screen::Settings => rsx! {
                            SettingsPanel { settings }
                        },
                    }
                }
            }
        }
    }
}
