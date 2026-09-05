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

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

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
    // Folders somebody allowed for this agent on purpose, on top of its own.
    // The wall used to be absolute: an agent set never to ask could not write
    // outside its folder by any means, and somebody who wanted a file on an
    // external disk every five minutes had no way to say so.
    for place in also_allowed(home) {
        allowed.push(format!("  (subpath {:?})", place.display().to_string()));
    }
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

/// Folders allowed on top of each errand's own, by the errand's folder.
///
/// Kept in the process rather than in a file, and deliberately so: the one
/// place a walled agent can write is its own folder, so a list kept there is a
/// list the agent could add to. This one it cannot reach.
fn registry() -> &'static Mutex<HashMap<PathBuf, Vec<PathBuf>>> {
    static ALSO: OnceLock<Mutex<HashMap<PathBuf, Vec<PathBuf>>>> = OnceLock::new();
    ALSO.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Let an errand write inside these folders as well as its own.
///
/// Replaces what was allowed before, so taking a folder back is the same call
/// with a shorter list. A relative path is not a folder anybody meant, and is
/// dropped rather than turned into one under the working directory.
pub fn also_allow(home: &Path, folders: Vec<PathBuf>) {
    let folders: Vec<PathBuf> = folders
        .into_iter()
        .filter(|f| f.is_absolute())
        .map(|f| f.canonicalize().unwrap_or(f))
        .collect();
    let mut all = registry().lock().unwrap_or_else(|e| e.into_inner());
    match folders.is_empty() {
        true => {
            all.remove(home);
        }
        false => {
            all.insert(home.to_path_buf(), folders);
        }
    }
}

/// The folders this errand may write in beyond its own.
pub fn also_allowed(home: &Path) -> Vec<PathBuf> {
    registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(home)
        .cloned()
        .unwrap_or_default()
}

/// Whether what a command printed is the wall refusing it.
///
/// The system's words for the wall are the same as its words for a permission
/// it has not been granted, which is exactly the confusion this module exists
/// to end: a model that reads "Operation not permitted" on an external disk
/// sends somebody to System Settings to grant access the app already has.
pub fn looks_like_the_wall(said: &str) -> bool {
    said.to_ascii_lowercase()
        .contains("operation not permitted")
}

