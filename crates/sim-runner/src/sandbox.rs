//! Every agent run is sealed in an OS sandbox, whatever the agent's own flags promise. A run
//! sees the system's programs and libraries, a copy of its agent's executable, an empty home
//! and an empty working directory, and only the network exits [`net`](crate::net) allows. It
//! never sees the user's files: not the world database, the run logs, the user's own agent
//! settings and history, or a repository the world happens to sit in. Linux uses `bwrap`
//! (bubblewrap), macOS `sandbox-exec`.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use tokio::process::Command;

use crate::net::{Exits, Gate, Route};
use crate::stage::stage;

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
    /// Files the agent ships beside its executable and needs.
    pub(crate) helpers: &'static [&'static str],
    /// Whether the run sees the system's programs, or only the libraries its own executables
    /// link to and the CA certificates. An agent whose own tools cannot all be switched off
    /// gets none, so a command it ran would have nothing to run.
    pub(crate) programs: bool,
    pub(crate) env: Vec<(&'static str, String)>,
    pub(crate) exits: Exits,
    /// A file to place in the run's home, by its path there, and read back when it ends.
    pub(crate) carry: Option<(&'static str, Vec<u8>)>,
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
    back: platform::Back,
    _gate: Gate,
    _dirs: tempfile::TempDir,
}

impl Sandboxed {
    /// What the run left in the carried file, once it has ended.
    pub(crate) fn carried(&self) -> Option<Vec<u8>> {
        platform::take(&self.back)
    }
}

/// A platform's sandbox for one run.
struct Built {
    cmd: std::process::Command,
    /// The run's home, as the run sees it.
    home: PathBuf,
    proxy: String,
    back: platform::Back,
    dirs: tempfile::TempDir,
}

/// Needs a Tokio runtime: the gate runs on it.
pub(crate) fn command(spec: &Spec) -> io::Result<Sandboxed> {
    let program = resolve(&spec.program)?;
    let agent = stage(spec.slot, &program, spec.helpers)?;
    let name = program.file_name().expect("resolved to a file");
    let mut gate = Gate::default();
    let b = platform::command(spec, &agent, name, &mut gate)?;
    let mut cmd = Command::from(b.cmd);
    let home = b.home.to_string_lossy().into_owned();
    let proxy = format!("http://{}", b.proxy);
    cmd.env_clear()
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
        back: b.back,
        _gate: gate,
        _dirs: b.dirs,
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

#[cfg(target_os = "linux")]
mod platform {
    use std::sync::Arc;

    use super::*;
    use crate::net::{BRIDGE_ARG, Carry};

    /// Read-only system paths a run may need: programs and libraries, and CA certificates.
    const SYSTEM: [&str; 5] = [
        "/usr",
        "/etc/ssl",
        "/etc/ca-certificates",
        "/etc/pki",
        "/etc/alternatives",
    ];

    /// What a run without programs gets of them, beside the libraries it links to.
    const CERTIFICATES: [&str; 5] = [
        "/usr/share/ca-certificates",
        "/etc/ssl",
        "/etc/ca-certificates",
        "/etc/pki",
        "/etc/ld.so.cache",
    ];

    pub(super) type Back = Option<Arc<Carry>>;

    pub(super) fn take(back: &Back) -> Option<Vec<u8>> {
        back.as_ref()?.back.lock().expect("never poisoned").take()
    }

    /// No network but loopback, and home, working directory and `/tmp` on size-capped memory
    /// filesystems that exist only inside the sandbox, so no host path names them.
    fn base(system: &[&str]) -> Vec<OsString> {
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
        for p in system {
            a.extend(["--ro-bind-try".into(), p.into(), p.into()]);
        }
        // Merged-/usr systems link these into /usr; older ones keep real directories.
        for d in ["/bin", "/sbin", "/lib", "/lib32", "/lib64"] {
            let d = Path::new(d);
            match std::fs::read_link(d) {
                Ok(target) => a.extend(["--symlink".into(), target.into(), d.into()]),
                Err(_) if d.is_dir() && system.contains(&"/usr") => {
                    a.extend(["--ro-bind".into(), d.into(), d.into()])
                }
                Err(_) => {}
            }
        }
        a.extend(["--chdir".into(), "/work".into()]);
        a
    }

    /// The system paths for a run: everything, or only the certificates and the shared
    /// libraries `programs` load, by the paths `ldd` gives.
    pub(super) fn system(programs: Option<&[PathBuf]>) -> io::Result<Vec<OsString>> {
        let Some(programs) = programs else {
            return Ok(base(&SYSTEM));
        };
        let mut a = base(&CERTIFICATES);
        for p in programs {
            let out = std::process::Command::new("ldd").arg(p).output()?;
            for lib in String::from_utf8_lossy(&out.stdout)
                .split_whitespace()
                .filter(|w| w.starts_with('/'))
            {
                a.extend(["--ro-bind".into(), lib.into(), lib.into()]);
            }
        }
        Ok(a)
    }

    pub(super) fn bare(program: &Path) -> io::Result<(std::process::Command, ())> {
        let mut cmd = std::process::Command::new("bwrap");
        cmd.args(base(&SYSTEM)).arg("--").arg(program);
        Ok((cmd, ()))
    }

    /// The gate's sockets live in the user's runtime directory, whose path names no user.
    /// The run sees them read-only: it can connect, but write nothing there.
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
        agent: &Path,
        name: &std::ffi::OsStr,
        gate: &mut Gate,
    ) -> io::Result<Built> {
        let dirs = sockets()?;
        let bridge_exe = resolve(&spec.bridge)?;
        let bridge_name = bridge_exe.file_name().expect("resolved to a file");
        let bridge = stage("bridge", &bridge_exe, &[])?;
        let mut own = vec![bridge.join(bridge_name), agent.join(name)];
        own.extend(spec.helpers.iter().map(|h| agent.join(h)));
        let mut a = system((!spec.programs).then_some(&own[..]))?;
        a.extend(["--ro-bind".into(), agent.into(), "/opt/agent".into()]);
        a.extend(["--ro-bind".into(), bridge.into(), "/opt/empire".into()]);
        a.extend(["--ro-bind".into(), dirs.path().into(), "/run/empire".into()]);
        a.extend([
            "--".into(),
            Path::new("/opt/empire").join(bridge_name).into(),
            BRIDGE_ARG.into(),
        ]);
        let mut tunnels = spec.exits.tunnels.clone();
        tunnels.sort();
        tunnels.dedup();
        let port = (3128..).find(|p| tunnels.iter().all(|t| t.port() != *p));
        let proxy = format!("127.0.0.1:{}", port.expect("a free port below 65536"));
        let back = spec.carry.as_ref().map(|(_, give)| {
            Arc::new(Carry {
                give: give.clone(),
                ..Carry::default()
            })
        });
        let routes =
            std::iter::once((proxy.clone(), Route::proxy(&spec.exits)))
                .chain(tunnels.iter().map(|t| (t.to_string(), Route::Tunnel(*t))))
                .chain(spec.carry.iter().zip(&back).map(|((file, _), c)| {
                    (format!("@/home/agent/{file}"), Route::Carry(c.clone()))
                }));
        for (i, (inside, route)) in routes.enumerate() {
            let sock = format!("{i}.sock");
            gate.serve(
                tokio::net::UnixListener::bind(dirs.path().join(&sock))?,
                route,
            );
            a.push(format!("{inside}=/run/empire/{sock}").into());
        }
        a.extend(["--".into(), Path::new("/opt/agent").join(name).into()]);
        let mut cmd = std::process::Command::new("bwrap");
        cmd.args(a);
        Ok(Built {
            cmd,
            home: "/home/agent".into(),
            proxy,
            back,
            dirs,
        })
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;

    /// The carried file, in the run's home on the host.
    pub(super) type Back = Option<PathBuf>;

    pub(super) fn take(back: &Back) -> Option<Vec<u8>> {
        crate::fs::read_carried(back.as_ref()?)
    }

    /// Deny everything, then allow the system, the agent's executables, the run's own
    /// directories and the gate's and tunnels' loopback ports. Nothing under `/Users` or
    /// `/Library` is readable or even listable, and of the system services only those for
    /// certificates and logging are reachable: not the keychain, user defaults or the
    /// directory service, which would give away the login or the user's name.
    fn profile(ports: &[u16], programs: bool) -> String {
        let net: String = ports
            .iter()
            .map(|p| format!("(allow network-outbound (remote ip \"localhost:{p}\"))\n"))
            .collect();
        let exec = if programs {
            "(allow process-exec)"
        } else {
            "(allow process-exec (subpath (param \"AGENT\")))"
        };
        format!(
            r#"(version 1)
(deny default)
{exec}
(allow process-fork signal sysctl-read ipc-posix-shm iokit-open)
(deny sysctl-read (sysctl-name "kern.hostname"))
(allow mach-lookup (global-name "com.apple.trustd" "com.apple.trustd.agent" "com.apple.logd"
  "com.apple.system.logger" "com.apple.system.notification_center"))
(allow file-read-metadata (literal "/") (literal "/var") (literal "/etc") (literal "/tmp")
  (literal "/private") (literal "/private/var") (subpath "/private/var/folders"))
(allow file-read* (literal "/") (subpath "/usr") (subpath "/bin") (subpath "/System")
  (subpath "/private/etc") (subpath "/private/var/db/timezone") (subpath "/private/var/select")
  (subpath "/Library/Developer/CommandLineTools") (subpath "/dev")
  (subpath (param "AGENT")) (subpath (param "RUN")))
(allow file-write* (subpath (param "RUN")) (subpath "/dev"))
{net}"#
        )
    }

    /// macOS cannot remap paths, so the run's directories live in the system temporary
    /// directory, whose path names no user.
    fn sandbox(program: &Path, agent: &Path, ports: &[u16], programs: bool) -> io::Result<Built> {
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
            .args(param("RUN", &root))
            .arg("-p")
            .arg(profile(ports, programs))
            .arg(program)
            .current_dir(&work);
        Ok(Built {
            cmd,
            home,
            proxy: String::new(),
            back: None,
            dirs,
        })
    }

    pub(super) fn bare(program: &Path) -> io::Result<(std::process::Command, tempfile::TempDir)> {
        let b = sandbox(program, program, &[], true)?;
        Ok((b.cmd, b.dirs))
    }

    pub(super) fn command(
        spec: &Spec,
        agent: &Path,
        name: &std::ffi::OsStr,
        gate: &mut Gate,
    ) -> io::Result<Built> {
        let l = std::net::TcpListener::bind("127.0.0.1:0")?;
        l.set_nonblocking(true)?;
        let proxy = l.local_addr()?;
        gate.serve(
            tokio::net::TcpListener::from_std(l)?,
            Route::proxy(&spec.exits),
        );
        let mut ports = vec![proxy.port()];
        ports.extend(spec.exits.tunnels.iter().map(|t| t.port()));
        let mut b = sandbox(&agent.join(name), agent, &ports, spec.programs)?;
        if let Some((file, give)) = &spec.carry {
            let path = b.home.join(file);
            if let Some(dir) = path.parent() {
                crate::fs::private_dir(dir)?;
            }
            std::io::Write::write_all(&mut crate::fs::private_file(&path, false)?, give)?;
            b.back = Some(path);
        }
        b.proxy = proxy.to_string();
        Ok(b)
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

    /// Run `body` sandboxed behind a stand-in bridge that only starts the program. The agent
    /// lives in the user's home, as a user's agents do.
    async fn run(body: &str) -> String {
        let home = std::env::var_os("HOME").unwrap();
        let host = tempfile::Builder::new()
            .prefix(".empire-test-")
            .tempdir_in(home)
            .unwrap();
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
            helpers: &[],
            programs: true,
            env: vec![("SEAT_THING", "given".into())],
            exits: Exits::default(),
            carry: None,
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

    /// Executables anywhere under the library directories, which a run without programs must
    /// not be able to start.
    fn library_programs() -> Vec<PathBuf> {
        let mut found = Vec::new();
        let mut dirs: Vec<PathBuf> = ["/usr/lib", "/usr/lib64", "/usr/libexec"]
            .map(PathBuf::from)
            .into();
        while let Some(d) = dirs.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let (p, Ok(m)) = (e.path(), e.metadata()) else {
                    continue;
                };
                if m.is_dir() && dirs.len() < 2000 {
                    dirs.push(p);
                } else if m.is_file()
                    && m.permissions().mode() & 0o111 != 0
                    && !p.to_string_lossy().contains(".so")
                    && found.len() < 200
                {
                    found.push(p);
                }
            }
        }
        found
    }

    #[test]
    fn a_run_without_programs_has_its_libraries_but_nothing_else_to_run() {
        let mut a = platform::system(Some(&[PathBuf::from("/usr/bin/env")])).unwrap();
        a.extend(
            [
                "--ro-bind",
                "/usr/bin/env",
                "/opt/agent/env",
                "--",
                "/opt/agent/env",
            ]
            .map(OsString::from),
        );
        let run = |args: &[&OsString]| {
            std::process::Command::new("bwrap")
                .args(&a)
                .args(args)
                .output()
                .unwrap()
        };
        let out = run(&[]);
        assert!(
            out.status.success(),
            "a linked program finds its libraries: {} {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
        let mut tried: Vec<PathBuf> = ["/bin/sh", "/usr/bin/sh", "/usr/bin/python3"]
            .map(PathBuf::from)
            .into();
        tried.extend(library_programs());
        for p in tried {
            let out = run(&[&p.clone().into_os_string(), &"--version".into()]);
            let err = String::from_utf8_lossy(&out.stderr);
            assert!(
                err.contains("No such file"),
                "{} could run: {err}",
                p.display()
            );
        }
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
        let out = run("cat /proc/self/mountinfo; touch /run/empire/x 2>&1").await;
        let home = std::env::var("HOME").unwrap();
        assert!(out.contains("Read-only file system"), "{out}");
        for line in out.lines().filter(|l| l.split(' ').count() > 4) {
            let f: Vec<&str> = line.split(' ').collect();
            let (root, at) = (f[3], f[4]);
            if ["/home/agent", "/work", "/tmp"].contains(&at) {
                assert_eq!(root, "/", "{line}");
            }
            assert!(!root.starts_with(&home), "{line}");
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
