//! Which Mac shows the desktop chats. Every Mac signed in to the same
//! Claude / ChatGPT account lists the same conversations under the same ids,
//! so the Chat tab has no device list: the core asks every reachable Mac for
//! its chat list and uses one that can show it now.

use muster_agent::desktop_chat::ChatApp;
use muster_proto::v1;

/// A Mac the client can reach.
#[derive(Clone, Debug)]
pub struct Candidate {
    pub device: String,
    pub name: String,
    /// The route the backend uses for it (the local socket for this Mac).
    pub route: String,
    pub rtt_ms: u32,
    pub is_self: bool,
}

/// Why `catalog`'s Mac cannot show `app`'s chats now; `None` when it can.
/// A Mac whose app an agent operation holds is skipped: the agent keeps it,
/// and chat operations there would fail at once.
pub fn unusable(app: ChatApp, catalog: &v1::AgentChatCatalog) -> Option<String> {
    if !catalog.app_running {
        return Some(format!("{} is not running", app.name()));
    }
    if catalog.busy == "agent" {
        return Some(format!("{} is in use by an agent operation", app.name()));
    }
    None
}

/// The best usable Mac among the answers, or why none is. Preferred: the
/// app already in Chat mode (nothing to switch), no agent session of that
/// app running, no chat operation in progress there, then the nearest (this
/// Mac first).
pub fn choose(
    app: ChatApp,
    answers: &[(Candidate, Result<v1::AgentChatCatalog, String>)],
) -> Result<usize, String> {
    let best = answers
        .iter()
        .enumerate()
        .filter_map(|(i, (candidate, answer))| {
            let catalog = answer.as_ref().ok()?;
            unusable(app, catalog)
                .is_none()
                .then_some((i, candidate, catalog))
        })
        .min_by_key(|(_, candidate, catalog)| {
            (
                !catalog.chat_mode,
                catalog.agent_working,
                catalog.busy == "chat",
                if candidate.is_self {
                    0
                } else {
                    candidate.rtt_ms.max(1)
                },
            )
        });
    if let Some((i, _, _)) = best {
        return Ok(i);
    }
    if answers.is_empty() {
        return Err(format!(
            "No Mac is reachable to show {} chats. Desktop chats run on a Mac with Muster and {} open",
            app.name(),
            app.name()
        ));
    }
    let reasons: Vec<String> = answers
        .iter()
        .map(|(candidate, answer)| {
            let why = match answer {
                Ok(catalog) => unusable(app, catalog).unwrap_or_default(),
                Err(error) => error.clone(),
            };
            format!("{}: {why}", candidate.name)
        })
        .collect();
    Err(format!(
        "No Mac can show {} chats now — {}",
        app.name(),
        reasons.join("; ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mac(name: &str, rtt_ms: u32, is_self: bool) -> Candidate {
        Candidate {
            device: name.to_lowercase(),
            name: name.into(),
            route: String::new(),
            rtt_ms,
            is_self,
        }
    }

    fn catalog(running: bool, chat_mode: bool, busy: &str, working: bool) -> v1::AgentChatCatalog {
        v1::AgentChatCatalog {
            provider: "claude".into(),
            app_running: running,
            chat_mode,
            busy: busy.into(),
            agent_working: working,
            ..Default::default()
        }
    }

    #[test]
    fn prefers_chat_mode_then_idle_agents_then_the_nearest() {
        let answers = vec![
            (mac("Near", 5, false), Ok(catalog(true, false, "", false))),
            (
                mac("Chatting", 40, false),
                Ok(catalog(true, true, "", false)),
            ),
            (mac("Working", 1, false), Ok(catalog(true, true, "", true))),
        ];
        assert_eq!(choose(ChatApp::Claude, &answers), Ok(1));

        let answers = vec![
            (mac("Far", 80, false), Ok(catalog(true, true, "", false))),
            (mac("Here", 0, true), Ok(catalog(true, true, "", false))),
        ];
        assert_eq!(choose(ChatApp::Claude, &answers), Ok(1));
    }

    #[test]
    fn skips_macs_an_agent_holds_or_without_the_app() {
        let answers = vec![
            (
                mac("Held", 1, true),
                Ok(catalog(true, true, "agent", false)),
            ),
            (
                mac("Closed", 1, false),
                Ok(catalog(false, false, "", false)),
            ),
            (
                mac("Old", 1, false),
                Err("does not offer desktop chats".into()),
            ),
            (
                mac("Free", 90, false),
                Ok(catalog(true, false, "chat", true)),
            ),
        ];
        assert_eq!(choose(ChatApp::Claude, &answers), Ok(3));

        let none = choose(ChatApp::ChatGpt, &answers[..3]).unwrap_err();
        assert!(
            none.contains("Held: ChatGPT is in use by an agent operation"),
            "{none}"
        );
        assert!(none.contains("Closed: ChatGPT is not running"), "{none}");
        assert!(none.contains("Old: does not offer desktop chats"), "{none}");
        assert!(
            choose(ChatApp::Claude, &[])
                .unwrap_err()
                .contains("No Mac is reachable")
        );
    }
}
