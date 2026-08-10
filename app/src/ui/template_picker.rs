//! Choosing which template a report follows.
//!
//! Used from two places — starting a report, and changing the template of one already
//! open — so it lives here rather than inside either screen.
//!
//! ## Why a report can be re-pointed at all
//!
//! A report holds a *snapshot* of its template, which is what stops editing a template
//! from reaching back into reports already written from it. That protection cuts both
//! ways: it also means a report cannot follow a template it was not started from, and
//! before this there was no way to change one's mind. Picking here replaces the snapshot
//! deliberately, which is a different act from a template edit leaking into it by accident.
//!
//! Prose already written is left alone. It was produced against the old structure, and
//! silently discarding it would cost more than it saves — the next **Write report** is what
//! applies the new one.

use dioxus::prelude::*;
use report_core::store::{self, Summary};
use report_core::Template;

use crate::ui::kit::{Button, List, Notice, NoticeKind, PageBody, PageHead, Row, Variant};

/// A full-screen list of saved templates.
#[component]
pub fn TemplatePicker(
    templates: Vec<Summary>,
    title: String,
    subtitle: String,
    /// Offer a report with no structure at all. Worth having when starting one — notes
    /// without a generated report are a legitimate thing to want — and pointless when
    /// changing an existing report's template, where it would only take structure away.
    #[props(default)]
    allow_none: bool,
    on_pick: EventHandler<Template>,
    on_cancel: EventHandler<()>,
) -> Element {
    // Local, because a failure to load one template is about this screen and nothing above
    // it needs to hear about it.
    let mut error = use_signal(|| None::<String>);

    rsx! {
        PageHead {
            title,
            subtitle,
            actions: rsx! {
                Button {
                    label: "Cancel".to_string(),
                    variant: Variant::Quiet,
                    onclick: move |_| on_cancel.call(()),
                }
            },
        }

        PageBody {
            if let Some(problem) = error() {
                Notice { kind: NoticeKind::Error, message: problem }
            }

            List {
                for item in templates.iter() {
                    Row {
                        key: "{item.id}",
                        name: item.name.clone(),
                        when: store::relative_time(item.updated),
                        onopen: {
                            let id = item.id;
                            move |_| match store::load_template(id) {
                                // Loaded here rather than in the caller so every caller
                                // gets the same error handling for a template that was
                                // deleted between listing it and clicking it.
                                Ok(template) => on_pick.call(template),
                                Err(problem) => error.set(Some(format!("{problem:#}"))),
                            }
                        },
                    }
                }

                if allow_none {
                    Row {
                        name: "No template".to_string(),
                        from: "Just notes, no generated structure".to_string(),
                        // Named for what it produces rather than for being empty: the name
                        // lands in the report's snapshot and shows in the library's "from"
                        // column, where "Untitled template" would read as an oversight.
                        onopen: move |_| on_pick.call(Template::new("No template")),
                    }
                }
            }
        }
    }
}
