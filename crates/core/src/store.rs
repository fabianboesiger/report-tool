//! Templates and reports, in the database.
//!
//! This module used to open by arguing for one JSON file per item — inspectable,
//! diffable, emailable. Two things turned that around, both measured in the code that
//! replaced it: drawing the library list read every byte of every report to pick five
//! fields out of it, and autosave rewrote both documents plus a template snapshot every
//! two seconds. See [`crate::db`] for the detail.
//!
//! What the files were good for is not lost, only made explicit:
//! [`export_template`] and [`import_template`] move a template in and out as `.json`.
//!
//! ## Every report keeps a copy of its template
//!
//! [`Report::template`] is a *snapshot*, not a foreign key, and that is why there is no
//! relation between the two tables. Templates are meant to be edited — that is the whole
//! point of the builder — and a report renders by walking its template alongside the
//! generated value. If reports referenced a shared row, renaming a field or deleting a
//! section would silently break every report ever made from it.
//!
//! The report row also carries `template_name`, denormalised out of that snapshot at
//! write time. It is what lets the list query be covered by an index instead of reaching
//! into JSON.

use anyhow::{Context, Result};
use report_doc::RichDoc;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db;
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
/// Deliberately more than an id and a name: the library screen shows which template a
/// report came from and whether it has been written yet. Every field here is a column,
/// so building one costs no document text — which is the difference between this and
/// the file-per-report layout it replaced, where the same list parsed every report in
/// full.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub id: Uuid,
    pub name: String,
    /// Seconds since the epoch. A real column for both, since storage moved to SQLite —
    /// a template's used to be its file's mtime, which any copy or sync rewrote.
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
    /// How many top-level fields a template has. Always zero for a report.
    ///
    /// Here because a template with no fields produces nothing, and several of them
    /// carry the same default name — so without this the library shows a column of
    /// identical `Untitled template` rows with no way to tell which one has work in it.
    pub fields: usize,
}

pub fn save_template(template: &Template) -> Result<()> {
    db::insert_template(&db::open()?, template, now())
}

pub fn load_template(id: Uuid) -> Result<Template> {
    let connection = db::open()?;
    let body: Option<String> = connection
        .query_row("SELECT body FROM templates WHERE id = ?1", [id.to_string()], |row| row.get(0))
        .optional()
        .context("reading the template")?;
    let body = body.ok_or_else(|| anyhow::anyhow!("no template with id {id}"))?;
    serde_json::from_str(&body).context("parsing the stored template")
}

/// Delete a template. Deleting one that is already gone is the outcome the caller
/// wanted, so it is not an error.
pub fn delete_template(id: Uuid) -> Result<()> {
    db::open()?
        .execute("DELETE FROM templates WHERE id = ?1", [id.to_string()])
        .context("deleting the template")?;
    Ok(())
}

/// Templates, most recently touched first.
///
/// `updated` is now a real column. It used to be the file's mtime, because a template
/// file carried no timestamp at all — a property of the filesystem rather than of the
/// data, and one that any copy or sync would silently rewrite.
pub fn list_templates() -> Result<Vec<Summary>> {
    let connection = db::open()?;
    let mut statement = connection
        .prepare(
            // `json_array_length` does read the body, which the report listing is
            // careful never to do. Acceptable here and not there: a template is a few
            // kilobytes of structure with no notes and no prose in it, while a report
            // body is the notes, the generated document *and* a template snapshot.
            "SELECT id, name, updated, \
                    coalesce(json_array_length(body, '$.nodes'), 0) \
             FROM templates ORDER BY updated DESC, name ASC",
        )
        .context("preparing the template list")?;

    let rows = statement
        .query_map([], |row| {
            let id: String = row.get(0)?;
            Ok(Summary {
                // A row whose id will not parse is not worth failing a whole listing
                // over; nil sorts harmlessly and the name still shows.
                id: id.parse().unwrap_or(Uuid::nil()),
                name: row.get(1)?,
                // Stored as `i64` because SQLite has no unsigned 64-bit integer; these
                // are epoch seconds, so the round trip is lossless.
                updated: row.get::<_, i64>(2)? as u64,
                // A template is its own source, and has nothing to be a draft of.
                template_name: String::new(),
                generated: false,
                fields: row.get::<_, i64>(3)? as usize,
            })
        })
        .context("listing templates")?;

    rows.collect::<rusqlite::Result<Vec<_>>>().context("reading the template list")
}

pub fn save_report(report: &Report) -> Result<()> {
    let mut report = report.clone();
    report.updated = now();
    // Autosave calls this every couple of seconds, so `created` must survive: the
    // upsert keeps the stored value rather than taking the one in hand, which for a
    // report assembled fresh by `Workspace::save_report` is simply "now".
    db::insert_report(&db::open()?, &report)
}

