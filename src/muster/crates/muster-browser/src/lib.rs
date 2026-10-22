//! Agent Halo browser adapter. This private socket is local-user only. Remote
//! callers go through musterd browser scopes, policy and locked-screen presence.
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const HOST: &str = "dev.linsheng.muster.browser";
pub const ORIGIN: &str = "chrome-extension://ngfbfehgemldmajkjhjahbechooakidc/";
pub const MAX_REQUEST: usize = 256 * 1024;
pub const MAX_RESPONSE: usize = 8 * 1024 * 1024;
pub const DEADLINE: std::time::Duration = std::time::Duration::from_secs(3);
/// A `fetch` carries a request body, so its frame is larger than an operation
/// that only names a node. Chrome caps one message to the extension at 1 MB.
pub const MAX_FETCH_REQUEST: usize = 768 * 1024;
/// Reaching a site is network-bound, not a local call, so it waits longer
/// than the operations that only read or drive a tab already on screen.
pub const FETCH_DEADLINE: std::time::Duration = std::time::Duration::from_secs(20);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
pub enum Request {
    Ping {},
    Tabs {},
    Sessions {},
    Open {
        url: String,
    },
    Close {
        #[serde(rename = "tabId")]
        tab_id: u32,
    },
    Detach {
        #[serde(rename = "tabId")]
        tab_id: u32,
    },
    Snapshot {
        #[serde(rename = "tabId")]
        tab_id: u32,
        #[serde(default)]
        all: bool,
    },
    Find {
        #[serde(rename = "tabId")]
        tab_id: u32,
        #[serde(default)]
        role: String,
        #[serde(default)]
        name: String,
    },
    Click {
        #[serde(rename = "tabId")]
        tab_id: u32,
        document: u64,
        #[serde(rename = "ref")]
        node: u64,
    },
    Type {
        #[serde(rename = "tabId")]
        tab_id: u32,
        document: u64,
        #[serde(rename = "ref")]
        node: u64,
        text: String,
        #[serde(default)]
        clear: bool,
    },
    Press {
        #[serde(rename = "tabId")]
        tab_id: u32,
        document: u64,
        #[serde(rename = "ref")]
        node: u64,
        key: String,
    },
    /// One request to `url`, made by the browser as itself — with the session
    /// the person is already signed in with (ADR 0026). It opens no tab and
    /// touches nothing on screen. The browser keeps the credentials: the
    /// caller supplies no cookie and receives none.
    Fetch {
        url: String,
        /// Uppercase HTTP method; empty means GET.
        #[serde(default)]
        method: String,
        /// Lowercase header names, as the caller sends them. The browser adds
        /// its own, and refuses the ones only it may set.
        #[serde(default)]
        headers: Vec<(String, String)>,
        /// Base64 request body; empty for none.
        #[serde(default)]
        body: String,
    },
}
impl Request {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Open { url } => {
                let url = url::Url::parse(url)?;
                ensure!(
                    matches!(url.scheme(), "http" | "https") && url.host_str().is_some(),
                    "only HTTP(S) URLs are supported"
                );
                ensure!(
                    url.username().is_empty() && url.password().is_none(),
                    "URL credentials are not supported"
                );
            }
            Self::Find { role, name, .. } => ensure!(
                !role.is_empty() || !name.is_empty(),
                "find requires role or name"
            ),
            Self::Press { key, .. } => ensure!(
                key.chars().count() == 1
                    || [
                        "Enter",
                        "Tab",
                        "Escape",
                        "Backspace",
                        "Delete",
                        "Space",
                        "ArrowUp",
                        "ArrowDown",
                        "ArrowLeft",
                        "ArrowRight",
                        "Home",
                        "End",
                        "PageUp",
                        "PageDown"
                    ]
                    .contains(&key.as_str()),
                "unsupported key"
            ),
            Self::Fetch {
                url,
                method,
                headers,
                ..
            } => {
                let url = url::Url::parse(url)?;
                ensure!(
                    matches!(url.scheme(), "http" | "https") && url.host_str().is_some(),
                    "only HTTP(S) URLs are supported"
                );
                ensure!(
                    url.username().is_empty() && url.password().is_none(),
                    "URL credentials are not supported"
                );
                ensure!(
                    method.is_empty()
                        || (method.bytes().all(|b| b.is_ascii_uppercase())
                            && !["CONNECT", "TRACE"].contains(&method.as_str())),
                    "unsupported method"
                );
                for (name, value) in headers {
                    ensure!(
                        !name.is_empty()
                            && name
                                .bytes()
                                .all(|b| b.is_ascii_lowercase() || b"!#$%&'*+-.^_`|~0123456789".contains(&b)),
                        "invalid header name"
                    );
                    // The browser decides who the request is: a caller does not
                    // get to name the host or hand over a cookie of its own.
                    ensure!(
                        !["cookie", "host", "content-length", "connection"]
                            .contains(&name.as_str()),
                        "{name} is the browser's to set"
                    );
                    ensure!(
                        !value.contains(['\r', '\n']),
                        "a header value spans one line"
                    );
                }
            }
            _ => {}
        }
        let value = serde_json::to_value(self)?;
        for field in ["tabId", "document", "ref"] {
            if let Some(n) = value.get(field) {
                ensure!(
                    n.as_u64().is_some_and(|n| n > 0 && n <= i32::MAX as u64),
                    "invalid {field}"
                );
            }
        }
        ensure!(
            serde_json::to_vec(self)?.len() <= self.max_request(),
            "request too large"
        );
        Ok(())
    }
    pub fn is_mutation(&self) -> bool {
        match self {
            Self::Open { .. }
            | Self::Close { .. }
            | Self::Click { .. }
            | Self::Type { .. }
            | Self::Press { .. } => true,
            // Reading a page is not; anything else may have changed something
            // at the site, so a failure's outcome is unknown.
            Self::Fetch { method, .. } => !matches!(method.as_str(), "" | "GET" | "HEAD"),
            _ => false,
        }
    }

    /// How long this operation may take. Driving a tab that is already on
    /// screen is local; reaching a site is not.
    pub fn deadline(&self) -> std::time::Duration {
        match self {
            Self::Fetch { .. } => FETCH_DEADLINE,
            _ => DEADLINE,
        }
    }

    /// The largest frame this operation may be sent in.
    pub fn max_request(&self) -> usize {
        match self {
            Self::Fetch { .. } => MAX_FETCH_REQUEST,
            _ => MAX_REQUEST,
        }
    }
}

