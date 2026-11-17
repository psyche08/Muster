//! Redaction: what leaves the device has had secrets and personal data
//! replaced. Runs on the device, before compression, so the collector never
//! holds the originals.
//!
//! Removed outright: credentials in `key=value` / `key: value` form, bearer
//! and basic credentials, JWTs, well-known API key formats, PEM blocks, URL
//! user info, e-mail addresses, mainland China mobile and identity-card
//! numbers. Replaced by a pseudonym (a keyed hash under a salt that never
//! leaves the device, so one value maps to one token across uploads but
//! cannot be guessed back): public IP addresses, MAC addresses, the account
//! name, the host name and the network's member names. Home directories
//! become `~`. Private, loopback, link-local, CGNAT and benchmark addresses
//! stay as they are: they say which path a connection took and name nobody.
//!
//! Redaction is pattern-based; it cannot know every secret a free-form log
//! line may carry, so log statements must still not print secrets or
//! conversation content (ADR 0228).

use std::net::{Ipv4Addr, Ipv6Addr};

use hmac::{Hmac, Mac};
use regex::{Captures, Regex};
use sha2::Sha256;

/// Names too generic to be anyone's: a device called "iPhone" keeps the word
/// readable everywhere else in the log.
const GENERIC_NAMES: &[&str] = &[
    "android",
    "desktop",
    "device",
    "imac",
    "ipad",
    "iphone",
    "linux",
    "localhost",
    "mac",
    "mac mini",
    "mac studio",
    "macbook",
    "macbook air",
    "macbook pro",
    "muster",
    "phone",
    "root",
    "server",
    "user",
    "windows",
];
/// Shorter literals are not redacted: they would match inside ordinary words.
const MIN_LITERAL: usize = 3;

pub struct Redactor {
    salt: [u8; 32],
    home: Option<String>,
    literals: Option<Regex>,
}

struct Patterns {
    pem: Regex,
    key_value: Regex,
    bearer: Regex,
    basic: Regex,
    jwt: Regex,
    api_key: Regex,
    url_userinfo: Regex,
    email: Regex,
    home: Regex,
    mac: Regex,
    ipv6: Regex,
    ipv4: Regex,
    cn_id: Regex,
    cn_mobile: Regex,
}

fn patterns() -> &'static Patterns {
    static PATTERNS: std::sync::OnceLock<Patterns> = std::sync::OnceLock::new();
    PATTERNS.get_or_init(|| {
        let re = |pattern: &str| Regex::new(pattern).expect("redaction pattern");
        Patterns {
            pem: re(r"(?s)-----BEGIN [A-Z0-9 ]+-----.*?-----END [A-Z0-9 ]+-----"),
            key_value: re(concat!(
                r#"(?i)\b((?:access|refresh|id|auth|session|api|push|device|bearer|client|csrf|xsrf)?[_-]?"#,
                r#"(?:token|secret|password|passwd|pwd|passphrase|apikey|api_key|api-key|ssoid|ticket|cookie|"#,
                r#"authorization|credential|credentials|private[_-]?key|sessionid|session[_-]id)s?)"#,
                r#"("?\s*[:=]\s*"?)([^\s"',;&)}\]]+)"#,
            )),
            bearer: re(r"(?i)\b(bearer)\s+[A-Za-z0-9._~+/=-]{8,}"),
            basic: re(r"\b(Basic)\s+([A-Za-z0-9+/]{8,}={0,2})"),
            jwt: re(r"\beyJ[A-Za-z0-9_-]{4,}\.[A-Za-z0-9_-]{4,}\.[A-Za-z0-9_-]{4,}"),
            api_key: re(concat!(
                r"\b(?:sk-(?:ant-|proj-)?[A-Za-z0-9_-]{16,}|gh[pousr]_[A-Za-z0-9]{20,}|",
                r"github_pat_[A-Za-z0-9_]{20,}|xox[abprs]-[A-Za-z0-9-]{10,}|AKIA[0-9A-Z]{16}|",
                r"AIza[0-9A-Za-z_-]{35}|glpat-[A-Za-z0-9_-]{20,})",
            )),
            url_userinfo: re(r"([A-Za-z][A-Za-z0-9+.-]*://)[^/\s@]+@"),
            email: re(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}\b"),
            home: re(r"(/(?:Users|home)/|[A-Za-z]:\\Users\\)([^/\\\s:]+)"),
            mac: re(r"\b[0-9A-Fa-f]{2}(?:[:-][0-9A-Fa-f]{2}){5}\b"),
            ipv6: re(r"[0-9A-Fa-f:]*:[0-9A-Fa-f]*:[0-9A-Fa-f:.]*"),
            ipv4: re(r"\b(?:\d{1,3}\.){3}\d{1,3}\b"),
            cn_id: re(r"\b\d{17}[\dXx]\b"),
            cn_mobile: re(r"\b1[3-9]\d{9}\b"),
        }
    })
}

