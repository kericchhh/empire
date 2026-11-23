//! A run's only way out. The agent reaches its provider through an HTTPS proxy that admits
//! only the provider's hosts, and the world's MCP server and any local model server through
//! tunnels to exactly those addresses. Everything else is unreachable.
//!
//! On Linux the run has no network at all, so a bridge inside the sandbox (Empire's own
//! executable, see [`bridge_if_asked`]) listens on the agent's addresses and relays each
//! connection over a Unix socket to the gate here. On macOS the sandbox profile admits only
//! the gate's and the tunnels' loopback ports.

use std::ffi::{OsStr, OsString};
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::task::JoinSet;

/// The first argument that makes an Empire executable act as a run's bridge.
pub(crate) const BRIDGE_ARG: &str = "__empire-bridge";

/// Where a run may connect.
#[derive(Debug, Clone, Default)]
pub(crate) struct Exits {
    /// `host:port` pairs the proxy admits.
    pub(crate) hosts: Vec<String>,
    /// Loopback addresses reachable directly, at the same address inside the run.
    pub(crate) tunnels: Vec<SocketAddr>,
}

/// The loopback address an `http://` URL points at, for a tunnel to it.
pub(crate) fn loopback(url: &str) -> io::Result<SocketAddr> {
    url.strip_prefix("http://")
        .and_then(|r| r.split('/').next())
        .and_then(|a| a.parse::<SocketAddr>().ok())
        .filter(|a| a.ip().is_loopback())
        .ok_or_else(|| {
            io::Error::other(format!("`{url}` is not an http://<loopback ip>:<port> URL"))
        })
}

#[cfg(target_os = "linux")]
pub(crate) type Listener = tokio::net::UnixListener;
#[cfg(target_os = "macos")]
pub(crate) type Listener = tokio::net::TcpListener;

/// What the gate does with a connection.
#[derive(Clone)]
pub(crate) enum Route {
    Proxy(Arc<[String]>),
    #[cfg(target_os = "linux")]
    Tunnel(SocketAddr),
}

/// The gate's listeners. Dropping it stops them.
#[derive(Default)]
pub(crate) struct Gate {
    tasks: JoinSet<()>,
}

impl Gate {
    pub(crate) fn serve(&mut self, l: Listener, route: Route) {
        self.tasks.spawn(async move {
            while let Ok((s, _)) = l.accept().await {
                let route = route.clone();
                tokio::spawn(async move {
                    match route {
                        Route::Proxy(hosts) => proxy(s, hosts).await,
                        #[cfg(target_os = "linux")]
                        Route::Tunnel(to) => tunnel(s, to).await,
                    }
                });
            }
        });
    }
}

const HEAD_LIMIT: usize = 8 * 1024;

/// Serve one proxy connection: a `CONNECT host:port` to an admitted host is joined to it,
/// anything else is refused.
async fn proxy(mut s: impl AsyncRead + AsyncWrite + Unpin, hosts: Arc<[String]>) {
    let mut buf = Vec::new();
    let end = loop {
        let mut chunk = [0; 1024];
        match s.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        if buf.len() > HEAD_LIMIT {
            return;
        }
    };
    let head = String::from_utf8_lossy(&buf[..end]);
    let mut words = head.split_whitespace();
    let target = match (words.next(), words.next()) {
        (Some("CONNECT"), Some(t)) if hosts.iter().any(|h| h == t) => t.to_owned(),
        (_, t) => {
            tracing::info!(target = t.unwrap_or(""), "run's connection refused");
            let _ = s
                .write_all(b"HTTP/1.1 403 Forbidden\r\ncontent-length: 0\r\n\r\n")
                .await;
            return;
        }
    };
    let Ok(mut up) = TcpStream::connect(&target).await else {
        let _ = s
            .write_all(b"HTTP/1.1 502 Bad Gateway\r\ncontent-length: 0\r\n\r\n")
            .await;
        return;
    };
    if s.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await
        .is_ok()
        && up.write_all(&buf[end..]).await.is_ok()
    {
        let _ = tokio::io::copy_bidirectional(&mut s, &mut up).await;
    }
}

