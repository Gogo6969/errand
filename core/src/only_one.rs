//! One copy of this app at a time.
//!
//! Errand is installed by hand, over the top of the last one, which makes two
//! copies likelier here than in an app that updates itself: one in Downloads
//! and one in Applications, both double-clicked, both perfectly willing to run.
//!
//! They share one SQLite store, and the app already knew: the comment on the
//! startup sweep says two copies were never supported, and the sweep itself
//! deletes every MCP socket it finds, including the ones belonging to a copy
//! that is using them this second. Two thirty-second clock loops firing the
//! same routines against one store is a path to losing work rather than a
//! cosmetic problem.
//!
//! An advisory lock on a file beside the store, rather than a pid file. A pid
//! file has to be believed: after a crash it names a process that is not there,
//! or worse, a pid the system has since given to something else. A lock held by
//! an open file descriptor is released by the kernel when the process ends,
//! however it ends, so there is no stale state to reason about and nothing to
//! clean up after a crash.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

/// The lock, held for as long as this is alive.
///
/// The file handle is the lock: dropping it, or the process ending for any
/// reason, releases it. So this is kept for the life of the app and never
/// otherwise looked at.
#[derive(Debug)]
pub struct OnlyOne {
    _held: File,
}

/// Take the lock, or say who has it.
///
/// The message is for somebody who has just double-clicked a second copy and
/// needs to know why nothing happened, so it says what to do rather than what
/// went wrong.
pub fn take(here: &Path) -> Result<OnlyOne, String> {
    let at = here.join("running.lock");
    let held = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&at)
        .map_err(|why| format!("could not open {}: {why}", at.display()))?;

    // Non-blocking on purpose. Waiting would leave a second copy sitting in the
    // dock with no window and no explanation, which is a worse version of the
    // problem this is here to solve.
    let took = unsafe { libc::flock(fd_of(&held), libc::LOCK_EX | libc::LOCK_NB) };
    if took != 0 {
        return Err(
            "Errand is already running. Look for its window, or in the dock. \
             Two copies would share one set of conversations and would both run \
             your routines."
                .to_string(),
        );
    }

    // Which copy has it, for somebody looking at this from a terminal. Written
    // after the lock is held, so it is never the reason a lock is not taken.
    let mut noting = &held;
    let _ = noting.write_all(
        format!(
            "{}\n{}\n",
            std::process::id(),
            std::env::current_exe()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        )
        .as_bytes(),
    );
    Ok(OnlyOne { _held: held })
}

/// The number the system knows this file by.
fn fd_of(file: &File) -> i32 {
    use std::os::unix::io::AsRawFd;
    file.as_raw_fd()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Somewhere of this test's own, cleared first, so a run does not inherit a
    /// lock from the run before it.
    fn somewhere(named: &str) -> std::path::PathBuf {
        let at = std::env::temp_dir().join(format!("errand-only-one-{named}"));
        let _ = std::fs::remove_dir_all(&at);
        std::fs::create_dir_all(&at).expect("somewhere to put it");
        at
    }

    #[test]
    fn a_second_copy_is_turned_away_and_told_where_the_first_one_is() {
        // The failure this stops is not cosmetic: two clock loops firing the
        // same routines against one SQLite store loses work.
        let here = somewhere("two");
        let first = take(&here).expect("the first copy takes the lock");

        let refused = take(&here)
            .map(|_| ())
            .expect_err("a second copy was allowed in");
        assert!(refused.contains("already running"), "{refused}");
        // Said as what to do rather than as what went wrong, because the person
        // reading it has just double-clicked something and seen nothing happen.
        assert!(
            refused.contains("dock") || refused.contains("window"),
            "{refused}"
        );

        // And letting go gives it up, so quitting and reopening works.
        drop(first);
        take(&here).expect("the lock was not released");
    }

    #[test]
    fn the_lock_says_which_copy_is_holding_it() {
        // For somebody looking at this from a terminal, wondering which of the
        // two apps they have is the one that answered.
        let here = somewhere("which");
        let held = take(&here).expect("the lock");
        let said = std::fs::read_to_string(here.join("running.lock")).expect("readable");
        assert!(
            said.starts_with(&std::process::id().to_string()),
            "{said:?}"
        );
        drop(held);
    }

    #[test]
    fn a_lock_file_left_behind_by_a_crash_is_not_in_the_way() {
        // The whole reason this is a lock rather than a pid file. A pid file
        // has to be believed, and after a crash it names a process that is not
        // there, or a pid the system has since given to something else.
        let here = somewhere("stale");
        std::fs::write(here.join("running.lock"), "99999\n/nowhere/Errand\n")
            .expect("a file from a copy that died");
        take(&here).expect("a leftover file kept the app out");
    }
}
