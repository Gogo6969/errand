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
//! What may be written is a short list on purpose, and every entry on it was
//! added because something real stopped working without it. A wall that
//! produces misleading errors is worse than no wall, because somebody will
//! follow the advice: with `~/.npm` closed, `npx` failed with npm's own advice
//! to run `sudo chown` on a directory that was fine. The answer to that is
//! never to open the person's own folders again, whose contents their tools
//! run later, but to give each tool a folder of the errand's own, which is
//! what `kept_in_its_own_folder` does.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Where sandboxing lives on macOS.
///
/// Deprecated by Apple for years and still the only thing of its kind that a
/// plain process can ask for without entitlements or a helper.
const THE_SANDBOX: &str = "/usr/bin/sandbox-exec";

/// Where each tool keeps its downloads and caches while walled in, inside
/// the errand's own folder, by the variable that tells the tool so.
///
/// They used to be the person's own: `~/.npm`, `~/.cargo`, `~/.cache`,
/// `~/Library/Caches` and the rest were writable from inside the wall, because
/// without them `npx` failed with npm's advice to run `sudo chown` on a folder
/// that was fine. But what is in those folders is run later, outside the wall,
/// by the person's own tools: a package `npx` reuses, a crate `cargo` builds,
/// a wheel `pip` installs, a browser Playwright starts. A teammate that could
/// write there could leave code behind for them. Each tool is pointed at a
/// folder of the errand's own instead, which it can fill as it likes, and the
/// person's folders are only read. Checked on 4 October 2026 with npm, npx,
/// node-gyp, pnpm, bun, deno, cargo, pip, uv, go, swift, clang and git.
const KEPT_IN_ITS_OWN_FOLDER: &[(&str, &str)] = &[
    ("XDG_CACHE_HOME", ".cache"),
    ("npm_config_cache", ".cache/npm"),
    ("npm_package_config_node_gyp_devdir", ".cache/node-gyp"),
    ("PNPM_HOME", ".cache/pnpm"),
    ("CARGO_HOME", ".cache/cargo"),
    ("BUN_INSTALL_CACHE_DIR", ".cache/bun"),
    ("DENO_DIR", ".cache/deno"),
    ("UV_CACHE_DIR", ".cache/uv"),
    ("UV_TOOL_DIR", ".cache/uv-tools"),
    ("UV_TOOL_BIN_DIR", ".cache/uv-tools/bin"),
    ("PIP_CACHE_DIR", ".cache/pip"),
    ("PYTHONPYCACHEPREFIX", ".cache/pycache"),
    ("MPLCONFIGDIR", ".cache/matplotlib"),
    ("IPYTHONDIR", ".cache/ipython"),
    ("JUPYTER_CONFIG_DIR", ".cache/jupyter/config"),
    ("JUPYTER_RUNTIME_DIR", ".cache/jupyter/runtime"),
    ("GOCACHE", ".cache/go-build"),
    ("GOMODCACHE", ".cache/go-mod"),
    ("GOPATH", ".cache/go"),
    ("HF_HOME", ".cache/huggingface"),
    ("CLANG_MODULE_CACHE_PATH", ".cache/clang-modules"),
    ("NODE_COMPILE_CACHE", ".cache/node-compile-cache"),
    ("ZSH_COMPDUMP", ".cache/zcompdump"),
];

/// The variables above, with the errand's folder filled in, and a few that
/// are values rather than places: Go's module cache is made writable so the
/// folder can be cleared, and rustup installs no toolchain from inside.
pub fn kept_in_its_own_folder(home: &Path) -> Vec<(String, String)> {
    let mut set: Vec<(String, String)> = KEPT_IN_ITS_OWN_FOLDER
        .iter()
        .map(|(name, place)| (name.to_string(), format!("{}/{place}", home.display())))
        .collect();
    // Temporary files in a folder of its own with a short name, rather than
    // inside its folder: a socket's path has to fit in 104 bytes, and one
    // made under a teammate's folder, ninety-odd characters already, did not,
    // so Python's process pools and tmux failed there.
    let temporary = its_own_temporary_folder(home);
    set.push(("TMPDIR".into(), format!("{}/", temporary.display())));
    set.push(("CLAUDE_CODE_TMPDIR".into(), temporary.display().to_string()));
    // zsh keeps a here-document in a file named by this, not by TMPDIR, and
    // its own default is in /tmp, which the wall keeps: every `cat <<EOF` a
    // teammate's commands ran failed with "can't create temp file".
    set.push(("TMPPREFIX".into(), format!("{}/zsh", temporary.display())));
    // Claude Code's own memory, which it keeps beside its record and reads
    // into every later session in the folder: a teammate could write its own
    // instructions there, around the rule that it changes only through the
    // person's yes. Errand's notes are its memory instead.
    set.push(("CLAUDE_CODE_DISABLE_AUTO_MEMORY".into(), "1".into()));
    set.push(("GOFLAGS".into(), "-modcacherw".into()));
    set.push(("RUSTUP_AUTO_INSTALL".into(), "0".into()));
    set
}

/// Where an errand's temporary files go: a folder of its own under the
/// system's shared one, named after a digest of its folder so that every
/// errand has a different one and the name stays short.
pub fn its_own_temporary_folder(home: &Path) -> PathBuf {
    use sha2::Digest;
    let real = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());
    let digest = sha2::Sha256::digest(real.to_string_lossy().as_bytes());
    let short: String = digest.iter().take(6).map(|b| format!("{b:02x}")).collect();
    PathBuf::from(format!("/private/tmp/errand-{short}"))
}

/// Make that folder, only the person's to read, and never through a link
/// something else put there first.
fn make_its_own_temporary_folder(at: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if std::fs::symlink_metadata(at).is_ok_and(|m| !m.file_type().is_dir()) {
        std::fs::remove_file(at).ok();
    }
    if std::fs::create_dir_all(at).is_ok() {
        std::fs::set_permissions(at, std::fs::Permissions::from_mode(0o700)).ok();
    }
}

/// Where Claude Code keeps a conversation, relative to the home directory:
/// one folder per working directory, named after it with everything but
/// letters and digits made a dash.
const CLAUDE_CODE_KEEPS_CONVERSATIONS_IN: &str = ".claude/projects";

/// The folder Claude Code keeps this working directory's conversations in.
///
/// Named after the real path, as Claude Code names it: a folder reached
/// through a link was given a record folder Claude Code never wrote to, and
/// the conversation could not be carried on.
pub fn claude_codes_folder_for(theirs: &Path, cwd: &Path) -> PathBuf {
    let theirs = theirs
        .canonicalize()
        .unwrap_or_else(|_| theirs.to_path_buf());
    let cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    let named: String = cwd
        .to_string_lossy()
        .chars()
        .map(|c| match c.is_ascii_alphanumeric() {
            true => c,
            false => '-',
        })
        .collect();
    theirs.join(CLAUDE_CODE_KEEPS_CONVERSATIONS_IN).join(named)
}

/// The system's own temporary folder for the person, by its real path.
///
/// Asked of the system rather than read from `TMPDIR`, because that is what
/// the programs that use it do, and it is set to the errand's own folder
/// inside the wall.
pub fn the_systems_temporary_folder() -> Option<PathBuf> {
    let mut buffer = vec![0 as libc::c_char; 1024];
    // SAFETY: the buffer is as long as said, and confstr writes at most that
    // much, terminated.
    let wrote = unsafe {
        libc::confstr(
            libc::_CS_DARWIN_USER_TEMP_DIR,
            buffer.as_mut_ptr(),
            buffer.len(),
        )
    };
    if wrote == 0 || wrote > buffer.len() {
        return None;
    }
    // SAFETY: confstr terminated what it wrote.
    let said = unsafe { std::ffi::CStr::from_ptr(buffer.as_ptr()) };
    PathBuf::from(said.to_string_lossy().to_string())
        .canonicalize()
        .ok()
}

