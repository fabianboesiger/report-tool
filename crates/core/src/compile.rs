//! Compiling a template into the constraints that make generation reliable.
//!
//! A template is first lowered into a [`Shape`] — an intermediate form describing
//! the JSON the model must produce — and everything downstream is derived from that
//! one value:
//!
//! ```text
//!                      ┌─ to_json_schema() ──▶ OpenAI response_format
//!   Template ─▶ Shape ─┼─ to_gbnf() ─────────▶ LlamaSampler::grammar
//!                      ├─ accepts() ─────────▶ validation before rendering
//!                      └─ (render.rs) ───────▶ markdown
//! ```
//!
//! The shared IR is deliberate. The two backends speak different dialects — OpenAI
//! takes JSON Schema, `llama-cpp-2` exposes only GBNF — and the entire design rests
//! on them describing the *same* shape. Deriving both from one traversal makes
//! agreement structural rather than something a test has to keep catching: a bug can
//! live in one mapping, but the two cannot disagree about what the structure is.
//!
//! ## Why the shape is enforced rather than requested
//!
//! Asking a model to "produce these sections" is a request it may decline. Encoding
//! the sections as a schema or a grammar makes malformed output *unrepresentable*:
//! an omitted optional is a `null` the model has to write, a repeat is an array
//! whose length the notes decide, and headings never appear in the model's output at
//! all — [`crate::render`] supplies those from the template.

use serde_json::{json, Map, Value};

use crate::template::{NodeKind, Template, TemplateNode};

/// The key holding a section's generated heading text.
///
/// Reserved: a section's children can never use it, since the two would collide in
/// the same JSON object and one would silently overwrite the other.
pub const HEADING_KEY: &str = "heading";

/// The JSON shape a template requires, and the single source of truth for the schema
/// emitter, the grammar emitter, the validator and the renderer.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// A run of prose.
    Text { description: String },
    /// A list of short strings.
    List { description: String, ordered: bool, min: Option<u32>, max: Option<u32> },
    /// A heading plus nested fields. The only shape that deepens the heading level.
    Section { description: String, fields: Vec<Field> },
    /// Nested fields with no heading of their own: the document root, and the body
    /// of one repetition.
    Group { fields: Vec<Field> },
    /// Content the model may omit, by writing `null`.
    Optional { description: String, inner: Box<Shape> },
    /// Content repeated once per occurrence in the notes.
    Repeat {
        description: String,
        item_label: String,
        min: Option<u32>,
        max: Option<u32>,
        item: Box<Shape>,
    },
}

/// One named field of a [`Shape::Section`] or [`Shape::Group`].
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub key: String,
    /// The template label, used to describe the field in the prompt's field guide.
    pub label: String,
    pub shape: Shape,
}

impl Shape {
    /// Lower a template into its shape.
    pub fn compile(template: &Template) -> Shape {
        Shape::Group { fields: fields_of(&template.nodes, &[]) }
    }

    /// The description attached to this shape, if any.
    pub fn description(&self) -> &str {
        match self {
            Shape::Text { description }
            | Shape::List { description, .. }
            | Shape::Section { description, .. }
            | Shape::Optional { description, .. }
            | Shape::Repeat { description, .. } => description,
            Shape::Group { .. } => "",
        }
    }
}

/// Build the field list for a set of sibling nodes.
///
/// `reserved` holds keys already taken in this object (just `heading` inside a
/// section). Collisions are resolved here rather than in the template so that the
/// user's chosen keys are never silently rewritten on disk.
fn fields_of(nodes: &[TemplateNode], reserved: &[&str]) -> Vec<Field> {
    let mut used: Vec<String> = reserved.iter().map(|s| s.to_string()).collect();
    let mut fields = Vec::with_capacity(nodes.len());

    for node in nodes {
        let key = unique(&node.key, &mut used);
        let shape = match &node.kind {
            NodeKind::Paragraph { description } => Shape::Text { description: description.clone() },
            NodeKind::List { description, ordered, min_items, max_items } => Shape::List {
                description: description.clone(),
                ordered: *ordered,
                min: *min_items,
                max: *max_items,
            },
            NodeKind::Section { heading_description, children } => Shape::Section {
                description: heading_description.clone(),
                fields: fields_of(children, &[HEADING_KEY]),
            },
            NodeKind::Optional { description, children } => Shape::Optional {
                description: description.clone(),
                inner: Box::new(Shape::Group { fields: fields_of(children, &[]) }),
            },
            NodeKind::Repeat { description, item_label, min, max, children } => Shape::Repeat {
                description: description.clone(),
                item_label: item_label.clone(),
                min: *min,
                max: *max,
                item: Box::new(Shape::Group { fields: fields_of(children, &[]) }),
            },
        };
        fields.push(Field { key, label: node.label.clone(), shape });
    }
    fields
}

