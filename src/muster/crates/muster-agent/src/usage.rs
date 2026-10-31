//! Turn accounting from provider records, with ccusage-compatible input/cache
//! semantics. Prices are a dated offline snapshot, not the user's actual bill.
use crate::model::{Message, MessageKind, Role, parse_ts};
use muster_proto::v1::{CacheExpiry, TurnUsage};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

#[derive(Clone, Default)]
pub struct Tracker {
    model: String,
    tier: String,
    previous: Option<[u64; 4]>,
    requests: HashMap<String, TurnUsage>,
    unknown_requests: HashSet<String>,
    request_order: Vec<String>,
    previous_observed: Option<(String, u64)>,
    modern_codex: bool,
    turn_id: String,
    claude_turn_index: Option<u64>,
    retention: String,
    active: bool,
    sequence: u64,
    changes: crate::changes::Changes,
    cwd: std::path::PathBuf,
}
fn count(v: &Value, key: &str) -> u64 {
    v[key].as_u64().unwrap_or(0)
}
fn codex_counts(v: &Value) -> [u64; 4] {
    [
        count(v, "input_tokens"),
        count(v, "output_tokens"),
        count(v, "cached_input_tokens"),
        v["cache_write_input_tokens"]
            .as_u64()
            .unwrap_or_else(|| count(v, "cache_creation_tokens")),
    ]
}
fn valid_codex_counts(v: &Value) -> bool {
    v["input_tokens"].as_u64().is_some()
        && v["output_tokens"].as_u64().is_some()
        && v["cached_input_tokens"].as_u64().is_some()
        && count(v, "cached_input_tokens").saturating_add(codex_counts(v)[3])
            <= count(v, "input_tokens")
}
fn estimate(
    model: &str,
    tier: &str,
    input: u64,
    output: u64,
    read: u64,
    write: u64,
    write_hour: u64,
) -> Option<f64> {
    static PRICES: OnceLock<Value> = OnceLock::new();
    static FAST: OnceLock<Value> = OnceLock::new();
    let all = PRICES.get_or_init(|| {
        serde_json::from_str(include_str!("usage-prices.json")).expect("price snapshot")
    });
    let base = &all[model]["cost"];
    let mut rates = base;
    if let Some(tiers) = base["tiers"].as_array() {
        for t in tiers {
            if t["tier"]["type"] == "context" && input > count(&t["tier"], "size") {
                rates = t;
            }
        }
    }
    let rate = |key: &str| rates[key].as_f64().or_else(|| base[key].as_f64());
    let regular = input.saturating_sub(read).saturating_sub(write);
    let input_rate = rate("input")?;
    let mut cost = regular as f64 * input_rate + output as f64 * rate("output")?;
    if read > 0 {
        cost += read as f64 * rate("cache_read")?;
    }
    if write > 0 {
        cost += write.saturating_sub(write_hour) as f64 * rate("cache_write")?;
        cost += write_hour as f64 * input_rate * 2.0;
    }
    let multiplier = match tier {
        "" | "default" | "standard" => 1.0,
        "priority" | "fast" => {
            let fast = FAST.get_or_init(|| {
                serde_json::from_str(include_str!("usage-fast-prices.json")).expect("fast prices")
            });
            fast["exact"][model].as_f64().or_else(|| {
                fast["normalized_prefix"]
                    .as_object()?
                    .iter()
                    .find(|(k, _)| model.starts_with(k.as_str()))
                    .and_then(|(_, v)| v.as_f64())
            })?
        }
        _ => return None,
    };
    Some(cost * multiplier / 1_000_000.0)
}
impl Tracker {
    fn start(&mut self) {
        self.changes.reset();
        self.requests.clear();
        self.request_order.clear();
        self.modern_codex = false;
        self.unknown_requests.clear();
        self.active = true;
    }
    fn sample(&self, counts: [u64; 4], write_hour: u64, recorded: Option<f64>) -> TurnUsage {
        let [input, output, read, write] = counts;
        let cost = recorded.or_else(|| {
            estimate(
                &self.model,
                &self.tier,
                input,
                output,
                read,
                write,
                write_hour,
            )
        });
        TurnUsage {
            model: self.model.clone(),
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: read,
            cache_write_tokens: write,
            tokens_known: true,
            cache_hit_percent: if input > 0 {
                read as f64 * 100.0 / input as f64
            } else {
                0.0
            },
            cost_known: cost.is_some(),
            cost_usd: cost.unwrap_or(0.0),
            cost_source: if recorded.is_some() {
                "recorded"
            } else if cost.is_some() {
                "estimated"
            } else {
                "unavailable"
            }
            .into(),
            pricing_date: "2026-09-28".into(),
            changes: Vec::new(),
            changes_incomplete: false,
            cache_hit_known: input > 0,
            ..Default::default()
        }
    }
    fn record_request(
        &mut self,
        key: String,
        mut usage: TurnUsage,
        ts: u64,
        mut expiry: CacheExpiry,
    ) {
        if let Some(old) = self.requests.get(&key) {
            // Repeated content blocks / streaming updates do not refresh cache.
            if usage.output_tokens < old.output_tokens {
                return;
            }
            usage.cache_expiry = old.cache_expiry.clone();
        } else {
            if self.request_order.is_empty()
                && let Some((model, previous_ts)) = &self.previous_observed
                && model == &usage.model
                && *previous_ts > 0
                && ts > *previous_ts
            {
                expiry.observed_gap_seconds = ts - previous_ts;
            }
            self.previous_observed = (ts > 0).then(|| (usage.model.clone(), ts));
            self.request_order.push(key.clone());
            usage.cache_expiry = Some(expiry);
        }
        self.unknown_requests.remove(&key);
        self.requests.insert(key, usage);
    }
    fn finish(&mut self, id: u64, ts: u64) -> Option<Message> {
        if !self.active {
            return None;
        }
        self.active = false;
        let mut sum = TurnUsage {
            model: self.model.clone(),
            tokens_known: !self.requests.is_empty() && self.unknown_requests.is_empty(),
            cost_known: !self.requests.is_empty() && self.unknown_requests.is_empty(),
            cost_source: "recorded".into(),
            pricing_date: "2026-09-28".into(),
            ..Default::default()
        };
        for usage in self.requests.values() {
            sum.input_tokens += usage.input_tokens;
            sum.output_tokens += usage.output_tokens;
            sum.cache_read_tokens += usage.cache_read_tokens;
            sum.cache_write_tokens += usage.cache_write_tokens;
            sum.cost_known &= usage.cost_known;
            sum.cost_usd += usage.cost_usd;
            if usage.cost_source != "recorded" {
                sum.cost_source = "estimated".into();
            }
            if sum.model != usage.model {
                sum.model = "multiple models".into();
            }
        }
        if !sum.cost_known {
            sum.cost_source = "unavailable".into();
            sum.cost_usd = 0.0;
        }
        if sum.input_tokens > 0 {
            sum.cache_hit_percent = sum.cache_read_tokens as f64 * 100.0 / sum.input_tokens as f64;
        }
        sum.cache_hit_known = sum.tokens_known && sum.input_tokens > 0;
        sum.request_count = self.requests.len() as u64;
        if let Some(first) = self
            .request_order
            .first()
            .and_then(|key| self.requests.get(key))
        {
            sum.first_request_cache_hit_known = first.cache_hit_known;
            sum.first_request_cache_hit_percent = first.cache_hit_percent;
        }
        sum.cache_expiry = self
            .request_order
            .last()
            .and_then(|key| self.requests.get(key))
            .and_then(|last| last.cache_expiry.clone());
        if let Some(expiry) = &mut sum.cache_expiry {
            expiry.observed_gap_seconds = self
                .request_order
                .first()
                .and_then(|key| self.requests.get(key))
                .and_then(|first| first.cache_expiry.as_ref())
                .map_or(0, |first| first.observed_gap_seconds);
        }
        if !sum.tokens_known {
            sum.first_request_cache_hit_known = false;
            sum.cache_expiry = Some(crate::cache::unknown(
                "Incomplete turn usage; cache expiry cannot be estimated reliably.",
            ));
        }
        sum.changes = self.changes.snapshot(&self.cwd);
        sum.changes_incomplete = self.changes.incomplete();
        Some(Message {
            id: format!("{id}:usage"),
            role: Role::System,
            kind: MessageKind::TurnSummary,
            text: String::new(),
            ts_unix: ts,
            usage: Some(sum),
            phase: String::new(),
            tool_name: String::new(),
            tool_call_id: String::new(),
        })
    }
    pub fn codex(&mut self, offset: u64, record: &Value) -> Option<Message> {
        let p = &record["payload"];
        let ts = record["timestamp"].as_str().map(parse_ts).unwrap_or(0);
        if let Some(cwd) = p["cwd"].as_str() {
            self.cwd = cwd.into();
        }
        if self.active {
            self.changes.codex(p);
        }
        if record["type"] == "turn_context" {
            self.retention = p["prompt_cache_retention"].as_str().unwrap_or("").into();
            if let Some(model) = p["model"].as_str() {
                self.model = model.into();
            }
            if let Some(tier) = p["service_tier"].as_str() {
                self.tier = tier.into();
            }
        }
        if let Some(tier) = p["thread_settings"]["service_tier"].as_str() {
            self.tier = tier.into();
        }
        if record["type"] == "token_usage_record" && self.active {
            if p["turn_id"]
                .as_str()
                .is_some_and(|id| !self.turn_id.is_empty() && id != self.turn_id)
            {
                return None;
            }
            if !self.modern_codex {
                // A new-format rollout has both records and legacy token_count
                // mirrors. Use response IDs, never sum both streams.
                if !self.requests.is_empty() {
                    self.unknown_requests.insert("mixed_usage_formats".into());
                    self.previous_observed = None;
                }
                self.requests.clear();
                self.request_order.clear();
                self.modern_codex = true;
            }
            let key = p["response_id"]
                .as_str()
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| offset.to_string());
            if valid_codex_counts(&p["usage"]) {
                let counts = codex_counts(&p["usage"]);
                let expiry = crate::cache::codex(
                    ts,
                    &self.model,
                    &self.retention,
                    counts[2].saturating_add(counts[3]),
                );
                self.record_request(key, self.sample(counts, 0, None), ts, expiry);
            } else {
                self.unknown_requests.insert(key);
            }
            return None;
        }
        if record["type"] != "event_msg" {
            return None;
        }
        match p["type"].as_str() {
            Some("task_started") => {
                let id = p["turn_id"].as_str().unwrap_or("");
                if !self.active || id.is_empty() || id != self.turn_id {
                    self.start();
                    self.turn_id = id.into();
                }
            }
            Some("token_count") => {
                let info = &p["info"];
                let total = info["total_token_usage"]
                    .as_object()
                    .map(|_| codex_counts(&info["total_token_usage"]));
                let duplicate = total.is_some() && total == self.previous;
                let counts = if duplicate {
                    None
                } else {
                    valid_codex_counts(&info["last_token_usage"])
                        .then(|| codex_counts(&info["last_token_usage"]))
                        .or_else(|| {
                            total.zip(self.previous).and_then(|(t, prev)| {
                                // A missing baseline or reset cannot be attributed
                                // to this turn. Do not show a session total as a turn.
                                (valid_codex_counts(&info["total_token_usage"])
                                    && (0..4).all(|i| t[i] >= prev[i]))
                                .then(|| std::array::from_fn(|i| t[i] - prev[i]))
                            })
                        })
                };
                if let Some(t) = total {
                    self.previous = Some(t);
                }
                if self.active
                    && !self.modern_codex
                    && !duplicate
                    && info.is_object()
                    && counts.is_none()
                {
                    self.unknown_requests.insert(format!("legacy:{offset}"));
                }
                if self.active
                    && !self.modern_codex
                    && let Some(counts) = counts
                {
                    if counts[2]
                        .checked_add(counts[3])
                        .is_none_or(|cache| cache > counts[0])
                    {
                        self.unknown_requests.insert(format!("legacy:{offset}"));
                        return None;
                    }
                    self.sequence += 1;
                    let expiry = crate::cache::codex(
                        ts,
                        &self.model,
                        &self.retention,
                        counts[2].saturating_add(counts[3]),
                    );
                    self.record_request(
                        self.sequence.to_string(),
                        self.sample(counts, 0, None),
                        ts,
                        expiry,
                    );
                }
            }
            Some("task_complete" | "turn_aborted")
                if p["turn_id"]
                    .as_str()
                    .is_none_or(|id| self.turn_id.is_empty() || id == self.turn_id) =>
            {
                return self.finish(offset, ts);
            }
            _ => {}
        }
        None
    }
    pub fn claude(&mut self, offset: u64, record: &Value) -> Option<Message> {
        if record["isSidechain"] == true || record["isMeta"] == true {
            return None;
        }
        if let Some(cwd) = record["cwd"].as_str() {
            self.cwd = cwd.into();
        }
        let message = &record["message"];
        let ts = record["timestamp"].as_str().map(parse_ts).unwrap_or(0);
        if record["type"] == "user"
            && record["isCompactSummary"] != true
            && record["isVisibleInTranscriptOnly"] != true
        {
            let content = &message["content"];
            // A turn starts with what the viewer shows as the user's prompt.
            let visible_prompt = |text: &str| !crate::claude::user_text(text).is_empty();
            if content.as_str().is_some_and(visible_prompt)
                || content.as_array().is_some_and(|a| {
                    a.iter().any(|b| {
                        b["type"] == "text" && b["text"].as_str().is_some_and(visible_prompt)
                    }) && !a.iter().any(|b| b["type"] == "tool_result")
                })
            {
                let next_index = record["turnPosition"]["turnIndex"].as_u64();
                // Explicit same-turn steering is a continuation. Otherwise a
                // real user prompt starts a new turn; retain the interrupted
                // turn as partial rather than dropping or merging its usage.
                if !self.active || next_index.is_none() || next_index != self.claude_turn_index {
                    let interrupted = if self.active
                        && (!self.requests.is_empty() || !self.unknown_requests.is_empty())
                    {
                        self.unknown_requests
                            .insert("missing_turn_completion".into());
                        self.finish(offset, 0)
                    } else {
                        None
                    };
                    self.start();
                    self.claude_turn_index = next_index;
                    if interrupted.is_some() {
                        return interrupted;
                    }
                }
            }
        }
        if record["type"] == "user" {
            self.changes.claude(record);
        }
        if record["type"] == "assistant" {
            if let Some(model) = message["model"].as_str() {
                self.model = model.into();
            }
            let message_key = format!(
                "{}:{}",
                message["id"].as_str().unwrap_or(""),
                record["requestId"].as_str().unwrap_or("")
            );
            if !self.active && message_key != ":" && self.requests.contains_key(&message_key) {
                return None;
            }
            if !self.active {
                self.start();
            }
            self.changes.claude(record);
            let usage = &message["usage"];
            self.tier = usage["speed"]
                .as_str()
                .or_else(|| usage["service_tier"].as_str())
                .unwrap_or("")
                .into();
            let key = if message_key == ":" {
                offset.to_string()
            } else {
                message_key
            };
            if usage["input_tokens"].as_u64().is_some() && usage["output_tokens"].as_u64().is_some()
            {
                let read = count(usage, "cache_read_input_tokens");
                let write = count(usage, "cache_creation_input_tokens");
                let input = count(usage, "input_tokens") + read + write;
                let recorded = record["costUSD"]
                    .as_f64()
                    .filter(|c| c.is_finite() && *c >= 0.0);
                self.unknown_requests.remove(&key);
                let next = self.sample(
                    [input, count(usage, "output_tokens"), read, write],
                    count(&usage["cache_creation"], "ephemeral_1h_input_tokens"),
                    recorded,
                );
                let expiry = crate::cache::claude(ts, usage, read, write);
                self.record_request(key, next, ts, expiry);
            } else if !self.requests.contains_key(&key) {
                self.unknown_requests.insert(key);
            }
            if matches!(
                message["stop_reason"].as_str(),
                Some("end_turn" | "stop_sequence")
            ) {
                return self.finish(offset, ts);
            }
        }
        if record["type"] == "system" && record["subtype"] == "turn_duration" {
            return self.finish(offset, ts);
        }
        None
    }
}

