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

use crate::i18n::{self, t};
use crate::ui::confirm;
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

    // The name is passed in rather than looked up in `new_report`, so translating it stays on
    // the render path: `Workspace` is in a module with no i18n context to consume.
    let create = use_callback(move |template: Template| {
        match workspace.new_report(template, t!("workspace-untitled-report")) {
            Ok(()) => {
                error.set(None);
                screen.set(Screen::Editor);
            }
            Err(problem) => error.set(Some(format!("{problem:#}"))),
        }
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
                title: t!("reports-start-title"),
                subtitle: t!("reports-start-subtitle"),
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
            title: t!("reports-title"),
            subtitle: subtitle(&all),
            actions: rsx! {
                if !all.is_empty() {
                    SearchField {
                        value: query(),
                        placeholder: t!("reports-search"),
                        oninput: move |text| query.set(text),
                    }
                }
                Button {
                    label: t!("reports-new"),
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
                    title: t!("reports-empty-title"),
                    hint: t!("reports-empty-hint"),
                    action: rsx! {
                        Button {
                            label: t!("reports-new"),
                            icon: Icon::Plus,
                            variant: Variant::Primary,
                            onclick: move |_| begin.call(starter_empty.clone()),
                        }
                    },
                }
            } else if shown.is_empty() {
                EmptyState {
                    icon: Icon::Search,
                    title: t!("reports-nomatch-title"),
                    hint: t!("reports-nomatch-hint"),
                }
            } else {
                List {
                    for report in shown.iter() {
                        Row {
                            key: "{report.id}",
                            name: report.name.clone(),
                            from: report.template_name.clone(),
                            tag: Some((
                                if report.generated { t!("reports-tag-final") } else { t!("reports-tag-draft") },
                                !report.generated,
                            )),
                            when: i18n::relative_time(report.updated),
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
///
/// The count is a Fluent plural selector rather than a `match` on 1, which is what makes it
/// right in a language whose rules are not English's.
fn subtitle(reports: &[Summary]) -> String {
    match reports.len() {
        0 => String::new(),
        count => t!(
            "reports-count",
            count: count as i64,
            when: i18n::relative_time(reports[0].updated)
        ),
    }
}

/// Delete a report, once the user has confirmed it.
fn delete(id: uuid::Uuid, name: String, workspace: Workspace, mut error: Signal<Option<String>>) {
    // Translated here, before the spawn: `t!` resolves the catalogue out of the component's
    // context, and inside a task past an `await` there is no scope left to resolve it from.
    let action = t!("reports-delete-action");
    let consequence = t!("confirm-no-undo");
    spawn(async move {
        if !confirm::destructive(&action, &name, &consequence).await {
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
