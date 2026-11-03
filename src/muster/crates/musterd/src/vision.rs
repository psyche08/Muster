//! The owner's own vision-capable LLM looks at the app window a native send
//! failed in (ADR 0116). The configuration is local-owner only; the API key
//! is write-only and lives in musterd's own login-keychain item. The rules
//! (endpoints, request formats, what reaches the phone) are
//! `muster_agent::vision`.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use muster_agent::cloud::{Request, Response};
use muster_agent::vision::{self as rules, Config, Format};
use muster_proto::v1::{self, error::Code, op_result::Status, response::Resp};
use tokio_util::sync::CancellationToken;

use crate::{Daemon, Error};

const CONFIG_KEY: &str = "vision.config";
#[cfg(target_os = "macos")]
const KEY_SERVICE: &str = "dev.linsheng.musterd.vision";
#[cfg(target_os = "macos")]
const KEY_ACCOUNT: &str = "api-key";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Longest reply body kept; a two-sentence answer is far smaller.
const MAX_BODY: usize = 1024 * 1024;
const MAX_KEY: usize = 4096;
/// Longest failure text sent along; the daemon's own messages are shorter.
const MAX_FAILURE: usize = 600;
#[cfg(target_os = "macos")]
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(4);

/// A native send that may be looked at if it fails.
pub struct Target {
    pub session: String,
    /// The message itself, removed from the failure text before it leaves.
    pub text: String,
}

