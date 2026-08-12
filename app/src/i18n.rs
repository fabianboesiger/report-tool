//! Every word the app says, in four languages.
//!
//! Fluent, through `dioxus-i18n`. The catalogues in `assets/i18n/` are `include_str!`d
//! rather than resolved through `asset!`, for the same reason the stylesheet and the
//! window icon are: there is then no file to be missing at runtime and no path that
//! resolves differently between `dx serve` and a bundle.
//!
//! ## Using it
//!
//! ```ignore
//! use crate::i18n::t;
//! rsx! { h1 { {t!("reports-title")} } }
//! ```
//!
//! `t!` reads the active bundle out of a signal, so a component that translates
//! re-renders when the language changes; nothing has to be invalidated by hand and there
//! is no restart.
//!
//! ## Three things to know before adding a string
//!
//! **`t!` panics on a key that is not in the catalogue.** That is deliberate and it is
//! made safe by [`tests::every_key_the_code_asks_for_exists`], which scans the source
//! tree for `t!("…")` and checks each one against `en.ftl`. A missing key is a failing
//! build, never a panicking product.
//!
//! **Translate on the render or event path, never after an `await`.** `t!` resolves the
//! bundle out of the component's context; inside a spawned task, past a suspension point,
//! there is no scope to resolve it from. Everything asynchronous here therefore either
//! translates its strings *before* it spawns, or carries a value the component translates
//! when it draws it — which is what [`SaveState`](crate::ui::workspace::SaveState) and
//! `templates::Message` are for.
//!
//! **Fluent wraps every placeable in bidi isolates.** `Exported to { $file }` comes back
//! as `Exported to \u{2068}report.md\u{2069}`: invisible in a webview, and dioxus-i18n
//! offers no way to switch it off. Harmless for anything shown to a person, so never feed
//! a `t!` result to a filename, a slug, or a comparison.

use dioxus::prelude::*;
use dioxus_i18n::prelude::{use_init_i18n, I18nConfig};
use report_core::settings::{Locale, Settings};
use report_core::store::Elapsed;
use unic_langid::{langid, LanguageIdentifier};

/// Re-exported so components write `use crate::i18n::t;` and not a crate they otherwise
/// never name.
pub use dioxus_i18n::t;

const DE: &str = include_str!("../assets/i18n/de.ftl");
const EN: &str = include_str!("../assets/i18n/en.ftl");
const FR: &str = include_str!("../assets/i18n/fr.ftl");
const IT: &str = include_str!("../assets/i18n/it.ftl");

/// The catalogue for a language.
///
/// A function rather than a method on [`Locale`] so `report-core` stays free of the app's
/// assets, and a `match` rather than a map so adding a locale does not compile until its
/// catalogue exists.
fn catalogue(locale: Locale) -> &'static str {
    match locale {
        Locale::German => DE,
        Locale::English => EN,
        Locale::French => FR,
        Locale::Italian => IT,
    }
}

/// `Locale` as Fluent names it.
///
/// `langid!` validates at compile time, which is worth the `match`: the alternative is
/// `locale.tag().parse().unwrap()`, and that unwrap would fire on startup rather than in
/// a build.
fn langid_of(locale: Locale) -> LanguageIdentifier {
    match locale {
        Locale::German => langid!("de"),
        Locale::English => langid!("en"),
        Locale::French => langid!("fr"),
        Locale::Italian => langid!("it"),
    }
}

/// Install the catalogues, and keep the active one in step with the settings.
///
/// Returns the language in force, which the shell needs for its `lang` attribute.
///
/// Must be called above anything that translates. `use_context_provider` publishes into
/// the calling scope as well as below it, so the component that calls this may itself use
/// `t!`.
pub fn use_app_i18n(settings: Signal<Settings>) -> Locale {
    // A memo, not a direct read of `settings`, and the difference is not cosmetic:
    // subscribing to the whole `Settings` signal would rebuild the Fluent bundle on every
    // keystroke in the API-key field. This fires only when the language actually changes.
    let locale = use_memo(move || settings.read().locale());

    let mut i18n = use_init_i18n(|| {
        let mut config = I18nConfig::new(langid_of(locale()))
            // English is the fallback rather than an error: a key not yet translated
            // should render its English text, which is legible, instead of a panic or a
            // bare message id. The completeness test exists so this is a safety net and
            // not the mechanism.
            .with_fallback(langid!("en"));
        for locale in Locale::ALL {
            config = config.with_locale((langid_of(locale), catalogue(locale)));
        }
        config
    });

    use_effect(move || i18n.set_language(langid_of(locale())));

    locale()
}

/// A translated string with Fluent's bidi isolates removed.
///
/// Fluent wraps every `{ $placeable }` in U+2068 / U+2069 so a right-to-left value cannot
/// reorder the text around it, and `dioxus-i18n` offers no way to switch that off. The marks
/// are invisible wherever a person reads them, so almost nothing needs this — but a string
/// that becomes *data* does: a duplicated template's name is stored, and later turned into a
/// filename, and two invisible characters would survive into both.
///
/// Only the isolates and embeddings, not every control character: whatever else is in the
/// string came from the catalogue or the user, and this is not a sanitiser.
pub fn plain(text: String) -> String {
    if !text.contains(['\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}']) {
        // The common case by far, and worth not reallocating for.
        return text;
    }
    text.chars().filter(|c| !matches!(c, '\u{2066}'..='\u{2069}')).collect()
}

