//! Local-owner Apple container lifecycle; credentials never cross the app ABI.
use crate::Error;
use muster_proto::v1;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
mod bridge;

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
    installed: bool,
    // Stable shared path retained after a physical data-directory migration.
    mount_dir: String,
}
pub struct Manager {
    path: PathBuf,
    executable: Option<PathBuf>,
    gate: tokio::sync::Mutex<()>,
    bridge: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    last_error: std::sync::Mutex<String>,
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
            path: root.join("multica.json"),
            executable: None,
            gate: tokio::sync::Mutex::new(()),
            bridge: tokio::sync::Mutex::new(None),
            last_error: std::sync::Mutex::new(String::new()),
        }
    }
    async fn command(&self, args: &[&str], seconds: u64) -> Result<Vec<u8>, Error> {
        let executable = self.executable.clone().map(Ok).unwrap_or_else(binary)?;
        run(&executable, args, seconds).await
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
        let _guard = self
            .gate
            .try_lock()
            .map_err(|_| Error::Busy("Multica setup is already running".into()))?;
        let action = v1::multica_request::Action::try_from(r.action)
            .map_err(|_| Error::Invalid("Unknown Multica action".into()))?;
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
            if row.is_some() && c.container_name != old.container_name {
                return Err(Error::Invalid("Keep the installed container name".into()));
            }
            if row.is_some() && c.data_dir != old.data_dir {
                if row
                    .as_ref()
                    .is_some_and(|r| r["status"]["state"] == "running")
                {
                    return Err(Error::Conflict("Turn off keep-alive and stop the container before moving its data directory".into()));
                }
                relocate(Path::new(&old.data_dir), Path::new(&c.data_dir))?;
                if c.mount_dir.is_empty() {
                    c.mount_dir = old.data_dir.clone();
                }
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
            busy: false,
        })
    }
    pub async fn maintain(&self, socket: &Path) {
        let Ok(_guard) = self.gate.try_lock() else {
            return;
        };
        let result = async {
            let c = self.load()?;
            if !c.keep_alive {
                return Ok(());
            }
            self.ensure(&c, socket).await
        }
        .await;
        *self.last_error.lock().unwrap() = result
            .err()
            .map(|e: Error| e.to_string())
            .unwrap_or_default();
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
        if c.managed {
            let (_, row) = self.observed(c.clone()).await?;
            let gateway = row
                .as_ref()
                .and_then(|r| r["status"]["networks"].as_array())
                .and_then(|ns| ns.first())
                .and_then(|n| n["ipv4Gateway"].as_str())
                .ok_or_else(|| {
                    Error::Unavailable("Container bridge address is unavailable".into())
                })?;
            bridge::ensure(self, c, socket, gateway).await?;
        }
        if !daemon_running(self, c).await {
            self.command(
                &[
                    "exec",
                    "--env",
                    &format!("MULTICA_WORKSPACES_ROOT={}", workspaces(c)),
                    &c.container_name,
                    "sh", "-c", "if [ -f /root/.multica/muster-environment ]; then xargs -0 sh -c 'exec env \"$@\" multica daemon start' starter < /root/.multica/muster-environment; else exec multica daemon start; fi",
                ],
                30,
            )
            .await?;
        }
        Ok(())
    }
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
        installed: old.installed,
        mount_dir: old.mount_dir.clone(),
    })
}
#[cfg(unix)]
fn relocate(old: &Path, new: &Path) -> Result<(), Error> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if old.components().count() < 3
        || home
            .as_ref()
            .is_some_and(|h| old == h || old == h.join("Documents"))
    {
        return Err(Error::Invalid(
            "Refusing to move a home or system directory".into(),
        ));
    }
    if new.exists()
        || new.symlink_metadata().is_ok()
        || new.starts_with(old)
        || old.is_symlink()
        || old.parent().is_none()
    {
        return Err(Error::Invalid(
            "Migration needs a new, non-nested directory and an ordinary source directory".into(),
        ));
    }
    std::fs::rename(old, new).map_err(io)?;
    if let Err(e) = std::os::unix::fs::symlink(new, old) {
        std::fs::rename(new, old).map_err(io)?;
        return Err(io(e));
    }
    Ok(())
}
#[cfg(not(unix))]
fn relocate(_: &Path, _: &Path) -> Result<(), Error> {
    Err(Error::Unavailable("Requires macOS".into()))
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
            .command(
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
    manager.command(&["exec", &c.container_name, "sh", "-c",
        "set -eu; apt-get update -qq; apt-get install -y -qq curl ca-certificates socat git; curl --proto '=https' --proto-redir '=https' -fsSL \"$1\" -o /tmp/multica-install.sh; bash /tmp/multica-install.sh; test -x /usr/local/bin/multica",
        "setup", &c.installer_url], 300).await?;
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
    let installer = root.join("install.sh");
    let download = tokio::process::Command::new("/usr/bin/curl")
        .args([
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "-fsSL",
            &c.installer_url,
            "-o",
        ])
        .arg(&installer)
        .kill_on_drop(true)
        .output();
    let out = tokio::time::timeout(Duration::from_secs(60), download)
        .await
        .map_err(|_| Error::Unavailable("Installer download timed out".into()))?
        .map_err(io)?;
    if !out.status.success() {
        return Err(Error::Unavailable(
            "Host Multica installer download failed".into(),
        ));
    }
    let out = tokio::time::timeout(
        Duration::from_secs(120),
        tokio::process::Command::new("/bin/bash")
            .arg(&installer)
            .env("MULTICA_BIN_DIR", root.join("bin"))
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| Error::Unavailable("Host Multica installation timed out".into()))?
    .map_err(io)?;
    if !out.status.success() || !root.join("bin/multica").is_file() {
        return Err(Error::Unavailable(
            "Host Multica CLI installation failed".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
    #[cfg(unix)]
    #[test]
    fn migration_preserves_data_and_original_shared_path_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("legacy");
        let new = dir.path().join("Documents Multica");
        std::fs::create_dir(&old).unwrap();
        std::fs::write(old.join("task"), "keep").unwrap();
        relocate(&old, &new).unwrap();
        assert!(old.is_symlink());
        assert_eq!(std::fs::read(old.join("task")).unwrap(), b"keep");
        assert_eq!(std::fs::read(new.join("task")).unwrap(), b"keep");
        assert!(relocate(&new, &old).is_err());
        assert_eq!(std::fs::read(new.join("task")).unwrap(), b"keep");
    }
}

#[cfg(all(test, unix))]
mod lifecycle_tests {
    use super::*;
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
}
