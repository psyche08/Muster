//! Where a CLI session runs, prepared before its first child starts: an
//! optional Git worktree of the requested repository, and an optional record
//! that Claude Code trusts the session directory.
//!
//! Both are on the caller's request only (`OrcaCreateRequest.worktree` and
//! `.trust`); a plain create still runs in the directory it names and writes
//! nothing outside Muster's own store.
use super::{Error, unavailable};
use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

/// Worktrees live where Claude Code's desktop app puts its own, so a person
/// finds them in one place whichever tool opened them.
const WORKTREES: &str = ".claude/worktrees";

/// The permission modes a CLI session may ask for. Anything that skips or
/// pre-approves every prompt (`bypassPermissions`, `dontAsk`) is refused:
/// a remote caller must not be able to switch the device's approvals off.
pub(super) fn permission_mode(agent: &str, mode: &str) -> Result<String, Error> {
    match mode {
        "" | "default" => Ok(String::new()),
        // Claude's classifier approves what it judges safe and still asks
        // for the rest, through the same prompt path as the default mode.
        "auto" if agent == "claude" => Ok("auto".into()),
        "auto" => Err(Error::Invalid(
            "Permission mode auto applies to Claude sessions only".into(),
        )),
        _ => Err(Error::Invalid(
            "Permission mode must be default or auto".into(),
        )),
    }
}

/// A worktree name becomes one path component under `.claude/worktrees`.
pub(super) fn check_name(name: &str) -> Result<(), Error> {
    let valid = !name.is_empty()
        && name.len() <= 64
        && !name.starts_with(['.', '-'])
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if valid {
        Ok(())
    } else {
        Err(Error::Invalid(
            "Worktree names are 1-64 of [A-Za-z0-9._-] and do not start with . or -".into(),
        ))
    }
}

/// Open the worktree `name` of the repository that holds `directory`, at
/// `<main checkout>/.claude/worktrees/<name>`. An existing worktree of the
/// same repository is reused as it is; a new one is added detached at the
/// main checkout's `HEAD`. Muster never fetches: the session brings the
/// worktree to the commit it needs.
pub(super) fn worktree(directory: &Path, name: &str) -> Result<PathBuf, Error> {
    check_name(name)?;
    let listed = git(directory, &["worktree", "list", "--porcelain"])?;
    let main = main_checkout(&listed)?;
    let path = main.join(WORKTREES).join(name);
    if path.exists() {
        let path = canonical(&path)?;
        if !registered(&listed, &path) {
            return Err(Error::Conflict(format!(
                "{} exists and is not a worktree of this repository",
                path.display()
            )));
        }
        ignore(&main)?;
        return Ok(path);
    }
    std::fs::create_dir_all(main.join(WORKTREES))
        .map_err(|_| unavailable("Could not create the worktree directory"))?;
    let target = path
        .to_str()
        .ok_or_else(|| Error::Invalid("Worktree path must be UTF-8".into()))?;
    let added = git(
        &main,
        &["worktree", "add", "--detach", "--quiet", target, "HEAD"],
    );
    if let Err(error) = added {
        // Another create may have added the same worktree a moment ago.
        let again = git(&main, &["worktree", "list", "--porcelain"])?;
        if !path.exists() || !registered(&again, &canonical(&path)?) {
            return Err(error);
        }
    }
    ignore(&main)?;
    canonical(&path)
}

fn canonical(path: &Path) -> Result<PathBuf, Error> {
    std::fs::canonicalize(path).map_err(|_| unavailable("Worktree directory unavailable"))
}

/// The first entry of `git worktree list` is the main checkout; a bare
/// repository has none to put worktrees beside.
fn main_checkout(listed: &str) -> Result<PathBuf, Error> {
    let mut entry = listed.split("\n\n").next().unwrap_or_default().lines();
    let path = entry
        .next()
        .and_then(|line| line.strip_prefix("worktree "))
        .ok_or_else(|| unavailable("Git did not name the repository's main checkout"))?;
    if entry.any(|line| line == "bare") {
        return Err(Error::Invalid(
            "A bare repository has no checkout to hold worktrees".into(),
        ));
    }
    canonical(Path::new(path))
}

