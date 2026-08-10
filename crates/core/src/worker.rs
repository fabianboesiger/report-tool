//! Out-of-process inference workers.
//!
//! The heavy engines run in **persistent child processes**, never in the GUI process,
//! and never in the *same* process as each other. The binary re-executes itself with
//! [`WORKER_FLAG`]; parent and child speak JSON lines over stdin and stdout.
//!
//! # DO NOT MERGE THE TWO WORKERS
//!
//! `llama-cpp-2` and `whisper-rs` each vendor **their own copy of ggml**. The two
//! copies are built independently and need not agree on `sizeof(ggml_tensor)` or on
//! the offset of any field within it. They link without complaint, and the linker
//! keeps **one** of each duplicated symbol.
//!
//! This is not theoretical, and macOS does not save you. In a binary linking both
//! engines, `nm` shows a single `ggml_metal_device_free`, and a process that loaded
//! only a *llama* model aborts on exit inside **whisper's** copy of
//! `ggml-metal-device.m` — one engine's call bound to the other's implementation.
//! Linux's flat ELF namespace offers even less protection.
//!
//! Keeping one engine per process sidesteps the question at runtime: a given process
//! only ever initialises one ggml backend and loads one model, so the two copies are
//! never live at once. The remaining exposure is at *exit*, where C++ static
//! destructors run — which is why [`run_child`] leaves without running them.
//!
//! Two further benefits fall out of the same structure. A worker stays alive with its
//! model **resident**, so a run of generations pays the multi-gigabyte load cost once
//! and the KV cache survives between them. And a model that exhausts memory or
//! crashes takes down a child, not the user's unsaved report.

use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// argv marker that puts the binary into worker mode. Followed by the worker kind.
pub const WORKER_FLAG: &str = "--inference-worker";

/// Which engine a worker hosts. One per process — see the warning above.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Llm,
    /// Speech-to-text. Reserved now so the separation is structural from the start
    /// rather than something to retrofit once both engines are in use.
    Stt,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Llm => "llm",
            Kind::Stt => "stt",
        }
    }

    pub fn parse(text: &str) -> Option<Kind> {
        match text {
            "llm" => Some(Kind::Llm),
            "stt" => Some(Kind::Stt),
            _ => None,
        }
    }
}

/// A unit of work, one JSON line on the child's stdin.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Request {
    GenerateJson {
        model_path: PathBuf,
        context_tokens: usize,
        system: String,
        user: String,
        grammar: String,
        temperature: f32,
    },
    Transcribe {
        model_path: PathBuf,
        /// 16 kHz mono `f32` samples, little-endian, base64.
        ///
        /// Base64 rather than a binary frame so the protocol stays one JSON line per
        /// message. A 30-second clip is about 2.6 MB encoded, which is fine for a
        /// one-shot request; continuous dictation would want a framed channel.
        pcm_base64: String,
        language: Option<String>,
    },
}

/// A reply, one JSON line on the child's stdout. A request produces any number of
/// `Progress` lines and exactly one `Done` or `Error`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    /// Emitted before a multi-gigabyte load so the UI can say why it is waiting.
    Loading {
        model_path: PathBuf,
    },
    Progress {
        tokens: usize,
    },
    Done {
        text: String,
    },
    Error {
        message: String,
    },
}

// ---------------------------------------------------------------------------
// Parent side
// ---------------------------------------------------------------------------

#[cfg(feature = "worker")]
mod parent {
    use super::*;
    use anyhow::{anyhow, bail, Context};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::process::{Child, ChildStdin, ChildStdout, Command};

    struct Running {
        child: Child,
        stdin: ChildStdin,
        stdout: BufReader<ChildStdout>,
    }

    /// A persistent child process holding one engine.
    pub struct Worker {
        kind: Kind,
        /// Also the request lock: the model is a single resource, so requests are
        /// serialised rather than queued into a child that could not run them
        /// concurrently anyway.
        running: tokio::sync::Mutex<Option<Running>>,
    }

    impl Worker {
        pub fn new(kind: Kind) -> Self {
            Self { kind, running: tokio::sync::Mutex::new(None) }
        }

