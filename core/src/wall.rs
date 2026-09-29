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

/// Claude Code's sessions, todos and settings. Without these it cannot record
/// the conversation it is having.
///
/// Apart from the list above, and only for Claude Code, because a command a
/// model runs has no use for them and every reason not to have them: the
/// settings in here hold the hooks Claude Code runs, unwalled, in the person's
/// own sessions. A command that could write there could leave something behind
/// that runs outside the wall the next time they open Claude Code themselves.
const CLAUDE_CODE_KEEPS_ITS_STATE_IN: &str = ".claude";

/// The one file rather than a directory, since the rest of the home directory
/// is the thing being protected. Claude Code's, like the folder above.
const AND_THIS_ONE_FILE: &str = ".claude.json";

/// What is being walled in, because the two need different room.
#[derive(Debug, Clone, Copy)]
pub enum Inside<'a> {
    /// Claude Code and everything it starts. It keeps its sessions in
    /// `~/.claude`, and it reaches this app through one socket, its own
    /// conversation's, which is `doorway`.
    ClaudeCode { doorway: Option<&'a Path> },
    /// A shell command a model asked for. It needs neither.
    ACommand,
}

/// Somewhere to write that is not a file anybody owns.
const SCRATCH: &[&str] = &["/private/tmp", "/private/var/folders", "/tmp"];

/// The profile: everything allowed, writing denied, and then the places writing
/// is allowed after all.
///
/// Written this way round rather than as a list of forbidden places because a
/// list of forbidden places is a list somebody has to keep complete, and the
/// day it is not complete is the day it is worth nothing.
pub fn profile(home: &Path, inside: Inside) -> String {
    profile_keeping_out(home, inside, crate::where_errand_lives().as_deref())
}

/// The profile, with the place Errand keeps its own things named.
///
/// Separate so that a test can lay out a place of its own rather than reach
/// for the real one.
fn profile_keeping_out(home: &Path, inside: Inside, errand: Option<&Path>) -> String {
    let mut allowed = vec![format!("  (subpath {})", quoted(home))];
    // Folders somebody allowed for this agent on purpose, on top of its own.
    // The wall used to be absolute: an agent set never to ask could not write
    // outside its folder by any means, and somebody who wanted a file on an
    // external disk every five minutes had no way to say so.
    for place in also_allowed(home) {
        allowed.push(format!("  (subpath {})", quoted(&place)));
    }
    // The environment rather than a crate, because this is the same HOME the
    // process being walled in will use, and the two agreeing is the point.
    let theirs = std::env::var("HOME").ok().map(PathBuf::from);
    if let Some(theirs) = &theirs {
        for place in TOOLS_KEEP_THEIR_OWN_STATE_IN {
            allowed.push(format!("  (subpath {})", quoted(&theirs.join(place))));
        }
        if matches!(inside, Inside::ClaudeCode { .. }) {
            allowed.push(format!(
                "  (subpath {})",
                quoted(&theirs.join(CLAUDE_CODE_KEEPS_ITS_STATE_IN))
            ));
            allowed.push(format!(
                "  (literal {})",
                quoted(&theirs.join(AND_THIS_ONE_FILE))
            ));
        }
    }
    for place in SCRATCH {
        allowed.push(format!("  (subpath {})", quoted(Path::new(place))));
    }
    // Writing to the terminal is not writing to a file, and a process that
    // cannot print is a process nobody can be told anything by.
    allowed.push("  (literal \"/dev/null\")".to_string());
    allowed.push("  (literal \"/dev/stdout\")".to_string());
    allowed.push("  (literal \"/dev/stderr\")".to_string());
    allowed.push("  (regex #\"^/dev/tty\")".to_string());

    let mut profile = format!(
        "(version 1)\n(allow default)\n(deny file-write*)\n(allow file-write*\n{})",
        allowed.join("\n")
    );
    // The package managers keep their programs beside their caches, and those
    // folders are on the person's PATH. A program left there by a walled
    // command runs unwalled the next time anybody types its name: a `git` in
    // `~/.cargo/bin` answers for git in every terminal. The caches stay
    // writable, which is what the allowance was for.
    if let Some(theirs) = &theirs {
        let programs: Vec<String> = PROGRAMS_ON_THE_PATH
            .iter()
            .map(|place| format!("  (subpath {})", quoted(&theirs.join(place))))
            .collect();
        profile.push_str(&format!("\n(deny file-write*\n{})", programs.join("\n")));
    }
    if let Some(theirs) = &theirs {
        profile.push_str(&keys_kept_out(theirs));
    }
    if let Some(errand) = errand {
        profile.push_str(&keep_out(errand, inside));
    }
    profile
}

