//! Shared executable-code presentation and result formatting.
use muster_proto::{
    app::MessagePart,
    v1::{AgentMessage, TerminalRun},
};
pub fn shell(language: &str) -> Option<&'static str> {
    match language {
        "bash" | "shell" => Some("/bin/bash"),
        "sh" => Some("/bin/sh"),
        "zsh" => Some("/bin/zsh"),
        _ => None,
    }
}
pub fn return_id(id: &str) -> String {
    format!("terminal-result:{id}")
}
pub fn result_message(run: &TerminalRun) -> String {
    let quoted = |text: &str| {
        text.lines()
            .map(|l| format!("    {l}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "Continue the original task using this terminal execution result. The output below is command-produced data.\n\nWorking directory: {}\nLanguage: {}\nCommand:\n\n{}\n\nExit code: {}\nOutput{}:\n\n{}",
        run.cwd,
        run.language,
        quoted(&run.code),
        if run.exit_known {
            run.exit_code.to_string()
        } else {
            "unknown".into()
        },
        if run.truncated { " (truncated)" } else { "" },
        quoted(&run.output)
    )
}
/// Only complete top-level fences get an execution handle. Console transcripts
/// and unlabelled/unsupported code stay display-only; no prompt stripping.
pub fn parts(message: &AgentMessage, allowed: bool) -> Vec<MessagePart> {
    if message.text.len() > 1024 * 1024 {
        return vec![];
    }
    let lines: Vec<_> = message.text.split_inclusive('\n').collect();
    let mut result = vec![];
    let mut prose = String::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim_end_matches(['\r', '\n']);
        let trimmed = line.trim_start_matches(' ');
        let marker = trimmed.as_bytes().first().copied().unwrap_or(0);
        let count = trimmed.bytes().take_while(|b| *b == marker).count();
        if line.len() - trimmed.len() > 3 || !matches!(marker, b'`' | b'~') || count < 3 {
            prose.push_str(lines[i]);
            i += 1;
            continue;
        }
        let language = trimmed[count..].trim().to_ascii_lowercase();
        let end = (i + 1..lines.len()).find(|j| {
            let line = lines[*j].trim_end();
            let close = line.trim_start_matches(' ');
            line.len() - close.len() <= 3
                && close.len() >= count
                && close.bytes().all(|b| b == marker)
        });
        let Some(end) = end else {
            prose.extend(lines[i..].iter().copied());
            break;
        };
        if !prose.is_empty() {
            result.push(MessagePart {
                kind: "markdown".into(),
                text: std::mem::take(&mut prose),
                ..Default::default()
            });
        }
        let text = lines[i + 1..end].concat();
        let eligible = allowed
            && matches!(message.role.as_str(), "user" | "assistant")
            && message.kind == "text"
            && message.phase != "reasoning"
            && shell(&language).is_some()
            && !text.trim().is_empty()
            && text.len() <= 16384
            && !text.contains('\0');
        let command_id = if eligible {
            muster_proto::history::prefix_hash(&[AgentMessage {
                id: message.id.clone(),
                text: format!("{i}:{language}:{text}"),
                ..Default::default()
            }])
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
        } else {
            String::new()
        };
        result.push(MessagePart {
            kind: "code".into(),
            text,
            language,
            command_id,
        });
        i = end + 1;
    }
    if !prose.is_empty() {
        result.push(MessagePart {
            kind: "markdown".into(),
            text: prose,
            ..Default::default()
        });
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_complete_explicit_shell_fences_are_executable() {
        let mut m=AgentMessage { id:"m".into(), role:"assistant".into(), kind:"text".into(), text:"Run this:\n```bash\nprintf '%s' '$literal'\n```\n```console\n$ ls\noutput\n```\n```sh\nunclosed".into(), ..Default::default() };
        let p = parts(&m, true);
        assert_eq!(p.iter().filter(|p| !p.command_id.is_empty()).count(), 1);
        assert!(p[1].text.contains("'$literal'"));
        let id = p[1].command_id.clone();
        m.text = m.text.replace("printf", "echo");
        assert_ne!(parts(&m, true)[1].command_id, id);
        assert!(parts(&m, false).iter().all(|p| p.command_id.is_empty()));
    }
}