        /// Send a request and wait for its result, reporting progress as it arrives.
        pub async fn request(
            &self,
            request: Request,
            mut on_progress: impl FnMut(Event),
        ) -> Result<String> {
            let mut guard = self.running.lock().await;
            if guard.is_none() {
                *guard = Some(spawn(self.kind)?);
            }

            match exchange(guard.as_mut().expect("just spawned"), request, &mut on_progress).await {
                Ok(text) => Ok(text),
                Err(error) => {
                    // A broken pipe means the child died — commonly the OS killing it
                    // for memory. Dropping it here means the next attempt starts a
                    // fresh one instead of the app being permanently broken until
                    // restart.
                    if error.downcast_ref::<std::io::Error>().is_some() {
                        tracing::warn!(
                            "worker[{}]: died ({error}); it will be restarted",
                            self.kind.as_str()
                        );
                        *guard = None;
                    }
                    Err(error)
                }
            }
        }

        /// Stop the child and release its model.
        pub async fn shutdown(&self) {
            let mut guard = self.running.lock().await;
            if let Some(mut running) = guard.take() {
                let _ = running.child.kill().await;
            }
        }
    }

    fn spawn(kind: Kind) -> Result<Running> {
        let exe = std::env::current_exe().context("locating this executable")?;
        let mut child = Command::new(&exe)
            .arg(WORKER_FLAG)
            .arg(kind.as_str())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            // Inherited on purpose: the child's log lines belong in the same terminal
            // as the parent's, and a model that fails to load says why on stderr.
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| {
                format!("spawning the {} worker ({})", kind.as_str(), exe.display())
            })?;

        let stdin = child.stdin.take().ok_or_else(|| anyhow!("worker stdin was not piped"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("worker stdout was not piped"))?;
        tracing::info!("worker[{}]: started", kind.as_str());
        Ok(Running { child, stdin, stdout: BufReader::new(stdout) })
    }

    async fn exchange(
        running: &mut Running,
        request: Request,
        on_progress: &mut impl FnMut(Event),
    ) -> Result<String> {
        let mut line = serde_json::to_string(&request).context("encoding the request")?;
        line.push('\n');
        running.stdin.write_all(line.as_bytes()).await?;
        running.stdin.flush().await?;

        let mut buffer = String::new();
        loop {
            buffer.clear();
            let read = running.stdout.read_line(&mut buffer).await?;
            if read == 0 {
                // EOF: the child exited without answering.
                bail!(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "the inference worker exited without replying",
                ));
            }
            let trimmed = buffer.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<Event>(trimmed) {
                Ok(Event::Done { text }) => return Ok(text),
                Ok(Event::Error { message }) => bail!(message),
                Ok(event) => on_progress(event),
                // Anything the child prints that is not an event — a stray println,
                // a library writing to stdout — is noise rather than a protocol
                // failure, so it is logged and skipped.
                Err(_) => tracing::debug!("worker: ignoring non-event output {trimmed:?}"),
            }
        }
    }

    /// The process-wide transcription worker.
    ///
    /// Separate from [`llm_worker`] and deliberately so — see the module docs. It is
    /// also why dictating while a report generates is possible at all: two processes,
    /// two models, neither blocking the other.
    pub fn stt_worker() -> &'static Worker {
        static WORKER: std::sync::OnceLock<Worker> = std::sync::OnceLock::new();
        WORKER.get_or_init(|| Worker::new(Kind::Stt))
    }

    /// The process-wide generation worker.
    ///
    /// One per process, created on first use and kept for the lifetime of the app.
    /// Building a new one per request would reload several gigabytes of weights every
    /// time the user pressed Generate.
    pub fn llm_worker() -> &'static Worker {
        static WORKER: std::sync::OnceLock<Worker> = std::sync::OnceLock::new();
        WORKER.get_or_init(|| Worker::new(Kind::Llm))
    }
}

#[cfg(feature = "worker")]
pub use parent::{llm_worker, stt_worker, Worker};

// ---------------------------------------------------------------------------
// Child side
// ---------------------------------------------------------------------------

