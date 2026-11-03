//! The configuration file: a list of rules, each forwarding one protocol from
//! one listening address to one upstream address, and the limits shared by
//! every rule.

use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::time::Duration;

use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Proto {
    Tcp,
    Udp,
}

impl std::fmt::Display for Proto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Proto::Tcp => "tcp",
            Proto::Udp => "udp",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub proto: Proto,
    /// A specific local address. Waited for when it is not up yet, as a
    /// Muster vnet address is until musterd brings its interface up.
    pub listen: SocketAddr,
    pub upstream: SocketAddr,
}

impl std::fmt::Display for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {} -> {}", self.proto, self.listen, self.upstream)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub rules: Vec<Rule>,
    /// How long a TCP connection to the upstream may take.
    #[serde(default = "default_connect_timeout_secs")]
    pub connect_timeout_secs: u64,
    /// A UDP client with no traffic either way for this long loses its
    /// upstream socket; its next datagram starts a new one.
    #[serde(default = "default_udp_idle_secs")]
    pub udp_idle_secs: u64,
    /// Per TCP rule; further clients wait in the listen backlog.
    #[serde(default = "default_max")]
    pub max_tcp_connections: usize,
    /// Per UDP rule; datagrams from further clients are dropped.
    #[serde(default = "default_max")]
    pub max_udp_sessions: usize,
}

fn default_connect_timeout_secs() -> u64 {
    5
}
fn default_udp_idle_secs() -> u64 {
    60
}
fn default_max() -> usize {
    1024
}

/// How rules behave; `Config` sets these, tests shorten them.
#[derive(Clone, Debug)]
pub struct Options {
    pub connect_timeout: Duration,
    pub udp_idle: Duration,
    pub max_tcp_connections: usize,
    pub max_udp_sessions: usize,
    /// How long a listener waits before binding again.
    pub rebind: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(default_connect_timeout_secs()),
            udp_idle: Duration::from_secs(default_udp_idle_secs()),
            max_tcp_connections: default_max(),
            max_udp_sessions: default_max(),
            rebind: Duration::from_secs(2),
        }
    }
}

impl Config {
    pub fn parse(text: &str) -> Result<Self, String> {
        let config: Config = serde_json::from_str(text).map_err(|e| e.to_string())?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.rules.is_empty() {
            return Err("no rules".into());
        }
        if self.connect_timeout_secs == 0 || self.udp_idle_secs == 0 {
            return Err("timeouts must be at least one second".into());
        }
        if self.max_tcp_connections == 0 || self.max_udp_sessions == 0 {
            return Err("limits must be at least 1".into());
        }
        let mut listening = BTreeSet::new();
        for rule in &self.rules {
            // A wildcard would offer the upstream on every network this
            // machine is on, not just the one the rule was written for.
            if rule.listen.ip().is_unspecified() {
                return Err(format!(
                    "{rule}: listen on a specific address, not {}",
                    rule.listen.ip()
                ));
            }
            if rule.listen.port() == 0 {
                return Err(format!("{rule}: the listen port must be fixed"));
            }
            if rule.upstream.ip().is_unspecified() || rule.upstream.port() == 0 {
                return Err(format!("{rule}: the upstream needs an address and a port"));
            }
            if rule.listen == rule.upstream {
                return Err(format!("{rule}: forwards to itself"));
            }
            if !listening.insert((rule.proto, rule.listen)) {
                return Err(format!("{} {} is listed twice", rule.proto, rule.listen));
            }
        }
        Ok(())
    }

    pub fn options(&self) -> Options {
        Options {
            connect_timeout: Duration::from_secs(self.connect_timeout_secs),
            udp_idle: Duration::from_secs(self.udp_idle_secs),
            max_tcp_connections: self.max_tcp_connections,
            max_udp_sessions: self.max_udp_sessions,
            ..Options::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(rules: &str) -> Result<Config, String> {
        Config::parse(&format!(r#"{{"rules": [{rules}]}}"#))
    }

    #[test]
    fn one_port_forwarded_for_both_protocols() {
        let config = rules(
            r#"{"proto": "tcp", "listen": "192.168.13.13:7897", "upstream": "127.0.0.1:7897"},
               {"proto": "udp", "listen": "192.168.13.13:7897", "upstream": "127.0.0.1:7897"}"#,
        )
        .unwrap();
        assert_eq!(config.rules.len(), 2);
        assert_eq!(config.rules[1].proto, Proto::Udp);
        let options = config.options();
        assert_eq!(options.connect_timeout, Duration::from_secs(5));
        assert_eq!(options.udp_idle, Duration::from_secs(60));
        assert_eq!(options.max_tcp_connections, 1024);
    }

    #[test]
    fn the_example_configuration_is_valid() {
        Config::parse(include_str!("../config.example.json")).unwrap();
    }

    /// A wildcard listener would expose the upstream on the physical LAN
    /// too, beyond what the rule's author had in mind.
    #[test]
    fn a_rule_listens_on_one_address() {
        for listen in ["0.0.0.0:7897", "[::]:7897"] {
            let config = rules(&format!(
                r#"{{"proto": "tcp", "listen": "{listen}", "upstream": "127.0.0.1:7897"}}"#
            ));
            assert!(config.is_err(), "{listen}");
        }
    }

    #[test]
    fn mistakes_are_refused() {
        for bad in [
            r#"{"proto": "tcp", "listen": "127.0.0.1:0", "upstream": "127.0.0.1:7897"}"#,
            r#"{"proto": "tcp", "listen": "127.0.0.2:7897", "upstream": "127.0.0.1:0"}"#,
            r#"{"proto": "tcp", "listen": "127.0.0.2:7897", "upstream": "0.0.0.0:7897"}"#,
            r#"{"proto": "udp", "listen": "127.0.0.1:7897", "upstream": "127.0.0.1:7897"}"#,
            r#"{"proto": "sctp", "listen": "127.0.0.2:7897", "upstream": "127.0.0.1:7897"}"#,
            r#"{"proto": "tcp", "listen": "127.0.0.2:7897", "upstream": "127.0.0.1:7897", "x": 1}"#,
            r#"{"proto": "tcp", "listen": "127.0.0.2:7897", "upstream": "127.0.0.1:7897"},
               {"proto": "tcp", "listen": "127.0.0.2:7897", "upstream": "127.0.0.1:8080"}"#,
        ] {
            assert!(rules(bad).is_err(), "{bad}");
        }
        assert!(Config::parse(r#"{"rules": []}"#).is_err());
        assert!(
            Config::parse(
                r#"{"rules": [{"proto": "tcp", "listen": "127.0.0.2:1", "upstream": "127.0.0.1:1"}],
                    "udp_idle_secs": 0}"#
            )
            .is_err()
        );
    }
}
