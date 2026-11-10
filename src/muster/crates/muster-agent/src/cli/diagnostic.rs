use serde_json::Value;

/// Project only the provider's user-facing message, never a raw frame or debug
/// metadata. Some Codex versions put a JSON API error inside the message string.
pub(super) fn error_text(error: &Value) -> Option<String> {
    let message = error.as_str().or_else(|| error["message"].as_str())?;
    let nested = (message.len() <= 65_536)
        .then(|| serde_json::from_str::<Value>(message).ok())
        .flatten();
    let message = nested
        .as_ref()
        .and_then(|v| v["error"]["message"].as_str())
        .unwrap_or(message)
        .trim();
    if message.is_empty() {
        return None;
    }
    let mut chars = message
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t');
    let mut text: String = chars.by_ref().take(2000).collect();
    if chars.next().is_some() {
        text.push('…');
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn unwraps_model_version_error_without_dumping_transport_metadata() {
        let value = json!({"message":json!({"status":400,"error":{"message":"The model requires a newer version of Codex."},"debug":"not for display"}).to_string()});
        assert_eq!(
            error_text(&value).as_deref(),
            Some("The model requires a newer version of Codex.")
        );
        assert_eq!(error_text(&json!({"debug":"no user message"})), None);
    }
    #[test]
    fn messages_are_bounded_at_unicode_boundaries() {
        let text = error_text(&json!({"message":"错".repeat(2100)})).unwrap();
        assert_eq!(text.chars().count(), 2001);
        assert!(text.ends_with('…'));
        assert_eq!(error_text(&json!({"message":"  "})), None);
    }
}