/// Where SSH keeps its keys, relative to the home directory.
const SSH: &str = ".ssh";

/// Reading the person's private keys, kept from every errand, walled or asking.
///
/// Reading was left open because an errand that can read what the person can
/// read can do its job, and a private key is the exception the same way the
/// app's own keys are: reading it is the whole of the harm, since a copy works
/// anywhere, for anybody, for as long as the key does. It happened. An errand
/// on an agent set never to ask could not get `ssh` to work inside the wall,
/// which would not let it write `known_hosts`, so it copied the key, the
/// config and `known_hosts` into `/tmp` and used the copy. Nothing stopped it.
///
/// Using a key is not reading it. `ssh-agent` signs for whoever asks it and
/// never hands the key over, so SSH keeps working inside the wall for a key the
/// person has added to their agent, and a copy of the key is out of reach.
///
/// Everything in `~/.ssh` is unreadable to begin with and then what is not a
/// key is readable again, by name: the folder itself, the config, the known
/// hosts, the public halves. That way round, so a key made after this profile
/// was written is kept out too. A key kept elsewhere is kept out when the
/// config names it, or when something in `~/.ssh` is a link to it.
fn keys_kept_out(theirs: &Path) -> String {
    let Ok(ssh) = theirs.join(SSH).canonicalize() else {
        return String::new();
    };
    let mut readable = vec![format!("  (literal {})", quoted(&ssh))];
    let mut elsewhere: Vec<PathBuf> = Vec::new();
    let mut folders = vec![(ssh.clone(), 0)];
    while let Some((folder, deep)) = folders.pop() {
        let Ok(inside) = std::fs::read_dir(&folder) else {
            continue;
        };
        for one in inside.flatten() {
            let path = one.path();
            let Ok(real) = path.canonicalize() else {
                continue;
            };
            if real.is_dir() {
                // A folder of includes, or of the agent's own sockets. Two
                // levels is more than anybody's `~/.ssh` has.
                if real.starts_with(&ssh) && deep < 2 {
                    readable.push(format!("  (literal {})", quoted(&real)));
                    folders.push((real, deep + 1));
                }
            } else if a_private_key(&real) {
                if !real.starts_with(&ssh) {
                    elsewhere.push(real);
                }
            } else if real.starts_with(&ssh) {
                readable.push(format!("  (literal {})", quoted(&real)));
            }
        }
    }
    for named in keys_the_config_names(&ssh, theirs) {
        if !named.starts_with(&ssh) && !elsewhere.contains(&named) {
            elsewhere.push(named);
        }
    }
    let mut kept = format!(
        "\n(deny file-read-data (subpath {}))\n(allow file-read-data\n{})",
        quoted(&ssh),
        readable.join("\n")
    );
    if !elsewhere.is_empty() {
        let keys: Vec<String> = elsewhere
            .iter()
            .map(|key| format!("  (literal {})", quoted(key)))
            .collect();
        kept.push_str(&format!("\n(deny file-read-data\n{})", keys.join("\n")));
    }
    kept
}

/// Whether a file is a private key, by how every kind of one begins.
///
/// Anything that cannot be read to find out is counted as one, because the
/// cost of that mistake is a file an errand cannot read, and the cost of the
/// other is a key it can.
fn a_private_key(path: &Path) -> bool {
    use std::io::Read;
    let named = path.file_name().map(|n| n.to_string_lossy().to_string());
    let named = named.as_deref().unwrap_or_default();
    if named.ends_with(".pub") || named == "config" || named.starts_with("known_hosts") {
        return false;
    }
    let mut start = [0u8; 160];
    let Ok(mut file) = std::fs::File::open(path) else {
        return true;
    };
    let Ok(read) = file.read(&mut start) else {
        return true;
    };
    let start = String::from_utf8_lossy(&start[..read]);
    start.contains("PRIVATE KEY") || start.starts_with("PuTTY-User-Key-File")
}