fn unique(key: &str, used: &mut Vec<String>) -> String {
    let base = if key.is_empty() { "field" } else { key };
    let mut candidate = base.to_string();
    let mut n = 2;
    while used.contains(&candidate) {
        candidate = format!("{base}_{n}");
        n += 1;
    }
    used.push(candidate.clone());
    candidate
}

// ---------------------------------------------------------------------------
// JSON Schema
// ---------------------------------------------------------------------------

impl Shape {
    /// Emit a JSON Schema for OpenAI's `response_format` in **strict** mode.
    ///
    /// Strict mode imposes two rules that shape the output more than they look:
    ///
    /// 1. Every property must appear in `required`, and `additionalProperties` must
    ///    be `false`. Optionality is therefore expressed as a nullable type — which
    ///    is why [`Shape::Optional`] exists as a distinct variant rather than as an
    ///    absent key.
    /// 2. `minItems` / `maxItems` are **not supported** and cause the request to be
    ///    rejected outright. List and repeat bounds are folded into the field's
    ///    `description` instead, where they are a strong hint rather than a
    ///    guarantee. The local backend has no such limitation: [`Shape::to_gbnf`]
    ///    enforces the same bounds exactly.
    pub fn to_json_schema(&self) -> Value {
        match self {
            Shape::Text { description } => with_description(json!({"type": "string"}), description),

            Shape::List { description, min, max, .. } => with_description(
                json!({"type": "array", "items": {"type": "string"}}),
                &join_hint(description, &count_hint("items", *min, *max)),
            ),

            Shape::Section { description, fields } => {
                let mut props = Map::new();
                props.insert(
                    HEADING_KEY.to_string(),
                    with_description(json!({"type": "string"}), description),
                );
                object_schema(props, fields, false)
            }

            Shape::Group { fields } => object_schema(Map::new(), fields, false),

            Shape::Optional { description, inner } => {
                let mut schema = inner.to_json_schema();
                // Nullable, not absent — see the doc comment above.
                schema["type"] = json!(["object", "null"]);
                with_description(
                    schema,
                    &join_hint(
                        description,
                        "Write null for this field if it does not apply to these notes.",
                    ),
                )
            }

            Shape::Repeat { description, item_label, min, max, item } => {
                let unit = if item_label.is_empty() {
                    "entries".into()
                } else {
                    format!("{item_label} entries")
                };
                with_description(
                    json!({"type": "array", "items": item.to_json_schema()}),
                    &join_hint(description, &count_hint(&unit, *min, *max)),
                )
            }
        }
    }
}

fn object_schema(mut props: Map<String, Value>, fields: &[Field], nullable: bool) -> Value {
    for field in fields {
        props.insert(field.key.clone(), field.shape.to_json_schema());
    }
    // Strict mode requires every key listed, hence keys() rather than a filter.
    let required: Vec<&String> = props.keys().collect();
    json!({
        "type": if nullable { json!(["object", "null"]) } else { json!("object") },
        "properties": props,
        "required": required,
        "additionalProperties": false,
    })
}

fn with_description(mut schema: Value, description: &str) -> Value {
    if !description.trim().is_empty() {
        schema["description"] = json!(description.trim());
    }
    schema
}

