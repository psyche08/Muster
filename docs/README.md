# Documentation Index

| Document | Purpose |
|---|---|
| [ARCHITECTURE.md](ARCHITECTURE.md) | **The sole target architecture**: Codex / Claude / Catdesk Desktop → Agent Halo; iTerm2 / Ghostty → independent adapters; all share Muster Networking + Hardware Security. A short entry page (overview, requirements, principles) with a map to its parts in [architecture/](architecture/): runtime, identity and trust, network, services, Agent Halo, Multica, migration, decisions, status and the legacy baseline. All new work follows it. |
| [MODULES.md](MODULES.md) | The module map: crate layers, the dependency rules CI enforces, where each product state machine lives, and which product knowledge is data. |
| [APP_PROFILES.md](APP_PROFILES.md) | The desktop app profile DSL: one TOML file per app (Codex, Claude Code, CatDesk) compiled into `muster_agent::profile`; schema, strategies, capabilities, how profiles are proven. |
| [PROTOCOL.md](PROTOCOL.md) | Wire formats: roster, pairing, links, relay transports, forwarding, vnet, muster-rpc. |
| [SECURITY.md](SECURITY.md) | Threat model and the invariants the code must keep. |
| [LOCKED_USE.md](LOCKED_USE.md) | Locked Use: short-lived signed authorization, temporary unlock, cover and relock; implementation boundaries and device acceptance. |
| [LOCKED_USE_STELLA_REVIEW.md](LOCKED_USE_STELLA_REVIEW.md) | Source comparison of Locked Use with the Stella reference project. |
| [AGENT_INTERFACE.md](AGENT_INTERFACE.md) | Agent catalog, conversation view, turn usage and file previews across clients; links to its dated field records. |
| [DESKTOP_CHAT_MODE.md](DESKTOP_CHAT_MODE.md) | The Claude and ChatGPT apps' ordinary conversations (Chat tab), read and driven through Accessibility. |
| [QUESTION_CARDS.md](QUESTION_CARDS.md) | Structured agent questions (Claude `AskUserQuestion`, Codex `request_user_input`) and how answers are delivered. |
| [CHAT_CATALOG.md](CHAT_CATALOG.md) | The desktop Chats catalog: sources, identity resolution, caching. |
| [MAC_SETUP.md](MAC_SETUP.md) | The Mac startup setup sheet and permission checks. |
| [DESKTOP_UPDATES.md](DESKTOP_UPDATES.md) | Desktop updates (Velopack), permissions restart, packaging and the Multica runbook. Release records up to 1.0.94 are in `validation/`; later ones in ADRs. |
| [NETWORK_DIAGNOSTICS.md](NETWORK_DIAGNOSTICS.md) | Relay access diagnostics and dual-transport acceptance records. |
| [IOS_PERFORMANCE_DIAGNOSTICS.md](IOS_PERFORMANCE_DIAGNOSTICS.md) | Built-in iOS performance monitoring and log export. |
| [P0-validation.md](P0-validation.md) | Four physical-device checks for migration phase 0: purpose, procedure, criteria, fallback. |
| [third-party/](third-party/) | Notices for bundled third-party material. |
| [validation/](validation/) | Dated release, deployment and field records moved out of the reference documents, plus one-off validation reports. |
| [history/](history/) | Superseded architecture, design, test, and acceptance documents, including the AgentHalo UI specifications (`history/design/`). **Historical records only**: neither the target architecture nor a backlog. See [history/README.md](history/README.md) for the index. |

Documentation conventions:

- Update the architecture part that owns a topic when the target architecture changes; do not create another "v2" file. Record decisions in section 7 (`architecture/decisions.md`).
- Keep documents progressively disclosed: a short entry page first, detail in parts linked from it, so a reader loads only what a change needs.
- Put one-off verification and field-investigation records in `history/`, with a date in the filename.
- Do not add more acceptance-gate documents; record each phase's exit criteria in the table in section 5 (`architecture/migration.md`).
