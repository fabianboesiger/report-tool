//! The open report, and keeping it on disk.
//!
//! The workspace is held as separate signals rather than as one `Report` value, because
//! the editor binds to a `Signal<RichDoc>` and nesting documents inside a report signal
//! would mean every keystroke rewriting the whole report. A [`Report`] is assembled at
//! save time and taken apart at load time, which is also the moment the template snapshot
//! is captured.
//!
//! ## There is no Save button
//!
//! [`use_autosave`] writes the report shortly after it stops changing, and
//! [`Workspace::new_report`] gives a report its file the moment it is created. Those two
//! facts together are what let the Save / Open / New toolbar go: opening is a list,
//! saving is not a decision the user has to make, and there is no state in which work
//! exists on screen but not on disk.

use std::time::Duration;

use dioxus::prelude::*;
use report_core::store::{self, Report};
use report_core::Template;
use report_doc::{markdown::to_markdown, RichDoc};
use uuid::Uuid;

use crate::i18n::t;

/// Everything the library reads and writes.
///
/// All fields are `Signal`s, so this is `Copy` and stays valid across renders — the same
/// reason the editor's state is shaped this way. It also means the signals outlive any one
/// screen, so leaving the editor and coming back finds the notes intact.
#[derive(Clone, Copy, PartialEq)]
pub struct Workspace {
    pub template: Signal<Template>,
    pub notes: Signal<RichDoc>,
    pub generated: Signal<RichDoc>,
    pub has_report: Signal<bool>,
    pub report_id: Signal<Option<Uuid>>,
    pub report_name: Signal<String>,
    /// When the model last wrote the report, for the report pane's "Written 14:02 · edit
    /// freely". `None` until it has.
    pub written_at: Signal<Option<u64>>,
    /// Bumped after every successful write. The library screens read it and re-list, which
    /// is how they reflect disk without polling it.
    pub revision: Signal<u64>,
}

impl Workspace {
    /// Assemble the current state into a report and write it.
    pub fn save_report(&self) -> anyhow::Result<()> {
        let mut report = Report::new(self.report_name.read().clone(), self.template.read().clone());
        // Reuse the id when saving over an existing report, so saving twice does not leave
        // two copies in the library.
        if let Some(id) = *self.report_id.read() {
            report.id = id;
        }
        report.notes = self.notes.read().clone();
        report.generated = self.has_report.read().then(|| self.generated.read().clone());

        store::save_report(&report)?;
        self.report_id.clone().set(Some(report.id));
        // `with_mut`, not `set(read() + 1)`: the read guard outlives the expression it is
        // written in, so the `set` would find the signal still borrowed and panic.
        self.revision.clone().with_mut(|count| *count = count.wrapping_add(1));
        Ok(())
    }

    pub fn open_report(&self, id: Uuid) -> anyhow::Result<()> {
        let report = store::load_report(id)?;
        // The report's own snapshot becomes the working template: regenerating an old
        // report must use the template it was written against, not whatever the builder
        // happens to hold now.
        self.template.clone().set(report.template);
        self.notes.clone().set(report.notes);
        // Not the generation time — that is not recorded on disk — but the last time the
        // report was touched, which is the honest answer to "when was this written".
        self.written_at.clone().set(report.generated.is_some().then_some(report.updated));
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

    /// Start a new report from `template`, and **write it immediately**.
    ///
    /// Creating a report is a deliberate act; saving it is not. Committing the file here
    /// is what lets [`use_autosave`] be a pure update, and it removes the only question
    /// autosave would otherwise have to answer — whether an untouched, unnamed workspace
    /// deserves to exist on disk. The answer was always ambiguous; this makes it moot.
    /// `name` is the placeholder the report opens with — passed in rather than chosen here,
    /// because it is translated and this module has no i18n context to translate from. See
    /// `crate::i18n`.
    pub fn new_report(&self, template: Template, name: String) -> anyhow::Result<()> {
        self.template.clone().set(template);
        self.notes.clone().set(RichDoc::empty_paragraph());
        self.generated.clone().set(RichDoc::empty_paragraph());
        self.has_report.clone().set(false);
        self.written_at.clone().set(None);
        self.report_id.clone().set(None);
        self.report_name.clone().set(name);
        self.save_report()
    }
}

/// What autosave last did, for the head's subtitle.
///
/// A value, not a message: autosave runs inside a spawned task and the words belong to
/// whichever language the app is in, so the component turns this into text when it draws it.
#[derive(Clone, PartialEq)]
pub enum SaveState {
    Saved,
    Saving,
    /// The whole error chain. Still English — `report-core`'s diagnostics are out of scope
    /// for translation — but the sentence around it is not.
    Failed(String),
}

impl SaveState {
    pub fn message(&self) -> String {
        match self {
            SaveState::Saved => t!("workspace-saved-automatically"),
            SaveState::Saving => t!("workspace-saving"),
            SaveState::Failed(error) => t!("workspace-save-failed", error: error.as_str()),
        }
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, SaveState::Failed(_))
    }
}