/// Render list/repeat bounds as prose, since strict mode forbids the keywords.
fn count_hint(unit: &str, min: Option<u32>, max: Option<u32>) -> String {
    match (min, max) {
        (Some(a), Some(b)) if a == b => format!("Provide exactly {a} {unit}."),
        (Some(a), Some(b)) => format!("Provide between {a} and {b} {unit}."),
        (Some(a), None) => format!("Provide at least {a} {unit}."),
        (None, Some(b)) => format!("Provide at most {b} {unit}."),
        (None, None) => String::new(),
    }
}

fn join_hint(description: &str, hint: &str) -> String {
    match (description.trim(), hint.trim()) {
        ("", h) => h.to_string(),
        (d, "") => d.to_string(),
        (d, h) => format!("{d} {h}"),
    }
}

// ---------------------------------------------------------------------------
// GBNF
// ---------------------------------------------------------------------------

impl Shape {
    /// Emit a GBNF grammar accepting exactly the JSON this shape describes.
    ///
    /// Rules are named `r0`, `r1`, … rather than after the template's keys: a rule
    /// name is an identifier in GBNF, and deriving one from user text would mean
    /// escaping it correctly forever. Sequential names sidestep that entirely, and
    /// the grammar is machine-read anyway.
    ///
    /// Unlike the schema, this enforces list and repeat *counts* exactly — the local
    /// backend has no strict-mode restriction to work around.
    pub fn to_gbnf(&self) -> String {
        let mut g = Grammar::default();
        let root = g.expr(self);
        let mut out = format!("root ::= ws {root} ws\n");
        for (name, body) in &g.rules {
            out.push_str(&format!("{name} ::= {body}\n"));
        }
        // The JSON primitives, taken from llama.cpp's own json.gbnf. `ws` is bounded
        // rather than `*` so a model cannot satisfy the grammar forever by emitting
        // whitespace.
        out.push_str(concat!(
            "string ::= \"\\\"\" char* \"\\\"\"\n",
            "char ::= [^\"\\\\\\x7F\\x00-\\x1F] | \"\\\\\" ([\"\\\\bfnrt/] | \"u\" [0-9a-fA-F]{4})\n",
            "ws ::= [ \\t\\n]{0,20}\n",
        ));
        out
    }
}

#[derive(Default)]
struct Grammar {
    rules: Vec<(String, String)>,
    next: usize,
}

impl Grammar {
    /// Define a rule with a fresh name and return a reference to it.
    fn define(&mut self, body: String) -> String {
        let name = format!("r{}", self.next);
        self.next += 1;
        self.rules.push((name.clone(), body));
        name
    }

    /// A reference to a rule matching `shape`, or an inline primitive.
    fn expr(&mut self, shape: &Shape) -> String {
        match shape {
            Shape::Text { .. } => "string".to_string(),

            Shape::List { min, max, .. } => {
                let body = array_body("string", *min, *max);
                self.define(body)
            }

            Shape::Section { fields, .. } => {
                let mut entries = vec![(HEADING_KEY.to_string(), "string".to_string())];
                for field in fields {
                    let expr = self.expr(&field.shape);
                    entries.push((field.key.clone(), expr));
                }
                let body = object_body(&entries);
                self.define(body)
            }

            Shape::Group { fields } => {
                let mut entries = Vec::with_capacity(fields.len());
                for field in fields {
                    let expr = self.expr(&field.shape);
                    entries.push((field.key.clone(), expr));
                }
                let body = object_body(&entries);
                self.define(body)
            }

            Shape::Optional { inner, .. } => {
                let inner = self.expr(inner);
                self.define(format!("{inner} | \"null\""))
            }

            Shape::Repeat { min, max, item, .. } => {
                let item = self.expr(item);
                let body = array_body(&item, *min, *max);
                self.define(body)
            }
        }
    }
}

/// `{"k1": <e1>, "k2": <e2>}` with the keys in template order.
///
/// Fixing the order is free here and makes the grammar simpler than an
/// any-permutation rule would be; the renderer walks the template, so it never
/// depended on the order in the first place.
fn object_body(entries: &[(String, String)]) -> String {
    if entries.is_empty() {
        // An empty object is still valid JSON and still a valid section — a section
        // whose children the user has not added yet.
        return "\"{\" ws \"}\"".to_string();
    }
    let inner: Vec<String> =
        entries.iter().map(|(key, expr)| format!("\"\\\"{key}\\\"\" ws \":\" ws {expr}")).collect();
    format!("\"{{\" ws {} ws \"}}\"", inner.join(" ws \",\" ws "))
}

