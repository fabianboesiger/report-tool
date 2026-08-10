//! Resumable file downloads.
//!
//! Models are gigabytes. On a laptop that means a download will be interrupted —
//! the lid closes, the train enters a tunnel, the app is quit. Restarting from zero
//! each time would make the first run of the app a game of chance, so a partial
//! download is kept and continued.
//!
//! ## How resuming stays honest
//!
//! Continuing a download means trusting that the bytes already on disk belong to the
//! same file the server is about to send more of. Two things guard that:
//!
//! - The **ETag** is recorded beside the partial file. Hugging Face returns the
//!   file's SHA-256 as its ETag, so a model that was re-uploaded between sessions
//!   produces a different one and the download restarts rather than splicing two
//!   different files into a plausible-looking whole.
//! - The **status code** is checked. A server that ignores `Range` answers `200` with
//!   the whole file, and appending that to what we already have would silently
//!   corrupt it. Only `206 Partial Content` is treated as a continuation.
//!
//! The finished file is renamed into place, so a path that exists is always a
//! complete download and never a truncated one.

use std::path::{Path, PathBuf};

// Only `fetch` uses these, and it is feature-gated.
#[cfg(feature = "download")]
use anyhow::{Context, Result};

/// How far along a download is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub downloaded: u64,
    /// `None` when the server did not say — rare, but a progress bar has to cope.
    pub total: Option<u64>,
}

impl Progress {
    pub fn fraction(&self) -> Option<f32> {
        let total = self.total?;
        (total > 0).then(|| (self.downloaded as f64 / total as f64) as f32)
    }
}

/// Where the in-progress bytes and their ETag live.
fn partial_paths(dest: &Path) -> (PathBuf, PathBuf) {
    let mut part = dest.as_os_str().to_os_string();
    part.push(".part");
    let part = PathBuf::from(part);

    let mut meta = part.as_os_str().to_os_string();
    meta.push(".etag");
    (part, PathBuf::from(meta))
}

/// Whether `dest` is already a finished download.
pub fn is_complete(dest: &Path) -> bool {
    dest.is_file()
}

/// Bytes already downloaded towards `dest`, for showing progress before a request
/// has been made.
pub fn resume_offset(dest: &Path) -> u64 {
    let (part, _) = partial_paths(dest);
    std::fs::metadata(part).map(|m| m.len()).unwrap_or(0)
}

/// How many times in a row an attempt may fail *without moving the download forward*.
///
/// Reset by any progress at all, so a connection that drops every few hundred
/// megabytes continues indefinitely, while one failing instantly gives up quickly.
#[cfg(feature = "download")]
const MAX_STALLED_ATTEMPTS: u32 = 8;

/// How long to wait before the next attempt.
#[cfg(feature = "download")]
fn backoff(stalled: u32) -> std::time::Duration {
    // Doubling, capped: a flaky connection recovers in seconds, and waiting minutes
    // would look like the download had died.
    std::time::Duration::from_secs(1u64 << stalled.min(5))
}

/// Download `url` to `dest`, continuing a previous attempt where possible.
///
/// **Retries by itself.** A multi-gigabyte transfer over a real connection *will* be
/// cut short — a CDN drops the connection, a laptop changes network, a train enters a
/// tunnel — and reqwest reports it as "end of file before message length reached".
/// Treating that as fatal would leave the user reopening the app over and over to
/// inch a 5 GB file forward, so each interruption just resumes from the bytes already
/// written.
///
/// Returns immediately if `dest` already exists. Dropping the future leaves the
/// partial file in place, which is what makes quitting the app mid-download safe.
#[cfg(feature = "download")]
pub async fn fetch(url: &str, dest: &Path, mut on_progress: impl FnMut(Progress)) -> Result<()> {
    if is_complete(dest) {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    let mut stalled = 0u32;
    loop {
        let before = resume_offset(dest);
        let error = match attempt(url, dest, &mut on_progress).await {
            Ok(()) => return Ok(()),
            Err(error) => error,
        };

        // Some failures will never succeed however often they are tried.
        if error.downcast_ref::<Fatal>().is_some() {
            return Err(error);
        }

        // Forward movement earns a fresh budget; standing still spends it.
        stalled = if resume_offset(dest) > before { 0 } else { stalled + 1 };
        if stalled > MAX_STALLED_ATTEMPTS {
            return Err(error.context(format!(
                "gave up after {MAX_STALLED_ATTEMPTS} attempts that made no progress. \
                 {} is kept and will resume next time.",
                human_bytes(resume_offset(dest))
            )));
        }

        let wait = backoff(stalled);
        tracing::warn!(
            "download: {error:#} — resuming from {} in {}s",
            human_bytes(resume_offset(dest)),
            wait.as_secs()
        );
        tokio::time::sleep(wait).await;
    }
}

/// An error there is no point retrying.
#[cfg(feature = "download")]
#[derive(Debug)]
struct Fatal(String);

#[cfg(feature = "download")]
impl std::fmt::Display for Fatal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(feature = "download")]
impl std::error::Error for Fatal {}

