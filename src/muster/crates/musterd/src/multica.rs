//! Local-owner Apple container lifecycle; credentials never cross the app ABI.
use crate::Error;
use muster_proto::v1;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
mod bridge;
mod browse;
mod installation;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Config {
    data_dir: String,
    installer_url: String,
    server_url: String,
    app_url: String,
    keep_alive: bool,
    container_name: String,
    managed: bool,
    runtime_bridge: bool,
    installed: bool,
    // Stable shared path retained after a physical data-directory migration.
    mount_dir: String,
}
pub struct Manager {
    installation: installation::Installation,
    path: PathBuf,
    executable: Option<PathBuf>,
    gate: tokio::sync::Mutex<()>,
    browse_gate: tokio::sync::Semaphore,
    bridge: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    last_error: std::sync::Mutex<String>,
    // Tests supply the host Multica CLI so installation can be exercised
    // without touching an operator's ~/.local/bin.
    #[cfg(test)]
    host_cli_candidates: Option<Vec<PathBuf>>,
}
impl Drop for Manager {
    fn drop(&mut self) {
        if let Some(task) = self.bridge.get_mut().take() {
            task.abort();
        }
    }
}
impl Manager {
    pub fn new(root: &Path) -> Self {
        Self {
            installation: Default::default(),
            path: root.join("multica.json"),
            executable: None,
            gate: tokio::sync::Mutex::new(()),
            browse_gate: tokio::sync::Semaphore::new(2),
            bridge: tokio::sync::Mutex::new(None),
            last_error: std::sync::Mutex::new(String::new()),
            #[cfg(test)]
            host_cli_candidates: None,
        }
    }
    async fn command(&self, args: &[&str], seconds: u64) -> Result<Vec<u8>, Error> {
        let executable = self.executable.clone().map(Ok).unwrap_or_else(binary)?;
        run(&executable, args, seconds).await
    }
    async fn install_command(&self, args: &[&str], seconds: u64) -> Result<(), Error> {
        let executable = self.executable.clone().map(Ok).unwrap_or_else(binary)?;
        self.installation
            .run(tokio::process::Command::new(executable).args(args), seconds)
            .await
    }
    fn load(&self) -> Result<Config, Error> {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|_| Error::Invalid("Multica configuration is unreadable".into())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config {
                data_dir: default_root(),
                container_name: "muster-multica".into(),
                ..Default::default()
            }),
            Err(e) => Err(io(e)),
        }
    }
    fn save(&self, config: &Config) -> Result<(), Error> {
        let bytes = serde_json::to_vec(config).map_err(|e| Error::Invalid(e.to_string()))?;
        let temp = self.path.with_extension("json.tmp");
        private_write(&temp, &bytes)?;
        std::fs::rename(temp, &self.path).map_err(io)
    }
    async fn observed(&self, mut c: Config) -> Result<(Config, Option<serde_json::Value>), Error> {
        let list = self
            .command(&["list", "--all", "--format", "json"], 10)
            .await?;
        let rows: Vec<serde_json::Value> = serde_json::from_slice(&list)
            .map_err(|_| Error::Unavailable("Could not decode Apple container inventory".into()))?;
        let row = rows
            .iter()
            .find(|r| r["id"].as_str() == Some(c.container_name.as_str()))
            .or_else(|| {
                if !self.path.exists() {
                    rows.iter().find(|r| r["id"] == "mlt")
                } else {
                    None
                }
            })
            .cloned();
        if let Some(row) = &row {
            c.container_name = row["id"].as_str().unwrap_or_default().into();
            if !self.path.exists() {
                if let Some(mount) = row["configuration"]["mounts"]
                    .as_array()
                    .and_then(|m| m.iter().find(|m| m["source"] == m["destination"]))
                {
                    c.data_dir = mount["source"].as_str().unwrap_or(&c.data_dir).to_owned();
                    c.mount_dir = c.data_dir.clone();
                }
            }
        }
        if row
            .as_ref()
            .is_some_and(|r| r["status"]["state"] == "running")
            && (c.app_url.is_empty() || c.server_url.is_empty())
            && let Ok(output) = self
                .command(
                    &[
                        "exec",
                        &c.container_name,
                        "multica",
                        "config",
                        "get",
                        "app_url",
                    ],
                    5,
                )
                .await
        {
            let text = String::from_utf8_lossy(&output);
            for line in text.lines() {
                if let Some(value) = line.trim().strip_prefix("app_url:")
                    && c.app_url.is_empty()
                    && valid_https(value.trim())
                {
                    c.app_url = value.trim().into();
                }
                if let Some(value) = line.trim().strip_prefix("server_url:")
                    && c.server_url.is_empty()
                    && valid_https(value.trim())
                {
                    c.server_url = value.trim().into();
                }
            }
        }
        Ok((c, row))
    }
    pub async fn request(
        &self,
        r: v1::MulticaRequest,
        socket: &Path,
    ) -> Result<v1::MulticaStatus, Error> {
        if !cfg!(target_os = "macos") {
            return Ok(v1::MulticaStatus::default());
        }
        let action = v1::multica_request::Action::try_from(r.action)
            .map_err(|_| Error::Invalid("Unknown Multica action".into()))?;
        if action == v1::multica_request::Action::Progress {
            return Ok(v1::MulticaStatus {
                installation: Some(self.installation.snapshot()),
                ..Default::default()
            });
        }
        // CHECK may recover an in-flight installation without waiting on its gate.
        if action == v1::multica_request::Action::Check {
            return self.status().await;
        }
        let _guard = self
            .gate
            .try_lock()
            .map_err(|_| Error::Busy("Multica setup is already running".into()))?;
        let installing = action == v1::multica_request::Action::Install;
        let _attempt = installing.then(|| self.installation.begin());
        let mut result = self.request_inner(r, socket, action).await;
        if installing {
            let error = match &result {
                Err(error) => Some(error.to_string()),
                Ok(_) => {
                    let error = self.last_error.lock().unwrap().clone();
                    (!error.is_empty()).then_some(error)
                }
            };
            self.installation.finish(error.as_deref());
            // Even validation failures return their completed diagnostics to the UI.
            if result.is_err() {
                result = self.status().await;
            }
            if let Ok(status) = &mut result {
                status.installation = Some(self.installation.snapshot());
                status.busy = false;
                if let Some(error) = error {
                    status.message = error;
                }
            }
        }
        result
    }
    async fn request_inner(
        &self,
        r: v1::MulticaRequest,
        socket: &Path,
        action: v1::multica_request::Action,
    ) -> Result<v1::MulticaStatus, Error> {
        *self.last_error.lock().unwrap() = String::new();
        if action != v1::multica_request::Action::Check {
            let current = self.load()?;
            // Install/opt-in may start Apple's runtime. A read-only CHECK never does.
            let (old, row) = match self.observed(current.clone()).await {
                Ok(observed) => observed,
                Err(error) if action == v1::multica_request::Action::Install || r.keep_alive => {
                    self.command(&["system", "start"], 60)
                        .await
                        .map_err(|_| error)?;
                    self.observed(current).await?
                }
                Err(error) => return Err(error),
            };
            let mut c = validate(r, &old)?;
            if action == v1::multica_request::Action::Install {
                c.runtime_bridge = true;
            }
            if row.is_some() && c.container_name != old.container_name {
                return Err(Error::Invalid("Keep the installed container name".into()));
            }
            if let Some(row) = row.as_ref()
                && c.data_dir != old.data_dir
            {
                if row["status"]["state"] == "running" {
                    return Err(Error::Conflict("Turn off keep-alive and stop the container before moving its data directory".into()));
                }
                accept_migrated_root(&old, &c, row)?;
                c.mount_dir = c.data_dir.clone();
            }
            if action == v1::multica_request::Action::Install
                && (row.is_none() || (c.managed && !c.installed))
            {
                if c.installer_url.is_empty() {
                    return Err(Error::Invalid(
                        "Enter the trusted HTTPS Multica installer URL".into(),
                    ));
                }
                c.managed = true;
                c.mount_dir = c.data_dir.clone();
                self.save(&c)?;
                if let Err(error) = install(self, &c, row.is_none()).await {
                    *self.last_error.lock().unwrap() = error.to_string();
                    return self.status().await;
                }
                c.installed = true;
                self.save(&c)?;
            } else {
                if row
                    .as_ref()
                    .is_some_and(|r| r["status"]["state"] == "running")
                {
                    c.installed = self
                        .command(
                            &[
                                "exec",
                                &c.container_name,
                                "test",
                                "-x",
                                "/usr/local/bin/multica",
                            ],
                            5,
                        )
                        .await
                        .is_ok();
                    if !c.managed && c.keep_alive && daemon_running(self, &c).await {
                        // Keep the legacy bridge environment inside its own container.
                        self.command(&["exec", &c.container_name, "sh", "-c",
                            "umask 077; p=$(cat /root/.multica/daemon.pid); case $p in ''|*[!0-9]*) exit 1;; esac; awk 'BEGIN{RS=\"\\0\";ORS=\"\\0\"} /^(MUSTER_BRIDGE_(TOKEN|HOST|PORT|VERSION)|MULTICA_WORKSPACES_ROOT)=/' /proc/$p/environ > /root/.multica/muster-environment"], 5).await?;
                    }
                }
                self.save(&c)?;
                if row
                    .as_ref()
                    .is_some_and(|r| r["status"]["state"] == "running")
                {
                    for (key, value) in [("server_url", &c.server_url), ("app_url", &c.app_url)] {
                        if !value.is_empty() {
                            self.command(
                                &[
                                    "exec",
                                    &c.container_name,
                                    "multica",
                                    "config",
                                    "set",
                                    key,
                                    value,
                                ],
                                15,
                            )
                            .await?;
                        }
                    }
                }
            }
            if action == v1::multica_request::Action::Install && !c.managed {
                self.installation.stage("检查已有容器依赖");
                self.install_command(&["exec", &c.container_name, "sh", "-c",
                    "set -eu; command -v git >/dev/null && command -v ssh >/dev/null && command -v socat >/dev/null || { apt-get update -qq; DEBIAN_FRONTEND=noninteractive apt-get install -y -qq git openssh-client socat; }"], 300).await?;
            }
            if action == v1::multica_request::Action::Install
                && !Path::new(&c.data_dir).join("bin/multica").is_file()
            {
                install_host(self, &c).await?;
            }
            if action == v1::multica_request::Action::Install {
                self.installation.stage("检查容器与服务状态");
            }
            *self.last_error.lock().unwrap() = if c.keep_alive {
                self.ensure(&c, socket)
                    .await
                    .err()
                    .map(|e| e.to_string())
                    .unwrap_or_default()
            } else {
                String::new()
            };
        }
        self.status().await
    }
    async fn status(&self) -> Result<v1::MulticaStatus, Error> {
        let mut c = self.load()?;
        let (row, inventory_error) = match self.observed(c.clone()).await {
            Ok((found, row)) => {
                c = found;
                (row, None)
            }
            Err(e) => (None, Some(e.to_string())),
        };
        let running = row
            .as_ref()
            .is_some_and(|r| r["status"]["state"] == "running");
        let installed = if running {
            self.command(
                &[
                    "exec",
                    &c.container_name,
                    "test",
                    "-x",
                    "/usr/local/bin/multica",
                ],
                5,
            )
            .await
            .is_ok()
        } else {
            row.is_some() && c.installed
        };
        let installed = installed && (!c.managed || c.installed);
        let daemon = running && installed && daemon_running(self, &c).await;
        // A saved root whose installed container still mounts another path needs
        // the attended offline migration before Settings may accept it.
        let migration_required =
            self.path.is_file() && row.as_ref().is_some_and(|r| !mounts_match(&c, r));
        let version = if installed && running {
            self.guest_version(&c).await
        } else {
            String::new()
        };
        let message = inventory_error.unwrap_or_else(|| {
            let error = self.last_error.lock().unwrap().clone();
            if !error.is_empty() {
                return error;
            }
            if daemon {
                "Multica daemon is running"
            } else if installed {
                "Installed; start the container and log in to Multica"
            } else if row.is_some() {
                "Container exists; installation is incomplete"
            } else {
                "Install Multica in an Apple container"
            }
            .into()
        });
        let executable = binary().unwrap_or_else(|_| PathBuf::from("container"));
        let login_command = format!(
            "{} exec -it {} multica login",
            quote(&executable.to_string_lossy()),
            quote(&c.container_name)
        );
        let progress = self.installation.snapshot();
        Ok(v1::MulticaStatus {
            supported: true,
            installed,
            running: daemon,
            data_dir: c.data_dir,
            installer_url: c.installer_url,
            server_url: c.server_url,
            app_url: c.app_url,
            keep_alive: c.keep_alive,
            container_name: c.container_name,
            message,
            login_command,
            busy: progress.busy,
            installation: (progress.busy || progress.finished).then_some(progress),
            migration_required,
            version,
        })
    }
    /// Remove task-scoped launchers left by a previous bridge process. The bridge
    /// listener does not exist yet at daemon startup, so no active turn owns them.
    pub async fn startup(&self) {
        if !cfg!(target_os = "macos") {
            return;
        }
        let Ok(c) = self.load() else { return };
        if c.data_dir.is_empty() || (!c.keep_alive && !c.runtime_bridge) {
            return;
        }
        if let Err(error) = bridge::revoke_stale(&c) {
            log::warn!("Multica task credential cleanup failed: {error}");
        }
    }
    async fn guest_version(&self, c: &Config) -> String {
        self.command(&["exec", &c.container_name, "multica", "--version"], 5)
            .await
            .ok()
            .map(|bytes| version_text(&bytes))
            .unwrap_or_default()
    }
    pub async fn maintain(&self, socket: &Path) {
        let Ok(_guard) = self.gate.try_lock() else {
            return;
        };
        let result = async {
            let c = self.load()?;
            if !c.keep_alive && !c.runtime_bridge {
                return Ok(());
            }
            if c.keep_alive {
                return self.ensure(&c, socket).await;
            }
            // Opted into the runtime bridge but not supervision: reconcile only an
            // already running container, never start one, its daemon or a task.
            self.reconcile_runtime(&c, socket).await
        }
        .await;
        *self.last_error.lock().unwrap() = result
            .err()
            .map(|e: Error| e.to_string())
            .unwrap_or_default();
    }
    async fn reconcile_runtime(&self, c: &Config, socket: &Path) -> Result<(), Error> {
        let (_, row) = self.observed(c.clone()).await?;
        let Some(row) = row else { return Ok(()) };
        if row["status"]["state"] != "running" {
            return Ok(());
        }
        self.ensure_bridge(c, socket, &row).await
    }
    async fn ensure_bridge(
        &self,
        c: &Config,
        socket: &Path,
        row: &serde_json::Value,
    ) -> Result<(), Error> {
        if !(c.managed || c.runtime_bridge) {
            return Ok(());
        }
        let gateway = row["status"]["networks"]
            .as_array()
            .and_then(|ns| ns.first())
            .and_then(|n| n["ipv4Gateway"].as_str())
            .ok_or_else(|| Error::Unavailable("Container bridge address is unavailable".into()))?;
        bridge::ensure(self, c, socket, gateway).await
    }
    async fn ensure(&self, c: &Config, socket: &Path) -> Result<(), Error> {
        let (_, row) = match self.observed(c.clone()).await {
            Ok(observed) => observed,
            Err(error) => {
                self.command(&["system", "start"], 60)
                    .await
                    .map_err(|_| error)?;
                self.observed(c.clone()).await?
            }
        };
        let row = row.ok_or_else(|| {
            Error::Unavailable("Multica container is missing; use Install".into())
        })?;
        if row["status"]["state"] != "running" {
            self.command(&["start", &c.container_name], 60).await?;
        }
        if c.managed || c.runtime_bridge {
            let (_, row) = self.observed(c.clone()).await?;
            let row = row.ok_or_else(|| {
                Error::Unavailable("Multica container is missing; use Install".into())
            })?;
            self.ensure_bridge(c, socket, &row).await?;
        }
        if !daemon_running(self, c).await {
            let args = daemon_start_args(c);
            self.command(&args.iter().map(String::as_str).collect::<Vec<_>>(), 30)
                .await?;
        }
        Ok(())
    }
}
fn daemon_start_args(c: &Config) -> Vec<String> {
    let mut args = vec![
        "exec".into(),
        "--env".into(),
        format!("MULTICA_WORKSPACES_ROOT={}", workspaces(c)),
    ];
    if c.managed || c.runtime_bridge {
        args.extend([
            "--env".into(),
            "MULTICA_CODEX_PATH=/usr/local/bin/codex".into(),
            "--env".into(),
            "MULTICA_NATIVE_CLAUDE_PATH=/usr/local/bin/muster-native-claude".into(),
        ]);
    }
    args.extend([c.container_name.clone(), "sh".into(), "-c".into(),
        "if [ -f /root/.multica/muster-environment ]; then xargs -0 sh -c 'exec env \"$@\" multica daemon start' starter < /root/.multica/muster-environment; else exec multica daemon start; fi".into()]);
    args
}
fn default_root() -> String {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join("Documents/Multica")
        .to_string_lossy()
        .into_owned()
}
fn io(e: std::io::Error) -> Error {
    Error::Unavailable(e.to_string())
}
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
fn binary() -> Result<PathBuf, Error> {
    ["/opt/homebrew/bin/container", "/usr/local/bin/container"]
        .into_iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
        .ok_or_else(|| {
            Error::Unavailable(
                "Install Apple container first: https://github.com/apple/container/releases".into(),
            )
        })
}
fn valid_https(text: &str) -> bool {
    url::Url::parse(text).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
    })
}

