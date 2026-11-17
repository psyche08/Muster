//! What an upload would carry: the newest lines of a log file, redacted as
//! the uploader redacts them (with a throwaway salt). Nothing is sent.
//!
//!   cargo run -p muster-logs --example redact_preview -- ~/Library/Logs/Muster/musterd.log [lines] [name…]

use std::io::{Read, Seek, SeekFrom};

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .expect("usage: redact_preview <log file> [lines] [name…]");
    let lines: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(200);
    let mut names: Vec<String> = args.collect();
    names.extend(std::env::var("USER"));
    let mut file = std::fs::File::open(&path)?;
    let size = file.metadata()?.len();
    file.seek(SeekFrom::Start(size.saturating_sub(lines as u64 * 400)))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes);
    let tail: Vec<&str> = text.lines().rev().take(lines).collect();
    let redactor =
        muster_logs::redact::Redactor::new(rand::random(), std::env::var("HOME").ok(), &names);
    for line in tail.into_iter().rev() {
        println!("{}", redactor.redact(line));
    }
    Ok(())
}
