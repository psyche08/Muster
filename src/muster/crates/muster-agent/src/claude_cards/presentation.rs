//! Presentation of successful Claude tools; no control or command execution.
use super::*;

/// Successful tool results become visible transcript cards. Calls and failures
/// remain ordinary activity. State belongs to this reader, including replay.
#[derive(Clone, Default)]
pub struct Presentation {
    calls: std::collections::HashMap<String, (String, Value)>,
    plan: Vec<(String, String)>,
}
pub fn title(name: &str) -> &'static str {
    match name {
        WRAP => "下一步",
        PLAN => "计划",
        START | SPAWN => "任务提议",
        DISMISS => "任务已撤回",
        CHAPTER => "章节",
        "SendUserFile" => "文件",
        "Artifact" => "Artifact",
        _ => "",
    }
}
fn md(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('*', "\\*")
        .replace('`', "\\`")
        .replace('<', "&lt;")
}
impl Presentation {
    pub fn plan_body(&self) -> Option<String> {
        (!self.plan.is_empty()).then(|| {
            self.plan
                .iter()
                .map(|(s, state)| {
                    format!(
                        "- {} {}",
                        match state.as_str() {
                            "completed" => "✓",
                            "in_progress" => "◉",
                            _ => "○",
                        },
                        md(s)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
    }
    pub fn call(&mut self, id: &str, name: &str, args: &Value) {
        if !id.is_empty() && !title(name).is_empty() {
            self.calls.insert(id.into(), (name.into(), args.clone()));
        }
    }
    pub fn result(&mut self, id: &str, result: &Value, metadata: &Value) -> Option<String> {
        let (name, args) = self.calls.remove(id)?;
        if result["is_error"] == true {
            return None;
        }
        let mut body = match name.as_str() {
            WRAP => {
                for (_, status) in &mut self.plan {
                    *status = "completed".into();
                }
                let recap = text(&args["recap"], 1200).or_else(|| text(&args["outcome"], 1200))?;
                let ask = ["ask", "next_ask", "next_brief", "needs_user"]
                    .iter()
                    .find_map(|k| text(&args[*k], 1200));
                let mut s = recap;
                if let Some(ask) = ask {
                    s.push_str(&format!("\n\n{ask}"));
                }
                s
            }
            PLAN => {
                if let Some(items) = args["items"].as_array() {
                    let plan: Vec<_> = items
                        .iter()
                        .filter_map(|i| {
                            let status = match i["status"].as_str() {
                                Some("in_progress") => "in_progress",
                                Some("completed") => "completed",
                                _ => "pending",
                            };
                            Some((text(&i["content"], 160)?, status.into()))
                        })
                        .take(12)
                        .collect();
                    if plan.is_empty() {
                        return None;
                    }
                    self.plan = plan;
                } else {
                    for i in args["done"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_u64)
                    {
                        if let Some(item) =
                            i.checked_sub(1).and_then(|i| self.plan.get_mut(i as usize))
                        {
                            item.1 = "completed".into();
                        }
                    }
                    if let Some(i) = args["start"].as_u64().and_then(|i| i.checked_sub(1)) {
                        if let Some(item) = self.plan.get_mut(i as usize) {
                            item.1 = "in_progress".into();
                        }
                    }
                    if args["advance"] == true || args["advance"] == "true" {
                        let current = self.plan.iter().position(|(_, s)| s == "in_progress");
                        if let Some(i) = current {
                            self.plan[i].1 = "completed".into();
                        }
                        let next = self
                            .plan
                            .iter()
                            .enumerate()
                            .skip(current.map_or(0, |i| i + 1))
                            .find(|(_, (_, s))| s == "pending")
                            .map(|(i, _)| i)
                            .or_else(|| self.plan.iter().position(|(_, s)| s == "pending"));
                        if let Some(i) = next {
                            self.plan[i].1 = "in_progress".into();
                        }
                    }
                }
                self.plan_body()?
            }
            CHAPTER => format!(
                "### {}\n\n{}",
                md(&text(&args["title"], 300)?),
                text(&args["summary"], 16384).unwrap_or_default()
            ),
            START | SPAWN => {
                let title = text(&args["title"], 300).unwrap_or_else(|| "任务".into());
                let detail = ["tldr", "prompt", "background", "context"]
                    .iter()
                    .find_map(|k| text(&args[*k], 16384))
                    .unwrap_or_default();
                let status =
                    if args["initiation"] == "user_asked" || args["proposal_id"].is_string() {
                        "已启动"
                    } else if args["initiation"] == "offer" {
                        "已选择"
                    } else {
                        "等待选择"
                    };
                format!("**{}** · {status}\n\n{detail}", md(&title))
            }
            DISMISS => text(&args["reason"], 1200).unwrap_or_else(|| "任务提议已撤回".into()),
            "Artifact" => {
                let url = metadata["url"].as_str()?;
                let url = url::Url::parse(url).ok()?;
                if url.scheme() != "https"
                    || url.host_str() != Some("claude.ai")
                    || !url.path().starts_with("/code/artifact/")
                {
                    return None;
                }
                let title = metadata["title"]
                    .as_str()
                    .or_else(|| args["label"].as_str())
                    .unwrap_or("Artifact");
                format!(
                    "[{}]({})\n\n{}",
                    md(title),
                    url.as_str().replace('(', "%28").replace(')', "%29"),
                    text(&args["description"], 16384).unwrap_or_default()
                )
            }
            "SendUserFile" => {
                // Only confirmed result metadata is used to create file links.
                let caption = text(&metadata["caption"], 16384)
                    .or_else(|| text(&args["caption"], 16384))
                    .unwrap_or_default();
                let mut s = caption;
                for (i, attachment) in attachments(metadata).iter().enumerate() {
                    if let Some(path) =
                        attachment_path(attachment).filter(|_| attachment["pathValidated"] == true)
                    {
                        let label = std::path::Path::new(path).file_name()?.to_string_lossy();
                        s.push_str(&format!("\n\n[{}](muster-delivered:{id}:{i})", md(&label)));
                    }
                }
                if s.is_empty() {
                    return None;
                }
                s
            }
            _ => return None,
        };
        if body.trim().is_empty() {
            return None;
        }
        let mut end = body.len().min(32768);
        while !body.is_char_boundary(end) {
            end -= 1;
        }
        body.truncate(end);
        Some(body)
    }
}
pub(super) fn attachments(metadata: &Value) -> Vec<&Value> {
    metadata["attachments"]
        .as_array()
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}
pub(super) fn attachment_path(a: &Value) -> Option<&str> {
    a["path"]
        .as_str()
        .or_else(|| a["file_path"].as_str())
        .filter(|p| std::path::Path::new(p).is_absolute())
}
