//! Where an errand may write, enforced by the system rather than by asking.
//!
//! There are two different things here that are easy to confuse. Asking is a
//! conversation: an agent says it would like to write a file and somebody says
//! yes. A wall is not a conversation, and it is what is left when nobody is
//! going to be asked.
//!
//! A local model gets the wall always, because it has no asking of its own:
//! every tool it can reach was written here, and the judgement about which of
//! them is dangerous was made here too. Claude Code asks for itself, and asking
//! is the better mechanism while it is switched on, because it explains itself
//! and a wall does not. So the wall goes up around Claude Code exactly when the
//! asking is switched off, and nowhere else.
//!
//! The allowances below are not generous, they are load-bearing. Every one of
//! them was added because something real stopped working without it, and the
//! way it stopped working is the argument for keeping the list honest: with
//! `~/.npm` missing, `npx` failed with npm's own advice to run `sudo chown` on
//! a directory that was fine. A wall that produces misleading errors somewhere
//! else is worse than no wall, because somebody will follow the advice.

use std::path::Path;

/// Where sandboxing lives on macOS.
///
/// Deprecated by Apple for years and still the only thing of its kind that a
/// plain process can ask for without entitlements or a helper.
const THE_SANDBOX: &str = "/usr/bin/sandbox-exec";

/// Places that are not the errand's own folder and still have to be writable,
/// relative to the home directory.
///
/// These are all one kind of thing: somewhere a tool keeps its own working
/// state. None of them is anybody's documents.
const TOOLS_KEEP_THEIR_OWN_STATE_IN: &[&str] = &[
    // Claude Code's sessions, todos and settings. Without these it cannot
    // record the conversation it is having.
    ".claude",
    // Package managers, which is how most MCP servers are started. `npx`
    // without `~/.npm` fails with advice to change the ownership of a
    // directory that is not the problem.
    ".npm",
    ".cache",
    ".bun",
    ".deno",
    ".cargo",
    "Library/Caches",
];

/// The one file rather than a directory, since the rest of the home directory
/// is the thing being protected.
const AND_THIS_ONE_FILE: &str = ".claude.json";

/// Somewhere to write that is not a file anybody owns.
const SCRATCH: &[&str] = &["/private/tmp", "/private/var/folders", "/tmp"];

/// The profile: everything allowed, writing denied, and then the places writing
/// is allowed after all.
///
/// Written this way round rather than as a list of forbidden places because a
/// list of forbidden places is a list somebody has to keep complete, and the
/// day it is not complete is the day it is worth nothing.
pub fn profile(home: &Path) -> String {
    let mut allowed = vec![format!("  (subpath {:?})", home.display().to_string())];
    // The environment rather than a crate, because this is the same HOME the
    // process being walled in will use, and the two agreeing is the point.
    if let Ok(theirs) = std::env::var("HOME") {
        let theirs = Path::new(&theirs);
        for place in TOOLS_KEEP_THEIR_OWN_STATE_IN {
            allowed.push(format!(
                "  (subpath {:?})",
                theirs.join(place).display().to_string()
            ));
        }
        allowed.push(format!(
            "  (literal {:?})",
            theirs.join(AND_THIS_ONE_FILE).display().to_string()
        ));
    }
    for place in SCRATCH {
        allowed.push(format!("  (subpath {place:?})"));
    }
    // Writing to the terminal is not writing to a file, and a process that
    // cannot print is a process nobody can be told anything by.
    allowed.push("  (literal \"/dev/null\")".to_string());
    allowed.push("  (literal \"/dev/stdout\")".to_string());
    allowed.push("  (literal \"/dev/stderr\")".to_string());
    allowed.push("  (regex #\"^/dev/tty\")".to_string());

    format!(
        "(version 1)\n(allow default)\n(deny file-write*)\n(allow file-write*\n{})",
        allowed.join("\n")
    )
}

/// Whether there is anything here to build a wall with.
///
/// False on anything that is not a Mac, and on a Mac where Apple has finally
/// removed the thing they have been calling deprecated for a decade.
pub fn possible() -> bool {
    Path::new(THE_SANDBOX).exists()
}

/// A command that runs `program` inside the wall.
///
/// The caller adds the arguments, so this reads the same way as building the
/// command directly and there is no second shape to remember.
pub fn around(program: &str, home: &Path) -> tokio::process::Command {
    let mut walled = tokio::process::Command::new(THE_SANDBOX);
    walled.arg("-p").arg(profile(home)).arg(program);
    walled
}

