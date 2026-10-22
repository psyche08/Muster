//! Diagnostics a member sends for inspection, such as a phone's performance
//! log: saved under /tmp/muster-reports on this device. The sender gives only
//! a known kind and bounded bytes; the daemon names the file after the sender
//! and the time, and keeps the newest files only.
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use muster_proto::v1;
use muster_rpc::{Context, Principal};

use crate::{Daemon, Error};

pub const DIR: &str = "/tmp/muster-reports";
/// Saved reports kept; older ones are removed when a new one arrives.
const KEEP: usize = 50;

pub fn upload(
    daemon: &Daemon,
    ctx: &Context,
    r: v1::DiagnosticsUploadRequest,
) -> Result<v1::DiagnosticsSaved, Error> {
    let sender = match ctx.principal {
        Principal::Local => daemon.name(),
        Principal::Remote(peer) => daemon
            .roster()
            .read()
            .unwrap()
            .state()
            .active(&peer)
            .map(|m| m.record.name.clone())
            .ok_or_else(|| Error::Unauthorized("not an active member".into()))?,
    };
    let path = save(Path::new(DIR), &sender, &r.kind, &r.data, SystemTime::now())?;
    log::info!(
        "diagnostics from {sender}: {} bytes saved to {}",
        r.data.len(),
        path.display()
    );
    Ok(v1::DiagnosticsSaved {
        path: path.display().to_string(),
    })
}

pub fn save(
    dir: &Path,
    sender: &str,
    kind: &str,
    data: &[u8],
    now: SystemTime,
) -> Result<PathBuf, Error> {
    if !muster_proto::DIAGNOSTICS_KINDS.contains(&kind) {
        return Err(Error::Invalid(format!("unknown diagnostics kind {kind:?}")));
    }
    if data.is_empty() || data.len() > muster_proto::MAX_DIAGNOSTICS {
        return Err(Error::Invalid(format!(
            "diagnostics must be 1 to {} bytes",
            muster_proto::MAX_DIAGNOSTICS
        )));
    }
    let io = |error: std::io::Error| Error::Unavailable(format!("saving diagnostics: {error}"));
    std::fs::create_dir_all(dir).map_err(io)?;
    let stem = format!("{}-{}-{kind}", file_name(sender), utc_stamp(now));
    let mut path = dir.join(format!("{stem}.jsonl"));
    let mut n = 1;
    while path.exists() {
        n += 1;
        path = dir.join(format!("{stem}-{n}.jsonl"));
    }
    let temporary = dir.join(format!(".{stem}.{}.tmp", std::process::id()));
    std::fs::write(&temporary, data).map_err(io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600)).map_err(io)?;
    }
    std::fs::rename(&temporary, &path).map_err(io)?;
    prune(dir);
    Ok(path)
}

/// A device name as a file name: letters and digits of any script, `-` and
/// `_`; anything else becomes `_`.
fn file_name(sender: &str) -> String {
    let name: String = sender
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(40)
        .collect();
    if name.trim_matches('_').is_empty() {
        "device".into()
    } else {
        name
    }
}

/// `YYYYMMDD-HHMMSSZ` in UTC.
fn utc_stamp(now: SystemTime) -> String {
    let secs = now.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rest) = ((secs / 86_400) as i64, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

/// Keeps the newest KEEP reports; touches only the files this module wrote.
fn prune(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut reports: Vec<(SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            !name.starts_with('.') && name.ends_with(".jsonl")
        })
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    if reports.len() <= KEEP {
        return;
    }
    reports.sort();
    for (_, path) in &reports[..reports.len() - KEEP] {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn saves_a_named_report_and_refuses_unknown_or_oversized_data() {
        let dir = tempfile::tempdir().unwrap();
        let at = UNIX_EPOCH + Duration::from_secs(1_791_072_003); // 2026-10-04 00:00:03 UTC
        let path = save(dir.path(), "Tester's iPhone", "performance", b"{}\n", at).unwrap();
        assert_eq!(
            path.file_name().unwrap(),
            "Tester_s_iPhone-20261004-000003Z-performance.jsonl"
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"{}\n");
        let again = save(dir.path(), "Tester's iPhone", "performance", b"x\n", at).unwrap();
        assert!(again.to_string_lossy().ends_with("-performance-2.jsonl"));
        assert!(save(dir.path(), "p", "secrets", b"x", at).is_err());
        assert!(save(dir.path(), "p", "performance", b"", at).is_err());
        let big = vec![b'x'; muster_proto::MAX_DIAGNOSTICS + 1];
        assert!(save(dir.path(), "p", "performance", &big, at).is_err());
    }

    #[test]
    fn names_are_file_safe_and_dates_are_utc() {
        assert_eq!(file_name("../../etc"), "______etc");
        assert_eq!(file_name("华为 Mate"), "华为_Mate");
        assert_eq!(file_name("  "), "device");
        assert_eq!(utc_stamp(UNIX_EPOCH), "19700101-000000Z");
        assert_eq!(
            utc_stamp(UNIX_EPOCH + Duration::from_secs(951_782_400)),
            "20000229-000000Z"
        );
    }

    #[test]
    fn keeps_only_the_newest_reports() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("notes.txt"), "kept").unwrap();
        for n in 0..KEEP + 3 {
            let at = UNIX_EPOCH + Duration::from_secs(1_800_000_000 + n as u64);
            save(dir.path(), "phone", "performance", b"x\n", at).unwrap();
        }
        let reports = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".jsonl"))
            .count();
        assert_eq!(reports, KEEP);
        assert!(dir.path().join("notes.txt").exists());
    }
}