/// A path as a regular expression matching exactly it, in the profile's
/// language. Nothing for a path the language could not hold.
fn as_a_pattern(path: &Path) -> Option<String> {
    let text = path.display().to_string();
    if text.contains('"') {
        return None;
    }
    let mut out = String::new();
    for c in text.chars() {
        if "\\.^$*+?()[]{}|".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    Some(out)
}

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

/// The profile: everything allowed, writing denied, and then the places writing
/// is allowed after all.
///
/// Written this way round rather than as a list of forbidden places because a
/// list of forbidden places is a list somebody has to keep complete, and the
/// day it is not complete is the day it is worth nothing.
pub fn profile(home: &Path, inside: Inside) -> String {
    // The environment rather than a crate, because this is the same HOME the
    // process being walled in will use, and the two agreeing is the point.
    let theirs = std::env::var("HOME").ok().map(PathBuf::from);
    profile_keeping_out(
        home,
        inside,
        crate::where_errand_lives().as_deref(),
        theirs.as_deref(),
    )
}

/// The profile, with the place Errand keeps its own things named, and the
/// person's home folder.
///
/// Separate so that a test can lay out places of its own rather than reach
/// for the real ones.
fn profile_keeping_out(
    home: &Path,
    inside: Inside,
    errand: Option<&Path>,
    theirs: Option<&Path>,
) -> String {
    // By its real path, because the sandbox compares real paths: a folder
    // named through a link would be a rule that matches nothing, and the
    // errand could write nowhere at all.
    let real = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());
    let mut allowed = vec![format!("  (subpath {})", quoted(&real))];
    // Folders somebody allowed for this agent on purpose, on top of its own.
    // The wall used to be absolute: an agent set never to ask could not write
    // outside its folder by any means, and somebody who wanted a file on an
    // external disk every five minutes had no way to say so.
    for place in also_allowed(home) {
        allowed.push(format!("  (subpath {})", quoted(&place)));
    }
    // Claude Code's record of this conversation, so it can be carried on, and
    // nothing else of Claude Code's: its settings, hooks, skills and plugins
    // are run by the person's own sessions, outside every wall, and so is the
    // list of servers in `~/.claude.json`. Run with a stand-in for the model on
    // 4 October 2026, it finished turns and carried them on with only this,
    // and said what it could not save and went on.
    let record = theirs.map(|theirs| claude_codes_folder_for(theirs, home));
    if let (Some(record), Inside::ClaudeCode { .. }) = (&record, inside) {
        allowed.push(format!("  (subpath {})", quoted(record)));
    }
    // Its own temporary folder, made by the app before it starts.
    allowed.push(format!(
        "  (subpath {})",
        quoted(&its_own_temporary_folder(home))
    ));
    // The system's own temporary folder, and only what the system's own tools
    // make there whatever TMPDIR says: Swift's working folders, Foundation's
    // items, lock files, and `mktemp`'s names. None of the rest. That folder
    // also holds what the person's programs keep and run again, such as the
    // cache the system's `git` and `clang` are found through, and a teammate
    // that could rewrite it could put another program in their place.
    if let Some(temporary) = the_systems_temporary_folder() {
        if let Some(pattern) = as_a_pattern(&temporary) {
            allowed.push(format!(
                "  (regex #\"^{pattern}/TemporaryDirectory\\.[^/]+(/|$)\")"
            ));
            allowed.push(format!(
                "  (subpath {})",
                quoted(&temporary.join("TemporaryItems"))
            ));
            allowed.push(format!("  (regex #\"^{pattern}/[^/]+\\.lock$\")"));
            // `mktemp` and `mktemp -t name`, which ignore TMPDIR on macOS.
            allowed.push(format!("  (regex #\"^{pattern}/tmp\\.[A-Za-z0-9]+(/|$)\")"));
            allowed.push(format!(
                "  (regex #\"^{pattern}/[A-Za-z0-9_-]+\\.[A-Za-z0-9]{{8}}(/|$)\")"
            ));
        }
    }
    // Writing to the terminal is not writing to a file, and a process that
    // cannot print is a process nobody can be told anything by.
    allowed.push("  (literal \"/dev/null\")".to_string());
    allowed.push("  (literal \"/dev/stdout\")".to_string());
    allowed.push("  (literal \"/dev/stderr\")".to_string());
    allowed.push("  (regex #\"^/dev/tty\")".to_string());
    // Every process asks for this and is refused, harmlessly; allowed so the
    // refusal is not mistaken for anything.
    allowed.push("  (literal \"/dev/dtracehelper\")".to_string());

    let mut profile = format!(
        "(version 1)\n(allow default)\n(deny file-write*)\n(allow file-write*\n{})",
        allowed.join("\n")
    );
    profile.push_str(&its_own_settings_kept(home));
    // The same for every folder it was allowed besides its own, a team's
    // shared one among them: the person opens those as projects, and an
    // engine started there reads the settings at their top as the person's.
    // Kept whole as well, so a member cannot move a team's folder away.
    for place in also_allowed(home) {
        profile.push_str(&its_own_settings_kept(&place));
    }
    // Claude Code's own memory beside its record, which it would read into
    // every later session in this folder: switched off, and kept unwritable
    // for a version that ignores the switch.
    if let (Some(record), Inside::ClaudeCode { .. }) = (&record, inside) {
        profile.push_str(&format!(
            "\n(deny file-write* (subpath {}))",
            quoted(&record.join("memory"))
        ));
    }
    if let Some(theirs) = theirs {
        profile.push_str(&keys_kept_out(theirs));
    }
    profile.push_str(&daemons_kept_out(home));
    if let Some(errand) = errand {
        profile.push_str(&keep_out(errand, inside, Some(home)));
    }
    profile.push_str(NOTHING_IS_OPENED_FROM_INSIDE);
    profile.push_str(NOTHING_DRIVES_THE_SCREEN);
    profile
}

/// What an engine reads from the top of its own folder as its settings, by
/// name: Claude Code's project settings, with their hooks and allow-rules, its
/// instructions, and the servers it starts. A teammate that could write these
/// would be choosing its own rules, and they would hold the day it is set to
/// ask rather than walled. Matched without regard to case, as the disk is.
const WHAT_AN_ENGINE_READS: &[&str] = &[".claude", ".mcp.json", "CLAUDE.md", "CLAUDE.local.md"];

/// Its own folder may not be renamed or removed, nor its settings written.
///
/// The folder itself as well as the names, because a rule on a path inside a
/// writable folder holds only while the folder stays where it is: renamed to
/// somewhere else writable, edited there, and renamed back, the rule never
/// matches. Seen happen, on a throwaway folder, to the rule that kept
/// programs out of `~/.cargo/bin`.
fn its_own_settings_kept(home: &Path) -> String {
    let home = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());
    let mut kept = vec![format!("  (literal {})", quoted(&home))];
    for name in WHAT_AN_ENGINE_READS {
        kept.push(format!("  (subpath {})", quoted(&home.join(name))));
    }
    format!("\n(deny file-write*\n{})", kept.join("\n"))
}

/// Where the system keeps the sockets of its own services: the one that
/// answers for names, and the key agent's.
const NAMES_ARE_LOOKED_UP_AT: &str = "/private/var/run/mDNSResponder";
const THE_KEY_AGENT_LISTENS_AT: &str = "^/private/var/run/com\\.apple\\.launchd\\.[^/]+/Listeners$";

/// No socket of anybody else's server, other than the two the system needs.
///
/// A server that runs commands for whoever connects to its socket runs them
/// outside the wall: a terminal multiplexer, a container daemon, a helper of
/// the person's own. So connecting to a local socket is refused, and then
/// allowed again for looking up names, for the key agent, so SSH keeps
/// working, and for anything the errand itself started in its own folder.
/// Connections over the network are untouched. Errand's own doorway is
/// allowed back after this, by `keep_out`.
fn daemons_kept_out(home: &Path) -> String {
    let home = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());
    format!(
        "\n(deny network-outbound (remote unix-socket (path-prefix \"/\")))\n\
         (allow network-outbound\n  (literal \"{NAMES_ARE_LOOKED_UP_AT}\")\n  \
         (regex #\"{THE_KEY_AGENT_LISTENS_AT}\")\n  (subpath {}))",
        quoted(&home)
    )
}

/// Nothing inside the wall may drive the screen, should Errand ever be
/// allowed to: Accessibility is how one program presses another's buttons,
/// and synthetic input is how it types. Every teammate runs as Errand, so
/// whatever the app may do on screen, a teammate could, its own cards and a
/// terminal included. Today the app holds neither and the system refuses;
/// this is for the day somebody grants it.
///
/// And no Apple Event to another app: a running Terminal told to run a
/// command would run it outside the wall. Every teammate transcript on record
/// already shows the system refusing sandboxed senders (-10004, from Mail,
/// Calendar, Finder, Messages, Notes, Reminders and System Events); this says
/// so in the wall as well, whatever a later macOS decides.
pub const NOTHING_DRIVES_THE_SCREEN: &str =
    "\n(deny mach-lookup (global-name \"com.apple.axserver\"))\n\
     (deny iokit-open-user-client (iokit-user-client-class \"IOHIDParamUserClient\"))\n\
     (deny appleevent-send)";

