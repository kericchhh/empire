//! Empire's files on the user's machine: the worlds, and the Claude login seats use. The
//! desktop app and the headless server share them.
//!
//! Seat runs never see the user's own Claude Code login: it refreshes and rotates its tokens,
//! which a sandboxed copy would break. They use a long-lived token made with
//! `claude setup-token` instead, which draws on the same subscription. Empire keeps it in a
//! file only the user can read, and hands it to each run through its environment.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::fs::{private_dir, private_file};

/// The app's identifier, so the desktop app and the headless server share one directory.
const APP_ID: &str = "dev.empire.app";

#[derive(Debug, thiserror::Error)]
pub enum LocalError {
    #[error("this system has no user data directory")]
    NoDataDir,
    #[error(
        "Claude Code is not connected to Empire: run `claude setup-token`, then save the token \
         with `sim-server login` or in the app's AI providers"
    )]
    NoClaudeToken,
    #[error("that is not a token from `claude setup-token`")]
    BadToken,
    #[error("a world name may not be empty or contain `/`, `\\` or `.`")]
    BadWorldName,
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Empire's directory: on Linux `~/.local/share/dev.empire.app`, on macOS
/// `~/Library/Application Support/dev.empire.app`.
fn data_dir() -> Result<PathBuf, LocalError> {
    dirs::data_dir()
        .map(|d| d.join(APP_ID))
        .ok_or(LocalError::NoDataDir)
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> LocalError {
    let path = path.to_owned();
    move |source| LocalError::Io { path, source }
}

/// The directory of named worlds, readable only by the user since worlds hold seat tokens.
pub fn worlds_dir() -> Result<PathBuf, LocalError> {
    let dir = data_dir()?.join("worlds");
    private_dir(&dir).map_err(io(&dir))?;
    Ok(dir)
}

/// The database of the world called `name`. Its run logs go beside it as `<name>.runs`.
pub fn world_db(name: &str) -> Result<PathBuf, LocalError> {
    if name.is_empty() || name.contains(['/', '\\', '.']) {
        return Err(LocalError::BadWorldName);
    }
    Ok(worlds_dir()?.join(format!("{name}.db")))
}

/// The saved Claude token.
pub fn claude_token() -> Result<String, LocalError> {
    read_token(&data_dir()?)
}

/// Save the token `claude setup-token` printed, readable only by the user.
pub fn save_claude_token(token: &str) -> Result<(), LocalError> {
    write_token(&data_dir()?, token)
}

fn token_file(dir: &Path) -> PathBuf {
    dir.join("claude-token")
}

fn read_token(dir: &Path) -> Result<String, LocalError> {
    let path = token_file(dir);
    match std::fs::read_to_string(&path) {
        Ok(t) if !t.trim().is_empty() => Ok(t.trim().to_owned()),
        Ok(_) => Err(LocalError::NoClaudeToken),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(LocalError::NoClaudeToken),
        Err(e) => Err(io(&path)(e)),
    }
}

fn write_token(dir: &Path, token: &str) -> Result<(), LocalError> {
    let token = token.trim();
    if token.is_empty() || token.contains(|c: char| c.is_whitespace() || c.is_control()) {
        return Err(LocalError::BadToken);
    }
    let path = token_file(dir);
    private_dir(dir).map_err(io(dir))?;
    let mut f = private_file(&path, false).map_err(io(&path))?;
    f.set_permissions(std::fs::Permissions::from_mode(0o600))
        .and_then(|()| f.write_all(token.as_bytes()))
        .map_err(io(&path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_token_is_private_and_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            read_token(dir.path()),
            Err(LocalError::NoClaudeToken)
        ));
        let file = token_file(dir.path());
        std::fs::write(&file, "old").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_token(dir.path(), "  sk-ant-oat01-abc\n").unwrap();
        assert_eq!(read_token(dir.path()).unwrap(), "sk-ant-oat01-abc");
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn pasted_junk_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        for bad in ["", "   ", "two words", "a\u{7}b"] {
            assert!(matches!(
                write_token(dir.path(), bad),
                Err(LocalError::BadToken)
            ));
        }
    }

    #[test]
    fn a_world_name_cannot_leave_the_worlds_directory() {
        for bad in ["", "..", "a/b", "a\\b", "rome.db"] {
            assert!(
                matches!(world_db(bad), Err(LocalError::BadWorldName)),
                "{bad}"
            );
        }
    }
}