/// The bridge socket the native host serves under a daemon state directory.
pub fn socket_in(state_dir: &std::path::Path) -> PathBuf {
    state_dir.join("browser/bridge.sock")
}

/// The host's own socket. Chrome starts the host without musterd's
/// arguments, so it always uses the default state directory (or
/// `$MUSTER_DIR`); a daemon given another `--dir` cannot reach it.
pub fn socket_path() -> Result<PathBuf> {
    Ok(socket_in(
        &muster_paths::state_dir().ok_or_else(|| anyhow::anyhow!("no state directory"))?,
    ))
}

/// Native messaging uses native-endian u32 lengths. Bound before allocating.
pub async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R, max: usize) -> Result<Value> {
    let mut header = [0; 4];
    reader.read_exact(&mut header).await?;
    let len = u32::from_ne_bytes(header) as usize;
    ensure!(len > 0 && len <= max, "invalid frame size {len}");
    let mut data = vec![0; len];
    reader.read_exact(&mut data).await?;
    Ok(serde_json::from_slice(&data)?)
}
pub async fn write_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    value: &Value,
    max: usize,
) -> Result<()> {
    let data = serde_json::to_vec(value)?;
    ensure!(data.len() <= max, "frame too large");
    writer.write_all(&(data.len() as u32).to_ne_bytes()).await?;
    writer.write_all(&data).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(unix)]
pub async fn call(path: &Path, request: &Request) -> Result<Value> {
    request.validate()?;
    let deadline = request.deadline();
    let mut stream =
        tokio::time::timeout(DEADLINE, tokio::net::UnixStream::connect(path)).await??;
    // Never retry after any bytes have been sent: a mutation may have executed.
    let exchange = async {
        // Host serializes operations. Do not queue a write in the socket backlog:
        // the client must still be waiting when the host is ready to accept it.
        let ready = read_frame(&mut stream, 128).await?;
        ensure!(ready == json!({"ready": true}), "invalid browser handshake");
        write_frame(
            &mut stream,
            &serde_json::to_value(request)?,
            request.max_request(),
        )
        .await?;
        let reply = read_frame(&mut stream, MAX_RESPONSE).await?;
        if let Some(error) = reply.get("error") {
            bail!("{error}");
        }
        reply
            .get("result")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing result"))
    };
    tokio::time::timeout(deadline + std::time::Duration::from_secs(1), exchange)
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "browser request timed out; outcome unknown, do not automatically retry"
            )
        })?
}