fn registered(listed: &str, path: &Path) -> bool {
    listed
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .any(|listed| std::fs::canonicalize(listed).is_ok_and(|p| p == path))
}

/// Keep the worktrees out of the main checkout's `git status`, the way
/// Claude Code does: one line in the repository's own `info/exclude`, added
/// only when nothing already ignores them.
fn ignore(main: &Path) -> Result<(), Error> {
    let ignored = Command::new("git")
        .arg("-C")
        .arg(main)
        .args(["check-ignore", "-q", WORKTREES])
        .output()
        .map_err(|_| unavailable("Git is not available on this device"))?;
    if ignored.status.success() {
        return Ok(());
    }
    let common = git(
        main,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let info = Path::new(common.trim()).join("info");
    std::fs::create_dir_all(&info)
        .map_err(|_| unavailable("Could not open the repository's info/exclude"))?;
    let exclude = info.join("exclude");
    let before = std::fs::read(&exclude).unwrap_or_default();
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&exclude)
        .map_err(|_| unavailable("Could not open the repository's info/exclude"))?;
    let lead = if before.is_empty() || before.ends_with(b"\n") {
        ""
    } else {
        "\n"
    };
    writeln!(file, "{lead}/{WORKTREES}/")
        .map_err(|_| unavailable("Could not update the repository's info/exclude"))
}

fn git(dir: &Path, args: &[&str]) -> Result<String, Error> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|_| unavailable("Git is not available on this device"))?;
    if !out.status.success() {
        let said = String::from_utf8_lossy(&out.stderr);
        let first = said.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        let first: String = first.chars().take(300).collect();
        return Err(if first.contains("not a git repository") {
            Error::Invalid("A worktree needs a directory inside a Git repository".into())
        } else {
            unavailable(&format!("git {} failed: {first}", args[0]))
        });
    }
    String::from_utf8(out.stdout).map_err(|_| unavailable("Git output was not UTF-8"))
}

/// Claude Code's global config, the file that records trusted folders:
/// `$CLAUDE_CONFIG_DIR/.claude.json`, else `~/.claude.json`. The session's
/// own `CLAUDE_CONFIG_DIR` wins over the daemon's, as it does in the child.
pub(super) fn claude_config(session_dir: Option<&str>) -> Result<PathBuf, Error> {
    let dir = session_dir
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from))
        .or_else(std::env::home_dir)
        .ok_or_else(|| unavailable("No home directory for Claude's config"))?;
    // While a non-empty legacy `.config.json` exists Claude reads that
    // instead, and a trust record written beside it would not apply.
    let home = session_dir
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from))
        .or_else(|| std::env::home_dir().map(|h| h.join(".claude")));
    let legacy = home.is_some_and(|home| {
        std::fs::metadata(home.join(".config.json")).is_ok_and(|m| m.len() > 0)
    });
    if legacy {
        return Err(unavailable(
            "Claude Code still uses its legacy .config.json here; open Claude once to migrate it",
        ));
    }
    Ok(dir.join(".claude.json"))
}

/// How long Claude Code's config lock may be busy before a trust write gives
/// up, and how old it must be to count as abandoned. Claude takes it with
/// proper-lockfile, whose default stale age is ten seconds and which renews a
/// held lock well inside that.
const LOCK_WAIT: Duration = Duration::from_secs(5);
const LOCK_STALE: Duration = Duration::from_secs(10);

/// Record `dir` as trusted in Claude Code's config, as accepting its trust
/// dialog there would: `projects[dir].hasTrustDialogAccepted = true`.
///
/// Claude writes this file itself, often, from every running instance. The
/// write here follows its protocol so neither loses the other's change: it
/// holds the same `<config>.lock` directory lock Claude takes for its own
/// saves, re-reads the file under it, and replaces the file atomically.
/// Every other key and value keeps its text and its order.
pub(super) fn trust(config: &Path, dir: &str) -> Result<(), Error> {
    let lock = lock(config)?;
    let result = write_trust(config, dir);
    drop(lock);
    result
}

struct Lock(PathBuf);
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir(&self.0);
    }
}

