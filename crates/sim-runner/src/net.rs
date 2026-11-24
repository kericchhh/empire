//! A run's only way out. The agent reaches its provider through an HTTPS proxy that admits
//! only the provider's hosts, and the world's MCP server and any local model server through
//! tunnels to exactly those addresses. Everything else is unreachable.
//!
//! On Linux the run has no network at all, so a bridge inside the sandbox (Empire's own
//! executable, see [`bridge_if_asked`]) listens on the agent's addresses and relays each
//! connection over a Unix socket to the gate here. The same sockets carry a file into the
//! run's home and back out, so the run needs no writable host directory. On macOS the sandbox
//! profile admits only the gate's and the tunnels' loopback ports.

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
#[derive(Default)]
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

/// A file carried into a run, and what the run left in its place.
#[cfg(target_os = "linux")]
#[derive(Default)]
pub(crate) struct Carry {
    pub(crate) give: Vec<u8>,
    pub(crate) back: std::sync::Mutex<Option<Vec<u8>>>,
}

/// What the gate does with a connection.
#[derive(Clone)]
pub(crate) enum Route {
    Proxy(Arc<[String]>),
    #[cfg(target_os = "linux")]
    Tunnel(SocketAddr),
    #[cfg(target_os = "linux")]
    Carry(Arc<Carry>),
}