/// AX projection is business logic, so it lives in Rust rather than the JS shim.
pub fn project(request: &Request, mut result: Value) -> Result<Value> {
    if !matches!(request, Request::Snapshot { .. } | Request::Find { .. }) {
        return Ok(result);
    }
    let nodes = result
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("missing AX nodes"))?;
    ensure!(nodes.len() <= 50_000, "AX tree too large");
    let compact = |n: &Value, depth: usize| {
        let mut out = json!({"ref": n["backendDOMNodeId"], "role": n["role"]["value"].as_str().unwrap_or(""), "name": n["name"]["value"].as_str().unwrap_or("").trim(), "depth": depth});
        if let Some(v) = n.get("value").and_then(|v| v.get("value")) {
            out["value"] = v.clone();
        }
        if let Some(props) = n["properties"].as_array() {
            let mut kept = serde_json::Map::new();
            for p in props {
                let key = p["name"].as_str().unwrap_or("");
                if [
                    "focused", "checked", "pressed", "expanded", "selected", "disabled",
                    "required", "invalid", "readonly", "level", "url",
                ]
                .contains(&key)
                {
                    kept.insert(key.into(), p["value"]["value"].clone());
                }
            }
            out["props"] = Value::Object(kept);
        }
        out
    };
    let mut out = Vec::new();
    if matches!(request, Request::Find { .. }) {
        out.extend(
            nodes
                .iter()
                .filter(|n| n["ignored"] != true)
                .map(|n| compact(n, 0)),
        );
    } else {
        let by_id: std::collections::HashMap<_, _> = nodes
            .iter()
            .filter_map(|n| n["nodeId"].as_str().map(|id| (id, n)))
            .collect();
        let mut seen = std::collections::HashSet::new();
        let mut stack: Vec<_> = nodes
            .iter()
            .filter(|n| n.get("parentId").is_none())
            .rev()
            .map(|n| (n, 0, Arc::<str>::from(""), Arc::<str>::from("")))
            .collect();
        while let Some((node, depth, parent_name, parent_value)) = stack.pop() {
            let id = node["nodeId"].as_str().unwrap_or("");
            if !seen.insert(id) {
                continue;
            }
            let item = compact(node, depth);
            let name = item["name"].as_str().unwrap_or("");
            let role = item["role"].as_str().unwrap_or("");
            let all = matches!(request, Request::Snapshot { all: true, .. });
            let keep = node["ignored"] != true
                && role != "InlineTextBox"
                && (all
                    || !((name.is_empty() && ["generic", "none", "LabelText"].contains(&role))
                        || (role == "StaticText"
                            && (name == parent_name.as_ref() || name == parent_value.as_ref()))));
            let (next_name, next_value) = if keep {
                (
                    Arc::<str>::from(name),
                    Arc::<str>::from(item["value"].as_str().unwrap_or("")),
                )
            } else {
                (parent_name, parent_value)
            };
            if let Some(children) = node["childIds"].as_array() {
                for child in children.iter().rev() {
                    if let Some(n) = child.as_str().and_then(|id| by_id.get(id)) {
                        stack.push((
                            n,
                            depth + usize::from(keep),
                            next_name.clone(),
                            next_value.clone(),
                        ));
                    }
                }
            }
            if keep {
                out.push(item);
            }
        }
    }
    result["nodes"] = Value::Array(out);
    Ok(result)
}

/// Register only this extension, using a stable executable path supplied by the bundle.
pub fn install_host(executable: &Path, directory: &Path) -> Result<()> {
    let executable = executable.canonicalize()?;
    ensure!(executable.is_file(), "native host executable missing");
    std::fs::create_dir_all(directory)?;
    let manifest = json!({"name":HOST,"description":"Agent Halo browser accessibility","path":executable,"type":"stdio","allowed_origins":[ORIGIN]});
    std::fs::write(
        directory.join(format!("{HOST}.json")),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(())
}
