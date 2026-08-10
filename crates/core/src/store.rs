//! Templates and reports on disk.
//!
//! Plain JSON files, one per item, under the platform data directory — not a
//! database. At this scale a database buys nothing and costs the property that makes
//! these files useful: a template is a single readable file a user can diff, put in
//! version control, or send to a colleague.
//!
//! ## Every report keeps a copy of its template
//!
//! [`Report::template`] is a *snapshot*, not a reference. Templates are meant to be
//! edited — that is the whole point of the builder — and a report renders by walking
//! its template alongside the generated value. If reports pointed at a shared
//! template, renaming a field or deleting a section would silently break every report
//! ever made from it. Copying a few kilobytes per report removes that class of bug
//! entirely.

use std::path::Path;

use anyhow::{Context, Result};
use report_doc::RichDoc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::template::Template;

/// A report: the notes, what was generated from them, and the template used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,
    pub name: String,
    /// A snapshot — see the module docs.
    pub template: Template,
    #[serde(default)]
    pub notes: RichDoc,
    /// `None` until the report has been generated at least once.
    #[serde(default)]
    pub generated: Option<RichDoc>,
    #[serde(default)]
    pub created: u64,
    #[serde(default)]
    pub updated: u64,
}

impl Report {
    pub fn new(name: impl Into<String>, template: Template) -> Self {
        let now = now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            template,
            notes: RichDoc::empty_paragraph(),
            generated: None,
            created: now,
            updated: now,
        }
    }
}

/// Enough to list something without loading it.
///
/// Deliberately more than an id and a name. The library screen shows which template a
/// report came from and whether it has been written yet, and finding that out any other
/// way would mean deserialising every report's notes and prose in order to draw one
/// list. [`list`] already parses each file into a `Value`; these are two more field
/// reads out of a value that is in hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub id: Uuid,
    pub name: String,
    /// Seconds since the epoch. A report's own stamp; a template's file mtime.
    pub updated: u64,
    /// The name of the template *snapshot* the report holds.
    ///
    /// Read from the snapshot rather than by following an id, because the snapshot is
    /// what the report was actually written against — the template it came from may
    /// since have been renamed or deleted, and the list must not go blank when it is.
    ///
    /// Empty for a template, which is its own source.
    pub template_name: String,
    /// Whether the report has been generated at least once, shown as Draft or Final.
    /// Always false for a template.
    pub generated: bool,
}

pub fn save_template(template: &Template) -> Result<()> {
    write_json(&crate::paths::templates_dir()?.join(format!("{}.json", template.id)), template)
}

pub fn load_template(id: Uuid) -> Result<Template> {
    read_json(&crate::paths::templates_dir()?.join(format!("{id}.json")))
}

pub fn delete_template(id: Uuid) -> Result<()> {
    remove(&crate::paths::templates_dir()?.join(format!("{id}.json")))
}

pub fn list_templates() -> Result<Vec<Summary>> {
    let mut templates = list(&crate::paths::templates_dir()?, |value, modified| {
        Some(Summary {
            id: value.get("id")?.as_str()?.parse().ok()?,
            name: value.get("name")?.as_str()?.to_string(),
            // Templates carry no timestamp of their own, so the file's mtime orders
            // them. It used to be hardcoded to zero under a comment saying this, which
            // left the list in whatever order the directory happened to yield.
            updated: modified,
            // A template is its own source, and has nothing to be a draft of.
            template_name: String::new(),
            generated: false,
        })
    })?;
    templates.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| a.name.cmp(&b.name)));
    Ok(templates)
}

pub fn save_report(report: &Report) -> Result<()> {
    let mut report = report.clone();
    report.updated = now();
    write_json(&crate::paths::reports_dir()?.join(format!("{}.json", report.id)), &report)
}

pub fn load_report(id: Uuid) -> Result<Report> {
    read_json(&crate::paths::reports_dir()?.join(format!("{id}.json")))
}

pub fn delete_report(id: Uuid) -> Result<()> {
    remove(&crate::paths::reports_dir()?.join(format!("{id}.json")))
}