fn validate(r: v1::MulticaRequest, old: &Config) -> Result<Config, Error> {
    let path = if let Some(tail) = r.data_dir.strip_prefix("~/") {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(tail)
    } else {
        PathBuf::from(&r.data_dir)
    };
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if !path.is_absolute()
        || path.components().count() < 3
        || path.parent().is_none()
        || home
            .as_ref()
            .is_some_and(|h| path == *h || path == h.join("Documents"))
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        || r.data_dir.contains(['\n', '\r', ':', ','])
    {
        return Err(Error::Invalid(
            "Choose a dedicated absolute data directory without .., colons or commas".into(),
        ));
    }
    if !r
        .container_name
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphanumeric)
        || r.container_name.len() > 63
        || !r
            .container_name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(Error::Invalid("Use a simple Multica container name".into()));
    }
    for text in [&r.installer_url, &r.server_url, &r.app_url] {
        if text.is_empty() {
            continue;
        }
        let url =
            url::Url::parse(text).map_err(|_| Error::Invalid("Enter a valid HTTPS URL".into()))?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(Error::Invalid(
                "Multica URLs must use HTTPS without embedded credentials".into(),
            ));
        }
    }
    Ok(Config {
        data_dir: path.to_string_lossy().into_owned(),
        installer_url: r.installer_url,
        server_url: r.server_url,
        app_url: r.app_url,
        keep_alive: r.keep_alive,
        container_name: r.container_name,
        managed: old.managed,
        runtime_bridge: old.runtime_bridge,
        installed: old.installed,
        mount_dir: old.mount_dir.clone(),
    })
}
// A save adopts a completed offline migration. It never creates a legacy
// symlink or edits Apple's version-specific container metadata.
fn accept_migrated_root(old: &Config, new: &Config, row: &serde_json::Value) -> Result<(), Error> {
    let old_path = Path::new(&old.data_dir);
    let new_path = Path::new(&new.data_dir);
    if !matches!(old_path.symlink_metadata(), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
        || !new_path.is_dir()
        || new_path.is_symlink()
    {
        return Err(Error::Conflict(
            "Complete the offline data migration and remove the old path before saving; no compatibility link is created".into(),
        ));
    }
    let retains_old = row["configuration"]["mounts"]
        .as_array()
        .is_some_and(|mounts| {
            mounts.iter().any(|mount| {
                ["source", "destination"].iter().any(|key| {
                    mount[key]
                        .as_str()
                        .is_some_and(|path| Path::new(path).starts_with(old_path))
                })
            })
        });
    if !mounts_match(new, row) || retains_old {
        return Err(Error::Conflict(
            "Update the stopped container's bind mounts to the new data root before saving".into(),
        ));
    }
    Ok(())
}
/// Whether the installed container already serves `c`'s data root. A managed
/// install mounts the root, its config and its workspaces; an adopted container
/// mounts the root at the same path on host and guest.
fn mounts_match(c: &Config, row: &serde_json::Value) -> bool {
    let Some(mounts) = row["configuration"]["mounts"].as_array() else {
        return false;
    };
    let data = Path::new(&c.data_dir);
    let mounted = |source: &Path, destination: &Path| {
        mounts.iter().any(|mount| {
            mount["source"].as_str() == source.to_str()
                && mount["destination"].as_str() == destination.to_str()
        })
    };
    if c.managed {
        mounted(data, Path::new("/muster"))
            && mounted(&data.join("config"), Path::new("/root/.multica"))
            && mounted(&data.join("workspaces"), &data.join("workspaces"))
    } else {
        mounted(data, data)
    }
}
/// The first line of a guest `multica --version` probe, bounded and printable.
fn version_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .filter(|ch| !ch.is_control())
        .take(120)
        .collect::<String>()
        .trim()
        .to_owned()
}
fn private_write(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(io)?.write_all(bytes).map_err(io)
}
async fn run(executable: &Path, args: &[&str], seconds: u64) -> Result<Vec<u8>, Error> {
    let mut command = tokio::process::Command::new(executable);
    command
        .args(args)
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null());
    let out = tokio::time::timeout(Duration::from_secs(seconds), command.output())
        .await
        .map_err(|_| {
            Error::Unavailable(
                "Multica container operation timed out; check status before retrying".into(),
            )
        })?
        .map_err(io)?;
    if !out.status.success() {
        return Err(Error::Unavailable(format!(
            "Apple container {} failed ({}); inspect its local logs",
            args.first().unwrap_or(&"operation"),
            out.status
        )));
    }
    Ok(out.stdout)
}
fn workspaces(c: &Config) -> String {
    let root = if c.mount_dir.is_empty() {
        &c.data_dir
    } else {
        &c.mount_dir
    };
    if c.managed {
        format!("{root}/workspaces")
    } else {
        root.clone()
    }
}
async fn daemon_running(manager: &Manager, c: &Config) -> bool {
    manager.command(&["exec", &c.container_name, "sh", "-c", "p=$(cat /root/.multica/daemon.pid 2>/dev/null) || exit 1; case $p in ''|*[!0-9]*) exit 1;; esac; tr '\\0' ' ' < /proc/$p/cmdline | grep -q 'multica.*daemon'"], 5).await.is_ok()
}
async fn install(manager: &Manager, c: &Config, create: bool) -> Result<(), Error> {
    manager.installation.stage("准备数据目录与工作区");
    let root = Path::new(&c.data_dir);
    let marker = root.join(".muster-owned");
    if root.exists()
        && std::fs::read_dir(root).map_err(io)?.next().is_some()
        && std::fs::read(&marker).ok().as_deref() != Some(b"muster-multica-v1")
    {
        return Err(Error::Invalid(
            "Choose an empty data directory for a new installation".into(),
        ));
    }
    std::fs::create_dir_all(root).map_err(io)?;
    private_write(&marker, b"muster-multica-v1")?;
    for child in ["config", "workspaces", "bin"] {
        std::fs::create_dir_all(root.join(child)).map_err(io)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700)).map_err(io)?;
    }
    private_write(
        &root.join("runtime-command"),
        include_bytes!("../../../deploy/multica/runtime-command"),
    )?;
    if create {
        manager
            .installation
            .stage("下载容器镜像并创建容器（可能需要几分钟）");
        manager
            .install_command(
                &[
                    "run",
                    "--detach",
                    "--name",
                    &c.container_name,
                    "--cpus",
                    "2",
                    "--memory",
                    "2g",
                    "--volume",
                    &format!("{}:/muster", c.data_dir),
                    "--volume",
                    &format!("{0}:{0}", workspaces(c)),
                    "--volume",
                    &format!("{}/config:/root/.multica", c.data_dir),
                    "debian:bookworm-slim",
                    "sleep",
                    "infinity",
                ],
                180,
            )
            .await?;
    }
    manager
        .installation
        .stage("安装容器依赖与 Multica CLI（可能需要几分钟）");
    manager.install_command(&["exec", &c.container_name, "sh", "-c",
        "set -eu; apt-get update -qq; apt-get install -y -qq curl ca-certificates socat git openssh-client; curl --proto '=https' --proto-redir '=https' -fsSL \"$1\" -o /tmp/multica-install.sh; bash /tmp/multica-install.sh; test -x /usr/local/bin/multica",
        "setup", &c.installer_url], 300).await?;
    manager.installation.stage("写入 Multica 服务配置");
    for (key, value) in [("server_url", &c.server_url), ("app_url", &c.app_url)] {
        if !value.is_empty() {
            manager
                .command(
                    &[
                        "exec",
                        &c.container_name,
                        "multica",
                        "config",
                        "set",
                        key,
                        value,
                    ],
                    15,
                )
                .await?;
        }
    }
    install_host(manager, c).await
}