pub fn config(daemon: &Daemon) -> Config {
    daemon
        .store()
        .kv_get(CONFIG_KEY)
        .ok()
        .flatten()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_config(daemon: &Daemon, config: &Config) -> Result<(), Error> {
    let bytes = serde_json::to_vec(config).map_err(|e| Error::Invalid(e.to_string()))?;
    daemon.store().kv_set(CONFIG_KEY, &bytes)?;
    Ok(())
}

/// The local owner's request: get, save, test, clear the key.
pub async fn request(daemon: &Daemon, r: v1::VisionRequest) -> Result<v1::VisionStatus, Error> {
    use v1::vision_request::Action;
    let action =
        Action::try_from(r.action).map_err(|_| Error::Invalid("unknown vision action".into()))?;
    if !cfg!(target_os = "macos") {
        return Ok(v1::VisionStatus {
            message: "Muster looks at app windows only on macOS".into(),
            ..Default::default()
        });
    }
    let message = match action {
        Action::Get => String::new(),
        Action::Save => save(daemon, r).await?,
        Action::Test => test(daemon).await,
        Action::ClearKey => {
            blocking(key::delete).await?;
            let mut config = config(daemon);
            config.enabled = false;
            save_config(daemon, &config)?;
            "API key removed; Muster no longer looks".into()
        }
    };
    Ok(status(daemon, message))
}

fn status(daemon: &Daemon, message: String) -> v1::VisionStatus {
    let config = config(daemon);
    v1::VisionStatus {
        supported: true,
        enabled: config.enabled,
        api_format: config.format.name().into(),
        base_url: config.base_url,
        model: config.model,
        key_stored: key::exists(),
        screen_recording: capture::allowed(),
        message,
        busy: false,
    }
}

async fn save(daemon: &Daemon, r: v1::VisionRequest) -> Result<String, Error> {
    let config = Config {
        enabled: r.enabled,
        format: Format::parse(&r.api_format).map_err(Error::Invalid)?,
        base_url: r.base_url.trim().into(),
        model: r.model.trim().into(),
    };
    let new_key = r.api_key.trim().to_owned();
    if !new_key.is_empty()
        && (new_key.len() > MAX_KEY || new_key.chars().any(|c| c.is_control() || c.is_whitespace()))
    {
        return Err(Error::Invalid("the API key is not valid".into()));
    }
    config
        .validate(!new_key.is_empty() || key::exists())
        .map_err(Error::Invalid)?;
    if !new_key.is_empty() {
        blocking(move || key::set(&new_key)).await?;
    }
    save_config(daemon, &config)?;
    if !config.enabled {
        return Ok("Saved; Muster does not look".into());
    }
    // The person is at this Mac: the one moment the system prompt helps.
    if !capture::allowed() {
        blocking(|| Ok(capture::request())).await?;
    }
    Ok("Saved; Muster looks when a native send fails".into())
}

async fn test(daemon: &Daemon) -> String {
    let config = Config {
        enabled: true,
        ..config(daemon)
    };
    let result = async {
        config.validate(key::exists())?;
        let key = blocking(|| key::get(true))
            .await
            .map_err(|e| e.to_string())?
            .ok_or("enter an API key")?;
        let request = rules::test_request(&config, &key)?;
        let response = send(request).await?;
        rules::reply(config.format, &key, &response)
    }
    .await;
    match result {
        Ok(text) => format!(
            "The model answered: {}",
            rules::diagnosis(&text).unwrap_or_default()
        ),
        Err(error) => format!("Test failed: {error}"),
    }
}

/// The reply a failed send gets, with what the owner's LLM sees appended.
/// Anything else, and every failure while the feature is off, the Mac is
/// locked or the caller has gone, comes back unchanged.
pub async fn after_send(
    daemon: &Arc<Daemon>,
    target: Target,
    mut reply: Resp,
    cancelled: CancellationToken,
) -> Resp {
    // Without Accessibility the reply already names the fix.
    if !worth_a_look(&reply)
        || daemon.platform().locked()
        || daemon.platform().accessibility() == Some(false)
    {
        return reply;
    }
    let config = config(daemon);
    // A session whose app is unknown has no window to look at.
    let app = daemon
        .agent()
        .and_then(|agent| crate::agent_bridge::session_app(agent, &target.session));
    let Some((app, _)) = app.filter(|_| config.enabled) else {
        return reply;
    };
    let message = match &mut reply {
        Resp::Error(e) => &mut e.message,
        Resp::OpResult(r) => &mut r.message,
        _ => return reply,
    };
    let failure = without(message, &target.text);
    let look = tokio::select! {
        _ = cancelled.cancelled() => return reply,
        result = tokio::time::timeout(rules::BUDGET, look(app, &config, &failure)) => {
            result.unwrap_or_else(|_| Err("it took too long".into()))
        }
    };
    if let Err(error) = &look {
        log::info!("vision: no look at a failed send: {error}");
    }
    *message = rules::annotate(message, look);
    reply
}

/// Failures the screen may explain. A conflict, a busy app, an invalid
/// request, a queue timeout or an unconfirmed earlier send already says why.
fn worth_a_look(reply: &Resp) -> bool {
    match reply {
        Resp::Error(e) => matches!(
            Code::try_from(e.code),
            Ok(Code::Unavailable | Code::NotFound)
        ),
        Resp::OpResult(r) => match Status::try_from(r.status) {
            Ok(Status::Unknown) => true,
            Ok(Status::Rejected) => {
                r.unconfirmed.is_empty()
                    && ["unavailable: ", "not found: ", "outcome unknown: "]
                        .iter()
                        .any(|prefix| r.message.starts_with(prefix))
            }
            _ => false,
        },
        _ => false,
    }
}

/// The failure text, bounded, without the message being sent.
fn without(failure: &str, text: &str) -> String {
    let text = text.trim();
    let failure = if text.chars().count() >= 4 {
        failure.replace(text, "[message]")
    } else {
        failure.to_owned()
    };
    failure.chars().take(MAX_FAILURE).collect()
}

async fn look(
    app: &'static muster_agent::profile::App,
    config: &Config,
    failure: &str,
) -> Result<String, String> {
    let png = blocking(move || capture::window(app))
        .await
        .inspect_err(|error| log::info!("vision: no window capture: {error}"))
        .ok();
    let key = blocking(|| key::get(false))
        .await
        .map_err(|e| e.to_string())?
        .ok_or("no API key is stored")?;
    let request = rules::request(
        config,
        &key,
        &rules::Look {
            app: app.app.agent_label,
            failure,
            png: png.as_deref(),
        },
    )?;
    let response = send(request).await?;
    let text = rules::reply(config.format, &key, &response)?;
    rules::diagnosis(&text).ok_or_else(|| "the reply is empty".into())
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, Error> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| Error::Unavailable(e.to_string()))?
        .map_err(Error::Unavailable)
}

fn client() -> Result<&'static reqwest::Client, String> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            // Plain HTTP only reaches a loopback endpoint: Shared Rust
            // checks every URL before a request is built.
            let builder = reqwest::Client::builder()
                .connect_timeout(CONNECT_TIMEOUT)
                .timeout(rules::BUDGET)
                .redirect(reqwest::redirect::Policy::none());
            crate::cloud_http::with_platform_trust(builder)
                .and_then(|b| Ok(b.build()?))
                .map_err(|_| "the HTTPS client could not be set up".to_owned())
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// Never formats a reqwest error: the URL or a header could carry a secret.
async fn send(request: Request) -> Result<Response, String> {
    let method = reqwest::Method::from_bytes(request.method.as_bytes())
        .map_err(|_| "the request method is not valid".to_owned())?;
    let mut builder = client()?.request(method, &request.url);
    for (name, value) in &request.headers {
        builder = builder.header(name, value);
    }
    if let Some(body) = request.body {
        builder = builder.body(body);
    }
    let failed = |phase: &str, e: &reqwest::Error| {
        format!(
            "the LLM API {phase} failed: {}",
            crate::cloud_http::transport_cause(e)
        )
    };
    let response = builder.send().await.map_err(|e| failed("request", &e))?;
    let status = response.status().as_u16();
    let bytes = response.bytes().await.map_err(|e| failed("reply", &e))?;
    if bytes.len() > MAX_BODY {
        return Err(format!(
            "the LLM API reply of {} bytes is too large",
            bytes.len()
        ));
    }
    Ok(Response {
        status,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    })
}