/// Where SSH keeps its keys, relative to the home directory.
const SSH: &str = ".ssh";

/// Every copy of a key, wherever it is kept: anything inside a folder called
/// `.ssh`, and any file named the way `ssh-keygen` names a private key. The
/// public halves end in `.pub` and are not matched.
const KEYS_ANYWHERE: &str =
    "(regex #\"/\\.ssh(/|$)\")\n  (regex #\"/id_(rsa|dsa|ecdsa|ed25519)(_sk)?$\")";

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
///
/// And before any of that, every other copy: any folder called `.ssh`
/// wherever it is, and any file with a private key's usual name. A teammate
/// asked about a disk listed the Time Machine backup of the person's home,
/// `.ssh` and all, and the key in that backup is the same key. Only the live
/// `~/.ssh` gets anything back, by the rules after these, which win.
fn keys_kept_out(theirs: &Path) -> String {
    let every_copy = format!("\n(deny file-read-data\n  {KEYS_ANYWHERE})");
    let Ok(ssh) = theirs.join(SSH).canonicalize() else {
        return every_copy;
    };
    let (readable, elsewhere) = what_ssh_holds(&ssh, theirs);
    let readable: Vec<String> = readable
        .iter()
        .map(|place| format!("  (literal {})", quoted(place)))
        .collect();
    let mut kept = format!(
        "{every_copy}\n(deny file-read-data (subpath {}))\n(allow file-read-data\n{})",
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

/// What the person's `.ssh` holds, from its real path: what may be read there
/// (the folder, its folders of includes, and every file in them that is not a
/// key), and the keys kept somewhere else that it links to or names.
fn what_ssh_holds(ssh: &Path, theirs: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut readable = vec![ssh.to_path_buf()];
    let mut elsewhere: Vec<PathBuf> = Vec::new();
    let mut folders = vec![(ssh.to_path_buf(), 0)];
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
                if real.starts_with(ssh) && deep < 2 {
                    readable.push(real.clone());
                    folders.push((real, deep + 1));
                }
            } else if a_private_key(&real) {
                if !real.starts_with(ssh) {
                    elsewhere.push(real);
                }
            } else if real.starts_with(ssh) {
                readable.push(real);
            }
        }
    }
    for named in keys_the_config_names(ssh, theirs) {
        if !named.starts_with(ssh) && !elsewhere.contains(&named) {
            elsewhere.push(named);
        }
    }
    (readable, elsewhere)
}

/// The person's keys kept outside `.ssh`, which `.ssh` links to or their SSH
/// configuration names, by their real paths. For the file tools, which the
/// wall does not stand in front of.
pub fn keys_kept_elsewhere() -> Vec<PathBuf> {
    let Some(theirs) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    let Ok(ssh) = theirs.join(SSH).canonicalize() else {
        return Vec::new();
    };
    what_ssh_holds(&ssh, &theirs).1
}

/// Whether a file is a private key, by how every kind of one begins.
///
/// Anything that cannot be read to find out is counted as one, because the
/// cost of that mistake is a file an errand cannot read, and the cost of the
/// other is a key it can.
pub(crate) fn a_private_key(path: &Path) -> bool {
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

/// The machines the person's SSH config names, by the names it gives them.
///
/// Only the names: never an address, a user or a key. Patterns (`*`, `?`,
/// `!`) name no one machine and are left out.
pub fn machines_in_ssh_config(theirs: &Path) -> Vec<String> {
    let Ok(config) = std::fs::read_to_string(theirs.join(SSH).join("config")) else {
        return Vec::new();
    };
    let mut named: Vec<String> = Vec::new();
    for line in config.lines() {
        let line = line.trim();
        let Some((key, value)) = line.split_once(|c: char| c.is_whitespace() || c == '=') else {
            continue;
        };
        if !key.eq_ignore_ascii_case("Host") {
            continue;
        }
        for name in value.trim_start_matches('=').split_whitespace() {
            let name = name.trim_matches('"');
            if !name.is_empty()
                && !name.contains(['*', '?', '!'])
                && !named.iter().any(|n| n == name)
            {
                named.push(name.to_string());
            }
        }
    }
    named
}

/// The keys the SSH config says to use, wherever they are kept.
pub(crate) fn keys_the_config_names(ssh: &Path, theirs: &Path) -> Vec<PathBuf> {
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

/// Nothing inside the wall may ask macOS to open anything: an app, a file, a
/// folder or a page, nor start a job of its own with launchd.
///
/// macOS opens things in a process of its own, outside every sandbox, so
/// `open` was a door straight through the wall: a teammate could build an app
/// in its own folder, open it, and have it write anywhere and add itself to
/// what starts at login. Tried on 4 October 2026 with a test app that only
/// wrote a file, from inside this very profile, and it wrote it. `lsopen` is
/// Apple's own name for asking LaunchServices to open something, so this
/// stops `open` and NSWorkspace alike rather than one program by name, and
/// AppleScript's way of starting an app that is not running. It does not stop
/// AppleScript talking to an app that is already running, and `launchctl` can
/// still look; it only cannot start a job. A program the teammate built still
/// runs when it runs it directly: inside the wall, which is the point.
///
/// Only in the full wall. A teammate on ask or edits is not walled for writing,
/// so it could leave a login item for next time whatever this said, and there
/// it would only break what the person approves card by card.
///
/// What it does instead is ask, with `open_outside`: the person sees a card
/// and their click is what opens it, done by the app.
pub const NOTHING_IS_OPENED_FROM_INSIDE: &str = "\n(deny lsopen)\n(deny job-creation)";

/// Whether an address opens the System Settings pane that would let Errand
/// drive or see the screen: Accessibility, Screen Recording, Input Monitoring,
/// or posting input.
///
/// Every teammate runs as Errand. Today the app holds none of these, so a
/// teammate's click or screenshot fails, and that is the real gate. A teammate
/// can still ask the person to grant one, on a card or in a link, so those are
/// never opened for it. Compared on letters only, lower case and with
/// percent-escapes undone, so neither case, punctuation nor an escape hides
/// one.
pub fn gives_the_screen(address: &str) -> bool {
    let letters = letters_of(address);
    letters.contains("systempreferences")
        && [
            "accessibility",
            "screencapture",
            "screenrecording",
            "listenevent",
            "postevent",
            "inputmonitoring",
        ]
        .iter()
        .any(|pane| letters.contains(pane))
}

/// An address as its letters and digits alone, lower case, with
/// percent-escapes undone and letters folded the way the disk and Settings
/// fold them (a long s is an s), so no spelling hides what it names.
fn letters_of(address: &str) -> String {
    let bytes = address.as_bytes();
    let hex = |b: u8| (b as char).to_digit(16);
    let mut undone: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                undone.push((high * 16 + low) as u8);
                i += 3;
                continue;
            }
        }
        undone.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&undone)
        .chars()
        .flat_map(char::to_uppercase)
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

/// The panes of System Settings a teammate may send the person to: the ones
/// an errand really needs granted to the app, and nothing that gives the
/// screen. Listed rather than refused by kind, because a list of refusals has
/// to know every pane there is, and the top of Privacy & Security is one click
/// from all of them.
const PANES_A_TEAMMATE_MAY_OPEN: &[&str] = &[
    "privacyautomation",
    "privacyallfiles",
    "privacymicrophone",
    "privacycalendars",
    "privacycontacts",
    "privacyreminders",
    "privacyphotos",
    "notificationssettings",
];

/// Whether a teammate may have this Settings pane opened for the person.
pub fn a_pane_a_teammate_may_open(address: &str) -> bool {
    let letters = letters_of(address);
    letters.starts_with("xapplesystempreferences")
        && !gives_the_screen(address)
        && PANES_A_TEAMMATE_MAY_OPEN
            .iter()
            .any(|pane| letters.contains(pane))
}

/// Whether what a command printed is the wall refusing to open something.
///
/// The system says only `_LSOpenURLsWithCompletionHandler() failed with error
/// -54`, which tells a model nothing it can act on.
pub fn an_opening_was_refused(said: &str) -> bool {
    said.contains("_LSOpenURLsWithCompletionHandler()")
        || said.contains("LSOpenURLsWithRole() failed")
}