impl Tracker {
    /// DeepSeek Harness: `turn/start` and `turn/end` bound a turn; each
    /// `assistant/message` is one request, whose `usage` counts uncached
    /// input apart from cache reads and writes. It records neither a cache
    /// lifetime nor file changes Muster reads.
    pub fn deepseek(&mut self, offset: u64, record: &Value) -> Option<Message> {
        let data = &record["data"];
        let ts = record["time"].as_u64().map_or(0, |ms| ms / 1000);
        if let Some(cwd) = record["cwd"].as_str().or_else(|| data["cwd"].as_str()) {
            self.cwd = cwd.into();
        }
        match record["type"].as_str()? {
            "turn/start" => {
                // A turn the log never closed (the app stopped mid-turn).
                let interrupted = if self.active {
                    self.unknown_requests
                        .insert("missing_turn_completion".into());
                    self.finish(offset, 0)
                } else {
                    None
                };
                self.start();
                self.changes.untracked();
                interrupted
            }
            "assistant/message" => {
                if !self.active {
                    self.start();
                    self.changes.untracked();
                }
                let message = &data["message"];
                if let Some(model) = message["source"]["model"].as_str() {
                    self.model = model.into();
                }
                let key = message["id"]
                    .as_str()
                    .map_or_else(|| offset.to_string(), str::to_owned);
                let usage = &data["usage"];
                if usage["inputTokens"].as_u64().is_some()
                    && usage["outputTokens"].as_u64().is_some()
                {
                    let read = count(usage, "cacheReadTokens");
                    let write = count(usage, "cacheWriteTokens");
                    let input = count(usage, "inputTokens") + read + write;
                    let next =
                        self.sample([input, count(usage, "outputTokens"), read, write], 0, None);
                    let expiry =
                        crate::cache::unknown("DeepSeek Harness records no cache lifetime.");
                    self.record_request(key, next, ts, expiry);
                } else if !self.requests.contains_key(&key) {
                    self.unknown_requests.insert(key);
                }
                None
            }
            "turn/end" => self.finish(offset, ts),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A DeepSeek Harness turn sums its requests; cache reads count as
    /// input, no price is guessed, and file changes stay unknown.
    #[test]
    fn deepseek_turns_sum_requests_without_a_price() {
        let mut t = Tracker::default();
        assert!(
            t.deepseek(
                0,
                &json!({"type":"turn/start","time":1_000,"data":{"turn":1}})
            )
            .is_none()
        );
        let reply = |id: &str, input: u64, read: u64| {
            json!({"type":"assistant/message","time":2_000,"data":{"turn":1,"step":1,
                "message":{"role":"assistant","id":id,"content":[],
                    "source":{"kind":"model","provider":"deepseek-official","model":"deepseek-v4.1-flash"}},
                "usage":{"inputTokens":input,"outputTokens":2,"cacheReadTokens":read,"cacheWriteTokens":0}}})
        };
        assert!(t.deepseek(1, &reply("m1", 7779, 1152)).is_none());
        assert!(t.deepseek(2, &reply("m2", 100, 0)).is_none());
        let summary = t
            .deepseek(3, &json!({"type":"turn/end","time":3_000,"data":{"turn":1,"reason":{"kind":"completed"}}}))
            .expect("turn summary");
        assert_eq!(summary.id, "3:usage");
        assert_eq!(summary.ts_unix, 3);
        let usage = summary.usage.unwrap();
        assert_eq!(usage.model, "deepseek-v4.1-flash");
        assert_eq!(
            (usage.input_tokens, usage.cache_read_tokens),
            (7779 + 1152 + 100, 1152)
        );
        assert_eq!((usage.output_tokens, usage.request_count), (4, 2));
        assert!(usage.tokens_known && !usage.cost_known);
        assert_eq!(usage.cost_source, "unavailable");
        assert!(usage.changes.is_empty() && usage.changes_incomplete);
        // A turn the log never ended is closed, partial, by the next start.
        t.deepseek(4, &json!({"type":"turn/start","time":4_000}));
        t.deepseek(5, &reply("m3", 10, 0));
        let partial = t
            .deepseek(6, &json!({"type":"turn/start","time":5_000}))
            .unwrap();
        assert!(!partial.usage.unwrap().tokens_known);
    }
    #[test]
    fn codex_turns_deduplicate_cumulative_events_and_do_not_double_count_cache() {
        let mut t = Tracker::default();
        t.codex(
            0,
            &json!({"type":"event_msg","payload":{"type":"task_started"}}),
        );
        t.codex(
            1,
            &json!({"type":"turn_context","payload":{"model":"gpt-6-astra"}}),
        );
        let record = json!({"type":"event_msg","payload":{"type":"token_count","info":{
            "total_token_usage":{"input_tokens":100,"cached_input_tokens":80,"output_tokens":10},
            "last_token_usage":{"input_tokens":100,"cached_input_tokens":80,"output_tokens":10,"reasoning_output_tokens":5}}}});
        t.codex(2, &record);
        t.codex(3, &record);
        let end = json!({"type":"event_msg","timestamp":"2026-09-28T05:00:00Z","payload":{"type":"task_complete"}});
        let m = t.codex(4, &end).unwrap();
        let u = m.usage.unwrap();
        assert_eq!(
            (u.input_tokens, u.output_tokens, u.cache_read_tokens),
            (100, 10, 80)
        );
        assert_eq!(u.cache_hit_percent, 80.0);
        assert!(u.cost_known);
        assert!((u.cost_usd - 0.00078).abs() < 1e-10);
        assert!(t.codex(5, &end).is_none());
        t.codex(
            6,
            &json!({"type":"event_msg","payload":{"type":"task_started"}}),
        );
        t.codex(7,&json!({"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":130,"cached_input_tokens":90,"output_tokens":12}}}}));
        let u = t.codex(8, &end).unwrap().usage.unwrap();
        assert_eq!(
            (u.input_tokens, u.output_tokens, u.cache_read_tokens),
            (30, 2, 10)
        );
    }
    #[test]
    fn claude_tool_requests_are_summed_once_and_cache_write_is_input() {
        let mut t = Tracker::default();
        t.claude(0, &json!({"type":"user","message":{"content":"work"}}));
        let r = json!({"type":"assistant","requestId":"r1","message":{"id":"m1","model":"claude-sonnet-4-6","stop_reason":"tool_use","usage":{"input_tokens":20,"output_tokens":10,"cache_read_input_tokens":70,"cache_creation_input_tokens":10}}});
        t.claude(1, &r);
        t.claude(2, &r);
        let end = json!({"type":"assistant","requestId":"r2","message":{"id":"m2","model":"claude-sonnet-4-6","stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":90}}});
        let u = t.claude(3, &end).unwrap().usage.unwrap();
        assert_eq!(
            (
                u.input_tokens,
                u.output_tokens,
                u.cache_read_tokens,
                u.cache_write_tokens
            ),
            (200, 15, 160, 10)
        );
        assert_eq!(u.cache_hit_percent, 80.0);
        assert!((u.cost_usd - 0.0004005).abs() < 1e-10);
        assert!(t.claude(4, &end).is_none());
        assert!(
            t.claude(5, &json!({"type":"system","subtype":"turn_duration"}))
                .is_none()
        );
    }
    #[test]
    fn claude_fast_and_missing_requests_are_not_silently_standard_or_complete() {
        let mut t = Tracker::default();
        let r = json!({"type":"assistant","requestId":"r1","message":{"id":"m1","model":"claude-opus-4-6","stop_reason":"tool_use","usage":{"input_tokens":100,"output_tokens":10,"speed":"fast"}}});
        t.claude(0, &r);
        let end = json!({"type":"system","subtype":"turn_duration"});
        let usage = t.claude(1, &end).unwrap().usage.unwrap();
        assert!((usage.cost_usd - 0.0045).abs() < 1e-10);
        t.claude(2, &json!({"type":"user","message":{"content":"again"}}));
        t.claude(3,&json!({"type":"assistant","message":{"id":"missing","model":"claude-opus-4-6","content":[]}}));
        t.claude(4, &r);
        let usage = t.claude(5, &end).unwrap().usage.unwrap();
        assert!(!usage.tokens_known);
        assert!(!usage.cost_known);
    }

    fn codex_start(t: &mut Tracker, id: &str) {
        t.codex(
            0,
            &json!({"type":"event_msg","payload":{"type":"task_started","turn_id":id}}),
        );
        t.codex(
            1,
            &json!({"type":"turn_context","payload":{"model":"gpt-6-astra"}}),
        );
    }
    fn codex_request(t: &mut Tracker, turn: &str, response: &str, ts: &str, input: u64, read: u64) {
        t.codex(
            2,
            &json!({"type":"token_usage_record","timestamp":ts,"payload":{
            "turn_id":turn,"response_id":response,
            "usage":{"input_tokens":input,"cached_input_tokens":read,"output_tokens":10}}}),
        );
    }
    fn codex_end(t: &mut Tracker, id: &str) -> TurnUsage {
        t.codex(
            9,
            &json!({"type":"event_msg","timestamp":"2026-10-01T06:00:00Z",
            "payload":{"type":"task_complete","turn_id":id}}),
        )
        .unwrap()
        .usage
        .unwrap()
    }

    #[test]
    fn response_records_are_turn_scoped_deduplicated_and_weighted() {
        let mut t = Tracker::default();
        codex_start(&mut t, "one");
        codex_request(&mut t, "one", "r1", "2026-10-01T05:00:00Z", 100, 0);
        // The old cumulative stream is a mirror, not another request.
        t.codex(3, &json!({"type":"event_msg","payload":{"type":"token_count","info":{
            "total_token_usage":{"input_tokens":10000,"cached_input_tokens":9000,"output_tokens":100},
            "last_token_usage":{"input_tokens":100,"cached_input_tokens":0,"output_tokens":10}}}}));
        codex_request(&mut t, "one", "r2", "2026-10-01T05:01:00Z", 900, 800);
        codex_request(&mut t, "one", "r2", "2026-10-01T05:20:00Z", 900, 800);
        codex_request(
            &mut t,
            "other-turn",
            "foreign",
            "2026-10-01T05:21:00Z",
            9000,
            8000,
        );
        let u = codex_end(&mut t, "one");
        assert_eq!(
            (u.input_tokens, u.output_tokens, u.request_count),
            (1000, 20, 2)
        );
        assert_eq!(u.cache_hit_percent, 80.0); // not mean(0%, 88.89%)
        assert!(u.cache_hit_known && u.first_request_cache_hit_known);
        assert_eq!(u.first_request_cache_hit_percent, 0.0);
        let expiry = u.cache_expiry.unwrap();
        assert_eq!(expiry.from_unix, parse_ts("2026-10-01T05:31:00Z"));
        assert_eq!(expiry.until_unix, 0); // no invented hard expiry deadline
        codex_start(&mut t, "two");
        codex_request(&mut t, "two", "r3", "2026-10-01T05:41:00Z", 200, 100);
        let u = codex_end(&mut t, "two");
        assert_eq!((u.input_tokens, u.cache_hit_percent), (200, 50.0));
        assert_eq!(u.cache_expiry.unwrap().observed_gap_seconds, 2400);
    }

    #[test]
    fn cumulative_without_baseline_or_after_reset_is_not_turn_usage() {
        let mut t = Tracker::default();
        for (id, total) in [("one", 10000), ("two", 10)] {
            codex_start(&mut t, id);
            t.codex(2, &json!({"type":"event_msg","payload":{"type":"token_count","info":{
                "total_token_usage":{"input_tokens":total,"cached_input_tokens":0,"output_tokens":10}}}}));
            let u = codex_end(&mut t, id);
            assert!(!u.tokens_known && !u.cache_hit_known && !u.cost_known);
            assert_eq!(u.cache_expiry.unwrap().from_unix, 0);
        }
    }

    #[test]
    fn zero_input_and_missing_usage_are_not_zero_percent_hits() {
        let mut t = Tracker::default();
        codex_start(&mut t, "one");
        codex_request(&mut t, "one", "r1", "2026-10-01T05:00:00Z", 0, 0);
        assert!(!codex_end(&mut t, "one").cache_hit_known);
        t.claude(0, &json!({"type":"user","message":{"content":"work"}}));
        let u = t
            .claude(
                1,
                &json!({"type":"assistant","message":{
            "id":"m1","model":"claude-sonnet-4-6","stop_reason":"end_turn","usage":{}}}),
            )
            .unwrap()
            .usage
            .unwrap();
        assert!(!u.tokens_known && !u.cache_hit_known);
    }

    #[test]
    fn claude_steering_and_tool_results_do_not_reset_turn_or_refresh_cache() {
        let mut t = Tracker::default();
        t.claude(0, &json!({"type":"user","message":{"content":"work"}}));
        let first = json!({"type":"assistant","timestamp":"2026-10-01T05:00:00Z",
            "requestId":"r1","message":{"id":"m1","model":"claude-sonnet-4-6",
            "stop_reason":"tool_use","usage":{"input_tokens":20,"output_tokens":10,
                "cache_read_input_tokens":70,"cache_creation_input_tokens":10,
                "cache_creation":{"ephemeral_1h_input_tokens":10,"ephemeral_5m_input_tokens":0}}}});
        t.claude(1, &first);
        t.claude(2, &json!({"type":"user","message":{"content":[{"type":"text","text":"steering"},{"type":"tool_result","content":"ok"}]}}));
        t.claude(
            3,
            &json!({"type":"user","isCompactSummary":true,"message":{"content":"summary"}}),
        );
        let mut last = first.clone();
        last["message"]["id"] = json!("m2");
        last["requestId"] = json!("r2");
        last["timestamp"] = json!("2026-10-01T05:01:00Z");
        t.claude(4, &last);
        last["timestamp"] = json!("2026-10-01T05:10:00Z");
        last["message"]["usage"]["output_tokens"] = json!(15);
        t.claude(5, &last);
        let u = t
            .claude(
                6,
                &json!({"type":"system","subtype":"turn_duration",
            "timestamp":"2026-10-01T05:20:00Z"}),
            )
            .unwrap()
            .usage
            .unwrap();
        assert_eq!(
            (u.input_tokens, u.output_tokens, u.request_count),
            (200, 25, 2)
        );
        assert_eq!(u.cache_hit_percent, 70.0);
        let expiry = u.cache_expiry.unwrap();
        assert_eq!(expiry.from_unix, parse_ts("2026-10-01T06:01:00Z"));
        assert_eq!(expiry.until_unix, expiry.from_unix);
    }

    #[test]
    fn claude_interrupted_turn_is_preserved_without_merging_into_next_prompt() {
        let mut t = Tracker::default();
        t.claude(
            0,
            &json!({"type":"user","turnPosition":{"turnIndex":1},"message":{"content":"first"}}),
        );
        t.claude(1, &json!({"type":"assistant","message":{"id":"m1","model":"claude-sonnet-4-6",
            "stop_reason":"tool_use","usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":90}}}));
        assert!(t.claude(2, &json!({"type":"user","turnPosition":{"turnIndex":1},"message":{"content":"steering"}})).is_none());
        let partial = t.claude(3, &json!({"type":"user","turnPosition":{"turnIndex":2},"message":{"content":"second"}})).unwrap();
        assert_eq!(partial.ts_unix, 0); // no invented completion timestamp
        let u = partial.usage.unwrap();
        assert_eq!(
            (u.input_tokens, u.output_tokens, u.request_count),
            (100, 5, 1)
        );
        assert!(!u.tokens_known && !u.cache_hit_known);
        let u = t.claude(4, &json!({"type":"assistant","message":{"id":"m2","model":"claude-sonnet-4-6",
            "stop_reason":"end_turn","usage":{"input_tokens":20,"output_tokens":10,"cache_read_input_tokens":0}}})).unwrap().usage.unwrap();
        assert!(u.tokens_known);
        assert_eq!(
            (u.input_tokens, u.output_tokens, u.request_count),
            (20, 10, 1)
        );
    }

    #[test]
    fn prices_use_per_request_context_tiers_and_unknown_is_not_free() {
        assert!(
            (estimate("gpt-6-astra", "", 300_000, 100, 200_000, 0, 0).unwrap() - 2.4075).abs()
                < 1e-10
        );
        assert!(estimate("unknown", "", 1, 1, 0, 0, 0).is_none());
        assert!(estimate("gpt-6-astra", "unrecognized", 1, 1, 0, 0, 0).is_none());
        assert!(
            (estimate("gpt-6-astra", "priority", 100, 10, 80, 0, 0).unwrap() - 0.00156).abs()
                < 1e-10
        );
    }
}

#[cfg(test)]
mod change_integration_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn completed_turn_carries_only_its_confirmed_changes() {
        let mut t = Tracker::default();
        t.codex(0, &json!({"type":"turn_context","payload":{"cwd":"/repo"}}));
        t.codex(
            1,
            &json!({"type":"event_msg","payload":{"type":"task_started"}}),
        );
        t.codex(2, &json!({"type":"event_msg","payload":{"type":"item_completed","item":{"type":"FileChange","status":"completed","id":"a","changes":{"/repo/a":{"type":"add","content":"hello\n"}}}}}));
        let first = t
            .codex(
                3,
                &json!({"type":"event_msg","payload":{"type":"task_complete"}}),
            )
            .unwrap();
        assert_eq!(first.usage.unwrap().changes[0].open_path, "a");
        t.codex(
            4,
            &json!({"type":"event_msg","payload":{"type":"task_started"}}),
        );
        let second = t
            .codex(
                5,
                &json!({"type":"event_msg","payload":{"type":"task_complete"}}),
            )
            .unwrap();
        assert!(second.usage.unwrap().changes.is_empty());
    }
}