impl Redactor {
    /// `salt` keys the pseudonyms; `home` is the account's home directory;
    /// `literals` are names to replace wherever they appear as whole words
    /// (the account and host names, the network's device names).
    pub fn new(salt: [u8; 32], home: Option<String>, literals: &[String]) -> Self {
        let mut names: Vec<&str> = literals
            .iter()
            .map(|name| name.trim())
            .filter(|name| name.chars().count() >= MIN_LITERAL)
            .filter(|name| !GENERIC_NAMES.contains(&name.to_lowercase().as_str()))
            .collect();
        // Longest first, so "Ada's MacBook" wins over "Ada".
        names.sort_by_key(|name| std::cmp::Reverse(name.len()));
        names.dedup();
        let literals = (!names.is_empty())
            .then(|| {
                let alternatives: Vec<String> = names
                    .iter()
                    .map(|name| {
                        let word =
                            |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
                        let start = if word(name.chars().next()) { r"\b" } else { "" };
                        let end = if word(name.chars().last()) { r"\b" } else { "" };
                        format!("{start}{}{end}", regex::escape(name))
                    })
                    .collect();
                Regex::new(&format!("(?i)(?:{})", alternatives.join("|"))).ok()
            })
            .flatten();
        let home = home
            .map(|h| h.trim_end_matches(['/', '\\']).to_string())
            .filter(|h| h.len() > 1);
        Self {
            salt,
            home,
            literals,
        }
    }

    /// A stable pseudonym for `value` in `class`.
    fn token(&self, class: &str, value: &str) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.salt).expect("any key length");
        mac.update(class.as_bytes());
        mac.update(&[0]);
        mac.update(value.as_bytes());
        let digest = mac.finalize().into_bytes();
        format!("<{class}:{}>", hex::encode(&digest[..4]))
    }

    pub fn redact(&self, text: &str) -> String {
        let p = patterns();
        let mut text = text.to_string();
        apply(&mut text, &p.pem, |_| "<pem>".into());
        apply(&mut text, &p.jwt, |_| "<jwt>".into());
        apply(&mut text, &p.api_key, |_| "<secret>".into());
        apply(&mut text, &p.bearer, |c| format!("{} <secret>", &c[1]));
        // "Basic" is a credential only when what follows looks encoded, not
        // in "basic authentication failed".
        apply(&mut text, &p.basic, |c| {
            if c[2]
                .bytes()
                .any(|b| b.is_ascii_digit() || b"+/=".contains(&b))
            {
                format!("{} <secret>", &c[1])
            } else {
                c[0].to_string()
            }
        });
        apply(&mut text, &p.key_value, |c| {
            format!("{}{}<secret>", &c[1], &c[2])
        });
        apply(&mut text, &p.url_userinfo, |c| format!("{}<user>@", &c[1]));
        apply(&mut text, &p.email, |_| "<email>".into());
        if let Some(home) = &self.home
            && text.contains(home.as_str())
        {
            text = replace_path_prefix(&text, home);
        }
        apply(&mut text, &p.home, |c| {
            format!("{}{}", &c[1], self.token("user", &c[2]))
        });
        if let Some(literals) = &self.literals {
            apply(&mut text, literals, |c| {
                self.token("name", &c[0].to_lowercase())
            });
        }
        apply(&mut text, &p.mac, |c| {
            self.token("mac", &c[0].to_lowercase())
        });
        apply(&mut text, &p.ipv6, |c| self.ipv6(&c[0]));
        apply(&mut text, &p.ipv4, |c| match c[0].parse::<Ipv4Addr>() {
            Ok(ip) if !keeps_v4(ip) => self.token("ip4", &c[0]),
            _ => c[0].to_string(),
        });
        apply(&mut text, &p.cn_id, |_| "<id>".into());
        apply(&mut text, &p.cn_mobile, |_| "<phone>".into());
        text
    }

    fn ipv6(&self, candidate: &str) -> String {
        // The run may carry a separator before or after the address
        // ("addr: 2001:db8::1:"), or an IPv4 tail.
        let trimmed = candidate.trim_end_matches([':', '.']);
        let trimmed = trimmed
            .strip_prefix(':')
            .filter(|t| !t.starts_with(':'))
            .unwrap_or(trimmed);
        match trimmed.parse::<Ipv6Addr>() {
            Ok(ip) if !keeps_v6(ip) => {
                let at = candidate.find(trimmed).unwrap_or(0);
                format!(
                    "{}{}{}",
                    &candidate[..at],
                    self.token("ip6", &ip.to_string()),
                    &candidate[at + trimmed.len()..]
                )
            }
            _ => candidate.to_string(),
        }
    }
}

