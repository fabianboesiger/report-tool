//! The database: opening it, keeping its schema current, and the one-time import of
//! the JSON files that used to be the storage layer.
//!
//! ## Why a database at all
//!
//! Templates and reports were one JSON file each, which was a deliberate choice and
//! became the wrong one for two measurable reasons.
//!
//! The library screen read **every byte of every report** to draw a list: summarising
//! a report meant parsing the whole file, and a report carries its notes, its generated
//! prose *and* a full template snapshot. The five fields a row needs are precisely the
//! ones that do not require any of that. [`crate::store::list_reports`] is now a query
//! over indexed columns that touches no document text at all.
//!
//! And autosave rewrites a report every two seconds. As files that meant re-serialising
//! both documents and the snapshot, then a temporary file and a rename — so typing a
//! sentence in the notes rewrote the generated report.
//!
//! ## A connection per operation
//!
//! The obvious design is a process-wide `OnceLock<Mutex<Connection>>`. It quietly
//! breaks the tests: [`crate::testenv::data_dir`] gives each test its own
//! `REPORT_DATA_DIR`, and a static connection would pin whichever temp directory
//! opened first, so every later test would read another test's database and pass or
//! fail on ordering.
//!
//! Opening is microseconds against an existing file, the busiest caller is autosave at
//! one write per two seconds, and per-call connections remove the lock, the poisoning
//! question and the isolation problem at once. The cost is rusqlite's statement cache,
//! which lives on the connection and is therefore forfeited — microseconds per trivial
//! statement, against tests that silently read each other's data. Worth it.
//!
//! **Do not "optimise" this into a shared connection.** That is the whole comment.

use anyhow::{Context, Result};
use rusqlite::Connection;

/// The schema version this build expects. Bump it and add a step to [`migrate`].
const SCHEMA_VERSION: u32 = 1;

/// Open the database, bringing its schema up to date.
pub fn open() -> Result<Connection> {
    let path = crate::paths::db_path()?;
    let connection = Connection::open(&path)
        .with_context(|| format!("opening the database at {}", path.display()))?;

    // WAL is recorded in the file, so this only does work the first time. It is what
    // lets the library list while autosave writes.
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .context("enabling write-ahead logging")?;
    // Per connection. NORMAL fsyncs at checkpoints rather than every commit: with WAL
    // that still survives a process crash, only a power cut can lose the last commits,
    // and the alternative is an fsync in the autosave path.
    connection.pragma_update(None, "synchronous", "NORMAL").context("setting synchronous")?;
    // Also per connection, and off by default. Nothing here uses foreign keys yet — a
    // report holds a template *snapshot*, not a reference — but a later migration that
    // adds one would otherwise be silently unenforced.
    connection.pragma_update(None, "foreign_keys", true).context("enabling foreign keys")?;

    migrate(&connection)?;
    Ok(connection)
}

/// Apply every migration the file is behind, in one transaction.
///
/// Versioned with `PRAGMA user_version` rather than a table of its own: it is a single
/// integer already in the file's header, and reading it on every open costs nothing.
fn migrate(connection: &Connection) -> Result<()> {
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .context("reading the schema version")?;

    if version == SCHEMA_VERSION {
        return Ok(());
    }
    anyhow::ensure!(
        version <= SCHEMA_VERSION,
        "this database is version {version} but this build only understands {SCHEMA_VERSION}. \
         It was written by a newer version of the app; upgrading is the way forward, and \
         opening it here would risk losing data."
    );

    // One transaction for the whole climb: a migration that fails halfway leaves the
    // file exactly as it was rather than half-shaped.
    connection.execute_batch("BEGIN")?;

    if version < 1 {
        connection.execute_batch(SCHEMA_V1).context("creating the schema")?;
    }

    // `pragma_update` cannot take a bound parameter, and this is a constant.
    connection
        .execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))
        .context("stamping the schema version")?;
    connection.execute_batch("COMMIT")?;

    tracing::info!("db: schema at version {SCHEMA_VERSION} (was {version})");

    // After the schema exists, and outside the transaction: importing reads the
    // filesystem, and a slow or failing import must not hold a write lock or undo the
    // schema it needs.
    if version < 1 {
        if let Err(error) = import_legacy(connection) {
            // Not fatal. The files are still on disk, untouched, and a working empty
            // library beats refusing to start.
            tracing::error!("db: could not import the old JSON files: {error:#}");
        }
    }

    Ok(())
}

