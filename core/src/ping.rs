//! Erreichbarkeits-Checks: ICMP-Ping und TCP-Port-Check.
//!
//! Desktop: System-ping (funktioniert überall). Android: `/system/bin/ping`
//! explizit (PATH im App-Prozess nicht garantiert), Fallback ICMP-Datagram-
//! Socket (SOCK_DGRAM + IPPROTO_ICMP — kein Raw-Socket/Root nötig, das Kernel
//! matcht Echo-Replies über ping_group_range).

use std::process::Command;

/// ICMP-Ping (1 Echo, 2s Timeout). Rückgabe: kurze Ausgabe-Zeile.
pub fn ping(host: &str) -> anyhow::Result<String> {
    #[cfg(target_os = "android")]
    {
        if let Ok(out) = ping_binary("/system/bin/ping", host) {
            return Ok(out);
        }
        icmp_echo(host)
    }
    #[cfg(not(target_os = "android"))]
    {
        ping_binary("ping", host)
    }
}

fn ping_binary(bin: &str, host: &str) -> anyhow::Result<String> {
    let out = Command::new(bin)
        .args(["-c", "1", "-W", "2", host])
        .output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout)
            .lines()
            .find(|l| l.starts_with("rtt") || l.starts_with("round-trip"))
            .unwrap_or("online")
            .to_string())
    } else {
        anyhow::bail!("keine antwort von {host}")
    }
}

/// TCP-Verbindungscheck: Port erreichbar? Rückgabe: Latenz.
pub fn tcp(host: &str, port: &str) -> anyhow::Result<String> {
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::{Duration, Instant};
    let port: u16 = port
        .trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("port '{port}' ist keine nummer"))?;
    let addrs: Vec<_> = (host, port)
        .to_socket_addrs()
        .map_err(|e| anyhow::anyhow!("dns {host}: {e}"))?
        .collect();
    let mut last = "unbekannter fehler".to_string();
    for addr in addrs {
        let t0 = Instant::now();
        match TcpStream::connect_timeout(&addr, Duration::from_secs(3)) {
            Ok(_) => {
                let ms = t0.elapsed().as_millis();
                return Ok(format!("{host}:{port} offen ({ms} ms)"));
            }
            Err(e) => last = e.to_string(),
        }
    }
    anyhow::bail!("tcp {host}:{port} nicht erreichbar: {last}")
}

/// Ein ICMP-Echo über einen unprivilegierten Datagram-Socket.
#[cfg(target_os = "android")]
fn icmp_echo(host: &str) -> anyhow::Result<String> {
    use std::net::ToSocketAddrs;
    use std::time::Instant;

    let addr = (host, 0u16)
        .to_socket_addrs()
        .map_err(|e| anyhow::anyhow!("dns {host}: {e}"))?
        .next()
        .ok_or_else(|| anyhow::anyhow!("dns {host}: keine adresse"))?;
    let std::net::IpAddr::V4(ip) = addr.ip() else {
        anyhow::bail!("icmp: nur ipv4");
    };

    let t0 = Instant::now();
    let buf = build_echo();
    let mut rx = [0u8; 1500];
    unsafe {
        let sock = libc::socket(libc::AF_INET, libc::SOCK_DGRAM, libc::IPPROTO_ICMP);
        if sock < 0 {
            anyhow::bail!("icmp-socket: {}", std::io::Error::last_os_error());
        }
        let close = |sock: i32| {
            let _ = libc::close(sock);
        };
        let sa = libc::sockaddr_in {
            sin_family: libc::AF_INET as libc::sa_family_t,
            sin_port: 0,
            sin_addr: libc::in_addr {
                s_addr: u32::from(ip).to_be(),
            },
            sin_zero: [0; 8],
        };
        if libc::connect(
            sock,
            &sa as *const libc::sockaddr_in as *const libc::sockaddr,
            std::mem::size_of::<libc::sockaddr_in>() as u32,
        ) != 0
        {
            close(sock);
            anyhow::bail!("icmp-connect: {}", std::io::Error::last_os_error());
        }
        let tv = libc::timeval {
            tv_sec: 2,
            tv_usec: 0,
        };
        if libc::setsockopt(
            sock,
            libc::SOL_SOCKET,
            libc::SO_RCVTIMEO,
            &tv as *const libc::timeval as *const libc::c_void,
            std::mem::size_of::<libc::timeval>() as u32,
        ) != 0
        {
            close(sock);
            anyhow::bail!("icmp-sockopt: {}", std::io::Error::last_os_error());
        }
        let sent = libc::send(sock, buf.as_ptr() as *const libc::c_void, buf.len(), 0);
        if sent < 0 {
            close(sock);
            anyhow::bail!("icmp-send: {}", std::io::Error::last_os_error());
        }
        let n = libc::recv(sock, rx.as_mut_ptr() as *mut libc::c_void, rx.len(), 0);
        close(sock);
        if n < 1 {
            anyhow::bail!("keine antwort von {host}");
        }
    }
    // Datagram-Socket: Kernel hat den IP-Header gestrippt → ICMP-Header vorn.
    if rx[0] != 0 {
        anyhow::bail!("icmp typ {}: keine echo-reply", rx[0]);
    }
    Ok(format!("online ({:?})", t0.elapsed()))
}

/// Echo-Request (typ 8) mit Checksumme als 64-Byte-Paket.
#[cfg(target_os = "android")]
fn build_echo() -> [u8; 64] {
    let mut buf = [0u8; 64];
    buf[0] = 8; // echo request
    buf[1] = 0; // code
    buf[2] = 0; // checksum (hier)
    buf[3] = 0;
    buf[4] = 0; // id (kernel überschreibt bei datagram-sockets)
    buf[5] = 0;
    buf[6] = 0; // seq
    buf[7] = 1;
    for (i, b) in buf[8..].iter_mut().enumerate() {
        *b = (i % 251) as u8;
    }
    let mut sum = 0u32;
    for pair in buf.as_chunks::<2>().0 {
        sum += u16::from_be_bytes(*pair) as u32;
    }
    let sum = !(sum as u16).wrapping_add((sum >> 16) as u16);
    buf[2..4].copy_from_slice(&sum.to_be_bytes());
    buf
}

#[cfg(all(test, target_os = "android"))]
mod tests {
    #[test]
    fn echo_checksum() {
        let buf = super::build_echo();
        assert_eq!(buf[0], 8);
        let mut sum = 0u32;
        for pair in buf.as_chunks::<2>().0 {
            sum += u16::from_be_bytes(*pair) as u32;
        }
        assert_eq!((sum as u16).wrapping_add((sum >> 16) as u16), 0xffff);
    }
}

#[cfg(all(test, not(target_os = "android")))]
mod tests {
    #[test]
    fn tcp_offen_localhost() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let out = super::tcp("127.0.0.1", &port.to_string()).unwrap();
        assert!(out.contains("offen"));
    }

    #[test]
    fn tcp_zu_gesperrt() {
        // port 1 auf loopback: nichts lauscht
        assert!(super::tcp("127.0.0.1", "1").is_err());
    }
}
