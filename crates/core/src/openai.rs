//! The OpenAI-compatible connector.
//!
//! Request and response types are hand-rolled rather than taken from a client crate.
//! The point of this backend is to reach *arbitrary* servers that claim OpenAI
//! compatibility — Ollama, LM Studio, llama-server, vLLM, Azure, a company gateway —
//! and they differ in exactly the places a client crate makes hardest to reach: the
//! base URL, the auth header, and which `response_format` values are honoured.
//!
//! ## Degrading rather than failing
//!
//! Structured output is where compatibility actually breaks down, so the connector
//! tries three things in order and reports which one worked:
//!
//! 1. `response_format: {"type": "json_schema", "strict": true}` — the shape is
//!    guaranteed, and this is what the whole design wants.
//! 2. `response_format: {"type": "json_object"}` with the schema pasted into the
//!    system prompt — the model is merely *asked* to match the shape.
//! 3. No `response_format` at all, schema still in the prompt.
//!
//! It falls back only on the specific failure that means "this server does not
//! support that": a 4xx naming `response_format` or `json_schema`. A timeout, a bad
//! key or a rate limit must surface as itself rather than being retried into a weaker
//! mode that then produces a subtly worse report.
//!
//! Validation in [`crate::compile::Shape::accepts`] is what makes the weaker modes
//! safe to use at all: whatever the server did, the value is checked against the
//! template before anything is rendered.

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::backend::{extract_json, truncate, JsonRequest, LlmBackend};
// The configuration lives in `settings` rather than here: it is plain serde data
// with no HTTP in it, and settings must still compile when this connector is not.
pub use crate::settings::OpenAiConfig;

/// How the request asked for structured output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// The shape is enforced by the server.
    StrictSchema,
    /// The server guarantees valid JSON but not the shape.
    JsonObject,
    /// Nothing is guaranteed; the schema is only described in the prompt.
    Prompt,
}

impl Mode {
    fn weaker(self) -> Option<Mode> {
        match self {
            Mode::StrictSchema => Some(Mode::JsonObject),
            Mode::JsonObject => Some(Mode::Prompt),
            Mode::Prompt => None,
        }
    }
}

pub struct OpenAiBackend {
    client: reqwest::Client,
    config: OpenAiConfig,
}

impl OpenAiBackend {
    pub fn new(config: OpenAiConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs.max(1)))
            .build()
            .context("building the HTTP client")?;
        Ok(Self { client, config })
    }

    async fn attempt(&self, request: &JsonRequest, mode: Mode) -> Result<Value, Failure> {
        let body = build_body(&self.config, request, mode);

        let mut http = self.client.post(self.config.endpoint()).json(&body);
        if !self.config.api_key.trim().is_empty() {
            http = http.bearer_auth(self.config.api_key.trim());
        }

        let response = http.send().await.map_err(|e| Failure::Fatal(anyhow!(e)))?;
        let status = response.status();
        let text = response.text().await.map_err(|e| Failure::Fatal(anyhow!(e)))?;

        if !status.is_success() {
            let message = error_message(&text);
            if status.is_client_error() && mentions_response_format(&message) {
                return Err(Failure::Unsupported(message));
            }
            return Err(Failure::Fatal(anyhow!(
                "{} returned {status}: {message}",
                self.config.endpoint()
            )));
        }

        let content = extract_content(&text).map_err(Failure::Fatal)?;
        extract_json(&content).map_err(Failure::Fatal)
    }
}

/// Distinguishes "this server cannot do that" from "this request failed".
enum Failure {
    Unsupported(String),
    Fatal(anyhow::Error),
}

#[async_trait]
impl LlmBackend for OpenAiBackend {
    async fn complete_json(&self, request: JsonRequest) -> Result<Value> {
        let mut mode = Mode::StrictSchema;
        loop {
            match self.attempt(&request, mode).await {
                Ok(value) => {
                    if mode != Mode::StrictSchema {
                        tracing::warn!(
                            "openai: server does not support strict schemas; used {mode:?}. \
                             The shape is checked locally instead."
                        );
                    }
                    return Ok(value);
                }
                Err(Failure::Unsupported(message)) => match mode.weaker() {
                    Some(next) => {
                        tracing::info!("openai: {mode:?} rejected ({message}), trying {next:?}");
                        mode = next;
                    }
                    None => bail!("the server rejected every response format: {message}"),
                },
                // Anything that is not "unsupported" is reported as itself: retrying
                // a bad key or a timeout in a weaker mode would turn a clear error
                // into a subtly worse report.
                Err(Failure::Fatal(error)) => return Err(error),
            }
        }
    }

    fn describe(&self) -> String {
        format!("{} at {}", self.config.model, self.config.base_url)
    }
}

fn build_body(config: &OpenAiConfig, request: &JsonRequest, mode: Mode) -> Value {
    let system = match mode {
        // Strict mode carries the schema itself, so repeating it in the prompt would
        // only spend context.
        Mode::StrictSchema => request.system.clone(),
        _ => format!(
            "{}\n\nReturn a single JSON object and nothing else — no prose, no code \
             fences — matching exactly this JSON Schema:\n\n{}",
            request.system,
            serde_json::to_string_pretty(&request.schema).unwrap_or_default()
        ),
    };

    let mut body = json!({
        "model": config.model,
        "temperature": request.temperature,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": request.user},
        ],
    });

    if let Some(max) = request.max_tokens {
        body["max_tokens"] = json!(max);
    }

    match mode {
        Mode::StrictSchema => {
            body["response_format"] = json!({
                "type": "json_schema",
                "json_schema": {
                    "name": request.schema_name,
                    "strict": true,
                    "schema": request.schema,
                },
            });
        }
        Mode::JsonObject => body["response_format"] = json!({"type": "json_object"}),
        Mode::Prompt => {}
    }

    body
}

