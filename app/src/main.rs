//! Report tool — desktop entry point.

mod audio;
mod dictate;
mod generate;
mod models;
mod ui;

use dioxus::prelude::*;
use report_core::settings::Settings;
use report_doc::RichDoc;
use report_editor::{doc_to_markdown, EditorRuntime};

use crate::dictate::use_dictation;
use crate::generate::{generate, Status};
use crate::models::use_model_downloads;
use crate::ui::{
    starter_template, use_autosave, EditorScreen, Rail, ReportsScreen, Screen, SettingsPanel,
    TemplatesScreen, Workspace,
};

fn main() {
    tracing_subscriber::fmt()
        // **stderr, not stdout.** `fmt()` defaults to stdout, and in an inference
        // worker stdout *is* the protocol channel — every log line would land in the
        // middle of the JSON the parent is parsing.
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                // The C++ engines are pinned to `warn`: loading a model emits a line
                // per tensor at info, which buries everything else. `RUST_LOG` still
                // overrides this when a model will not load and you need to see why.
                //
                // `llama-cpp-2` is hyphenated because that is the literal target the
                // crate compiles into its log metadata — `llama_cpp_2` matches
                // nothing, which is exactly the mistake that let the noise through.
                .unwrap_or_else(|_| {
                    "info,report_core=debug,report_editor=debug,\
                     llama-cpp-2=warn,whisper_rs=warn"
                        .into()
                }),
        )
        .init();

    // Before anything else: if this process was started as an inference worker it
    // must serve the pipe and never open a window. `run_child` does not return.
    report_core::worker::take_over_if_worker();

    dioxus::LaunchBuilder::desktop().with_cfg(window()).launch(App);
}

/// The window the app opens in.
///
/// Sized for what is actually on screen: a fixed rail, and a body that is two panes side
/// by side almost everywhere — notes beside the written report, the template beside the
/// prompt it produces. At the default 800×600 a webview gives each pane about 340 logical
/// pixels, which is narrower than the prose in it, so both wrap into columns and the app
/// looks broken before it has done anything.
///
/// The minimum is where the two-pane layout stops being usable rather than where it stops
/// rendering; below it the panes are worth less than the space they cost, and the
/// stylesheet drops to one column.
fn window() -> dioxus::desktop::Config {
    use dioxus::desktop::{Config, LogicalSize, WindowBuilder};

    let mut builder = WindowBuilder::new()
        // Also the macOS menu-bar name and what a window manager labels it.
        .with_title("Report tool")
        .with_inner_size(LogicalSize::new(1360.0, 900.0))
        .with_min_inner_size(LogicalSize::new(880.0, 600.0));

    // Set at runtime as well as in the bundle, because the two cover different places: the
    // bundle icon is what Finder, Explorer and a `.desktop` launcher read, and this is what
    // a Linux window manager's task list and every `dx serve` build show. macOS ignores it
    // and uses the bundle, which is why a missing icon here is a warning and not fatal.
    match window_icon() {
        Ok(icon) => builder = builder.with_window_icon(Some(icon)),
        Err(error) => tracing::warn!("window icon: {error:#}"),
    }

    Config::new().with_window(builder)
}

/// Decode the window icon that was compiled into the binary.
///
/// `include_bytes!`, so there is no file to be missing at runtime and no path to resolve
/// differently between `dx serve` and a bundle. The PNG comes from
/// `tools/make-icons.py`; it is 64×64 because window-manager task lists ask for something
/// in the 16–48 range and downscaling one source beats shipping four.
fn window_icon() -> anyhow::Result<dioxus::desktop::tao::window::Icon> {
    use anyhow::Context;

    let (pixels, width, height) = decode_icon(ICON_PNG)?;
    dioxus::desktop::tao::window::Icon::from_rgba(pixels, width, height)
        .context("building the window icon")
}

/// The icon bytes, compiled in.
const ICON_PNG: &[u8] = include_bytes!("../assets/icons/window-icon.png");

/// Decode a PNG to RGBA8 with its dimensions.
///
/// Split out from [`window_icon`] only so a test can assert the committed asset decodes —
/// `tao::window::Icon` exposes neither its size nor its pixels, so testing through it
/// could confirm nothing beyond "did not error".
fn decode_icon(bytes: &[u8]) -> anyhow::Result<(Vec<u8>, u32, u32)> {
    use anyhow::Context;

    // `Cursor`, because png 0.18 wants `Read + Seek` and a slice is only `Read`.
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    // The generator writes RGBA8 already; normalising anyway means a regenerated asset in
    // some other colour type still loads rather than silently producing a scrambled icon.
    decoder.set_transformations(png::Transformations::normalize_to_color8());

    let mut reader = decoder.read_info().context("reading the icon header")?;
    let mut pixels = vec![0u8; reader.output_buffer_size().context("icon is too large")?];
    let frame = reader.next_frame(&mut pixels).context("decoding the icon")?;

    anyhow::ensure!(
        frame.color_type == png::ColorType::Rgba,
        "expected RGBA, got {:?} — regenerate with tools/make-icons.py",
        frame.color_type,
    );
    pixels.truncate(frame.buffer_size());
    Ok((pixels, frame.width, frame.height))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The asset is generated by a script that is not run in CI, so the thing worth
    /// asserting is that what is *committed* still decodes — a regenerated PNG in the wrong
    /// colour type would otherwise degrade to a logged warning and a default icon.
    #[test]
    fn the_committed_window_icon_decodes_to_rgba() {
        let (pixels, width, height) = decode_icon(ICON_PNG).expect("the icon must decode");
        assert_eq!((width, height), (64, 64), "tools/make-icons.py writes 64x64");
        assert_eq!(pixels.len(), 64 * 64 * 4, "four bytes per pixel");
        // Not a blank tile: the glyph is off-white on near-black, so both must be present.
        assert!(pixels.chunks(4).any(|p| p[0] > 200), "no light pixels — the glyph is missing");
        assert!(pixels.chunks(4).any(|p| p[0] < 60), "no dark pixels — the tile is missing");
    }

    /// Garbage must fail rather than panic, since this runs before the window exists.
    #[test]
    fn a_corrupt_icon_is_an_error_not_a_panic() {
        assert!(decode_icon(b"this is not a png").is_err());
    }
}

