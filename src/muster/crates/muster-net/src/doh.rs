//! DNS for every external Muster connection: RFC 8484, pinned bootstrap
//! addresses, verified HTTPS and bounded answers. If all DoH attempts fail,
//! use the OS resolver scoped to a physical interface (never a tunnel).
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use futures_util::{StreamExt, stream::FuturesUnordered};
use hickory_proto::op::{Message, MessageType, OpCode, Query, ResponseCode};
use hickory_proto::rr::{DNSClass, Name, RData, RecordType};
use reqwest::dns::{Name as HttpName, Resolve, Resolving};

use crate::{Error, reconnect::CONNECT_TIMEOUT};

const MAX_WIRE: usize = 65535;
const MAX_CACHE: usize = 256;
const MAX_TTL: Duration = Duration::from_secs(300);
const PRIMARY_BOOTSTRAP: [&str; 2] = ["223.5.5.5:443", "223.6.6.6:443"];

/// A payload-free OS connection to the same endpoint can activate an iOS
/// on-demand network path before the shared transport retries its sockets.
pub fn bootstrap_endpoint() -> SocketAddr {
    PRIMARY_BOOTSTRAP[0].parse().expect("bootstrap IP")
}

struct NoSystemDns;
impl Resolve for NoSystemDns {
    fn resolve(&self, _: HttpName) -> Resolving {
        Box::pin(async { Err(std::io::Error::other("system DNS is disabled in Muster").into()) })
    }
}

struct Provider {
    client: reqwest::Client,
    url: &'static str,
}
#[derive(Clone)]
struct Answer {
    addresses: Vec<IpAddr>,
    expires: Instant,
}
type Cache = HashMap<(String, RecordType), Answer>;
struct Resolver {
    providers: Vec<Provider>,
    cache: Mutex<Cache>,
}
static RESOLVER: OnceLock<Result<Resolver, String>> = OnceLock::new();

fn resolver() -> Result<&'static Resolver, Error> {
    RESOLVER
        .get_or_init(|| {
            let mut providers = Vec::new();
            for (host, ips) in [
                ("dns.alidns.com", PRIMARY_BOOTSTRAP),
                ("cloudflare-dns.com", ["1.1.1.1:443", "1.0.0.1:443"]),
            ] {
                let addrs: Vec<SocketAddr> = ips
                    .iter()
                    .map(|v| v.parse().expect("bootstrap IP"))
                    .collect();
                let client = reqwest::Client::builder()
                    .no_proxy()
                    .https_only(true)
                    .dns_resolver(Arc::new(NoSystemDns))
                    .resolve_to_addrs(host, &addrs)
                    .redirect(reqwest::redirect::Policy::none())
                    .connect_timeout(CONNECT_TIMEOUT)
                    .timeout(CONNECT_TIMEOUT)
                    .build()
                    .map_err(|e| e.to_string())?;
                providers.push(Provider {
                    client,
                    url: if host == "dns.alidns.com" {
                        "https://dns.alidns.com/dns-query"
                    } else {
                        "https://cloudflare-dns.com/dns-query"
                    },
                });
            }
            Ok(Resolver {
                providers,
                cache: Mutex::new(HashMap::new()),
            })
        })
        .as_ref()
        .map_err(|e| Error::Io(format!("DoH client: {e}")))
}

pub fn invalidate() {
    if let Some(Ok(r)) = RESOLVER.get() {
        r.cache.lock().unwrap().clear();
    }
}

/// Hostnames stay hostnames until here. Numeric addresses need no DNS.
pub async fn resolve(host: &str, port: u16) -> Result<Vec<SocketAddr>, Error> {
    if let Ok(ip) = host.trim_matches(['[', ']']).parse::<IpAddr>() {
        return Ok(vec![SocketAddr::new(ip, port)]);
    }
    // Reject malformed names before either resolver sees them.
    Name::from_ascii(host).map_err(|e| Error::Protocol(format!("DNS name: {e}")))?;
    with_fallback(resolve_doh(host, port), || {
        crate::system_dns::resolve(host, port)
    })
    .await
}

