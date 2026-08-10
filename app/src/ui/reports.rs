//! The library: every report, most recently worked on first.
//!
//! This screen is what replaced the Save / Open / New toolbar. Opening is a click on a
//! row, saving happens by itself, and creating is one button — which between them is the
//! whole of what those five buttons used to do.
//!
//! ## New report asks which template
//!
//! A report is a template plus notes, and it keeps a *snapshot* of that template, so which
//! one it starts from is a decision that cannot be revisited later without invalidating
//! prose already written against it. New report therefore shows the saved templates and
//! waits — except when there are none, where there is nothing to choose between and the
//! built-in example is used directly rather than making the first click a dead end.

use dioxus::prelude::*;
use report_core::store::{self, Summary};
use report_core::Template;

use crate::ui::kit::{
    Button, EmptyState, Glyph, Icon, List, Notice, NoticeKind, PageBody, PageHead, Row, Variant,
};
use crate::ui::template_picker::TemplatePicker;
use crate::ui::workspace::Workspace;
use crate::ui::Screen;

#[component]
pub fn ReportsScreen(
    workspace: Workspace,
    screen: Signal<Screen>,
    /// The template a new report starts from, so this screen does not have to know how to
    /// build one.
    starter: Template,
) -> Element {
    let mut error = use_signal(|| None::<String>);
    let mut query = use_signal(String::new);
    // `Some` means the template chooser is open, holding the list as it was when the button
    // was pressed. Read at that moment rather than memoised: templates are written from
    // another screen with its own revision counter, so a memo here would go stale exactly
    // when someone had just added the template they now want to use.
    let mut choosing = use_signal(|| None::<Vec<Summary>>);

    // Re-listed whenever anything is written, which is what `revision` exists for: the
    // alternative is polling the directory on a timer.
    let reports = use_memo(move || {
        let _ = workspace.revision.read();
        store::list_reports().unwrap_or_default()
    });

    // `use_callback` rather than a closure: both of these are handed to more than one
    // button (the head and the empty state each offer "New report"), and a plain closure
    // that captures a signal is not `Copy`, so the second call site would be a move out of
    // an already-moved value.
    let open = use_callback(move |id| match workspace.open_report(id) {
        Ok(()) => {
            error.set(None);
            screen.set(Screen::Editor);
        }
        Err(problem) => error.set(Some(format!("{problem:#}"))),
    });

    let create = use_callback(move |template: Template| match workspace.new_report(template) {
        Ok(()) => {
            error.set(None);
            screen.set(Screen::Editor);
        }
        Err(problem) => error.set(Some(format!("{problem:#}"))),
    });

    // Offer the choice, or skip straight past it when there is nothing to choose.
    let begin = use_callback(move |fallback: Template| match store::list_templates() {
        Ok(saved) if !saved.is_empty() => {
            error.set(None);
            choosing.set(Some(saved));
        }
        Ok(_) => create.call(fallback),
        Err(problem) => error.set(Some(format!("{problem:#}"))),
    });

    // One clone per button. `Template` is not `Copy`, and both the head and the empty state
    // offer "New report".
    let (starter_head, starter_empty) = (starter.clone(), starter.clone());

    let all = reports();
    let needle = query().trim().to_lowercase();
    let shown: Vec<Summary> = if needle.is_empty() {
        all.clone()
    } else {
        all.iter()
            .filter(|report| {
                report.name.to_lowercase().contains(&needle)
                    || report.template_name.to_lowercase().contains(&needle)
            })
            .cloned()
            .collect()
    };

    if let Some(templates) = choosing() {
        return rsx! {
            TemplatePicker {
                templates,
                title: "Start a report".to_string(),
                subtitle: "Which template should it follow? The report keeps a copy, so \
                           editing the template later will not change this report."
                    .to_string(),
                allow_none: true,
                on_pick: move |template| {
                    choosing.set(None);
                    create.call(template);
                },
                on_cancel: move |_| choosing.set(None),
            }
        };
    }

    rsx! {
        PageHead {
            title: "Reports".to_string(),
            subtitle: subtitle(&all),
            actions: rsx! {
                if !all.is_empty() {
                    SearchField {
                        value: query(),
                        placeholder: "Search reports".to_string(),
                        oninput: move |text| query.set(text),
                    }
                }
                Button {
                    label: "New report".to_string(),
                    icon: Icon::Plus,
                    variant: Variant::Primary,
                    onclick: move |_| begin.call(starter_head.clone()),
                }
            },
        }

        PageBody {
            if let Some(problem) = error() {
                Notice { kind: NoticeKind::Error, message: problem }
            }

            if all.is_empty() {
                EmptyState {
                    icon: Icon::Document,
                    title: "No reports yet".to_string(),
                    hint: "Start one, jot down what you saw, and the template turns it into a written report.".to_string(),
                    action: rsx! {
                        Button {
                            label: "New report".to_string(),
                            icon: Icon::Plus,
                            variant: Variant::Primary,
                            onclick: move |_| begin.call(starter_empty.clone()),
                        }
                    },
                }
            } else if shown.is_empty() {
                EmptyState {
                    icon: Icon::Search,
                    title: "Nothing matches that".to_string(),
                    hint: "Try part of a report's name, or the template it was written from.".to_string(),
                }
            } else {
                List {
                    for report in shown.iter() {
                        Row {
                            key: "{report.id}",
                            name: report.name.clone(),
                            from: report.template_name.clone(),
                            tag: Some((
                                if report.generated { "Final".to_string() } else { "Draft".to_string() },
                                !report.generated,
                            )),
                            when: store::relative_time(report.updated),
                            onopen: {
                                let id = report.id;
                                move |_| open.call(id)
                            },
                            ondelete: {
                                let id = report.id;
                                let name = report.name.clone();
                                move |_| delete(id, name.clone(), workspace, error)
                            },
                        }
                    }
                }
            }
        }
    }
}

