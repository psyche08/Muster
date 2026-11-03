//! The owner's own vision-capable LLM looks at the app window a native send
//! failed in and says what is on screen (ADR 0116).
//!
//! This module owns the rules: which endpoints are acceptable, the two
//! request formats, how a reply is read and what of it reaches the phone.
//! The daemon supplies the key, the screenshot and the transport
//! ([`crate::cloud::Http`]). Nothing here ever sees the message being sent.

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::cloud::{Request, Response};

/// The longest diagnosis appended to a failure, in characters.
pub const MAX_DIAGNOSIS: usize = 300;
/// How long one look may take, connection to last byte.
pub const BUDGET: std::time::Duration = std::time::Duration::from_secs(20);
const MAX_TOKENS: u32 = 300;
const MAX_MODEL: usize = 200;
const MAX_URL: usize = 2048;
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// The request and reply format the endpoint speaks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// OpenAI-compatible chat completions, `{base}/chat/completions`.
    #[default]
    OpenAi,
    /// Anthropic Messages, `{base}/v1/messages`.
    Anthropic,
}

impl Format {
    pub fn parse(text: &str) -> Result<Self, String> {
        match text.trim() {
            "" | "openai" => Ok(Self::OpenAi),
            "anthropic" => Ok(Self::Anthropic),
            other => Err(format!("unknown API format {other:?}")),
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Anthropic => "anthropic",
        }
    }
}

/// The non-secret configuration, as stored.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub enabled: bool,
    pub format: Format,
    pub base_url: String,
    pub model: String,
}

impl Config {
    /// The endpoint a request goes to. HTTPS anywhere, or HTTP to this Mac
    /// for a local model server; never credentials, a query or a fragment,
    /// which could leak or redirect what is sent.
    pub fn endpoint(&self) -> Result<url::Url, String> {
        let base = self.base_url.trim();
        if base.is_empty() {
            return Err("the API base URL is empty".into());
        }
        if base.len() > MAX_URL {
            return Err("the API base URL is too long".into());
        }
        let url = url::Url::parse(base).map_err(|_| "the API base URL is not a URL".to_owned())?;
        let loopback = match url.host() {
            Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            None => return Err("the API base URL has no host".into()),
        };
        match url.scheme() {
            "https" => {}
            "http" if loopback => {}
            _ => return Err("the API base URL must use HTTPS (HTTP only to this Mac)".into()),
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err("the API base URL must not carry credentials".into());
        }
        if url.query().is_some() || url.fragment().is_some() {
            return Err("the API base URL must not have a query or fragment".into());
        }
        let path = url.path().trim_end_matches('/');
        let suffix = match self.format {
            Format::OpenAi => "/chat/completions",
            Format::Anthropic => "/v1/messages",
        };
        let mut endpoint = url.clone();
        endpoint.set_path(&format!("{path}{suffix}"));
        Ok(endpoint)
    }

    /// A configuration that can be saved; turning it on also needs a key.
    pub fn validate(&self, has_key: bool) -> Result<(), String> {
        let model = self.model.trim();
        if model.chars().count() > MAX_MODEL || model.chars().any(char::is_control) {
            return Err("the model name is not valid".into());
        }
        if !self.enabled {
            if !self.base_url.trim().is_empty() {
                self.endpoint()?;
            }
            return Ok(());
        }
        self.endpoint()?;
        if model.is_empty() {
            return Err("choose a model".into());
        }
        if !has_key {
            return Err("enter an API key".into());
        }
        Ok(())
    }
}

/// What the model is asked; the failure text is the daemon's own message.
pub struct Look<'a> {
    /// The desktop app's display name.
    pub app: &'a str,
    /// Why the send failed, as Muster reported it.
    pub failure: &'a str,
    /// A PNG of the app's window, when musterd may capture it.
    pub png: Option<&'a [u8]>,
}

const SYSTEM: &str = "You help someone who controls a macOS desktop app remotely from a phone. \
An automation tool tried to put a chat message into the app and send it, and failed. \
Look at the screenshot of the app window and say, in at most two short sentences, what on \
screen most likely stopped it: for example a dialog or sheet, a sign-in or update screen, a \
settings page or another view instead of a conversation, a conversation still loading, or no \
message box. Describe only what is visible; if nothing explains the failure, say so. Never \
repeat private text from the window and never give commands to run. Reply in Simplified \
Chinese, as plain text.";

