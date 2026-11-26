//! A member's scopes as an admin edits them (app.proto `AccessView`): the
//! Chats, Agents and Terminals modules behind their device-level switches,
//! then everything else. The groups, labels, summaries and join defaults are
//! decided here; a shell only draws them and sends the scopes left on.

use muster_core::{Scope, apps};
use muster_proto::app::{AccessItem, AccessSection, AccessView};

/// What a join is approved with unless the admin changes it.
const JOIN_DEFAULTS: &[&str] = &[
    "chats",
    "chats:chatgpt",
    "chats:claude",
    "agents",
    "agents:codex",
    "agents:claude",
    "agents:catdesk",
    "agents:deepseek",
    "terminals",
];

/// Offered in "Other" whether held or not, in this order.
const OTHER: &[(&str, &str)] = &[
    ("desktop.view", "View an unlocked desktop"),
    (
        "desktop.mouse",
        "View an unlocked desktop and use the mouse",
    ),
    ("browser.view", "Read browser tabs and pages"),
    ("browser.control", "Control browser pages"),
    ("network.admin", "Administer the network"),
];

/// Independent legacy permissions accepted by the CLI.
const EARLIER: &[(&str, &str)] = &[
    ("agent.view", "View agent sessions, every provider"),
    (
        "agent.control",
        "Send to agents and approve, every provider",
    ),
    ("terminal.view", "View terminals"),
    ("terminal.control", "Type into terminals"),
];

pub fn join_defaults(advanced: bool) -> AccessView {
    let held: Vec<String> = JOIN_DEFAULTS.iter().map(|s| s.to_string()).collect();
    let mut result = view(&held, advanced);
    result.sections.retain(|section| section.id != "multica");
    result
}

/// The editor for a member holding `scopes` (as the roster reports them).
pub fn view(scopes: &[String], advanced: bool) -> AccessView {
    // Canonical text, so a scope is found however it was spelled.
    let mut rest: Vec<String> = scopes
        .iter()
        .map(|s| {
            s.parse::<Scope>()
                .map_or_else(|_| s.clone(), |p| p.to_string())
        })
        .collect();
    // Saving a legacy roster through the current editor writes the exact two
    // effective toggles. Revoking the last one cannot revive the old aggregate.
    if rest.iter().any(|s| s == "agents:orca") {
        rest.retain(|s| s != "agents:orca");
        if !rest.iter().any(|s| {
            s.strip_prefix("agents:")
                .is_some_and(|id| apps::native_cli(id).is_some())
        }) {
            rest.extend(
                apps::NATIVE_CLIS
                    .iter()
                    .map(|c| format!("agents:{}", c.provider)),
            );
        }
    }
    let normalized = rest.clone();
    rest.sort();
    rest.dedup();
    let mut take = |scope: &str| match rest.iter().position(|s| s == scope) {
        Some(at) => {
            rest.remove(at);
            true
        }
        None => false,
    };

    let chats = module(
        &mut take,
        "chats",
        "Chats",
        "Conversations shared by desktop chat providers.",
        providers(
            scopes,
            &apps::chat_providers(),
            Scope::chat_provider,
            chat_label,
        ),
        "chats",
    );
    let mut agents = module(
        &mut take,
        "agents",
        "Agents",
        "Coding-agent sessions: view, send, approve and interrupt. Control while the screen is \
         locked also asks for Face ID or Touch ID each time and needs the target to allow it; \
         it covers Chrome and the screen cover too.",
        providers(
            &normalized,
            &apps::agent_providers(),
            Scope::agent_provider,
            agent_label,
        ),
        "agents",
    );
    let locked = take("locked.control");
    agents.items.push(AccessItem {
        scope: "locked.control".into(),
        label: "Control while the screen is locked".into(),
        granted: locked,
        requires_toggle: false,
    });
    if locked {
        agents.summary.push_str(" · locked control");
    }
    let terminals = module(
        &mut take,
        "terminals",
        "Terminals",
        "View and type into every terminal session.",
        Vec::new(),
        "terminals",
    );

    let multica_control = take("multica.control");
    let mut multica = module(
        &mut take,
        "multica",
        "Multica",
        "Shared workspaces only. Control also requires the provider permission. Join first; all members must be online and upgraded before these permissions can be granted.",
        Vec::new(),
        "multica.view",
    );
    multica.items.push(item(
        "multica.control",
        "Control shared task sessions",
        multica_control,
        true,
    ));

    let network_label = if advanced {
        "All protocols and ports"
    } else {
        "Network access"
    };
    let mut network_items = vec![item("net:*", network_label, take("net:*"), false)];
    if advanced {
        network_items.push(item(
            "net:tcp/22",
            "SSH (tcp/22)",
            take("net:tcp/22"),
            false,
        ));
    }
    let earlier_items: Vec<_> = EARLIER
        .iter()
        .map(|(scope, label)| item(scope, label, take(scope), false))
        .collect();
    let mut other_items: Vec<AccessItem> = OTHER
        .iter()
        .map(|(scope, label)| item(scope, label, take(scope), false))
        .collect();
    for scope in rest {
        if scope.starts_with("net:") {
            if advanced {
                network_items.push(item(&scope, &scope, true, false));
            }
        } else {
            other_items.push(item(&scope, &scope, true, false));
        }
    }
    let mut sections = vec![
        chats,
        agents,
        terminals,
        AccessSection {
            id: "network".into(),
            title: "Network".into(),
            footer: "The receiving device chooses which network services it offers.".into(),
            summary: granted_labels(&network_items, "Off"),
            toggle: None,
            items: network_items,
        },
        multica,
        AccessSection {
            id: "other".into(),
            title: "Other".into(),
            footer: String::new(),
            summary: granted_labels(&other_items, "None"),
            toggle: None,
            items: other_items,
        },
    ];
    sections.push(AccessSection {
        id: "earlier".into(),
        title: "Provider-independent access".into(),
        footer: "These permissions apply to every provider, independently of module switches."
            .into(),
        summary: granted_labels(&earlier_items, "None"),
        toggle: None,
        items: earlier_items,
    });
    AccessView {
        sections,
        network_advanced: advanced,
        has_saved_port_rules: !advanced
            && scopes.iter().any(|s| s.starts_with("net:") && s != "net:*"),
    }
}

