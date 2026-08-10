//! Local generation: a quantized GGUF through llama.cpp, constrained by a grammar.
//!
//! Synchronous by design. This runs inside the inference worker process (see
//! [`crate::worker`]), which does one thing at a time and has no UI to keep
//! responsive; an async layer here would be ceremony around a loop that is
//! CPU-driving-GPU from start to finish.
//!
//! Adapted from the `privstory` project's `llm.rs`, which is where the awkward parts
//! were worked out. What carried over, and why each matters:
//!
//! - **The self-referential lifetime.** `LlamaContext<'a>` borrows the `LlamaModel`
//!   it came from, so the two cannot be sibling struct fields. The model is boxed and
//!   leaked for the process lifetime instead: a worker holds at most one model and
//!   lives until it exits, which trades a bounded, deliberate leak for a large amount
//!   of unsafe self-referential plumbing.
//! - **KV-cache prefix reuse.** Each prompt is diffed against the last one and only
//!   the differing suffix is decoded. This is worth more here than in a chat app:
//!   [`crate::prompt::system`] is byte-identical across regenerations, so pressing
//!   Generate a second time skips the entire prefill of the instructions and the
//!   field guide.
//! - **Chunked prefill**, requesting logits only for the final token.
//! - **Stateful UTF-8 decoding.** One token is not necessarily a whole character, so
//!   a partial sequence has to be carried across iterations.
//!
//! ## What is new here
//!
//! Generation is **grammar-constrained**. `llama-cpp-2` exposes no schema-based
//! constraint, only GBNF, which is why [`crate::compile::Shape`] emits both dialects
//! from one traversal. With the grammar leading the sampler chain, a token that would
//! break the JSON shape is never sampled at all — so unlike the remote path, there is
//! no weaker mode to fall back to and no malformed output to recover from.

use std::num::NonZeroU32;
use std::path::Path;

use anyhow::{Context, Result};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaChatMessage, LlamaChatTemplate, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;

/// A loaded model, ready to generate.
pub struct Llm {
    session: Session,
    context_tokens: usize,
    /// The template embedded in the GGUF, if it has one.
    template: Option<LlamaChatTemplate>,
}

impl Llm {
    /// Load a GGUF. Expensive: several gigabytes read and uploaded to the GPU.
    pub fn load(gguf: &Path, context_tokens: usize) -> Result<Self> {
        let session = Session::open(gguf, context_tokens)?;

        // The template the GGUF carries is rendered by llama.cpp's own jinja
        // implementation, so we do not maintain one. Models differ enough in their
        // turn markers that guessing wrong degrades output in ways that look like a
        // bad model rather than a bad prompt.
        let template = match session.model.chat_template(None) {
            Ok(template) => {
                tracing::info!("llm: using the chat template embedded in the GGUF");
                Some(template)
            }
            Err(error) => {
                tracing::warn!("llm: GGUF has no chat template ({error}), using a generic one");
                None
            }
        };

        Ok(Self { session, context_tokens, template })
    }

    /// Generate a JSON value constrained by `grammar`.
    ///
    /// `on_progress` is called with the running token count so a caller can show that
    /// something is happening; a local model on CPU can take minutes.
    pub fn generate_constrained(
        &mut self,
        system: &str,
        user: &str,
        grammar: &str,
        temperature: f32,
        mut on_progress: impl FnMut(usize),
    ) -> Result<String> {
        let prompt = self.render_prompt(system, user)?;

        let sampler = LlamaSampler::chain_simple([
            // First in the chain: it masks every token that would break the shape, so
            // the samplers after it only ever choose between valid continuations.
            LlamaSampler::grammar(self.session.model, grammar, "root")
                .context("compiling the generated GBNF grammar")?,
            // Low, and deliberately so. The structure is already guaranteed, so
            // sampling variety buys nothing here and costs faithfulness to the notes.
            LlamaSampler::temp(temperature),
            LlamaSampler::dist(seed()),
        ]);

        self.session.generate(&prompt, self.context_tokens, sampler, &mut on_progress)
    }