/// `[<item>, <item>, …]` honouring the count bounds exactly.
fn array_body(item: &str, min: Option<u32>, max: Option<u32>) -> String {
    let min = min.unwrap_or(0);
    if max == Some(0) {
        return "\"[\" ws \"]\"".to_string();
    }
    // Repetitions *after* the first, so the bounds shift by one.
    let rest = match (min.saturating_sub(1), max.map(|m| m.saturating_sub(1))) {
        (0, None) => "*".to_string(),
        (a, None) => format!("{{{a},}}"),
        (a, Some(b)) => format!("{{{a},{b}}}"),
    };
    let sequence = format!("{item} ( ws \",\" ws {item} ){rest}");
    if min == 0 {
        // Zero items must remain possible, so the whole sequence is optional.
        format!("\"[\" ws ( {sequence} ws )? \"]\"")
    } else {
        format!("\"[\" ws {sequence} ws \"]\"")
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Why a generated value did not fit its template.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeError {
    /// Dotted path to the offending value, e.g. `findings.defects[0].actions`.
    pub path: String,
    pub message: String,
}

impl std::fmt::Display for ShapeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for ShapeError {}

impl Shape {
    /// Check a generated value against this shape.
    ///
    /// Both constraint mechanisms should make this unnecessary, and it is still
    /// worth running: a remote server may not honour strict mode, a grammar may be
    /// cut short by the context limit, and a half-rendered report with a clear error
    /// behind it is far easier to act on than one silently missing a section.
    pub fn accepts(&self, value: &Value) -> Result<(), ShapeError> {
        self.check(value, "")
    }

    fn check(&self, value: &Value, path: &str) -> Result<(), ShapeError> {
        let err = |message: String| Err(ShapeError { path: path.to_string(), message });
        match self {
            Shape::Text { .. } => match value {
                Value::String(_) => Ok(()),
                other => err(format!("expected a string, found {}", kind_of(other))),
            },

            Shape::List { min, max, .. } => {
                let Some(items) = value.as_array() else {
                    return err(format!("expected an array, found {}", kind_of(value)));
                };
                check_count(items.len(), *min, *max, path)?;
                for (i, item) in items.iter().enumerate() {
                    if !item.is_string() {
                        return Err(ShapeError {
                            path: format!("{path}[{i}]"),
                            message: format!("expected a string, found {}", kind_of(item)),
                        });
                    }
                }
                Ok(())
            }

            Shape::Section { fields, .. } => {
                let Some(obj) = value.as_object() else {
                    return err(format!("expected an object, found {}", kind_of(value)));
                };
                match obj.get(HEADING_KEY) {
                    Some(Value::String(_)) => {}
                    Some(other) => {
                        return Err(ShapeError {
                            path: join(path, HEADING_KEY),
                            message: format!("expected a string, found {}", kind_of(other)),
                        })
                    }
                    None => {
                        return Err(ShapeError {
                            path: join(path, HEADING_KEY),
                            message: "missing".into(),
                        })
                    }
                }
                check_fields(fields, obj, path)
            }

            Shape::Group { fields } => {
                let Some(obj) = value.as_object() else {
                    return err(format!("expected an object, found {}", kind_of(value)));
                };
                check_fields(fields, obj, path)
            }

            Shape::Optional { inner, .. } => match value {
                Value::Null => Ok(()),
                other => inner.check(other, path),
            },

            Shape::Repeat { min, max, item, .. } => {
                let Some(items) = value.as_array() else {
                    return err(format!("expected an array, found {}", kind_of(value)));
                };
                check_count(items.len(), *min, *max, path)?;
                for (i, entry) in items.iter().enumerate() {
                    item.check(entry, &format!("{path}[{i}]"))?;
                }
                Ok(())
            }
        }
    }
}

fn check_fields(fields: &[Field], obj: &Map<String, Value>, path: &str) -> Result<(), ShapeError> {
    for field in fields {
        let child = join(path, &field.key);
        match obj.get(&field.key) {
            Some(value) => field.shape.check(value, &child)?,
            // An absent optional is tolerated as well as an explicit null: some
            // servers drop null keys, and refusing the report over that would be
            // pedantry at the user's expense.
            None if matches!(field.shape, Shape::Optional { .. }) => {}
            None => return Err(ShapeError { path: child, message: "missing".into() }),
        }
    }
    Ok(())
}

fn check_count(
    len: usize,
    min: Option<u32>,
    max: Option<u32>,
    path: &str,
) -> Result<(), ShapeError> {
    if let Some(min) = min {
        if len < min as usize {
            return Err(ShapeError {
                path: path.to_string(),
                message: format!("expected at least {min} entries, found {len}"),
            });
        }
    }
    if let Some(max) = max {
        if len > max as usize {
            return Err(ShapeError {
                path: path.to_string(),
                message: format!("expected at most {max} entries, found {len}"),
            });
        }
    }
    Ok(())
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_string()
    } else {
        format!("{path}.{key}")
    }
}

fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::fixture;

    /// A value matching the fixture template, used across the schema, grammar and
    /// renderer tests so all three are checked against the same structure.
    pub(crate) fn sample() -> Value {
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
                        "actions": ["Monitor for three months", "Re-point the affected joints"]
                    }
                ]
            },
            "follow_up": {
                "next_steps": ["Book a follow-up for March"]
            }
        })
    }

    #[test]
    fn the_sample_fits_the_fixture_shape() {
        let shape = Shape::compile(&fixture::template());
        assert_eq!(shape.accepts(&sample()), Ok(()));
    }

    #[test]
    fn an_omitted_optional_is_accepted_as_null() {
        let shape = Shape::compile(&fixture::template());
        let mut value = sample();
        value["follow_up"] = Value::Null;
        assert_eq!(shape.accepts(&value), Ok(()));
    }

    #[test]
    fn a_missing_required_field_is_reported_with_its_path() {
        let shape = Shape::compile(&fixture::template());
        let mut value = sample();
        value["findings"].as_object_mut().unwrap().remove("overview");
        let err = shape.accepts(&value).unwrap_err();
        assert_eq!(err.path, "findings.overview");
        assert_eq!(err.message, "missing");
    }

    #[test]
    fn a_missing_section_heading_is_reported() {
        let shape = Shape::compile(&fixture::template());
        let mut value = sample();
        value["findings"].as_object_mut().unwrap().remove("heading");
        assert_eq!(shape.accepts(&value).unwrap_err().path, "findings.heading");
    }

    #[test]
    fn repeat_bounds_are_enforced_with_an_indexed_path() {
        let shape = Shape::compile(&fixture::template());
        let mut value = sample();
        // The fixture requires at least one defect.
        value["findings"]["defects"] = json!([]);
        assert!(shape.accepts(&value).unwrap_err().message.contains("at least 1"));

        // And at most five actions per defect.
        let mut value = sample();
        value["findings"]["defects"][0]["actions"] = json!(["a", "b", "c", "d", "e", "f"]);
        let err = shape.accepts(&value).unwrap_err();
        assert_eq!(err.path, "findings.defects[0].actions");
        assert!(err.message.contains("at most 5"));
    }

    #[test]
    fn a_wrongly_typed_value_is_reported_rather_than_coerced() {
        let shape = Shape::compile(&fixture::template());
        let mut value = sample();
        value["summary"] = json!(42);
        let err = shape.accepts(&value).unwrap_err();
        assert_eq!(err.path, "summary");
        assert!(err.message.contains("expected a string"));
    }

    #[test]
    fn a_section_child_cannot_collide_with_the_reserved_heading_key() {
        // Without the reservation the two would share a key in the same object and
        // one would silently overwrite the other, losing a whole field.
        use crate::template::{NodeKind, TemplateNode};
        let mut t = crate::template::Template::new("t");
        t.nodes = vec![TemplateNode::new(
            "Area",
            NodeKind::Section {
                heading_description: "the area".into(),
                children: vec![TemplateNode::new(
                    "Heading",
                    NodeKind::Paragraph {
                        description: "a paragraph the user happened to call Heading".into(),
                    },
                )],
            },
        )];

        let Shape::Group { fields } = Shape::compile(&t) else { panic!("root is a group") };
        let Shape::Section { fields: inner, .. } = &fields[0].shape else { panic!("section") };
        assert_eq!(inner[0].key, "heading_2");
    }

    // ----- JSON Schema -----

    #[test]
    fn the_schema_obeys_strict_mode_everywhere() {
        let schema = Shape::compile(&fixture::template()).to_json_schema();

        // Walk every object in the schema and check the two strict-mode rules.
        fn walk(node: &Value, path: &str) {
            if node.get("type").and_then(Value::as_str) == Some("object")
                || node
                    .get("type")
                    .and_then(Value::as_array)
                    .is_some_and(|t| t.contains(&json!("object")))
            {
                assert_eq!(
                    node.get("additionalProperties"),
                    Some(&json!(false)),
                    "additionalProperties must be false at {path}"
                );
                let props = node["properties"].as_object().unwrap();
                let required: Vec<&str> = node["required"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap())
                    .collect();
                for key in props.keys() {
                    assert!(
                        required.contains(&key.as_str()),
                        "{key} missing from required at {path}"
                    );
                }
                assert_eq!(required.len(), props.len(), "extra required entries at {path}");
                for (key, child) in props {
                    walk(child, &format!("{path}.{key}"));
                }
            }
            if let Some(items) = node.get("items") {
                walk(items, &format!("{path}[]"));
            }
        }
        walk(&schema, "");
    }

    #[test]
    fn strict_mode_forbidden_keywords_are_never_emitted() {
        // `minItems`/`maxItems` cause OpenAI to reject the request outright, so the
        // bounds must reach the model as prose instead.
        let text = Shape::compile(&fixture::template()).to_json_schema().to_string();
        assert!(!text.contains("minItems"), "{text}");
        assert!(!text.contains("maxItems"), "{text}");
        assert!(text.contains("at least 1"), "bounds must survive as a description hint");
        assert!(text.contains("between 1 and 5"));
    }

    #[test]
    fn an_optional_is_nullable_rather_than_absent() {
        let schema = Shape::compile(&fixture::template()).to_json_schema();
        let follow_up = &schema["properties"]["follow_up"];
        assert_eq!(follow_up["type"], json!(["object", "null"]));
        // Still listed as required: strict mode demands it, and nullability is what
        // carries the optionality.
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("follow_up")));
    }

    // ----- GBNF -----
    //
    // These are the tests the whole design leans on. The grammar text is never read
    // back anywhere else in the build — llama.cpp parses it at runtime, with a model
    // loaded — so without them the emitter's escaping and repetition arithmetic
    // would only be checked by manual testing, late and expensively.

    fn grammar_of(shape: &Shape) -> crate::gbnf_match::Grammar {
        crate::gbnf_match::Grammar::parse(&shape.to_gbnf())
    }

    #[test]
    fn the_grammar_accepts_exactly_what_the_schema_describes() {
        let shape = Shape::compile(&fixture::template());
        let json = serde_json::to_string(&sample()).unwrap();
        assert!(
            grammar_of(&shape).accepts(&json),
            "the emitted grammar rejected a value its own schema accepts:\n{}\n{json}",
            shape.to_gbnf()
        );
    }

    #[test]
    fn the_grammar_tolerates_the_whitespace_a_model_actually_emits() {
        let shape = Shape::compile(&fixture::template());
        let pretty = serde_json::to_string_pretty(&sample()).unwrap();
        assert!(grammar_of(&shape).accepts(&pretty), "pretty-printed JSON must parse too");
    }

    #[test]
    fn the_grammar_admits_an_omitted_optional() {
        let shape = Shape::compile(&fixture::template());
        let mut value = sample();
        value["follow_up"] = Value::Null;
        let json = serde_json::to_string(&value).unwrap();
        assert!(grammar_of(&shape).accepts(&json), "null must satisfy an optional");
    }

    #[test]
    fn the_grammar_enforces_repeat_bounds_the_schema_cannot() {
        // This is the local backend's advantage: strict mode has to express these
        // as prose, but a grammar makes an out-of-range count unrepresentable.
        let shape = Shape::compile(&fixture::template());
        let g = grammar_of(&shape);

        let mut value = sample();
        value["findings"]["defects"] = json!([]);
        assert!(
            !g.accepts(&serde_json::to_string(&value).unwrap()),
            "an empty array must be rejected where the template requires at least one"
        );

        let mut value = sample();
        value["findings"]["defects"][0]["actions"] = json!(["a", "b", "c", "d", "e", "f"]);
        assert!(
            !g.accepts(&serde_json::to_string(&value).unwrap()),
            "six actions must be rejected where the template allows at most five"
        );
    }

    #[test]
    fn the_grammar_rejects_a_missing_or_misspelled_key() {
        let shape = Shape::compile(&fixture::template());
        let g = grammar_of(&shape);

        let mut value = sample();
        value.as_object_mut().unwrap().remove("summary");
        assert!(!g.accepts(&serde_json::to_string(&value).unwrap()));

        let mut value = sample();
        let summary = value["summary"].clone();
        value.as_object_mut().unwrap().remove("summary");
        value["Summary"] = summary;
        assert!(!g.accepts(&serde_json::to_string(&value).unwrap()));
    }

    #[test]
    fn the_grammar_rejects_a_string_where_a_structure_belongs() {
        let shape = Shape::compile(&fixture::template());
        let mut value = sample();
        value["findings"] = json!("just some prose");
        assert!(!grammar_of(&shape).accepts(&serde_json::to_string(&value).unwrap()));
    }

    #[test]
    fn generated_strings_may_contain_quotes_and_backslashes() {
        // Prose routinely contains quotation marks; if the string rule got these
        // wrong, generation would stall mid-sentence on a token it cannot emit.
        let shape = Shape::compile(&fixture::template());
        let mut value = sample();
        value["summary"] = json!(r#"The tenant said "the wall is damp", citing C:\notes."#);
        let json = serde_json::to_string(&value).unwrap();
        assert!(grammar_of(&shape).accepts(&json), "{json}");
    }

    #[test]
    fn unicode_prose_is_accepted() {
        let shape = Shape::compile(&fixture::template());
        let mut value = sample();
        value["summary"] = json!("Gebäudehülle in gutem Zustand — Fassade geprüft.");
        assert!(grammar_of(&shape).accepts(&serde_json::to_string(&value).unwrap()));
    }

    #[test]
    fn an_empty_template_still_produces_a_usable_grammar() {
        // The state a template is in the moment the user creates it.
        let shape = Shape::compile(&crate::template::Template::new("empty"));
        assert!(grammar_of(&shape).accepts("{}"));
    }

    #[test]
    fn an_unbounded_list_accepts_any_count() {
        use crate::template::{NodeKind, Template, TemplateNode};
        let mut t = Template::new("t");
        t.nodes = vec![TemplateNode::new(
            "Items",
            NodeKind::List {
                description: "anything".into(),
                ordered: false,
                min_items: None,
                max_items: None,
            },
        )];
        let shape = Shape::compile(&t);
        let g = grammar_of(&shape);
        assert!(g.accepts(r#"{"items":[]}"#));
        assert!(g.accepts(r#"{"items":["a"]}"#));
        assert!(g.accepts(r#"{"items":["a","b","c","d"]}"#));
    }

    #[test]
    fn descriptions_reach_the_schema() {
        let schema = Shape::compile(&fixture::template()).to_json_schema();
        assert_eq!(
            schema["properties"]["summary"]["description"],
            json!("Two or three sentences summarising the visit.")
        );
        assert_eq!(
            schema["properties"]["findings"]["properties"]["heading"]["description"],
            json!("A heading naming the inspected area.")
        );
    }
}