async fn with_fallback<D, F, S>(doh: D, system: F) -> Result<Vec<SocketAddr>, Error>
where
    D: std::future::Future<Output = Result<Vec<SocketAddr>, Error>>,
    F: FnOnce() -> S,
    S: std::future::Future<Output = Result<Vec<SocketAddr>, Error>>,
{
    let primary_error = match doh.await {
        Ok(addresses) if !addresses.is_empty() => return Ok(addresses),
        Ok(_) => "empty DoH answer".to_string(),
        Err(error) => error.to_string(),
    };
    log::warn!("DoH unavailable; trying physical-interface system DNS: {primary_error}");
    let addresses = system()
        .await
        .map_err(|error| Error::Io(format!("{primary_error}; system DNS fallback: {error}")))?;
    if addresses.is_empty() || addresses.iter().any(|a| is_fake(a.ip())) {
        return Err(Error::Protocol(
            "system DNS returned empty or synthetic benchmark addresses".into(),
        ));
    }
    Ok(addresses)
}

async fn resolve_doh(host: &str, port: u16) -> Result<Vec<SocketAddr>, Error> {
    let mut name = Name::from_ascii(host).map_err(|e| Error::Protocol(format!("DNS name: {e}")))?;
    name.set_fqdn(true);
    let r = resolver()?;
    let (v4, v6) = tokio::join!(
        r.lookup(&name, RecordType::A),
        r.lookup(&name, RecordType::AAAA)
    );
    let mut result = Vec::new();
    for answer in [&v4, &v6].into_iter().filter_map(|r| r.as_ref().ok()) {
        result.extend(answer.addresses.iter().map(|ip| SocketAddr::new(*ip, port)));
    }
    if result.is_empty() {
        return Err(Error::Io(format!(
            "DoH {host}: no address (A: {}; AAAA: {})",
            v4.err()
                .map(|e| e.to_string())
                .unwrap_or_else(|| "empty".into()),
            v6.err()
                .map(|e| e.to_string())
                .unwrap_or_else(|| "empty".into())
        )));
    }
    Ok(result)
}

impl Resolver {
    async fn lookup(&self, name: &Name, kind: RecordType) -> Result<Answer, Error> {
        let key = (name.to_lowercase().to_ascii(), kind);
        if let Some(answer) = self
            .cache
            .lock()
            .unwrap()
            .get(&key)
            .filter(|a| a.expires > Instant::now())
            .cloned()
        {
            return Ok(answer);
        }
        let query = Query::query(name.clone(), kind);
        let mut request = Message::new();
        request
            .set_id(0)
            .set_recursion_desired(true)
            .add_query(query.clone());
        let body = request
            .to_vec()
            .map_err(|e| Error::Protocol(e.to_string()))?;
        let mut attempts = FuturesUnordered::new();
        for provider in &self.providers {
            attempts.push(query_provider(provider, &query, &body));
        }
        let mut last_error = Error::Io("DoH providers unavailable".into());
        while let Some(result) = attempts.next().await {
            match result {
                Ok(answer) => {
                    let mut cache = self.cache.lock().unwrap();
                    cache.retain(|_, a| a.expires > Instant::now());
                    if cache.len() >= MAX_CACHE {
                        cache.clear();
                    }
                    cache.insert(key, answer.clone());
                    return Ok(answer);
                }
                Err(error) => last_error = error,
            }
        }
        Err(last_error)
    }
}

