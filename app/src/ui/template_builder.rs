//! The template builder.
//!
//! A template is a tree of instructions, and this shows it as the document it will
//! become: a section's description is set in the type size of the heading it will
//! produce, a list's description carries a bullet, and containers are nested boxes.
//! What the user arranges here is what a report looks like.
//!
//! ## The crate boundary
//!
//! Every description field is a [`EditableText`] from `report-editor`, so the caret
//! handling, the paste filtering and the focus guard are all the same code the notes
//! and report panes use. The container chrome — the chips, the add buttons, the
//! nesting — lives here, because `report-editor` must never learn what a `Template`
//! is.
//!
//! The one adapter that costs is the identity: `EditableText` addresses blocks by
//! [`BlockId`], and a template node has a [`NodeId`]. Both are UUIDs, so the mapping
//! is a newtype swap rather than a lookup table, and nothing had to change in the
//! editor crate to make this work.
//!
//! Structural keys (Enter, Tab) are deliberately ignored here: a description is one
//! instruction, not a document. Shift+Enter still inserts a line break, because the
//! shim leaves that to the browser.

use dioxus::prelude::*;
use report_core::template::{NodeId, NodeKind, Template, TemplateNode};
use report_doc::{BlockId, Span};
use report_editor::{use_bridge, EditableText, Focus, RawEvent};

use crate::ui::confirm;
use crate::ui::kit::{Button, Icon, IconButton, NumberField, Variant};

/// A template node's identity as the editor addresses it.
fn block_id(id: NodeId) -> BlockId {
    BlockId(id.0)
}

/// The reverse. Events arrive keyed by block, and every surface has to recognise its
/// own; a node id that is not in this template simply belongs to another surface.
fn node_id(id: BlockId) -> NodeId {
    NodeId(id.0)
}

#[component]
pub fn TemplateBuilder(template: Signal<Template>) -> Element {
    let bridge = use_bridge();
    let focus = use_signal(Focus::default);
    let mut seen = use_signal(|| 0u64);

    use_effect(move || {
        let Some(delivery) = bridge.latest() else { return };
        // `peek`, not a read: subscribing to the counter this effect writes would
        // make it re-trigger itself forever.
        if delivery.seq <= *seen.peek() {
            return;
        }
        seen.set(delivery.seq);
        handle(delivery.event, template, focus);
    });

    let name = template.read().name.clone();
    let purpose = template.read().description.clone();
    let nodes = template.read().nodes.clone();

    rsx! {
        div { class: "tpl",
            input {
                class: "tpl-name",
                value: "{name}",
                placeholder: "Template name",
                "aria-label": "Template name",
                oninput: move |event| template.write().name = event.value(),
            }
            input {
                class: "tpl-purpose",
                value: "{purpose}",
                placeholder: "What is this kind of report for?",
                "aria-label": "What this kind of report is for",
                oninput: move |event| template.write().description = event.value(),
            }

            NodeList { template, focus, parent: None, nodes, depth: 0 }
        }
    }
}

#[component]
fn NodeList(
    template: Signal<Template>,
    focus: Signal<Focus>,
    parent: Option<NodeId>,
    nodes: Vec<TemplateNode>,
    depth: u8,
) -> Element {
    // A new template has no fields at all, and the outline was then five dashed buttons
    // under two empty inputs — indistinguishable from a screen where nothing happened.
    // Saying what a template *is* turns the same buttons into an obvious next step.
    let explain_first_field = nodes.is_empty() && parent.is_none();

    rsx! {
        div { class: "tpl-list",
            for node in nodes.iter() {
                NodeCard { key: "{node.id}", template, focus, node: node.clone(), depth }
            }
            if explain_first_field {
                p { class: "tpl-hint",
                    "A template is a list of fields. Each one becomes a part of the report, and \
                     what you write in it tells the model what belongs there. Add the first:"
                }
            }
            AddRow { template, parent }
        }
    }
}

/// How many fields a delete would take along, at any depth.
fn count_nested(children: &[TemplateNode]) -> usize {
    children.iter().map(|child| 1 + child.kind.children().map_or(0, count_nested)).sum()
}

