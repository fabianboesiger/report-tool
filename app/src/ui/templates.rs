//! Templates: a list, and the builder for whichever one is open.
//!
//! Two states in one screen, held in a `Signal<Option<Uuid>>` local to it rather than as a
//! fifth [`Screen`](crate::ui::Screen) variant — "which template am I editing" is state of
//! this screen, not of the app.
//!
//! ## Export and import
//!
//! Templates live in a database, so a template is no longer a file that can be emailed to
//! a colleague or committed alongside the reports it produces. These two buttons hand that
//! back deliberately rather than letting it disappear with the storage change.

use dioxus::prelude::*;
use report_core::store;
use report_core::template::{NodeKind, Template, TemplateNode};
use uuid::Uuid;

use crate::i18n::{self, t};
use crate::ui::confirm;
use crate::ui::kit::{
    Button, EmptyState, Icon, List, Notice, NoticeKind, PageBody, PageHead, Row, Variant,
};
use crate::ui::TemplateBuilder;

/// What this screen last did.
///
/// A value rather than a rendered string, because half of these arrive from a spawned task —
/// the file dialogs are `async` — and `t!` needs the component's context, which a task past
/// an `await` no longer has. The component turns this into words when it draws it.
#[derive(Clone, PartialEq)]
enum Message {
    Saved,
    Exported(String),
    ExportFailed(String),
    NotText,
    Imported(String),
    /// A `report-core` error chain, still English: those are diagnostics rather than copy.
    Failed(String),
}

impl Message {
    fn kind(&self) -> NoticeKind {
        match self {
            Message::Saved | Message::Exported(_) | Message::Imported(_) => NoticeKind::Ok,
            Message::ExportFailed(_) | Message::NotText | Message::Failed(_) => NoticeKind::Error,
        }
    }

    fn text(&self) -> String {
        match self {
            Message::Saved => t!("templates-saved"),
            Message::Exported(file) => t!("templates-exported", file: file.as_str()),
            Message::ExportFailed(error) => t!("templates-export-failed", error: error.as_str()),
            Message::NotText => t!("templates-not-text"),
            Message::Imported(name) => t!("templates-imported", name: name.as_str()),
            Message::Failed(error) => error.clone(),
        }
    }
}