#[component]
fn App() -> Element {
    // Resolved once at startup so the chosen backend appears in the log before any
    // model is loaded; a wrong choice here is otherwise only visible as slowness.
    use_hook(|| match report_core::gpu::select() {
        Ok(backend) => tracing::info!("compute backend: {}", backend.as_str()),
        Err(error) => tracing::error!("gpu: {error}"),
    });

    let screen = use_signal(|| Screen::Reports);
    let settings = use_signal(Settings::load);

    // **Two templates, not one.** These were a single signal, and sharing it was a data
    // bug rather than a shortcut: opening a template on the Templates screen reassigned the
    // open report's snapshot, autosave subscribes to that signal, and the report was
    // rewritten on disk to follow a template nobody had chosen for it. A snapshot exists
    // precisely so editing a template cannot reach into reports already written from it,
    // and one shared signal handed that protection straight back.
    //
    // The report's template changes only when a report is opened, or when someone picks a
    // different one for it — see `ui::template_picker`.
    let report_template = use_signal(starter_template);
    // The builder's working copy. Editing it touches no report until it is saved and a
    // report is pointed at it.
    let editing_template = use_signal(starter_template);

    // Owned here rather than inside a screen, so leaving the editor and coming back finds
    // the notes intact. Only the editor's focus guard resets, which is what leaving a text
    // field should do anyway.
    let workspace = Workspace {
        template: report_template,
        notes: use_signal(RichDoc::empty_paragraph),
        generated: use_signal(RichDoc::empty_paragraph),
        has_report: use_signal(|| false),
        report_id: use_signal(|| None),
        report_name: use_signal(|| "Untitled report".to_string()),
        written_at: use_signal(|| None),
        revision: use_signal(|| 0),
    };

    // Starts on open and resumes an interrupted download from where it stopped.
    let downloads = use_model_downloads(settings);
    let dictation = use_dictation(workspace.notes, settings);
    let save_state = use_autosave(workspace);
    let mut status = use_signal(|| Status::Idle);

    // The developer group in Settings reads a template through context rather than a prop,
    // so nothing on the path down to it has to carry a parameter that only exists in a
    // debug build. It shows "what the model is actually sent", so it is the *report's*
    // template it needs, not whatever the builder happens to have open.
    use_context_provider(|| report_template);

    // A callback rather than a closure so it stays `Copy` across the rsx tree.
    let run = use_callback(move |_: ()| {
        let template = workspace.template.read().clone();
        let notes_markdown = doc_to_markdown(&workspace.notes.read());
        let settings = settings.read().clone();
        let mut generated = workspace.generated;
        let mut has_report = workspace.has_report;
        let mut written_at = workspace.written_at;

        status.set(Status::Running);
        spawn(async move {
            match generate(template, notes_markdown, settings).await {
                Ok(document) => {
                    generated.set(document);
                    has_report.set(true);
                    written_at.set(Some(report_core::store::now()));
                    status.set(Status::Done);
                }
                // The whole chain, so the message names the cause rather than the
                // symptom — "Incorrect API key" instead of "request failed".
                Err(error) => status.set(Status::Failed(format!("{error:#}"))),
            }
        });
    });

    let theme = settings.read().appearance;

    rsx! {
        style { dangerous_inner_html: crate::ui::CSS }
        EditorRuntime {
            // `data-theme` lives here rather than on `<html>`, which rsx does not render.
            // Setting it through `document::eval` would queue behind the editor bridge's
            // own channel and paint one frame in the wrong theme first — a white flash on
            // every launch for anyone who chose Dark. See the token block in `app.css`.
            div { class: "shell", "data-theme": theme.attribute(),
                Rail { screen, settings }

                main { class: "main",
                    match screen() {
                        Screen::Reports => rsx! {
                            ReportsScreen {
                                workspace,
                                screen,
                                starter: starter_template(),
                            }
                        },
                        Screen::Editor => rsx! {
                            EditorScreen {
                                workspace,
                                dictation: dictation.clone(),
                                downloads,
                                status,
                                save_state,
                                on_generate: move |_| run.call(()),
                            }
                        },
                        Screen::Templates => rsx! {
                            TemplatesScreen { template: editing_template }
                        },
                        Screen::Settings => rsx! {
                            SettingsPanel { settings }
                        },
                    }
                }
            }
        }
    }
}