/// Known providers in their order, then any other the member holds.
fn providers(
    scopes: &[String],
    known: &[&str],
    provider_of: fn(&Scope) -> Option<&str>,
    label: fn(&str) -> &str,
) -> Vec<(String, String)> {
    let parsed: Vec<Scope> = scopes.iter().filter_map(|s| s.parse().ok()).collect();
    let mut ids: Vec<String> = known.iter().map(|p| p.to_string()).collect();
    for held in parsed.iter().filter_map(provider_of) {
        if !ids.iter().any(|id| id == held) {
            ids.push(held.to_string());
        }
    }
    ids.into_iter()
        .map(|id| (label(&id).to_string(), id))
        .collect()
}

fn module(
    take: &mut impl FnMut(&str) -> bool,
    id: &str,
    title: &str,
    footer: &str,
    providers: Vec<(String, String)>,
    switch: &str,
) -> AccessSection {
    let on = take(switch);
    let items: Vec<AccessItem> = providers
        .iter()
        .map(|(label, provider)| {
            let scope = format!("{switch}:{provider}");
            let granted = take(&scope);
            item(&scope, label, granted, true)
        })
        .collect();
    let summary = match (on, items.is_empty()) {
        (false, _) => "Off".to_string(),
        (true, true) => "On".to_string(),
        (true, false) => granted_labels(&items, "No provider"),
    };
    AccessSection {
        id: id.into(),
        title: title.into(),
        footer: footer.into(),
        summary,
        toggle: Some(item(switch, title, on, false)),
        items,
    }
}

fn item(scope: &str, label: &str, granted: bool, requires_toggle: bool) -> AccessItem {
    AccessItem {
        scope: scope.into(),
        label: label.into(),
        granted,
        requires_toggle,
    }
}

fn granted_labels(items: &[AccessItem], none: &str) -> String {
    let labels: Vec<&str> = items
        .iter()
        .filter(|i| i.granted)
        .map(|i| i.label.as_str())
        .collect();
    if labels.is_empty() {
        none.into()
    } else {
        labels.join(", ")
    }
}

/// An unknown provider (from a newer member) shows its own name.
fn chat_label(provider: &str) -> &str {
    apps::chat_label(provider).unwrap_or(provider)
}