#[cfg(target_os = "linux")]
async fn tunnel(mut s: impl AsyncRead + AsyncWrite + Unpin, to: SocketAddr) {
    if let Ok(mut up) = TcpStream::connect(to).await {
        let _ = tokio::io::copy_bidirectional(&mut s, &mut up).await;
    }
}

/// When this process was started as a run's bridge, act as one and exit with the agent's
/// status; otherwise return at once. Every executable that hosts a runner calls this first.
pub fn bridge_if_asked() {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() == Some(OsStr::new(BRIDGE_ARG)) {
        std::process::exit(bridge(args.collect()).unwrap_or_else(|e| {
            eprintln!("empire bridge: {e}");
            1
        }));
    }
}

/// `<listen>=<socket>... -- <program> <args>...`: relay each listen address to its socket,
/// then run the program.
fn bridge(args: Vec<OsString>) -> io::Result<i32> {
    use std::os::unix::net::UnixStream;
    let split = args
        .iter()
        .position(|a| a == "--")
        .ok_or_else(|| io::Error::other("no `--` before the program"))?;
    for spec in &args[..split] {
        let spec = spec.to_string_lossy();
        let (addr, sock) = spec
            .split_once('=')
            .ok_or_else(|| io::Error::other(format!("bad route `{spec}`")))?;
        let listener = std::net::TcpListener::bind(addr)?;
        let sock = sock.to_owned();
        std::thread::spawn(move || {
            for conn in listener.incoming().flatten() {
                if let Ok(up) = UnixStream::connect(&sock) {
                    relay(conn, up);
                }
            }
        });
    }
    let (program, rest) = args[split + 1..]
        .split_first()
        .ok_or_else(|| io::Error::other("no program after `--`"))?;
    let status = std::process::Command::new(program).args(rest).status()?;
    Ok(status.code().unwrap_or(1))
}

fn relay(a: std::net::TcpStream, b: std::os::unix::net::UnixStream) {
    let (Ok(a2), Ok(b2)) = (a.try_clone(), b.try_clone()) else {
        return;
    };
    let pipe = |mut from: Box<dyn io::Read + Send>, mut to: Box<dyn io::Write + Send>| {
        std::thread::spawn(move || {
            let _ = io::copy(&mut from, &mut to);
        })
    };
    pipe(Box::new(a), Box::new(b2));
    pipe(Box::new(b), Box::new(a2));
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn ask(hosts: &[&str], request: &str) -> String {
        let hosts: Arc<[String]> = hosts.iter().map(|h| h.to_string()).collect();
        let (mut client, server) = tokio::io::duplex(4096);
        tokio::spawn(proxy(server, hosts));
        client.write_all(request.as_bytes()).await.unwrap();
        let mut out = vec![0; 64];
        let n = client.read(&mut out).await.unwrap();
        String::from_utf8_lossy(&out[..n]).into_owned()
    }

    #[test]
    fn only_a_loopback_address_can_be_tunnelled() {
        assert_eq!(
            loopback("http://127.0.0.1:7878/mcp").unwrap(),
            "127.0.0.1:7878".parse().unwrap()
        );
        for bad in [
            "https://127.0.0.1:1/",
            "http://localhost:1/",
            "http://10.0.0.1:80/",
        ] {
            assert!(loopback(bad).is_err(), "{bad}");
        }
    }

    #[tokio::test]
    async fn the_proxy_joins_admitted_hosts_only() {
        let up = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = up.local_addr().unwrap().to_string();
        tokio::spawn(async move {
            let (mut s, _) = up.accept().await.unwrap();
            s.write_all(b"hello").await.unwrap();
        });
        let ok = ask(
            &[&addr],
            &format!("CONNECT {addr} HTTP/1.1\r\nhost: x\r\n\r\n"),
        )
        .await;
        assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
        for refused in [
            "CONNECT evil.example:443 HTTP/1.1\r\n\r\n".to_owned(),
            format!("GET http://{addr}/ HTTP/1.1\r\n\r\n"),
        ] {
            assert!(ask(&[&addr], &refused).await.starts_with("HTTP/1.1 403"));
        }
    }
}
