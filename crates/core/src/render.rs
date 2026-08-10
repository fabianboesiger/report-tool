//! Rendering a generated value into a document.
//!
//! This is where the design pays off. The model returns only *text*, keyed by the
//! template's field names; every structural decision — which headings exist, what
//! level each one sits at, the order of everything, whether a list is bulleted or
//! numbered — is made here, from the template. A model cannot invent a heading, skip
//! a section or nest one wrongly, because it never had the opportunity to express
//! any of that.
//!
//! ## Why generated strings are still parsed as markdown
//!
//! A model asked for "two or three sentences" will sometimes return two paragraphs,
//! or reach for `**emphasis**` because the field's description invited it. Passing
//! that through verbatim would put literal asterisks in the report. So each string is
//! parsed as markdown — but *demoted*: a heading the model wrote inside a paragraph
//! field becomes a paragraph, and a string destined for a list item or a heading is
//! flattened to inline content. The template keeps its authority over structure while
//! the model's inline formatting survives.

use report_doc::{Block, BlockKind, RichDoc, Span};
use serde_json::Value;

use crate::compile::{Field, Shape, ShapeError, HEADING_KEY};
use crate::template::Template;

/// Compile `template`, check `value` against it, and render the document.
pub fn render(template: &Template, value: &Value) -> Result<RichDoc, ShapeError> {
    let shape = Shape::compile(template);
    shape.accepts(value)?;
    Ok(render_shape(&shape, value))
}

/// Render a value already known to fit `shape`.
///
/// Total by construction: anything that does not fit is skipped rather than
/// panicking, so a value that slipped past validation still produces a report the
/// user can fix by hand rather than an error page.
pub fn render_shape(shape: &Shape, value: &Value) -> RichDoc {
    let mut blocks = Vec::new();
    emit(shape, value, 0, &mut blocks);
    if blocks.is_empty() {
        return RichDoc::empty_paragraph();
    }
    let mut doc = RichDoc::from_blocks(blocks);
    doc.normalize();
    doc
}

/// `depth` is the number of enclosing sections, so a section's own heading sits at
/// `depth + 1`. Optional and repeat containers pass it through unchanged — see the
/// note on transparency in [`crate::template`].
fn emit(shape: &Shape, value: &Value, depth: u8, out: &mut Vec<Block>) {
    match shape {
        Shape::Text { .. } => {
            if let Some(text) = value.as_str() {
                out.extend(prose(text));
            }
        }

        Shape::List { ordered, .. } => {
            let Some(items) = value.as_array() else { return };
            for item in items {
                let Some(text) = item.as_str() else { continue };
                let spans = inline(text);
                if spans.is_empty() {
                    continue;
                }
                let kind = if *ordered {
                    BlockKind::NumberedItem { indent: 0 }
                } else {
                    BlockKind::BulletItem { indent: 0 }
                };
                out.push(Block::new(kind, spans));
            }
        }

        Shape::Section { fields, .. } => {
            let Some(obj) = value.as_object() else { return };
            if let Some(heading) = obj.get(HEADING_KEY).and_then(Value::as_str) {
                let spans = inline(heading);
                if !spans.is_empty() {
                    // Clamp rather than overflow: markdown has six heading levels,
                    // and a template nested deeper must still produce a heading.
                    let level = (depth + 1).min(BlockKind::MAX_HEADING_LEVEL);
                    out.push(Block::new(BlockKind::Heading { level }, spans));
                }
            }
            emit_fields(fields, obj, depth + 1, out);
        }

        Shape::Group { fields } => {
            let Some(obj) = value.as_object() else { return };
            emit_fields(fields, obj, depth, out);
        }

        Shape::Optional { inner, .. } => {
            // Null is the model saying "this does not apply", and it must leave no
            // trace at all — not an empty heading, not a blank line.
            if !value.is_null() {
                emit(inner, value, depth, out);
            }
        }

        Shape::Repeat { item, .. } => {
            let Some(entries) = value.as_array() else { return };
            for entry in entries {
                emit(item, entry, depth, out);
            }
        }
    }
}

fn emit_fields(
    fields: &[Field],
    obj: &serde_json::Map<String, Value>,
    depth: u8,
    out: &mut Vec<Block>,
) {
    // Iterate the fields, not the object: the template decides the order of the
    // report, and a model that returned its keys in some other order changes nothing.
    for field in fields {
        // A missing key is the same as an explicit null, which only an optional can
        // legitimately be — and validation has already rejected anything else.
        if let Some(value) = obj.get(&field.key) {
            emit(&field.shape, value, depth, out);
        }
    }
}