#[component]
fn NodeCard(
    template: Signal<Template>,
    focus: Signal<Focus>,
    node: TemplateNode,
    depth: u8,
) -> Element {
    let id = node.id;
    let block = block_id(id);
    let description = node.description().to_string();

    // The description is rendered through the same focus guard as any other editable
    // block, so it is frozen while the user is typing in it.
    let html = focus.read().html_for(block, &[Span::plain(description)]);
    let is_container = node.kind.is_container();
    // Only a section deepens the heading level; optional and repeat are transparent,
    // exactly as they are when the report is rendered.
    let child_depth = if matches!(node.kind, NodeKind::Section { .. }) { depth + 1 } else { depth };

    rsx! {
        div { class: "node node-{node.kind.tag()}",
            div { class: "node-head",
                span { class: "kind", "{kind_label(&node.kind)}" }
                input {
                    class: "lbl",
                    value: "{node.label}",
                    placeholder: "Field name",
                    "aria-label": "Field name",
                    oninput: move |event| {
                        template.write().set_label(id, event.value());
                    },
                }
                span { class: "acts",
                    IconButton {
                        icon: Icon::ChevronUp,
                        title: "Move up".to_string(),
                        onclick: move |_| { template.write().move_by(id, -1); },
                    }
                    IconButton {
                        icon: Icon::ChevronDown,
                        title: "Move down".to_string(),
                        onclick: move |_| { template.write().move_by(id, 1); },
                    }
                    IconButton {
                        icon: Icon::Close,
                        title: "Delete this field and everything in it".to_string(),
                        onclick: {
                            // A container takes its children with it, and the button is a
                            // small × next to two harmless move buttons. The count goes in
                            // the message because that is the part nobody sees coming.
                            let label = node.label.clone();
                            let nested = node.kind.children().map_or(0, count_nested);
                            move |_| {
                                let label = label.clone();
                                spawn(async move {
                                    let named = if label.trim().is_empty() {
                                        "this field".to_string()
                                    } else {
                                        label.clone()
                                    };
                                    let consequence = match nested {
                                        0 => confirm::NO_UNDO.to_string(),
                                        1 => format!("The field inside it goes too. {}", confirm::NO_UNDO),
                                        n => format!("The {n} fields inside it go too. {}", confirm::NO_UNDO),
                                    };
                                    if confirm::destructive("Delete field", &named, &consequence).await {
                                        template.write().remove(id);
                                    }
                                });
                            }
                        },
                    }
                }
            }

            EditableText {
                id: block,
                html,
                class: "node-desc {description_class(&node.kind, depth)}",
                placeholder: placeholder(&node.kind),
            }

            Options { template, node: node.clone() }

            if is_container {
                div { class: "kids",
                    NodeList {
                        template, focus,
                        parent: Some(id),
                        nodes: node.kind.children().unwrap_or_default().to_vec(),
                        depth: child_depth,
                    }
                }
            }
        }
    }
}

/// Per-kind settings: the bounds and flags that have no place in a description.
#[component]
fn Options(template: Signal<Template>, node: TemplateNode) -> Element {
    let id = node.id;
    match &node.kind {
        NodeKind::List { ordered, min_items, max_items, .. } => {
            let (ordered, min_items, max_items) = (*ordered, *min_items, *max_items);
            rsx! {
                div { class: "node-opts",
                    label {
                        input {
                            r#type: "checkbox",
                            checked: ordered,
                            onchange: move |event| {
                                if let Some(found) = template.write().find_mut(id) {
                                    if let NodeKind::List { ordered, .. } = &mut found.kind {
                                        *ordered = event.checked();
                                    }
                                }
                            },
                        }
                        // "numbered", not "ordered": one is what the user sees on the page,
                        // the other is what the JSON field happens to be called.
                        "numbered"
                    }
                    NumberField {
                        label: "at least".to_string(), value: min_items,
                        on_change: move |v| set_list_bounds(template, id, Some(v), None),
                    }
                    NumberField {
                        label: "at most".to_string(), value: max_items,
                        on_change: move |v| set_list_bounds(template, id, None, Some(v)),
                    }
                }
            }
        }
        NodeKind::Repeat { item_label, min, max, .. } => {
            let (item_label, min, max) = (item_label.clone(), *min, *max);
            rsx! {
                div { class: "node-opts",
                    label {
                        "one per "
                        input {
                            r#type: "text",
                            value: "{item_label}",
                            placeholder: "defect",
                            oninput: move |event| {
                                if let Some(found) = template.write().find_mut(id) {
                                    if let NodeKind::Repeat { item_label, .. } = &mut found.kind {
                                        *item_label = event.value();
                                    }
                                }
                            },
                        }
                    }
                    NumberField {
                        label: "at least".to_string(), value: min,
                        on_change: move |v| set_repeat_bounds(template, id, Some(v), None),
                    }
                    NumberField {
                        label: "at most".to_string(), value: max,
                        on_change: move |v| set_repeat_bounds(template, id, None, Some(v)),
                    }
                }
            }
        }
        _ => rsx! {},
    }
}

