//! Empire's files on the user's machine: the worlds, and the logins seats use. The desktop
//! app and the headless server share them.
//!
//! Seat runs never see the user's own agent logins: those refresh and rotate their tokens,
//! which a sandboxed copy would break. Claude seats use a long-lived token made with
//! `claude setup-token`, which draws on the same subscription; Empire keeps it in a file only
//! the user can read, and hands it to each run through its environment. Codex has no such
//! token, so Codex seats use a Codex home of Empire's own, logged in once with `codex login`.

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
    #[error(
        "Codex is not connected to Empire: run `sim-server login codex`, or connect it in the \
         app's AI providers"
    )]
    NoCodexLogin,
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

/// The logins Empire holds: the Claude token and the logged-in Codex home, each `None` when
/// that agent is not connected.
pub fn logins() -> Result<(Option<String>, Option<PathBuf>), LocalError> {
    Ok((missing(claude_token())?, missing(codex_login())?))
}

fn missing<T>(r: Result<T, LocalError>) -> Result<Option<T>, LocalError> {
    match r {
        Err(LocalError::NoClaudeToken | LocalError::NoCodexLogin) => Ok(None),
        r => r.map(Some),
    }
}

/// Empire's own Codex home, readable only by the user.
pub fn codex_home() -> Result<PathBuf, LocalError> {
    let dir = data_dir()?.join("codex");
    private_dir(&dir).map_err(io(&dir))?;
    Ok(dir)
}

/// Empire's Codex home, once it holds a login.
pub fn codex_login() -> Result<PathBuf, LocalError> {
    let home = codex_home()?;
    match home.join("auth.json").is_file() {
        true => Ok(home),
        false => Err(LocalError::NoCodexLogin),
    }
}

/// `codex login` into Empire's Codex home. It runs outside any sandbox, as the user's own.
pub fn codex_login_command(codex: &Path) -> Result<std::process::Command, LocalError> {
    #[expect(
        clippy::disallowed_methods,
        reason = "a login is the user's own, not an agent run"
    )]
    let mut cmd = std::process::Command::new(codex);
    cmd.arg("login").env("CODEX_HOME", codex_home()?);
    Ok(cmd)
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
    // A token copied from a terminal picks up the line breaks it was wrapped at.
    let token: String = token.chars().filter(|c| !c.is_whitespace()).collect();
    if !token.starts_with("sk-ant-") || token.contains(char::is_control) {
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
        write_token(dir.path(), "  sk-ant-oat01-a\nbc \n").unwrap();
        assert_eq!(read_token(dir.path()).unwrap(), "sk-ant-oat01-abc");
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn pasted_junk_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        for bad in ["", "   ", "not a token", "sk-ant-a\u{7}b"] {
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
