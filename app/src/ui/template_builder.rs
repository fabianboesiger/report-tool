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
        div { class: "tb",
            div { class: "tb-meta",
                input {
                    class: "tb-name",
                    value: "{name}",
                    placeholder: "Template name",
                    oninput: move |event| template.write().name = event.value(),
                }
                input {
                    class: "tb-purpose",
                    value: "{purpose}",
                    placeholder: "What is this kind of report for?",
                    oninput: move |event| template.write().description = event.value(),
                }
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
    rsx! {
        div { class: "tb-list",
            for node in nodes.iter() {
                NodeCard { key: "{node.id}", template, focus, node: node.clone(), depth }
            }
            AddRow { template, parent }
        }
    }
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
        div { class: "tb-node tb-{node.kind.tag()}",
            div { class: "tb-head",
                span { class: "tb-chip tb-chip-{node.kind.tag()}", "{chip(&node.kind)}" }
                input {
                    class: "tb-label",
                    value: "{node.label}",
                    placeholder: "Field name",
                    oninput: move |event| {
                        template.write().set_label(id, event.value());
                    },
                }
                // The JSON key, shown because it is what a generated report is
                // stored against and it deliberately does not follow a rename.
                span { class: "tb-key", title: "JSON key (fixed once created)", "{node.key}" }
                span { class: "tb-spacer" }
                button {
                    class: "tb-btn", title: "Move up",
                    onclick: move |_| { template.write().move_by(id, -1); },
                    "↑"
                }
                button {
                    class: "tb-btn", title: "Move down",
                    onclick: move |_| { template.write().move_by(id, 1); },
                    "↓"
                }
                button {
                    class: "tb-btn tb-danger", title: "Delete this field and everything in it",
                    onclick: move |_| { template.write().remove(id); },
                    "✕"
                }
            }

            EditableText {
                id: block,
                html,
                class: "tb-desc {description_class(&node.kind, depth)}",
                placeholder: placeholder(&node.kind),
            }

            Options { template, node: node.clone() }

            if is_container {
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

/// Per-kind settings: the bounds and flags that have no place in a description.
#[component]
fn Options(template: Signal<Template>, node: TemplateNode) -> Element {
    let id = node.id;
    match &node.kind {
        NodeKind::List { ordered, min_items, max_items, .. } => {
            let (ordered, min_items, max_items) = (*ordered, *min_items, *max_items);
            rsx! {
                div { class: "tb-opts",
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
                        " numbered"
                    }
                    Bound {
                        label: "min".to_string(), value: min_items,
                        on_change: move |v| set_list_bounds(template, id, Some(v), None),
                    }
                    Bound {
                        label: "max".to_string(), value: max_items,
                        on_change: move |v| set_list_bounds(template, id, None, Some(v)),
                    }
                }
            }
        }
        NodeKind::Repeat { item_label, min, max, .. } => {
            let (item_label, min, max) = (item_label.clone(), *min, *max);
            rsx! {
                div { class: "tb-opts",
                    input {
                        class: "tb-item-label",
                        value: "{item_label}",
                        placeholder: "one entry per… (e.g. defect)",
                        oninput: move |event| {
                            if let Some(found) = template.write().find_mut(id) {
                                if let NodeKind::Repeat { item_label, .. } = &mut found.kind {
                                    *item_label = event.value();
                                }
                            }
                        },
                    }
                    Bound {
                        label: "min".to_string(), value: min,
                        on_change: move |v| set_repeat_bounds(template, id, Some(v), None),
                    }
                    Bound {
                        label: "max".to_string(), value: max,
                        on_change: move |v| set_repeat_bounds(template, id, None, Some(v)),
                    }
                }
            }
        }
        _ => rsx! {},
    }
}

/// An optional count. Empty means unbounded, which is the common case and so is the
/// default; a `0` would mean something quite different.
#[component]
fn Bound(label: String, value: Option<u32>, on_change: EventHandler<Option<u32>>) -> Element {
    let shown = value.map(|v| v.to_string()).unwrap_or_default();
    rsx! {
        label { class: "tb-bound",
            "{label} "
            input {
                r#type: "number",
                min: "0",
                value: "{shown}",
                oninput: move |event| {
                    let text = event.value();
                    on_change.call(if text.trim().is_empty() { None } else { text.trim().parse().ok() });
                },
            }
        }
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
        div { class: "tb-add",
            button { class: "tb-add-btn", onclick: add("paragraph"), "+ Paragraph" }
            button { class: "tb-add-btn", onclick: add("list"), "+ List" }
            button { class: "tb-add-btn", onclick: add("section"), "+ Section" }
            button { class: "tb-add-btn", onclick: add("optional"), "+ Optional" }
            button { class: "tb-add-btn", onclick: add("repeat"), "+ Repeat" }
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

fn chip(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::Paragraph { .. } => "¶ Paragraph",
        NodeKind::List { .. } => "• List",
        NodeKind::Section { .. } => "§ Section",
        NodeKind::Optional { .. } => "? Optional",
        NodeKind::Repeat { .. } => "↺ Repeat",
    }
}

/// Style the instruction like the thing it produces, so the template reads as the
/// document it will become.
fn description_class(kind: &NodeKind, depth: u8) -> String {
    match kind {
        NodeKind::Section { .. } => {
            let level = (depth + 1).min(report_doc::BlockKind::MAX_HEADING_LEVEL);
            format!("tb-as-h{level}")
        }
        NodeKind::Paragraph { .. } => "tb-as-p".into(),
        NodeKind::List { .. } => "tb-as-list".into(),
        _ => "tb-as-note".into(),
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
