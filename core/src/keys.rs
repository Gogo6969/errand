//! Somebody else's API keys.
//!
//! One rule shapes all of this: a key goes in and never comes back out. It is
//! written when it is typed, read by the one thing that makes the request, and
//! there is no command, no event and no log line anywhere that returns one. The
//! window is told whether a key exists, never what it is.
//!
//! Where it is kept is decided in `put` and `get` below and nowhere else, so
//! that changing the answer is changing two functions.

use std::path::PathBuf;

use anyhow::{Context, Result};

/// Where keys live, one file each.
///
/// A directory of its own rather than the store, because the store is the file
/// somebody copies when something is wrong with it and hands to somebody else.
/// A key that travels inside a database nobody thought of as secret is how keys
/// end up somewhere they were never meant to be.
fn where_keys_live() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join("Library/Application Support/Errand/keys"))
}

/// The file for one backend.
///
/// Named after the backend's id, which this app makes, so nothing anybody typed
/// becomes a filename. A label with a slash in it would otherwise decide where
/// the file goes.
fn file_for(id: &str) -> Option<PathBuf> {
    let safe: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    match safe.is_empty() {
        true => None,
        false => Some(where_keys_live()?.join(safe)),
    }
}

/// Keep a key.
pub fn remember(id: &str, key: &str) -> Result<()> {
    let at = file_for(id).context("there is nowhere to keep keys")?;
    let dir = at.parent().context("no directory")?;
    std::fs::create_dir_all(dir).with_context(|| format!("making {}", dir.display()))?;
    permit(dir, 0o700);

    // Written, then narrowed, and in that order for the reason the sockets are:
    // a umask belongs to the whole process, so setting one around this would
    // hand it to every other thread creating a file at the same moment. The
    // window where the file exists at the default mode is real and sits inside
    // a directory that is already nobody else's business.
    std::fs::write(&at, key.as_bytes()).with_context(|| format!("writing {}", at.display()))?;
    permit(&at, 0o600);
    Ok(())
}

/// Look one up, for the one thing that makes the request.
///
/// Returns nothing rather than an error when there is none, because "this
/// backend has no key" is an ordinary state and not a fault.
pub fn look_up(id: &str) -> Option<String> {
    let at = file_for(id)?;
    let said = std::fs::read_to_string(at).ok()?;
    let said = said.trim().to_string();
    match said.is_empty() {
        true => None,
        false => Some(said),
    }
}

/// Throw one away.
///
/// Called when a backend is forgotten. A key left behind for something nobody
/// can see any more is a secret somebody does not know they still have.
pub fn forget(id: &str) -> Result<()> {
    let Some(at) = file_for(id) else {
        return Ok(());
    };
    match std::fs::remove_file(&at) {
        Ok(()) => Ok(()),
        // Already gone is the outcome that was wanted.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("removing {}", at.display())),
    }
}

/// What to tell somebody about where their key goes, before they type one.
///
/// Said out loud in the window rather than left for them to wonder about. A
/// person handing an app a paid API key is entitled to know where it is being
/// put, and an app that does not say is one that has decided not to be asked.
pub const WHERE_THEY_GO: &str =
    "Kept in a file only you can read, in Errand's own folder, one per provider. \
     It is sent to that provider and to nowhere else, and nothing here will show \
     it to you again.";

fn permit(what: &std::path::Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(what, std::fs::Permissions::from_mode(mode));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_can_be_kept_and_read_back_and_thrown_away() {
        let id = format!("test-{}", std::process::id());
        assert_eq!(look_up(&id), None, "something was there already");

        remember(&id, "sk-not-a-real-key").expect("kept");
        assert_eq!(look_up(&id).as_deref(), Some("sk-not-a-real-key"));

        forget(&id).expect("forgotten");
        assert_eq!(look_up(&id), None);
        // Twice is not a mistake worth stopping over.
        forget(&id).expect("forgotten again");
    }

    #[test]
    fn a_kept_key_is_readable_by_nobody_else() {
        use std::os::unix::fs::PermissionsExt;
        let id = format!("modes-{}", std::process::id());
        remember(&id, "sk-not-a-real-key").expect("kept");

        let at = file_for(&id).expect("a path");
        let mode = std::fs::metadata(&at).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the key file is readable by somebody else");

        let dir = std::fs::metadata(at.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(dir, 0o700, "the folder keys are in is not private");

        forget(&id).ok();
    }

    #[test]
    fn nothing_anybody_types_decides_where_a_file_goes() {
        // The id is made here, not typed, so this should never come up. It is
        // checked anyway because the cost of being wrong about that is writing
        // a file wherever somebody's label pointed.
        let at = file_for("../../../etc/passwd").expect("a path");
        assert!(at.ends_with("etcpasswd"), "{}", at.display());
        assert!(at.starts_with(where_keys_live().unwrap()));

        assert!(file_for("").is_none());
        assert!(file_for("../..").is_none());
    }

    #[test]
    fn what_somebody_is_told_about_their_key_says_where_it_goes_and_where_it_does_not() {
        // Somebody handing an app a paid API key is entitled to know both.
        assert!(WHERE_THEY_GO.contains("only you can read"));
        assert!(WHERE_THEY_GO.contains("nowhere else"));
    }
}