/// The request for one look.
pub fn request(config: &Config, key: &str, look: &Look<'_>) -> Result<Request, String> {
    let url = config.endpoint()?.to_string();
    let text = match look.png {
        Some(_) => format!(
            "App: {}\nWhat the automation reported: {}",
            look.app, look.failure
        ),
        None => format!(
            "App: {}\nWhat the automation reported: {}\nNo screenshot is available \
             (screen recording is not allowed); answer from the report only and say that \
             the window could not be seen.",
            look.app, look.failure
        ),
    };
    body(config, key, &url, SYSTEM, &text, look.png)
}

/// A text-only request that proves the endpoint, key and model answer.
pub fn test_request(config: &Config, key: &str) -> Result<Request, String> {
    let url = config.endpoint()?.to_string();
    body(
        config,
        key,
        &url,
        "Answer with the single word OK.",
        "Are you there?",
        None,
    )
}

fn body(
    config: &Config,
    key: &str,
    url: &str,
    system: &str,
    text: &str,
    png: Option<&[u8]>,
) -> Result<Request, String> {
    if key.is_empty() || key.chars().any(|c| c.is_control() || c == ' ') {
        return Err("the stored API key is not valid".into());
    }
    let image = png.map(|png| base64::engine::general_purpose::STANDARD.encode(png));
    let model = config.model.trim();
    let (headers, body) = match config.format {
        Format::OpenAi => {
            let mut content = vec![json!({"type": "text", "text": text})];
            if let Some(image) = &image {
                content.push(json!({
                    "type": "image_url",
                    "image_url": {"url": format!("data:image/png;base64,{image}")},
                }));
            }
            (
                vec![("authorization".to_owned(), format!("Bearer {key}"))],
                json!({
                    "model": model,
                    "max_tokens": MAX_TOKENS,
                    "messages": [
                        {"role": "system", "content": system},
                        {"role": "user", "content": content},
                    ],
                }),
            )
        }
        Format::Anthropic => {
            let mut content = Vec::new();
            if let Some(image) = &image {
                content.push(json!({
                    "type": "image",
                    "source": {"type": "base64", "media_type": "image/png", "data": image},
                }));
            }
            content.push(json!({"type": "text", "text": text}));
            (
                vec![
                    ("x-api-key".to_owned(), key.to_owned()),
                    ("anthropic-version".to_owned(), ANTHROPIC_VERSION.to_owned()),
                ],
                json!({
                    "model": model,
                    "max_tokens": MAX_TOKENS,
                    "system": system,
                    "messages": [{"role": "user", "content": content}],
                }),
            )
        }
    };
    let mut headers = headers;
    headers.push(("content-type".to_owned(), "application/json".to_owned()));
    Ok(Request {
        method: "POST",
        url: url.to_owned(),
        headers,
        body: Some(body.to_string()),
    })
}

/// The model's text, or why there is none. Error texts are bounded and have
/// the key removed: a provider may echo what it was sent.
pub fn reply(format: Format, key: &str, response: &Response) -> Result<String, String> {
    let parsed: Option<Value> = serde_json::from_str(&response.body).ok();
    if !(200..300).contains(&response.status) {
        let detail = parsed
            .as_ref()
            .and_then(|v| {
                v.pointer("/error/message")
                    .or_else(|| v.pointer("/message"))
                    .and_then(Value::as_str)
            })
            .map(|m| format!(": {}", bounded(&redact(m, key), 160)))
            .unwrap_or_default();
        return Err(format!(
            "the LLM API answered HTTP {}{detail}",
            response.status
        ));
    }
    let value = parsed.ok_or_else(|| "the LLM API reply is not JSON".to_owned())?;
    let text = match format {
        Format::OpenAi => match value.pointer("/choices/0/message/content") {
            Some(Value::String(text)) => text.clone(),
            Some(Value::Array(parts)) => texts(parts),
            _ => String::new(),
        },
        Format::Anthropic => value
            .get("content")
            .and_then(Value::as_array)
            .map(|blocks| texts(blocks))
            .unwrap_or_default(),
    };
    let text = redact(&text, key);
    if text.trim().is_empty() {
        return Err("the LLM API reply has no text".into());
    }
    Ok(text)
}

fn texts(parts: &[Value]) -> String {
    parts
        .iter()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join(" ")
}