pub fn load_report(id: Uuid) -> Result<Report> {
    let connection = db::open()?;
    let row = connection
        .query_row(
            "SELECT name, template, notes, generated, created, updated
             FROM reports WHERE id = ?1",
            [id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            },
        )
        .optional()
        .context("reading the report")?;

    let (name, template, notes, generated, created, updated) =
        row.ok_or_else(|| anyhow::anyhow!("no report with id {id}"))?;

    Ok(Report {
        id,
        name,
        template: serde_json::from_str(&template).context("parsing the stored template")?,
        notes: serde_json::from_str(&notes).context("parsing the stored notes")?,
        generated: match generated {
            Some(text) => Some(serde_json::from_str(&text).context("parsing the stored report")?),
            None => None,
        },
        created: created as u64,
        updated: updated as u64,
    })
}

/// Delete a report. Idempotent, for the same reason as [`delete_template`].
pub fn delete_report(id: Uuid) -> Result<()> {
    db::open()?
        .execute("DELETE FROM reports WHERE id = ?1", [id.to_string()])
        .context("deleting the report")?;
    Ok(())
}

/// Reports, most recently worked on first — the one a user wants next is almost always
/// the one they had open last.
///
/// **Reads no document text.** Every column here is small and indexed; the notes, the
/// generated prose and the template snapshot are never touched. That is the whole reason
/// this module moved to a database, so it is worth stating where the query lives.
pub fn list_reports() -> Result<Vec<Summary>> {
    let connection = db::open()?;
    let mut statement = connection
        .prepare(
            "SELECT id, name, template_name, updated, generated IS NOT NULL
             FROM reports ORDER BY updated DESC, name ASC",
        )
        .context("preparing the report list")?;

    let rows = statement
        .query_map([], |row| {
            let id: String = row.get(0)?;
            Ok(Summary {
                id: id.parse().unwrap_or(Uuid::nil()),
                name: row.get(1)?,
                template_name: row.get(2)?,
                updated: row.get::<_, i64>(3)? as u64,
                // `NULL` until the report has been written once, which is exactly the
                // Draft/Final distinction the library row draws.
                generated: row.get(4)?,
                // Only a template is a list of fields; a report is the prose one made.
                fields: 0,
            })
        })
        .context("listing reports")?;

    rows.collect::<rusqlite::Result<Vec<_>>>().context("reading the report list")
}

// ---------------------------------------------------------------------------
// Sharing a template as a file
// ---------------------------------------------------------------------------

/// A template as portable `.json`.
///
/// Pretty-printed, because the point of a file is that a person can read it, diff it and
/// commit it. This is the property the database took away, handed back deliberately.
pub fn export_template(template: &Template) -> Result<String> {
    serde_json::to_string_pretty(template).context("serialising the template")
}

/// Read a template from exported `.json` and store it under a **fresh id**.
///
/// Not the id in the file. Importing a colleague's copy of a template you also have
/// would otherwise overwrite yours silently — a duplicate is recoverable by deleting
/// one, a silent overwrite is not. The name gains a suffix if it collides, so the two
/// are distinguishable in the list.
pub fn import_template(json: &str) -> Result<Template> {
    let mut template: Template =
        serde_json::from_str(json).context("this file is not a report-tool template")?;

    template.id = Uuid::new_v4();
    let taken: Vec<String> = list_templates()?.into_iter().map(|s| s.name).collect();
    template.name = unique_name(&template.name, &taken);

    save_template(&template)?;
    Ok(template)
}

