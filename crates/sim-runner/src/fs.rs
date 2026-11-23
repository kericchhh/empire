//! Run logs hold seat tokens and characters' secrets, so the runner's files are private to
//! the user who runs it.

use std::fs::{DirBuilder, File, OpenOptions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;

pub(crate) fn private_dir(path: &Path) -> io::Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(path)
}

/// Opened for writing, replacing its contents or, with `append`, adding to them.
pub(crate) fn private_file(path: &Path, append: bool) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .append(append)
        .truncate(!append)
        .mode(0o600)
        .open(path)
}

/// The most a run may carry back out in a file.
pub(crate) const CARRY_LIMIT: u64 = 1 << 20;

/// The file at `path` if it is a regular file, not a link, of at most [`CARRY_LIMIT`] bytes:
/// a run decides what it leaves there.
pub(crate) fn read_carried(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    let m = std::fs::symlink_metadata(path).ok()?;
    if !m.is_file() || m.len() > CARRY_LIMIT {
        return None;
    }
    let mut out = Vec::new();
    File::open(path)
        .ok()?
        .take(CARRY_LIMIT + 1)
        .read_to_end(&mut out)
        .ok()?;
    (out.len() as u64 <= CARRY_LIMIT).then_some(out)
}