async fn install_host(manager: &Manager, c: &Config) -> Result<(), Error> {
    let root = Path::new(&c.data_dir);
    manager.installation.stage("安装本机 Multica CLI");
    #[cfg(test)]
    let mut candidates = manager.host_cli_candidates.clone().unwrap_or_default();
    #[cfg(not(test))]
    let mut candidates = Vec::new();
    if candidates.is_empty() {
        if let Some(home) = std::env::var_os("HOME") {
            candidates.push(PathBuf::from(home).join(".local/bin/multica"));
        }
        candidates.extend([
            PathBuf::from("/opt/homebrew/bin/multica"),
            PathBuf::from("/usr/local/bin/multica"),
        ]);
    }
    for candidate in candidates {
        if let Ok(version) = run(&candidate, &["--version"], 5).await
            && String::from_utf8_lossy(&version)
                .trim_start()
                .starts_with("multica ")
        {
            std::fs::create_dir_all(root.join("bin")).map_err(io)?;
            std::fs::copy(&candidate, root.join("bin/multica")).map_err(io)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(
                    root.join("bin/multica"),
                    std::fs::Permissions::from_mode(0o755),
                )
                .map_err(io)?;
            }
            return Ok(());
        }
    }
    if c.installer_url.is_empty() {
        return Err(Error::Invalid(
            "Enter the trusted HTTPS Multica installer URL".into(),
        ));
    }
    let installer = root.join("install.sh");
    manager
        .installation
        .run(
            tokio::process::Command::new("/usr/bin/curl")
                .args([
                    "--proto",
                    "=https",
                    "--proto-redir",
                    "=https",
                    "-fsSL",
                    &c.installer_url,
                    "-o",
                ])
                .arg(&installer),
            60,
        )
        .await?;
    manager
        .installation
        .run(
            tokio::process::Command::new("/bin/bash")
                .arg(&installer)
                .env("MULTICA_BIN_DIR", root.join("bin")),
            120,
        )
        .await?;
    if !root.join("bin/multica").is_file() {
        return Err(Error::Unavailable(
            "Host Multica CLI installation failed".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bridge_start_selects_native_claude_without_changing_legacy_daemons() {
        let mut c = Config::default();
        assert!(
            !daemon_start_args(&c)
                .iter()
                .any(|a| a.contains("MULTICA_NATIVE_CLAUDE_PATH"))
        );
        c.runtime_bridge = true;
        let args = daemon_start_args(&c);
        assert!(
            args.contains(&"MULTICA_NATIVE_CLAUDE_PATH=/usr/local/bin/muster-native-claude".into())
        );
        assert!(args.contains(&"MULTICA_CODEX_PATH=/usr/local/bin/codex".into()));
        assert!(!args.iter().any(|a| a.starts_with("MULTICA_CLAUDE_PATH=")));
    }
    fn request(path: &Path) -> v1::MulticaRequest {
        v1::MulticaRequest {
            data_dir: path.to_string_lossy().into_owned(),
            container_name: "muster-multica".into(),
            app_url: "https://scheduler.example.invalid".into(),
            ..Default::default()
        }
    }
    #[test]
    fn config_rejects_injection_and_unsafe_paths() {
        let old = Config::default();
        let mut r = request(Path::new("/tmp/Multica Data"));
        assert!(validate(r.clone(), &old).is_ok());
        for path in [
            "relative",
            "/",
            "/tmp/../home",
            "/tmp/a:b",
            "/tmp/a,b",
            "/tmp/a\nb",
        ] {
            r.data_dir = path.into();
            assert!(validate(r.clone(), &old).is_err(), "{path}");
        }
        r = request(Path::new("/tmp/Multica"));
        r.container_name = "--help".into();
        assert!(validate(r.clone(), &old).is_err());
        r.container_name = "mlt;touch bad".into();
        assert!(validate(r.clone(), &old).is_err());
        r.container_name = "mlt".into();
        for url in [
            "http://scheduler.example.invalid",
            "https://user:secret@scheduler.example.invalid",
            "file:///tmp/a",
        ] {
            r.app_url = url.into();
            assert!(validate(r.clone(), &old).is_err());
        }
    }
    #[test]
    fn config_survives_restart_without_embedding_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manager::new(dir.path());
        let c = validate(request(&dir.path().join("data")), &Config::default()).unwrap();
        m.save(&c).unwrap();
        assert_eq!(
            Manager::new(dir.path()).load().unwrap().data_dir,
            c.data_dir
        );
        assert!(default_root().ends_with("/Documents/Multica"));
    }
    #[test]
    fn adopted_native_bridge_survives_settings_save_and_restart() {
        let dir = tempfile::tempdir().unwrap();
        let manager = Manager::new(dir.path());
        let mut old: Config = serde_json::from_str(
            r#"{"managed":false,"runtime_bridge":true,"keep_alive":true,"container_name":"mlt"}"#,
        )
        .unwrap();
        old.data_dir = dir.path().join("data").to_string_lossy().into_owned();
        let saved = validate(request(Path::new(&old.data_dir)), &old).unwrap();
        manager.save(&saved).unwrap();
        let restored = Manager::new(dir.path()).load().unwrap();
        assert!(!restored.managed);
        assert!(restored.runtime_bridge);
        assert!(
            daemon_start_args(&restored)
                .iter()
                .any(|a| a == "MULTICA_CODEX_PATH=/usr/local/bin/codex")
        );
    }
    #[test]
    fn migration_requires_removed_old_root_and_matching_mounts() {
        let dir = tempfile::tempdir().unwrap();
        let old_path = dir.path().join("legacy");
        let new_path = dir.path().join("Documents Multica");
        std::fs::create_dir(&old_path).unwrap();
        std::fs::write(old_path.join("task"), "keep").unwrap();
        let old = Config {
            data_dir: old_path.to_string_lossy().into(),
            ..Default::default()
        };
        let mut new = Config {
            data_dir: new_path.to_string_lossy().into(),
            ..Default::default()
        };
        let mut row = serde_json::json!({"configuration":{"mounts":[{"source":new_path,"destination":new_path}]}});
        assert!(accept_migrated_root(&old, &new, &row).is_err());
        assert_eq!(std::fs::read(old_path.join("task")).unwrap(), b"keep");
        assert!(!new_path.exists());
        std::fs::rename(&old_path, &new_path).unwrap();
        accept_migrated_root(&old, &new, &row).unwrap();
        assert!(!old_path.exists());
        assert_eq!(std::fs::read(new_path.join("task")).unwrap(), b"keep");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&new_path, &old_path).unwrap();
            assert!(accept_migrated_root(&old, &new, &row).is_err());
            std::fs::remove_file(&old_path).unwrap();
        }
        row["configuration"]["mounts"][0]["destination"] = serde_json::json!(old_path);
        assert!(accept_migrated_root(&old, &new, &row).is_err());
        new.managed = true;
        row["configuration"]["mounts"] = serde_json::json!([
            {"source":new_path,"destination":"/muster"},
            {"source":new_path.join("config"),"destination":"/root/.multica"},
            {"source":new_path.join("workspaces"),"destination":new_path.join("workspaces")}
        ]);
        accept_migrated_root(&old, &new, &row).unwrap();
        row["configuration"]["mounts"][2]["source"] =
            serde_json::json!(old_path.join("workspaces"));
        assert!(accept_migrated_root(&old, &new, &row).is_err());
    }
}