/// Parse a generated string into block content, demoting any headings it contains.
fn prose(text: &str) -> Vec<Block> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let mut doc = report_doc::markdown::from_markdown(text);
    for block in &mut doc.blocks {
        // Only the template may create headings.
        if matches!(block.kind, BlockKind::Heading { .. }) {
            block.kind = BlockKind::Paragraph;
        }
    }
    doc.blocks.into_iter().filter(|b| !b.is_empty()).collect()
}

/// Parse a generated string down to inline content only.
///
/// Used where the surroundings are already a single block — a list item, a heading —
/// so any block structure the model produced is flattened into one run of text.
fn inline(text: &str) -> Vec<Span> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let doc = report_doc::markdown::from_markdown(text);
    let mut spans: Vec<Span> = Vec::new();
    for (i, block) in doc.blocks.iter().enumerate() {
        if i > 0 && !spans.is_empty() {
            spans.push(Span::plain(" "));
        }
        spans.extend(block.content.iter().cloned());
    }
    let mut block = Block::new(BlockKind::Paragraph, spans);
    block.normalize();
    block.content
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::{fixture, NodeKind, TemplateNode};
    use report_doc::markdown::to_markdown;
    use report_doc::Marks;
    use serde_json::json;

    fn sample() -> Value {
        json!({
            "summary": "A routine inspection of the east wing.",
            "findings": {
                "heading": "East Wing",
                "overview": "Generally sound, with localised damage.",
                "defects": [
                    {
                        "location": {
                            "heading": "North wall",
                            "detail": "Hairline cracking below the second window."
                        },
                        "actions": ["Monitor for three months", "Re-point the joints"]
                    },
                    {
                        "location": { "heading": "Roof", "detail": "Two slipped tiles." },
                        "actions": ["Replace the tiles"]
                    }
                ]
            },
            "follow_up": { "next_steps": ["Book a follow-up for March"] }
        })
    }

    #[test]
    fn the_rendered_report_matches_the_template_exactly() {
        let doc = render(&fixture::template(), &sample()).unwrap();
        assert_eq!(
            to_markdown(&doc),
            "\
A routine inspection of the east wing.

# East Wing

Generally sound, with localised damage.

## North wall

Hairline cracking below the second window.

1. Monitor for three months
2. Re-point the joints

## Roof

Two slipped tiles.

1. Replace the tiles

- Book a follow-up for March
"
        );
    }

    #[test]
    fn heading_levels_follow_section_depth_not_the_model() {
        let doc = render(&fixture::template(), &sample()).unwrap();
        let levels: Vec<u8> = doc
            .blocks
            .iter()
            .filter_map(|b| match b.kind {
                BlockKind::Heading { level } => Some(level),
                _ => None,
            })
            .collect();
        // "East Wing" is a top-level section; each defect's location sits one deeper,
        // and the repeat wrapping them does not add a level of its own.
        assert_eq!(levels, [1, 2, 2]);
    }

    #[test]
    fn a_repeat_emits_one_group_per_entry() {
        let doc = render(&fixture::template(), &sample()).unwrap();
        let text = to_markdown(&doc);
        assert!(text.contains("North wall"));
        assert!(text.contains("Roof"));
        assert_eq!(text.matches("## ").count(), 2);
    }

    #[test]
    fn an_empty_repeat_emits_nothing_where_the_template_allows_it() {
        let mut t = fixture::template();
        // Relax the fixture's minimum so an empty array is legal here.
        if let NodeKind::Section { children, .. } = &mut t.nodes[1].kind {
            if let NodeKind::Repeat { min, .. } = &mut children[1].kind {
                *min = None;
            }
        }
        let mut value = sample();
        value["findings"]["defects"] = json!([]);
        let text = to_markdown(&render(&t, &value).unwrap());
        assert!(!text.contains("##"), "no defect headings should appear: {text}");
        assert!(text.contains("# East Wing"), "the enclosing section stays");
    }

    #[test]
    fn a_null_optional_leaves_no_trace() {
        let mut value = sample();
        value["follow_up"] = Value::Null;
        let text = to_markdown(&render(&fixture::template(), &value).unwrap());
        assert!(!text.contains("Book a follow-up"));
        // Not even a stray blank line or empty bullet.
        assert!(text.ends_with("1. Replace the tiles\n"), "{text:?}");
    }

    #[test]
    fn an_absent_optional_key_is_treated_like_null() {
        let mut value = sample();
        value.as_object_mut().unwrap().remove("follow_up");
        assert!(render(&fixture::template(), &value).is_ok());
    }

    #[test]
    fn field_order_comes_from_the_template_not_the_response() {
        // A model that returns its keys in another order must not reorder the report.
        let mut reordered = serde_json::Map::new();
        let value = sample();
        let obj = value.as_object().unwrap();
        for key in ["follow_up", "findings", "summary"] {
            reordered.insert(key.to_string(), obj[key].clone());
        }
        let doc = render(&fixture::template(), &Value::Object(reordered)).unwrap();
        let text = to_markdown(&doc);
        let summary = text.find("A routine inspection").unwrap();
        let findings = text.find("# East Wing").unwrap();
        let follow_up = text.find("Book a follow-up").unwrap();
        assert!(summary < findings && findings < follow_up, "{text}");
    }

    #[test]
    fn inline_markdown_in_generated_prose_becomes_real_formatting() {
        let mut value = sample();
        value["summary"] = json!("The **north wall** shows _hairline_ cracking.");
        let doc = render(&fixture::template(), &value).unwrap();
        let marks: Vec<Marks> = doc.blocks[0].content.iter().map(|s| s.marks).collect();
        assert!(marks.contains(&Marks::BOLD), "{:?}", doc.blocks[0].content);
        assert!(marks.contains(&Marks::ITALIC));
    }

    #[test]
    fn a_heading_the_model_wrote_inside_a_paragraph_is_demoted() {
        // Only the template may create headings; the words still have to survive.
        let mut value = sample();
        value["summary"] = json!("# Overview\n\nAll in order.");
        let doc = render(&fixture::template(), &value).unwrap();
        assert!(doc
            .blocks
            .iter()
            .all(|b| !matches!(b.kind, BlockKind::Heading { level: 1 }) || b.text() != "Overview"));
        assert!(to_markdown(&doc).contains("Overview"));
        assert!(to_markdown(&doc).contains("All in order."));
    }

    #[test]
    fn a_multi_paragraph_answer_becomes_multiple_blocks() {
        let mut value = sample();
        value["summary"] = json!("First paragraph.\n\nSecond paragraph.");
        let doc = render(&fixture::template(), &value).unwrap();
        assert_eq!(doc.blocks[0].text(), "First paragraph.");
        assert_eq!(doc.blocks[1].text(), "Second paragraph.");
    }

    #[test]
    fn a_multi_block_list_item_is_flattened_into_one_item() {
        let mut value = sample();
        value["follow_up"]["next_steps"] = json!(["One thing.\n\nAnd another."]);
        let doc = render(&fixture::template(), &value).unwrap();
        let items: Vec<&Block> = doc.blocks.iter().filter(|b| b.kind.is_list_item()).collect();
        let last = items.last().unwrap();
        assert_eq!(last.text(), "One thing. And another.");
    }

    #[test]
    fn blank_generated_text_produces_no_empty_blocks() {
        let mut value = sample();
        value["summary"] = json!("   ");
        value["follow_up"]["next_steps"] = json!(["", "  "]);
        let doc = render(&fixture::template(), &value).unwrap();
        assert!(doc.blocks.iter().all(|b| !b.is_empty()), "{:?}", doc.blocks);
    }

    #[test]
    fn heading_level_is_clamped_at_the_markdown_maximum() {
        // Seven nested sections; the deepest must still be a heading, not `#######`.
        let mut node = TemplateNode::new(
            "L7",
            NodeKind::Section {
                heading_description: "deepest".into(),
                children: vec![TemplateNode::new(
                    "Body",
                    NodeKind::Paragraph { description: "text".into() },
                )],
            },
        );
        for i in (1..=6).rev() {
            node = TemplateNode::new(
                format!("L{i}"),
                NodeKind::Section { heading_description: "nested".into(), children: vec![node] },
            );
        }
        let mut t = Template::new("deep");
        t.nodes = vec![node];

        let mut inner = json!({"heading": "H7", "body": "text"});
        for i in (1..=6).rev() {
            inner = json!({"heading": format!("H{i}"), format!("l{}", i + 1): inner});
        }
        let value = json!({ "l1": inner });

        let doc = render(&t, &value).unwrap();
        let levels: Vec<u8> = doc
            .blocks
            .iter()
            .filter_map(|b| match b.kind {
                BlockKind::Heading { level } => Some(level),
                _ => None,
            })
            .collect();
        assert_eq!(levels, [1, 2, 3, 4, 5, 6, 6], "the seventh clamps rather than overflowing");
    }

    #[test]
    fn a_value_that_does_not_fit_the_template_is_rejected_with_its_path() {
        let mut value = sample();
        value["findings"]["overview"] = json!(7);
        let err = render(&fixture::template(), &value).unwrap_err();
        assert_eq!(err.path, "findings.overview");
    }

    #[test]
    fn an_empty_template_renders_an_editable_document() {
        // Zero blocks would render nothing for the user to click into, so a template
        // with no nodes yet must still open as an editable document.
        let doc = render(&Template::new("empty"), &json!({})).unwrap();
        assert_eq!(doc.blocks.len(), 1);
        assert_eq!(doc.blocks[0].kind, BlockKind::Paragraph);
        assert!(doc.blocks[0].is_empty());
    }
}
