//! The app's own UI: everything that knows what a template and a report are.

pub mod library;
pub mod settings_panel;
pub mod template_builder;

pub use library::{LibraryBar, Workspace};
pub use settings_panel::SettingsPanel;
pub use template_builder::TemplateBuilder;

/// App chrome and template-builder styling.
///
/// The editor's own stylesheet is injected separately by
/// `report_editor::EditorRuntime`; this only covers what the app draws around it.
pub const CSS: &str = include_str!("../assets/app.css");
