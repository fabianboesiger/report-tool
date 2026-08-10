//! Templates: a list, and the builder for whichever one is open.
//!
//! Two states in one screen, held in a `Signal<Option<Uuid>>` local to it rather than as a
//! fifth [`Screen`](crate::ui::Screen) variant — "which template am I editing" is state of
//! this screen, not of the app.

use dioxus::prelude::*;
use report_core::store;
use report_core::template::{NodeKind, Template, TemplateNode};
use uuid::Uuid;

use crate::ui::kit::{
    Button, EmptyState, Icon, List, Notice, NoticeKind, PageBody, PageHead, Row, Variant,
};
use crate::ui::TemplateBuilder;

#[component]
pub fn TemplatesScreen(template: Signal<Template>) -> Element {
    // `None` is the list; `Some` is the builder, editing the template in `template`.
    let mut editing = use_signal(|| None::<Uuid>);
    let mut revision = use_signal(|| 0u64);
    let mut message = use_signal(|| None::<(NoticeKind, String)>);

    let saved = use_memo(move || {
        let _ = revision.read();
        store::list_templates().unwrap_or_default()
    });

    let mut save = move || match store::save_template(&template.read()) {
        Ok(()) => {
            revision.with_mut(|count| *count += 1);
            message.set(Some((NoticeKind::Ok, "Template saved".to_string())));
        }
        Err(error) => message.set(Some((NoticeKind::Error, format!("{error:#}")))),
    };

    if let Some(id) = editing() {
        let name = template.read().name.clone();
        let purpose = template.read().description.clone();
        return rsx! {
            PageHead {
                title: if name.is_empty() { "Untitled template".to_string() } else { name },
                subtitle: purpose,
                actions: rsx! {
                    Button {
                        label: "Back".to_string(),
                        variant: Variant::Quiet,
                        onclick: move |_| {
                            editing.set(None);
                            message.set(None);
                        },
                    }
                    Button {
                        label: "Duplicate".to_string(),
                        variant: Variant::Normal,
                        onclick: move |_| {
                            // A fresh id, so saving writes a second file rather than
                            // overwriting the one this was copied from.
                            let mut copy = template.read().clone();
                            copy.id = Uuid::new_v4();
                            copy.name = format!("{} copy", copy.name);
                            template.set(copy);
                            editing.set(Some(template.read().id));
                            save();
                        },
                    }
                    Button {
                        label: "Save template".to_string(),
                        variant: Variant::Primary,
                        onclick: move |_| save(),
                    }
                },
            }
            PageBody {
                if let Some((kind, text)) = message() {
                    Notice { kind, message: text }
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
            title: "Templates".to_string(),
            subtitle: match all.len() {
                0 => String::new(),
                1 => "The shape each kind of report follows · 1 saved".to_string(),
                count => format!("The shape each kind of report follows · {count} saved"),
            },
            actions: rsx! {
                Button {
                    label: "New template".to_string(),
                    icon: Icon::Plus,
                    variant: Variant::Primary,
                    onclick: move |_| {
                        template.set(blank_template());
                        editing.set(Some(template.read().id));
                        save();
                    },
                }
            },
        }

        PageBody {
            if let Some((kind, text)) = message() {
                Notice { kind, message: text }
            }

            if all.is_empty() {
                EmptyState {
                    icon: Icon::Layout,
                    title: "No templates yet".to_string(),
                    hint: "A template captures the shape of a report once — its headings, their order, and what each part is for. Every report you write from it follows that shape.".to_string(),
                    action: rsx! {
                        Button {
                            label: "New template".to_string(),
                            icon: Icon::Plus,
                            variant: Variant::Primary,
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
                            when: store::relative_time(item.updated),
                            onopen: {
                                let id = item.id;
                                move |_| match store::load_template(id) {
                                    Ok(loaded) => {
                                        template.set(loaded);
                                        editing.set(Some(id));
                                        message.set(None);
                                    }
                                    Err(error) => {
                                        message.set(Some((NoticeKind::Error, format!("{error:#}"))))
                                    }
                                }
                            },
                            ondelete: {
                                let id = item.id;
                                move |_| match store::delete_template(id) {
                                    Ok(()) => revision.with_mut(|count| *count += 1),
                                    Err(error) => {
                                        message.set(Some((NoticeKind::Error, format!("{error:#}"))))
                                    }
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

/// An empty template, for someone who knows what they want.
fn blank_template() -> Template {
    let mut template = Template::new("Untitled template");
    template.description = String::new();
    template
}

/// Something to open onto, exercising every node kind.
///
/// Offered from the empty state rather than seeded at startup: a first-time user gets a
/// worked example to take apart, and everyone else gets a library that only contains what
/// they made.
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