const SCHEMA_V1: &str = "
CREATE TABLE templates (
    id      TEXT    PRIMARY KEY NOT NULL,
    name    TEXT    NOT NULL,
    body    TEXT    NOT NULL,
    created INTEGER NOT NULL,
    updated INTEGER NOT NULL
);
CREATE INDEX templates_by_updated ON templates(updated DESC, name ASC);

CREATE TABLE reports (
    id            TEXT    PRIMARY KEY NOT NULL,
    name          TEXT    NOT NULL,
    template      TEXT    NOT NULL,
    template_name TEXT    NOT NULL,
    notes         TEXT    NOT NULL,
    generated     TEXT,
    created       INTEGER NOT NULL,
    updated       INTEGER NOT NULL
);
CREATE INDEX reports_by_updated ON reports(updated DESC, name ASC);

CREATE TABLE settings (
    id   INTEGER PRIMARY KEY CHECK (id = 1),
    body TEXT NOT NULL
);
";

// ---------------------------------------------------------------------------
// The one-time import
// ---------------------------------------------------------------------------

/// Bring the old per-file storage into the database, then move it aside.
///
/// Runs once, when the schema is first created. Every failure mode here is treated as
/// "skip it and say so" rather than "abort": this code meets data the user has no other
/// copy of, and refusing to start over one unreadable file would be the worst possible
/// response to it.
fn import_legacy(connection: &Connection) -> Result<()> {
    let mut templates = 0usize;
    let mut reports = 0usize;
    let mut skipped = 0usize;

    if let Ok(dir) = crate::paths::legacy_templates_dir() {
        for (path, value) in json_files(&dir) {
            match serde_json::from_value::<crate::template::Template>(value) {
                Ok(template) => {
                    // Stamped with the file's mtime, which is the only timestamp a
                    // template file ever had — the old listing sorted by it. Losing it
                    // would drop every imported template to the bottom of the list.
                    let when = modified_at(&path);
                    match insert_template(connection, &template, when) {
                        Ok(()) => templates += 1,
                        Err(error) => {
                            tracing::warn!("db: could not import {}: {error:#}", path.display());
                            skipped += 1;
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!("db: skipping {}: {error}", path.display());
                    skipped += 1;
                }
            }
        }
    }

    if let Ok(dir) = crate::paths::legacy_reports_dir() {
        for (path, value) in json_files(&dir) {
            match serde_json::from_value::<crate::store::Report>(value) {
                Ok(report) => match insert_report(connection, &report) {
                    Ok(()) => reports += 1,
                    Err(error) => {
                        tracing::warn!("db: could not import {}: {error:#}", path.display());
                        skipped += 1;
                    }
                },
                Err(error) => {
                    tracing::warn!("db: skipping {}: {error}", path.display());
                    skipped += 1;
                }
            }
        }
    }

    let mut settings = false;
    if let Ok(path) = crate::paths::legacy_settings_path() {
        if let Ok(text) = std::fs::read_to_string(&path) {
            match serde_json::from_str::<crate::settings::Settings>(&text) {
                Ok(loaded) => {
                    if let Err(error) = write_settings(connection, &loaded) {
                        tracing::warn!("db: could not import settings: {error:#}");
                    } else {
                        settings = true;
                    }
                }
                Err(error) => tracing::warn!("db: skipping {}: {error}", path.display()),
            }
        }
    }

    if templates + reports + skipped == 0 && !settings {
        // A fresh install. Nothing to move aside, and nothing worth a log line.
        return Ok(());
    }

    tracing::info!(
        "db: imported {templates} template(s), {reports} report(s), settings: {settings}, \
         skipped {skipped} unreadable file(s)"
    );
    move_legacy_aside();
    Ok(())
}

/// Rename the old locations out of the way.
///
/// **Renamed, never deleted.** This runs exactly once, on the user's only copy, driven
/// by code that has never run before on their machine. If the import got something
/// wrong, the original is still sitting there under a slightly different name.
fn move_legacy_aside() {
    let moves = [
        crate::paths::legacy_templates_dir().ok(),
        crate::paths::legacy_reports_dir().ok(),
        crate::paths::legacy_settings_path().ok(),
    ];
    for path in moves.into_iter().flatten() {
        if !path.exists() {
            continue;
        }
        let mut target = path.clone().into_os_string();
        target.push(".imported");
        let target = std::path::PathBuf::from(target);
        // If a previous attempt already left one, keep it: it is the older, more
        // original copy of the two.
        if target.exists() {
            continue;
        }
        match std::fs::rename(&path, &target) {
            Ok(()) => tracing::info!("db: moved {} aside", path.display()),
            Err(error) => tracing::warn!("db: could not move {} aside: {error}", path.display()),
        }
    }
}

/// Every `*.json` in `dir`, parsed loosely, paired with its path.
///
/// Loosely on purpose: a file that will not parse is skipped by the caller rather than
/// failing the import. There is a `broken.json` on the author's machine that proves the
/// point.
fn json_files(dir: &std::path::Path) -> Vec<(std::path::PathBuf, serde_json::Value)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
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
        {
            Some(value) => out.push((path, value)),
            None => tracing::warn!("db: skipping unreadable {}", path.display()),
        }
    }
    out
}

