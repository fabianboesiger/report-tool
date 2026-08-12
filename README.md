# Report tool

Turns rough notes into structured reports. A **template** captures the shape of a
report and the intent of each part once; an LLM fills that shape from notes you type
or dictate. Everything can run locally.

```
pick a template → jot or dictate notes → Generate → edit → export .md
```

## How it works

A template is compiled into a JSON *shape*, the model is constrained to produce
exactly that shape, and the markdown is rendered from the result in Rust. Headings,
their levels, ordering and formatting therefore come from the template and never from
the model — which only ever supplies the text that goes inside them.

Constraining needs two dialects, because the backends speak different ones:

| Backend | Constraint | Enforced by |
|---|---|---|
| Remote (OpenAI-compatible) | JSON Schema | the server's `response_format` |
| Local (`llama.cpp`) | GBNF | the grammar sampler, per token |

Both are emitted from **one** traversal of the template (`report-core/src/compile.rs`),
so they cannot disagree about the structure. Whatever comes back is validated against
the template before anything is rendered.

## Layout

```
crates/doc/      report-doc     document model, markdown + HTML IO, block edits
crates/editor/   report-editor  WYSIWYG block editor (no JS framework)
crates/core/     report-core    templates, compilers, backends, workers, storage
app/             report-tool    Dioxus desktop app
```

`report-doc` sits below both `report-editor` and `report-core`, so neither depends on
the other and `report-core` never links Dioxus.

## Running

```sh
# Fast loop: no C++ engines, no models, builds in seconds.
cargo run -p report-tool --no-default-features

# Everything, including local inference.
cargo run -p report-tool
```

## Language

German, English, French and Italian. One setting under **Settings**, because it is one
decision — it sets the interface, the language reports are written in, and what dictation
expects to hear. It follows the operating system by default and falls back to English for a
language we do not ship.

Reports used to come out in whatever language the notes happened to be in: the system prompt
said "write in the same language as the notes" and left it to the model. It now names the
language outright, so the report language is something you chose rather than something you
discover. Dictation follows the same setting, with **Detect automatically** still on offer
for notes that switch language mid-recording.

The interface strings live in `app/assets/i18n/*.ftl` — [Fluent], one file per language,
compiled into the binary with `include_str!` like every other asset here. `en.ftl` is the
source of truth and the fallback. Three tests hold the set together: the catalogues must
define exactly the same keys, every `t!("…")` in the source must exist in `en.ftl`, and every
screen must draw in all four languages. That last one reads the log rather than expecting a
panic, because `dioxus-core` catches panics inside a component — see the module docs in
`app/src/ui.rs`, which explain how the first draft of it passed while checking nothing.

**The German, French and Italian have not been read by native speakers.** They are marked as
such at the top of each file.

**To test detection by hand**, note that `LANG` does nothing on macOS — `sys-locale` reads
CoreFoundation there:

```sh
./report-tool -AppleLanguages "(fr-CH)"   # macOS
LANG=fr_CH.UTF-8 ./report-tool            # Linux
```

[Fluent]: https://projectfluent.org

## Backends

Pick a backend under **Settings**:

- **Remote** — any OpenAI-compatible server: `api.openai.com/v1`, `localhost:11434/v1`
  for Ollama, LM Studio, a company gateway. Falls back through `json_schema` →
  `json_object` → prompt-only if the server does not support strict schemas.
  The API key is stored in **plain text** in the app's data directory.
- **Local** — a GGUF on this machine. Nothing leaves the device.
- **Stub** — placeholder text shaped by the template. Exercises the whole flow
  without a model.

## Models

The app fetches these itself on first open, smallest first, resuming where it left
off if you quit. Setting a path in Settings suppresses the download for that model.

| | Model | Size | Licence |
|---|---|---|---|
| Report | Gemma 4 E4B, QAT 4-bit | 5.2 GB | Apache-2.0 |
| Dictation | Whisper large-v3-turbo, 5-bit | 574 MB | MIT / Apache-2.0 |

**Why not a reasoning model.** Generation is grammar-constrained from the first
token, so a model that opens with `<think>` cannot: the grammar admits only `{`.
Qwen3.5 reasons by default and needs `enable_thinking=false` as a chat-template
argument, which `apply_chat_template` gives no way to send. Gemma 4 inverts it —
thinking is enabled by *adding* a `<|think|>` token — so doing nothing leaves it in
the mode we need.

**Why QAT.** We ship 4-bit out of necessity. Google's QAT releases are trained
quantized, so the same download size carries noticeably more of the full model's
quality than a quantization applied afterwards.

**Why turbo, and not `.en`.** Six times faster than large-v3 for a fraction of a
percent of word error rate, and smaller than `medium`. The multilingual build, because
these notes are as likely to be German as English — and because dictation is now *told*
which of the four to expect rather than detecting it.

## Storage

Templates, reports and settings live in one SQLite file, `report-tool.db`, under the
platform data directory (`~/Library/Application Support/ch.ajila.report-tool` on macOS).
`REPORT_DATA_DIR` overrides the location, which makes a portable install possible and is
what lets the storage tests run against real files.

They were previously one JSON file each. Two things decided the change, both measurable:

- **Listing the library read every byte of every report.** Summarising one meant parsing
  the whole file, and a report holds its notes, its generated prose *and* a template
  snapshot. The list needs five small fields — for one report here that is 75 bytes
  against 1,789, and the ratio grows with the library. `list_reports` is now covered by
  an index and reads no document text.
- **Autosave rewrites a report every two seconds.** As files, typing a sentence in the
  notes re-serialised the generated report and the template snapshot too.

Two things follow from it:

- **Templates export and import as `.json`** from the Templates screen. A template used
  to be a file you could email a colleague or commit; the buttons hand that back rather
  than letting it vanish with the storage change. An import always lands as a new
  template — never overwriting one you already have.
- **A report still carries a snapshot of its template**, not a reference, which is why
  there is no relation between the two tables. Editing a template must not reach back
  into reports already written from it.

The first launch after upgrading imports any old `templates/`, `reports/` and
`settings.json`, then renames them to `*.imported`. **Renamed, not deleted** — that code
runs once, on data with no other copy. A file that will not parse is skipped with a
warning rather than failing the import.

## Build prerequisites

The local engines are C++, and SQLite is C.

**All platforms:** a C compiler for SQLite, plus CMake and a C++ compiler for the
inference engines. A `--no-default-features` build needs only the C compiler — it used to
need no native toolchain at all, and the SQLite dependency changed that.

**Linux** — Dioxus desktop links webkit2gtk, and `cpal` needs ALSA:

```sh
sudo apt-get install -y pkg-config libglib2.0-dev libgtk-3-dev \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev libxdo-dev libasound2-dev
```

**GPU.** Metal is enabled automatically on macOS. Windows and Linux use Vulkan, which
covers NVIDIA, AMD, Intel and integrated GPUs from one binary:

```sh
cargo build -p report-tool --features vulkan   # needs the Vulkan SDK (glslc)
cargo build -p report-tool --features cuda     # NVIDIA only
```

`REPORT_BACKEND=cpu` forces CPU. An explicit GPU request against a build with no GPU
backend is a hard error rather than a silent hundredfold slowdown.

## Two things worth knowing before changing this

**The LLM and the transcriber run in separate processes.** `llama-cpp-2` and
`whisper-rs` each vendor their own copy of ggml, and the two copies need not agree on
`sizeof(ggml_tensor)`. They link without complaint; on Linux, symbol interposition can
bind a call to the wrong one, and it presents as a mysterious model bug rather than a
link error. See the warning at the top of `report-core/src/worker.rs`.

**`llama_sampler_sample` accepts the token internally.** Calling `accept` again
advances a grammar twice per token and aborts the process. Without a grammar the same
mistake only corrupts the repeat-penalty window, which is invisible — see the comment
in `report-core/src/llm.rs`.

## Development

```sh
cargo test --workspace                      # fast; where the logic is
cargo clippy --workspace -- -D warnings
cargo fmt --all

# Local generation end to end, without the UI.
cargo run -p report-core --example llm_smoke --features inference,metal -- model.gguf
```

`REPORT_GBNF` overrides the emitted grammar in that example, for isolating which
construct a given llama.cpp build rejects.

## Releases

Two workflows in `.github/workflows/`:

| | Trigger | Does |
|---|---|---|
| `test.yml` | push, PR, dispatch | fmt, clippy, the stub build and tests; then compiles each GPU backend on the platform that ships it |
| `release.yml` | push to `main`, dispatch | verifies, builds an installer per platform, and publishes when the version is new |

**To cut a release**, bump `version` under `[workspace.package]` in the root `Cargo.toml`
and push to `main`. The pipeline tags `v<version>`, builds all three installers and
attaches them. A push whose version is already released still builds — it just does not
publish, so the installers arrive as workflow artifacts instead. `workflow_dispatch` on any
branch is therefore a usable "give me installers" button.

Artifacts: `.dmg` for macOS on Apple silicon, `.deb` for x86-64 Linux, and an NSIS
`setup.exe` for x86-64 Windows. Windows and Linux carry the Vulkan backend, which covers
NVIDIA, AMD, Intel and integrated GPUs from one file; macOS gets Metal automatically.

The macOS app is **ad-hoc signed, not notarised** — enough to stop Gatekeeper calling it
damaged, but a first launch still needs right-click → Open. Real signing needs a paid
Developer ID in repository secrets.

Bumping `dioxus` means bumping `DX_VERSION` in `release.yml` in the same commit: `dx`
refuses to build a project whose dioxus version it does not match.

## Third-party

The icon set in `app/src/ui/kit/icon.rs` is vendored from [Lucide](https://lucide.dev)
v1.31.0, ISC licensed. The full notice — which ISC requires be kept with the copies — is
in [`licenses/LICENSE-lucide`](licenses/LICENSE-lucide). Each `match` arm names its
upstream icon so the set can be refreshed against a later release.

The application icon is built from the same source: Lucide's `notebook-pen`, set in a
rounded tile. Everything in `app/assets/icons/` is generated —

```sh
python3 tools/make-icons.py     # needs cairosvg + pillow; iconutil for the .icns
```

— so changing the mark means editing that script and re-running it rather than editing
binaries. It writes the two editable `.svg` masters alongside the rasters, and explains
why the small sizes are not simply the large one scaled down. Because a stock glyph is
not a distinctive trademark, this is a mark for internal tooling rather than a brand.
