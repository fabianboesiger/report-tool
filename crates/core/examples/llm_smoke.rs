//! End-to-end smoke test for the local path, without the UI.
//!
//! Loads a GGUF, compiles a template into a grammar, generates under that grammar,
//! validates the result against the template and renders it to markdown — the whole
//! chain except the editor. Confirms GPU offload works on this platform and that the
//! emitted GBNF is accepted by llama.cpp's own parser, which is the one thing the
//! unit tests cannot check for themselves.
//!
//! ```text
//! cargo run -p report-core --example llm_smoke --features inference,metal -- <model.gguf>
//! ```

use std::path::PathBuf;
use std::time::Instant;

use report_core::compile::Shape;
use report_core::template::{NodeKind, Template, TemplateNode};
use report_core::{prompt, render};

fn main() -> anyhow::Result<()> {
    let Some(path) = std::env::args().nth(1).map(PathBuf::from) else {
        anyhow::bail!("usage: llm_smoke <model.gguf>");
    };
    let context_tokens: usize =
        std::env::var("REPORT_CONTEXT").ok().and_then(|v| v.parse().ok()).unwrap_or(4096);

    let template = template();
    let shape = Shape::compile(&template);
    // `REPORT_GBNF` overrides the emitted grammar, for isolating which construct a
    // given llama.cpp build rejects.
    let probing = std::env::var("REPORT_GBNF").is_ok();
    let grammar = std::env::var("REPORT_GBNF").unwrap_or_else(|_| shape.to_gbnf());
    // `REPORT_LANGUAGE` picks the language the report is asked for, so the smoke test can
    // check that a German or French run is still grammar-constrained and still validates.
    // English by default, matching the notes below.
    let locale = std::env::var("REPORT_LANGUAGE")
        .ok()
        .and_then(|tag| report_core::Locale::from_tag(&tag))
        .unwrap_or(report_core::Locale::English);
    let system = prompt::system(&template, locale);
    let user = prompt::user(NOTES);

    eprintln!("--- grammar ---\n{grammar}");
    eprintln!("--- loading {} ---", path.display());

    let started = Instant::now();
    let mut llm = report_core::llm::Llm::load(&path, context_tokens)?;
    eprintln!("loaded in {:.1}s", started.elapsed().as_secs_f32());

    for round in 1..=2 {
        // Twice on purpose: the second run must reuse the KV cache for the whole
        // system prompt, since it is byte-identical. A second run as slow as the
        // first means the prefix reuse is not working.
        let started = Instant::now();
        let mut tokens = 0usize;
        let text = llm.generate_constrained(&system, &user, &grammar, 0.3, |n| tokens = n)?;
        let elapsed = started.elapsed().as_secs_f32();

        eprintln!(
            "\n--- run {round}: {tokens} tokens in {elapsed:.1}s ({:.1} tok/s) ---",
            tokens as f32 / elapsed.max(0.001)
        );
        println!("{text}");

        if probing {
            eprintln!("--- probe succeeded ---");
            return Ok(());
        }

        // The value must fit the template, not merely be valid JSON.
        let value: serde_json::Value = serde_json::from_str(&text)?;
        shape.accepts(&value).map_err(|e| anyhow::anyhow!("does not fit the template: {e}"))?;

        let document = render::render(&template, &value)?;
        eprintln!("--- rendered ---\n{}", report_doc::markdown::to_markdown(&document));
    }

    Ok(())
}

const NOTES: &str = "north wall: hairline cracking below the second window, about 30 cm long. \
                     roof: two slipped tiles above the west gable. \
                     tenant reports damp after heavy rain and wants a follow-up before winter.";

/// Small but structurally complete: a paragraph, a section with a generated heading,
/// a repeat and a bounded list. Enough to exercise every part of the grammar without
/// asking a 0.6B model for an essay.
fn template() -> Template {
    let mut template = Template::new("Site inspection");
    template.description = "A record of a building inspection visit.".into();
    template.nodes = vec![
        TemplateNode::new(
            "Summary",
            NodeKind::Paragraph {
                description: "One or two sentences summarising the visit.".into(),
            },
        ),
        TemplateNode::new(
            "Findings",
            NodeKind::Section {
                heading_description: "A short heading naming the inspected building area.".into(),
                children: vec![TemplateNode::new(
                    "Defects",
                    NodeKind::Repeat {
                        description: "One entry per defect mentioned in the notes.".into(),
                        item_label: "defect".into(),
                        min: Some(1),
                        max: Some(4),
                        children: vec![
                            TemplateNode::new(
                                "Detail",
                                NodeKind::Paragraph {
                                    description: "Where the defect is and what is wrong.".into(),
                                },
                            ),
                            TemplateNode::new(
                                "Actions",
                                NodeKind::List {
                                    description: "Short recommended actions.".into(),
                                    ordered: true,
                                    min_items: Some(1),
                                    max_items: Some(3),
                                },
                            ),
                        ],
                    },
                )],
            },
        ),
    ];
    template
}