/// What to say when something could not be opened from inside the wall.
pub fn nothing_opens_from_inside() -> &'static str {
    "That was the wall: nothing inside it can open an app, a file, a folder or a page. \
     To have something you made opened, call open_outside with its path and why, and the \
     person decides. For a web page they should see, use over_to_you with `where`. Never \
     look for another way round it."
}

/// The places in Errand's own folder no teammate reaches by any means, each
/// with whether it is a whole folder or one file, given Errand's real path.
///
/// The wall keeps commands out of them. The file tools run in the app,
/// outside the wall, and ask the same list through `kept_from_teammates`, so
/// a folder somebody allowed that happened to hold Errand's would not hand
/// them over there either.
fn never_reached(errand: &Path) -> Vec<(PathBuf, bool)> {
    let mut never = vec![
        (errand.join("keys"), true),
        // What a teammate asked to have opened outside its wall, copied there
        // so that what runs is what the person agreed to: changed from inside
        // a wall, it would be a way back out of it.
        (errand.join("opened"), true),
    ];
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
    if let Ok(beside) = std::fs::read_dir(errand) {
        for one in beside.flatten() {
            let copy = one.path();
            if one.file_name().to_string_lossy().contains(".db") && !stores.contains(&copy) {
                stores.push(copy);
            }
        }
    }
    never.extend(stores.into_iter().map(|store| (store, false)));
    // Every teammate's home: what it remembers and how it is told to work,
    // written out for the person. No teammate writes any, and none reads
    // another's. Its own it may read, which the wall allows after this.
    never.push((errand.join(crate::home::HOMES), true));
    never.push((errand.join(crate::home::INDEX), true));
    never
}

