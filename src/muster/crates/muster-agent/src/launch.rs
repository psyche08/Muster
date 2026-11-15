//! How a provider recorded a session being started (ADR 0212): the client
//! named in its own transcript or thread metadata. Display only; it never
//! decides who may drive a session.

/// Muster's own native CLI sessions (`orca:`).
pub const MUSTER: &str = "muster";

/// Claude's `entrypoint`: `claude-desktop`, `cli`, `sdk-cli`, ….
pub fn claude(entrypoint: Option<&str>) -> &'static str {
    match entrypoint.unwrap_or("") {
        "claude-desktop" => "desktop",
        "cli" => "cli",
        e if e.starts_with("sdk") => "sdk",
        e if e.contains("vscode") || e.contains("jetbrains") || e.ends_with("ide") => "ide",
        _ => "",
    }
}

/// Codex's `session_meta.originator` and `source` (the same columns in
/// `state_5.sqlite`). The originator names the client; `source` only says
/// which protocol surface (`vscode` is every app-server client).
pub fn codex(originator: Option<&str>, source: Option<&str>) -> &'static str {
    let originator = originator.unwrap_or("").to_ascii_lowercase();
    match originator.as_str() {
        o if o.contains("desktop") => return "desktop",
        "codex_vscode" => return "ide",
        "codex_cli_rs" => return "cli",
        "codex_exec" => return "exec",
        o if o.starts_with("codex_sdk") => return "sdk",
        _ => {}
    }
    match source.unwrap_or("") {
        "cli" => "cli",
        "exec" => "exec",
        "mcp" => "sdk",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_metadata_names_the_starting_client() {
        assert_eq!(claude(Some("claude-desktop")), "desktop");
        assert_eq!(claude(Some("cli")), "cli");
        assert_eq!(claude(Some("sdk-cli")), "sdk");
        assert_eq!(claude(Some("claude-vscode")), "ide");
        assert_eq!(claude(None), "");
        assert_eq!(codex(Some("Codex Desktop"), Some("vscode")), "desktop");
        assert_eq!(codex(Some("codex_cli_rs"), Some("cli")), "cli");
        assert_eq!(codex(None, Some("cli")), "cli");
        assert_eq!(codex(Some("codex_exec"), None), "exec");
        assert_eq!(codex(None, Some("exec")), "exec");
        assert_eq!(codex(Some("codex_vscode"), Some("vscode")), "ide");
        assert_eq!(
            codex(None, Some("vscode")),
            "",
            "the surface alone does not name the client"
        );
    }
}