#[cfg(all(test, unix))]
mod lifecycle_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn progress_is_available_while_mutations_remain_exclusive() {
        let dir = tempfile::tempdir().unwrap();
        let manager = Manager::new(dir.path());
        let _gate = manager.gate.lock().await;
        let _attempt = manager.installation.begin();
        let progress = manager
            .request(
                v1::MulticaRequest {
                    action: v1::multica_request::Action::Progress as i32,
                    ..Default::default()
                },
                Path::new("unused"),
            )
            .await
            .unwrap();
        assert!(progress.installation.unwrap().busy);
        assert!(
            manager
                .request(
                    v1::MulticaRequest {
                        action: v1::multica_request::Action::Install as i32,
                        ..Default::default()
                    },
                    Path::new("unused")
                )
                .await
                .is_err()
        );
        assert!(manager.installation.snapshot().busy);
        manager.installation.finish(None);
    }

    #[tokio::test]
    async fn supervision_restarts_only_stopped_services_and_is_opt_in() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("container-fixture");
        std::fs::write(
            &script,
            r#"#!/bin/sh
set -eu
root=$(dirname "$0")
printf '%s\n' "$*" >> "$root/calls"
case "$1" in
list)
  test -f "$root/runtime" || exit 1
  state=stopped
  test ! -f "$root/running" || state=running
  printf '[{"id":"mlt","status":{"state":"%s"}}]' "$state"
  ;;
