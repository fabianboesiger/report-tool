//! The generation backend, behind one trait.
//!
//! Both backends receive the *same* prompt and the *same* shape; they differ only in
//! how the shape is enforced, because the two speak different dialects:
//!
//! - remote: JSON Schema, in OpenAI's `response_format`
//! - local:  GBNF, through `LlamaSampler::grammar`
//!
//! [`JsonRequest`] therefore carries both, and each backend ignores the one it cannot
//! use. Carrying both rather than making the caller choose is what keeps the call
//! site identical: swapping the backend changes a line in settings, not the
//! generation flow.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

/// One constrained-generation request.
#[derive(Debug, Clone)]
pub struct JsonRequest {
    pub system: String,
    pub user: String,
    /// For the remote backend's `response_format`.
    pub schema: Value,
    /// For the local backend's grammar sampler.
    pub grammar: String,
    /// Names the schema in the API request; some servers log or key on it.
    pub schema_name: String,
    pub max_tokens: Option<u32>,
    /// Low by default: the structure comes from the constraint, so sampling
    /// creativity buys nothing here and costs faithfulness to the notes.
    pub temperature: f32,
}

impl JsonRequest {
    pub fn new(system: String, user: String, schema: Value, grammar: String) -> Self {
        Self {
            system,
            user,
            schema,
            grammar,
            schema_name: "report".to_string(),
            max_tokens: None,
            temperature: 0.3,
        }
    }
}

/// A source of structured report content.
#[async_trait]
pub trait LlmBackend: Send + Sync {
    /// Generate a value matching `request.schema`.
    ///
    /// Returning the parsed JSON rather than a rendered document keeps the backend
    /// ignorant of templates: validation and rendering happen once, in
    /// [`crate::render`], whichever backend produced the value.
    async fn complete_json(&self, request: JsonRequest) -> Result<Value>;

    /// How this backend should be described in the UI and the logs.
    fn describe(&self) -> String;
}

/// A backend that returns a canned value shaped by the request's own schema.
///
/// Not a toy: `--no-default-features` builds have no inference engine, and this is
/// what lets the whole generation flow — the button, the progress state, validation,
/// rendering, the editor receiving a document — be exercised in a build that
/// compiles in seconds. It fills every field with a placeholder naming that field, so
/// a structural mistake in the template is visible without a model.
pub struct StubBackend;

#[async_trait]
impl LlmBackend for StubBackend {
    async fn complete_json(&self, request: JsonRequest) -> Result<Value> {
        Ok(sample_for(&request.schema, "field"))
    }

    fn describe(&self) -> String {
        "stub (no model)".to_string()
    }
}

/// Build a placeholder value satisfying `schema`.
fn sample_for(schema: &Value, name: &str) -> Value {
    let types = schema.get("type");
    let is = |wanted: &str| match types {
        Some(Value::String(t)) => t == wanted,
        Some(Value::Array(list)) => list.iter().any(|t| t == wanted),
        _ => false,
    };

    if is("object") {
        let mut out = serde_json::Map::new();
        if let Some(props) = schema.get("properties").and_then(Value::as_object) {
            for (key, child) in props {
                out.insert(key.clone(), sample_for(child, key));
            }
        }
        return Value::Object(out);
    }
    if is("array") {
        let item = schema.get("items").map(|i| sample_for(i, name)).unwrap_or(Value::Null);
        return Value::Array(vec![item]);
    }
    // No markdown metacharacters: square brackets are link syntax and would come
    // back out of the exporter as `\[summary\]`.
    Value::String(format!("TODO: {name}"))
}

/// Parse a model's output into JSON, tolerating the wrappers models add.
///
/// Shared by both backends. The local one is grammar-constrained and so should never
/// need the recovery below — but a bug in the emitted grammar would otherwise surface
/// as an unreadable failure rather than a clear message, and being tolerant costs
/// nothing. The remote one needs it constantly: even when asked for bare JSON, models
/// wrap the answer in fences or preface it with a sentence, and insisting on a clean
/// parse would reject a response whose content is perfectly good.
pub fn extract_json(content: &str) -> Result<Value> {
    let text = content.trim();
    if let Ok(value) = serde_json::from_str::<Value>(text) {
        return Ok(value);
    }

    let unfenced = strip_fence(text);
    if let Ok(value) = serde_json::from_str::<Value>(unfenced.trim()) {
        return Ok(value);
    }

    // Last resort: the outermost braces. Handles a preamble, a trailing remark, or
    // both at once.
    if let (Some(start), Some(end)) = (unfenced.find('{'), unfenced.rfind('}')) {
        if end > start {
            if let Ok(value) = serde_json::from_str::<Value>(&unfenced[start..=end]) {
                return Ok(value);
            }
        }
    }

    anyhow::bail!("the model did not return usable JSON: {}", truncate(content, 400))
}