#[cfg(target_os = "macos")]
mod key {
    use muster_platform::macos::vision::Secret;

    fn secret() -> Secret {
        Secret::new(super::KEY_SERVICE, super::KEY_ACCOUNT)
    }
    pub fn exists() -> bool {
        secret().exists()
    }
    pub fn get(interactive: bool) -> Result<Option<String>, String> {
        secret().get(interactive)
    }
    pub fn set(key: &str) -> Result<(), String> {
        secret().set(key)
    }
    pub fn delete() -> Result<(), String> {
        secret().delete()
    }
}

#[cfg(not(target_os = "macos"))]
mod key {
    pub fn exists() -> bool {
        false
    }
    pub fn get(_interactive: bool) -> Result<Option<String>, String> {
        Ok(None)
    }
    pub fn set(_key: &str) -> Result<(), String> {
        Err("not supported on this platform".into())
    }
    pub fn delete() -> Result<(), String> {
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod capture {
    use muster_platform::macos::{native_app, vision};

    pub fn allowed() -> bool {
        vision::capture_allowed()
    }
    pub fn request() -> bool {
        vision::request_capture()
    }
    pub fn window(app: &muster_agent::profile::App) -> Result<Vec<u8>, String> {
        vision::window_png(&native_app::app_pids(app), super::CAPTURE_TIMEOUT)
    }
}

#[cfg(not(target_os = "macos"))]
mod capture {
    pub fn allowed() -> bool {
        false
    }
    pub fn request() -> bool {
        false
    }
    pub fn window(_app: &muster_agent::profile::App) -> Result<Vec<u8>, String> {
        Err("not supported on this platform".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(status: Status, message: &str, unconfirmed: &str) -> Resp {
        Resp::OpResult(v1::OpResult {
            status: status as i32,
            message: message.into(),
            result_id: String::new(),
            unconfirmed: unconfirmed.into(),
        })
    }

    #[test]
    fn only_failures_the_screen_may_explain_are_looked_at() {
        assert!(worth_a_look(&muster_rpc::err(Code::Unavailable, "x")));
        assert!(worth_a_look(&muster_rpc::err(Code::NotFound, "x")));
        for code in [
            Code::Conflict,
            Code::Busy,
            Code::Invalid,
            Code::Timeout,
            Code::Unauthorized,
        ] {
            assert!(!worth_a_look(&muster_rpc::err(code, "x")), "{code:?}");
        }
        assert!(worth_a_look(&op(Status::Unknown, "outcome unknown: x", "")));
        assert!(worth_a_look(&op(
            Status::Rejected,
            "unavailable: not visible",
            ""
        )));
        assert!(worth_a_look(&op(
            Status::Rejected,
            "not found: no composer",
            ""
        )));
        assert!(worth_a_look(&op(
            Status::Rejected,
            "outcome unknown: x; this request reached no provider submission and was not sent",
            ""
        )));
        for message in [
            "conflict: local input",
            "busy: the app is held",
            "invalid: empty",
            "timeout: queue",
        ] {
            assert!(
                !worth_a_look(&op(Status::Rejected, message, "")),
                "{message}"
            );
        }
        assert!(!worth_a_look(&op(
            Status::Rejected,
            "unavailable: x",
            "op-1"
        )));
        assert!(!worth_a_look(&op(Status::Applied, "", "")));
        assert!(!worth_a_look(&op(Status::Duplicate, "", "")));
    }

    #[test]
    fn the_message_being_sent_never_leaves_in_the_failure_text() {
        let failure = "outcome unknown: the composer still shows hello world";
        assert_eq!(
            without(failure, " hello world "),
            "outcome unknown: the composer still shows [message]"
        );
        assert_eq!(without("unavailable: hi", "hi"), "unavailable: hi");
        assert_eq!(without(&"x".repeat(2000), "").chars().count(), MAX_FAILURE);
    }
}