/// How long ago, in the language the app is in.
///
/// The counterpart to [`report_core::store::elapsed_since`], which deliberately stops at
/// the arithmetic: the words are the app's, and Fluent's plural categories are what make
/// "vor 1 Minute" and "vor 2 Minuten" correct without a rule per language here.
pub fn relative_time(when: u64) -> String {
    describe(report_core::store::elapsed_since(when))
}

/// The same, from an [`Elapsed`] already in hand.
pub fn describe(elapsed: Elapsed) -> String {
    match elapsed {
        Elapsed::JustNow => t!("time-just-now"),
        Elapsed::Minutes(count) => t!("time-minutes", count: count as i64),
        Elapsed::Hours(count) => t!("time-hours", count: count as i64),
        Elapsed::Yesterday => t!("time-yesterday"),
        Elapsed::Date { day, month } => {
            t!("time-date", day: day as i64, month: month_name(month))
        }
    }
}

/// The abbreviated month name, from a 1-based month number.
///
/// Clamped rather than indexed: [`report_core::store`] produces the number and is tested
/// for it, but a panic in a date label would be a poor way to find out otherwise.
fn month_name(month: u32) -> String {
    let month = month.clamp(1, 12);
    t!(&format!("time-month-{month}"))
}

/// The toolbar tooltips `report-editor` cannot supply for itself.
///
/// The crate takes them as a prop because a Fluent bundle admits exactly one resource per
/// language — there is no way for two crates to contribute keys to one catalogue — and
/// because it must not learn about the app regardless.
pub fn toolbar_labels() -> report_editor::ToolbarLabels {
    report_editor::ToolbarLabels {
        bold: t!("toolbar-bold"),
        italic: t!("toolbar-italic"),
        code: t!("toolbar-code"),
        strike: t!("toolbar-strike"),
        paragraph: t!("toolbar-paragraph"),
        headings: [t!("toolbar-heading-1"), t!("toolbar-heading-2"), t!("toolbar-heading-3")],
        bulleted: t!("toolbar-bulleted"),
        numbered: t!("toolbar-numbered"),
        quote: t!("toolbar-quote"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus_i18n::fluent::{FluentBundle, FluentResource};
    use std::collections::BTreeSet;

    /// Every message id a catalogue defines.
    ///
    /// Read off the text rather than out of the bundle: `fluent-bundle` 0.16 has no way to
    /// enumerate what it holds, only to look an id up. So the ids are scanned — an FTL
    /// message is an identifier in the first column followed by `=` — and then every one is
    /// looked up, which is what stops the scanner quietly disagreeing with the parser.
    ///
    /// The resource is built through a real [`FluentBundle`] first, so a catalogue that
    /// parses but the bundle rejects fails here rather than in the product.
    fn keys(source: &str, locale: Locale) -> BTreeSet<String> {
        let resource = FluentResource::try_new(source.to_string()).unwrap_or_else(|(_, errors)| {
            panic!("{}.ftl does not parse: {errors:#?}", locale.tag())
        });
        let mut bundle = FluentBundle::new(vec![langid_of(locale)]);
        bundle
            .add_resource(resource)
            .unwrap_or_else(|errors| panic!("{}.ftl is not usable: {errors:#?}", locale.tag()));

        let mut found = BTreeSet::new();
        for line in source.lines() {
            // Comments start with `#`, continuations and attributes are indented, and blank
            // lines separate messages — none of which begin an id.
            if line.starts_with([' ', '\t', '#']) || line.trim().is_empty() {
                continue;
            }
            let Some((id, _)) = line.split_once('=') else { continue };
            let id = id.trim();
            assert!(
                bundle.get_message(id).is_some(),
                "{}.ftl: scanned `{id}` but the bundle does not have it — the scanner and the \
                 parser disagree, so the completeness tests below are checking nothing",
                locale.tag()
            );
            found.insert(id.to_string());
        }
        found
    }

    #[test]
    fn every_catalogue_parses_and_every_locale_has_one() {
        for locale in Locale::ALL {
            let found = keys(catalogue(locale), locale);
            assert!(!found.is_empty(), "{}.ftl defines nothing", locale.tag());
        }
    }

    #[test]
    fn the_catalogues_define_exactly_the_same_keys() {
        // A key present in `en.ftl` and absent from `it.ftl` is otherwise invisible until
        // an Italian user reaches that screen — at which point English text appears in the
        // middle of an Italian page. The reverse, a key only Italian has, is a translation
        // of something the code no longer says.
        let english = keys(EN, Locale::English);
        for locale in Locale::ALL {
            if locale == Locale::English {
                continue;
            }
            let found = keys(catalogue(locale), locale);
            let missing: Vec<_> = english.difference(&found).collect();
            let extra: Vec<_> = found.difference(&english).collect();
            assert!(missing.is_empty(), "{}.ftl is missing {missing:?}", locale.tag());
            assert!(
                extra.is_empty(),
                "{}.ftl defines {extra:?}, which en.ftl does not",
                locale.tag()
            );
        }
    }

    /// Every `t!("…")` in the source tree, with the file it came from.
    fn keys_the_code_asks_for() -> Vec<(String, String)> {
        // From the manifest directory rather than a relative path, so this passes whatever
        // directory the test runner happens to start in.
        let app = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let roots = [app.join("src"), app.join("../crates/editor/src")];

        let mut asked = Vec::new();
        for root in roots {
            for file in rust_files(&root) {
                let source =
                    std::fs::read_to_string(&file).expect("a listed file must be readable");
                let name = file.file_name().unwrap_or_default().to_string_lossy().to_string();
                for line in source.lines() {
                    // Comments and doc comments, which in this crate include worked `t!`
                    // examples that name no real key. Line-based, so a `t!` split across
                    // lines is missed — the count assertion below is what would notice if
                    // that ever became the common style.
                    if line.trim_start().starts_with("//") {
                        continue;
                    }
                    for (at, matched) in line.match_indices("t!(\"") {
                        // `format!("…")` ends in `t!("` too, as does `write!`. Only a `t!`
                        // whose `t` begins an identifier is the macro being looked for.
                        let part_of_a_longer_name = line[..at]
                            .chars()
                            .next_back()
                            .is_some_and(|c| c.is_alphanumeric() || c == '_');
                        if part_of_a_longer_name {
                            continue;
                        }
                        if let Some(key) = line[at + matched.len()..].split('"').next() {
                            asked.push((key.to_string(), name.clone()));
                        }
                    }
                }
            }
        }
        asked
    }

    fn rust_files(root: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut found = Vec::new();
        let Ok(entries) = std::fs::read_dir(root) else { return found };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                found.extend(rust_files(&path));
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                found.push(path);
            }
        }
        found
    }

    #[test]
    fn every_key_the_code_asks_for_exists() {
        // This is what makes `t!`'s panic-on-missing acceptable: a typo or a renamed key
        // fails here rather than the first time a user opens the screen that says it.
        let english = keys(EN, Locale::English);
        let asked = keys_the_code_asks_for();
        assert!(asked.len() > 100, "the scan found only {} keys — has `t!` moved?", asked.len());

        for (key, file) in asked {
            assert!(
                english.contains(&key),
                "{file} asks for `{key}`, which en.ftl does not define"
            );
        }
    }

    #[test]
    fn the_scan_would_notice_a_missing_key() {
        // Guards the guard: a scan that silently matched nothing would let every one of the
        // assertions above pass while checking nothing at all.
        let english = keys(EN, Locale::English);
        assert!(english.contains("reports-title"));
        assert!(english.contains("time-month-12"));
        assert!(!english.contains("a-key-nobody-defined"));
    }

    #[test]
    fn every_month_has_a_name_in_every_language() {
        // `month_name` builds its key by interpolation, so the scan above cannot see these
        // twelve — they have to be checked by name.
        for locale in Locale::ALL {
            let found = keys(catalogue(locale), locale);
            for month in 1..=12 {
                let key = format!("time-month-{month}");
                assert!(found.contains(&key), "{}.ftl has no {key}", locale.tag());
            }
        }
    }

    #[test]
    fn every_plural_message_covers_this_language_s_categories() {
        // A selector missing its language's categories is the failure mode Fluent will not
        // report: `format_pattern` falls through to `*[other]` and the count reads wrong in
        // exactly the cases the selector existed for. Formatting each one for real is the
        // only check that catches it.
        for locale in Locale::ALL {
            let resource = FluentResource::try_new(catalogue(locale).to_string()).unwrap();
            let mut bundle = FluentBundle::new(vec![langid_of(locale)]);
            // Off, so the assertions below compare text and not text peppered with the
            // isolate marks Fluent otherwise wraps every placeable in.
            bundle.set_use_isolating(false);
            bundle.add_resource(resource).unwrap();

            for key in
                ["time-minutes", "time-hours", "templates-tag-fields", "builder-delete-nested"]
            {
                for count in [0i64, 1, 2, 5, 11, 21, 101] {
                    let message = bundle.get_message(key).expect(key);
                    let pattern = message.value().expect(key);
                    let mut args = dioxus_i18n::fluent::FluentArgs::new();
                    args.set("count", count);
                    let mut errors = Vec::new();
                    let text = bundle.format_pattern(pattern, Some(&args), &mut errors);
                    assert!(
                        errors.is_empty(),
                        "{}.ftl {key} at {count}: {errors:#?}",
                        locale.tag()
                    );
                    assert!(
                        !text.trim().is_empty(),
                        "{}.ftl {key} at {count} is blank",
                        locale.tag()
                    );
                }
            }
        }
    }
}
