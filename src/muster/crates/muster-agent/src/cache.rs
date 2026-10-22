//! Cache lifetime estimates are provider-policy evidence, never server expiry
//! claims. Usage timestamps approximate access time; a turn's end does not.
use muster_proto::v1::CacheExpiry;
use serde_json::Value;

pub fn unknown(detail: &str) -> CacheExpiry {
    CacheExpiry {
        label: "Cache TTL unknown".into(),
        detail: detail.into(),
        ..Default::default()
    }
}

fn window(ts: u64, from: u64, until: u64, detail: &str) -> CacheExpiry {
    if ts == 0 {
        return unknown("Cache access timestamp unavailable.");
    }
    CacheExpiry {
        from_unix: ts.saturating_add(from),
        until_unix: if until == 0 {
            0
        } else {
            ts.saturating_add(until)
        },
        label: ttl_label(from, until),
        detail: format!(
            "{detail} Based on the last request's usage timestamp, not turn completion. Later reuse may refresh matching prefixes; this is not live server cache state."
        ),
        ..Default::default()
    }
}

// Keep duration/range semantics in Shared Rust. Apps render this label as-is;
// neither a turn timestamp nor a cache hit percentage establishes a TTL.
fn ttl_label(from: u64, until: u64) -> String {
    fn duration(seconds: u64) -> String {
        if seconds % 3600 == 0 {
            format!("{} h", seconds / 3600)
        } else {
            format!("{} min", seconds / 60)
        }
    }
    let from = duration(from);
    if until == 0 {
        format!("Estimated cache TTL: at least {from}")
    } else if from == duration(until) {
        format!("Estimated cache TTL: {from}")
    } else {
        format!("Estimated cache TTL: {from}–{}", duration(until))
    }
}

pub fn codex(ts: u64, model: &str, retention: &str, cached: u64) -> CacheExpiry {
    if cached == 0 {
        return unknown(
            "No cache read/write evidence in the last request. A miss does not establish cache expiry.",
        );
    }
    // Official policy snapshot 2026-10-01. Do not generalize to unknown models
    // or treat API policy as a server guarantee for a desktop subscription.
    if model.starts_with("gpt-6-")
        || model.starts_with("gpt-6.")
        || model == "gpt-6"
        || model.starts_with("gpt-5.6")
    {
        window(
            ts,
            1800,
            0,
            "OpenAI model API policy: at least 30 minutes after write/reuse; desktop retention is not exposed.",
        )
    } else {
        match retention {
            "in_memory" => window(
                ts,
                300,
                600,
                "Recorded in-memory policy: typically 5–10 minutes idle, possibly up to 1 hour.",
            ),
            "24h" => window(
                ts,
                1800,
                86400,
                "Recorded extended policy: typically around 30 minutes, possibly up to 24 hours.",
            ),
            _ if model == "gpt-5.5" || model.starts_with("gpt-5.5-") => window(
                ts,
                1800,
                86400,
                "OpenAI model API policy: typically around 30 minutes, possibly up to 24 hours; desktop retention is not exposed.",
            ),
            _ => unknown(
                "Retention policy is not recorded for this model. Hit percentage alone cannot determine a TTL.",
            ),
        }
    }
}

pub fn claude(ts: u64, usage: &Value, read: u64, write: u64) -> CacheExpiry {
    let five = usage["cache_creation"]["ephemeral_5m_input_tokens"]
        .as_u64()
        .unwrap_or(0);
    let hour = usage["cache_creation"]["ephemeral_1h_input_tokens"]
        .as_u64()
        .unwrap_or(0);
    if write > 0 && five.saturating_add(hour) == write {
        let (from, until) = if five == 0 {
            (3600, 3600)
        } else if hour == 0 {
            (300, 300)
        } else {
            (300, 3600)
        };
        window(
            ts,
            from,
            until,
            "Recorded Claude write TTLs for newly cached prefixes. Previously read prefixes can have different expiry times.",
        )
    } else if read > 0 || write > 0 {
        window(
            ts,
            300,
            3600,
            "Claude supports 5-minute and 1-hour TTLs; this request does not identify the TTL of its cached prefixes.",
        )
    } else {
        unknown(
            "No cache read/write evidence in the last request. A miss does not establish cache expiry.",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn visible_ttl_labels_keep_exact_range_lower_bound_and_unknown_distinct() {
        assert_eq!(
            window(100, 300, 300, "").label,
            "Estimated cache TTL: 5 min"
        );
        assert_eq!(
            window(100, 300, 3600, "").label,
            "Estimated cache TTL: 5 min–1 h"
        );
        assert_eq!(
            window(100, 1800, 0, "").label,
            "Estimated cache TTL: at least 30 min"
        );
        assert_eq!(window(0, 300, 300, "").label, "Cache TTL unknown");
        assert_eq!(unknown("No evidence").label, "Cache TTL unknown");
    }

    #[test]
    fn claude_write_ttls_mixed_reads_and_missing_timestamps() {
        for (five, hour, from, until) in [(10, 0, 400, 400), (0, 10, 3700, 3700), (4, 6, 400, 3700)]
        {
            let usage = json!({"cache_creation":{"ephemeral_5m_input_tokens":five,"ephemeral_1h_input_tokens":hour}});
            let expiry = claude(100, &usage, 90, 10);
            assert_eq!((expiry.from_unix, expiry.until_unix), (from, until));
            assert_eq!(claude(0, &usage, 90, 10).from_unix, 0);
        }
        let read_only = claude(100, &json!({}), 90, 0);
        assert_eq!((read_only.from_unix, read_only.until_unix), (400, 3700));
        assert_eq!(claude(100, &json!({}), 0, 0).from_unix, 0);
    }

    #[test]
    fn codex_policy_is_model_specific_and_does_not_guess_from_hit_rate() {
        assert_eq!(codex(100, "gpt-6-astra", "", 90).from_unix, 1900);
        assert_eq!(codex(100, "gpt-6-astra", "", 0).from_unix, 0);
        assert_eq!(codex(100, "unknown", "", 90).from_unix, 0);
        let memory = codex(100, "gpt-5", "in_memory", 90);
        assert_eq!((memory.from_unix, memory.until_unix), (400, 700));
        let extended = codex(100, "gpt-5", "24h", 90);
        assert_eq!((extended.from_unix, extended.until_unix), (1900, 86500));
    }
}
