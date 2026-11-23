//! Every agent run is sealed in an OS sandbox, whatever the agent's own flags promise. A run
//! sees the system's programs and libraries, its agent's executable, an empty home and an
//! empty working directory, and the network. It never sees the user's files: not the world
//! database, the run logs, the user's own agent settings and history, or a repository the
//! world happens to sit in. Linux uses `bwrap` (bubblewrap), macOS `sandbox-exec`.
//!
//! Not yet closed, and harmless only while agents have no tools of their own: the network is
//! the host's (local services and the LAN included), and on Linux the agent executable's host
//! path shows in `/proc/self/mountinfo`.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use tokio::process::Command;

/// Host variables a run keeps: how to reach the network, nothing about the user.
const PASS_ENV: [&str; 8] = [
    "HTTPS_PROXY",
    "HTTP_PROXY",
    "NO_PROXY",
    "https_proxy",
    "http_proxy",
    "no_proxy",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
];

#[derive(Debug, thiserror::Error)]
#[error(
    "agent runs cannot be sandboxed: {0}. On Linux install `bubblewrap`; on Ubuntu 24.04 and \
     later also allow it unprivileged user namespaces (see the README)"
)]
pub struct SandboxError(String);

/// A command that runs inside a fresh sandbox. Keep it until the run ends: on macOS it owns
/// the run's directories, which go with it.
pub(crate) struct Sandboxed {
    pub(crate) cmd: Command,
    _dirs: Option<tempfile::TempDir>,
}

/// Sandbox `program` with only `env` and a minimal base environment. A bare name is looked up
/// on the host's `PATH`.
pub(crate) fn command(program: &Path, env: &[(&str, &str)]) -> io::Result<Sandboxed> {
    let (cmd, home, dirs) = platform::command(&resolve(program)?)?;
    let mut cmd = Command::from(cmd);
    let home = home.to_string_lossy().into_owned();
    cmd.env_clear()
        .envs(std::env::vars_os().filter(|(k, _)| PASS_ENV.iter().any(|p| k == *p)))
        .envs([
            ("PATH", "/usr/bin:/bin"),
            ("HOME", &home),
            ("TMPDIR", &format!("{home}/tmp")),
            ("USER", "agent"),
            ("LANG", "C.UTF-8"),
        ])
        .envs(env.iter().copied());
    Ok(Sandboxed { cmd, _dirs: dirs })
}

/// Whether this machine can sandbox agent runs. Agents are never started unsandboxed.
pub fn check() -> Result<(), SandboxError> {
    let fail = |e: &dyn std::fmt::Display| SandboxError(e.to_string());
    let (mut cmd, _, _dirs) =
        platform::command(Path::new("/usr/bin/true")).map_err(|e| fail(&e))?;
    let out = cmd.output().map_err(|e| fail(&e))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(fail(&String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// The real file behind `program`, so a symlink in the user's `PATH` cannot point the mount
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
    use super::*;

    /// Read-only system paths a program may need: libraries, CA certificates, DNS.
    const SYSTEM: [&str; 6] = [
        "/usr",
        "/etc/resolv.conf",
        "/etc/ssl",
        "/etc/ca-certificates",
        "/etc/pki",
        "/etc/alternatives",
    ];

    /// Home, working directory and `/tmp` are size-capped memory filesystems that exist only
    /// inside the sandbox, so no host path names them and nothing is left behind.
    pub(super) fn command(
        program: &Path,
    ) -> io::Result<(std::process::Command, PathBuf, Option<tempfile::TempDir>)> {
        let mut a: Vec<OsString> = [
            "--unshare-all",
            "--share-net",
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
        let inside = if program.starts_with("/usr") {
            program.to_owned()
        } else {
            let name = program
                .file_name()
                .expect("a canonical file path has a name");
            let inside = Path::new("/opt/agent").join(name);
            a.extend(["--ro-bind".into(), program.into(), inside.clone().into()]);
            inside
        };
        a.extend(["--chdir".into(), "/work".into(), "--".into(), inside.into()]);
        let mut cmd = std::process::Command::new("bwrap");
        cmd.args(a);
        Ok((cmd, "/home/agent".into(), None))
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;

    /// Deny everything, then allow the system, the agent's executable and the run's own
    /// directories. The user's home is never readable, and of the system services only those
    /// for DNS, certificates and logging are reachable: not the keychain, user defaults or
    /// the directory service, which would give away the login or the user's name.
    const PROFILE: &str = r#"(version 1)
(deny default)
(allow process-exec process-fork signal sysctl-read ipc-posix-shm iokit-open network*)
(deny sysctl-read (sysctl-name "kern.hostname"))
(allow mach-lookup (global-name "com.apple.dnssd.service" "com.apple.trustd"
  "com.apple.trustd.agent" "com.apple.SystemConfiguration.configd"
  "com.apple.SystemConfiguration.DNSConfiguration" "com.apple.logd"
  "com.apple.system.logger" "com.apple.system.notification_center"))
(allow file-read-metadata)
(allow file-read* (literal "/") (subpath "/usr") (subpath "/bin") (subpath "/System")
  (subpath "/Library") (subpath "/private/etc") (subpath "/private/var/db/timezone")
  (subpath "/dev") (literal (param "PROGRAM")) (subpath (param "HOME")) (subpath (param "WORK")))
(allow file-write* (subpath (param "HOME")) (subpath (param "WORK")) (subpath "/dev"))
"#;

    /// macOS cannot remap paths, so the run's directories live in the system temporary
    /// directory, whose path names no user.
    pub(super) fn command(
        program: &Path,
    ) -> io::Result<(std::process::Command, PathBuf, Option<tempfile::TempDir>)> {
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
        cmd.arg("-p")
            .arg(PROFILE)
            .args(param("PROGRAM", program))
            .args(param("HOME", &home))
            .args(param("WORK", &work))
            .arg(program)
            .current_dir(&work);
        Ok((cmd, home, Some(dirs)))
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    async fn run(script: &str) -> String {
        let host = tempfile::tempdir().unwrap();
        let secret = host.path().join("world.db");
        std::fs::write(&secret, "the hidden state").unwrap();
        let agent = host.path().join("agent");
        std::fs::write(&agent, format!("#!/bin/sh\n{script}\ntrue\n")).unwrap();
        std::fs::set_permissions(&agent, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut run = command(&agent, &[("SEAT_THING", "given")]).unwrap();
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
    async fn no_host_path_but_the_agents_shows_in_the_mount_table() {
        let out = run("cat /proc/self/mountinfo").await;
        let user = std::env::var("USER").unwrap_or_default();
        for line in out.lines() {
            let f: Vec<&str> = line.split(' ').collect();
            let (root, at) = (f[3], f[4]);
            if ["/home/agent", "/work", "/tmp"].contains(&at) {
                assert_eq!(root, "/", "{line}");
            }
            if !at.starts_with("/opt/agent") && user.len() > 1 {
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
        assert!(!out.contains("CARGO"), "{out}");
    }
}