/// One sentence about the library, or nothing when there is nothing to say.
fn subtitle(reports: &[Summary]) -> String {
    match reports.len() {
        0 => String::new(),
        1 => format!("1 report · last written {}", store::relative_time(reports[0].updated)),
        count => {
            format!("{count} reports · last written {}", store::relative_time(reports[0].updated))
        }
    }
}

/// Delete a report, once the user has confirmed it.
///
/// Confirmed through the platform dialog rather than an in-app one: this is the only
/// destructive action in the app, it is reached from a hover-revealed button, and there is
/// no undo behind it. `rfd` is already here for the export dialog.
fn delete(id: uuid::Uuid, name: String, workspace: Workspace, mut error: Signal<Option<String>>) {
    spawn(async move {
        let confirmed = rfd::AsyncMessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title("Delete report")
            .set_description(format!("Delete “{name}”? This cannot be undone."))
            .set_buttons(rfd::MessageButtons::OkCancel)
            .show()
            .await;
        if confirmed != rfd::MessageDialogResult::Ok {
            return;
        }
        match store::delete_report(id) {
            Ok(()) => {
                // If the deleted report is the one on screen, forget its id so autosave
                // does not write the file straight back again.
                if *workspace.report_id.peek() == Some(id) {
                    workspace.report_id.clone().set(None);
                }
                workspace.revision.clone().with_mut(|count| *count = count.wrapping_add(1));
                error.set(None);
            }
            Err(problem) => error.set(Some(format!("{problem:#}"))),
        }
    });
}

/// The search box.
///
/// Private to this screen: one caller, so it is not in the kit. Promote it the day
/// Templates wants one too.
#[component]
fn SearchField(value: String, placeholder: String, oninput: EventHandler<String>) -> Element {
    rsx! {
        div { class: "search",
            Glyph { icon: Icon::Search }
            input {
                r#type: "search",
                value: "{value}",
                placeholder: "{placeholder}",
                "aria-label": "{placeholder}",
                oninput: move |event| oninput.call(event.value()),
            }
        }
    }
}
