//! Exercises the resumable downloader against a real server.
//!
//! Resuming is the part that cannot be unit-tested: it depends on the server
//! honouring `Range`, on the status code being read correctly, and on the partial
//! file surviving a restart. All three only show up against the real thing.
//!
//! ```text
//! cargo run -p report-core --example download_smoke --features download -- dictation
//! ```
//!
//! Interrupt it with Ctrl-C and run it again: it must continue from where it stopped
//! rather than starting over.

use std::time::Instant;

use report_core::catalog;
use report_core::download;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let which = std::env::args().nth(1).unwrap_or_else(|| "dictation".to_string());
    let spec = match which.as_str() {
        "report" => catalog::REPORT_MODEL,
        _ => catalog::DICTATION_MODEL,
    };

    let path = spec.path()?;
    println!("{} → {}", spec.name, path.display());
    println!("license: {}", spec.license);

    if download::is_complete(&path) {
        let size = std::fs::metadata(&path)?.len();
        println!("already downloaded ({})", download::human_bytes(size));
        return Ok(());
    }

    let resumed = download::resume_offset(&path);
    if resumed > 0 {
        println!("resuming from {}", download::human_bytes(resumed));
    }

    let started = Instant::now();
    let mut last_line = Instant::now();
    download::fetch(spec.url, &path, |progress| {
        // Throttled, or the terminal becomes the bottleneck.
        if last_line.elapsed().as_millis() < 250 {
            return;
        }
        last_line = Instant::now();
        let rate = (progress.downloaded.saturating_sub(resumed)) as f64
            / started.elapsed().as_secs_f64().max(0.001);
        match progress.fraction() {
            Some(fraction) => print!(
                "\r{:>5.1}%  {} / {}  at {}/s      ",
                fraction * 100.0,
                download::human_bytes(progress.downloaded),
                download::human_bytes(progress.total.unwrap_or(0)),
                download::human_bytes(rate as u64)
            ),
            None => print!("\r{}      ", download::human_bytes(progress.downloaded)),
        }
        use std::io::Write;
        let _ = std::io::stdout().flush();
    })
    .await?;

    let size = std::fs::metadata(&path)?.len();
    println!("\ndone: {} in {:.0}s", download::human_bytes(size), started.elapsed().as_secs_f32());
    Ok(())
}