pub fn list_reports() -> Result<Vec<Summary>> {
    let mut reports = list(&crate::paths::reports_dir()?, |value, _modified| {
        Some(Summary {
            id: value.get("id")?.as_str()?.parse().ok()?,
            name: value.get("name")?.as_str()?.to_string(),
            // The report's own stamp, not the file's: `save_report` sets it, and it is
            // what the sort below and the row's relative time both mean by "updated".
            updated: value.get("updated").and_then(serde_json::Value::as_u64).unwrap_or(0),
            template_name: value
                .get("template")
                .and_then(|template| template.get("name"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            // `null` until the report has been written once, which is exactly the
            // Draft/Final distinction the library row draws.
            generated: value.get("generated").is_some_and(|value| !value.is_null()),
        })
    })?;
    // Most recently worked on first: the one a user wants next is almost always the
    // one they had open last.
    reports.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| a.name.cmp(&b.name)));
    Ok(reports)
}

/// Seconds since the epoch.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A timestamp as a person would say it: "2 hours ago", "Yesterday", "4 Aug".
///
/// Here rather than in the app for the same reason [`crate::download::human_bytes`] is:
/// it formats a value this module produces, and a pure function is worth more in the
/// fast test job than beside the component that happens to draw it.
///
/// Switches from elapsed time to a date at two days, which is where "47 hours ago" stops
/// being easier to read than the date itself.
pub fn relative_time(when: u64) -> String {
    relative_to(when, now())
}

/// The same, against an explicit clock.
///
/// Split out purely so it can be tested: a function that reads the wall clock can be
/// checked for shape and never for a value, and every interesting case here is about
/// exactly where a boundary falls.
pub fn relative_to(when: u64, now: u64) -> String {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;

    // Saturating: a report copied from a machine whose clock runs ahead would otherwise
    // wrap and the row would read "584942417355 years ago".
    match now.saturating_sub(when) {
        elapsed if elapsed < MINUTE => "Just now".to_string(),
        elapsed if elapsed < HOUR => plural(elapsed / MINUTE, "minute"),
        elapsed if elapsed < DAY => plural(elapsed / HOUR, "hour"),
        elapsed if elapsed < 2 * DAY => "Yesterday".to_string(),
        _ => short_date(when),
    }
}

fn plural(count: u64, unit: &str) -> String {
    if count == 1 {
        format!("1 {unit} ago")
    } else {
        format!("{count} {unit}s ago")
    }
}

/// "4 Aug", from a unix timestamp, without a date crate.
///
/// Hinnant's `civil_from_days`: shifting the epoch to 1 March puts a leap day at the end
/// of the year, after which every month has a closed-form length. A dozen lines, exact
/// for every date this app will ever hold, and no dependency.
///
/// **UTC, not local time.** A report saved at 23:30 in Zurich therefore shows the
/// following day's date once it is more than two days old. Fixing that needs a timezone
/// database — the `time` crate, or `libc::localtime_r` — and this is a six-character
/// label on a row that also carries a name; it is not worth a dependency, and it is
/// worth saying out loud rather than leaving as a puzzle.
fn short_date(when: u64) -> String {
    const MONTHS: [&str; 12] =
        ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

    let days = (when / 86_400) as i64;
    // Shift the era to start on 1 March 0000, so February — and its leap day — is the
    // last month of the year rather than the second.
    let shifted = days + 719_468;
    let era = if shifted >= 0 { shifted } else { shifted - 146_096 } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as usize;
    // Undo the March shift: months 0..=9 are March..December, 10 and 11 are January and
    // February of the following year.
    let month = if shifted_month < 10 { shifted_month + 3 } else { shifted_month - 9 } as usize;

    format!("{day} {}", MONTHS[month - 1])
}

// ---------------------------------------------------------------------------

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let text = serde_json::to_string_pretty(value).context("serialising")?;
    // Through a temporary file and a rename, which is atomic on every platform we
    // ship to. A crash midway through a direct write would leave a truncated file,
    // and the report it held would be unopenable rather than merely out of date.
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, text).with_context(|| format!("writing {}", temporary.display()))?;
    std::fs::rename(&temporary, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

fn remove(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        // Deleting something already gone is the outcome the caller wanted.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("deleting {}", path.display())),
    }
}

