//! Running processes by executable path, through libproc. `pgrep` does not
//! see every GUI process here (the Claude desktop app's main process among
//! them), so the bridge asks the kernel directly.

use std::path::Path;

/// Pids whose executable path ends with `suffix` (e.g.
/// `Claude.app/Contents/MacOS/Claude`).
pub fn pids_by_executable_suffix(suffix: &str) -> Vec<u32> {
    let mut out = Vec::new();
    // SAFETY: a null buffer asks for the size only.
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    if count <= 0 {
        return out;
    }
    let mut pids = vec![0i32; count as usize + 64];
    let bytes = (pids.len() * std::mem::size_of::<i32>()) as i32;
    // SAFETY: the buffer is valid for `bytes` bytes.
    let got = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
    if got <= 0 {
        return out;
    }
    let mut path = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    for &pid in &pids[..got as usize] {
        if pid <= 0 {
            continue;
        }
        // SAFETY: the buffer is valid for its length; the call writes a NUL-terminated path.
        let len = unsafe { libc::proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32) };
        if len <= 0 {
            continue;
        }
        let text = String::from_utf8_lossy(&path[..len as usize]);
        if Path::new(text.as_ref())
            .to_str()
            .is_some_and(|p| p.ends_with(suffix))
        {
            out.push(pid as u32);
        }
    }
    out
}

/// Observe files held by the signed Codex desktop's own worker. This does not
/// invoke a provider CLI or confer an Accessibility controller.
pub async fn codex_runtime_files() -> Vec<(
    muster_agent::controller::ProcessIdentity,
    std::path::PathBuf,
)> {
    use super::codesign;
    use muster_agent::controller::ProcessIdentity;
    use std::time::Duration;
    let output = match tokio::time::timeout(
        Duration::from_secs(2),
        tokio::process::Command::new("/bin/ps")
            .args(["-axo", "pid=,ppid=,comm="])
            .kill_on_drop(true)
            .output(),
    )
    .await
    {
        Ok(Ok(o)) if o.status.success() => o,
        _ => return Vec::new(),
    };
    let mut rows = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let mut fields = line.trim().splitn(2, char::is_whitespace);
        let Some(pid) = fields.next().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let Some(rest) = fields.next() else { continue };
        let mut fields = rest.trim().splitn(2, char::is_whitespace);
        let Some(parent) = fields.next().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        rows.push((pid, parent, fields.next().unwrap_or("").trim().to_string()));
    }
    let roots: Vec<_> = rows
        .iter()
        .filter(|(pid, _, path)| {
            path.ends_with("ChatGPT.app/Contents/MacOS/ChatGPT")
                && codesign::process_satisfies(*pid, &muster_core::apps::CODEX.requirement())
        })
        .map(|(pid, _, _)| *pid)
        .collect();
    let workers: Vec<_> = rows
        .iter()
        .filter(|(pid, parent, path)| {
            roots.contains(parent)
                && path.contains("/ChatGPT.app/Contents/")
                && path.ends_with("/codex")
                && codesign::process_satisfies(
                    *pid,
                    &format!(
                        "anchor apple generic and certificate leaf[subject.OU] = \"{}\"",
                        muster_core::apps::CODEX.team_id
                    ),
                )
        })
        .filter_map(|(pid, _, _)| {
            codesign::process_start(*pid).map(|start| ProcessIdentity { pid: *pid, start })
        })
        .collect();
    if workers.is_empty() {
        return Vec::new();
    }
    let pids = workers
        .iter()
        .map(|p| p.pid.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let output = match tokio::time::timeout(
        Duration::from_secs(2),
        tokio::process::Command::new("/usr/sbin/lsof")
            .args(["-n", "-P", "-a", "-p", &pids, "-Fpn"])
            .kill_on_drop(true)
            .output(),
    )
    .await
    {
        Ok(Ok(o)) => o,
        _ => return Vec::new(),
    };
    let live: Vec<_> = workers
        .into_iter()
        .filter(|p| codesign::process_start(p.pid) == Some(p.start))
        .collect();
    let mut process = None;
    let mut files = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(pid) = line.strip_prefix('p').and_then(|s| s.parse::<u32>().ok()) {
            process = live.iter().find(|p| p.pid == pid).copied();
        } else if let Some(path) = line.strip_prefix('n') {
            if let Some(identity) = process
                && path.ends_with(".jsonl")
            {
                files.push((identity, std::path::PathBuf::from(path)));
            }
        }
    }
    files
}

/// Read Claude's per-worker registration and validate the actual launch and signer.
/// The signed GUI remaining alive is not evidence that a session worker exists.
pub fn claude_runtime_sessions() -> Vec<(
    muster_agent::claude_runtime::Registration,
    muster_agent::controller::ProcessState,
)> {
    use muster_agent::controller::ProcessState;
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let root = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(home).join(".claude"))
        .join("sessions");
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten().take(4096) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(pid) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        if !entry
            .metadata()
            .is_ok_and(|m| m.is_file() && m.len() <= 65536)
        {
            continue;
        }
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let Some(registration) = muster_agent::claude_runtime::parse(&bytes, pid) else {
            continue;
        };
        let state = match super::codesign::observe_process_start(pid) {
            Ok(Some(start)) if start / 1_000_000 == registration.process.start / 1_000_000 => {
                if !super::codesign::process_satisfies(pid, crate::requirements::CLAUDE_CLI) {
                    continue;
                }
                ProcessState::Running
            }
            Ok(_) => ProcessState::Ended,
            Err(_) => ProcessState::Unknown,
        };
        out.push((registration, state));
    }
    out
}