/// Run as a worker. Never returns.
///
/// Reads one request per line, answers with events, and keeps its model resident
/// between requests.
pub fn run_child(kind: Kind) -> ! {
    use std::io::{BufRead, Write};

    quiet_engine_logs();
    tracing::info!("worker[{}]: ready", kind.as_str());

    let stdin = std::io::stdin();
    let mut state = ChildState::default();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            // The parent closed the pipe: it has gone away, and so should we.
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }

        let mut emit = |event: Event| {
            let mut out = std::io::stdout().lock();
            if let Ok(encoded) = serde_json::to_string(&event) {
                let _ = writeln!(out, "{encoded}");
                // Flushed per line: the parent is blocked on `read_line`, so a
                // buffered progress event would arrive only once something else
                // pushed it out — which for the final event is never.
                let _ = out.flush();
            }
        };

        match serde_json::from_str::<Request>(&line) {
            Ok(request) => {
                if let Err(error) = state.handle(kind, request, &mut emit) {
                    emit(Event::Error { message: format!("{error:#}") });
                }
            }
            Err(error) => emit(Event::Error { message: format!("unreadable request: {error}") }),
        }
    }

    tracing::info!("worker[{}]: stdin closed, exiting", kind.as_str());
    leave();
}

/// Route the C++ engines' logging into `tracing` instead of straight to stderr.
///
/// Both llama.cpp and whisper.cpp narrate loudly — loading a model emits one line per
/// tensor, several hundred of them — and they write to **stderr**, which a worker
/// inherits from its parent. Under `dx serve` that stderr is captured and every line
/// of it is labelled `ERROR`, so a perfectly healthy model load looks like hundreds of
/// failures scrolling past:
///
/// ```text
/// 65.07s ERROR [macos] create_tensor: loading tensor blk.30.ffn_norm.weight
/// ```
///
/// Routed through `tracing`, the same lines carry their real level and the usual
/// `RUST_LOG` filter applies — so they are off by default and available when a model
/// genuinely fails to load.
#[cfg(feature = "inference")]
fn quiet_engine_logs() {
    // Enabled, not suppressed: a model that fails to load explains why in these logs,
    // and throwing them away would trade noise for a silent failure.
    llama_cpp_2::send_logs_to_tracing(llama_cpp_2::LogOptions::default().with_logs_enabled(true));
    whisper_rs::install_logging_hooks();
}

#[cfg(not(feature = "inference"))]
fn quiet_engine_logs() {}

/// Exit without running C++ static destructors.
///
/// Both ggml copies register teardown at exit, and because the linker kept only one
/// of each duplicated symbol, the destructor that runs is not necessarily the one
/// belonging to the engine this process actually used. It aborts — after the work is
/// finished and delivered, so it costs nothing but a crash report, which is exactly
/// the kind of noise that sends someone hunting a bug that is not there.
///
/// Skipping them is safe here because a worker owns nothing that needs flushing: every
/// event is written and flushed as a whole line the moment it is produced, and the
/// model's memory is the operating system's problem once the process is gone.
fn leave() -> ! {
    #[cfg(unix)]
    // SAFETY: `_exit` is async-signal-safe and simply ends the process. Nothing in
    // this process holds state that outlives it.
    unsafe {
        libc::_exit(0)
    }
    #[cfg(not(unix))]
    std::process::exit(0)
}

#[derive(Default)]
struct ChildState {
    /// The loaded model and the path it came from, kept between requests so the load
    /// cost is paid once.
    #[cfg(feature = "inference")]
    llm: Option<(PathBuf, crate::llm::Llm)>,
    /// Never populated in the same process as `llm` — the worker kind decides which
    /// of the two a process may load, and the other stays `None` forever.
    #[cfg(feature = "inference")]
    stt: Option<(PathBuf, crate::stt::Stt)>,
}