/// Whether a path is a private key by where it is or what it is called: in a
/// folder called `.ssh`, or named the way `ssh-keygen` names one. The rule
/// `KEYS_ANYWHERE` gives the wall, by path alone and folded the way the disk
/// folds names, for the file tools, which run outside the wall. Never by
/// what is in the file: a file about to be written has nothing in it yet.
pub fn a_key_by_its_name(at: &Path) -> bool {
    let fold = |s: &str| {
        s.chars()
            .flat_map(char::to_uppercase)
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    if at
        .components()
        .any(|c| fold(&c.as_os_str().to_string_lossy()) == SSH)
    {
        return true;
    }
    let name = fold(&at.file_name().unwrap_or_default().to_string_lossy());
    let base = name.strip_suffix("_sk").unwrap_or(&name);
    matches!(base, "id_rsa" | "id_dsa" | "id_ecdsa" | "id_ed25519")
}

/// The same, and the keys kept outside `.ssh` that `.ssh` links to or the
/// person's SSH configuration names, which the wall keeps from commands as
/// well.
pub fn a_key_kept_from_teammates(at: &Path) -> bool {
    a_key_by_its_name(at) || keys_kept_elsewhere().iter().any(|key| key == at)
}

/// Whether a path, by its real path, is one of the places in Errand's own
/// folder no teammate reaches. For the file tools, which the wall does not
/// stand in front of.
pub fn kept_from_teammates(at: &Path) -> bool {
    crate::where_errand_lives().is_some_and(|errand| kept_in(&errand, at))
}

/// Whether a path is one of those places in this Errand folder. Compared
/// the way the disk compares names, not only by case: `ERRAND.DB` is the
/// store, and `keyſ`, with a long s, is the keys folder.
fn kept_in(errand: &Path, at: &Path) -> bool {
    let errand = errand
        .canonicalize()
        .unwrap_or_else(|_| errand.to_path_buf());
    let folded = |p: &Path| {
        p.to_string_lossy()
            .chars()
            .flat_map(char::to_uppercase)
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    let at = folded(at);
    never_reached(&errand).iter().any(|(place, whole)| {
        let place = folded(place);
        at == place || (*whole && at.starts_with(&format!("{place}/")))
    })
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
fn keep_out(errand: &Path, inside: Inside, working_in: Option<&Path>) -> String {
    // The real path, because the sandbox compares real paths: a place named
    // through a link would be a rule that matches nothing.
    let errand = errand
        .canonicalize()
        .unwrap_or_else(|_| errand.to_path_buf());
    let never: Vec<String> = never_reached(&errand)
        .iter()
        .map(|(place, whole)| match whole {
            true => format!("  (subpath {})", quoted(place)),
            false => format!("  (literal {})", quoted(place)),
        })
        .collect();
    let mut kept = format!("\n(deny file-read* file-write*\n{})", never.join("\n"));
    if let Some(own) = whose_home(&errand, working_in) {
        kept.push_str(&format!(
            "\n(allow file-read* (subpath {}))",
            quoted(&crate::home::of(&errand, &own))
        ));
    }
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

/// Which teammate a working folder belongs to: the one whose folder it is
/// under Errand's threads, by its name there.
fn whose_home(errand: &Path, working_in: Option<&Path>) -> Option<String> {
    let real = working_in?.canonicalize().ok()?;
    let threads = errand.join("threads");
    (real.parent()? == threads)
        .then(|| real.file_name().map(|n| n.to_string_lossy().to_string()))
        .flatten()
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
    without_what_is_harmless(said)
        .to_ascii_lowercase()
        .contains("operation not permitted")
}

/// What a command printed, without the refusals that change nothing.
///
/// The system's `git`, `python3` and `clang` are found through a cache of
/// where the developer tools are, kept in the system's temporary folder, and
/// the wall keeps that cache from being rewritten. Each of them says so, as
/// "couldn't create cache file ... Operation not permitted", and then works.
/// Read as the wall stopping the command, it sent a model looking for a
/// folder to have allowed.
fn without_what_is_harmless(said: &str) -> String {
    said.lines()
        .filter(|line| {
            let line = line.to_ascii_lowercase();
            !(line.contains("couldn't create cache file") && line.contains("xcrun_db"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether what a command printed is SSH refused one of the person's private
/// keys, which the wall keeps from every errand on purpose.
///
/// `ssh` asks the key agent first and reads a key file only for a key the
/// agent does not hold, so a refusal to read one means exactly that: the key
/// it needed is not loaded. It says so as `Load key "<path>": Operation not
/// permitted`, and `ssh-add` handed a key says `<path>: Operation not
/// permitted`. Kept apart from every other refusal, because the advice for
/// those, allowing a folder under Allowed, can never fix this one, and was
/// exactly what a teammate refused its key was given.
pub fn a_key_was_kept_out(said: &str) -> bool {
    said.contains(A_KEY_NOT_LOADED)
        || without_what_is_harmless(said).lines().any(|line| {
            let line = line.to_ascii_lowercase();
            line.contains("operation not permitted")
                && (line.contains("load key")
                    || line.contains("loading key")
                    || line.contains("/id_")
                    || (line.contains("/.ssh/")
                        && !line.contains("known_hosts")
                        && !line.contains("/config")))
        })
}

/// The line a step is shown with when SSH was refused a key, in place of
/// the first line of what it printed, which says only how it exited: "exited
/// 255" under a step is where somebody looks first, and the app puts up the
/// button that loads the key from this line.
pub const A_KEY_NOT_LOADED: &str = "SSH was refused your key: it isn't loaded in the key agent";

/// The line a step is shown with: the first line of what it printed, made
/// short by `shorten`, unless what it printed is SSH refused a key.
pub fn the_line_for_a_step(said: &str, shorten: impl Fn(&str) -> String) -> String {
    match a_key_was_kept_out(said) {
        true => A_KEY_NOT_LOADED.to_string(),
        false => shorten(said),
    }
}

/// What to say instead, when it was a key.
pub fn the_key_is_not_loaded() -> String {
    "That \"Operation not permitted\" is SSH being refused the person's private key, and \
     that is on purpose: no errand can read a private key, and nothing under Allowed changes \
     that. SSH works without reading it, through the key agent, which uses the key for you \
     and never hands it over; the key this needed is not loaded in the agent. Tell the person \
     exactly that, and that Errand loads it for them: Errand is showing them a Load my SSH key \
     button, and the same button is under Settings, SSH key. Once it is loaded, the same \
     command works. Never copy a key or look for another way to read one."
        .to_string()
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
         directory, {}, and your own temporary folder, which TMPDIR names.{more} \
         Everywhere else on this \
         Mac is read-only to you, whatever permissions the app has: a write there fails \
         with \"Operation not permitted\", and that is the wall, never a macOS setting. \
         Full Disk Access does not change it, so never send the person to System \
         Settings for it. If an errand needs a file somewhere else, say so plainly: the \
         person can allow that folder for you under Allowed, choosing \"a folder\", and \
         then it works. Until then, do the work inside your own folder.\n\n\
         Private keys are not readable here: the SSH keys in ~/.ssh are kept from you, \
         and SSH works only with a key that is loaded in the person's key agent. If ssh \
         says it cannot load a key (Load key ...: Operation not permitted), the key is not \
         loaded: stop and tell the person so, and that Errand loads it for them with Load \
         my SSH key, under Settings, SSH key. Allowed has nothing to do with keys, so never \
         send them there for it. If a host is not yet in known_hosts, stop and say that \
         plainly too. Never copy a key, an SSH config or known_hosts somewhere else, and \
         never look for another way round the wall: the wall is the person's decision, \
         and working round it is the one thing that is never the errand.\n\n\
         Nothing inside the wall can open anything: an app, a file, a folder or a page. \
         `open` and NSWorkspace fail here, and nothing can start a launchd job. To have something you made \
         opened, an app you built or a report, call open_outside with its path and why: \
         the person sees a card and decides, and an app can also be started at every \
         login if they agree. A bare program or script has to go in a .app bundle first. \
         For a web page they should see, use over_to_you with `where`.\n\n\
         At the top of your folder, and of every other folder you may write in, .claude, \
         .mcp.json, CLAUDE.md and CLAUDE.local.md are \
         not yours to write: they would be the settings an engine reads, and you change what \
         you are only through suggest_learning. Clone or unpack a project into a subfolder, \
         never into your folder itself. Other programs' local sockets are closed to you \
         (a terminal multiplexer, a container daemon): the web, names and the SSH key agent \
         still work, and so does a socket of your own inside your folder.\n\n\
         Your tools keep their caches in your folder, under .cache, and their temporary \
         files in the folder TMPDIR names; the person's own (~/.npm, ~/.cargo, ~/.cache, \
         ~/Library/Caches, /tmp) are read-only to you, so the first download of a package \
         is yours to make again. Install packages locally (npm install, npx, a venv with \
         python3 -m venv .venv, cargo): global installs (npm -g, pip --user, rustup \
         toolchains, playwright install) cannot work here, and npm's advice to use sudo or \
         chown never applies. Write temporary files in $TMPDIR or your folder, never in \
         /tmp. To build a Swift package use swift build --disable-sandbox --cache-path \
         .cache/swiftpm --config-path .cache/swiftpm-config --security-path \
         .cache/swiftpm-security; with xcodebuild, pass -derivedDataPath .build/xcode. For \
         plots use MPLBACKEND=Agg. The system's git, python3 and clang may print \
         \"couldn't create cache file ... xcrun_db ... Operation not permitted\": that line \
         is harmless and the command still works.",
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
    // Every tool's caches in the errand's own folder, and its temporary files
    // in a folder of its own, which are the places inside the wall they can
    // be written. Made here, outside it.
    make_its_own_temporary_folder(&its_own_temporary_folder(home));
    for (name, place) in kept_in_its_own_folder(home) {
        if name == "XDG_CACHE_HOME" {
            std::fs::create_dir_all(&place).ok();
        }
        walled.env(name, place);
    }
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
pub fn kept_out(
    program: &str,
    doorway: Option<&Path>,
    working_in: &Path,
) -> tokio::process::Command {
    match (possible(), crate::where_errand_lives()) {
        (true, Some(errand)) => {
            let theirs = std::env::var("HOME").ok().map(PathBuf::from);
            let mut kept = tokio::process::Command::new(THE_SANDBOX);
            kept.arg("-p")
                .arg(only_kept_out(
                    &errand,
                    doorway,
                    theirs.as_deref(),
                    working_in,
                ))
                .arg(program);
            kept
        }
        _ => tokio::process::Command::new(program),
    }
}

/// The profile for that: everything allowed, and then the app's own things,
/// the person's private keys, and the agent's own settings not.
fn only_kept_out(
    errand: &Path,
    doorway: Option<&Path>,
    theirs: Option<&Path>,
    working_in: &Path,
) -> String {
    format!(
        "(version 1)\n(allow default){}{}{}",
        its_own_settings_kept(working_in),
        theirs.map(keys_kept_out).unwrap_or_default(),
        keep_out(errand, Inside::ClaudeCode { doorway }, Some(working_in)),
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
         walled in instead: it can write inside {} and its own temporary folder, \
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
         this agent runs without being asked, so it can write only inside {} and its \
         own temporary folder, whatever access the app has been granted. Full Disk \
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

    /// Not a test: writes the profile and environment this module would use,
    /// for a probe to run things under exactly them. Run by hand with
    /// ERRAND_PROBE_HOME (the errand's folder), ERRAND_PROBE_INSIDE (command or
    /// claude), ERRAND_PROBE_THEIRS (the home folder to keep out, which may be
    /// a stand-in) and ERRAND_PROBE_OUT (where to write).
    #[test]
    #[ignore = "a tool for probes, run by hand"]
    fn write_the_profile_for_a_probe() {
        let home = PathBuf::from(std::env::var("ERRAND_PROBE_HOME").expect("ERRAND_PROBE_HOME"));
        let theirs =
            PathBuf::from(std::env::var("ERRAND_PROBE_THEIRS").expect("ERRAND_PROBE_THEIRS"));
        let out = PathBuf::from(std::env::var("ERRAND_PROBE_OUT").expect("ERRAND_PROBE_OUT"));
        let inside = match std::env::var("ERRAND_PROBE_INSIDE").as_deref() {
            Ok("claude") => Inside::ClaudeCode { doorway: None },
            _ => Inside::ACommand,
        };
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(
            out.join("profile.sb"),
            profile_keeping_out(&home, inside, None, Some(&theirs)),
        )
        .unwrap();
        let env: Vec<String> = kept_in_its_own_folder(&home)
            .into_iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        std::fs::write(out.join("env.txt"), env.join("\n")).unwrap();
    }

    #[test]
    fn a_walled_command_is_given_those_folders_and_its_own_temporary_one() {
        let home = std::env::temp_dir().join(format!("errand-env-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        let walled = around("/bin/sh", &home, Inside::ACommand);
        let set: HashMap<String, String> = walled
            .as_std()
            .get_envs()
            .filter_map(|(k, v)| {
                Some((
                    k.to_string_lossy().to_string(),
                    v?.to_string_lossy().to_string(),
                ))
            })
            .collect();
        let temporary = its_own_temporary_folder(&home);
        assert_eq!(
            set.get("TMPDIR"),
            Some(&format!("{}/", temporary.display()))
        );
        assert_eq!(
            set.get("TMPPREFIX"),
            Some(&format!("{}/zsh", temporary.display()))
        );
        assert!(set
            .get("CARGO_HOME")
            .is_some_and(|c| c.contains("/.cache/cargo")));
        assert_eq!(
            set.get("CLAUDE_CODE_DISABLE_AUTO_MEMORY")
                .map(String::as_str),
            Some("1")
        );
        // Made, as a real folder, with a name short enough for a socket in it.
        assert!(std::fs::symlink_metadata(&temporary).is_ok_and(|m| m.is_dir()));
        assert!(
            temporary.to_string_lossy().len() < 40,
            "{}",
            temporary.display()
        );
        std::fs::remove_dir_all(&home).ok();
        std::fs::remove_dir(&temporary).ok();
    }

    #[test]
    fn each_tool_keeps_its_caches_in_the_errands_own_folder() {
        // The person's own tool folders are run from later, outside the wall,
        // so each tool is pointed at one of the errand's own instead.
        let home = Path::new("/tmp/an-errand");
        let set: HashMap<String, String> = kept_in_its_own_folder(home).into_iter().collect();
        for name in [
            "npm_config_cache",
            "CARGO_HOME",
            "PIP_CACHE_DIR",
            "UV_CACHE_DIR",
            "GOMODCACHE",
            "CLANG_MODULE_CACHE_PATH",
        ] {
            let at = set.get(name).unwrap_or_else(|| panic!("{name} is not set"));
            assert!(at.starts_with("/tmp/an-errand/"), "{name} = {at}");
        }
        assert_eq!(set.get("GOFLAGS").map(String::as_str), Some("-modcacherw"));
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
                .arg(only_kept_out(
                    &errand,
                    None,
                    std::env::var("HOME").ok().map(PathBuf::from).as_deref(),
                    &errand.join("agent"),
                ))
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
    fn nothing_of_the_persons_but_claude_codes_record_of_this_folder_is_writable() {
        if !possible() {
            return;
        }
        // A home folder of its own, laid out like the person's, so a rule that
        // failed could only ever write here.
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("errand-their-folders-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let theirs = root.join("person");
        let home = theirs.join("Library/Application Support/Errand/threads/agent-1");
        std::fs::create_dir_all(&home).unwrap();
        let own = claude_codes_folder_for(&theirs, &home);
        std::fs::create_dir_all(&own).unwrap();
        let elsewhere = theirs.join(".claude/projects/-Users-someone-else");
        std::fs::create_dir_all(&elsewhere).unwrap();
        for place in [
            ".cargo/bin",
            ".npm/_npx",
            ".cache/uv",
            "Library/Caches/ms-playwright",
            ".claude/skills",
        ] {
            std::fs::create_dir_all(theirs.join(place)).unwrap();
        }
        std::fs::write(theirs.join(".claude.json"), "{}").unwrap();
        std::fs::write(theirs.join(".claude/settings.json"), "{}").unwrap();

        let run = |inside: Inside, command: &str| {
            std::process::Command::new(THE_SANDBOX)
                .arg("-p")
                .arg(profile_keeping_out(&home, inside, None, Some(&theirs)))
                .arg("/bin/sh")
                .arg("-c")
                .arg(command)
                .output()
                .expect("sandbox-exec runs")
                .status
                .success()
        };
        let claude = Inside::ClaudeCode { doorway: None };
        for (inside, kind) in [(Inside::ACommand, "a command"), (claude, "Claude Code")] {
            for refused in [
                ".cargo/bin/git",
                ".npm/_npx/planted.js",
                ".cache/uv/planted",
                "Library/Caches/ms-playwright/planted",
                ".claude/skills/planted.md",
                ".claude/settings.json",
                ".claude.json",
                ".claude/projects/-Users-someone-else/planted.jsonl",
            ] {
                let at = theirs.join(refused);
                assert!(
                    !run(inside, &format!("echo x >> {}", quoted(&at))),
                    "{kind} wrote {refused}"
                );
            }
            // Nor the shared temporary folders, where the person's own things sit.
            assert!(
                !run(inside, "echo x > /private/tmp/errand-wall-tmp-probe"),
                "{kind} wrote /private/tmp"
            );
        }
        assert!(!Path::new("/private/tmp/errand-wall-tmp-probe").exists());
        // Claude Code's record of this very folder, and only Claude Code's,
        // without the memory beside it that later sessions would read.
        let record = own.join("a-session.jsonl");
        assert!(
            run(claude, &format!("echo x >> {}", quoted(&record))),
            "Claude Code could not keep its record"
        );
        assert!(
            !run(
                claude,
                &format!(
                    "mkdir -p {} && echo x > {}",
                    quoted(&own.join("memory")),
                    quoted(&own.join("memory/MEMORY.md"))
                )
            ),
            "Claude Code could write its own memory"
        );
        assert!(!run(
            Inside::ACommand,
            &format!("echo x >> {}", quoted(&own.join("b.jsonl")))
        ));
        // And the system's temporary folder only for what Swift makes there.
        if let Some(temporary) = the_systems_temporary_folder() {
            let made = temporary.join(format!("TemporaryDirectory.errand{}", std::process::id()));
            assert!(run(
                Inside::ACommand,
                &format!("mkdir {} && rmdir {}", quoted(&made), quoted(&made))
            ));
            assert!(!run(
                Inside::ACommand,
                &format!("echo x >> {}", quoted(&temporary.join("xcrun_db")))
            ));
        }
        std::fs::remove_dir_all(&root).ok();
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
                .arg(profile_keeping_out(
                    &home,
                    inside,
                    Some(&errand),
                    std::env::var("HOME").ok().map(PathBuf::from).as_deref(),
                ))
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
    fn the_keys_ssh_keeps_elsewhere_are_the_ones_it_links_to_or_names() {
        let theirs = std::env::temp_dir().join(format!("errand-elsewhere-{}", std::process::id()));
        std::fs::remove_dir_all(&theirs).ok();
        let (ssh, team) = (theirs.join(".ssh"), theirs.join("teams/t1"));
        std::fs::create_dir_all(&ssh).unwrap();
        std::fs::create_dir_all(&team).unwrap();
        // Built here, so the secret scan has no key-shaped text to find.
        let begins = format!("-----BEGIN OPENSSH {} KEY-----", "PRIVATE");
        std::fs::write(team.join("github_deploy"), &begins).unwrap();
        std::fs::write(team.join("linked_key"), &begins).unwrap();
        std::os::unix::fs::symlink(team.join("linked_key"), ssh.join("work")).unwrap();
        std::fs::write(
            ssh.join("config"),
            format!(
                "Host example\n  IdentityFile {}\n",
                team.join("github_deploy").display()
            ),
        )
        .unwrap();
        let ssh = ssh.canonicalize().unwrap();
        let (_, elsewhere) = what_ssh_holds(&ssh, &theirs);
        let team = team.canonicalize().unwrap();
        assert!(
            elsewhere.contains(&team.join("github_deploy")),
            "{elsewhere:?}"
        );
        assert!(
            elsewhere.contains(&team.join("linked_key")),
            "{elsewhere:?}"
        );
        std::fs::remove_dir_all(&theirs).ok();
    }

    #[test]
    fn a_key_is_known_by_its_folder_or_its_name_the_way_the_wall_knows_it() {
        for key in [
            "/x/teams/t1/deploy/id_ed25519",
            "/x/teams/t1/id_rsa_sk",
            "/x/teams/t1/ID_ECDSA",
            "/x/teams/t1/.ssh/config",
            "/x/teams/t1/a/.SSH/anything",
        ] {
            assert!(
                a_key_by_its_name(Path::new(key)),
                "{key} was not taken for a key"
            );
        }
        for not_a_key in [
            "/x/teams/t1/id_ed25519.pub",
            "/x/teams/t1/ssh-notes.md",
            "/x/teams/t1/convert.py",
        ] {
            assert!(
                !a_key_by_its_name(Path::new(not_a_key)),
                "{not_a_key} was taken for a key"
            );
        }
    }

    #[test]
    fn the_file_tools_are_kept_from_the_same_places_as_commands() {
        let errand = std::env::temp_dir().join(format!("errand-kept-{}", std::process::id()));
        std::fs::remove_dir_all(&errand).ok();
        std::fs::create_dir_all(errand.join("keys")).unwrap();
        std::fs::create_dir_all(errand.join("teams/t1")).unwrap();
        std::fs::write(errand.join("errand.db"), "").unwrap();
        std::fs::write(errand.join("errand-before-clearing.db"), "").unwrap();
        let real = errand.canonicalize().unwrap();
        for kept in [
            "keys/anthropic",
            "KEYS/anthropic",
            "key\u{17f}/anthropic",
            "errand.db",
            "ERRAND.DB",
            "errand.db-wal",
            "errand-before-clearing.db",
            "opened/App.app",
            crate::home::HOMES,
        ] {
            assert!(kept_in(&errand, &real.join(kept)), "{kept} was not kept");
        }
        for open in ["teams/t1/part.md", "keysake/notes.txt", "errand.dbx/notes"] {
            assert!(!kept_in(&errand, &real.join(open)), "{open} was kept");
        }
        std::fs::remove_dir_all(&errand).ok();
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
    fn the_machines_the_ssh_config_names_are_known_by_name_and_nothing_else() {
        let theirs = std::env::temp_dir().join(format!("errand-machines-{}", std::process::id()));
        std::fs::create_dir_all(theirs.join(".ssh")).unwrap();
        std::fs::write(
            theirs.join(".ssh").join("config"),
            "Host studio archive\n  HostName 192.0.2.7\n  User me\n  IdentityFile ~/.ssh/id_test\n\
             Host *.internal !secret\n  User nobody\nhost=pi\nHost *\n  ServerAliveInterval 30\n",
        )
        .unwrap();
        assert_eq!(
            machines_in_ssh_config(&theirs),
            vec!["studio", "archive", "pi"]
        );
        assert!(machines_in_ssh_config(&theirs.join("nowhere")).is_empty());
        std::fs::remove_dir_all(&theirs).ok();
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
        // The copies nobody thinks of: a backup of the whole home folder, the
        // way Time Machine keeps one, and a key left in Downloads.
        let backed_up = theirs
            .join("Backups")
            .join("2026-09-30-181132.previous")
            .join("Data")
            .join("Users")
            .join("me")
            .join(".ssh");
        std::fs::create_dir_all(&backed_up).unwrap();
        std::fs::copy(&key, backed_up.join("id_test")).unwrap();
        std::fs::copy(ssh.join("config"), backed_up.join("config")).unwrap();
        std::fs::create_dir_all(theirs.join("Downloads")).unwrap();
        std::fs::copy(&key, theirs.join("Downloads").join("id_ed25519")).unwrap();

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
            &backed_up.join("id_test"),
            &backed_up.join("config"),
            &theirs.join("Downloads").join("id_ed25519"),
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

    /// A walled shell under the full wall, on throwaway places only: no
    /// errand folder of the real app and no home folder of the person's.
    fn walled(home: &Path, command: &str) -> (bool, String) {
        let out = std::process::Command::new(THE_SANDBOX)
            .arg("-p")
            .arg(profile_keeping_out(home, Inside::ACommand, None, None))
            .arg("/bin/sh")
            .arg("-c")
            .arg(command)
            .current_dir(home)
            .output()
            .expect("sandbox-exec runs");
        (
            out.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        )
    }

    #[tokio::test]
    async fn a_here_document_works_in_zsh_inside_the_wall() {
        if !possible() {
            return;
        }
        let home = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("errand-heredoc-wall-{}", std::process::id()));
        std::fs::remove_dir_all(&home).ok();
        std::fs::create_dir_all(&home).unwrap();
        // How Claude Code's commands write a file, through the person's zsh.
        let out = shell(&home, "/bin/zsh -fc 'cat <<END\nhere\nEND'")
            .output()
            .await
            .expect("it runs");
        assert!(
            out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "here",
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn a_team_folder_is_written_by_its_members_only_and_kept_whole() {
        if !possible() {
            return;
        }
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("errand-team-wall-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let member = root.join("member");
        let other = root.join("other");
        let team = root.join("teams").join("t1");
        for place in [&member, &other, &team] {
            std::fs::create_dir_all(place).unwrap();
        }
        also_allow(&member, vec![team.clone()]);
        let at = |name: &str| quoted(&team.join(name));
        // A member writes there, a folder down as well, and keeps a repository.
        let (ok, said) = walled(
            &member,
            &format!(
                "echo hi > {} && mkdir -p {} && echo x > {} && git init -q {}",
                at("plan.md"),
                at("src"),
                at("src/a.txt"),
                at("repo")
            ),
        );
        assert!(ok, "a member could not write the team's folder: {said}");
        // Somebody not on the team cannot.
        let (ok, said) = walled(&other, &format!("echo hi > {}", at("theirs.md")));
        assert!(!ok, "a teammate not on the team wrote there: {said}");
        assert!(!team.join("theirs.md").exists());
        // No member plants an engine's settings at its top, or moves it away.
        for attempt in [
            format!(
                "mkdir -p {} && echo '{{}}' > {}",
                at(".claude"),
                at(".claude/settings.json")
            ),
            format!("echo x > {}", at("CLAUDE.md")),
            format!("echo '{{}}' > {}", at(".mcp.json")),
            format!("mv {} {}", quoted(&team), quoted(&member.join("away"))),
        ] {
            let (ok, said) = walled(&member, &attempt);
            assert!(!ok, "{attempt} went through: {said}");
        }
        assert!(team.join("plan.md").exists() && !member.join("away").exists());
        also_allow(&member, vec![]);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn its_own_folder_stays_where_it_is_and_its_settings_are_not_its_to_write() {
        if !possible() {
            return;
        }
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("errand-own-settings-wall-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let home = root.join("agent");
        std::fs::create_dir_all(home.join("project")).unwrap();
        // Ordinary work goes on.
        assert!(
            walled(
                &home,
                "echo hi > notes.txt && mkdir -p a/b && echo x > a/b/c && git init -q repo"
            )
            .0
        );
        assert!(
            walled(&home, "echo hi > project/CLAUDE.md").0,
            "a project's own, a folder down"
        );
        // Its settings, however they are spelt or reached, are not.
        for attempt in [
            "mkdir -p .claude && echo '{}' > .claude/settings.json",
            "echo x > CLAUDE.md",
            "echo x > claude.md",
            "echo x > CLAUDE.local.md",
            "echo '{}' > .mcp.json",
            "echo '{}' > .MCP.json",
            "echo '{}' > .mcp.j\u{17f}on",
            "mkdir side && echo '{}' > side/settings.json && mv side .claude",
            "ln -s /tmp/elsewhere .claude",
        ] {
            let (ok, said) = walled(&home, attempt);
            assert!(!ok, "{attempt} went through: {said}");
        }
        assert!(!home.join(".claude").exists() && !home.join(".mcp.json").exists());
        assert!(!home.join("CLAUDE.md").exists() && !home.join("claude.md").exists());
        // Nor can the folder itself be moved somewhere writable, edited there
        // and moved back, which is how a rule on a path inside it is undone.
        // Somewhere really writable, so the move fails only for the rule.
        if let Some(temporary) = the_systems_temporary_folder() {
            let away = temporary.join(format!(
                "TemporaryDirectory.errand-away-{}",
                std::process::id()
            ));
            let (ok, said) = walled(&home, &format!("mv {} {}", quoted(&home), quoted(&away)));
            assert!(!ok, "the folder was moved: {said}");
            assert!(home.exists() && !away.exists());
            let (ok, said) = walled(
                &home,
                &format!("mkdir -p {} && rmdir {}", quoted(&away), quoted(&away)),
            );
            assert!(
                ok,
                "the place moved to is not writable, so this proves nothing: {said}"
            );
        }
        // mktemp names its own files in the system's temporary folder,
        // whatever TMPDIR says, and is allowed there.
        let (ok, said) = walled(
            &home,
            "f=$(mktemp) && echo x > \"$f\" && rm \"$f\" && d=$(mktemp -d) && rmdir \"$d\"",
        );
        assert!(ok, "mktemp: {said}");
        let away = root.join("away");
        let (ok, said) = walled(&home, &format!("mv {} {}", home.display(), away.display()));
        assert!(!ok, "the folder was moved: {said}");
        assert!(home.exists() && !away.exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn no_other_server_s_socket_is_reachable_from_inside_and_its_own_is() {
        if !possible() {
            return;
        }
        use std::os::unix::net::UnixListener;
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("errand-sockets-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let home = root.join("agent");
        std::fs::create_dir_all(&home).unwrap();
        let listen = |at: PathBuf| {
            let listener = UnixListener::bind(&at).unwrap();
            std::thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    drop(stream);
                }
            });
            at
        };
        let theirs = listen(root.join("someone-elses.sock"));
        let its_own = listen(home.join("its-own.sock"));
        let reach = |at: &Path| {
            walled(
                &home,
                &format!("/usr/bin/nc -U {} < /dev/null", at.display()),
            )
        };
        let (ok, said) = reach(&theirs);
        assert!(!ok, "another server's socket was reached: {said}");
        let (ok, said) = reach(&its_own);
        assert!(ok, "its own socket was refused: {said}");
        // Names are still looked up, through the system's own socket.
        let (ok, said) = walled(&home, "/usr/bin/dscacheutil -q host -a name localhost");
        assert!(ok && said.contains("ip_address"), "names: {said}");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_teammate_reads_its_own_home_writes_none_and_reads_no_other() {
        if !possible() {
            return;
        }
        let errand = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("errand-homes-wall-{}", std::process::id()));
        std::fs::remove_dir_all(&errand).ok();
        let home = errand.join("threads").join("a1");
        std::fs::create_dir_all(&home).unwrap();
        for who in ["a1", "a2"] {
            let at = crate::home::of(&errand, who);
            std::fs::create_dir_all(&at).unwrap();
            std::fs::write(at.join("memory.md"), format!("{who}'s notes")).unwrap();
        }
        std::fs::create_dir_all(errand.join(crate::home::INDEX)).unwrap();
        std::os::unix::fs::symlink(
            crate::home::of(&errand, "a2"),
            errand.join(crate::home::INDEX).join("Other"),
        )
        .unwrap();
        let run = |inside: Inside, command: &str| {
            let out = std::process::Command::new(THE_SANDBOX)
                .arg("-p")
                .arg(profile_keeping_out(&home, inside, Some(&errand), None))
                .arg("/bin/sh")
                .arg("-c")
                .arg(command)
                .output()
                .expect("sandbox-exec runs");
            (
                out.status.success(),
                String::from_utf8_lossy(&out.stdout).to_string(),
            )
        };
        let own = crate::home::of(&errand, "a1").join("memory.md");
        let other = crate::home::of(&errand, "a2").join("memory.md");
        for inside in [Inside::ACommand, Inside::ClaudeCode { doorway: None }] {
            let (ok, said) = run(inside, &format!("cat {}", quoted(&own)));
            assert!(
                ok && said.contains("a1's notes"),
                "its own home was not readable: {said}"
            );
            assert!(
                !run(inside, &format!("echo x >> {}", quoted(&own))).0,
                "it wrote its own home"
            );
            assert!(
                !run(inside, &format!("cat {}", quoted(&other))).0,
                "it read another's home"
            );
            let through = errand
                .join(crate::home::INDEX)
                .join("Other")
                .join("memory.md");
            assert!(
                !run(inside, &format!("cat {}", quoted(&through))).0,
                "it read another's home through its listing"
            );
        }
        // Nor where the person approves each step.
        let kept = only_kept_out(&errand, None, None, &home);
        let (wrote, _) = {
            let out = std::process::Command::new(THE_SANDBOX)
                .arg("-p")
                .arg(&kept)
                .arg("/bin/sh")
                .arg("-c")
                .arg(format!("echo x >> {}", quoted(&own)))
                .output()
                .unwrap();
            (out.status.success(), ())
        };
        assert!(!wrote, "an agent that asks wrote its home");
        std::fs::remove_dir_all(&errand).ok();
    }

    #[test]
    fn no_teammate_can_send_the_person_to_give_errand_the_screen() {
        for pane in [
            "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Accessibility",
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
            "X-APPLE.SYSTEMPREFERENCES:com.apple.preference.security?privacy_listenevent",
            "x-apple.systempreferences:com.apple.preference.security?Privacy%5FPostEvent",
            "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Screen-Capture",
        ] {
            assert!(gives_the_screen(pane), "{pane}");
        }
        for fine in [
            "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Automation",
            "x-apple.systempreferences:com.apple.Notifications-Settings.extension",
            "https://example.com/accessibility",
        ] {
            assert!(!gives_the_screen(fine), "{fine}");
        }
        // Spelt with a long s, and with an escape before a letter that is not
        // one byte long, which used to stop the check with a panic.
        assert!(gives_the_screen(
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Acce\u{17f}\u{17f}ibility"
        ));
        assert!(!gives_the_screen("x-apple.systempreferences:%\u{e9}"));
        // Only the panes on the list open for a teammate: never the top of
        // Privacy & Security, one click from the rest.
        assert!(a_pane_a_teammate_may_open(
            "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Automation"
        ));
        assert!(a_pane_a_teammate_may_open(
            "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=com.errandai.errand"
        ));
        for refused in [
            "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension",
            "x-apple.systempreferences:com.apple.preference.security",
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
            "x-apple.systempreferences:com.apple.preference.universalaccess",
            "https://example.com/?Privacy_Automation",
        ] {
            assert!(!a_pane_a_teammate_may_open(refused), "{refused}");
        }
    }

    #[test]
    fn nothing_inside_the_wall_may_open_anything_or_start_a_job() {
        let home = Path::new("/tmp/errand-wall-open");
        for inside in [Inside::ACommand, Inside::ClaudeCode { doorway: None }] {
            let profile = profile_keeping_out(home, inside, None, None);
            assert!(profile.contains("(deny lsopen)"), "{profile}");
            assert!(profile.contains("(deny job-creation)"), "{profile}");
        }
        let errand = std::env::temp_dir().join("errand-wall-open-kept");
        // Not where the person approves each command: there it would only
        // break `open -a Simulator` and the like, and close nothing.
        let kept = only_kept_out(&errand, None, None, home);
        assert!(!kept.contains("(deny lsopen)"), "{kept}");
        // And a model refused is told why, and what to do instead.
        assert!(an_opening_was_refused(
            "_LSOpenURLsWithCompletionHandler() failed with error -54 for the file /x/A.app."
        ));
        assert!(!an_opening_was_refused("exited 1"));
        assert!(nothing_opens_from_inside().contains("open_outside"));
        assert!(what_the_wall_means(home).contains("open_outside"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_app_built_inside_the_wall_cannot_be_opened_from_inside_it() {
        // The door this closes: macOS opens an app in a process of its own,
        // outside the sandbox, so an app a teammate built and opened wrote
        // wherever it liked. Run rather than read, with an app that only
        // writes a file outside the folder it was built in.
        if !possible() {
            return;
        }
        let place = std::env::temp_dir().join(format!("errand-wall-open-{}", std::process::id()));
        std::fs::remove_dir_all(&place).ok();
        let home = place.join("home");
        let probe = home.join("Probe.app").join("Contents");
        std::fs::create_dir_all(probe.join("MacOS")).expect("a bundle");
        let place = place.canonicalize().expect("a real path");
        let home = place.join("home");
        let marker = place.join("opened-outside");
        std::fs::write(
            home.join("Probe.app/Contents/Info.plist"),
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict>\
             <key>CFBundleExecutable</key><string>probe</string>\
             <key>CFBundleIdentifier</key><string>test.errand.wall.probe</string>\
             <key>LSBackgroundOnly</key><true/></dict></plist>",
        )
        .unwrap();
        let program = home.join("Probe.app/Contents/MacOS/probe");
        std::fs::write(
            &program,
            format!("#!/bin/sh\necho out > '{}'\n", marker.display()),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();

        let opened = shell(&home, "/usr/bin/open -g Probe.app")
            .output()
            .await
            .expect("it runs");
        std::thread::sleep(std::time::Duration::from_secs(3));
        assert!(
            !marker.exists(),
            "an app opened from inside the wall ran outside it: {}",
            String::from_utf8_lossy(&opened.stderr)
        );
        assert!(!opened.status.success(), "open said it opened it");
        std::fs::remove_dir_all(&place).ok();
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
        // A key refused is a key not loaded, and Allowed is not where that is
        // fixed. It was where a teammate refused its key sent somebody.
        assert!(said.contains("Load my SSH key"), "{said}");
        assert!(
            said.contains("Allowed has nothing to do with keys"),
            "{said}"
        );
        also_allow(home, vec![]);
    }

    #[test]
    fn a_key_refused_is_told_from_a_folder_refused() {
        // What ssh printed for a teammate asked to check another Mac's disk,
        // with the agent empty, and what ssh-add says handed a key.
        for refused in [
            "Load key \"/Users/me/.ssh/id_ed25519\": Operation not permitted\n\
             me@192.0.2.10: Permission denied (publickey).",
            "Load key \"/Users/me/.ssh/studio_key\": Operation not permitted",
            "/Users/me/.ssh/id_rsa: Operation not permitted",
        ] {
            assert!(a_key_was_kept_out(refused), "{refused}");
            assert!(looks_like_the_wall(refused), "{refused}");
        }
        // Every other refusal is the wall's ordinary kind, a folder, and is
        // answered the ordinary way.
        for not_a_key in [
            "cp: /Volumes/Disk/clip.mp4: Operation not permitted",
            "touch: /Users/me/.ssh/known_hosts: Operation not permitted",
            "me@192.0.2.10: Permission denied (publickey).",
            "Load key \"/Users/me/.ssh/id_ed25519\": incorrect passphrase",
        ] {
            assert!(!a_key_was_kept_out(not_a_key), "{not_a_key}");
        }
        // Under the step, and to the app, it is said as what it is rather
        // than as how ssh exited, which is all a first line would say.
        let printed = "exited 255\nLoad key \"/Users/me/.ssh/id_ed25519\": Operation not permitted";
        let first = |s: &str| s.lines().next().unwrap_or("").to_string();
        assert_eq!(the_line_for_a_step(printed, first), A_KEY_NOT_LOADED);
        assert!(a_key_was_kept_out(&the_line_for_a_step(printed, first)));
        assert_eq!(
            the_line_for_a_step("exited 1\nno such file", first),
            "exited 1"
        );
        let said = the_key_is_not_loaded();
        assert!(said.contains("not loaded"), "{said}");
        assert!(said.contains("Load my SSH key"), "{said}");
        assert!(said.contains("nothing under Allowed"), "{said}");
        assert!(!said.contains("choosing \"a folder\""), "{said}");
    }
}
