//! OS adapter for interface-scoped system DNS. The caller owns fallback policy.
use std::net::SocketAddr;

use crate::Error;

pub async fn resolve(host: &str, port: u16) -> Result<Vec<SocketAddr>, Error> {
    #[cfg(target_vendor = "apple")]
    {
        let host = host.to_owned();
        tokio::task::spawn_blocking(move || apple::resolve(&host, port))
            .await
            .map_err(|e| Error::Io(format!("system DNS task: {e}")))?
    }
    #[cfg(not(target_vendor = "apple"))]
    {
        let _ = (host, port);
        Err(Error::Io(
            "physical-interface system DNS is not supported on this platform".into(),
        ))
    }
}

#[cfg(target_vendor = "apple")]
mod apple {
    use super::*;
    use std::collections::BTreeMap;
    use std::ffi::{CString, c_char, c_void};
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    use std::time::Instant;

    type Service = *mut c_void;
    type Reply = unsafe extern "C" fn(
        Service,
        u32,
        u32,
        i32,
        *const c_char,
        *const libc::sockaddr,
        u32,
        *mut c_void,
    );
    #[link(name = "System")]
    unsafe extern "C" {
        fn DNSServiceGetAddrInfo(
            service: *mut Service,
            flags: u32,
            interface: u32,
            protocol: u32,
            hostname: *const c_char,
            reply: Reply,
            context: *mut c_void,
        ) -> i32;
        fn DNSServiceRefSockFD(service: Service) -> i32;
        fn DNSServiceProcessResult(service: Service) -> i32;
        fn DNSServiceRefDeallocate(service: Service);
    }

    #[derive(Default)]
    struct Answers {
        ips: Vec<IpAddr>,
        error: Option<i32>,
    }

    struct Query {
        service: Service,
        answers: Box<Answers>,
        name: String,
    }
    impl Drop for Query {
        fn drop(&mut self) {
            // SAFETY: each successful query owns its reference. Callbacks run
            // synchronously in ProcessResult on this same worker thread.
            unsafe { DNSServiceRefDeallocate(self.service) };
        }
    }

    unsafe extern "C" fn reply(
        _service: Service,
        flags: u32,
        _interface: u32,
        error: i32,
        _host: *const c_char,
        address: *const libc::sockaddr,
        _ttl: u32,
        context: *mut c_void,
    ) {
        // SAFETY: context points to the Box retained until after deallocation.
        let answers = unsafe { &mut *context.cast::<Answers>() };
        if error != 0 {
            answers.error = Some(error);
            return;
        }
        if address.is_null() || flags & 2 == 0 {
            return;
        }
        // SAFETY: the API supplies the matching sockaddr variant for sa_family.
        let ip = unsafe {
            match i32::from((*address).sa_family) {
                libc::AF_INET => IpAddr::V4(Ipv4Addr::from(
                    (*address.cast::<libc::sockaddr_in>())
                        .sin_addr
                        .s_addr
                        .to_ne_bytes(),
                )),
                libc::AF_INET6 => IpAddr::V6(Ipv6Addr::from(
                    (*address.cast::<libc::sockaddr_in6>()).sin6_addr.s6_addr,
                )),
                _ => return,
            }
        };
        if !answers.ips.contains(&ip) && answers.ips.len() < 32 {
            answers.ips.push(ip);
        }
    }

    // Darwin's wired/Wi-Fi interfaces are enN, cellular interfaces pdp_ipN.
    // Never fall back to interface 0 (which could select a tunnel resolver).
    fn physical_name(name: &str) -> bool {
        ["en", "pdp_ip"].iter().any(|prefix| {
            name.strip_prefix(prefix)
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()))
        })
    }

    pub fn resolve(host: &str, port: u16) -> Result<Vec<SocketAddr>, Error> {
        let host =
            CString::new(host).map_err(|_| Error::Protocol("invalid DNS hostname".into()))?;
        let mut interfaces = BTreeMap::new();
        for interface in crate::interfaces::snapshot()
            .map_err(|e| Error::Io(e.to_string()))?
            .iter()
        {
            if physical_name(&interface.name)
                && !interface.is_loopback()
                && let Some(index) = interface.index.filter(|i| *i != 0)
            {
                interfaces.insert(index, interface.name.clone());
            }
        }
        let mut queries = Vec::new();
        let mut errors = Vec::new();
        for (index, name) in interfaces {
            let mut answers = Box::<Answers>::default();
            let mut service = std::ptr::null_mut();
            // SAFETY: pointers remain alive for the query. Protocol 0 lets the
            // OS select routable address families; the interface is explicit.
            let result = unsafe {
                DNSServiceGetAddrInfo(
                    &mut service,
                    0x10000,
                    index,
                    0,
                    host.as_ptr(),
                    reply,
                    (&mut *answers as *mut Answers).cast(),
                )
            };
            if result == 0 {
                queries.push(Query {
                    service,
                    answers,
                    name,
                });
            } else {
                errors.push(format!("{name}: {result}"));
            }
        }
        if queries.is_empty() {
            return Err(Error::Io(format!(
                "no physical interface DNS query available: {}",
                errors.join(", ")
            )));
        }
        let deadline = Instant::now() + crate::reconnect::CONNECT_TIMEOUT;
        while Instant::now() < deadline {
            let mut descriptors: Vec<_> = queries
                .iter()
                .map(|q| libc::pollfd {
                    // SAFETY: service is live, and its fd is owned by DNSService.
                    fd: unsafe { DNSServiceRefSockFD(q.service) },
                    events: libc::POLLIN,
                    revents: 0,
                })
                .collect();
            let wait_ms = deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .min(200) as i32;
            // SAFETY: descriptor array is writable and lives through poll.
            let ready = unsafe {
                libc::poll(
                    descriptors.as_mut_ptr(),
                    descriptors.len() as libc::nfds_t,
                    wait_ms,
                )
            };
            if ready < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(Error::Io(format!("system DNS poll: {error}")));
            }
            for (query, descriptor) in queries.iter_mut().zip(&descriptors) {
                if descriptor.revents & libc::POLLIN != 0 {
                    // SAFETY: ready descriptor; callback uses only this query's
                    // stable Box and finishes before this call returns.
                    let result = unsafe { DNSServiceProcessResult(query.service) };
                    if result != 0 {
                        query.answers.error = Some(result);
                    }
                }
                if !query.answers.ips.is_empty() {
                    log::info!("system DNS answered via physical interface {}", query.name);
                    return Ok(query
                        .answers
                        .ips
                        .iter()
                        .map(|ip| SocketAddr::new(*ip, port))
                        .collect());
                }
            }
        }
        for query in &queries {
            errors.push(format!("{}: {:?}", query.name, query.answers.error));
        }
        Err(Error::Io(format!(
            "physical-interface DNS timed out: {}",
            errors.join(", ")
        )))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn physical_interfaces_exclude_tunnels_and_peer_links() {
            for name in ["en0", "en12", "pdp_ip0"] {
                assert!(physical_name(name));
            }
            for name in [
                "utun0", "tun0", "bridge0", "lo0", "awdl0", "llw0", "en", "en0.1",
            ] {
                assert!(!physical_name(name));
            }
        }
        #[tokio::test]
        #[ignore = "live interface-scoped system DNS"]
        async fn live_physical_dns() {
            let addresses = super::super::resolve("muster.linsheng.dev", 9443)
                .await
                .unwrap();
            assert!(!addresses.is_empty());
            eprintln!("physical DNS addresses: {addresses:?}");
        }
    }
}