/// A shell command run inside the wall, in the errand's own folder.
///
/// `-lc` rather than `-c`, so what somebody types behaves the way it does in
/// their own terminal, with their own PATH.
///
/// The working directory is set here rather than left to the caller, because a
/// command that runs in the wrong place is walled in around somewhere it is not
/// standing: `> notes.txt` then means a file in whatever directory the app
/// happened to start in, and the wall refuses a write that should have been
/// fine. Left to callers once, and missed once.
pub fn shell(home: &Path, command: &str) -> tokio::process::Command {
    let mut sh = around("/bin/sh", home);
    sh.arg("-lc").arg(command).current_dir(home);
    sh
}

/// What to say when something could not be written, in place of what the system
/// says.
///
/// The system says "operation not permitted", and everything above it invents
/// its own explanation for that: npm decides the directory has the wrong owner
/// and tells somebody to fix it with `sudo`. Anybody who follows that advice has
/// been sent to change the permissions on a directory that was never the
/// problem, by a wall that would not let them write there anyway.
pub fn why_it_could_not_write(where_to: &Path, home: &Path) -> String {
    format!(
        "{} could not be written to. This agent runs without being asked about \
         anything, so it is walled in instead, and it can only write inside {} and \
         the usual temporary places. Nothing is wrong with the folder itself. Either \
         work inside the agent's own folder, or set the agent back to asking first, \
         where you decide each time rather than up front.",
        where_to.display(),
        home.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_errands_own_folder_is_writable_and_the_home_directory_is_not() {
        let home = Path::new("/tmp/an-errand");
        let said = profile(home);
        assert!(said.contains("(deny file-write*)"));
        assert!(said.contains("/tmp/an-errand"), "{said}");

        // The whole point: the place the folder sits inside is not writable
        // just because the folder is.
        let theirs = std::path::PathBuf::from(std::env::var("HOME").expect("a home"));
        let bare = format!("  (subpath {:?})", theirs.display().to_string());
        assert!(
            !said.contains(&bare),
            "the whole home directory was made writable:\n{said}"
        );
    }

    #[test]
    fn the_places_tools_keep_their_own_state_are_all_allowed() {
        // Each of these was added because something real stopped working, and
        // the failure was misleading every time. Losing one silently would put
        // that back.
        let said = profile(Path::new("/tmp/an-errand"));
        let theirs = std::path::PathBuf::from(std::env::var("HOME").expect("a home"));
        for place in TOOLS_KEEP_THEIR_OWN_STATE_IN {
            let want = theirs.join(place).display().to_string();
            assert!(said.contains(&want), "{place} is not allowed:\n{said}");
        }
        assert!(said.contains(&theirs.join(AND_THIS_ONE_FILE).display().to_string()));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_folder_with_a_quote_in_its_name_cannot_end_the_profile_early() {
        // The profile is a program and a path goes into it as data. A path that
        // closed its own string would have the rest of itself read as more
        // rules, which is how a wall ends up letting something through.
        //
        // Checked by running it rather than by reading it, because what matters
        // is what the sandbox makes of the text, not what this file thinks it
        // wrote.
        if !possible() {
            return;
        }
        let awkward = std::env::temp_dir()
            .join("errand-wall-a\"folder\" (allow file-write* (subpath \"/\"))");
        std::fs::create_dir_all(&awkward).expect("a folder with a quote in it");

        let out = std::env::var("HOME").map(std::path::PathBuf::from).unwrap();
        let out = out.join("errand-wall-escape-should-not-exist.txt");
        std::fs::remove_file(&out).ok();

        let said = shell(&awkward, &format!("echo x > {:?}", out.display()))
            .output()
            .await
            .expect("the sandbox runs");

        assert!(
            !out.exists(),
            "a path escaped its quotes and the wall let a write through: {}",
            String::from_utf8_lossy(&said.stderr)
        );
        // And the profile was understood rather than rejected, or the test
        // above would pass for the wrong reason.
        let inside = shell(&awkward, "echo x > inside.txt")
            .output()
            .await
            .expect("the sandbox runs");
        assert!(
            awkward.join("inside.txt").exists(),
            "the profile was refused outright: {}",
            String::from_utf8_lossy(&inside.stderr)
        );
        std::fs::remove_dir_all(&awkward).ok();
    }

    #[test]
    fn what_it_says_about_a_refused_write_names_the_wall_rather_than_the_folder() {
        // npm's version of this sends somebody to `sudo chown` a directory that
        // was never the problem.
        let said =
            why_it_could_not_write(Path::new("/Users/someone/Desktop/x"), Path::new("/tmp/a"));
        assert!(said.contains("walled in"), "{said}");
        assert!(said.contains("Nothing is wrong with the folder"), "{said}");
        assert!(said.contains("asking first"), "{said}");
        assert!(!said.to_lowercase().contains("sudo"), "{said}");
    }
}