fn lock(config: &Path) -> Result<Lock, Error> {
    let mut path = config.as_os_str().to_owned();
    path.push(".lock");
    let path = PathBuf::from(path);
    let deadline = SystemTime::now() + LOCK_WAIT;
    loop {
        match std::fs::create_dir(&path) {
            Ok(()) => return Ok(Lock(path)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let stale = std::fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|age| age > LOCK_STALE);
                if stale {
                    // What proper-lockfile itself does with an abandoned lock.
                    let _ = std::fs::remove_dir(&path);
                    continue;
                }
                if SystemTime::now() > deadline {
                    return Err(unavailable(
                        "Claude Code is holding its config lock; try again",
                    ));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return Err(unavailable("Could not lock Claude's config")),
        }
    }
}

fn write_trust(config: &Path, dir: &str) -> Result<(), Error> {
    // Claude may keep the file behind a symlink; replace what it points at.
    let target = std::fs::canonicalize(config).unwrap_or_else(|_| config.to_owned());
    let text = match std::fs::read_to_string(&target) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(_) => return Err(unavailable("Could not read Claude's config")),
    };
    let Some(updated) = trusted(&text, dir)? else {
        return Ok(());
    };
    let parent = target
        .parent()
        .ok_or_else(|| unavailable("Claude's config has no directory"))?;
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(&target)
            .map(|m| m.permissions().mode() & 0o777)
            .unwrap_or(0o600)
    };
    let temp = parent.join(format!(".claude.json.muster-{}", super::random()));
    let staged = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(mode);
        }
        let mut file = options.open(&temp)?;
        file.write_all(updated.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temp, &target)
    })();
    if staged.is_err() {
        let _ = std::fs::remove_file(&temp);
        return Err(unavailable("Could not replace Claude's config"));
    }
    Ok(())
}

/// The config text with `dir` trusted, or `None` when it already is.
fn trusted(text: &str, dir: &str) -> Result<Option<String>, Error> {
    let corrupt = || unavailable("Claude's config is not a JSON object; it was left unchanged");
    let mut root: Ordered = if text.trim().is_empty() {
        Ordered::default()
    } else {
        serde_json::from_str(text).map_err(|_| corrupt())?
    };
    let mut projects: Ordered = match root.get("projects") {
        Some(raw) => serde_json::from_str(raw.get()).map_err(|_| corrupt())?,
        None => Ordered::default(),
    };
    let mut project: Ordered = match projects.get(dir) {
        Some(raw) => serde_json::from_str(raw.get()).map_err(|_| corrupt())?,
        None => Ordered::default(),
    };
    if project
        .get("hasTrustDialogAccepted")
        .is_some_and(|v| v.get() == "true")
    {
        return Ok(None);
    }
    project.set("hasTrustDialogAccepted", Node::raw("true"));
    projects.set(dir, Node::Object(project));
    root.set("projects", Node::Object(projects));
    let mut out = serde_json::to_string_pretty(&root).map_err(|_| corrupt())?;
    if text.ends_with('\n') || text.trim().is_empty() {
        out.push('\n');
    }
    Ok(Some(out))
}

