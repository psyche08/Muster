//! Agent Halo browser service. No shell commands, no temporary OS unlock.
use crate::{Daemon, Error};
use muster_proto::v1::{self, op_result::Status, response::Resp};
use std::sync::Arc;

pub fn parse(json: &str) -> Result<muster_browser::Request, Error> {
    if json.len() > muster_browser::MAX_REQUEST {
        return Err(Error::Invalid("browser request too large".into()));
    }
    let request: muster_browser::Request =
        serde_json::from_str(json).map_err(|e| Error::Invalid(e.to_string()))?;
    request
        .validate()
        .map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(request)
}

pub fn permission(request: &muster_browser::Request) -> muster_core::Permission {
    muster_core::Permission::Browser {
        control: request.is_mutation(),
    }
}

pub async fn setup(daemon: &Daemon, install: bool) -> Result<v1::BrowserStatus, Error> {
    let executable = std::env::current_exe().map_err(|e| Error::Unavailable(e.to_string()))?;
    let contents = executable
        .parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| Error::Unavailable("Muster app bundle unavailable".into()))?;
    let extension = contents.join("Resources/AgentHaloBrowser");
    let host = contents.join("Helpers/muster-browser-host");
    let home = std::env::var_os("HOME")
        .ok_or_else(|| Error::Unavailable("home directory unavailable".into()))?;
    let manifest_dir = if cfg!(target_os = "macos") {
        std::path::PathBuf::from(home)
            .join("Library/Application Support/Google/Chrome/NativeMessagingHosts")
    } else {
        std::path::PathBuf::from(home).join(".config/google-chrome/NativeMessagingHosts")
    };
    if install {
        if !extension.join("manifest.json").is_file() {
            return Err(Error::Unavailable(
                "Install the current Muster desktop app containing Agent Halo Browser first".into(),
            ));
        }
        muster_browser::install_host(&host, &manifest_dir)
            .map_err(|e| Error::Unavailable(e.to_string()))?;
    }
    let installed = std::fs::read(manifest_dir.join(format!("{}.json", muster_browser::HOST)))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .is_some_and(|m| {
            m["path"].as_str() == host.to_str()
                && m["allowed_origins"] == serde_json::json!([muster_browser::ORIGIN])
                && host.is_file()
        });
    let connected = muster_browser::call(
        &muster_browser::socket_in(&daemon.config.dir),
        &muster_browser::Request::Ping {},
    )
    .await
    .is_ok();
    Ok(v1::BrowserStatus {
        installed,
        connected,
        extension_path: if extension.join("manifest.json").is_file() {
            extension.to_string_lossy().into_owned()
        } else {
            String::new()
        },
        message: if connected {
            "Chrome extension connected"
        } else if installed {
            "Native host ready. Load the extension in Chrome, then Check connection."
        } else {
            "Set up the native host, then load the extension in Chrome."
        }
        .into(),
    })
}

pub async fn call(daemon: &Arc<Daemon>, request: v1::BrowserCallRequest) -> Result<Resp, Error> {
    use muster_proto::Message;
    let operation = parse(&request.json)?;
    if request.op_id.is_empty() || request.op_id.len() > 128 {
        return Err(Error::Invalid(
            "browser op_id is required (max 128 bytes)".into(),
        ));
    }
    let ledger = daemon.store().ledger();
    let result = |status: Status, message: String, data: String| {
        Resp::OpResult(v1::OpResult {
            status: status as i32,
            message,
            result_id: data,
            ..Default::default()
        })
    };
    match ledger.begin(&request.op_id, "browser.call", &request.encode_to_vec())? {
        muster_store::Begin::Fresh => {
            match muster_browser::call(&muster_browser::socket_in(&daemon.config.dir), &operation)
                .await
            {
                Ok(data) => {
                    let data = data.to_string();
                    ledger.complete(&request.op_id, data.as_bytes())?;
                    Ok(result(Status::Applied, String::new(), data))
                }
                // Keep pending: even a partial type/click must not be replayed.
                Err(error) => Ok(result(Status::Unknown, error.to_string(), String::new())),
            }
        }
        muster_store::Begin::Duplicate(Some(data)) => Ok(result(
            Status::Duplicate,
            String::new(),
            String::from_utf8_lossy(&data).into_owned(),
        )),
        muster_store::Begin::Conflict => Ok(result(
            Status::Rejected,
            "op_id reused with different request".into(),
            String::new(),
        )),
        _ => Ok(result(
            Status::Unknown,
            "previous browser outcome is unknown; not retried".into(),
            String::new(),
        )),
    }
}