fn set_list_bounds(
    mut template: Signal<Template>,
    id: NodeId,
    min: Option<Option<u32>>,
    max: Option<Option<u32>>,
) {
    if let Some(node) = template.write().find_mut(id) {
        if let NodeKind::List { min_items, max_items, .. } = &mut node.kind {
            if let Some(v) = min {
                *min_items = v;
            }
            if let Some(v) = max {
                *max_items = v;
            }
        }
    }
}

fn set_repeat_bounds(
    mut template: Signal<Template>,
    id: NodeId,
    lower: Option<Option<u32>>,
    upper: Option<Option<u32>>,
) {
    if let Some(node) = template.write().find_mut(id) {
        if let NodeKind::Repeat { min, max, .. } = &mut node.kind {
            if let Some(v) = lower {
                *min = v;
            }
            if let Some(v) = upper {
                *max = v;
            }
        }
    }
}

#[component]
fn AddRow(template: Signal<Template>, parent: Option<NodeId>) -> Element {
    let add = move |kind: &'static str| {
        move |_| {
            template.write().append(parent, blank(kind));
        }
    };
    rsx! {
        div { class: "add",
            // Labelled as what they do to the report, not as what they are in the tree.
            Button { label: "+ Paragraph".to_string(), variant: Variant::Ghost, onclick: add("paragraph") }
            Button { label: "+ List".to_string(), variant: Variant::Ghost, onclick: add("list") }
            Button { label: "+ Section".to_string(), variant: Variant::Ghost, onclick: add("section") }
            Button { label: "+ Only sometimes".to_string(), variant: Variant::Ghost, onclick: add("optional") }
            Button { label: "+ Repeats".to_string(), variant: Variant::Ghost, onclick: add("repeat") }
        }
    }
}

fn blank(kind: &str) -> TemplateNode {
    match kind {
        "list" => TemplateNode::new(
            "List",
            NodeKind::List {
                description: String::new(),
                ordered: false,
                min_items: None,
                max_items: None,
            },
        ),
        "section" => TemplateNode::new(
            "Section",
            NodeKind::Section { heading_description: String::new(), children: Vec::new() },
        ),
        "optional" => TemplateNode::new(
            "Optional",
            NodeKind::Optional { description: String::new(), children: Vec::new() },
        ),
        "repeat" => TemplateNode::new(
            "Repeat",
            NodeKind::Repeat {
                description: String::new(),
                item_label: String::new(),
                min: None,
                max: None,
                children: Vec::new(),
            },
        ),
        _ => TemplateNode::new("Paragraph", NodeKind::Paragraph { description: String::new() }),
    }
}

/// What a node is, said in the words of the report rather than of the tree.
///
/// "Optional" and "Repeat" are the names of the node kinds; "Sometimes" and "Repeats" are
/// what they do to the document. Nobody arranging a report thinks in node kinds, and the
/// glyph prefixes the chips used to carry (`¶`, `§`, `↺`) were decoration on a word that
/// already said it.
fn kind_label(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::Paragraph { .. } => "Paragraph",
        NodeKind::List { .. } => "List",
        NodeKind::Section { .. } => "Section",
        NodeKind::Optional { .. } => "Sometimes",
        NodeKind::Repeat { .. } => "Repeats",
    }
}