    /// Build the prompt in the model's own format.
    ///
    /// Three routes, tried in order, because the obvious one fails on exactly the
    /// models we ship:
    ///
    /// 1. **The template embedded in the GGUF.** `llama_chat_apply_template` is not a
    ///    Jinja engine — it recognises a fixed set of formats by looking for marker
    ///    substrings such as `<start_of_turn>`. Gemma 4's 2026 template builds its
    ///    markers inside Jinja macros, so it contains none of them and detection
    ///    returns `ffi error -1`.
    /// 2. **A built-in renderer chosen by `general.architecture`.** llama.cpp has a
    ///    correct Gemma renderer; it just could not tell that this was Gemma. Naming
    ///    it directly gets the right format from llama.cpp rather than from a
    ///    hand-written guess here.
    /// 3. A generic ChatML prompt, as a last resort.
    ///
    /// Getting this wrong is quiet rather than loud: a Gemma model prompted with
    /// ChatML still answers, and simply leaks `<|im_end|>` into the middle of the
    /// report as literal text.
    fn render_prompt(&self, system: &str, user: &str) -> Result<String> {
        if let Some(template) = &self.template {
            if let Some(prompt) = self.apply(template, Some(system), user) {
                return Ok(prompt);
            }
            // Some families reject a `system` role outright; the instructions still
            // have to reach the model, so fold them into the user turn.
            if let Some(prompt) = self.apply(template, None, &fold_system_into_user(system, user)) {
                tracing::debug!(
                    "llm: this template has no system role; folded it into the user turn"
                );
                return Ok(prompt);
            }
        }

        if let Some(name) = self.builtin_template_name() {
            if let Ok(template) = LlamaChatTemplate::new(name) {
                if let Some(prompt) = self.apply(&template, Some(system), user) {
                    tracing::info!("llm: using llama.cpp's built-in `{name}` chat template");
                    return Ok(prompt);
                }
            }
        }

        tracing::warn!(
            "llm: no usable chat template, falling back to ChatML — output may contain \
             stray turn markers"
        );
        Ok(generic_chat_prompt(system, user))
    }

    /// Render through a template, or `None` if llama.cpp refuses it.
    fn apply(
        &self,
        template: &LlamaChatTemplate,
        system: Option<&str>,
        user: &str,
    ) -> Option<String> {
        let mut messages = Vec::with_capacity(2);
        if let Some(system) = system {
            messages.push(LlamaChatMessage::new("system".to_string(), system.to_string()).ok()?);
        }
        messages.push(LlamaChatMessage::new("user".to_string(), user.to_string()).ok()?);

        // `add_ass`: append the assistant header so the model continues rather than
        // opening a new turn.
        match self.session.model.apply_chat_template(template, &messages, true) {
            Ok(prompt) => Some(prompt),
            Err(error) => {
                tracing::debug!("llm: chat template rejected this message shape ({error})");
                None
            }
        }
    }

    /// A built-in llama.cpp template name for this model's architecture.
    fn builtin_template_name(&self) -> Option<&'static str> {
        let architecture = self.session.model.meta_val_str("general.architecture").ok()?;
        builtin_for_architecture(&architecture)
    }
}

/// Map a GGUF architecture onto one of llama.cpp's built-in chat templates.
///
/// Deliberately short. It exists only for models whose *embedded* template llama.cpp
/// cannot recognise, which in practice means new macro-based Jinja templates. Families
/// whose built-in name is ambiguous — `llama2` versus `llama3`, `mistral-v1` versus
/// `-v7` — are left out on purpose: guessing wrong there produces a subtly malformed
/// prompt, and their embedded templates are detected correctly anyway. Anything absent
/// falls through to ChatML, which is what most recent instruct models use.
fn builtin_for_architecture(architecture: &str) -> Option<&'static str> {
    const KNOWN: &[(&str, &str)] = &[
        // `gemma4`, `gemma3`, `gemma2` all render the same way.
        ("gemma", "gemma"),
        ("phi4", "phi4"),
        ("phi3", "phi3"),
        ("falcon3", "falcon3"),
        ("command-r", "command-r"),
        ("chatglm", "chatglm4"),
        ("exaone", "exaone4"),
        ("minicpm", "minicpm"),
        ("orion", "orion"),
        ("rwkv", "rwkv-world"),
    ];
    let architecture = architecture.to_ascii_lowercase();
    KNOWN.iter().find(|(prefix, _)| architecture.starts_with(prefix)).map(|(_, name)| *name)
}

/// Put the instructions at the top of the user turn.
///
/// Separated by a rule rather than run together, so the model can see where the
/// standing instructions end and this report's notes begin.
fn fold_system_into_user(system: &str, user: &str) -> String {
    if system.trim().is_empty() {
        return user.to_string();
    }
    format!("{}\n\n---\n\n{}", system.trim(), user)
}

/// A last-resort prompt for GGUFs that carry no template.
///
/// ChatML, because it is what most recent instruct models are tuned on and what
/// llama.cpp itself falls back to.
fn generic_chat_prompt(system: &str, user: &str) -> String {
    format!(
        "<|im_start|>system\n{system}<|im_end|>\n\
         <|im_start|>user\n{user}<|im_end|>\n\
         <|im_start|>assistant\n"
    )
}

fn seed() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u32)
        .unwrap_or(299_792_458)
}

