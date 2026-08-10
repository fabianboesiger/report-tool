//! The template model: the structure of a report, and the intent of each part.
//!
//! A template says *what a report is made of* and *what each piece should contain*,
//! never what it says. The user writes a description per node — "summarise the
//! condition of the facade in two or three sentences" — and the model writes the
//! prose to fit.
//!
//! ## Why the containers are what they are
//!
//! - [`NodeKind::Section`] groups content under a heading. Its level is its **depth
//!   among enclosing sections**, computed rather than stored, so nesting a section
//!   inside another cannot leave the heading level stale.
//! - [`NodeKind::Optional`] is content that may be omitted. It compiles to a
//!   nullable object, so "leave this out" is a value the model can return rather
//!   than an instruction it may ignore.
//! - [`NodeKind::Repeat`] is content that occurs an unknown number of times — one
//!   group per defect, per room, per attendee. It compiles to an array, so the count
//!   comes from the notes rather than from a guess baked into the template.
//!
//! `Optional` and `Repeat` are **transparent for heading levels**: wrapping a
//! section in a repeat must not push its heading a level deeper, because the reader
//! of the finished report sees a list of sections at one level, not a hierarchy.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable identity for a template node, used as the key in the builder UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub Uuid);

impl NodeId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A report template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Template {
    #[serde(default)]
    pub id: Uuid,
    pub name: String,
    /// What this kind of report is for. Goes into the system prompt as the framing
    /// for everything else.
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub nodes: Vec<TemplateNode>,
}

impl Template {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: String::new(),
            nodes: Vec::new(),
        }
    }

    /// Depth-first walk over every node, paired with its section depth.
    ///
    /// The single traversal both compilers and the renderer agree on. Section depth
    /// counts only enclosing [`NodeKind::Section`]s — see the module docs.
    pub fn walk(&self) -> Vec<(&TemplateNode, u8)> {
        let mut out = Vec::new();
        collect(&self.nodes, 0, &mut out);
        out
    }

    pub fn find(&self, id: NodeId) -> Option<&TemplateNode> {
        self.walk().into_iter().map(|(n, _)| n).find(|n| n.id == id)
    }
}

fn collect<'a>(nodes: &'a [TemplateNode], depth: u8, out: &mut Vec<(&'a TemplateNode, u8)>) {
    for node in nodes {
        out.push((node, depth));
        if let Some(children) = node.kind.children() {
            // Only a section deepens the heading level; optional and repeat are
            // transparent.
            let next =
                if matches!(node.kind, NodeKind::Section { .. }) { depth + 1 } else { depth };
            collect(children, next, out);
        }
    }
}

/// One node of a template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateNode {
    #[serde(default)]
    pub id: NodeId,
    /// The JSON key this node occupies in the generated object.
    ///
    /// Held separately from `label` and never derived at compile time, because it is
    /// the one thing the schema, the grammar and the renderer must all agree on: if
    /// it moved when the user retitled a node, a report generated a moment earlier
    /// would no longer render.
    pub key: String,
    /// Human-readable name, shown in the builder.
    pub label: String,
    #[serde(flatten)]
    pub kind: NodeKind,
}

impl TemplateNode {
    /// Build a node, deriving its key from the label.
    ///
    /// Callers that already hold a key (loading from disk, renaming a label) must
    /// keep it — see the `key` field docs.
    pub fn new(label: impl Into<String>, kind: NodeKind) -> Self {
        let label = label.into();
        Self { id: NodeId::new(), key: slug(&label), label, kind }
    }

    /// The node's instruction to the model.
    pub fn description(&self) -> &str {
        match &self.kind {
            NodeKind::Paragraph { description }
            | NodeKind::List { description, .. }
            | NodeKind::Optional { description, .. }
            | NodeKind::Repeat { description, .. } => description,
            NodeKind::Section { heading_description, .. } => heading_description,
        }
    }
}

/// What a node produces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeKind {
    /// One or more sentences of prose.
    Paragraph { description: String },
    /// A bulleted or numbered list.
    List {
        description: String,
        #[serde(default)]
        ordered: bool,
        #[serde(default)]
        min_items: Option<u32>,
        #[serde(default)]
        max_items: Option<u32>,
    },
    /// A heading plus nested content. The heading text is generated from
    /// `heading_description`; its *level* comes from nesting depth.
    Section {
        heading_description: String,
        #[serde(default)]
        children: Vec<TemplateNode>,
    },
    /// Content that may be omitted entirely.
    Optional {
        /// When to include it — "only if defects were found".
        description: String,
        #[serde(default)]
        children: Vec<TemplateNode>,
    },
    /// Content repeated once per occurrence of something in the notes.
    Repeat {
        description: String,
        /// What one repetition is, for the builder UI and the prompt ("one per
        /// defect").
        #[serde(default)]
        item_label: String,
        #[serde(default)]
        min: Option<u32>,
        #[serde(default)]
        max: Option<u32>,
        #[serde(default)]
        children: Vec<TemplateNode>,
    },
}

impl NodeKind {
    pub fn children(&self) -> Option<&[TemplateNode]> {
        match self {
            NodeKind::Section { children, .. }
            | NodeKind::Optional { children, .. }
            | NodeKind::Repeat { children, .. } => Some(children),
            _ => None,
        }
    }

    pub fn children_mut(&mut self) -> Option<&mut Vec<TemplateNode>> {
        match self {
            NodeKind::Section { children, .. }
            | NodeKind::Optional { children, .. }
            | NodeKind::Repeat { children, .. } => Some(children),
            _ => None,
        }
    }

    pub fn is_container(&self) -> bool {
        self.children().is_some()
    }