/// Pull the assistant's message out of a chat completions response.
fn extract_content(body: &str) -> Result<String> {
    let value: Value = serde_json::from_str(body)
        .with_context(|| format!("the server's reply was not JSON: {}", truncate(body, 400)))?;

    value["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| anyhow!("no message content in the reply: {}", truncate(body, 400)))
}

/// The human-readable part of an error body, whatever shape it takes.
fn error_message(body: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(body) {
        for path in [&["error", "message"][..], &["message"][..], &["detail"][..]] {
            let mut current = &value;
            for key in path {
                current = &current[*key];
            }
            if let Some(text) = current.as_str() {
                return text.to_string();
            }
        }
    }
    truncate(body, 400)
}

/// Whether an error is the server saying it does not support the requested format.
fn mentions_response_format(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    ["response_format", "json_schema", "response format", "structured output"]
        .iter()
        .any(|needle| lower.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::Shape;
    use crate::template::fixture;

    fn request() -> JsonRequest {
        let shape = Shape::compile(&fixture::template());
        JsonRequest::new("SYSTEM".into(), "NOTES".into(), shape.to_json_schema(), shape.to_gbnf())
    }

    #[test]
    fn strict_mode_sends_the_schema_in_the_response_format() {
        let body = build_body(&OpenAiConfig::default(), &request(), Mode::StrictSchema);
        assert_eq!(body["response_format"]["type"], "json_schema");
        assert_eq!(body["response_format"]["json_schema"]["strict"], true);
        assert!(
            body["response_format"]["json_schema"]["schema"]["properties"]["summary"].is_object()
        );
        // The schema is already in the request, so repeating it in the prompt would
        // only spend context.
        assert_eq!(body["messages"][0]["content"], "SYSTEM");
    }

    #[test]
    fn the_weaker_modes_move_the_schema_into_the_prompt() {
        for mode in [Mode::JsonObject, Mode::Prompt] {
            let body = build_body(&OpenAiConfig::default(), &request(), mode);
            let system = body["messages"][0]["content"].as_str().unwrap();
            assert!(system.starts_with("SYSTEM"), "{mode:?}");
            assert!(system.contains("\"summary\""), "the schema must reach the model: {mode:?}");
        }
        let body = build_body(&OpenAiConfig::default(), &request(), Mode::JsonObject);
        assert_eq!(body["response_format"]["type"], "json_object");
        let body = build_body(&OpenAiConfig::default(), &request(), Mode::Prompt);
        assert!(body.get("response_format").is_none());
    }

    #[test]
    fn the_notes_always_travel_as_the_user_turn() {
        for mode in [Mode::StrictSchema, Mode::JsonObject, Mode::Prompt] {
            let body = build_body(&OpenAiConfig::default(), &request(), mode);
            assert_eq!(body["messages"][1]["role"], "user");
            assert_eq!(body["messages"][1]["content"], "NOTES");
        }
    }

    #[test]
    fn each_mode_degrades_to_the_next_and_then_stops() {
        assert_eq!(Mode::StrictSchema.weaker(), Some(Mode::JsonObject));
        assert_eq!(Mode::JsonObject.weaker(), Some(Mode::Prompt));
        assert_eq!(Mode::Prompt.weaker(), None);
    }

    #[test]
    fn only_a_response_format_complaint_triggers_a_fallback() {
        // The distinction that matters: retrying a bad key or a rate limit in a
        // weaker mode would turn a clear error into a subtly worse report.
        assert!(mentions_response_format("Invalid parameter: 'response_format'"));
        assert!(mentions_response_format("json_schema is not supported by this model"));
        assert!(mentions_response_format("Structured Output is unavailable"));

        assert!(!mentions_response_format("Incorrect API key provided"));
        assert!(!mentions_response_format("Rate limit reached for gpt-4o-mini"));
        assert!(!mentions_response_format("context length exceeded"));
        // Verified against a running Ollama: a 404 for an unknown model must be
        // reported as itself, not retried into a weaker format.
        assert!(!mentions_response_format("model 'nonexistent' not found"));
    }

    #[test]
    fn error_messages_are_found_in_the_shapes_servers_actually_use() {
        assert_eq!(
            error_message(r#"{"error":{"message":"Incorrect API key","type":"auth"}}"#),
            "Incorrect API key"
        );
        assert_eq!(error_message(r#"{"message":"model not found"}"#), "model not found");
        assert_eq!(error_message(r#"{"detail":"Not Found"}"#), "Not Found");
        // A plain-text or HTML body from a proxy still has to say something useful.
        assert_eq!(error_message("502 Bad Gateway"), "502 Bad Gateway");
    }

    #[test]
    fn the_assistant_message_is_read_out_of_a_normal_reply() {
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"{\"a\":1}"}}]}"#;
        assert_eq!(extract_content(body).unwrap(), r#"{"a":1}"#);
    }

    #[test]
    fn a_reply_that_is_not_a_chat_completion_says_so_with_the_body() {
        // What a misconfigured base URL produces: a 200 from something else entirely.
        let error = extract_content("<html>hello</html>").unwrap_err().to_string();
        assert!(error.contains("not JSON"), "{error}");
        let error = extract_content(r#"{"object":"list","data":[]}"#).unwrap_err().to_string();
        assert!(error.contains("no message content"), "{error}");
    }
}