/// `name`, or `name (2)`, `name (3)`… until it is not already taken.
fn unique_name(name: &str, taken: &[String]) -> String {
    if !taken.iter().any(|existing| existing == name) {
        return name.to_string();
    }
    (2..)
        .map(|n| format!("{name} ({n})"))
        .find(|candidate| !taken.iter().any(|existing| existing == candidate))
        // `(2..)` is unbounded, so this cannot be reached; `expect` documents that
        // rather than inventing a fallback name nobody would ever see.
        .expect("an unbounded range always yields a free name")
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

    /// Exercises the real database through `REPORT_DATA_DIR`.
    ///
    /// Serialisation is covered above; what this adds is everything that only shows up
    /// against SQLite — the upsert, the summary projection, and deleting.
    #[test]
    fn the_store_round_trips_through_a_real_database() {
        // Holds the shared lock and cleans up on drop; see `crate::testenv`.
        let _dir = crate::testenv::data_dir("store");

        // A library that has never been written must list empty, not error.
        assert!(list_reports().unwrap().is_empty());
        assert!(list_templates().unwrap().is_empty());

        let template = fixture::template();
        save_template(&template).unwrap();
        assert_eq!(load_template(template.id).unwrap(), template);
        let listed = list_templates().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Site Inspection");
        // A real column now, not the file mtime it used to be.
        assert!(listed[0].updated > 0);
        // The count the library row shows. Counted by SQLite out of the stored body, so
        // it is worth checking against a template whose fields are known.
        assert_eq!(listed[0].fields, template.nodes.len());

        // An empty template must report zero rather than failing the projection — that
        // is the state a just-created template is in, and the state the row marks.
        let empty = Template::new("Untitled template");
        assert!(empty.nodes.is_empty(), "a new template starts with no fields");
        save_template(&empty).unwrap();
        let all = list_templates().unwrap();
        let found = all.iter().find(|s| s.id == empty.id).expect("the empty template is listed");
        assert_eq!(found.fields, 0);
        delete_template(empty.id).unwrap();

        let mut report = Report::new("March visit", template.clone());
        report.notes = report_doc::markdown::from_markdown("north wall cracked");
        save_report(&report).unwrap();

        let loaded = load_report(report.id).unwrap();
        assert_eq!(loaded.name, "March visit");
        assert_eq!(loaded.notes, report.notes);
        assert_eq!(loaded.template, report.template);
        assert!(loaded.updated > 0, "saving must stamp the time");

        // The library row's two extra columns, read without touching a document.
        let listed = list_reports().unwrap();
        assert_eq!(listed[0].template_name, "Site Inspection");
        assert!(!listed[0].generated, "a report with no prose yet is a draft");

        // Saving again must update rather than duplicate.
        save_report(&loaded).unwrap();
        assert_eq!(list_reports().unwrap().len(), 1);

        // Once written, the row must flip from Draft to Final.
        let mut written = loaded.clone();
        written.generated = Some(report_doc::markdown::from_markdown("# Findings\n\nAll noted."));
        save_report(&written).unwrap();
        assert!(list_reports().unwrap()[0].generated);
        assert!(load_report(report.id).unwrap().generated.is_some());

        delete_report(report.id).unwrap();
        assert!(load_report(report.id).is_err());
        // Deleting twice is the caller getting what they asked for.
        delete_report(report.id).unwrap();
        assert!(list_reports().unwrap().is_empty());

        delete_template(template.id).unwrap();
        delete_template(template.id).unwrap();
        assert!(list_templates().unwrap().is_empty());
    }

    #[test]
    fn a_missing_id_is_an_error_rather_than_a_default_value() {
        // Silently returning an empty report would look like data loss to whoever
        // opened it.
        let _dir = crate::testenv::data_dir("store-missing");
        assert!(load_report(Uuid::new_v4()).is_err());
        assert!(load_template(Uuid::new_v4()).is_err());
    }

    #[test]
    fn an_exported_template_can_be_imported_beside_the_original() {
        // The property the database took away: a template as a file you can send someone.
        let _dir = crate::testenv::data_dir("store-share");

        let template = fixture::template();
        save_template(&template).unwrap();
        let json = export_template(&template).unwrap();

        let imported = import_template(&json).unwrap();

        // A fresh id, not the one in the file. Preserving it would silently overwrite
        // the local copy, which is not recoverable; a duplicate is.
        assert_ne!(imported.id, template.id);
        // Same structure, so it still compiles and renders.
        assert_eq!(imported.nodes, template.nodes);
        // Distinguishable in the list.
        assert_eq!(imported.name, "Site Inspection (2)");

        let names: Vec<String> = list_templates().unwrap().into_iter().map(|s| s.name).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"Site Inspection".to_string()));
        assert!(names.contains(&"Site Inspection (2)".to_string()));

        // Importing a third time keeps counting rather than colliding.
        assert_eq!(import_template(&json).unwrap().name, "Site Inspection (3)");
    }

    #[test]
    fn importing_something_that_is_not_a_template_says_so() {
        let _dir = crate::testenv::data_dir("store-badimport");
        let error = import_template("{\"nope\": true}").unwrap_err().to_string();
        assert!(error.contains("not a report-tool template"), "{error}");
        assert!(import_template("this is not json at all").is_err());
    }

    #[test]
    fn unique_name_counts_upward_past_the_names_already_taken() {
        let taken = ["Visit".to_string(), "Visit (2)".to_string(), "Visit (4)".to_string()];
        assert_eq!(unique_name("Visit", &taken), "Visit (3)");
        assert_eq!(unique_name("Other", &taken), "Other");
        assert_eq!(unique_name("Visit", &[]), "Visit");
    }

    #[test]
    fn reports_are_listed_most_recently_worked_on_first() {
        let summary = |name: &str, updated| Summary {
            id: Uuid::new_v4(),
            name: name.into(),
            updated,
            template_name: String::new(),
            generated: false,
            fields: 0,
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