async fn query_provider(provider: &Provider, query: &Query, body: &[u8]) -> Result<Answer, Error> {
    let mut response = provider
        .client
        .post(provider.url)
        .header("Content-Type", "application/dns-message")
        .header("Accept", "application/dns-message")
        .body(body.to_vec())
        .send()
        .await
        .map_err(|e| Error::Io(format!("DoH HTTPS: {e:?}")))?
        .error_for_status()
        .map_err(|e| Error::Io(format!("DoH HTTP: {e}")))?;
    if response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_none_or(|v| v.split(';').next().unwrap_or("").trim() != "application/dns-message")
    {
        return Err(Error::Protocol("DoH response is not a DNS message".into()));
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_WIRE as u64)
    {
        return Err(Error::Protocol("DoH response too large".into()));
    }
    let age = response
        .headers()
        .get("age")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| Error::Io(e.to_string()))?
    {
        if bytes.len() + chunk.len() > MAX_WIRE {
            return Err(Error::Protocol("DoH response too large".into()));
        }
        bytes.extend_from_slice(&chunk);
    }
    let now = Instant::now();
    let mut answer = parse_answer(&bytes, query, now)?;
    // An intermediary may serve a cached DNS message. Do not extend its TTL.
    answer.expires = now
        + answer
            .expires
            .saturating_duration_since(now)
            .saturating_sub(Duration::from_secs(age as u64));
    Ok(answer)
}

