//! Saving, opening and exporting.
//!
//! The workspace is held as separate signals rather than as one `Report` value,
//! because the editor binds to a `Signal<RichDoc>` and nesting documents inside a
//! report signal would mean every keystroke rewriting the whole report. A [`Report`]
//! is assembled at save time and taken apart at load time, which is also the moment
//! the template snapshot is captured.

use dioxus::prelude::*;
use report_core::store::{self, Report, Summary};
use report_core::Template;
use report_doc::{markdown::to_markdown, RichDoc};
use uuid::Uuid;

/// Everything the library needs to read and write.
///
/// All fields are `Signal`s, so this is `Copy` and stays valid across renders — the
/// same reason the editor's state is shaped this way.
#[derive(Clone, Copy, PartialEq)]
pub struct Workspace {
    pub template: Signal<Template>,
    pub notes: Signal<RichDoc>,
    pub generated: Signal<RichDoc>,
    pub has_report: Signal<bool>,
    pub report_id: Signal<Option<Uuid>>,
    pub report_name: Signal<String>,
}

impl Workspace {
    /// Assemble the current state into a report and write it.
    fn save_report(&self) -> anyhow::Result<()> {
        let mut report = Report::new(self.report_name.read().clone(), self.template.read().clone());
        // Reuse the id when saving over an existing report, so saving twice does not
        // leave two copies in the library.
        if let Some(id) = *self.report_id.read() {
            report.id = id;
        }
        report.notes = self.notes.read().clone();
        report.generated = self.has_report.read().then(|| self.generated.read().clone());

        store::save_report(&report)?;
        self.report_id.clone().set(Some(report.id));
        Ok(())
    }

    fn open_report(&self, id: Uuid) -> anyhow::Result<()> {
        let report = store::load_report(id)?;
        // The report's own snapshot becomes the working template: regenerating an old
        // report must use the template it was written against, not whatever the
        // builder happens to hold now.
        self.template.clone().set(report.template);
        self.notes.clone().set(report.notes);
        match report.generated {
            Some(document) => {
                self.generated.clone().set(document);
                self.has_report.clone().set(true);
            }
            None => self.has_report.clone().set(false),
        }
        self.report_name.clone().set(report.name);
        self.report_id.clone().set(Some(id));
        Ok(())
    }

    fn new_report(&self) {
        self.notes.clone().set(RichDoc::empty_paragraph());
        self.generated.clone().set(RichDoc::empty_paragraph());
        self.has_report.clone().set(false);
        self.report_id.clone().set(None);
        self.report_name.clone().set("Untitled report".to_string());
    }
}

#[component]
pub fn LibraryBar(workspace: Workspace) -> Element {
    let mut status = use_signal(String::new);
    // Re-read after every write so the lists reflect what is actually on disk.
    let mut reports = use_signal(|| store::list_reports().unwrap_or_default());
    let mut templates = use_signal(|| store::list_templates().unwrap_or_default());

    let report_open = move |id: Uuid| match workspace.open_report(id) {
        Ok(()) => status.set("Opened".into()),
        Err(error) => status.set(format!("{error:#}")),
    };

    rsx! {
        div { class: "lib",
            input {
                class: "lib-name",
                value: "{workspace.report_name}",
                placeholder: "Report name",
                oninput: move |event| workspace.report_name.clone().set(event.value()),
            }

            button {
                class: "lib-btn",
                onclick: move |_| {
                    match workspace.save_report() {
                        Ok(()) => {
                            status.set("Saved".into());
                            reports.set(store::list_reports().unwrap_or_default());
                        }
                        Err(error) => status.set(format!("{error:#}")),
                    }
                },
                "Save report"
            }

            Picker { label: "Open".to_string(), items: reports(), on_pick: report_open }

            button {
                class: "lib-btn",
                onclick: move |_| {
                    workspace.new_report();
                    status.set("New report".into());
                },
                "New"
            }

            span { class: "lib-sep" }

            button {
                class: "lib-btn",
                onclick: move |_| {
                    match store::save_template(&workspace.template.read()) {
                        Ok(()) => {
                            status.set("Template saved".into());
                            templates.set(store::list_templates().unwrap_or_default());
                        }
                        Err(error) => status.set(format!("{error:#}")),
                    }
                },
                "Save template"
            }

            Picker {
                label: "Templates".to_string(),
                items: templates(),
                on_pick: move |id| {
                    match store::load_template(id) {
                        Ok(template) => {
                            workspace.template.clone().set(template);
                            status.set("Template loaded".into());
                        }
                        Err(error) => status.set(format!("{error:#}")),
                    }
                },
            }

            span { class: "lib-spacer" }

            button {
                class: "lib-btn",
                disabled: !workspace.has_report.read().to_owned(),
                onclick: move |_| export(workspace, status),
                "Export .md"
            }

            span { class: "lib-status", "{status}" }
        }
    }
}

/// A dropdown that fires once and resets, so picking the same entry twice works.
#[component]
fn Picker(label: String, items: Vec<Summary>, on_pick: EventHandler<Uuid>) -> Element {
    rsx! {
        select {
            class: "lib-pick",
            disabled: items.is_empty(),
            // Bound to the placeholder every render rather than to a selection: this
            // is an action menu, not a piece of state, and leaving the last choice
            // showing would suggest otherwise.
            value: "",
            onchange: move |event| {
                if let Ok(id) = event.value().parse::<Uuid>() {
                    on_pick.call(id);
                }
            },
            option { value: "", disabled: true, selected: true,
                if items.is_empty() { "{label} (none saved)" } else { "{label}…" }
            }
            for item in items.iter() {
                option { key: "{item.id}", value: "{item.id}", "{item.name}" }
            }
        }
    }
}

/// Write the generated report to a file the user picks.
fn export(workspace: Workspace, mut status: Signal<String>) {
    let markdown = to_markdown(&workspace.generated.read());
    let suggested = format!("{}.md", slug(&workspace.report_name.read()));

    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .set_file_name(&suggested)
            .add_filter("Markdown", &["md"])
            .save_file()
            .await
        else {
            // Cancelling is a normal outcome, not a failure.
            return;
        };
        match file.write(markdown.as_bytes()).await {
            Ok(()) => status.set(format!("Exported to {}", file.file_name())),
            Err(error) => status.set(format!("Export failed: {error}")),
        }
    });
}

/// A filename-safe version of the report name.
fn slug(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() {
        "report".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggested_filenames_are_safe_on_every_platform() {
        // A slash or a colon in the name would either create a directory or be
        // rejected outright by the save dialog.
        assert_eq!(slug("March visit"), "March-visit");
        assert_eq!(slug("2026/03 site: north"), "2026-03-site--north");
        assert_eq!(slug("  "), "report");
        assert_eq!(slug(""), "report");
        assert_eq!(slug("---"), "report");
        assert_eq!(slug("Gebäude"), "Gebäude", "non-ASCII letters are fine in a filename");
    }
}