/// A JSON object read without reordering it or re-encoding its values.
#[derive(Default)]
struct Ordered(Vec<(String, Node)>);
enum Node {
    Raw(Box<RawValue>),
    Object(Ordered),
}
impl Node {
    fn raw(text: &str) -> Self {
        Node::Raw(RawValue::from_string(text.to_owned()).expect("valid JSON literal"))
    }
}
impl Ordered {
    /// The last value under `key`, which is the one a JSON reader keeps.
    fn get(&self, key: &str) -> Option<&RawValue> {
        self.0
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .and_then(|(_, v)| match v {
                Node::Raw(raw) => Some(raw.as_ref()),
                Node::Object(_) => None,
            })
    }
    fn set(&mut self, key: &str, value: Node) {
        match self.0.iter_mut().rev().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = value,
            None => self.0.push((key.to_owned(), value)),
        }
    }
}
impl<'de> Deserialize<'de> for Ordered {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Fields;
        impl<'de> Visitor<'de> for Fields {
            type Value = Ordered;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Ordered, A::Error> {
                let mut fields = Vec::new();
                while let Some((key, value)) = map.next_entry::<String, Box<RawValue>>()? {
                    fields.push((key, Node::Raw(value)));
                }
                Ok(Ordered(fields))
            }
        }
        d.deserialize_map(Fields)
    }
}
impl Serialize for Ordered {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}
impl Serialize for Node {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Node::Raw(raw) => raw.serialize(s),
            Node::Object(object) => object.serialize(s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_keeps_every_other_key_and_its_order() {
        let text = "{\n  \"zeta\": 1,\n  \"projects\": {\n    \"/b\": {\n      \"z\": [1, 2],\n      \"hasTrustDialogAccepted\": false\n    },\n    \"/a\": {\n      \"keep\": \"x\"\n    }\n  },\n  \"alpha\": {\"nested\": true}\n}\n";
        let out = trusted(text, "/b").unwrap().unwrap();
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(value["projects"]["/b"]["hasTrustDialogAccepted"], true);
        assert_eq!(value["projects"]["/b"]["z"], serde_json::json!([1, 2]));
        assert_eq!(value["projects"]["/a"]["keep"], "x");
        assert_eq!(value["alpha"]["nested"], true);
        // Order and untouched values survive as written.
        assert!(out.find("\"zeta\"").unwrap() < out.find("\"projects\"").unwrap());
        assert!(out.find("\"projects\"").unwrap() < out.find("\"alpha\"").unwrap());
        assert!(out.find("\"/b\"").unwrap() < out.find("\"/a\"").unwrap());
        assert!(out.contains("{\"nested\": true}"));
        assert!(out.ends_with("}\n"));
    }

    #[test]
    fn trust_adds_a_project_and_leaves_a_trusted_one_alone() {
        let out = trusted("{\"numStartups\": 3}", "/w").unwrap().unwrap();
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(value["numStartups"], 3);
        assert_eq!(value["projects"]["/w"]["hasTrustDialogAccepted"], true);
        assert!(trusted(&out, "/w").unwrap().is_none());
        assert!(trusted("", "/w").unwrap().is_some());
    }

    #[test]
    fn trust_refuses_a_config_it_cannot_read_as_an_object() {
        for text in ["[]", "{\"projects\": []}", "not json"] {
            assert!(trusted(text, "/w").is_err(), "{text}");
        }
    }

    #[test]
    fn trust_waits_out_a_live_lock_and_clears_an_abandoned_one() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join(".claude.json");
        std::fs::write(&config, "{}").unwrap();
        let lock = dir.path().join(".claude.json.lock");
        std::fs::create_dir(&lock).unwrap();
        // Abandoned: older than proper-lockfile's stale age.
        let old = SystemTime::now() - Duration::from_secs(60);
        std::fs::File::open(&lock)
            .unwrap()
            .set_modified(old)
            .unwrap();
        trust(&config, "/w").unwrap();
        assert!(!lock.exists());
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&config).unwrap()).unwrap();
        assert_eq!(value["projects"]["/w"]["hasTrustDialogAccepted"], true);
    }

    #[test]
    fn only_default_and_auto_permission_modes_are_accepted() {
        assert_eq!(permission_mode("claude", "").unwrap(), "");
        assert_eq!(permission_mode("claude", "default").unwrap(), "");
        assert_eq!(permission_mode("claude", "auto").unwrap(), "auto");
        assert!(permission_mode("codex", "auto").is_err());
        for mode in [
            "bypassPermissions",
            "dontAsk",
            "acceptEdits",
            "plan",
            "Auto",
        ] {
            assert!(permission_mode("claude", mode).is_err(), "{mode}");
        }
    }

    #[test]
    fn worktree_names_are_one_plain_path_component() {
        for name in ["pr-mt-safe-net-1464", "a", "x.y_z"] {
            assert!(check_name(name).is_ok(), "{name}");
        }
        for name in [
            "",
            ".",
            "..",
            "-x",
            ".hidden",
            "a/b",
            "a b",
            &"x".repeat(65),
        ] {
            assert!(check_name(name).is_err(), "{name}");
        }
    }
}