/// Remove a leading fence and its closing counterpart.
fn strip_fence(text: &str) -> &str {
    let Some(rest) = text.strip_prefix("```") else { return text };
    // Drop the info string on the opening fence line.
    let rest = match rest.find('\n') {
        Some(i) => &rest[i + 1..],
        None => return text,
    };
    match rest.rfind("```") {
        Some(i) => &rest[..i],
        None => rest,
    }
}

/// Shorten text for an error message.
///
/// Counts characters, not bytes: model output is full of multi-byte characters and a
/// byte-based cut would panic on a boundary — turning a diagnostic into a crash.
pub(crate) fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let cut: String = text.chars().take(limit).collect();
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::Shape;
    use crate::template::fixture;
    use serde_json::json;

    #[test]
    fn bare_json_parses() {
        assert_eq!(extract_json(r#"{"a":1}"#).unwrap(), json!({"a": 1}));
        assert_eq!(extract_json("  \n{\"a\":1}\n ").unwrap(), json!({"a": 1}));
    }

    #[test]
    fn fenced_json_parses() {
        // Models wrap output in fences even when told not to; refusing this would
        // fail a response whose content is perfectly good.
        assert_eq!(extract_json("```json\n{\"a\":1}\n```").unwrap(), json!({"a": 1}));
        assert_eq!(extract_json("```\n{\"a\":1}\n```").unwrap(), json!({"a": 1}));
    }

    #[test]
    fn json_with_a_preamble_parses() {
        let content = "Sure! Here is the report:\n\n{\"a\": 1, \"b\": {\"c\": 2}}\n\nLet me know.";
        assert_eq!(extract_json(content).unwrap(), json!({"a": 1, "b": {"c": 2}}));
    }

    #[test]
    fn braces_inside_strings_do_not_confuse_the_recovery() {
        let content = "text {\"a\": \"a } brace\"} tail";
        assert_eq!(extract_json(content).unwrap(), json!({"a": "a } brace"}));
    }

    #[test]
    fn unusable_output_fails_with_the_output_in_the_message() {
        let error = extract_json("I am not able to help with that.").unwrap_err().to_string();
        assert!(error.contains("did not return usable JSON"), "{error}");
        assert!(error.contains("not able to help"), "the output must be visible: {error}");
    }

    #[test]
    fn a_very_long_failure_is_truncated_rather_than_flooding_the_log() {
        let error = extract_json(&"x".repeat(5000)).unwrap_err().to_string();
        assert!(error.chars().count() < 500, "{}", error.chars().count());
        assert!(error.ends_with('…'));
    }

    #[test]
    fn truncation_counts_characters_not_bytes() {
        // A byte-based cut would panic on a multi-byte boundary.
        let text = "ü".repeat(500);
        assert_eq!(truncate(&text, 10).chars().count(), 11);
    }

    #[tokio::test]
    async fn the_stub_satisfies_the_shape_it_was_asked_for() {
        // The point of the stub: exercising the flow must not mean bypassing the
        // validation the real backends are held to.
        let template = fixture::template();
        let shape = Shape::compile(&template);
        let request =
            JsonRequest::new(String::new(), String::new(), shape.to_json_schema(), shape.to_gbnf());

        let value = StubBackend.complete_json(request).await.unwrap();
        assert_eq!(shape.accepts(&value), Ok(()));
        // And it renders, so the editor receives a real document.
        assert!(crate::render::render(&template, &value).is_ok());
    }

    #[tokio::test]
    async fn the_stub_names_each_field_so_a_structural_mistake_is_visible() {
        let shape = Shape::compile(&fixture::template());
        let request =
            JsonRequest::new(String::new(), String::new(), shape.to_json_schema(), String::new());
        let value = StubBackend.complete_json(request).await.unwrap();
        assert_eq!(value["summary"], "TODO: summary");
        assert_eq!(value["findings"]["heading"], "TODO: heading");
        assert_eq!(value["findings"]["defects"][0]["actions"][0], "TODO: actions");
    }
}
