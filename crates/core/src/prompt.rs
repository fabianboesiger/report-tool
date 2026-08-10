//! Building the prompt that turns notes into a report.
//!
//! The two backends are constrained by different mechanisms, and only one of them
//! can carry the template's descriptions:
//!
//! - JSON Schema has a `description` per property, so a remote model sees the
//!   instructions attached to the fields themselves.
//! - **GBNF has nowhere to put them.** A grammar constrains shape and says nothing
//!   about meaning, so a locally-generated report would otherwise be structurally
//!   perfect and semantically random.
//!
//! Hence the *field guide*: the descriptions rendered as an outline in the system
//! prompt, one line per JSON path. Emitting it for both backends — rather than
//! relying on schema descriptions remotely and the guide locally — is deliberate.
//! One prompt means a template that reads well against a strong model reads the same
//! way against a local one, so prompt work done at M3 is not thrown away at M4.

use std::fmt::Write as _;

use crate::compile::{Field, Shape, HEADING_KEY};
use crate::template::Template;

/// The instructions given to the model, independent of any particular notes.
///
/// Kept free of the notes on purpose: it is byte-identical across regenerations and
/// across every report made from one template, which is exactly what the local
/// backend's KV-cache prefix reuse rewards — a second generation reuses the whole
/// prefix and skips straight to the notes.
pub fn system(template: &Template) -> String {
    let mut s = String::new();
    s.push_str(
        "You write structured reports from a practitioner's rough notes.\n\n\
         Rules:\n\
         - Use only what the notes support. Never invent findings, measurements, names or dates.\n\
         - Where the notes are silent on a field, say so plainly or keep it brief; do not pad.\n\
         - Write in the same language as the notes.\n\
         - Write prose only. Do not write headings, numbering or bullet markers: the surrounding \
           document structure is added afterwards, and anything you add would be duplicated.\n\
         - You may use **bold** and _italic_ within a passage where it genuinely aids reading.\n\n",
    );

    let _ = writeln!(s, "Report type: {}", template.name.trim());
    if !template.description.trim().is_empty() {
        let _ = writeln!(s, "Purpose: {}", template.description.trim());
    }

    let shape = Shape::compile(template);
    s.push_str("\nFill every field of the following structure.\n\n");
    s.push_str(&field_guide(&shape));
    s
}

/// The template's descriptions as an indented outline, one line per field.
pub fn field_guide(shape: &Shape) -> String {
    let mut out = String::new();
    guide(shape, 0, &mut out);
    if out.is_empty() {
        // A template with no fields yet. Saying so beats an empty section that
        // reads like a truncated prompt.
        out.push_str("(this template has no fields yet)\n");
    }
    out
}

fn guide(shape: &Shape, indent: usize, out: &mut String) {
    match shape {
        Shape::Group { fields } => {
            for field in fields {
                guide_field(field, indent, out);
            }
        }
        Shape::Section { fields, .. } => {
            for field in fields {
                guide_field(field, indent, out);
            }
        }
        _ => {}
    }
}

fn guide_field(field: &Field, indent: usize, out: &mut String) {
    let pad = "  ".repeat(indent);
    let label = &field.label;
    let description = field.shape.description().trim();

    match &field.shape {
        Shape::Text { .. } => {
            let _ = writeln!(out, "{pad}- {}: {} — {}", field.key, label, describe(description));
        }

        Shape::List { ordered, min, max, .. } => {
            let kind = if *ordered { "ordered list" } else { "list" };
            let _ = writeln!(
                out,
                "{pad}- {}: {} ({kind} of short entries{}) — {}",
                field.key,
                label,
                bounds(*min, *max, "entries"),
                describe(description)
            );
        }

        Shape::Section { fields, .. } => {
            let _ = writeln!(
                out,
                "{pad}- {}: {} (a section; \"{HEADING_KEY}\" is its title — {})",
                field.key,
                label,
                describe(description)
            );
            for child in fields {
                guide_field(child, indent + 1, out);
            }
        }

        Shape::Optional { inner, .. } => {
            let _ = writeln!(
                out,
                "{pad}- {}: {} (OPTIONAL — write null for the whole field if it does not apply. {})",
                field.key,
                label,
                describe(description)
            );
            guide(inner, indent + 1, out);
        }

        Shape::Repeat { item_label, min, max, item, .. } => {
            let unit = if item_label.trim().is_empty() { "entry" } else { item_label.trim() };
            let _ = writeln!(
                out,
                "{pad}- {}: {} (REPEATED — one entry per {unit} in the notes{}) — {}",
                field.key,
                label,
                bounds(*min, *max, "entries"),
                describe(description)
            );
            guide(item, indent + 1, out);
        }

        Shape::Group { fields } => {
            for child in fields {
                guide_field(child, indent, out);
            }
        }
    }
}