fn redact(text: &str, key: &str) -> String {
    if key.len() < 8 {
        return text.to_owned();
    }
    text.replace(key, "[key]")
}

fn bounded(text: &str, limit: usize) -> String {
    let flat = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if flat.chars().count() <= limit {
        return flat;
    }
    let mut cut: String = flat.chars().take(limit.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

/// What reaches the phone of the model's answer: one line, bounded.
pub fn diagnosis(text: &str) -> Option<String> {
    let line = bounded(text, MAX_DIAGNOSIS);
    (!line.is_empty()).then_some(line)
}

/// The failure as the phone shows it, with the look appended. The ledger
/// keeps the original: a diagnosis is advice, never an outcome.
pub fn annotate(failure: &str, look: Result<String, String>) -> String {
    match look {
        Ok(diagnosis) => format!("{failure}\nMuster looked: {diagnosis}"),
        Err(reason) => format!(
            "{failure}\nMuster could not look: {}",
            bounded(&reason, 200)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(format: Format, base: &str) -> Config {
        Config {
            enabled: true,
            format,
            base_url: base.into(),
            model: "vision-model".into(),
        }
    }

    fn body_of(request: &Request) -> Value {
        serde_json::from_str(request.body.as_deref().unwrap()).unwrap()
    }

    #[test]
    fn endpoints_follow_each_sdk_convention() {
        let openai = config(Format::OpenAi, "https://api.example.com/v1/");
        assert_eq!(
            openai.endpoint().unwrap().as_str(),
            "https://api.example.com/v1/chat/completions"
        );
        let anthropic = config(Format::Anthropic, "https://api.example.com");
        assert_eq!(
            anthropic.endpoint().unwrap().as_str(),
            "https://api.example.com/v1/messages"
        );
        let local = config(Format::OpenAi, "http://127.0.0.1:11434/v1");
        assert!(local.endpoint().is_ok());
        assert!(
            config(Format::OpenAi, "http://localhost:8080/v1")
                .endpoint()
                .is_ok()
        );
        assert!(
            config(Format::OpenAi, "http://[::1]:8080/v1")
                .endpoint()
                .is_ok()
        );
    }

    #[test]
    fn endpoints_that_could_leak_or_redirect_are_refused() {
        for base in [
            "",
            "api.example.com/v1",
            "http://api.example.com/v1",
            "http://192.168.1.2/v1",
            "ftp://api.example.com",
            "https://user:pass@api.example.com/v1",
            "https://api.example.com/v1?key=abc",
            "https://api.example.com/v1#x",
        ] {
            assert!(config(Format::OpenAi, base).endpoint().is_err(), "{base}");
        }
        assert!(Format::parse("gemini").is_err());
        assert_eq!(Format::parse("").unwrap(), Format::OpenAi);
        assert_eq!(Format::parse("anthropic").unwrap(), Format::Anthropic);
    }

    #[test]
    fn turning_it_on_needs_an_endpoint_a_model_and_a_key() {
        let mut on = config(Format::OpenAi, "https://api.example.com/v1");
        assert!(on.validate(true).is_ok());
        assert!(on.validate(false).is_err());
        on.model = " ".into();
        assert!(on.validate(true).is_err());
        on.model = "bad\nmodel".into();
        assert!(on.validate(true).is_err());
        let off = Config::default();
        assert!(off.validate(false).is_ok());
        let off_with_bad_url = Config {
            base_url: "http://example.com".into(),
            ..Config::default()
        };
        assert!(off_with_bad_url.validate(false).is_err());
    }

    #[test]
    fn openai_requests_carry_a_bearer_key_and_an_inline_image() {
        let c = config(Format::OpenAi, "https://api.example.com/v1");
        let look = Look {
            app: "Claude",
            failure: "The native prompt is not visible",
            png: Some(&[1, 2, 3]),
        };
        let r = request(&c, "sk-test-0000", &look).unwrap();
        assert_eq!(r.method, "POST");
        assert_eq!(r.url, "https://api.example.com/v1/chat/completions");
        assert!(
            r.headers
                .contains(&("authorization".into(), "Bearer sk-test-0000".into()))
        );
        let body = body_of(&r);
        assert_eq!(body["model"], "vision-model");
        assert_eq!(body["messages"][0]["role"], "system");
        let content = &body["messages"][1]["content"];
        assert!(content[0]["text"].as_str().unwrap().contains("not visible"));
        assert_eq!(content[1]["image_url"]["url"], "data:image/png;base64,AQID");
    }

    #[test]
    fn anthropic_requests_use_its_headers_and_image_blocks() {
        let c = config(Format::Anthropic, "https://api.example.com");
        let look = Look {
            app: "Codex",
            failure: "desktop changed",
            png: Some(&[1, 2, 3]),
        };
        let r = request(&c, "sk-test-0000", &look).unwrap();
        assert!(
            r.headers
                .contains(&("x-api-key".into(), "sk-test-0000".into()))
        );
        assert!(
            r.headers
                .contains(&("anthropic-version".into(), ANTHROPIC_VERSION.into()))
        );
        assert!(!r.headers.iter().any(|(name, _)| name == "authorization"));
        let body = body_of(&r);
        assert!(body["system"].as_str().unwrap().contains("screenshot"));
        let content = &body["messages"][0]["content"];
        assert_eq!(content[0]["source"]["data"], "AQID");
        assert_eq!(content[0]["source"]["media_type"], "image/png");
        assert!(content[1]["text"].as_str().unwrap().contains("Codex"));
    }

    #[test]
    fn a_look_without_a_screenshot_says_so_and_sends_no_image() {
        let c = config(Format::OpenAi, "https://api.example.com/v1");
        let look = Look {
            app: "Claude",
            failure: "x",
            png: None,
        };
        let body = body_of(&request(&c, "sk-test-0000", &look).unwrap());
        let content = body["messages"][1]["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert!(
            content[0]["text"]
                .as_str()
                .unwrap()
                .contains("No screenshot")
        );
        assert!(request(&c, "", &look).is_err());
        assert!(request(&c, "two words", &look).is_err());
    }

    #[test]
    fn replies_are_read_in_both_formats() {
        let ok = |body: &str| Response {
            status: 200,
            body: body.into(),
        };
        assert_eq!(
            reply(
                Format::OpenAi,
                "k",
                &ok(r#"{"choices":[{"message":{"content":"A dialog covers it."}}]}"#)
            )
            .unwrap(),
            "A dialog covers it."
        );
        assert_eq!(
            reply(
                Format::OpenAi,
                "k",
                &ok(r#"{"choices":[{"message":{"content":[{"type":"text","text":"One."},{"type":"text","text":"Two."}]}}]}"#)
            )
            .unwrap(),
            "One. Two."
        );
        assert_eq!(
            reply(
                Format::Anthropic,
                "k",
                &ok(r#"{"content":[{"type":"text","text":"Sign-in sheet."}]}"#)
            )
            .unwrap(),
            "Sign-in sheet."
        );
        assert!(reply(Format::Anthropic, "k", &ok(r#"{"content":[]}"#)).is_err());
        assert!(reply(Format::OpenAi, "k", &ok("not json")).is_err());
    }

    #[test]
    fn errors_are_bounded_and_never_carry_the_key() {
        let key = "sk-test-secret-0000";
        let response = Response {
            status: 401,
            body: format!(r#"{{"error":{{"message":"invalid key {key}"}}}}"#),
        };
        let error = reply(Format::OpenAi, key, &response).unwrap_err();
        assert!(error.contains("HTTP 401"));
        assert!(error.contains("[key]"));
        assert!(!error.contains(key));
        let echoed = Response {
            status: 200,
            body: format!(r#"{{"content":[{{"type":"text","text":"your key is {key}"}}]}}"#),
        };
        assert!(
            !reply(Format::Anthropic, key, &echoed)
                .unwrap()
                .contains(key)
        );
    }

    #[test]
    fn a_diagnosis_is_one_bounded_line_and_never_replaces_the_failure() {
        let long = format!("line one\n\u{7}line two {}", "x".repeat(400));
        let line = diagnosis(&long).unwrap();
        assert!(!line.contains('\n') && !line.contains('\u{7}'));
        assert_eq!(line.chars().count(), MAX_DIAGNOSIS);
        assert!(line.ends_with('…'));
        assert!(diagnosis(" \n ").is_none());
        let shown = annotate("unavailable: not visible", Ok("A dialog.".into()));
        assert!(shown.starts_with("unavailable: not visible\n"));
        assert!(shown.ends_with("Muster looked: A dialog."));
        assert!(annotate("x", Err("timeout".into())).ends_with("Muster could not look: timeout"));
    }
}
