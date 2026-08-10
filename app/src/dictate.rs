//! Dictation: hold the recorder open, then transcribe into the notes.
//!
//! The recorder deliberately does not live in a `Signal`. `cpal::Stream` is not
//! `Send`, and it has to stay on the thread that opened the device — the UI thread,
//! which is where the button is. An `Rc<RefCell<_>>` in a hook keeps it there.

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::Result;
use dioxus::prelude::*;
use report_core::settings::Settings;
use report_doc::{Block, RichDoc};

use crate::audio::Recorder;

#[derive(Debug, Clone, PartialEq)]
pub enum Dictation {
    Idle,
    Recording,
    /// Audio captured, waiting on the transcription worker.
    Transcribing,
    Failed(String),
}

impl Dictation {
    pub fn is_recording(&self) -> bool {
        matches!(self, Dictation::Recording)
    }

    pub fn message(&self) -> Option<&str> {
        match self {
            Dictation::Failed(error) => Some(error),
            _ => None,
        }
    }
}

/// Wires a record button to a notes document.
#[derive(Clone)]
pub struct DictationControl {
    recorder: Rc<RefCell<Option<Recorder>>>,
    pub state: Signal<Dictation>,
    notes: Signal<RichDoc>,
    settings: Signal<Settings>,
}

pub fn use_dictation(notes: Signal<RichDoc>, settings: Signal<Settings>) -> DictationControl {
    let recorder = use_hook(|| Rc::new(RefCell::new(None::<Recorder>)));
    let state = use_signal(|| Dictation::Idle);
    DictationControl { recorder, state, notes, settings }
}

impl DictationControl {
    /// Start or stop, whichever applies.
    pub fn toggle(&self) {
        if self.state.read().is_recording() {
            self.stop();
        } else {
            self.start();
        }
    }

    fn start(&self) {
        let mut state = self.state;
        match Recorder::start() {
            Ok(recorder) => {
                *self.recorder.borrow_mut() = Some(recorder);
                state.set(Dictation::Recording);
            }
            // The usual cause is a denied microphone permission, which the OS reports
            // as "no device" rather than as a refusal.
            Err(error) => state.set(Dictation::Failed(format!("{error:#}"))),
        }
    }

    fn stop(&self) {
        let mut state = self.state;
        let Some(recorder) = self.recorder.borrow_mut().take() else {
            state.set(Dictation::Idle);
            return;
        };

        let audio = match recorder.finish() {
            Ok(audio) => audio,
            Err(error) => {
                state.set(Dictation::Failed(format!("{error:#}")));
                return;
            }
        };

        let settings = self.settings.read().clone();
        let Some(model_path) = settings.dictation_model_path() else {
            state.set(Dictation::Failed(
                "the dictation model is not ready yet — it is still downloading, or set the \
                 path to a whisper ggml file in Settings"
                    .into(),
            ));
            return;
        };

        state.set(Dictation::Transcribing);
        let mut notes = self.notes;
        let language = settings.stt.language();

        spawn(async move {
            match transcribe(model_path, audio, language).await {
                Ok(text) if text.trim().is_empty() => {
                    state.set(Dictation::Failed("nothing was recognised in that recording".into()))
                }
                Ok(text) => {
                    append_transcript(&mut notes.write(), &text);
                    state.set(Dictation::Idle);
                }
                Err(error) => state.set(Dictation::Failed(format!("{error:#}"))),
            }
        });
    }
}

#[cfg(feature = "inference")]
async fn transcribe(
    model_path: std::path::PathBuf,
    audio: Vec<f32>,
    language: Option<String>,
) -> Result<String> {
    report_core::local::transcribe(model_path, &audio, language).await
}

/// Named explicitly rather than silently doing nothing, which would look like a
/// microphone that recorded but heard nothing.
#[cfg(not(feature = "inference"))]
async fn transcribe(
    _model_path: std::path::PathBuf,
    _audio: Vec<f32>,
    _language: Option<String>,
) -> Result<String> {
    anyhow::bail!("this build has no transcription engine (built without `inference`)")
}

/// Add the transcript to the end of the notes.
///
/// Appended as its own paragraph rather than merged into the last one: a dictated
/// passage is a separate thought, and merging would also mean rewriting a block the
/// user might be typing in — which the editor's focus guard exists to prevent.
pub fn append_transcript(document: &mut RichDoc, text: &str) {
    // Drop a trailing empty paragraph so dictating into a fresh document does not
    // leave a blank line above the transcript.
    if document.blocks.last().is_some_and(|block| block.is_empty()) {
        document.blocks.pop();
    }
    for paragraph in text.split("\n\n") {
        let paragraph = paragraph.trim();
        if !paragraph.is_empty() {
            document.blocks.push(Block::paragraph(paragraph));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use report_doc::markdown::to_markdown;

    #[test]
    fn a_transcript_is_appended_after_existing_notes() {
        let mut document = RichDoc::from_blocks(vec![Block::paragraph("typed note")]);
        append_transcript(&mut document, "first thought\n\nsecond thought");
        assert_eq!(
            document.blocks.iter().map(Block::text).collect::<Vec<_>>(),
            ["typed note", "first thought", "second thought"]
        );
    }

    #[test]
    fn dictating_into_an_empty_document_leaves_no_blank_line_above_it() {
        // A fresh notes pane holds one empty paragraph; appending after it would put
        // the transcript on the second line with a gap above.
        let mut document = RichDoc::empty_paragraph();
        append_transcript(&mut document, "the north wall is cracked");
        assert_eq!(document.blocks.len(), 1);
        assert_eq!(document.blocks[0].text(), "the north wall is cracked");
    }

    #[test]
    fn blank_stretches_in_a_transcript_do_not_become_empty_blocks() {
        let mut document = RichDoc::from_blocks(Vec::new());
        append_transcript(&mut document, "  \n\none thing\n\n   \n\nanother  ");
        assert_eq!(
            document.blocks.iter().map(Block::text).collect::<Vec<_>>(),
            ["one thing", "another"]
        );
    }

    #[test]
    fn the_markdown_the_model_receives_contains_the_transcript() {
        let mut document = RichDoc::empty_paragraph();
        append_transcript(&mut document, "roof tiles slipped");
        assert!(to_markdown(&document).contains("roof tiles slipped"));
    }
}