fn parse_answer(bytes: &[u8], query: &Query, now: Instant) -> Result<Answer, Error> {
    let m = Message::from_vec(bytes).map_err(|e| Error::Protocol(format!("DoH DNS: {e}")))?;
    if m.id() != 0
        || m.message_type() != MessageType::Response
        || m.op_code() != OpCode::Query
        || m.truncated()
        || m.queries() != [query.clone()]
        || m.response_code() != ResponseCode::NoError
    {
        return Err(Error::Protocol(
            "DoH response does not match a successful query".into(),
        ));
    }
    // Accept only addresses owned by the queried name or its CNAME chain.
    let mut names = HashSet::from([query.name().to_lowercase()]);
    let mut ttl = MAX_TTL.as_secs() as u32;
    for _ in 0..16 {
        let before = names.len();
        for record in m.answers() {
            if record.dns_class() == DNSClass::IN
                && names.contains(&record.name().to_lowercase())
                && let RData::CNAME(alias) = record.data()
            {
                names.insert(alias.0.to_lowercase());
                ttl = ttl.min(record.ttl());
            }
        }
        if names.len() == before {
            break;
        }
    }
    let mut addresses = Vec::new();
    for record in m.answers() {
        if record.dns_class() != DNSClass::IN || !names.contains(&record.name().to_lowercase()) {
            continue;
        }
        let ip = match record.data() {
            RData::A(value) if query.query_type() == RecordType::A => IpAddr::V4(value.0),
            RData::AAAA(value) if query.query_type() == RecordType::AAAA => IpAddr::V6(value.0),
            _ => continue,
        };
        if is_fake(ip) {
            return Err(Error::Protocol(
                "DoH returned a synthetic benchmark address".into(),
            ));
        }
        ttl = ttl.min(record.ttl());
        if !addresses.contains(&ip) {
            addresses.push(ip);
        }
    }
    // Empty answers are retried soon; never retain a negative response forever.
    if addresses.is_empty() {
        ttl = ttl.min(5);
    }
    Ok(Answer {
        addresses,
        expires: now + Duration::from_secs(ttl as u64),
    })
}
fn is_fake(ip: IpAddr) -> bool {
    matches!(ip, IpAddr::V4(v) if v.octets()[0] == 198 && v.octets()[1] & 0xfe == 18)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hickory_proto::rr::{
        Record,
        rdata::{A, CNAME},
    };

    fn query() -> Query {
        Query::query(Name::from_ascii("relay.example.").unwrap(), RecordType::A)
    }
    fn response() -> Message {
        let mut m = Message::new();
        m.set_id(0)
            .set_message_type(MessageType::Response)
            .add_query(query());
        m
    }
    #[test]
    fn cname_chain_is_bound_to_question_and_ttl() {
        let now = Instant::now();
        let mut m = response();
        m.add_answer(Record::from_rdata(
            query().name().clone(),
            10,
            RData::CNAME(CNAME(Name::from_ascii("target.example.").unwrap())),
        ));
        m.add_answer(Record::from_rdata(
            Name::from_ascii("target.example.").unwrap(),
            60,
            RData::A(A("203.0.113.5".parse().unwrap())),
        ));
        m.add_answer(Record::from_rdata(
            Name::from_ascii("unrelated.example.").unwrap(),
            1,
            RData::A(A("203.0.113.6".parse().unwrap())),
        ));
        let a = parse_answer(&m.to_vec().unwrap(), &query(), now).unwrap();
        assert_eq!(a.addresses, vec!["203.0.113.5".parse::<IpAddr>().unwrap()]);
        assert_eq!(a.expires, now + Duration::from_secs(10));
    }
    #[test]
    fn rejects_mismatched_truncated_and_fake_answers() {
        let mut m = response();
        m.set_id(123);
        assert!(parse_answer(&m.to_vec().unwrap(), &query(), Instant::now()).is_err());
        m.set_id(0).set_truncated(true);
        assert!(parse_answer(&m.to_vec().unwrap(), &query(), Instant::now()).is_err());
        m.set_truncated(false).add_answer(Record::from_rdata(
            query().name().clone(),
            10,
            RData::A(A("198.18.2.71".parse().unwrap())),
        ));
        assert!(parse_answer(&m.to_vec().unwrap(), &query(), Instant::now()).is_err());
        let other = Query::query(Name::from_ascii("other.example.").unwrap(), RecordType::A);
        assert!(parse_answer(&response().to_vec().unwrap(), &other, Instant::now()).is_err());
    }
    #[test]
    fn zero_ttl_is_not_extended() {
        let now = Instant::now();
        let mut m = response();
        m.add_answer(Record::from_rdata(
            query().name().clone(),
            0,
            RData::A(A("203.0.113.5".parse().unwrap())),
        ));
        assert_eq!(
            parse_answer(&m.to_vec().unwrap(), &query(), now)
                .unwrap()
                .expires,
            now
        );
    }
    #[tokio::test]
    async fn doh_bootstrap_never_uses_system_dns_and_numeric_addresses_need_no_lookup() {
        assert!(
            NoSystemDns
                .resolve("unconfigured.example".parse().unwrap())
                .await
                .is_err()
        );
        assert_eq!(
            resolve("127.0.0.1", 1234).await.unwrap(),
            vec!["127.0.0.1:1234".parse::<SocketAddr>().unwrap()]
        );
        assert_eq!(
            resolve("[::1]", 1234).await.unwrap(),
            vec!["[::1]:1234".parse::<SocketAddr>().unwrap()]
        );
    }

    #[tokio::test]
    async fn physical_dns_is_only_used_after_doh_failure() {
        let ip: SocketAddr = "203.0.113.5:9443".parse().unwrap();
        assert_eq!(
            with_fallback(async { Ok(vec![ip]) }, || async {
                panic!("successful DoH must not invoke system DNS")
            })
            .await
            .unwrap(),
            vec![ip]
        );
        assert_eq!(
            with_fallback(async { Err(Error::Timeout("DoH")) }, || async {
                Ok(vec![ip])
            })
            .await
            .unwrap(),
            vec![ip]
        );
        assert!(
            with_fallback(async { Ok(vec![]) }, || async {
                Ok(vec!["198.18.0.1:9443".parse().unwrap()])
            })
            .await
            .is_err()
        );
        assert!(
            with_fallback(async { Err(Error::Timeout("DoH")) }, || async {
                Err(Error::Timeout("physical DNS"))
            })
            .await
            .unwrap_err()
            .to_string()
            .contains("physical DNS")
        );
    }
}
