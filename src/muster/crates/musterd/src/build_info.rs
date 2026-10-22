//! Which build this is: the source commit build.rs compiled in, and the
//! release version of the installed package. Display only; never an
//! authorization or compatibility input.

/// `git rev-parse --short HEAD` when compiled; "unknown" outside a checkout.
pub const COMMIT: &str = env!("MUSTER_GIT_COMMIT");

/// The installed package's version (Velopack's `sq.version` beside the
/// executable on macOS and Windows), else the crate's.
pub fn installed_version() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|exe| std::fs::read_to_string(exe.with_file_name("sq.version")).ok())
        .and_then(|manifest| package_version(&manifest))
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
}

/// `<version>…</version>` of a Velopack nuspec manifest.
fn package_version(manifest: &str) -> Option<String> {
    let start = manifest.find("<version>")? + "<version>".len();
    let end = start + manifest[start..].find("</version>")?;
    let version = manifest[start..end].trim();
    (!version.is_empty()
        && version.len() <= 64
        && version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-+".contains(c)))
    .then(|| version.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_velopack_package_version() {
        let manifest = r#"<?xml version="1.0" encoding="utf-8"?>
<package><metadata><id>Muster</id><version>1.0.20</version><channel>osx-arm64</channel></metadata></package>"#;
        assert_eq!(package_version(manifest).as_deref(), Some("1.0.20"));
        assert_eq!(package_version("<version> </version>"), None);
        assert_eq!(package_version("<version>1.0<b></version>"), None);
        assert_eq!(package_version(r#"<?xml version="1.0"?>"#), None);
        assert!(!COMMIT.is_empty());
    }
}