impl Route {
    pub(crate) fn proxy(exits: &Exits) -> Self {
        Self::Proxy(exits.hosts.iter().cloned().collect())
    }
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
                        #[cfg(target_os = "linux")]
                        Route::Carry(c) => carry(s, &c).await,
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
            tracing::info!(to = t.unwrap_or(""), "run's connection refused");
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

/// `<` asks for the file; `>` hands back what the run left, which the gate acknowledges once
/// kept, so the run cannot end before it is.
#[cfg(target_os = "linux")]
async fn carry(mut s: impl AsyncRead + AsyncWrite + Unpin, c: &Carry) {
    let mut op = [0];
    if s.read_exact(&mut op).await.is_err() {
        return;
    }
    match &op {
        b"<" => {
            let _ = s.write_all(&c.give).await;
        }
        b">" => {
            let mut back = Vec::new();
            let limit = crate::fs::CARRY_LIMIT;
            if (&mut s)
                .take(limit + 1)
                .read_to_end(&mut back)
                .await
                .is_ok()
                && back.len() as u64 <= limit
            {
                *c.back.lock().expect("never poisoned") = Some(back);
                let _ = s.write_all(b"k").await;
            }
        }
        _ => {}
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

/// `<listen>=<socket>... [@<file>=<socket>] -- <program> <args>...`: relay each listen
/// address to its socket, fetch the file, run the program, then hand the file back.
fn bridge(args: Vec<OsString>) -> io::Result<i32> {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    let split = args
        .iter()
        .position(|a| a == "--")
        .ok_or_else(|| io::Error::other("no `--` before the program"))?;
    let (program, rest) = args[split + 1..]
        .split_first()
        .ok_or_else(|| io::Error::other("no program after `--`"))?;
    let mut carried = None;
    for spec in &args[..split] {
        let spec = spec.to_string_lossy();
        let (from, sock) = spec
            .split_once('=')
            .ok_or_else(|| io::Error::other(format!("bad route `{spec}`")))?;
        if let Some(file) = from.strip_prefix('@') {
            carried = Some((std::path::PathBuf::from(file), sock.to_owned()));
            continue;
        }
        let listener = std::net::TcpListener::bind(from)?;
        let sock = sock.to_owned();
        std::thread::spawn(move || {
            for conn in listener.incoming().flatten() {
                if let Ok(up) = UnixStream::connect(&sock) {
                    relay(conn, up);
                }
            }
        });
    }
    if let Some((file, sock)) = &carried {
        let mut s = UnixStream::connect(sock)?;
        s.write_all(b"<")?;
        let mut given = Vec::new();
        s.read_to_end(&mut given)?;
        if let Some(dir) = file.parent() {
            crate::fs::private_dir(dir)?;
        }
        crate::fs::private_file(file, false)?.write_all(&given)?;
    }
    #[expect(
        clippy::disallowed_methods,
        reason = "the bridge already runs inside the sandbox"
    )]
    let status = std::process::Command::new(program).args(rest).status()?;
    if let Some((file, sock)) = &carried
        && let Some(back) = crate::fs::read_carried(file)
    {
        let mut s = UnixStream::connect(sock)?;
        s.write_all(b">")?;
        s.write_all(&back)?;
        s.shutdown(std::net::Shutdown::Write)?;
        s.read_exact(&mut [0])?;
    }
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

    async fn ask(hosts: &[&str], request: &[u8]) -> String {
        let hosts: Arc<[String]> = hosts.iter().map(|h| h.to_string()).collect();
        let (mut client, server) = tokio::io::duplex(16 * 1024);
        tokio::spawn(proxy(server, hosts));
        client.write_all(request).await.unwrap();
        let mut out = vec![0; 64];
        let n = client.read(&mut out).await.unwrap_or(0);
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
        let early = tokio::spawn(async move {
            let (mut s, _) = up.accept().await.unwrap();
            s.write_all(b"hello").await.unwrap();
            let mut got = vec![0; 5];
            s.read_exact(&mut got).await.unwrap();
            got
        });
        let ok = ask(
            &[&addr],
            format!("CONNECT {addr} HTTP/1.1\r\nhost: x\r\n\r\nearly").as_bytes(),
        )
        .await;
        assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
        assert_eq!(early.await.unwrap(), b"early", "bytes after the head go on");
        let other = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let other = other.local_addr().unwrap();
        for refused in [
            "CONNECT evil.example:443 HTTP/1.1\r\n\r\n".to_owned(),
            format!("CONNECT {other} HTTP/1.1\r\n\r\n"),
            format!("GET http://{addr}/ HTTP/1.1\r\n\r\n"),
        ] {
            assert!(
                ask(&[&addr], refused.as_bytes())
                    .await
                    .starts_with("HTTP/1.1 403"),
                "{refused}"
            );
        }
        let huge = [b'a'; HEAD_LIMIT + 2048];
        assert_eq!(ask(&[&addr], &huge).await, "", "an endless head is dropped");
    }

    #[test]
    fn the_bridge_refuses_a_malformed_command_line() {
        let args = |a: &[&str]| a.iter().map(OsString::from).collect::<Vec<_>>();
        for bad in [
            args(&["127.0.0.1:0=/nowhere", "/bin/true"]),
            args(&["no-socket", "--", "/bin/true"]),
            args(&["127.0.0.1:0=/nowhere", "--"]),
        ] {
            assert!(bridge(bad.clone()).is_err(), "{bad:?}");
        }
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn a_carried_file_goes_in_and_comes_back() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("c.sock");
        let c = Arc::new(Carry {
            give: b"login v1".to_vec(),
            ..Carry::default()
        });
        let mut gate = Gate::default();
        gate.serve(
            tokio::net::UnixListener::bind(&sock).unwrap(),
            Route::Carry(c.clone()),
        );
        let file = dir.path().join("home/.agent/auth.json");
        let script = format!(
            "cat '{0}' > '{0}.seen'; printf 'login v2' > '{0}'",
            file.display()
        );
        let args: Vec<OsString> = [
            format!("@{}={}", file.display(), sock.display()),
            "--".into(),
            "/bin/sh".into(),
            "-c".into(),
            script,
        ]
        .map(OsString::from)
        .into();
        let code = tokio::task::spawn_blocking(move || bridge(args))
            .await
            .unwrap();
        assert_eq!(code.unwrap(), 0);
        let seen = std::fs::read(file.with_extension("json.seen")).unwrap();
        assert_eq!(seen, b"login v1");
        assert_eq!(c.back.lock().unwrap().as_deref(), Some(&b"login v2"[..]));
    }
}