    /// Short label for the builder's container chips.
    pub fn tag(&self) -> &'static str {
        match self {
            NodeKind::Paragraph { .. } => "paragraph",
            NodeKind::List { .. } => "list",
            NodeKind::Section { .. } => "section",
            NodeKind::Optional { .. } => "optional",
            NodeKind::Repeat { .. } => "repeat",
        }
    }
}

/// Derive a JSON-safe key from a human label.
///
/// Kept ASCII and conservative because the key travels into a GBNF grammar as a
/// literal string, where a stray quote or backslash would produce a grammar that
/// either fails to parse or — worse — parses into something subtly wrong.
pub fn slug(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut prev_underscore = false;
    for ch in label.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_underscore = false;
        } else if !prev_underscore && !out.is_empty() {
            out.push('_');
            prev_underscore = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        // Non-ASCII labels ("Zustandsbeurteilung" is fine, "状態" is not) would
        // otherwise collapse to nothing and collide with each other.
        out.push_str("field");
    }
    out
}

/// Make every key unique among its siblings, in place.
///
/// Duplicate keys are not a cosmetic problem: two siblings sharing one key produce a
/// JSON object where the second silently overwrites the first, so a whole section of
/// the report would vanish with no error anywhere.
pub fn deduplicate_keys(nodes: &mut [TemplateNode]) {
    let mut seen: Vec<String> = Vec::new();
    for node in nodes.iter_mut() {
        if node.key.is_empty() {
            node.key = slug(&node.label);
        }
        if seen.contains(&node.key) {
            let base = node.key.clone();
            let mut n = 2;
            while seen.contains(&format!("{base}_{n}")) {
                n += 1;
            }
            node.key = format!("{base}_{n}");
        }
        seen.push(node.key.clone());
        if let Some(children) = node.kind.children_mut() {
            deduplicate_keys(children);
        }
    }
}

#[cfg(test)]
pub(crate) mod fixture {
    use super::*;

    /// A template exercising all five node kinds and both kinds of nesting.
    ///
    /// Shared by the schema, grammar and renderer tests so that all three are
    /// checked against the *same* structure — which is what makes their agreement
    /// meaningful rather than three independent assertions.
    pub fn template() -> Template {
        let mut t = Template::new("Site Inspection");
        t.description = "A record of a building inspection visit.".into();
        t.nodes = vec![
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
                            NodeKind::Paragraph {
                                description: "What was observed overall.".into(),
                            },
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
                                                    description:
                                                        "What is wrong and how severe it is.".into(),
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
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_produces_json_and_grammar_safe_keys() {
        assert_eq!(slug("Follow-up actions"), "follow_up_actions");
        assert_eq!(slug("  Summary  "), "summary");
        assert_eq!(slug("A/B \"test\""), "a_b_test");
        assert_eq!(slug("Grüezi"), "gr_ezi");
        // A label with nothing ASCII in it must still yield a usable key.
        assert_eq!(slug("状態"), "field");
        assert_eq!(slug(""), "field");
    }

    #[test]
    fn duplicate_sibling_keys_are_made_unique() {
        // Two siblings sharing a key would silently overwrite each other in the
        // generated JSON, losing a whole section with no error.
        let mut nodes = vec![
            TemplateNode::new("Notes", NodeKind::Paragraph { description: String::new() }),
            TemplateNode::new("Notes", NodeKind::Paragraph { description: String::new() }),
            TemplateNode::new("Notes", NodeKind::Paragraph { description: String::new() }),
        ];
        deduplicate_keys(&mut nodes);
        assert_eq!(
            nodes.iter().map(|n| n.key.as_str()).collect::<Vec<_>>(),
            ["notes", "notes_2", "notes_3"]
        );
    }

    #[test]
    fn keys_only_have_to_be_unique_among_siblings() {
        // Nesting scopes the key, so the same label in two sections is fine.
        let mut nodes = vec![
            TemplateNode::new(
                "A",
                NodeKind::Section {
                    heading_description: String::new(),
                    children: vec![TemplateNode::new(
                        "Notes",
                        NodeKind::Paragraph { description: String::new() },
                    )],
                },
            ),
            TemplateNode::new(
                "B",
                NodeKind::Section {
                    heading_description: String::new(),
                    children: vec![TemplateNode::new(
                        "Notes",
                        NodeKind::Paragraph { description: String::new() },
                    )],
                },
            ),
        ];
        deduplicate_keys(&mut nodes);
        let key = |n: &TemplateNode| n.kind.children().unwrap()[0].key.clone();
        assert_eq!(key(&nodes[0]), "notes");
        assert_eq!(key(&nodes[1]), "notes");
    }

    #[test]
    fn section_depth_counts_only_sections() {
        let t = fixture::template();
        let depths: Vec<(String, u8)> =
            t.walk().into_iter().map(|(n, d)| (n.key.clone(), d)).collect();

        let depth_of = |key: &str| depths.iter().find(|(k, _)| k == key).unwrap().1;
        assert_eq!(depth_of("summary"), 0);
        assert_eq!(depth_of("findings"), 0);
        assert_eq!(depth_of("overview"), 1, "inside one section");
        // The repeat wrapping it must not push the nested section deeper: the
        // reader sees a list of sections at one level, not a hierarchy.
        assert_eq!(depth_of("defects"), 1);
        assert_eq!(depth_of("location"), 1, "repeat is transparent for heading level");
        assert_eq!(depth_of("detail"), 2);
        // Likewise for optional.
        assert_eq!(depth_of("follow_up"), 0);
        assert_eq!(depth_of("next_steps"), 0);
    }

    #[test]
    fn a_template_round_trips_through_json() {
        let t = fixture::template();
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(serde_json::from_str::<Template>(&json).unwrap(), t);
    }
}