system) touch "$root/runtime" ;;
start) touch "$root/running" ;;
exec)
  case "$*" in
    *'multica daemon start'*) touch "$root/daemon" ;;
    *'/proc/'*) test -f "$root/daemon" ;;
    *) exit 0 ;;
  esac
  ;;
*) exit 1 ;;
esac
"#,
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut manager = Manager::new(dir.path());
        manager.executable = Some(script);
        let mut config = Config {
            container_name: "mlt".into(),
            data_dir: dir.path().join("work").to_string_lossy().into_owned(),
            ..Default::default()
        };
        manager.save(&config).unwrap();
        manager.maintain(Path::new("unused.sock")).await;
        assert!(!dir.path().join("calls").exists());
        config.keep_alive = true;
        manager.save(&config).unwrap();
        manager.maintain(Path::new("unused.sock")).await;
        assert!(dir.path().join("running").exists());
        assert!(dir.path().join("daemon").exists());
        manager.maintain(Path::new("unused.sock")).await;
        let calls = std::fs::read_to_string(dir.path().join("calls")).unwrap();
        assert_eq!(calls.lines().filter(|s| s.starts_with("start ")).count(), 1);
        assert_eq!(
            calls
                .lines()
                .filter(|s| s.contains("multica daemon start"))
                .count(),
            1
        );
        assert_eq!(
            calls
                .lines()
                .filter(|s| s.starts_with("system start"))
                .count(),
            1
        );
        assert!(!calls.contains("install"));
        assert!(!calls.contains("restart"));
        std::fs::remove_file(dir.path().join("daemon")).unwrap();
        manager.maintain(Path::new("unused.sock")).await;
        assert!(dir.path().join("daemon").exists());
        assert!(manager.last_error.lock().unwrap().is_empty());
    }

    fn fake_container(root: &Path, fail_run: bool) -> PathBuf {
        let script = root.join("container");
        std::fs::write(
            &script,
            format!(
                r#"#!/bin/sh
printf '%s\n' "$*" >> "{calls}"
case "$1" in
  run) exit {run} ;;
  exec) exit 0 ;;
  list) echo '[]'; exit 0 ;;
  *) exit 0 ;;
