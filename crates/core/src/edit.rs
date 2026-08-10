//! Structural edits to a template, as pure functions.
//!
//! The same split as `report_doc::ops`: the builder UI decides *when* an edit
//! happens, and this decides *what* it does. Tree surgery is where the awkward cases
//! live — moving a node past a container, deleting the node a sibling's key was
//! deduplicated against — and here they are unit tests rather than things to
//! rediscover by clicking.
//!
//! Edits are addressed by index path rather than by mutable recursive search. A
//! function returning `&mut` from inside a loop over children is the shape the borrow
//! checker rejects without Polonius; finding the path immutably and then walking it
//! mutably sidesteps that entirely and is easier to read besides.

use crate::template::{deduplicate_keys, NodeId, Template, TemplateNode};

impl Template {
    pub fn find_mut(&mut self, id: NodeId) -> Option<&mut TemplateNode> {
        let path = path_to(&self.nodes, id)?;
        let index = *path.last()?;
        parent_of_mut(&mut self.nodes, &path)?.get_mut(index)
    }

    /// The children of `parent`, or the root nodes when it is `None`.
    pub fn children_mut(&mut self, parent: Option<NodeId>) -> Option<&mut Vec<TemplateNode>> {
        match parent {
            None => Some(&mut self.nodes),
            Some(id) => self.find_mut(id)?.kind.children_mut(),
        }
    }

    /// Append a node, keeping sibling keys unique.
    ///
    /// Returns false when `parent` is not a container — a paragraph cannot hold
    /// children, and quietly dropping the node would look like a broken button.
    pub fn append(&mut self, parent: Option<NodeId>, node: TemplateNode) -> bool {
        let Some(children) = self.children_mut(parent) else { return false };
        children.push(node);
        // Two siblings sharing a key would silently overwrite each other in the
        // generated JSON, losing a whole field with no error anywhere.
        deduplicate_keys(children);
        true
    }

    /// Remove a node and everything under it, returning what was removed.
    pub fn remove(&mut self, id: NodeId) -> Option<TemplateNode> {
        let path = path_to(&self.nodes, id)?;
        let index = *path.last()?;
        let siblings = parent_of_mut(&mut self.nodes, &path)?;
        if index >= siblings.len() {
            return None;
        }
        // Surviving siblings keep the keys they already have: a key is what a
        // generated report is stored against, so renumbering here would break every
        // report already made from this template.
        Some(siblings.remove(index))
    }

    /// Move a node among its siblings. `delta` is -1 for up, 1 for down.
    ///
    /// Deliberately confined to one level: dragging a node into or out of a container
    /// changes which object its key lives in, which is a different operation with
    /// different consequences for existing reports.
    pub fn move_by(&mut self, id: NodeId, delta: i32) -> bool {
        let Some(path) = path_to(&self.nodes, id) else { return false };
        let index = *path.last().expect("a path always ends at the node");
        let Some(siblings) = parent_of_mut(&mut self.nodes, &path) else { return false };

        let target = index as i32 + delta;
        if target < 0 || target as usize >= siblings.len() {
            return false;
        }
        siblings.swap(index, target as usize);
        true
    }

    /// Replace a node's instruction to the model.
    pub fn set_description(&mut self, id: NodeId, text: String) -> bool {
        use crate::template::NodeKind::*;
        let Some(node) = self.find_mut(id) else { return false };
        match &mut node.kind {
            Paragraph { description }
            | List { description, .. }
            | Optional { description, .. }
            | Repeat { description, .. } => *description = text,
            Section { heading_description, .. } => *heading_description = text,
        }
        true
    }

    /// Rename a node.
    ///
    /// The key deliberately does **not** follow the label. A key is the address a
    /// generated report is stored against, so moving it on a rename would leave every
    /// existing report unable to render — and renaming a field is exactly the kind of
    /// tidying a user does long after the first reports are written.
    pub fn set_label(&mut self, id: NodeId, label: String) -> bool {
        let Some(node) = self.find_mut(id) else { return false };
        node.label = label;
        true
    }