/// One request. Returns `Ok` only once the whole file is in place.
#[cfg(feature = "download")]
async fn attempt(url: &str, dest: &Path, on_progress: &mut impl FnMut(Progress)) -> Result<()> {
    use tokio::io::AsyncWriteExt;

    let (part, etag_path) = partial_paths(dest);
    let mut have = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    let previous_etag = std::fs::read_to_string(&etag_path).ok();

    let client = reqwest::Client::builder()
        // No overall timeout: this is a multi-gigabyte transfer. The read timeout
        // catches a connection that has actually stalled, without capping how long a
        // legitimately slow download may take.
        .read_timeout(std::time::Duration::from_secs(60))
        .build()
        .context("building the HTTP client")?;

    let mut request = client.get(url);
    if have > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={have}-"));
    }

    let response = request.send().await.with_context(|| format!("requesting {url}"))?;
    let status = response.status();

    // The partial file is at least as long as the remote one: the file changed, or a
    // previous run wrote past the end. Start over rather than asking forever for a
    // range that cannot exist.
    if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        let _ = std::fs::remove_file(&part);
        let _ = std::fs::remove_file(&etag_path);
        anyhow::bail!("the server rejected the resume point, so the download restarts");
    }
    // A missing file or a refused request will not fix itself; a 5xx or a rate limit
    // very well might.
    if status.is_client_error() && status != reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(Fatal(format!("{url} returned {status}")).into());
    }
    anyhow::ensure!(status.is_success(), "{url} returned {status}");

    let etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim_matches('"').to_string());

    // Two ways the bytes on disk can turn out not to be a prefix of what is arriving.
    let changed = matches!((&previous_etag, &etag), (Some(before), Some(now)) if before != now);
    let ignored_range = have > 0 && status != reqwest::StatusCode::PARTIAL_CONTENT;
    let restart = changed || ignored_range;

    if restart {
        if changed {
            tracing::info!("download: {url} changed since the last attempt, starting over");
        } else {
            tracing::info!("download: the server ignored the range request, starting over");
        }
        have = 0;
    }

    // `content-length` is the length of *this* response, so a continuation reports
    // only the remainder; the whole file is that plus what we already hold.
    let total = response.content_length().map(|len| len + have);

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(!restart)
        .truncate(restart)
        .open(&part)
        .await
        .with_context(|| format!("opening {}", part.display()))?;

    if let Some(etag) = &etag {
        let _ = std::fs::write(&etag_path, etag);
    }

    let mut downloaded = have;
    on_progress(Progress { downloaded, total });

    let mut response = response;
    // `chunk()` rather than a stream adapter: it needs no extra dependency, and the
    // loop is the natural place to write and report progress.
    //
    // A failure here is the common case, not an exception — the bytes already written
    // stay on disk and the caller resumes from them.
    loop {
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(error) => {
                // Flushed before returning, so the retry resumes from everything that
                // actually arrived rather than re-fetching this buffer.
                let _ = file.flush().await;
                return Err(anyhow::Error::new(error).context("the connection dropped"));
            }
        };
        file.write_all(&chunk).await.with_context(|| format!("writing {}", part.display()))?;
        downloaded += chunk.len() as u64;
        on_progress(Progress { downloaded, total });
    }

    file.flush().await.context("flushing the download")?;
    drop(file);

    if let Some(total) = total {
        let written = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        anyhow::ensure!(written == total, "the download ended early: {written} of {total} bytes");
    }

    // Renamed only once complete, so a path that exists is always a whole file — the
    // property the loader depends on when it decides a model is present.
    std::fs::rename(&part, dest)
        .with_context(|| format!("moving the download into {}", dest.display()))?;
    let _ = std::fs::remove_file(&etag_path);

    tracing::info!("download: finished {}", dest.display());
    Ok(())
}