/// What a model has to be told about the wall before it runs into it.
///
/// Told as well as enforced, because a wall that is only enforced produces a
/// bare "Operation not permitted", and every model that reads that invents the
/// same wrong reason: macOS, and a permission the person should go and grant.
/// The person then grants nothing, because they already had it, and the errand
/// is stuck on a door that was never the problem.
pub fn what_the_wall_means(home: &Path) -> String {
    let also = also_allowed(home);
    let more = match also.is_empty() {
        true => String::new(),
        false => format!(
            " It has also been allowed to write inside: {}.",
            also.iter()
                .map(|f| f.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    format!(
        "WHERE YOU CAN WRITE\n\n\
         You run inside Errand's own wall. You can write only inside your working \
         directory, {}, and the usual temporary places.{more} Everywhere else on this \
         Mac is read-only to you, whatever permissions the app has: a write there fails \
         with \"Operation not permitted\", and that is the wall, never a macOS setting. \
         Full Disk Access does not change it, so never send the person to System \
         Settings for it. If an errand needs a file somewhere else, say so plainly: the \
         person can allow that folder for you under Allowed, choosing \"a folder\", and \
         then it works. Until then, do the work inside your own folder.",
        home.display()
    )
}

/// Whether there is anything here to build a wall with.
///
/// False on anything that is not a Mac, and on a Mac where Apple has finally
/// removed the thing they have been calling deprecated for a decade.
pub fn possible() -> bool {
    Path::new(THE_SANDBOX).exists()
}

/// Whether a running process is inside a wall.
///
/// Asked of the kernel, which is the point: the wall goes with a process
/// through every fork and exec and there is no taking it off, so this is a
/// fact about the process and not a claim it makes. The front door reads it to
/// tell a walled agent's shell from the person at a terminal, and the parent
/// chain alone would not do: a `&` in a shell that then exits leaves a child
/// whose parent is launchd, and launchd is everybody's.
///
/// `sandbox_check` is deprecated the way `sandbox-exec` is, and is the same
/// age; the day one goes the other goes with it, and `None` here is that day,
/// or a process that is already gone.
pub fn holds(pid: u32) -> Option<bool> {
    extern "C" {
        // Variadic, because with an operation named it takes a filter after
        // the type. Asked with no operation at all it answers whether the
        // process is sandboxed in any way, which is the only question here.
        fn sandbox_check(
            pid: libc::pid_t,
            operation: *const libc::c_char,
            filter: libc::c_int,
            ...
        ) -> libc::c_int;
    }
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return None;
    };
    // SAFETY: a null operation is the documented way to ask whether the
    // process is in a sandbox at all, and no variadic argument is read for it.
    match unsafe { sandbox_check(pid, std::ptr::null(), 0) } {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
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
        "{} could not be written to, and that is Errand's own wall, not a macOS \
         permission. This agent runs without being asked about anything, so it is \
         walled in instead: it can write inside {} and the usual temporary places, \
         and nowhere else, whatever access the app has been granted. Full Disk Access \
         does not change it. Nothing is wrong with the folder itself. To let it write \
         there, allow that folder for this agent under Allowed, choosing \"a folder\"; \
         otherwise work inside the agent's own folder.",
        where_to.display(),
        home.display()
    )
}

/// The same, for a command that failed somewhere it did not name.
///
/// A shell command says only that something was not permitted, not where, so
/// the sentence has to work without a path.
pub fn the_wall_refused(home: &Path) -> String {
    format!(
        "That \"Operation not permitted\" is Errand's own wall, not a macOS permission: \
         this agent runs without being asked, so it can write only inside {} and the \
         usual temporary places, whatever access the app has been granted. Full Disk \
         Access does not change it. To write somewhere else, the folder has to be \
         allowed for this agent under Allowed, choosing \"a folder\".",
        home.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_process_inside_the_wall_is_known_to_be_and_one_outside_is_not() {
        // The front door tells a walled agent's shell from the owner's
        // terminal by asking the kernel, so what the kernel answers is pinned
        // here against a real wall. The walled child says so on its own
        // stdout before it is looked at, because `sandbox-exec` takes a moment
        // to put the wall up before it runs the program.
        if !possible() {
            return;
        }
        let mut walled = std::process::Command::new(THE_SANDBOX)
            .arg("-p")
            .arg("(version 1)(allow default)")
            .arg("/bin/sh")
            .arg("-c")
            .arg("echo up; sleep 30")
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("sandbox-exec starts");
        let mut said = String::new();
        std::io::Read::read_to_string(
            &mut std::io::Read::take(walled.stdout.take().expect("its stdout"), 3),
            &mut said,
        )
        .expect("it spoke");
        assert_eq!(said, "up\n");
        assert_eq!(holds(walled.id()), Some(true), "the wall was not seen");

        // A plain child is exactly as walled as this test is, which is not at
        // all unless whoever runs the tests has put a wall around them.
        let mut plain = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .expect("a plain child");
        assert_eq!(holds(plain.id()), holds(std::process::id()));
        assert!(holds(plain.id()).is_some(), "the kernel did not answer");

        for child in [&mut walled, &mut plain] {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

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
        assert!(said.contains("not a macOS permission"), "{said}");
        assert!(
            said.contains("Full Disk Access does not change it"),
            "{said}"
        );
        assert!(said.contains("choosing \"a folder\""), "{said}");
        assert!(!said.to_lowercase().contains("sudo"), "{said}");
    }

    #[test]
    fn a_folder_allowed_for_an_errand_is_writable_and_only_for_that_errand() {
        // What actually happened: somebody asked for a file on an external
        // disk every five minutes, the wall refused it, and there was no way
        // to say the disk was fine. Allowing it has to reach the profile, and
        // has to reach only the errand it was allowed for.
        let mine = Path::new("/tmp/errand-wall-mine");
        let other = Path::new("/tmp/errand-wall-other");
        also_allow(mine, vec![PathBuf::from("/Volumes/Somewhere")]);
        let said = profile(mine);
        assert!(said.contains("(subpath \"/Volumes/Somewhere\")"), "{said}");
        assert!(
            !profile(other).contains("/Volumes/Somewhere"),
            "the folder leaked into another errand's wall"
        );
        // Taking it back is the same call with nothing in it.
        also_allow(mine, vec![]);
        assert!(!profile(mine).contains("/Volumes/Somewhere"));
    }

    #[test]
    fn a_relative_folder_is_not_allowed_because_nobody_meant_one() {
        let home = Path::new("/tmp/errand-wall-relative");
        also_allow(home, vec![PathBuf::from("Documents")]);
        assert!(also_allowed(home).is_empty());
    }

    #[test]
    fn the_walls_refusal_is_recognised_however_it_is_capitalised() {
        assert!(looks_like_the_wall("sh: x.txt: Operation not permitted"));
        assert!(looks_like_the_wall(
            "EPERM: operation not permitted, open '/x'"
        ));
        assert!(!looks_like_the_wall("No such file or directory"));
    }

    #[test]
    fn what_a_model_is_told_about_the_wall_names_the_folders_it_may_use() {
        // The whole point of telling it: a model that reads a bare refusal
        // sends somebody to System Settings for access the app already has.
        let home = Path::new("/tmp/errand-wall-told");
        also_allow(home, vec![PathBuf::from("/Volumes/Disk")]);
        let said = what_the_wall_means(home);
        assert!(said.contains("/tmp/errand-wall-told"), "{said}");
        assert!(said.contains("/Volumes/Disk"), "{said}");
        assert!(said.contains("never a macOS setting"), "{said}");
        assert!(said.contains("Full Disk Access"), "{said}");
        also_allow(home, vec![]);
    }
}
