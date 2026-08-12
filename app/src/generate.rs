//! Turning a template and a page of notes into a report.
//!
//! The whole flow in one function, deliberately: every backend takes the same prompt
//! and the same shape, and the value that comes back is validated and rendered by the
//! same code regardless of where it came from. Swapping the backend is a line in
//! settings, not a branch here.

use anyhow::Result;
use report_core::backend::JsonRequest;
use report_core::compile::Shape;
use report_core::settings::Settings;
use report_core::{prompt, render, Template};
use report_doc::RichDoc;

/// What the button is doing.
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Idle,
    Running,
    /// The whole error chain, so a failure names its cause rather than its symptom.
    Failed(String),
    Done,
}

impl Status {
    pub fn is_running(&self) -> bool {
        matches!(self, Status::Running)
    }

    pub fn message(&self) -> Option<&str> {
        match self {
            Status::Failed(error) => Some(error),
            _ => None,
        }
    }
}

/// Generate a report.
pub async fn generate(
    template: Template,
    notes_markdown: String,
    settings: Settings,
) -> Result<RichDoc> {
    let shape = Shape::compile(&template);

    // Both constraints travel together; each backend uses the one it can enforce.
    let request = JsonRequest::new(
        // The report is written in the app's language, named outright in the prompt rather
        // than left for the model to infer from the notes.
        prompt::system(&template, settings.locale()),
        prompt::user(&notes_markdown),
        shape.to_json_schema(),
        shape.to_gbnf(),
    );

    let backend = settings.backend()?;
    tracing::info!("generate: using {}", backend.describe());

    let value = backend.complete_json(request).await?;
    tracing::debug!("generate: received {}", serde_json::to_string(&value).unwrap_or_default());

    // Validated against the template before anything is rendered: a server that
    // ignored the schema, or a generation cut short by a context limit, produces a
    // clear message naming the field rather than a report quietly missing a section.
    Ok(render::render(&template, &value)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use report_core::settings::Provider;
    use report_doc::markdown::to_markdown;

    fn template() -> Template {
        use report_core::template::{NodeKind, TemplateNode};
        let mut template = Template::new("Visit");
        template.nodes = vec![
            TemplateNode::new(
                "Summary",
                NodeKind::Paragraph { description: "What happened.".into() },
            ),
            TemplateNode::new(
                "Area",
                NodeKind::Section {
                    heading_description: "The area inspected.".into(),
                    children: vec![TemplateNode::new(
                        "Actions",
                        NodeKind::List {
                            description: "What to do.".into(),
                            ordered: true,
                            min_items: None,
                            max_items: None,
                        },
                    )],
                },
            ),
        ];
        template
    }

    #[tokio::test]
    async fn the_whole_flow_produces_a_document_shaped_by_the_template() {
        let settings = Settings { provider: Provider::Stub, ..Default::default() };
        let doc = generate(template(), "some notes".into(), settings).await.unwrap();

        // Structure comes from the template, so it is knowable without a model.
        let markdown = to_markdown(&doc);
        assert!(markdown.starts_with("TODO: summary"), "{markdown}");
        assert!(markdown.contains("# TODO: heading"), "{markdown}");
        assert!(markdown.contains("1. TODO: actions"), "{markdown}");
        // And it survives export unescaped, which square brackets would not.
        assert!(!markdown.contains('\\'), "placeholders must not need escaping: {markdown}");
    }

    #[tokio::test]
    async fn a_backend_that_cannot_be_built_fails_before_any_request() {
        // Isolated from this machine's data directory: without it the test passes or
        // fails depending on whether a model happens to have been downloaded here,
        // and on a developer's laptop it silently starts spawning worker processes.
        std::env::set_var(
            "REPORT_DATA_DIR",
            std::env::temp_dir().join("report-tool-generate-test"),
        );

        // Local with neither a configured path nor a downloaded model: the failure
        // must name what is missing and arrive immediately, not after a worker has
        // been spawned and a load attempted.
        let settings = Settings { provider: Provider::Local, ..Default::default() };
        let error = generate(template(), "notes".into(), settings).await.unwrap_err();
        let message = error.to_string();
        // Asserted per build, like the equivalent test in `report_core::settings`: a build
        // with no engine cannot be missing a *model*, and it refuses for its own equally
        // clear reason. Checking for "Settings" unconditionally made this fail on every
        // `--no-default-features` run, which is what CI does.
        if cfg!(feature = "inference") {
            assert!(
                message.contains("Settings"),
                "the message must say where to fix it: {message}"
            );
        } else {
            assert!(message.contains("no local engine"), "{message}");
        }
        std::env::remove_var("REPORT_DATA_DIR");
    }

    #[tokio::test]
    async fn empty_notes_still_generate_rather_than_erroring() {
        // Pressing Generate too early is a real state; it must produce a skeleton to
        // fill in, not a failure.
        let settings = Settings { provider: Provider::Stub, ..Default::default() };
        assert!(generate(template(), String::new(), settings).await.is_ok());
    }
}