/// Format a byte count for a progress line.
pub fn human_bytes(bytes: u64) -> String {
    const GB: f64 = 1_000_000_000.0;
    const MB: f64 = 1_000_000.0;
    let bytes = bytes as f64;
    if bytes >= GB {
        format!("{:.1} GB", bytes / GB)
    } else {
        format!("{:.0} MB", bytes / MB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_partial_file_sits_beside_its_target() {
        // Appended rather than replacing the extension: `model.gguf.part` keeps the
        // real name visible, and `with_extension` would have produced `model.part`,
        // which collides between a `.gguf` and a `.bin` of the same stem.
        let (part, etag) = partial_paths(Path::new("/models/llm/gemma.gguf"));
        assert_eq!(part, Path::new("/models/llm/gemma.gguf.part"));
        assert_eq!(etag, Path::new("/models/llm/gemma.gguf.part.etag"));
    }

    #[test]
    fn progress_reports_a_fraction_only_when_the_total_is_known() {
        assert_eq!(Progress { downloaded: 50, total: Some(200) }.fraction(), Some(0.25));
        assert_eq!(Progress { downloaded: 50, total: None }.fraction(), None);
        // A zero total would divide by zero rather than mean "finished".
        assert_eq!(Progress { downloaded: 0, total: Some(0) }.fraction(), None);
    }

    #[test]
    fn resume_offset_is_zero_when_nothing_has_been_downloaded() {
        assert_eq!(resume_offset(Path::new("/nowhere/absent.gguf")), 0);
        assert!(!is_complete(Path::new("/nowhere/absent.gguf")));
    }

    #[test]
    fn resume_offset_reads_the_partial_file() {
        let dir = std::env::temp_dir().join(format!("rt-dl-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("model.gguf");
        std::fs::write(dir.join("model.gguf.part"), vec![0u8; 1234]).unwrap();

        assert_eq!(resume_offset(&dest), 1234);
        assert!(!is_complete(&dest), "a partial file is not a finished download");

        std::fs::write(&dest, b"done").unwrap();
        assert!(is_complete(&dest));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(feature = "download")]
    #[test]
    fn backoff_recovers_quickly_and_never_looks_dead() {
        // A flaky connection is usually fine again within seconds; waiting minutes
        // would be indistinguishable from the download having died.
        assert_eq!(backoff(0).as_secs(), 1);
        assert_eq!(backoff(1).as_secs(), 2);
        assert_eq!(backoff(4).as_secs(), 16);
        assert_eq!(backoff(5).as_secs(), 32);
        assert_eq!(backoff(99).as_secs(), 32, "capped, not shifted into overflow");
    }

    #[cfg(feature = "download")]
    #[test]
    fn a_fatal_error_is_recognisable_through_the_context_added_around_it() {
        // The retry loop distinguishes the two by downcasting, so wrapping must not
        // hide it — a 404 would otherwise be retried eight times.
        let fatal: anyhow::Error = Fatal("404".into()).into();
        let wrapped = fatal.context("requesting the model").context("downloading");
        assert!(wrapped.downcast_ref::<Fatal>().is_some());

        let ordinary = anyhow::anyhow!("the connection dropped").context("downloading");
        assert!(ordinary.downcast_ref::<Fatal>().is_none());
    }

    #[test]
    fn sizes_read_the_way_a_download_dialog_should() {
        assert_eq!(human_bytes(574_000_000), "574 MB");
        assert_eq!(human_bytes(5_150_000_000), "5.2 GB");
        assert_eq!(human_bytes(0), "0 MB");
    }
}