/// How an export finished, for the notice under the split.
///
/// Same reason as [`SaveState`]: the file dialog is `async`, so the outcome arrives where
/// there is no context to translate in.
#[derive(Clone, PartialEq)]
pub enum Exported {
    To(String),
    Failed(String),
}

impl Exported {
    pub fn message(&self) -> String {
        match self {
            Exported::To(file) => t!("workspace-exported", file: file.as_str()),
            Exported::Failed(error) => t!("workspace-export-failed", error: error.as_str()),
        }
    }
}

/// Persist the workspace shortly after it stops changing.
///
/// Debounced rather than written per keystroke: a report is two documents plus a template
/// snapshot, and serialising all of that per character would put the JSON writer in the
/// typing path. Two seconds is long enough that a burst of typing is one write, and short
/// enough that a crash costs a sentence.
///
/// Only ever *updates* — see [`Workspace::new_report`] for why there is no create path
/// here.
pub fn use_autosave(workspace: Workspace) -> Signal<SaveState> {
    let mut state = use_signal(|| SaveState::Saved);
    // Counts edits. The watcher below increments it without ever reading it, which is what
    // stops the effect re-triggering itself — the same `peek`-shaped discipline the
    // template builder uses on its event sequence number.
    let mut edits = use_signal(|| 0u64);

    use_effect(move || {
        // Subscribe to everything a report is made of. Read, not peeked: these reads are
        // the subscription.
        let _ = workspace.notes.read();
        let _ = workspace.generated.read();
        let _ = workspace.report_name.read();
        let _ = workspace.template.read();
        let _ = workspace.has_report.read();
        edits.with_mut(|count| *count += 1);
    });

    use_effect(move || {
        let seen = *edits.read();
        // The first run is the mount, not an edit; writing here would rewrite every report
        // the moment it is opened and move it to the top of the library for no reason.
        if seen <= 1 {
            return;
        }
        // Nothing to update until the report has a file. `new_report` creates one, so this
        // only skips the window before the first report exists.
        if workspace.report_id.peek().is_none() {
            return;
        }

        spawn(async move {
            tokio::time::sleep(Duration::from_secs(2)).await;
            // A later edit arrived while we waited, so that run will do the write. Without
            // this every keystroke in a burst would queue its own save.
            if *edits.peek() != seen {
                return;
            }
            state.set(SaveState::Saving);
            match workspace.save_report() {
                Ok(()) => state.set(SaveState::Saved),
                // Named rather than swallowed: with no Save button, a failure here is the
                // only signal that work is not being kept.
                Err(error) => {
                    tracing::error!("autosave: {error:#}");
                    state.set(SaveState::Failed(format!("could not save — {error:#}")));
                }
            }
        });
    });

    state
}

/// Write the generated report to a file the user picks.
pub fn export(workspace: Workspace, mut status: Signal<Option<Exported>>) {
    let markdown = to_markdown(&workspace.generated.read());
    // From the report's name, which is the user's own text — never from a `t!` result, which
    // Fluent would have wrapped in invisible bidi isolates.
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
            Ok(()) => status.set(Some(Exported::To(file.file_name()))),
            Err(error) => status.set(Some(Exported::Failed(format!("{error}")))),
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
        // A slash or a colon in the name would either create a directory or be rejected
        // outright by the save dialog.
        assert_eq!(slug("March visit"), "March-visit");
        assert_eq!(slug("2026/03 site: north"), "2026-03-site--north");
        assert_eq!(slug("  "), "report");
        assert_eq!(slug(""), "report");
        assert_eq!(slug("---"), "report");
        assert_eq!(slug("Gebäude"), "Gebäude", "non-ASCII letters are fine in a filename");
    }
}
