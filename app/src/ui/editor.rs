//! Notes on the left, the written report on the right.
//!
//! The two panes are the whole screen. What used to be the right-hand half — a monospace
//! mirror of the same document — is gone; the report on screen already *is* the document,
//! and a second copy in monospace only invites the question of which one is real.

use dioxus::prelude::*;
use report_editor::Editor;

use crate::dictate::{Dictation, DictationControl};
use crate::generate::Status;
use crate::i18n::{self, t};
use crate::models::{ModelBanner, ModelStatus};
use crate::ui::kit::{
    Banner, Button, EmptyState, Icon, Notice, NoticeKind, PageBody, PageHead, Pane, Variant,
};
use crate::ui::template_picker::TemplatePicker;
use crate::ui::workspace::{export, Exported, SaveState, Workspace};

#[component]
#[allow(clippy::too_many_arguments)]
pub fn EditorScreen(
    workspace: Workspace,
    dictation: DictationControl,
    downloads: Signal<Vec<ModelStatus>>,
    status: Signal<Status>,
    save_state: Signal<SaveState>,
    on_generate: EventHandler<()>,
) -> Element {
    let exported = use_signal(|| None::<Exported>);
    // `Some` opens the template picker over this screen. Holds the list as it was when the
    // button was pressed, for the same reason the reports screen does.
    let mut choosing = use_signal(|| None::<Vec<report_core::store::Summary>>);
    let mut picker_error = use_signal(|| None::<String>);

    let running = status.read().is_running();
    let has_report = workspace.has_report.read().to_owned();
    let template_name = workspace.template.read().name.clone();
    let name = workspace.report_name.read().clone();

    // "Site inspection · saved automatically", which is where the absent Save button is
    // accounted for. A failure replaces it, because with no button this line is the only
    // place the user would learn that work is not being kept.
    let saved = save_state.read().clone();
    let subtitle = if template_name.is_empty() {
        saved.message()
    } else {
        format!("{template_name} · {}", saved.message())
    };

    if let Some(templates) = choosing() {
        return rsx! {
            TemplatePicker {
                templates,
                title: t!("editor-change-template-title"),
                subtitle: if has_report {
                    t!("editor-change-template-written")
                } else {
                    t!("editor-change-template-fresh")
                },
                on_pick: move |template| {
                    choosing.set(None);
                    // Autosave subscribes to this signal, so the new snapshot reaches the
                    // database on its own. Prose already written is deliberately left
                    // alone; it was produced against the old structure and the next
                    // generation is what replaces it.
                    workspace.template.clone().set(template);
                },
                on_cancel: move |_| choosing.set(None),
            }
        };
    }

    rsx! {
        PageHead {
            title: name,
            subtitle,
            on_title: move |text: String| workspace.report_name.clone().set(text),
            actions: rsx! {
                Button {
                    // Names the current template rather than saying "Change template": the
                    // subtitle carries it too, but the decision this button changes is the
                    // one thing about a report that cannot be inferred from its notes.
                    label: if template_name.is_empty() {
                        t!("editor-pick-template")
                    } else {
                        t!("editor-template-named", name: template_name.as_str())
                    },
                    icon: Icon::Layout,
                    variant: Variant::Quiet,
                    onclick: move |_| match report_core::store::list_templates() {
                        Ok(saved) if !saved.is_empty() => {
                            picker_error.set(None);
                            choosing.set(Some(saved));
                        }
                        // Nothing saved to switch to. Said out loud, because a button that
                        // does nothing when pressed reads as broken.
                        Ok(_) => picker_error.set(Some(t!("editor-no-templates-yet"))),
                        Err(problem) => picker_error.set(Some(format!("{problem:#}"))),
                    },
                }
                Button {
                    label: t!("editor-export"),
                    icon: Icon::Download,
                    variant: Variant::Quiet,
                    disabled: !has_report,
                    title: if has_report { String::new() } else { t!("editor-export-needs-report") },
                    onclick: move |_| export(workspace, exported),
                }
                Button {
                    label: if running { t!("editor-writing") } else { t!("editor-write") },
                    icon: Icon::Sparkle,
                    variant: Variant::Primary,
                    disabled: running,
                    onclick: move |_| on_generate.call(()),
                }
            },
        }

        ModelBanner { statuses: downloads }

        if let Some(problem) = picker_error() {
            Banner { warn: true,
                span { "{problem}" }
            }
        }

        if let Some(error) = status.read().message() {
            Banner { warn: true,
                span { {t!("editor-generate-failed", error: error)} }
            }
        }

        if saved.is_failed() {
            Banner { warn: true,
                span { {saved.message()} }
            }
        }

        PageBody { flush: true,
            div { class: "split",
                Pane {
                    label: t!("editor-pane-notes"),
                    body_class: String::new(),
                    actions: rsx! {
                        DictateButton { dictation: dictation.clone() }
                    },
                    if let Some(error) = dictation.state.read().message() {
                        Notice { kind: NoticeKind::Error, message: error.to_string() }
                    }
                    // No toolbar: notes are jottings, and a formatting bar over a
                    // scratchpad is an invitation to fuss with the input instead of
                    // writing it. Cmd+B/I/E and the markdown shortcuts still work — those
                    // come from the editor's key shim, not from the toolbar.
                    Editor {
                        doc: workspace.notes,
                        toolbar: false,
                        class: "notes".to_string(),
                        placeholder: t!("editor-notes-placeholder"),
                        labels: i18n::toolbar_labels(),
                    }
                }

                Pane {
                    label: t!("editor-pane-report"),
                    body_class: String::new(),
                    actions: rsx! {
                        if has_report {
                            // "Last written: just now · edit freely" — the second half matters
                            // as much as the first: the model's output is a draft to work on,
                            // not a result to accept, and nothing else on screen says so.
                            //
                            // A colon before the timestamp rather than the lowercasing this
                            // used to do, because a lowercase form is not one rule across
                            // these four languages: German capitalises "Gestern" wherever it
                            // stands and French does not.
                            span { class: "pane-hint",
                                match *workspace.written_at.read() {
                                    Some(when) => {
                                        t!("editor-written-hint", when: i18n::relative_time(when))
                                    }
                                    None => t!("editor-edit-freely"),
                                }
                            }
                        }
                    },
                    if has_report {
                        Editor {
                            doc: workspace.generated,
                            class: "doc".to_string(),
                            placeholder: String::new(),
                            labels: i18n::toolbar_labels(),
                        }
                    } else {
                        EmptyState {
                            title: t!("editor-empty-title"),
                            hint: t!("editor-empty-hint"),
                        }
                    }
                }
            }

            if let Some(outcome) = exported() {
                div { style: "padding:0 28px",
                    Notice { kind: NoticeKind::Ok, message: outcome.message() }
                }
            }
        }
    }
}

/// The record pill.
///
/// Stays here rather than in the kit: it has one caller and it knows what a [`Dictation`]
/// is, either of which would disqualify it.
#[component]
fn DictateButton(dictation: DictationControl) -> Element {
    let state = dictation.state.read().clone();
    let recording = state.is_recording();
    let transcribing = matches!(state, Dictation::Transcribing);

    let label = match state {
        Dictation::Recording => t!("dictate-stop"),
        Dictation::Transcribing => t!("dictate-transcribing"),
        _ => t!("dictate-start"),
    };

    rsx! {
        button {
            r#type: "button",
            class: if recording { "rec is-recording" } else { "rec" },
            disabled: transcribing,
            onclick: move |_| dictation.toggle(),
            span { class: "dot" }
            "{label}"
        }
    }
}
