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
        // The sandbox profile matches real paths: `/var` is a link to `/private/var`.
        std::env::temp_dir().canonicalize()?.join("empire-agents")
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

/// How long a copy stays unused before it is dropped: another run, or another Empire build,
/// may be about to mount an older version.
const UNUSED: Duration = Duration::from_secs(10 * 60);

/// A directory in the cache holding a copy of `program` and of each of its `helpers`, which
/// must sit beside it. It is keyed by `slot` and every file's size and time, so a new version
/// is copied again and an old one dropped once unused.
pub(crate) fn stage(slot: &str, program: &Path, helpers: &[&str]) -> io::Result<PathBuf> {
    let dir = program
        .parent()
        .ok_or_else(|| io::Error::other("an agent program needs a directory"))?;
    let name = program
        .file_name()
        .ok_or_else(|| io::Error::other("an agent program needs a file name"))?;
    let mut files = vec![program.to_owned()];
    files.extend(helpers.iter().map(|h| dir.join(h)));
    let mut key = slot.to_owned();
    for f in &files {
        let m = std::fs::metadata(f)
            .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", f.display())))?;
        key.push_str(&format!(
            "-{}-{}",
            m.len(),
            m.mtime() * 1_000_000_000 + m.mtime_nsec()
        ));
    }
    let root = root()?;
    let dest = root.join(&key);
    for _ in 0..2 {
        if !dest.exists() {
            let tmp = tempfile::Builder::new()
                .prefix(".stage-")
                .tempdir_in(&root)?;
            for f in &files {
                std::fs::copy(f, tmp.path().join(f.file_name().unwrap_or(name)))?;
            }
            match std::fs::rename(tmp.path(), &dest) {
                Ok(()) => std::mem::forget(tmp),
                Err(_) if dest.exists() => {}
                Err(e) => return Err(e),
            }
        }
        // Another process may prune it in between; then it is copied again.
        match std::fs::File::open(&dest) {
            Ok(d) => {
                d.set_modified(SystemTime::now())?;
                break;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        }
    }
    for e in std::fs::read_dir(&root)?.flatten() {
        let n = e.file_name();
        let n = n.to_string_lossy();
        let idle = e
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|t| t.elapsed().is_ok_and(|d| d > UNUSED));
        let ours = n.starts_with(&format!("{slot}-")) || n.starts_with(".stage-");
        if ours && n != key && idle {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
    Ok(dest)
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
        std::fs::write(src.path().join("unrelated"), "u").unwrap();
        let slot = format!("test-{}", std::process::id());
        let a = stage(&slot, &p, &["helper"]).unwrap();
        assert_eq!(stage(&slot, &p, &["helper"]).unwrap(), a);
        let mut copied: Vec<_> = std::fs::read_dir(&a)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        copied.sort();
        assert_eq!(copied, ["agent", "helper"]);
        let home = std::env::var("HOME").unwrap();
        assert!(!a.starts_with(&home), "{}", a.display());
        assert!(stage(&slot, &p, &["missing"]).is_err());
        std::fs::write(&p, "v2, longer").unwrap();
        let b = stage(&slot, &p, &[]).unwrap();
        assert_ne!(a, b);
        assert_eq!(
            std::fs::read_to_string(b.join("agent")).unwrap(),
            "v2, longer"
        );
        assert!(a.exists(), "a recently used version is kept");
        let long_ago = SystemTime::now() - 2 * UNUSED;
        std::fs::File::open(&a)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();
        stage(&slot, &p, &[]).unwrap();
        assert!(!a.exists(), "an unused version is dropped");
        std::fs::remove_dir_all(b).unwrap();
    }
}