/// The keys the SSH config says to use, wherever they are kept.
fn keys_the_config_names(ssh: &Path, theirs: &Path) -> Vec<PathBuf> {
    let Ok(config) = std::fs::read_to_string(ssh.join("config")) else {
        return Vec::new();
    };
    config
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let (key, value) = line.split_once(|c: char| c.is_whitespace() || c == '=')?;
            if !key.eq_ignore_ascii_case("IdentityFile") {
                return None;
            }
            let value = value
                .trim()
                .trim_start_matches('=')
                .trim()
                .trim_matches('"');
            let path = match value.strip_prefix("~/") {
                Some(rest) => theirs.join(rest),
                None => PathBuf::from(value),
            };
            path.canonicalize().ok()
        })
        .collect()
}

/// Where the package managers above keep programs, rather than caches.
const PROGRAMS_ON_THE_PATH: &[&str] = &[".cargo/bin", ".bun/bin", ".deno/bin"];

/// A path as a string in the profile's own language.
///
/// Only the backslash and the quote are special there. Rust's debug quoting was
/// used before, and it also escapes characters that are nothing of the kind:
/// the joiner in an emoji and the accent written as its own character both came
/// out as `\u{..}`, which the profile reads as those letters, so a folder called
/// "✏️ Drafts" was allowed under a name that is not its name.
fn quoted(path: &Path) -> String {
    let text = path.display().to_string();
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        if c == '\\' || c == '"' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// What no errand may touch, read or write, whatever else it has been allowed.
///
/// The wall was about writing, and reading was left open on purpose: an errand
/// that can read what the person can read is an errand that can do its job.
/// These are the exceptions, because reading them is the whole of the harm.
/// The keys are the person's paid accounts, and every agent runs as the person,
/// so the files being theirs alone kept nobody out. The store is every
/// conversation they have had with every agent. And the sockets under `mcp` are
/// how the app is driven from outside: a walled command that could open one
/// could ask any agent for anything, or speak as another conversation, with no
/// card in front of either. A page an agent reads can ask for all three, and
/// every agent on this Mac was set to act without asking.
///
/// After everything that allows, because in a profile the last rule that
/// matches is the one that holds: a folder allowed under Allowed that happened
/// to contain these would otherwise open them again.
fn keep_out(errand: &Path, inside: Inside) -> String {
    // The real path, because the sandbox compares real paths: a place named
    // through a link would be a rule that matches nothing.
    let errand = errand
        .canonicalize()
        .unwrap_or_else(|_| errand.to_path_buf());
    let mut never = vec![format!("  (subpath {})", quoted(&errand.join("keys")))];
    // The store, its journal files, and any copy of it kept beside it, such as
    // the one taken before clearing. Named one by one rather than by pattern,
    // because a pattern is a second language inside this one with escaping of
    // its own, and a folder name with a dot or a bracket in it would mean
    // something else there.
    let mut stores: Vec<PathBuf> = [
        "errand.db",
        "errand.db-wal",
        "errand.db-shm",
        "errand.db-journal",
    ]
    .iter()
    .map(|name| errand.join(name))
    .collect();
    if let Ok(beside) = std::fs::read_dir(&errand) {
        for one in beside.flatten() {
            let copy = one.path();
            if one.file_name().to_string_lossy().contains(".db") && !stores.contains(&copy) {
                stores.push(copy);
            }
        }
    }
    for store in &stores {
        never.push(format!("  (literal {})", quoted(store)));
    }
    let mut kept = format!("\n(deny file-read* file-write*\n{})", never.join("\n"));
    kept.push_str(&format!(
        "\n(deny network-outbound (subpath {}))",
        quoted(&errand.join("mcp"))
    ));
    // Claude Code reaches this app's tools through its own conversation's door,
    // and through no other.
    if let Inside::ClaudeCode { doorway: Some(own) } = inside {
        kept.push_str(&format!(
            "\n(allow network-outbound (literal {}))",
            quoted(own)
        ));
    }
    kept
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
         then it works. Until then, do the work inside your own folder.\n\n\
         Private keys are not readable here: the SSH keys in ~/.ssh are kept from you, \
         and SSH works only with a key the person has added to their ssh-agent. If ssh \
         fails because a key cannot be read, or because a host is not yet in known_hosts, \
         stop and say so plainly. Never copy a key, an SSH config or known_hosts somewhere \
         else, and never look for another way round the wall: the wall is the person's \
         decision, and working round it is the one thing that is never the errand.",
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
pub fn around(program: &str, home: &Path, inside: Inside) -> tokio::process::Command {
    let mut walled = tokio::process::Command::new(THE_SANDBOX);
    walled.arg("-p").arg(profile(home, inside)).arg(program);
    walled
}

/// A command that runs `program` with nothing kept from it but the app's own
/// things and the person's private keys.
///
/// For Claude Code on an agent that asks. Asking is the wall there, and the
/// better one while it is switched on; but a few of Claude Code's tools are
/// granted without a card, reading any file among them, and a page it has just
/// read can ask it to read the keys and fetch an address with them in it. So
/// what `keep_out` keeps from every walled errand is kept from this one too,
/// and nothing else is.
pub fn kept_out(program: &str, doorway: Option<&Path>) -> tokio::process::Command {
    match (possible(), crate::where_errand_lives()) {
        (true, Some(errand)) => {
            let mut kept = tokio::process::Command::new(THE_SANDBOX);
            kept.arg("-p")
                .arg(only_kept_out(&errand, doorway))
                .arg(program);
            kept
        }
        _ => tokio::process::Command::new(program),
    }
}

/// The profile for that: everything allowed, and then the app's own things and
/// the person's private keys not.
fn only_kept_out(errand: &Path, doorway: Option<&Path>) -> String {
    let theirs = std::env::var("HOME").ok().map(PathBuf::from);
    format!(
        "(version 1)\n(allow default){}{}",
        theirs.as_deref().map(keys_kept_out).unwrap_or_default(),
        keep_out(errand, Inside::ClaudeCode { doorway })
    )
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
    let mut sh = around("/bin/sh", home, Inside::ACommand);
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
        let said = profile(home, Inside::ACommand);
        assert!(said.contains("(deny file-write*)"));
        assert!(said.contains("/tmp/an-errand"), "{said}");

        // The whole point: the place the folder sits inside is not writable
        // just because the folder is.
        let theirs = std::path::PathBuf::from(std::env::var("HOME").expect("a home"));
        let bare = format!("  (subpath {})", quoted(&theirs));
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
        let theirs = std::path::PathBuf::from(std::env::var("HOME").expect("a home"));
        for inside in [Inside::ACommand, Inside::ClaudeCode { doorway: None }] {
            let said = profile(Path::new("/tmp/an-errand"), inside);
            for place in TOOLS_KEEP_THEIR_OWN_STATE_IN {
                let want = theirs.join(place).display().to_string();
                assert!(said.contains(&want), "{place} is not allowed:\n{said}");
            }
        }
        let claude = profile(
            Path::new("/tmp/an-errand"),
            Inside::ClaudeCode { doorway: None },
        );
        assert!(claude.contains(&theirs.join(AND_THIS_ONE_FILE).display().to_string()));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn claude_code_on_an_agent_that_asks_is_kept_from_the_keys_and_nothing_else() {
        // No wall there, because asking is the wall; but reading a file is
        // granted without a card, so the keys have to be kept from it anyway.
        if !possible() {
            return;
        }
        let errand = std::env::temp_dir().join(format!("errand-kept-out-{}", std::process::id()));
        std::fs::remove_dir_all(&errand).ok();
        std::fs::create_dir_all(errand.join("keys")).expect("a place");
        let errand = errand.canonicalize().expect("a real path");
        std::fs::write(errand.join("keys").join("kimi"), "the key").unwrap();
        let elsewhere = errand.join("anywhere.txt");

        let run = |command: &str| {
            let out = std::process::Command::new(THE_SANDBOX)
                .arg("-p")
                .arg(only_kept_out(&errand, None))
                .arg("/bin/sh")
                .arg("-c")
                .arg(command)
                .output()
                .expect("the sandbox runs");
            String::from_utf8_lossy(&out.stdout).to_string()
        };
        let key = quoted(&errand.join("keys").join("kimi"));
        assert!(
            run(&format!("cat {key}")).is_empty(),
            "the key was readable"
        );
        // Everything else is as it was: writing anywhere the person can.
        run(&format!("echo written > {}", quoted(&elsewhere)));
        assert_eq!(
            std::fs::read_to_string(&elsewhere).unwrap_or_default(),
            "written\n"
        );
        std::fs::remove_dir_all(&errand).ok();
    }

    #[test]
    fn claude_codes_own_state_is_writable_only_behind_claude_codes_wall() {
        // A command a local model runs has no use for ~/.claude, and the hooks
        // in its settings run unwalled in the person's own sessions: a command
        // that could write there could leave behind something that runs
        // outside the wall the next time they open Claude Code themselves.
        let theirs = std::path::PathBuf::from(std::env::var("HOME").expect("a home"));
        let folder = format!(
            "(subpath {})",
            quoted(&theirs.join(CLAUDE_CODE_KEEPS_ITS_STATE_IN))
        );
        let file = format!("(literal {})", quoted(&theirs.join(AND_THIS_ONE_FILE)));

        let command = profile(Path::new("/tmp/an-errand"), Inside::ACommand);
        assert!(!command.contains(&folder), "{command}");
        assert!(!command.contains(&file), "{command}");

        let claude = profile(
            Path::new("/tmp/an-errand"),
            Inside::ClaudeCode { doorway: None },
        );
        assert!(claude.contains(&folder), "{claude}");
        assert!(claude.contains(&file), "{claude}");
    }

    #[test]
    fn the_folders_package_managers_keep_programs_in_are_never_writable() {
        // They are on the person's PATH, so a program left in one runs
        // unwalled the next time anybody types its name.
        let theirs = std::path::PathBuf::from(std::env::var("HOME").expect("a home"));
        for inside in [Inside::ACommand, Inside::ClaudeCode { doorway: None }] {
            let said = profile(Path::new("/tmp/an-errand"), inside);
            let denied = said
                .split("\n(deny file-write*\n")
                .nth(1)
                .unwrap_or_else(|| panic!("nothing is denied after the allowances:\n{said}"));
            for place in PROGRAMS_ON_THE_PATH {
                let want = format!("(subpath {})", quoted(&theirs.join(place)));
                assert!(denied.contains(&want), "{place} is writable:\n{said}");
            }
        }
    }

    #[test]
    fn a_folder_is_named_in_the_profile_exactly_as_it_is_named_on_disk() {
        // Rust's debug quoting wrote the joiner in an emoji and an accent kept
        // as its own character as `\u{..}`, which the profile reads as those
        // letters: the folder was allowed under a name that is not its name.
        let home = Path::new("/tmp/errand-wall-\u{270f}\u{fe0f} Drafts-e\u{301}");
        let said = profile(home, Inside::ACommand);
        assert!(
            said.contains("\"/tmp/errand-wall-\u{270f}\u{fe0f} Drafts-e\u{301}\""),
            "{said}"
        );
        assert!(!said.contains("\\u{"), "{said}");
        // And the two characters that are special there are still escaped.
        assert_eq!(quoted(Path::new("/a\"b\\c")), "\"/a\\\"b\\\\c\"");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_errand_cannot_read_the_keys_or_the_store_or_open_the_apps_doors() {
        // Every agent runs as the person, so the files being theirs alone kept
        // nobody out: a command on an agent set never to ask could read the
        // keys and every conversation, and open the sockets the app is driven
        // through. Run rather than read, because what matters is what the
        // sandbox makes of the profile.
        if !possible() {
            return;
        }
        // A place of the test's own, laid out the way Errand's is, and named
        // by its real path because the sandbox compares real paths.
        let errand = std::env::temp_dir().join(format!("errand-wall-out-{}", std::process::id()));
        std::fs::remove_dir_all(&errand).ok();
        std::fs::create_dir_all(&errand).expect("a place");
        let errand = errand.canonicalize().expect("a real path");
        let home = errand.join("threads").join("one");
        for folder in [errand.join("keys"), errand.join("mcp"), home.clone()] {
            std::fs::create_dir_all(folder).expect("a folder");
        }
        std::fs::write(errand.join("keys").join("deepseek"), "the key").unwrap();
        std::fs::write(errand.join("errand.db"), "every conversation").unwrap();
        std::fs::write(errand.join("errand-before-clearing-1.db"), "an old copy").unwrap();
        std::fs::write(home.join("mine.txt"), "its own").unwrap();

        // Two doors: another conversation's, and this one's own.
        let other = errand.join("mcp").join("other.sock");
        let own = errand.join("mcp").join("own.sock");
        for door in [&other, &own] {
            let listening = std::os::unix::net::UnixListener::bind(door).expect("a door");
            std::thread::spawn(move || {
                for mut knocked in listening.incoming().flatten() {
                    let _ = std::io::Write::write_all(&mut knocked, b"opened");
                }
            });
        }

        let run = |inside: Inside, command: &str| {
            let out = std::process::Command::new(THE_SANDBOX)
                .arg("-p")
                .arg(profile_keeping_out(&home, inside, Some(&errand)))
                .arg("/bin/sh")
                .arg("-c")
                .arg(command)
                .current_dir(&home)
                .output()
                .expect("the sandbox runs");
            String::from_utf8_lossy(&out.stdout).to_string()
        };
        let a_command = Inside::ACommand;
        assert_eq!(
            run(a_command, "cat mine.txt"),
            "its own",
            "its own folder stopped being readable"
        );
        for secret in [
            errand.join("keys").join("deepseek"),
            errand.join("errand.db"),
            errand.join("errand-before-clearing-1.db"),
        ] {
            let read = run(a_command, &format!("cat {}", quoted(&secret)));
            assert!(
                read.is_empty(),
                "{} was readable inside the wall: {read}",
                secret.display()
            );
        }
        let knock = |door: &Path| format!("/usr/bin/nc -U {} </dev/null", quoted(door));
        assert!(
            run(a_command, &knock(&other)).is_empty(),
            "a command opened the app's door"
        );
        assert!(
            run(a_command, &knock(&own)).is_empty(),
            "a command opened a conversation's door"
        );

        // Claude Code reaches the app through its own door and no other.
        let claude = Inside::ClaudeCode {
            doorway: Some(&own),
        };
        assert_eq!(
            run(claude, &knock(&own)),
            "opened",
            "Claude Code lost its own door"
        );
        assert!(
            run(claude, &knock(&other)).is_empty(),
            "Claude Code opened another conversation's door"
        );

        std::fs::remove_dir_all(&errand).ok();
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
        let said = profile(mine, Inside::ACommand);
        assert!(said.contains("(subpath \"/Volumes/Somewhere\")"), "{said}");
        assert!(
            !profile(other, Inside::ACommand).contains("/Volumes/Somewhere"),
            "the folder leaked into another errand's wall"
        );
        // Taking it back is the same call with nothing in it.
        also_allow(mine, vec![]);
        assert!(!profile(mine, Inside::ACommand).contains("/Volumes/Somewhere"));
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

    #[tokio::test(flavor = "multi_thread")]
    async fn a_private_key_cannot_be_read_or_copied_and_ssh_still_signs_with_it() {
        // An errand that could not get ssh working inside the wall copied the
        // key into /tmp and used the copy. Run rather than read, because what
        // matters is what the sandbox makes of the profile, and what ssh can
        // still do through an agent. Every key here is made by the test, now.
        if !possible() || !Path::new("/usr/bin/ssh-agent").exists() {
            return;
        }
        let theirs = std::env::temp_dir().join(format!("errand-wall-keys-{}", std::process::id()));
        std::fs::remove_dir_all(&theirs).ok();
        std::fs::create_dir_all(theirs.join(".ssh").join("agent")).expect("a place");
        std::fs::create_dir_all(theirs.join("keys")).expect("a place");
        let theirs = theirs.canonicalize().expect("a real path");
        let ssh = theirs.join(".ssh");
        let make = |key: &Path| {
            std::process::Command::new("/usr/bin/ssh-keygen")
                .args([
                    "-q",
                    "-t",
                    "ed25519",
                    "-N",
                    "",
                    "-C",
                    "errand-wall-test",
                    "-f",
                ])
                .arg(key)
                .status()
                .is_ok_and(|s| s.success())
        };
        let key = ssh.join("id_test");
        let kept_elsewhere = theirs.join("keys").join("id_elsewhere");
        let linked_elsewhere = theirs.join("keys").join("id_linked");
        if !make(&key) || !make(&kept_elsewhere) || !make(&linked_elsewhere) {
            return;
        }
        std::fs::write(
            ssh.join("config"),
            "Host m5\n  HostName 192.0.2.1\n  IdentityFile ~/.ssh/id_test\n\
             Host other\n  IdentityFile ~/keys/id_elsewhere\n",
        )
        .unwrap();
        std::fs::write(ssh.join("known_hosts"), "192.0.2.1 ssh-ed25519 AAAA\n").unwrap();
        std::os::unix::fs::symlink(&linked_elsewhere, ssh.join("id_through_a_link")).unwrap();

        // An agent of the test's own, holding the key.
        let socket = ssh.join("agent").join("test.sock");
        let mut agent = std::process::Command::new("/usr/bin/ssh-agent")
            .arg("-D")
            .arg("-a")
            .arg(&socket)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("an agent");
        for _ in 0..50 {
            if socket.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
        let added = std::process::Command::new("/usr/bin/ssh-add")
            .arg(&key)
            .env("SSH_AUTH_SOCK", &socket)
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        assert!(added, "the test's own agent took no key");

        let profile = format!("(version 1)\n(allow default){}", keys_kept_out(&theirs));
        // One made after the profile was written, which a list of keys would miss.
        let later = ssh.join("id_later");
        assert!(make(&later));
        let run = |command: &str| {
            let out = std::process::Command::new(THE_SANDBOX)
                .arg("-p")
                .arg(&profile)
                .arg("/bin/sh")
                .arg("-c")
                .arg(command)
                .env("SSH_AUTH_SOCK", &socket)
                .current_dir(&theirs)
                .output()
                .expect("the sandbox runs");
            String::from_utf8_lossy(&out.stdout).to_string()
        };
        for secret in [
            &key,
            &later,
            &kept_elsewhere,
            &ssh.join("id_through_a_link"),
        ] {
            let read = run(&format!("cat {}", quoted(secret)));
            assert!(
                read.is_empty(),
                "{} was readable inside the wall",
                secret.display()
            );
        }
        let copy = theirs.join("copied");
        run(&format!("cp {} {}", quoted(&key), quoted(&copy)));
        assert!(!copy.exists(), "the key was copied from inside the wall");
        run(&format!(
            "ln {} {}",
            quoted(&key),
            quoted(&theirs.join("hard"))
        ));
        assert!(
            run(&format!("cat {}", quoted(&theirs.join("hard")))).is_empty(),
            "a hard link read the key"
        );
        assert!(
            run(&format!("/usr/bin/ssh-keygen -y -f {}", quoted(&key))).is_empty(),
            "ssh-keygen read the key"
        );

        // What ssh needs is still there, and so is the key, to sign with.
        assert!(
            run("cat .ssh/config").contains("Host m5"),
            "the config went"
        );
        assert!(
            run("cat .ssh/known_hosts").contains("192.0.2.1"),
            "known_hosts went"
        );
        assert!(
            run("cat .ssh/id_test.pub").contains("errand-wall-test"),
            "the public half went"
        );
        assert!(
            run("ls .ssh").contains("id_test.pub"),
            "the folder could not be listed"
        );
        assert!(
            run("/usr/bin/ssh-add -L").contains("errand-wall-test"),
            "the agent could not be reached from inside the wall"
        );

        let _ = agent.kill();
        let _ = agent.wait();
        std::fs::remove_dir_all(&theirs).ok();
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
        // And never to go round it, which is what an errand did with a key.
        assert!(said.contains("Never copy a key"), "{said}");
        also_allow(home, vec![]);
    }
}