impl ChildState {
    #[cfg(feature = "inference")]
    fn handle(&mut self, kind: Kind, request: Request, emit: &mut impl FnMut(Event)) -> Result<()> {
        match request {
            Request::GenerateJson {
                model_path,
                context_tokens,
                system,
                user,
                grammar,
                temperature,
            } => {
                anyhow::ensure!(
                    kind == Kind::Llm,
                    "the {} worker cannot generate text — this is the process split that keeps \
                     llama.cpp and whisper.cpp apart",
                    kind.as_str()
                );

                // Reload only when the path changes; the common case is the same
                // model for the whole session.
                let stale = self.llm.as_ref().is_none_or(|(path, _)| path != &model_path);
                if stale {
                    emit(Event::Loading { model_path: model_path.clone() });
                    // Dropped first so the old model's memory is released before the
                    // new one is read — otherwise both are resident at once, which on
                    // a machine sized for one is where the OOM killer arrives.
                    self.llm = None;
                    let llm = crate::llm::Llm::load(&model_path, context_tokens)?;
                    self.llm = Some((model_path, llm));
                }

                let (_, llm) = self.llm.as_mut().expect("just loaded");
                let text =
                    llm.generate_constrained(&system, &user, &grammar, temperature, |tokens| {
                        // Every 16 tokens: often enough to look alive, rarely enough that
                        // the pipe is not the bottleneck.
                        if tokens % 16 == 0 {
                            emit(Event::Progress { tokens });
                        }
                    })?;
                emit(Event::Done { text });
                Ok(())
            }

            Request::Transcribe { model_path, pcm_base64, language } => {
                anyhow::ensure!(
                    kind == Kind::Stt,
                    "the {} worker cannot transcribe — this is the process split that keeps \
                     llama.cpp and whisper.cpp apart",
                    kind.as_str()
                );

                let pcm = decode_pcm(&pcm_base64)?;

                let stale = self.stt.as_ref().is_none_or(|(path, _)| path != &model_path);
                if stale {
                    emit(Event::Loading { model_path: model_path.clone() });
                    // Dropped before the new one is read, so both are never resident.
                    self.stt = None;
                    let stt = crate::stt::Stt::load(&model_path)?;
                    self.stt = Some((model_path, stt));
                }

                let (_, stt) = self.stt.as_mut().expect("just loaded");
                let text = stt.transcribe(&pcm, language.as_deref())?;
                emit(Event::Done { text });
                Ok(())
            }
        }
    }

    #[cfg(not(feature = "inference"))]
    fn handle(
        &mut self,
        _kind: Kind,
        _request: Request,
        _emit: &mut impl FnMut(Event),
    ) -> Result<()> {
        anyhow::bail!("this build has no inference engine (built without `inference`)")
    }
}

