//! The file browser and git log behind a session's cwd. Every path is
//! resolved inside the cwd; anything escaping it is refused.

use std::path::{Component, Path, PathBuf};

use muster_proto::v1::{FileContent, FileEntry, FileList, GitCommit, GitLog};

use crate::Error;

const DEFAULT_MAX_BYTES: u64 = 512 * 1024;
const HARD_MAX_BYTES: u64 = 8 * 1024 * 1024;

fn confine(root: &Path, rel: &str) -> Result<PathBuf, Error> {
    let rel = rel.trim_start_matches('/');
    let mut out = root.to_path_buf();
    for component in Path::new(rel).components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => return Err(Error::Invalid("path escapes the session directory".into())),
        }
    }
    // Symlinks pointing outside are refused too.
    if out.exists() {
        let canonical = out.canonicalize().map_err(|e| Error::Io(e.to_string()))?;
        let root_canonical = root.canonicalize().map_err(|e| Error::Io(e.to_string()))?;
        if !canonical.starts_with(&root_canonical) {
            return Err(Error::Invalid("path escapes the session directory".into()));
        }
    }
    Ok(out)
}

pub async fn list(root: &Path, rel: &str) -> Result<FileList, Error> {
    let dir = confine(root, rel)?;
    let mut entries = Vec::new();
    let mut rd = tokio::fs::read_dir(&dir)
        .await
        .map_err(|e| Error::Io(e.to_string()))?;
    while let Ok(Some(entry)) = rd.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        let meta = match entry.metadata().await {
            Ok(m) => m,
            Err(_) => continue,
        };
        entries.push(FileEntry {
            name,
            dir: meta.is_dir(),
            size: meta.len(),
            modified_unix: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0),
        });
    }
    entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.cmp(&b.name)));
    Ok(FileList {
        path: rel.to_string(),
        entries,
    })
}

pub async fn read(root: &Path, rel: &str, max_bytes: u64) -> Result<FileContent, Error> {
    let path = confine(root, rel)?;
    let max = if max_bytes == 0 {
        DEFAULT_MAX_BYTES
    } else {
        max_bytes.min(HARD_MAX_BYTES)
    };
    let meta = tokio::fs::metadata(&path)
        .await
        .map_err(|e| Error::Io(e.to_string()))?;
    if meta.is_dir() {
        return Err(Error::Invalid("is a directory".into()));
    }
    let mut file = tokio::fs::File::open(&path)
        .await
        .map_err(|e| Error::Io(e.to_string()))?;
    let mut content = Vec::with_capacity(meta.len().min(max) as usize);
    use tokio::io::AsyncReadExt;
    (&mut file)
        .take(max)
        .read_to_end(&mut content)
        .await
        .map_err(|e| Error::Io(e.to_string()))?;
    Ok(FileContent {
        path: rel.to_string(),
        truncated: meta.len() > max,
        content,
    })
}

pub async fn git(root: &Path, limit: u32, diff_commit: &str) -> Result<GitLog, Error> {
    if !diff_commit.is_empty() {
        if !diff_commit.chars().all(|c| c.is_ascii_hexdigit())
            || diff_commit.len() < 7
            || diff_commit.len() > 64
        {
            return Err(Error::Invalid("commit must be a hex hash".into()));
        }
        let out = tokio::process::Command::new("git")
            .args(["show", "--stat", "--patch", "--no-color", diff_commit])
            .current_dir(root)
            .output()
            .await
            .map_err(|e| Error::Io(e.to_string()))?;
        if !out.status.success() {
            return Err(Error::Io(
                String::from_utf8_lossy(&out.stderr).trim().to_string(),
            ));
        }
        let mut diff = String::from_utf8_lossy(&out.stdout).into_owned();
        clip_diff(&mut diff, 2 * 1024 * 1024);
        return Ok(GitLog {
            commits: Vec::new(),
            diff,
        });
    }
    let limit = if limit == 0 { 50 } else { limit.min(500) };
    let out = tokio::process::Command::new("git")
        .args([
            "log",
            "--no-color",
            &format!("-n{limit}"),
            "--format=%H%x1f%an%x1f%at%x1f%s",
        ])
        .current_dir(root)
        .output()
        .await
        .map_err(|e| Error::Io(e.to_string()))?;
    if !out.status.success() {
        return Err(Error::Io(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    let commits = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\x1f');
            Some(GitCommit {
                hash: parts.next()?.to_string(),
                author: parts.next()?.to_string(),
                ts_unix: parts.next()?.parse().unwrap_or(0),
                subject: parts.next().unwrap_or("").to_string(),
            })
        })
        .collect();
    Ok(GitLog {
        commits,
        diff: String::new(),
    })
}

/// Clips a diff to at most `limit` bytes, on a character boundary
/// (`String::truncate` panics inside one), and says so at the end.
fn clip_diff(diff: &mut String, limit: usize) {
    if diff.len() <= limit {
        return;
    }
    let mut cut = limit;
    while !diff.is_char_boundary(cut) {
        cut -= 1;
    }
    diff.truncate(cut);
    diff.push_str("\n… (truncated)");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clipped_diff_never_splits_a_character() {
        let mut diff = "é".repeat(10);
        clip_diff(&mut diff, 5);
        assert_eq!(diff, "éé\n… (truncated)");
        let mut short = "abc".to_string();
        clip_diff(&mut short, 5);
        assert_eq!(short, "abc");
    }

    #[tokio::test]
    async fn paths_stay_inside_the_root() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/a.txt"), b"hello").unwrap();
        let listed = list(dir.path(), "sub").await.unwrap();
        assert_eq!(listed.entries.len(), 1);
        assert_eq!(listed.entries[0].name, "a.txt");
        let content = read(dir.path(), "sub/a.txt", 3).await.unwrap();
        assert_eq!(content.content, b"hel");
        assert!(content.truncated);
        assert!(list(dir.path(), "../").await.is_err());
        assert!(read(dir.path(), "/etc/passwd", 0).await.is_err());
        assert!(read(dir.path(), "sub/../../x", 0).await.is_err());
    }
}