#[component]
pub fn TemplatesScreen(template: Signal<Template>) -> Element {
    // `None` is the list; `Some` is the builder, editing the template in `template`.
    let mut editing = use_signal(|| None::<Uuid>);
    let mut revision = use_signal(|| 0u64);
    let mut message = use_signal(|| None::<Message>);

    let saved = use_memo(move || {
        let _ = revision.read();
        store::list_templates().unwrap_or_default()
    });

    let mut save = move || match store::save_template(&template.read()) {
        Ok(()) => {
            revision.with_mut(|count| *count += 1);
            message.set(Some(Message::Saved));
        }
        Err(error) => message.set(Some(Message::Failed(format!("{error:#}")))),
    };

    if let Some(id) = editing() {
        let name = template.read().name.clone();
        let purpose = template.read().description.clone();
        return rsx! {
            PageHead {
                title: if name.is_empty() { t!("templates-untitled") } else { name },
                subtitle: purpose,
                actions: rsx! {
                    Button {
                        label: t!("templates-back"),
                        variant: Variant::Quiet,
                        onclick: move |_| {
                            editing.set(None);
                            message.set(None);
                        },
                    }
                    Button {
                        label: t!("templates-duplicate"),
                        variant: Variant::Normal,
                        onclick: move |_| {
                            // A fresh id, so saving writes a second file rather than
                            // overwriting the one this was copied from.
                            let mut copy = template.read().clone();
                            copy.id = Uuid::new_v4();
                            // Through `plain`, because this name is stored and later becomes a
                            // filename: the isolate marks Fluent puts around `$name` would
                            // survive into both.
                            let named = t!("templates-copy-suffix", name: copy.name.as_str());
                            copy.name = i18n::plain(named);
                            template.set(copy);
                            editing.set(Some(template.read().id));
                            save();
                        },
                    }
                    Button {
                        label: t!("templates-export"),
                        variant: Variant::Normal,
                        onclick: move |_| export(template.read().clone(), message),
                    }
                    Button {
                        label: t!("templates-save"),
                        variant: Variant::Primary,
                        onclick: move |_| save(),
                    }
                },
            }
            PageBody {
                if let Some(message) = message() {
                    Notice { kind: message.kind(), message: message.text() }
                }
                TemplateBuilder { template }
            }
            // `id` is read so the builder remounts when a different template is opened,
            // which is what resets the editor's focus guard along with it.
            div { hidden: true, "{id}" }
        };
    }

    let all = saved();

    rsx! {
        PageHead {
            title: t!("templates-title"),
            subtitle: match all.len() {
                0 => String::new(),
                count => t!("templates-count", count: count as i64),
            },
            actions: rsx! {
                Button {
                    label: t!("templates-import"),
                    icon: Icon::Download,
                    variant: Variant::Normal,
                    onclick: move |_| import(revision, message),
                }
                Button {
                    label: t!("templates-new"),
                    icon: Icon::Plus,
                    variant: Variant::Primary,
                    onclick: move |_| {
                        template.set(blank_template(t!("templates-untitled")));
                        editing.set(Some(template.read().id));
                        save();
                    },
                }
            },
        }

        PageBody {
            if let Some(message) = message() {
                Notice { kind: message.kind(), message: message.text() }
            }

            if all.is_empty() {
                EmptyState {
                    icon: Icon::Layout,
                    title: t!("templates-empty-title"),
                    hint: t!("templates-empty-hint"),
                    // Two buttons, because there were two behaviours behind one label: the
                    // header's "New template" gives an empty one and this used to give a
                    // filled-in Site inspection example. Asking for a new template and
                    // getting somebody else's is a fair thing to call broken.
                    action: rsx! {
                        Button {
                            label: t!("templates-new"),
                            icon: Icon::Plus,
                            variant: Variant::Primary,
                            onclick: move |_| {
                                template.set(blank_template(t!("templates-untitled")));
                                editing.set(Some(template.read().id));
                                save();
                            },
                        }
                        Button {
                            label: t!("templates-start-example"),
                            variant: Variant::Normal,
                            onclick: move |_| {
                                template.set(starter_template());
                                editing.set(Some(template.read().id));
                                save();
                            },
                        }
                    },
                }
            } else {
                List {
                    for item in all.iter() {
                        Row {
                            key: "{item.id}",
                            name: item.name.clone(),
                            // Said in the list rather than only inside the builder: a
                            // template with no fields generates nothing, and the default
                            // name means several of them read identically otherwise.
                            tag: match item.fields {
                                0 => Some((t!("templates-tag-empty"), true)),
                                count => {
                                    Some((t!("templates-tag-fields", count: count as i64), false))
                                }
                            },
                            when: i18n::relative_time(item.updated),
                            onopen: {
                                let id = item.id;
                                move |_| match store::load_template(id) {
                                    Ok(loaded) => {
                                        template.set(loaded);
                                        editing.set(Some(id));
                                        message.set(None);
                                    }
                                    Err(error) => {
                                        message.set(Some(Message::Failed(format!("{error:#}"))))
                                    }
                                }
                            },
                            ondelete: {
                                let id = item.id;
                                let name = item.name.clone();
                                move |_| {
                                    let name = name.clone();
                                    // Translated before the spawn — see `crate::i18n`.
                                    let action = t!("templates-delete-action");
                                    let consequence = t!("templates-delete-consequence");
                                    spawn(async move {
                                        if !confirm::destructive(&action, &name, &consequence).await
                                        {
                                            return;
                                        }
                                        match store::delete_template(id) {
                                            Ok(()) => revision.with_mut(|count| *count += 1),
                                            Err(error) => message
                                                .set(Some(Message::Failed(format!("{error:#}")))),
                                        }
                                    });
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

/// Write a template out as `.json` for someone to send or commit.
fn export(template: Template, mut message: Signal<Option<Message>>) {
    let json = match store::export_template(&template) {
        Ok(json) => json,
        Err(error) => {
            message.set(Some(Message::Failed(format!("{error:#}"))));
            return;
        }
    };
    let suggested = format!("{}.json", file_stem(&template.name));

    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .set_file_name(&suggested)
            .add_filter("Template", &["json"])
            .save_file()
            .await
        else {
            // Cancelling is a normal outcome, not a failure.
            return;
        };
        match file.write(json.as_bytes()).await {
            Ok(()) => message.set(Some(Message::Exported(file.file_name()))),
            Err(error) => message.set(Some(Message::ExportFailed(format!("{error}")))),
        }
    });
}

/// Read a template someone sent, and store it as a new one.
fn import(mut revision: Signal<u64>, mut message: Signal<Option<Message>>) {
    spawn(async move {
        let Some(file) =
            rfd::AsyncFileDialog::new().add_filter("Template", &["json"]).pick_file().await
        else {
            return;
        };
        let bytes = file.read().await;
        let text = match std::str::from_utf8(&bytes) {
            Ok(text) => text,
            Err(_) => {
                message.set(Some(Message::NotText));
                return;
            }
        };
        // `import_template` assigns a fresh id and a non-colliding name, so this never
        // overwrites a template already here.
        match store::import_template(text) {
            Ok(imported) => {
                revision.with_mut(|count| *count += 1);
                message.set(Some(Message::Imported(imported.name)));
            }
            Err(error) => message.set(Some(Message::Failed(format!("{error:#}")))),
        }
    });
}

/// A filename-safe version of a template's name.
fn file_stem(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() {
        "template".to_string()
    } else {
        trimmed.to_string()
    }
}

/// An empty template, for someone who knows what they want.
///
/// The name is passed in because it is translated: a placeholder the user overwrites within
/// seconds should still arrive in their own language.
fn blank_template(name: String) -> Template {
    let mut template = Template::new(name);
    template.description = String::new();
    template
}

/// Something to open onto, exercising every node kind.
///
/// Offered from the empty state rather than seeded at startup: a first-time user gets a
/// worked example to take apart, and everyone else gets a library that only contains what
/// they made.
///
/// **The one English thing left in the UI, deliberately.** This is seeded *data*, not
/// chrome: its labels and descriptions are the user's to edit, they land in every report's
/// template snapshot, and they are fed to the model as instructions. Translating it means
/// deciding what happens to a template already made when the language changes (nothing may
/// happen to it) and how `template::slug` derives ASCII JSON keys from a non-English label.
/// Both are worth doing and neither is a string swap, so it is left for its own change.
pub fn starter_template() -> Template {
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
