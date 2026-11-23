//! Agent executables are copied, once per version, into a cache whose path names no user, and
//! mounted into runs from there: a mount's source path is visible inside the sandbox.

use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// This process's user: `/proc/self` on Linux; on macOS the per-user temporary directory.
fn uid() -> io::Result<u32> {
    let probe = if cfg!(target_os = "macos") {
        std::env::temp_dir()
    } else {
        PathBuf::from("/proc/self")
    };
    Ok(std::fs::metadata(probe)?.uid())
}

fn root() -> io::Result<PathBuf> {
    let uid = uid()?;
    let root = if cfg!(target_os = "macos") {
        std::env::temp_dir().join("empire-agents")
    } else {
        PathBuf::from(format!("/var/tmp/empire-{uid}"))
    };
    match std::fs::DirBuilder::new().mode(0o700).create(&root) {
        Err(e) if e.kind() != io::ErrorKind::AlreadyExists => return Err(e),
        _ => {}
    }
    // In a shared temporary directory anyone can create the path first.
    let m = std::fs::symlink_metadata(&root)?;
    if !m.is_dir() || m.uid() != uid || m.permissions().mode() & 0o077 != 0 {
        return Err(io::Error::other(format!(
            "{} is not a private directory of this user",
            root.display()
        )));
    }
    Ok(root)
}

/// How long a copy stays unused before it is dropped. Older versions may still be in use by
/// another run or another Empire build meanwhile.
const UNUSED: Duration = Duration::from_secs(24 * 60 * 60);

/// A copy of `program` in the cache, keyed by `slot` and the file's size and time, so a new
/// version is copied again and an old one dropped once unused. With `whole_dir`, the program's directory
/// is copied, for agents that ship helpers beside their executable.
pub(crate) fn stage(slot: &str, program: &Path, whole_dir: bool) -> io::Result<PathBuf> {
    let m = std::fs::metadata(program)?;
    let name = program
        .file_name()
        .ok_or_else(|| io::Error::other("an agent program needs a file name"))?;
    let root = root()?;
    let key = format!(
        "{slot}-{}-{}",
        m.len(),
        m.mtime_nsec() + m.mtime() * 1_000_000_000
    );
    let dest = root.join(&key);
    if !dest.exists() {
        let tmp = tempfile::Builder::new()
            .prefix(".stage-")
            .tempdir_in(&root)?;
        let src_dir = program.parent().filter(|_| whole_dir);
        match src_dir {
            Some(dir) => {
                for e in std::fs::read_dir(dir)? {
                    let p = e?.path();
                    if p.is_file() {
                        std::fs::copy(&p, tmp.path().join(p.file_name().expect("listed")))?;
                    }
                }
            }
            None => {
                std::fs::copy(program, tmp.path().join(name))?;
            }
        }
        match std::fs::rename(tmp.path(), &dest) {
            Ok(()) => std::mem::forget(tmp),
            Err(_) if dest.exists() => {}
            Err(e) => return Err(e),
        }
    }
    std::fs::File::open(&dest)?.set_modified(SystemTime::now())?;
    for e in std::fs::read_dir(&root)?.flatten() {
        let n = e.file_name();
        let n = n.to_string_lossy();
        let idle = e
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|t| t.elapsed().is_ok_and(|d| d > UNUSED));
        if n.starts_with(&format!("{slot}-")) && n != key && idle {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
    Ok(dest.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_is_copied_once_per_version_away_from_the_users_home() {
        let src = tempfile::tempdir().unwrap();
        let p = src.path().join("agent");
        std::fs::write(&p, "v1").unwrap();
        std::fs::write(src.path().join("helper"), "h").unwrap();
        let slot = format!("test-{}", std::process::id());
        let a = stage(&slot, &p, true).unwrap();
        assert_eq!(stage(&slot, &p, true).unwrap(), a);
        assert!(a.with_file_name("helper").exists());
        let home = std::env::var("HOME").unwrap();
        assert!(!a.starts_with(&home), "{}", a.display());
        std::fs::write(&p, "v2, longer").unwrap();
        let b = stage(&slot, &p, false).unwrap();
        assert_ne!(a, b);
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "v2, longer");
        assert!(a.exists(), "a recently used version is kept");
        let long_ago = SystemTime::now() - 2 * UNUSED;
        let old = std::fs::File::open(a.parent().unwrap()).unwrap();
        old.set_modified(long_ago).unwrap();
        stage(&slot, &p, false).unwrap();
        assert!(!a.exists(), "an unused version is dropped");
        std::fs::remove_dir_all(b.parent().unwrap()).unwrap();
    }
}