/// Style the instruction like the thing it produces, so the template reads as the
/// document it will become.
fn description_class(kind: &NodeKind, depth: u8) -> String {
    match kind {
        NodeKind::Section { .. } => {
            let level = (depth + 1).min(report_doc::BlockKind::MAX_HEADING_LEVEL);
            format!("as-h{level}")
        }
        NodeKind::Paragraph { .. } => "as-p".into(),
        NodeKind::List { .. } => "as-list".into(),
        _ => "as-note".into(),
    }
}

fn placeholder(kind: &NodeKind) -> String {
    match kind {
        NodeKind::Paragraph { .. } => "What should this paragraph say?",
        NodeKind::List { .. } => "What should each entry cover?",
        NodeKind::Section { .. } => "What should the heading be called?",
        NodeKind::Optional { .. } => "When should this be included?",
        NodeKind::Repeat { .. } => "What is repeated, and once per what?",
    }
    .to_string()
}

fn handle(event: RawEvent, mut template: Signal<Template>, mut focus: Signal<Focus>) {
    let block = event.block();
    let node = node_id(block);
    // Every editing surface sees every event, so each has to recognise its own. A
    // block id that is not a node in this template belongs to the notes or report
    // pane instead.
    if template.read().find(node).is_none() {
        return;
    }

    match event {
        // A description is a plain instruction, so any inline formatting the browser
        // produced is dropped rather than stored and silently lost on save.
        RawEvent::Input { html, .. } => {
            let text = plain_text(&html);
            template.write().set_description(node, text);
        }
        RawEvent::Blur { html, .. } => {
            let text = plain_text(&html);
            template.write().set_description(node, text);
            focus.write().leave(block);
        }
        RawEvent::Focus { .. } => {
            let description = template.read().find(node).map(|n| n.description().to_string());
            if let Some(description) = description {
                focus.write().enter(block, &[Span::plain(description)]);
            }
        }
        // Enter, Tab and the mark shortcuts have no meaning in a one-line
        // instruction; leaving them unhandled is what makes them no-ops.
        _ => {}
    }
}

fn plain_text(html: &str) -> String {
    report_doc::html::html_to_spans(html).iter().map(|span| span.text.as_str()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_kinds_are_named_after_what_they_do_to_the_report() {
        // The jargon this redesign set out to remove. If "Optional" or "Repeat" come back
        // it is because someone reached for the enum variant's name again.
        assert_eq!(
            kind_label(&NodeKind::Optional { description: String::new(), children: vec![] }),
            "Sometimes"
        );
        assert_eq!(
            kind_label(&NodeKind::Repeat {
                description: String::new(),
                item_label: String::new(),
                min: None,
                max: None,
                children: vec![],
            }),
            "Repeats"
        );
        for kind in [
            NodeKind::Paragraph { description: String::new() },
            NodeKind::Section { heading_description: String::new(), children: vec![] },
        ] {
            let label = kind_label(&kind);
            assert!(
                !label.contains('¶') && !label.contains('§') && !label.contains('↺'),
                "the glyph prefixes were decoration on a word that already said it: {label}"
            );
        }
    }

    #[test]
    fn a_sections_instruction_is_set_in_the_heading_it_will_produce() {
        // The whole point of the outline: an instruction reads as the thing it becomes.
        assert_eq!(
            description_class(
                &NodeKind::Section { heading_description: String::new(), children: vec![] },
                0
            ),
            "as-h1"
        );
        assert_eq!(
            description_class(
                &NodeKind::Section { heading_description: String::new(), children: vec![] },
                2
            ),
            "as-h3"
        );
        // Never past the deepest heading the document model has.
        let deep = description_class(
            &NodeKind::Section { heading_description: String::new(), children: vec![] },
            250,
        );
        assert_eq!(deep, format!("as-h{}", report_doc::BlockKind::MAX_HEADING_LEVEL));
    }
}