fn apply(text: &mut String, re: &Regex, f: impl Fn(&Captures<'_>) -> String) {
    if re.is_match(text) {
        *text = re.replace_all(text, |c: &Captures<'_>| f(c)).into_owned();
    }
}

/// `home` followed by a separator or the end of a word becomes `~`.
fn replace_path_prefix(text: &str, home: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(home) {
        let after = &rest[at + home.len()..];
        let boundary = after.chars().next().is_none_or(|c| {
            c == '/' || c == '\\' || !(c.is_alphanumeric() || c == '_' || c == '.' || c == '-')
        });
        out.push_str(&rest[..at]);
        out.push_str(if boundary { "~" } else { home });
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Addresses that identify no one outside the device's own networks.
fn keeps_v4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || a == 0
        // 100.64.0.0/10 shared address space (CGNAT).
        || (a == 100 && (64..128).contains(&b))
        // 198.18.0.0/15 benchmarking, used by local proxies' fake IPs.
        || (a == 198 && (b == 18 || b == 19))
        // 192.0.0.0/24 IETF protocol assignments (e.g. 192.0.0.6 on cellular).
        || (a == 192 && b == 0 && ip.octets()[2] == 0)
        || a >= 240
}

/// Only global unicast (2000::/3) outside the documentation prefix is
/// someone's address; everything else (loopback, link-local, ULA,
/// multicast, mapped private IPv4) stays.
fn keeps_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return keeps_v4(v4);
    }
    let first = ip.segments()[0];
    let global = (0x2000..0x4000).contains(&first);
    let documentation = first == 0x2001 && ip.segments()[1] == 0x0db8;
    !global || documentation
}

#[cfg(test)]
mod tests {
    use super::*;

    fn redactor() -> Redactor {
        Redactor::new(
            [7; 32],
            Some("/Users/alice".into()),
            &[
                "alice".into(),
                "Ada's MacBook".into(),
                "iPhone".into(),
                "workbox".into(),
                "张三的手机".into(),
            ],
        )
    }

    #[test]
    fn credentials_are_removed() {
        let r = redactor();
        for (line, gone) in [
            ("auth token=abcd1234efgh", "abcd1234efgh"),
            (r#"{"access_token": "s3cr3t-value"}"#, "s3cr3t-value"),
            ("password: hunter22", "hunter22"),
            ("Authorization: Bearer abcdefghijklmnop", "abcdefghijklmnop"),
            ("ssoid=0a1b2c3d4e5f&x=1", "0a1b2c3d4e5f"),
            (
                "key sk-ant-api03-AAAAAAAAAAAAAAAAAAAA",
                "AAAAAAAAAAAAAAAAAAAA",
            ),
            (
                // Split so the source holds no token-shaped literal.
                concat!("ghp", "_abcdefghijklmnopqrstuvwxyz0123"),
                "abcdefghijklmnopqrstuvwxyz0123",
            ),
            ("jwt eyJhbGciOi.eyJzdWIiOiIx.c2lnbmF0dXJl", "c2lnbmF0dXJl"),
            ("https://bob:pw@example.com/x", "bob:pw"),
            ("mail ada@example.com now", "ada@example.com"),
            ("call 13812345678 now", "13812345678"),
            ("id 11010519491231002X", "11010519491231002X"),
            (
                "-----BEGIN TEST KEY-----\nMIIabc\n-----END TEST KEY-----",
                "MIIabc",
            ),
        ] {
            let out = r.redact(line);
            assert!(!out.contains(gone), "{line:?} → {out:?}");
        }
    }

    #[test]
    fn home_user_and_names_are_pseudonymous() {
        let r = redactor();
        let out = r.redact(
            "watching /Users/alice/.codex/sessions; /Users/bob/x; approved Ada's MacBook (958afe55) \
             on workbox for alice; iPhone joined; 张三的手机 left",
        );
        assert!(out.contains("watching ~/.codex/sessions"), "{out}");
        assert!(!out.contains("bob"), "{out}");
        assert!(
            !out.contains("Ada's MacBook") && !out.contains("workbox"),
            "{out}"
        );
        assert!(!out.contains(" alice") && !out.contains("张三"), "{out}");
        // A generic device name stays readable; the device id stays.
        assert!(
            out.contains("iPhone joined") && out.contains("958afe55"),
            "{out}"
        );
        // The same name maps to the same token.
        let a = r.redact("Ada's MacBook");
        assert_eq!(a, r.redact("ada's macbook"));
        assert!(a.starts_with("<name:"), "{a}");
    }

    #[test]
    fn public_addresses_are_hashed_and_private_ones_kept() {
        let r = redactor();
        let line = "peer 958afe55 candidates [10.78.164.253:54723, 22.145.133.70:54723, \
                    192.168.1.4:54723, 192.0.0.6:54723, 198.19.0.1:54723, 100.96.1.2:1]; \
                    seen as 110.254.172.149:11905";
        let out = r.redact(line);
        for kept in [
            "10.78.164.253:54723",
            "192.168.1.4:54723",
            "192.0.0.6",
            "198.19.0.1",
            "100.96.1.2",
        ] {
            assert!(out.contains(kept), "{kept} lost: {out}");
        }
        assert!(
            !out.contains("22.145.133.70") && !out.contains("110.254.172.149"),
            "{out}"
        );
        assert!(out.contains(":11905"), "the port stays: {out}");
        let again = r.redact("110.254.172.149");
        assert!(out.contains(&again), "stable pseudonym: {out} / {again}");
        // Another device's salt gives another token.
        assert_ne!(
            Redactor::new([8; 32], None, &[]).redact("110.254.172.149"),
            again
        );
    }

    #[test]
    fn ipv6_global_addresses_are_hashed_and_others_kept() {
        let r = redactor();
        let out = r.redact(
            "via [2408:8207:1234:5678::1]:443 and fe80::1%en0, fd7a:115c::5, ::1, \
             2001:db8::7; at 13:46:33Z in muster_net::magicsock and musterd::network",
        );
        assert!(!out.contains("2408:8207"), "{out}");
        assert!(out.contains("]:443"), "{out}");
        for kept in [
            "fe80::1%en0",
            "fd7a:115c::5",
            "::1",
            "2001:db8::7",
            "13:46:33Z",
            "muster_net::magicsock",
            "musterd::network",
        ] {
            assert!(out.contains(kept), "{kept} lost: {out}");
        }
    }

    #[test]
    fn mac_addresses_are_hashed() {
        let out = redactor().redact("wifi a4:83:e7:12:34:56 up");
        assert!(
            !out.contains("a4:83:e7:12:34:56") && out.contains("<mac:"),
            "{out}"
        );
    }

    #[test]
    fn ordinary_log_lines_are_unchanged() {
        let r = redactor();
        for line in [
            "[2026-09-25T14:04:23Z INFO  musterd::network] approved iPhone (958afe55) with {AgentView}",
            "[2026-09-25T13:46:33Z INFO  muster_platform::signer] new Secure Enclave identity GUEP-X6MF-6FUR-UXBK",
            "relay muster.linsheng.dev:9443 via quic, rtt 23 ms, version 1.1.46 (136)",
            "device 3508fbf9a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c registered",
            "token bucket refilled; tokens 4",
        ] {
            assert_eq!(r.redact(line), line);
        }
    }

    #[test]
    fn home_prefix_needs_a_boundary() {
        assert_eq!(
            replace_path_prefix("/srv/alice2/x /srv/alice/y /srv/alice", "/srv/alice"),
            "/srv/alice2/x ~/y ~"
        );
    }
}