fn modified_at(path: &std::path::Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|since| since.as_secs())
        .unwrap_or_else(crate::store::now)
}

// ---------------------------------------------------------------------------
// The writes, shared between the importer and `store`
// ---------------------------------------------------------------------------

/// Insert or replace a template, preserving `created` if it is already there.
pub(crate) fn insert_template(
    connection: &Connection,
    template: &crate::template::Template,
    when: u64,
) -> Result<()> {
    let body = serde_json::to_string(template).context("serialising the template")?;
    connection
        .execute(
            "INSERT INTO templates (id, name, body, created, updated)
             VALUES (?1, ?2, ?3, ?4, ?4)
             ON CONFLICT(id) DO UPDATE SET name = ?2, body = ?3, updated = ?4",
            // `as i64`: SQLite has no unsigned 64-bit integer, and rusqlite therefore
            // refuses `u64`. These are epoch seconds, so the cast is lossless until the
            // year 292 billion.
            rusqlite::params![template.id.to_string(), template.name, body, when as i64],
        )
        .context("writing the template")?;
    Ok(())
}

/// Insert or replace a report.
///
/// `template_name` is denormalised out of the snapshot here rather than extracted with
/// `json_extract` when listing — that is what lets the list query be covered by an
/// index and read no document text.
pub(crate) fn insert_report(connection: &Connection, report: &crate::store::Report) -> Result<()> {
    let template = serde_json::to_string(&report.template).context("serialising the template")?;
    let notes = serde_json::to_string(&report.notes).context("serialising the notes")?;
    let generated = match &report.generated {
        Some(document) => Some(serde_json::to_string(document).context("serialising the report")?),
        None => None,
    };

    connection
        .execute(
            "INSERT INTO reports
                 (id, name, template, template_name, notes, generated, created, updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
                 name = ?2, template = ?3, template_name = ?4,
                 notes = ?5, generated = ?6, updated = ?8",
            rusqlite::params![
                report.id.to_string(),
                report.name,
                template,
                report.template.name,
                notes,
                generated,
                report.created as i64,
                report.updated as i64,
            ],
        )
        .context("writing the report")?;
    Ok(())
}

pub(crate) fn write_settings(
    connection: &Connection,
    settings: &crate::settings::Settings,
) -> Result<()> {
    let body = serde_json::to_string(settings).context("serialising the settings")?;
    connection
        .execute(
            "INSERT INTO settings (id, body) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET body = ?1",
            rusqlite::params![body],
        )
        .context("writing the settings")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_database_opens_migrated_and_empty() {
        let _dir = crate::testenv::data_dir("db-fresh");
        let connection = open().unwrap();

        let version: u32 =
            connection.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
        assert_eq!(version, SCHEMA_VERSION);

        let templates: u32 =
            connection.query_row("SELECT count(*) FROM templates", [], |r| r.get(0)).unwrap();
        assert_eq!(templates, 0);
    }

    #[test]
    fn opening_twice_is_idempotent() {
        // Migrations run on every open; a second one must find nothing to do rather
        // than re-creating tables or re-importing files.
        let _dir = crate::testenv::data_dir("db-twice");
        let first = open().unwrap();
        crate::db::insert_template(&first, &crate::template::fixture::template(), 100).unwrap();
        drop(first);

        let second = open().unwrap();
        let count: u32 =
            second.query_row("SELECT count(*) FROM templates", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1, "the second open must not have wiped or duplicated anything");
    }

    #[test]
    fn a_database_from_a_newer_build_is_refused_rather_than_mangled() {
        let _dir = crate::testenv::data_dir("db-future");
        let connection = open().unwrap();
        connection.execute_batch("PRAGMA user_version = 99").unwrap();
        drop(connection);

        let error = open().unwrap_err().to_string();
        assert!(error.contains("newer version"), "{error}");
    }

    #[test]
    fn write_ahead_logging_is_on() {
        // Not cosmetic: it is what lets the library list while autosave holds a write.
        let _dir = crate::testenv::data_dir("db-wal");
        let connection = open().unwrap();
        let mode: String =
            connection.pragma_query_value(None, "journal_mode", |row| row.get(0)).unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
    }

    /// The one-time import, against the exact shapes sitting on the author's machine:
    /// two template files, a report, a `broken.json`, and a `settings.json`.
    ///
    /// This code path runs once per user, on data they have no other copy of, so the
    /// behaviour that matters most is what it does with the file it cannot read.
    #[test]
    fn the_legacy_json_files_are_imported_and_moved_aside() {
        let dir = crate::testenv::data_dir("db-import");

        let templates = dir.path().join("templates");
        let reports = dir.path().join("reports");
        std::fs::create_dir_all(&templates).unwrap();
        std::fs::create_dir_all(&reports).unwrap();

        let mut first = crate::template::fixture::template();
        first.name = "Site inspection".into();
        let mut second = crate::template::fixture::template();
        second.id = uuid::Uuid::new_v4();
        second.name = "Compliance".into();
        for template in [&first, &second] {
            std::fs::write(
                templates.join(format!("{}.json", template.id)),
                serde_json::to_string(template).unwrap(),
            )
            .unwrap();
        }

        let mut report = crate::store::Report::new("March visit", first.clone());
        report.notes = report_doc::markdown::from_markdown("north wall cracked");
        std::fs::write(
            reports.join(format!("{}.json", report.id)),
            serde_json::to_string(&report).unwrap(),
        )
        .unwrap();
        // The one that must be skipped rather than aborting the import.
        std::fs::write(reports.join("broken.json"), "{ not json").unwrap();

        let mut settings = crate::settings::Settings::default();
        settings.openai.model = "carried-over".into();
        std::fs::write(dir.path().join("settings.json"), serde_json::to_string(&settings).unwrap())
            .unwrap();

        // Opening is what triggers the import.
        drop(open().unwrap());

        let listed = crate::store::list_templates().unwrap();
        assert_eq!(listed.len(), 2, "both templates came across");
        // The file's mtime became the row's `updated`, so imported templates keep their
        // order instead of all landing at the bottom.
        assert!(listed.iter().all(|summary| summary.updated > 0));

        let listed = crate::store::list_reports().unwrap();
        assert_eq!(listed.len(), 1, "the readable report came across, the broken one did not");
        assert_eq!(listed[0].name, "March visit");
        assert_eq!(listed[0].template_name, "Site inspection");

        let loaded = crate::store::load_report(report.id).unwrap();
        assert_eq!(loaded.notes, report.notes, "the notes survived the import");

        assert_eq!(
            crate::settings::Settings::load().openai.model,
            "carried-over",
            "the settings came across"
        );

        // Moved aside, **not deleted** — the import ran once, on the only copy.
        assert!(!templates.exists(), "the old directory should have been renamed");
        assert!(dir.path().join("templates.imported").is_dir());
        assert!(dir.path().join("reports.imported").is_dir());
        assert!(dir.path().join("settings.json.imported").is_file());
        // Including the file it could not read: that is the copy a human may need.
        assert!(dir.path().join("reports.imported/broken.json").is_file());

        // A second open must not re-import or duplicate anything.
        drop(open().unwrap());
        assert_eq!(crate::store::list_templates().unwrap().len(), 2);
        assert_eq!(crate::store::list_reports().unwrap().len(), 1);
    }

    #[test]
    fn a_fresh_install_imports_nothing_and_moves_nothing() {
        let dir = crate::testenv::data_dir("db-noimport");
        drop(open().unwrap());
        assert!(!dir.path().join("templates.imported").exists());
        assert!(!dir.path().join("settings.json.imported").exists());
    }

    #[test]
    fn saving_a_template_twice_updates_it_rather_than_duplicating() {
        let _dir = crate::testenv::data_dir("db-upsert");
        let connection = open().unwrap();
        let mut template = crate::template::fixture::template();

        insert_template(&connection, &template, 100).unwrap();
        template.name = "Renamed".into();
        insert_template(&connection, &template, 200).unwrap();

        let (count, name, created, updated): (u32, String, i64, i64) = connection
            .query_row("SELECT count(*), name, created, updated FROM templates", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(name, "Renamed");
        assert_eq!(created, 100, "the original creation time must survive an update");
        assert_eq!(updated, 200);
    }
}
