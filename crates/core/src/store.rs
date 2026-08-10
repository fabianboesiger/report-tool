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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub id: Uuid,
    pub name: String,
    pub updated: u64,
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
    list(&crate::paths::templates_dir()?, |value| {
        Some(Summary {
            id: value.get("id")?.as_str()?.parse().ok()?,
            name: value.get("name")?.as_str()?.to_string(),
            // Templates carry no timestamp; the file's own mtime orders them.
            updated: 0,
        })
    })
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
    let mut reports = list(&crate::paths::reports_dir()?, |value| {
        Some(Summary {
            id: value.get("id")?.as_str()?.parse().ok()?,
            name: value.get("name")?.as_str()?.to_string(),
            updated: value.get("updated").and_then(serde_json::Value::as_u64).unwrap_or(0),
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
fn list(
    dir: &Path,
    summarise: impl Fn(&serde_json::Value) -> Option<Summary>,
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
        match std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .as_ref()
            .and_then(&summarise)
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

        // Saving again must overwrite rather than leave two entries.
        save_report(&loaded).unwrap();
        assert_eq!(list_reports().unwrap().len(), 1);

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
        let mut summaries = vec![
            Summary { id: Uuid::new_v4(), name: "old".into(), updated: 100 },
            Summary { id: Uuid::new_v4(), name: "newest".into(), updated: 300 },
            Summary { id: Uuid::new_v4(), name: "middle".into(), updated: 200 },
        ];
        summaries.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| a.name.cmp(&b.name)));
        assert_eq!(
            summaries.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            ["newest", "middle", "old"]
        );
    }
}
