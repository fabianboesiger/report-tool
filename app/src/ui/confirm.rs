//! Asking before destroying something.
//!
//! Every delete in the app goes through here. There is no undo anywhere — a deleted report
//! is gone from the database, and a deleted template field takes its children with it — and
//! all three delete buttons are small, hover-revealed, and sit next to buttons that do
//! something harmless. One of them had already cost the author's own two templates before
//! this existed.
//!
//! The platform dialog rather than an in-app one: it is modal in the way an in-app panel is
//! not, it names the app, and `rfd` is already a dependency for the export and import
//! dialogs. The cost is that it is `async`, so callers spawn.

/// Ask, and report whether the user agreed.
///
/// `subject` is quoted in the message, so pass the thing's name rather than a description —
/// "March visit", not "the report". Naming what is about to go is most of the value: a
/// dialog that says "Delete this item?" answers a question nobody asked.
pub async fn destructive(action: &str, subject: &str, consequence: &str) -> bool {
    let result = rfd::AsyncMessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title(action)
        .set_description(format!("{action} “{subject}”? {consequence}"))
        .set_buttons(rfd::MessageButtons::OkCancel)
        .show()
        .await;
    result == rfd::MessageDialogResult::Ok
}

/// The sentence every one of these ends with.
///
/// A constant rather than repeated at each call site, so it cannot drift into three
/// slightly different promises about the same absence of undo.
pub const NO_UNDO: &str = "This cannot be undone.";