    /// The section nesting depth of a node, which is the heading level it renders at.
    pub fn depth_of(&self, id: NodeId) -> Option<u8> {
        self.walk().into_iter().find(|(node, _)| node.id == id).map(|(_, depth)| depth)
    }
}

/// Indices from the root down to `id`; the last entry is its position among siblings.
fn path_to(nodes: &[TemplateNode], id: NodeId) -> Option<Vec<usize>> {
    for (index, node) in nodes.iter().enumerate() {
        if node.id == id {
            return Some(vec![index]);
        }
        if let Some(children) = node.kind.children() {
            if let Some(rest) = path_to(children, id) {
                let mut path = Vec::with_capacity(rest.len() + 1);
                path.push(index);
                path.extend(rest);
                return Some(path);
            }
        }
    }
    None
}

/// The sibling list a path ends in.
fn parent_of_mut<'a>(
    nodes: &'a mut Vec<TemplateNode>,
    path: &[usize],
) -> Option<&'a mut Vec<TemplateNode>> {
    let mut current = nodes;
    for &index in &path[..path.len().saturating_sub(1)] {
        current = current.get_mut(index)?.kind.children_mut()?;
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::{fixture, NodeKind};

    fn paragraph(label: &str) -> TemplateNode {
        TemplateNode::new(label, NodeKind::Paragraph { description: String::new() })
    }

    fn labels(nodes: &[TemplateNode]) -> Vec<&str> {
        nodes.iter().map(|n| n.label.as_str()).collect()
    }

    #[test]
    fn append_adds_to_the_root_and_to_containers() {
        let mut t = fixture::template();
        assert!(t.append(None, paragraph("Conclusion")));
        assert_eq!(labels(&t.nodes).last(), Some(&"Conclusion"));

        let section = t.nodes[1].id;
        assert!(t.append(Some(section), paragraph("Extra")));
        assert_eq!(labels(t.nodes[1].kind.children().unwrap()).last(), Some(&"Extra"));
    }

    #[test]
    fn appending_to_a_leaf_is_refused_rather_than_silently_dropped() {
        let mut t = fixture::template();
        let leaf = t.nodes[0].id;
        assert!(!t.append(Some(leaf), paragraph("Nope")));
    }

    #[test]
    fn appended_siblings_never_share_a_key() {
        // A shared key would make one field overwrite the other in the generated
        // JSON, losing it with no error.
        let mut t = Template::new("t");
        for _ in 0..3 {
            assert!(t.append(None, paragraph("Notes")));
        }
        let keys: Vec<&str> = t.nodes.iter().map(|n| n.key.as_str()).collect();
        assert_eq!(keys, ["notes", "notes_2", "notes_3"]);
    }

    #[test]
    fn remove_takes_the_node_and_its_children() {
        let mut t = fixture::template();
        let section = t.nodes[1].id;
        let removed = t.remove(section).unwrap();
        assert_eq!(removed.label, "Findings");
        assert_eq!(removed.kind.children().unwrap().len(), 2, "children come with it");
        assert_eq!(labels(&t.nodes), ["Summary", "Follow-up"]);
    }

    #[test]
    fn remove_reaches_nested_nodes() {
        let mut t = fixture::template();
        let nested = t.nodes[1].kind.children().unwrap()[0].id;
        assert_eq!(t.remove(nested).unwrap().label, "Overview");
        assert_eq!(labels(t.nodes[1].kind.children().unwrap()), ["Defects"]);
    }

    #[test]
    fn removing_a_sibling_leaves_the_others_keys_alone() {
        // Keys are what generated reports are stored against; renumbering here would
        // break every report already made from this template.
        let mut t = Template::new("t");
        for _ in 0..3 {
            t.append(None, paragraph("Notes"));
        }
        let second = t.nodes[1].id;
        t.remove(second);
        let keys: Vec<&str> = t.nodes.iter().map(|n| n.key.as_str()).collect();
        assert_eq!(keys, ["notes", "notes_3"]);
    }

    #[test]
    fn move_by_reorders_among_siblings() {
        let mut t = fixture::template();
        let follow_up = t.nodes[2].id;
        assert!(t.move_by(follow_up, -1));
        assert_eq!(labels(&t.nodes), ["Summary", "Follow-up", "Findings"]);
        assert!(t.move_by(follow_up, 1));
        assert_eq!(labels(&t.nodes), ["Summary", "Findings", "Follow-up"]);
    }

    #[test]
    fn move_by_stops_at_the_ends_instead_of_escaping_the_container() {
        let mut t = fixture::template();
        let first = t.nodes[0].id;
        assert!(!t.move_by(first, -1));
        let last = t.nodes[2].id;
        assert!(!t.move_by(last, 1));
        assert_eq!(labels(&t.nodes), ["Summary", "Findings", "Follow-up"]);
    }

    #[test]
    fn move_by_operates_inside_the_containing_list_only() {
        let mut t = fixture::template();
        let nested = t.nodes[1].kind.children().unwrap()[1].id;
        assert!(t.move_by(nested, -1));
        assert_eq!(labels(t.nodes[1].kind.children().unwrap()), ["Defects", "Overview"]);
        // The root is untouched.
        assert_eq!(labels(&t.nodes), ["Summary", "Findings", "Follow-up"]);
    }

    #[test]
    fn descriptions_can_be_set_on_every_node_kind() {
        let mut t = fixture::template();
        for (node, _) in t.clone().walk() {
            assert!(t.set_description(node.id, "updated".into()), "{:?}", node.label);
            assert_eq!(t.find_mut(node.id).unwrap().description(), "updated");
        }
    }

    #[test]
    fn renaming_a_node_does_not_move_its_key() {
        // Renaming is exactly the kind of tidying done long after the first reports
        // exist; moving the key would leave all of them unable to render.
        let mut t = fixture::template();
        let id = t.nodes[0].id;
        let key = t.nodes[0].key.clone();
        assert!(t.set_label(id, "Executive summary".into()));
        assert_eq!(t.nodes[0].label, "Executive summary");
        assert_eq!(t.nodes[0].key, key);
    }

    #[test]
    fn edits_to_a_missing_node_are_refused_rather_than_panicking() {
        let mut t = fixture::template();
        let ghost = NodeId::new();
        assert!(!t.set_label(ghost, "x".into()));
        assert!(!t.set_description(ghost, "x".into()));
        assert!(!t.move_by(ghost, 1));
        assert!(t.remove(ghost).is_none());
        assert!(t.find_mut(ghost).is_none());
    }

    #[test]
    fn depth_matches_the_heading_level_a_section_will_render_at() {
        let t = fixture::template();
        let findings = t.nodes[1].id;
        let location = t.nodes[1].kind.children().unwrap()[1].kind.children().unwrap()[0].id;
        assert_eq!(t.depth_of(findings), Some(0));
        // A repeat wraps it, and repeats are transparent for heading level.
        assert_eq!(t.depth_of(location), Some(1));
    }

    #[test]
    fn an_edited_template_still_compiles_and_renders() {
        // The end-to-end contract: whatever the builder does, the result must still
        // produce a usable schema, grammar and prompt.
        let mut t = fixture::template();
        t.append(None, paragraph("Conclusion"));
        t.remove(t.nodes[1].id);
        t.set_description(t.nodes[0].id, "A short summary.".into());

        let shape = crate::compile::Shape::compile(&t);
        assert!(shape.to_json_schema()["properties"].get("conclusion").is_some());
        assert!(shape.to_gbnf().contains("conclusion"));
        assert!(crate::prompt::system(&t).contains("A short summary."));
    }
}