fn describe(description: &str) -> &str {
    if description.is_empty() {
        // Better than a dangling dash: an empty description is a gap in the
        // template, and the model should treat the label as the whole instruction.
        "no further guidance; follow the field name"
    } else {
        description
    }
}

fn bounds(min: Option<u32>, max: Option<u32>, unit: &str) -> String {
    match (min, max) {
        (Some(a), Some(b)) if a == b => format!(", exactly {a} {unit}"),
        (Some(a), Some(b)) => format!(", {a} to {b} {unit}"),
        (Some(a), None) => format!(", at least {a} {unit}"),
        (None, Some(b)) => format!(", at most {b} {unit}"),
        (None, None) => String::new(),
    }
}

/// The user turn: the notes, as markdown.
pub fn user(notes_markdown: &str) -> String {
    let notes = notes_markdown.trim();
    if notes.is_empty() {
        // An empty notes pane is a real state — the user pressed Generate too early.
        // Saying so explicitly beats sending a blank turn and getting invention back.
        return "The notes are empty. Produce the structure with each field left \
                as a brief placeholder stating that no information was recorded."
            .to_string();
    }
    format!("Notes:\n\n{notes}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::fixture;

    #[test]
    fn every_description_reaches_the_prompt() {
        // This is the only route a description has to a locally-generated report:
        // GBNF cannot carry them, so anything missing here is invisible to the model.
        let template = fixture::template();
        let prompt = system(&template);
        for (node, _) in template.walk() {
            let description = node.description();
            assert!(
                prompt.contains(description),
                "description {description:?} for {:?} never reached the prompt:\n{prompt}",
                node.label
            );
        }
    }

    #[test]
    fn the_guide_marks_optional_and_repeated_fields() {
        let guide = field_guide(&Shape::compile(&fixture::template()));
        assert!(guide.contains("OPTIONAL"), "{guide}");
        assert!(guide.contains("write null"), "{guide}");
        assert!(guide.contains("REPEATED"), "{guide}");
        assert!(guide.contains("one entry per defect"), "{guide}");
    }

    #[test]
    fn the_guide_states_the_bounds_the_schema_cannot() {
        let guide = field_guide(&Shape::compile(&fixture::template()));
        assert!(guide.contains("at least 1 entries"), "{guide}");
        assert!(guide.contains("1 to 5 entries"), "{guide}");
    }

    #[test]
    fn the_guide_is_indented_to_show_nesting() {
        let guide = field_guide(&Shape::compile(&fixture::template()));
        let line = guide.lines().find(|l| l.contains("overview")).unwrap();
        assert!(line.starts_with("  - "), "nested fields must be indented: {line:?}");
        let line = guide.lines().find(|l| l.contains("detail")).unwrap();
        assert!(line.starts_with("      - "), "{line:?}");
    }

    #[test]
    fn the_guide_uses_the_json_keys_the_model_must_actually_emit() {
        let guide = field_guide(&Shape::compile(&fixture::template()));
        assert!(guide.contains("follow_up:"), "keys, not labels: {guide}");
        assert!(guide.contains("next_steps:"));
    }

    #[test]
    fn the_prompt_forbids_the_structure_the_renderer_supplies() {
        // A model that writes its own headings would have them rendered inside a
        // paragraph, duplicating the heading the template already produces.
        let prompt = system(&fixture::template());
        assert!(prompt.contains("Do not write headings"));
    }

    #[test]
    fn the_system_prompt_is_stable_across_calls() {
        // Byte-identical prompts are what let the local backend reuse its KV cache
        // on a regeneration; any variation here silently costs a full prefill.
        let template = fixture::template();
        assert_eq!(system(&template), system(&template));
    }

    #[test]
    fn the_system_prompt_carries_no_notes() {
        // The notes belong in the user turn: keeping them out is what makes the
        // prefix reusable across every report built from one template.
        let template = fixture::template();
        let prompt = system(&template);
        assert!(!prompt.contains("Notes:"));
        assert!(user("some notes").contains("some notes"));
    }

    #[test]
    fn an_empty_template_still_produces_a_coherent_prompt() {
        let prompt = system(&Template::new("Blank"));
        assert!(prompt.contains("no fields yet"), "{prompt}");
    }

    #[test]
    fn empty_notes_are_stated_rather_than_sent_blank() {
        assert!(user("   ").contains("notes are empty"));
    }

    #[test]
    fn a_field_with_no_description_still_reads_as_an_instruction() {
        use crate::template::{NodeKind, TemplateNode};
        let mut t = Template::new("t");
        t.nodes = vec![TemplateNode::new(
            "Conclusion",
            NodeKind::Paragraph { description: String::new() },
        )];
        let guide = field_guide(&Shape::compile(&t));
        assert!(!guide.contains("— \n"), "no dangling dash: {guide:?}");
        assert!(guide.contains("follow the field name"), "{guide}");
    }
}
