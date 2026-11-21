//! Operating-system socket ownership for a known, still-owned child process.
//! The caller retains the process handle and checks liveness until teardown.
use std::net::SocketAddrV4;

#[cfg(target_os = "macos")]
pub fn owned(pid: u32, addr: SocketAddrV4, tcp: bool) -> bool {
    unsafe extern "C" {
        fn muster_owns_engine_socket(pid: i32, port: u16, tcp: bool) -> bool;
    }
    addr.ip().is_loopback()
        && i32::try_from(pid)
            .is_ok_and(|pid| unsafe { muster_owns_engine_socket(pid, addr.port(), tcp) })
}

#[cfg(windows)]
pub fn owned(pid: u32, addr: SocketAddrV4, tcp: bool) -> bool {
    use windows_sys::Win32::{
        Foundation::{ERROR_INSUFFICIENT_BUFFER, NO_ERROR},
        NetworkManagement::IpHelper::*,
        Networking::WinSock::AF_INET,
    };
    if pid == 0 || !addr.ip().is_loopback() {
        return false;
    }
    let table = |ptr, size: &mut u32| unsafe {
        if tcp {
            GetExtendedTcpTable(ptr, size, 0, AF_INET.into(), TCP_TABLE_OWNER_PID_ALL, 0)
        } else {
            GetExtendedUdpTable(ptr, size, 0, AF_INET.into(), UDP_TABLE_OWNER_PID, 0)
        }
    };
    let mut size = 0;
    if table(std::ptr::null_mut(), &mut size) != ERROR_INSUFFICIENT_BUFFER {
        return false;
    }
    for _ in 0..2 {
        if !(4..=16 * 1024 * 1024).contains(&size) {
            return false;
        }
        let mut words = vec![0u32; (size as usize).div_ceil(4)];
        match table(words.as_mut_ptr().cast(), &mut size) {
            ERROR_INSUFFICIENT_BUFFER => continue,
            NO_ERROR => {}
            _ => return false,
        }
        let row_words = if tcp {
            size_of::<MIB_TCPROW_OWNER_PID>()
        } else {
            size_of::<MIB_UDPROW_OWNER_PID>()
        } / 4;
        let rows = &words[1..];
        let count = words[0] as usize;
        if count > rows.len() / row_words {
            return false;
        }
        let mut found = false;
        for row in rows.chunks_exact(row_words).take(count) {
            let (local, port, owner) = if tcp {
                (row[1], row[2], row[5])
            } else {
                (row[0], row[1], row[2])
            };
            if u16::from_be(port as u16) == addr.port()
                && (local == 0 || local.to_ne_bytes() == addr.ip().octets())
            {
                if owner != pid {
                    return false;
                }
                found = true;
            }
        }
        return found;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, TcpListener, UdpSocket};

    #[test]
    fn checks_kernel_owners_for_loopback_tcp_and_udp() {
        let tcp = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let udp = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        for (addr, is_tcp) in [
            (tcp.local_addr().unwrap(), true),
            (udp.local_addr().unwrap(), false),
        ] {
            let std::net::SocketAddr::V4(addr) = addr else {
                unreachable!()
            };
            assert!(owned(std::process::id(), addr, is_tcp));
            assert!(!owned(0, addr, is_tcp));
        }
    }
}