/// Model, context and the tokens currently resident in the KV cache, owned together.
struct Session {
    /// Leaked for the process lifetime — see the module docs.
    model: &'static LlamaModel,
    ctx: LlamaContext<'static>,
    /// Exactly the tokens the KV cache holds for sequence 0. Diffed against each new
    /// prompt to decide how much can be reused.
    cached: Vec<LlamaToken>,
}

impl Session {
    fn open(gguf: &Path, context_tokens: usize) -> Result<Self> {
        // Checked first: initialising the ggml backend below compiles Metal shaders
        // and takes seconds, and a mistyped path should not cost that before saying
        // so.
        anyhow::ensure!(gguf.exists(), "no model file at {}", gguf.display());

        crate::gpu::load_backends();
        let device = crate::gpu::select()?;

        // One backend per process: `LlamaBackend::init` refuses a second call, and
        // the context borrows from it as well as from the model.
        static BACKEND: std::sync::OnceLock<&'static LlamaBackend> = std::sync::OnceLock::new();
        let backend = *BACKEND.get_or_init(|| {
            Box::leak(Box::new(LlamaBackend::init().expect("llama backend init failed")))
        });

        anyhow::ensure!(gguf.exists(), "no model file at {}", gguf.display());

        let params = LlamaModelParams::default().with_n_gpu_layers(device.n_gpu_layers());
        let model = LlamaModel::load_from_file(backend, gguf, &params)
            .with_context(|| format!("loading GGUF {}", gguf.display()))?;
        let model: &'static LlamaModel = Box::leak(Box::new(model));

        let n_ctx = u32::try_from(context_tokens).unwrap_or(8192).max(512);
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(n_ctx))
            // Prefill in large chunks; this is what makes a cold prompt fast.
            .with_n_batch(n_ctx.min(2048));
        let ctx = model.new_context(backend, ctx_params).context("creating the llama context")?;

        tracing::info!("llm: loaded {} on {} (n_ctx {n_ctx})", gguf.display(), device.as_str());

        Ok(Self { model, ctx, cached: Vec::new() })
    }

    fn generate(
        &mut self,
        prompt: &str,
        context_tokens: usize,
        mut sampler: LlamaSampler,
        on_progress: &mut impl FnMut(usize),
    ) -> Result<String> {
        // The rendered template already carries its own BOS and special tokens.
        let tokens = self
            .model
            .str_to_token(prompt, AddBos::Never)
            .map_err(|e| anyhow::anyhow!("tokenizing the prompt: {e}"))?;
        anyhow::ensure!(!tokens.is_empty(), "the prompt tokenized to nothing");
        anyhow::ensure!(
            tokens.len() < context_tokens,
            "the prompt is {} tokens but the context is {context_tokens}; \
             raise the context size or shorten the notes",
            tokens.len()
        );

        // How much of the cache is still valid? Never reuse the *whole* prompt: at
        // least one token must be decoded to produce logits to sample from.
        let mut reuse = common_prefix(&self.cached, &tokens);
        if reuse == tokens.len() {
            reuse = tokens.len() - 1;
        }

        // Drop everything after the shared prefix. The call reports whether the
        // partial removal actually happened — some memory layouts only support
        // clearing a whole sequence. If it declines, wipe and prefill from scratch,
        // so `cached` never claims more than the cache really holds.
        if reuse < self.cached.len() {
            let removed = self.ctx.clear_kv_cache_seq(Some(0), Some(reuse as u32), None)?;
            if !removed {
                tracing::debug!("llm: partial KV-cache removal declined, clearing the sequence");
                self.ctx.clear_kv_cache_seq(Some(0), None, None)?;
                reuse = 0;
            }
        }
        self.cached.truncate(reuse);

        let suffix = &tokens[reuse..];
        tracing::debug!(
            "llm: prompt {} tokens, {reuse} reused from cache, {} to prefill",
            tokens.len(),
            suffix.len()
        );

        let n_batch = self.ctx.n_batch() as usize;
        let mut batch = LlamaBatch::new(n_batch.max(1), 1);

        // Prefill in n_batch-sized chunks, asking for logits only on the very last
        // token of the whole prompt.
        for (chunk_index, chunk) in suffix.chunks(n_batch).enumerate() {
            batch.clear();
            let base = reuse + chunk_index * n_batch;
            let is_final = base + chunk.len() == tokens.len();
            for (offset, token) in chunk.iter().enumerate() {
                let wants_logits = is_final && offset == chunk.len() - 1;
                batch.add(*token, (base + offset) as i32, &[0], wants_logits)?;
            }
            self.ctx.decode(&mut batch)?;
        }
        self.cached.extend_from_slice(suffix);

        let budget = context_tokens.saturating_sub(tokens.len());
        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut out = String::new();

        for produced in 0..budget {
            // `sample` already calls `llama_sampler_accept` internally, so the token
            // must NOT be accepted again here. Doing so advances the grammar twice
            // per token: the first token is accepted, the second application finds a
            // state machine that has consumed a character nobody emitted, and
            // llama.cpp aborts the process on `GGML_ASSERT(!stacks.empty())`.
            //
            // Without a grammar the same double-accept merely corrupts the
            // repeat-penalty window, which is invisible — which is exactly why this
            // is easy to carry around unnoticed until a grammar is added.
            let token = sampler.sample(&self.ctx, -1);
            if self.model.is_eog_token(token) {
                break;
            }

            match self.model.token_to_piece(token, &mut decoder, false, None) {
                Ok(piece) => out.push_str(&piece),
                Err(error) => {
                    tracing::warn!("llm: detokenize failed: {error}");
                    break;
                }
            }
            on_progress(produced + 1);

            batch.clear();
            // `cached.len()` *is* the next position: it mirrors the KV cache exactly,
            // so a separate counter would only be a second source of truth able to
            // drift out of sync with it.
            batch.add(token, self.cached.len() as i32, &[0], true)?;
            self.ctx.decode(&mut batch)?;
            self.cached.push(token);

            if self.cached.len() + 1 >= context_tokens {
                // With a grammar this means the JSON is truncated, which the caller
                // will catch when it fails to parse — but saying so here names the
                // real cause.
                tracing::warn!("llm: hit the context limit, stopping generation");
                break;
            }
        }

        Ok(out)
    }
}