esac
"#,
                calls = root.join("calls").display(),
                run = if fail_run { 7 } else { 0 }
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        script
    }

    fn fake_host_cli(root: &Path) -> PathBuf {
        let script = root.join("multica-fixture");
        std::fs::write(&script, "#!/bin/sh\nprintf 'multica 0.6.5\\n'\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        script
    }

    #[tokio::test]
    async fn install_creates_the_root_container_and_host_cli_in_stages() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = Manager::new(dir.path());
        manager.executable = Some(fake_container(dir.path(), false));
        manager.host_cli_candidates = Some(vec![fake_host_cli(dir.path())]);
        let data = dir.path().join("Multica");
        let c = Config {
            data_dir: data.to_string_lossy().into_owned(),
            container_name: "muster-multica".into(),
            installer_url: "https://installer.example.invalid/multica.sh".into(),
            ..Default::default()
        };
        install(&manager, &c, true).await.unwrap();
        assert_eq!(
            std::fs::read(data.join(".muster-owned")).unwrap(),
            b"muster-multica-v1"
        );
        for child in ["config", "workspaces", "bin"] {
            assert!(data.join(child).is_dir(), "{child}");
        }
        assert!(data.join("runtime-command").is_file());
        let host_cli = data.join("bin/multica");
        assert!(host_cli.is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&host_cli).unwrap().permissions().mode() & 0o777,
                0o755
            );
        }
        let calls = std::fs::read_to_string(dir.path().join("calls")).unwrap();
        assert!(calls.contains("run --detach --name muster-multica"));
        assert!(calls.contains(&format!("{}:/muster", data.display())));
        assert!(calls.contains("bash /tmp/multica-install.sh"));
        let log = manager.installation.snapshot().log;
        for stage in [
            "准备数据目录与工作区",
            "下载容器镜像并创建容器",
            "安装容器依赖与 Multica CLI",
            "写入 Multica 服务配置",
            "安装本机 Multica CLI",
        ] {
            assert!(log.contains(stage), "{stage}");
        }
    }

    #[tokio::test]
    async fn a_failed_container_run_keeps_the_partial_root_and_reports_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = Manager::new(dir.path());
        manager.executable = Some(fake_container(dir.path(), true));
        manager.host_cli_candidates = Some(vec![fake_host_cli(dir.path())]);
        let data = dir.path().join("Multica");
        let c = Config {
            data_dir: data.to_string_lossy().into_owned(),
            container_name: "muster-multica".into(),
            installer_url: "https://installer.example.invalid/multica.sh".into(),
            ..Default::default()
        };
        let error = install(&manager, &c, true).await.unwrap_err().to_string();
        assert!(
            error.contains("Installation command failed") && error.contains("7"),
            "{error}"
        );
        // No automatic rollback: a retry starts from this explicit partial state.
        assert!(data.join(".muster-owned").is_file());
        assert!(!manager.installation.snapshot().succeeded);
    }

    #[test]
    fn version_probe_is_one_printable_bounded_line() {
        assert_eq!(version_text(b"multica 0.6.5\nsecond\n"), "multica 0.6.5");
        assert_eq!(version_text(b"\n\n"), "");
        let noisy = version_text(b"multica\x1b[31m 0.6.5\r\n");
        assert!(!noisy.chars().any(char::is_control), "{noisy:?}");
        assert_eq!(version_text(&[b'x'; 400]).chars().count(), 120);
    }

    #[test]
    fn migration_is_required_only_when_the_mounts_do_not_serve_the_saved_root() {
        let data = "/Volumes/Fixture/Documents/Multica";
        let legacy = |source: &str, destination: &str| serde_json::json!({"configuration": {"mounts": [{"source": source, "destination": destination}]}});
        let mut c = Config {
            data_dir: data.into(),
            ..Default::default()
        };
        assert!(mounts_match(&c, &legacy(data, data)));
        assert!(!mounts_match(&c, &legacy("/tmp/other", "/tmp/other")));
        assert!(!mounts_match(&c, &serde_json::json!({})));
        c.managed = true;
        let managed = serde_json::json!({"configuration": {"mounts": [
            {"source": data, "destination": "/muster"},
            {"source": format!("{data}/config"), "destination": "/root/.multica"},
            {"source": format!("{data}/workspaces"), "destination": format!("{data}/workspaces")},
        ]}});
        assert!(mounts_match(&c, &managed));
        let mut missing = managed.clone();
        missing["configuration"]["mounts"]
            .as_array_mut()
            .unwrap()
            .pop();
        assert!(!mounts_match(&c, &missing));
    }

    #[tokio::test]
    async fn status_reports_migration_need_and_the_guest_version() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("container");
        std::fs::write(
            &script,
            r#"#!/bin/sh
case "$*" in
  'list --all --format json') echo '[{"id":"mlt","status":{"state":"running"},"configuration":{"mounts":[{"source":"/tmp/elsewhere","destination":"/tmp/elsewhere"}]}}]' ;;
  *'--version'*) printf 'multica 0.6.5\n' ;;
  *'test -x /usr/local/bin/multica'*) exit 0 ;;
  *) exit 0 ;;
esac
"#,
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut manager = Manager::new(dir.path());
        manager.executable = Some(script);
        let data = dir.path().join("Multica");
        std::fs::create_dir_all(&data).unwrap();
        manager
            .save(&Config {
                data_dir: data.to_string_lossy().into_owned(),
                container_name: "mlt".into(),
                installed: true,
                ..Default::default()
            })
            .unwrap();
        let status = manager.status().await.unwrap();
        assert!(status.migration_required);
        assert_eq!(status.version, "multica 0.6.5");
    }
}
