//! Every agent run is sealed in an OS sandbox, whatever the agent's own flags promise. A run
//! sees the system's programs and libraries, a copy of its agent's executable, an empty home
//! and an empty working directory, and only the network exits [`net`](crate::net) allows. It
//! never sees the user's files: not the world database, the run logs, the user's own agent
//! settings and history, or a repository the world happens to sit in. Linux uses `bwrap`
//! (bubblewrap), macOS `sandbox-exec`.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::process::Command;

use crate::net::{Exits, Gate, Route};
use crate::stage::stage;

/// Host variables a run keeps. Proxies are the gate's, so the host's are not among them.
const PASS_ENV: [&str; 2] = ["SSL_CERT_FILE", "SSL_CERT_DIR"];

#[derive(Debug, thiserror::Error)]
#[error(
    "agent runs cannot be sandboxed: {0}. On Linux install `bubblewrap`; on Ubuntu 24.04 and \
     later also allow it unprivileged user namespaces (see the README)"
)]
pub struct SandboxError(String);

/// What to run in a sandbox.
pub(crate) struct Spec {
    /// The cache slot for the program's copy.
    pub(crate) slot: &'static str,
    /// A bare name is looked up on the host's `PATH`.
    pub(crate) program: PathBuf,
    /// Copy the program's whole directory, for agents that ship helpers beside it.
    pub(crate) whole_dir: bool,
    pub(crate) env: Vec<(&'static str, String)>,
    pub(crate) exits: Exits,
    /// An Empire executable that answers [`crate::bridge_if_asked`].
    #[cfg_attr(
        target_os = "macos",
        expect(dead_code, reason = "macOS runs need no bridge")
    )]
    pub(crate) bridge: PathBuf,
}

/// A command that runs inside a fresh sandbox, and the gate that is its only way out. Keep it
/// until the run ends.
pub(crate) struct Sandboxed {
    pub(crate) cmd: Command,
    _gate: Gate,
    _dirs: tempfile::TempDir,
}

/// Needs a Tokio runtime: the gate runs on it.
pub(crate) fn command(spec: &Spec) -> io::Result<Sandboxed> {
    let program = stage(spec.slot, &resolve(&spec.program)?, spec.whole_dir)?;
    let mut gate = Gate::default();
    let (cmd, home, proxy, dirs) = platform::command(spec, &program, &mut gate)?;
    let mut cmd = Command::from(cmd);
    let home = home.to_string_lossy().into_owned();
    let proxy = format!("http://{proxy}");
    cmd.env_clear()
        .envs(std::env::vars_os().filter(|(k, _)| PASS_ENV.iter().any(|p| k == *p)))
        .envs([
            ("PATH", "/usr/bin:/bin"),
            ("HOME", &home),
            ("TMPDIR", &format!("{home}/tmp")),
            ("USER", "agent"),
            ("LANG", "C.UTF-8"),
            ("HTTPS_PROXY", &proxy),
            ("HTTP_PROXY", &proxy),
            ("NO_PROXY", "127.0.0.1,localhost,::1"),
        ])
        .envs(spec.env.iter().map(|(k, v)| (k, v)));
    Ok(Sandboxed {
        cmd,
        _gate: gate,
        _dirs: dirs,
    })
}

/// Whether this machine can sandbox agent runs. Agents are never started unsandboxed.
pub fn check() -> Result<(), SandboxError> {
    let fail = |e: &dyn std::fmt::Display| SandboxError(e.to_string());
    let (mut cmd, _dirs) = platform::bare(Path::new("/usr/bin/true")).map_err(|e| fail(&e))?;
    let out = cmd.output().map_err(|e| fail(&e))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(fail(&String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// The real file behind `program`, so a symlink in the user's `PATH` cannot point the copy
/// elsewhere.
fn resolve(program: &Path) -> io::Result<PathBuf> {
    let found = if program.components().count() > 1 {
        Some(program.to_owned())
    } else {
        std::env::var_os("PATH").and_then(|p| {
            std::env::split_paths(&p)
                .map(|d| d.join(program))
                .find(|c| c.is_file())
        })
    };
    found
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("`{}` is not on PATH", program.display()),
            )
        })?
        .canonicalize()
}