fn agent_label(provider: &str) -> &str {
    if let Some(cli) = apps::native_cli(provider) {
        return cli.label;
    }
    apps::app(provider).map_or(provider, |app| app.agent_label)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scopes(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn editing_legacy_cli_scopes_exposes_only_the_effective_native_toggles() {
        let old = view(&scopes(&["agents", "agents:orca"]), false);
        let mixed = view(
            &scopes(&["agents", "agents:orca", "agents:codex-cli"]),
            false,
        );
        for (view, claude) in [(&old, true), (&mixed, false)] {
            let options = &section(view, "agents").items;
            assert!(options.iter().all(|i| i.scope != "agents:orca"));
            assert!(
                options
                    .iter()
                    .any(|i| i.scope == "agents:codex-cli" && i.granted)
            );
            assert_eq!(
                options
                    .iter()
                    .find(|i| i.scope == "agents:claude-cli")
                    .unwrap()
                    .granted,
                claude
            );
        }
    }

    fn section<'a>(view: &'a AccessView, id: &str) -> &'a AccessSection {
        view.sections.iter().find(|s| s.id == id).unwrap()
    }

    /// Every scope the view offers, with whether it is on.
    fn all(view: &AccessView) -> Vec<(String, bool)> {
        view.sections
            .iter()
            .flat_map(|s| s.toggle.iter().chain(&s.items))
            .map(|i| (i.scope.clone(), i.granted))
            .collect()
    }

    #[test]
    fn every_held_scope_appears_once_and_nothing_else_is_on() {
        let held = scopes(&[
            "agents",
            "agents:codex",
            "agents:catdesk",
            "chats:claude",
            "locked.control",
            "net:tcp/5900",
            "agent.view",
            "browser.view",
        ]);
        let view = view(&held, true);
        let offered = all(&view);
        let mut on: Vec<&str> = offered
            .iter()
            .filter(|(_, g)| *g)
            .map(|(s, _)| s.as_str())
            .collect();
        on.sort();
        let mut expected: Vec<&str> = held.iter().map(String::as_str).collect();
        expected.sort();
        assert_eq!(on, expected);
        let mut names: Vec<&str> = offered.iter().map(|(s, _)| s.as_str()).collect();
        let count = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), count, "a scope offered twice");

        let agents = section(&view, "agents");
        assert_eq!(agents.summary, "Codex, CatDesk · locked control");
        assert!(agents.items.iter().any(|i| i.scope == "agents:catdesk"));
        assert!(
            agents
                .items
                .iter()
                .find(|i| i.scope == "locked.control")
                .is_some_and(|i| !i.requires_toggle)
        );
        // A provider without its switch shows as held but the module as off.
        let chats = section(&view, "chats");
        assert_eq!(chats.summary, "Off");
        assert!(chats.items.iter().all(|i| i.requires_toggle));
        assert_eq!(section(&view, "terminals").summary, "Off");
        assert_eq!(
            section(&view, "other").summary,
            "Read browser tabs and pages"
        );
        assert_eq!(
            section(&view, "earlier").summary,
            "View agent sessions, every provider"
        );
    }

    #[test]
    fn basic_network_editor_displays_only_the_whole_service_grant() {
        let held = scopes(&["net:*", "net:tcp/22", "net:udp/2049"]);
        let basic = view(&held, false);
        assert!(!basic.network_advanced);
        assert!(basic.has_saved_port_rules);
        let items = &section(&basic, "network").items;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].scope, "net:*");
        assert!(items[0].granted);

        let advanced = view(&held, true);
        assert!(advanced.network_advanced);
        assert_eq!(section(&advanced, "network").items.len(), held.len());
    }

    #[test]
    fn join_defaults_turn_the_three_modules_on_and_nothing_risky() {
        let view = join_defaults(false);
        assert_eq!(section(&view, "chats").summary, "ChatGPT, Claude");
        assert_eq!(
            section(&view, "agents").summary,
            "Codex, Claude Code, CatDesk, DeepSeek Harness"
        );
        assert_eq!(section(&view, "terminals").summary, "On");
        assert_eq!(section(&view, "other").summary, "None");
        assert_eq!(section(&view, "earlier").summary, "None");
        assert!(all(&view).iter().all(
            |(scope, on)| !on || !matches!(scope.as_str(), "locked.control" | "network.admin")
        ));
    }

    #[test]
    fn a_switch_with_no_provider_says_so() {
        let view = view(&scopes(&["agents", "chats"]), false);
        assert_eq!(section(&view, "agents").summary, "No provider");
        assert_eq!(section(&view, "chats").summary, "No provider");
    }
}