/// Read every `*.json` in `dir` and summarise it.
///
/// A file that will not parse is logged and skipped rather than failing the listing:
/// one corrupt report must not make the library unopenable.
///
/// The summariser is handed the file's modification time alongside its contents, because
/// a template carries no timestamp of its own and the mtime is the only thing that can
/// order one.
fn list(
    dir: &Path,
    summarise: impl Fn(&serde_json::Value, u64) -> Option<Summary>,
) -> Result<Vec<Summary>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).with_context(|| format!("listing {}", dir.display())),
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        // A filesystem that cannot report a modification time yields zero, which sorts
        // last rather than failing the listing over a cosmetic column.
        let modified = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|since| since.as_secs())
            .unwrap_or(0);

        match std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .as_ref()
            .and_then(|value| summarise(value, modified))
        {
            Some(summary) => out.push(summary),
            None => tracing::warn!("store: skipping unreadable {}", path.display()),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::fixture;

    /// The store's IO is thin; what is worth testing is that the shapes survive a
    /// round trip through JSON, including the fields added later.
    #[test]
    fn a_report_round_trips_through_json() {
        let mut report = Report::new("March visit", fixture::template());
        report.notes = report_doc::markdown::from_markdown("north wall cracked");
        report.generated = Some(report_doc::markdown::from_markdown("# Findings\n\nAll noted."));

        let text = serde_json::to_string(&report).unwrap();
        let back: Report = serde_json::from_str(&text).unwrap();
        assert_eq!(back.name, report.name);
        assert_eq!(back.template, report.template);
        assert_eq!(back.generated, report.generated);
        assert_eq!(back.id, report.id);
    }

    #[test]
    fn a_report_carries_its_own_copy_of_the_template() {
        // The property the module exists to guarantee: editing the template a report
        // was made from must not reach back into the report.
        let mut template = fixture::template();
        let report = Report::new("visit", template.clone());

        template.remove(template.nodes[1].id);
        template.set_label(template.nodes[0].id, "Renamed".into());

        assert_eq!(report.template.nodes.len(), 3, "the snapshot is untouched");
        assert_eq!(report.template.nodes[0].label, "Summary");
        // And it still renders, which is what would break if it were a reference.
        let shape = crate::compile::Shape::compile(&report.template);
        assert!(shape.to_json_schema()["properties"].get("findings").is_some());
    }

    #[test]
    fn a_report_written_by_an_older_version_still_loads() {
        // Only the fields that must exist are required; everything else defaults, so
        // a file from before `generated` existed opens rather than erroring.
        let template = serde_json::to_string(&fixture::template()).unwrap();
        let text = format!(r#"{{"name":"old","template":{template}}}"#);
        let report: Report = serde_json::from_str(&text).unwrap();
        assert_eq!(report.name, "old");
        assert!(report.generated.is_none());
        assert!(!report.id.is_nil(), "a missing id must be generated, not left nil");
    }

    /// Exercises the real filesystem through `REPORT_DATA_DIR`.
    ///
    /// Serialisation is covered above; what this adds is the part that only fails on
    /// disk — the temp-file rename, listing a directory that does not exist yet, and
    /// a corrupt file among good ones.
    #[test]
    fn the_store_round_trips_through_real_files() {
        // Holds the shared lock and cleans up on drop; see `crate::testenv`.
        let _dir = crate::testenv::data_dir("store");

        // Listing a library that has never been written must be empty, not an error.
        assert!(list_reports().unwrap().is_empty());
        assert!(list_templates().unwrap().is_empty());

        let template = fixture::template();
        save_template(&template).unwrap();
        assert_eq!(load_template(template.id).unwrap(), template);
        let listed = list_templates().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Site Inspection");

        let mut report = Report::new("March visit", template.clone());
        report.notes = report_doc::markdown::from_markdown("north wall cracked");
        save_report(&report).unwrap();

        let loaded = load_report(report.id).unwrap();
        assert_eq!(loaded.name, "March visit");
        assert_eq!(loaded.notes, report.notes);
        assert!(loaded.updated > 0, "saving must stamp the time");

        // The library row's two columns, read out of the file rather than by loading it.
        let listed = list_reports().unwrap();
        assert_eq!(listed[0].template_name, "Site Inspection");
        assert!(!listed[0].generated, "a report with no prose yet is a draft");

        // A template's summary has a real mtime now, not the zero it used to carry.
        assert!(list_templates().unwrap()[0].updated > 0, "the file's mtime must order it");

        // Saving again must overwrite rather than leave two entries.
        save_report(&loaded).unwrap();
        assert_eq!(list_reports().unwrap().len(), 1);

        // Once written, the row must flip from Draft to Final.
        let mut written = loaded.clone();
        written.generated = Some(report_doc::markdown::from_markdown("# Findings\n\nAll noted."));
        save_report(&written).unwrap();
        assert!(list_reports().unwrap()[0].generated);

        // One unreadable file must not make the whole library unopenable.
        std::fs::write(crate::paths::reports_dir().unwrap().join("broken.json"), "{ not json")
            .unwrap();
        assert_eq!(list_reports().unwrap().len(), 1, "the good report is still listed");

        // No leftover temporaries from the atomic writes.
        let leftovers: Vec<_> = std::fs::read_dir(crate::paths::reports_dir().unwrap())
            .unwrap()
            .flatten()
            .filter(|e| e.path().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "a temp file survived the rename");

        delete_report(report.id).unwrap();
        assert!(load_report(report.id).is_err());
        // Deleting twice is the caller getting what they asked for.
        delete_report(report.id).unwrap();
    }

    #[test]
    fn reports_are_listed_most_recently_worked_on_first() {
        let summary = |name: &str, updated| Summary {
            id: Uuid::new_v4(),
            name: name.into(),
            updated,
            template_name: String::new(),
            generated: false,
        };
        let mut summaries = [summary("old", 100), summary("newest", 300), summary("middle", 200)];
        summaries.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| a.name.cmp(&b.name)));
        assert_eq!(
            summaries.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            ["newest", "middle", "old"]
        );
    }

    #[test]
    fn a_timestamp_reads_the_way_a_person_would_say_it() {
        const HOUR: u64 = 3600;
        const DAY: u64 = 24 * HOUR;
        // A fixed clock, so this cannot start failing next Tuesday.
        let now = 1_754_308_800; // 2025-08-04 12:00 UTC

        assert_eq!(relative_to(now, now), "Just now");
        assert_eq!(relative_to(now - 90, now), "1 minute ago");
        assert_eq!(relative_to(now - 45 * 60, now), "45 minutes ago");
        assert_eq!(relative_to(now - 2 * HOUR, now), "2 hours ago");
        // "1 hours ago" is the sort of thing nobody notices until a screenshot.
        assert_eq!(relative_to(now - HOUR - 1, now), "1 hour ago");
        // The boundary: still yesterday at 30 hours, a date by 50.
        assert_eq!(relative_to(now - 30 * HOUR, now), "Yesterday");
        assert_eq!(relative_to(now - 50 * HOUR, now), "2 Aug");
        assert_eq!(relative_to(now - 14 * DAY, now), "21 Jul");
    }

    #[test]
    fn a_clock_that_runs_ahead_does_not_underflow() {
        // A report copied from a machine ahead of this one. Unsaturated, `now - when`
        // wraps and the row reads "584942417355 years ago".
        let now = 1_754_308_800;
        assert_eq!(relative_to(now + 3600, now), "Just now");
    }

    #[test]
    fn the_short_date_survives_a_leap_day_and_the_epoch() {
        // The whole reason for the March-shifted era: 2024 is a leap year, and 29
        // February is the day a naive month-length table gets wrong.
        assert_eq!(short_date(1_709_164_800), "29 Feb"); // 2024-02-29 00:00 UTC
        assert_eq!(short_date(1_709_251_200), "1 Mar"); // the day after
        assert_eq!(short_date(0), "1 Jan"); // 1970-01-01
        assert_eq!(short_date(951_782_400), "29 Feb"); // 2000, a century leap year
        assert_eq!(short_date(1_078_012_800), "29 Feb"); // 2004
    }
}