fn hosts(exits: &Exits) -> Route {
    Route::Proxy(exits.hosts.iter().cloned().collect::<Arc<[String]>>())
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use crate::net::BRIDGE_ARG;

    /// Read-only system paths a program may need: libraries and CA certificates.
    const SYSTEM: [&str; 5] = [
        "/usr",
        "/etc/ssl",
        "/etc/ca-certificates",
        "/etc/pki",
        "/etc/alternatives",
    ];

    /// The proxy's address inside a run, which has a network of its own.
    const PROXY: &str = "127.0.0.1:3128";

    /// No network but loopback, and home, working directory and `/tmp` on size-capped memory
    /// filesystems that exist only inside the sandbox, so no host path names them.
    fn base() -> Vec<OsString> {
        let mut a: Vec<OsString> = [
            "--unshare-all",
            "--die-with-parent",
            "--new-session",
            "--hostname",
            "empire",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
        ]
        .map(OsString::from)
        .into();
        for d in ["/tmp", "/home/agent", "/work"] {
            a.extend([
                "--size".into(),
                "268435456".into(),
                "--tmpfs".into(),
                d.into(),
            ]);
        }
        a.extend(["--dir".into(), "/home/agent/tmp".into()]);
        for p in SYSTEM {
            a.extend(["--ro-bind-try".into(), p.into(), p.into()]);
        }
        // Merged-/usr systems link these into /usr; older ones keep real directories.
        for d in ["/bin", "/sbin", "/lib", "/lib32", "/lib64"] {
            let d = Path::new(d);
            match std::fs::read_link(d) {
                Ok(target) => a.extend(["--symlink".into(), target.into(), d.into()]),
                Err(_) if d.is_dir() => a.extend(["--ro-bind".into(), d.into(), d.into()]),
                Err(_) => {}
            }
        }
        a.extend(["--chdir".into(), "/work".into()]);
        a
    }

    pub(super) fn bare(program: &Path) -> io::Result<(std::process::Command, ())> {
        let mut cmd = std::process::Command::new("bwrap");
        cmd.args(base()).arg("--").arg(program);
        Ok((cmd, ()))
    }

    /// The gate's sockets live in the user's runtime directory, whose path names no user.
    fn sockets() -> io::Result<tempfile::TempDir> {
        let dir = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .filter(|d| d.is_dir())
            .unwrap_or_else(std::env::temp_dir);
        tempfile::Builder::new()
            .prefix("empire-run-")
            .tempdir_in(dir)
    }

    pub(super) fn command(
        spec: &Spec,
        program: &Path,
        gate: &mut Gate,
    ) -> io::Result<(std::process::Command, PathBuf, String, tempfile::TempDir)> {
        let dirs = sockets()?;
        let mut a = base();
        let name = program.file_name().expect("a staged program has a name");
        let agent = Path::new("/opt/agent").join(name);
        let (from, to) = match program.parent().filter(|_| spec.whole_dir) {
            Some(dir) => (dir, Path::new("/opt/agent")),
            None => (program, agent.as_path()),
        };
        a.extend(["--ro-bind".into(), from.into(), to.into()]);
        let bridge = stage("bridge", &resolve(&spec.bridge)?, false)?;
        a.extend([
            "--ro-bind".into(),
            bridge.into(),
            "/opt/empire/bridge".into(),
        ]);
        a.extend(["--bind".into(), dirs.path().into(), "/run/empire".into()]);
        a.extend(["--".into(), "/opt/empire/bridge".into(), BRIDGE_ARG.into()]);
        let routes = std::iter::once((PROXY.to_owned(), hosts(&spec.exits))).chain(
            spec.exits
                .tunnels
                .iter()
                .map(|t| (t.to_string(), Route::Tunnel(*t))),
        );
        for (i, (inside, route)) in routes.enumerate() {
            let sock = format!("{i}.sock");
            gate.serve(
                tokio::net::UnixListener::bind(dirs.path().join(&sock))?,
                route,
            );
            a.push(format!("{inside}=/run/empire/{sock}").into());
        }
        a.extend(["--".into(), agent.into()]);
        let mut cmd = std::process::Command::new("bwrap");
        cmd.args(a);
        Ok((cmd, "/home/agent".into(), PROXY.into(), dirs))
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;

    /// Deny everything, then allow the system, the agent's executable, the run's own
    /// directories and the gate's and tunnels' loopback ports. Of the system services only
    /// those for certificates and logging are reachable: not the keychain, user defaults or
    /// the directory service, which would give away the login or the user's name.
    fn profile(ports: &[u16]) -> String {
        let net: String = ports
            .iter()
            .map(|p| format!("(allow network-outbound (remote ip \"localhost:{p}\"))\n"))
            .collect();
        format!(
            r#"(version 1)
(deny default)
(allow process-exec process-fork signal sysctl-read ipc-posix-shm iokit-open)
(deny sysctl-read (sysctl-name "kern.hostname"))
(allow mach-lookup (global-name "com.apple.trustd" "com.apple.trustd.agent" "com.apple.logd"
  "com.apple.system.logger" "com.apple.system.notification_center"))
(allow file-read-metadata)
(allow file-read* (literal "/") (subpath "/usr") (subpath "/bin") (subpath "/System")
  (subpath "/Library") (subpath "/private/etc") (subpath "/private/var/db/timezone")
  (subpath "/dev") (subpath (param "AGENT")) (subpath (param "HOME")) (subpath (param "WORK")))
(allow file-write* (subpath (param "HOME")) (subpath (param "WORK")) (subpath "/dev"))
{net}"#
        )
    }

    /// macOS cannot remap paths, so the run's directories live in the system temporary
    /// directory, whose path names no user.
    fn sandbox(
        program: &Path,
        agent: &Path,
        ports: &[u16],
    ) -> io::Result<(std::process::Command, PathBuf, tempfile::TempDir)> {
        let dirs = tempfile::Builder::new().prefix("empire-run-").tempdir()?;
        let root = dirs.path().canonicalize()?;
        let (home, work) = (root.join("home"), root.join("work"));
        std::fs::create_dir_all(home.join("tmp"))?;
        std::fs::create_dir(&work)?;
        let param = |k: &str, v: &Path| {
            let mut s = OsString::from(format!("{k}="));
            s.push(v);
            ["-D".into(), s]
        };
        let mut cmd = std::process::Command::new("/usr/bin/sandbox-exec");
        cmd.args(param("AGENT", agent))
            .args(param("HOME", &home))
            .args(param("WORK", &work))
            .arg("-p")
            .arg(profile(ports))
            .arg(program)
            .current_dir(&work);
        Ok((cmd, home, dirs))
    }

    pub(super) fn bare(program: &Path) -> io::Result<(std::process::Command, tempfile::TempDir)> {
        let (cmd, _, dirs) = sandbox(program, program, &[])?;
        Ok((cmd, dirs))
    }

    pub(super) fn command(
        spec: &Spec,
        program: &Path,
        gate: &mut Gate,
    ) -> io::Result<(std::process::Command, PathBuf, String, tempfile::TempDir)> {
        let l = std::net::TcpListener::bind("127.0.0.1:0")?;
        l.set_nonblocking(true)?;
        let proxy = l.local_addr()?;
        gate.serve(tokio::net::TcpListener::from_std(l)?, hosts(&spec.exits));
        let mut ports = vec![proxy.port()];
        ports.extend(spec.exits.tunnels.iter().map(|t| t.port()));
        let agent = program
            .parent()
            .filter(|_| spec.whole_dir)
            .unwrap_or(program);
        let (cmd, home, dirs) = sandbox(program, agent, &ports)?;
        Ok((cmd, home, proxy.to_string(), dirs))
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    /// Run `body` sandboxed behind a stand-in bridge that only starts the program.
    async fn run(body: &str) -> String {
        let host = tempfile::tempdir().unwrap();
        let secret = host.path().join("world.db");
        std::fs::write(&secret, "the hidden state").unwrap();
        let agent = script(host.path(), "agent", &format!("{body}\ntrue"));
        let bridge = script(
            host.path(),
            "bridge",
            "while [ \"$1\" != -- ]; do shift; done; shift; exec \"$@\"",
        );
        let spec = Spec {
            slot: "test-sandbox",
            program: agent,
            whole_dir: false,
            env: vec![("SEAT_THING", "given".into())],
            exits: Exits::default(),
            bridge,
        };
        let mut run = command(&spec).unwrap();
        let out = run.cmd.arg(&secret).output().await.unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    #[test]
    fn this_machine_can_sandbox() {
        check().unwrap();
    }

    #[tokio::test]
    async fn a_run_cannot_read_the_users_files() {
        let out =
            run("cat \"$1\" 2>&1; ls -A /home; ls -A ~; echo \"$0\"; cat /etc/hostname 2>&1").await;
        assert!(!out.contains("the hidden state"), "{out}");
        assert!(out.contains("No such file"), "{out}");
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines.contains(&"agent") && lines.contains(&"tmp"), "{out}");
        assert!(lines.contains(&"/opt/agent/agent"), "{out}");
    }

    #[tokio::test]
    async fn no_host_path_naming_the_user_shows_in_the_mount_table() {
        let out = run("cat /proc/self/mountinfo").await;
        let user = std::env::var("USER").unwrap_or_default();
        for line in out.lines() {
            let f: Vec<&str> = line.split(' ').collect();
            let (root, at) = (f[3], f[4]);
            if ["/home/agent", "/work", "/tmp"].contains(&at) {
                assert_eq!(root, "/", "{line}");
            }
            if user.len() > 1 {
                assert!(!root.contains(&user), "{line}");
            }
        }
    }

    #[tokio::test]
    async fn a_run_has_an_empty_writable_workdir_and_only_the_given_environment() {
        let out = run("pwd; ls -A; echo hi > f && cat f; env").await;
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(&lines[..2], ["/work", "hi"], "{out}");
        assert!(out.contains("SEAT_THING=given") && out.contains("HOME=/home/agent"));
        assert!(out.contains("HTTPS_PROXY=http://127.0.0.1:3128"), "{out}");
        assert!(!out.contains("CARGO"), "{out}");
    }
}