// `LlamaContext` holds a raw pointer and so is not auto-`Send`. The whole session is
// owned exclusively by the worker's single thread and never shared, so moving it is
// sound.
unsafe impl Send for Session {}

/// Length of the longest common prefix of `a` and `b`.
fn common_prefix<T: PartialEq>(a: &[T], b: &[T]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_prefix_lengths() {
        assert_eq!(common_prefix::<u32>(&[], &[]), 0);
        assert_eq!(common_prefix(&[1, 2, 3], &[]), 0);
        assert_eq!(common_prefix(&[1, 2, 3], &[1, 2, 3]), 3);
        assert_eq!(common_prefix(&[1, 2, 3], &[1, 2, 9, 9]), 2);
        // The case this exists for: regenerating a report re-sends the identical
        // system prompt, so the whole instruction block is a prefix of the new one.
        assert_eq!(common_prefix(&[1, 2, 3], &[1, 2, 3, 4, 5]), 3);
    }

    #[test]
    fn gemma_maps_onto_llama_cpps_built_in_renderer() {
        // The case this exists for: Gemma 4's embedded template is invisible to
        // llama.cpp's substring detector, and without this the model would be
        // prompted with ChatML markers it does not speak.
        assert_eq!(builtin_for_architecture("gemma4"), Some("gemma"));
        assert_eq!(builtin_for_architecture("gemma3"), Some("gemma"));
        assert_eq!(builtin_for_architecture("GEMMA4"), Some("gemma"));
    }

    #[test]
    fn ambiguous_families_are_left_to_chatml_rather_than_guessed() {
        // `llama2` and `llama3` are different formats and the architecture does not
        // say which; a wrong guess is worse than the generic prompt, and their
        // embedded templates are detected correctly anyway.
        assert_eq!(builtin_for_architecture("llama"), None);
        assert_eq!(builtin_for_architecture("mistral"), None);
        // Qwen is ChatML, which is the fallback.
        assert_eq!(builtin_for_architecture("qwen3"), None);
        assert_eq!(builtin_for_architecture("something-new"), None);
    }

    #[test]
    fn folding_keeps_the_instructions_and_the_notes_apart() {
        let folded = fold_system_into_user("INSTRUCTIONS", "Notes: a crack.");
        assert!(folded.starts_with("INSTRUCTIONS"));
        assert!(folded.contains("---"), "the model must see where the notes begin");
        assert!(folded.ends_with("Notes: a crack."));
        // Nothing to fold in.
        assert_eq!(fold_system_into_user("  ", "just notes"), "just notes");
    }

    #[test]
    fn the_generic_prompt_closes_every_turn_and_opens_the_assistant() {
        // A malformed fallback prompt shows up as a model that rambles or answers
        // the wrong question, which is hard to trace back to here.
        let prompt = generic_chat_prompt("SYS", "USER");
        assert_eq!(prompt.matches("<|im_start|>").count(), 3);
        assert_eq!(prompt.matches("<|im_end|>").count(), 2);
        assert!(prompt.ends_with("<|im_start|>assistant\n"));
        assert!(prompt.contains("SYS") && prompt.contains("USER"));
    }
}