/// Encode 16 kHz mono `f32` samples for the wire.
pub fn encode_pcm(pcm: &[f32]) -> String {
    let mut bytes = Vec::with_capacity(pcm.len() * 4);
    for sample in pcm {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    base64_encode(&bytes)
}

/// Decode what [`encode_pcm`] produced.
pub fn decode_pcm(encoded: &str) -> Result<Vec<f32>> {
    let bytes = base64_decode(encoded)?;
    anyhow::ensure!(bytes.len() % 4 == 0, "the audio payload is not a whole number of samples");
    Ok(bytes.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect())
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64. Hand-rolled rather than pulled in as a dependency: it is one
/// screen of code used in exactly one place, and it keeps the protocol readable
/// without adding a crate to every build.
fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = u32::from_be_bytes([0, b[0], b[1], b[2]]);
        out.push(B64[(n >> 18 & 63) as usize] as char);
        out.push(B64[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 { B64[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { B64[(n & 63) as usize] as char } else { '=' });
    }
    out
}

fn base64_decode(text: &str) -> Result<Vec<u8>> {
    let value = |c: u8| -> Result<u32> {
        B64.iter()
            .position(|b| *b == c)
            .map(|i| i as u32)
            .ok_or_else(|| anyhow::anyhow!("invalid base64 character {:?}", c as char))
    };

    let cleaned: Vec<u8> =
        text.bytes().filter(|b| !b.is_ascii_whitespace() && *b != b'=').collect();
    let mut out = Vec::with_capacity(cleaned.len() / 4 * 3);
    for chunk in cleaned.chunks(4) {
        let mut n = 0u32;
        for (i, byte) in chunk.iter().enumerate() {
            n |= value(*byte)? << (18 - 6 * i);
        }
        // A chunk of n base64 characters carries n-1 bytes.
        for i in 0..chunk.len().saturating_sub(1) {
            out.push((n >> (16 - 8 * i)) as u8);
        }
    }
    Ok(out)
}

/// Check argv for [`WORKER_FLAG`] and, if present, become that worker.
///
/// Call at the very top of `main`, before any window is created: a worker must never
/// open a UI.
pub fn take_over_if_worker() {
    let args: Vec<String> = std::env::args().collect();
    let Some(position) = args.iter().position(|arg| arg == WORKER_FLAG) else {
        return;
    };
    let kind = args.get(position + 1).map(String::as_str).and_then(Kind::parse);
    match kind {
        Some(kind) => run_child(kind),
        None => {
            eprintln!("{WORKER_FLAG} needs a worker kind: llm or stt");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_kinds_round_trip_through_argv() {
        for kind in [Kind::Llm, Kind::Stt] {
            assert_eq!(Kind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(Kind::parse("both"), None, "there is no worker hosting both engines");
        assert_eq!(Kind::parse(""), None);
    }

    #[test]
    fn requests_and_events_survive_the_pipe() {
        let request = Request::GenerateJson {
            model_path: PathBuf::from("/models/x.gguf"),
            context_tokens: 8192,
            system: "sys".into(),
            user: "notes".into(),
            grammar: "root ::= \"{}\"".into(),
            temperature: 0.3,
        };
        let line = serde_json::to_string(&request).unwrap();
        assert!(!line.contains('\n'), "a request must fit on one line: {line}");
        match serde_json::from_str(&line).unwrap() {
            Request::GenerateJson { grammar, context_tokens, .. } => {
                assert_eq!(grammar, "root ::= \"{}\"");
                assert_eq!(context_tokens, 8192);
            }
            other => panic!("wrong variant: {other:?}"),
        }

        for event in [
            Event::Loading { model_path: PathBuf::from("/models/x.gguf") },
            Event::Progress { tokens: 32 },
            Event::Done { text: "{\"a\":1}".into() },
            Event::Error { message: "boom".into() },
        ] {
            let line = serde_json::to_string(&event).unwrap();
            assert!(!line.contains('\n'), "an event must fit on one line: {line}");
            serde_json::from_str::<Event>(&line).unwrap();
        }
    }

    #[test]
    fn audio_survives_the_round_trip_through_the_wire() {
        let pcm: Vec<f32> = (0..1000)
            .map(|n| (n as f32 / 100.0).sin() * if n % 7 == 0 { -1.0 } else { 1.0 })
            .collect();
        let encoded = encode_pcm(&pcm);
        assert!(!encoded.contains('\n'), "the payload must fit on one line");
        assert_eq!(decode_pcm(&encoded).unwrap(), pcm);
    }

    #[test]
    fn base64_handles_every_payload_length() {
        // The padding cases are where hand-rolled base64 goes wrong.
        for len in 0..16usize {
            let bytes: Vec<u8> = (0..len).map(|i| (i * 37 + 11) as u8).collect();
            let encoded = base64_encode(&bytes);
            assert_eq!(base64_decode(&encoded).unwrap(), bytes, "length {len}");
        }
    }

    #[test]
    fn base64_matches_the_standard_alphabet_and_padding() {
        // Checked against known vectors, since a private encoding that only round
        // trips with itself would be undetectably wrong.
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_decode("Zm9vYmFy").unwrap(), b"foobar");
    }

    #[test]
    fn a_corrupt_payload_is_reported_rather_than_producing_noise() {
        assert!(base64_decode("not valid!").is_err());
        assert!(decode_pcm("Zg==").is_err(), "one byte is not a whole f32 sample");
    }

    #[test]
    fn generated_content_containing_newlines_still_fits_one_line() {
        // Report prose is full of newlines; if they were not escaped, the parent
        // would read the first fragment as a whole event and desynchronise.
        let event = Event::Done { text: "first\nsecond\r\nthird".into() };
        let line = serde_json::to_string(&event).unwrap();
        assert!(!line.contains('\n'));
        let Event::Done { text } = serde_json::from_str(&line).unwrap() else { panic!() };
        assert_eq!(text, "first\nsecond\r\nthird");
    }
}
