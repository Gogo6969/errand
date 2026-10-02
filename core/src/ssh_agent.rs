//! The person's SSH keys, which teammates use through the key agent and never
//! read.
//!
//! No errand can read a private key: the wall keeps every copy out, wherever
//! it is kept (`keys_kept_out` in `wall.rs`). SSH still works inside the wall
//! through the key agent, which signs for whoever asks and never hands a key
//! over, but only for a key that is in the agent, and on a Mac nothing puts it
//! there: the agent starts empty after every restart. What that looked like
//! was a teammate asked to check the disk on another Mac, refused the key, and
//! told by this app that the refusal was its wall and to allow a folder under
//! Allowed, where nothing could ever have fixed it.
//!
//! So Errand loads the key itself, from outside the wall, the way somebody
//! would with `ssh-add` in a terminal. A passphrase is asked for in a macOS
//! window that hands it straight to `ssh-add`, which keeps it in the person's
//! Keychain. It never passes through Errand: not the window, not a log, not
//! the store.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

/// By its whole path, so that a PATH somebody changed cannot put another
/// program in its place.
const SSH_ADD: &str = "/usr/bin/ssh-add";

/// What answers for the passphrase when nobody is to be asked: nothing, and
/// a refusal, so a key that needs one is left alone rather than waited on.
const DECLINES: &str = "/usr/bin/false";

/// The keys `ssh` tries by itself, in `~/.ssh`, by the names `ssh-keygen`
/// gives them.
const USUAL_NAMES: &[&str] = &[
    "id_ed25519",
    "id_ecdsa",
    "id_rsa",
    "id_ed25519_sk",
    "id_ecdsa_sk",
];

/// What the key agent holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holds {
    /// This many keys, which teammates can use.
    Keys(usize),
    /// None: there is an agent, with nothing in it.
    Nothing,
    /// There is no agent to ask.
    NoAgent,
}

/// Ask the agent, which says what it holds and never hands a key over.
pub fn what_the_agent_holds() -> Holds {
    asking_the_agent(None)
}

/// The same, of an agent named by its socket, or of the one this app was
/// started with.
fn asking_the_agent(socket: Option<&Path>) -> Holds {
    let mut list = Command::new(SSH_ADD);
    list.arg("-l").stdin(Stdio::null());
    if let Some(socket) = socket {
        list.env("SSH_AUTH_SOCK", socket);
    }
    match list.output() {
        Ok(out) => holds(out.status.code(), &String::from_utf8_lossy(&out.stdout)),
        Err(_) => Holds::NoAgent,
    }
}

/// The same, from what `ssh-add -l` said: 0 and a line for each key, 1 for an
/// agent with none, 2 for no agent at all.
fn holds(code: Option<i32>, listed: &str) -> Holds {
    match code {
        Some(0) => match listed.lines().filter(|l| !l.trim().is_empty()).count() {
            0 => Holds::Nothing,
            keys => Holds::Keys(keys),
        },
        Some(1) => Holds::Nothing,
        _ => Holds::NoAgent,
    }
}

/// The keys there are to load: the ones `ssh` tries by itself, where there
/// are any, and any the SSH config names. Nothing is read from them but how
/// each one begins, which is what says a file is a private key at all.
pub fn keys_to_load(theirs: &Path) -> Vec<PathBuf> {
    let ssh = theirs.join(".ssh");
    let usual = USUAL_NAMES.iter().map(|name| ssh.join(name));
    let mut keys: Vec<PathBuf> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();
    for key in usual.chain(crate::wall::keys_the_config_names(&ssh, theirs)) {
        let Ok(real) = key.canonicalize() else {
            continue;
        };
        if real.is_file() && crate::wall::a_private_key(&real) && !seen.contains(&real) {
            seen.push(real);
            keys.push(key);
        }
    }
    keys
}

/// How a passphrase is asked for, where a key has one.
pub enum Asking<'a> {
    /// In a macOS window. The program `ssh-add` runs to put it up is written
    /// for the purpose into `keys` in Errand's own folder, which no errand can
    /// read or write, so nothing a teammate did can be the thing that asks.
    InAWindow { errand: &'a Path },
    /// Never: only a key with no passphrase, or one whose passphrase is in the
    /// Keychain already, is loaded. For Errand starting, where a window nobody
    /// asked for is the wrong thing to put on screen.
    Never,
}

/// What came of loading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    /// The keys added, by the names of their files.
    pub added: Vec<String>,
    /// What the agent holds now.
    pub holds: Holds,
    /// Why nothing was added, in words for the person, when nothing was.
    pub why_not: Option<String>,
}

/// One at a time: a second press while the passphrase window is up would put
/// a second window behind the first.
static LOADING: AtomicBool = AtomicBool::new(false);

/// Load the person's keys into the agent, from outside the wall.
///
/// Blocks for as long as the passphrase window is up, so it belongs on a
/// thread of its own.
pub fn load(theirs: &Path, asking: Asking) -> Loaded {
    if LOADING.swap(true, Ordering::SeqCst) {
        return Loaded {
            added: Vec::new(),
            holds: what_the_agent_holds(),
            why_not: Some(ALREADY.to_string()),
        };
    }
    struct Over;
    impl Drop for Over {
        fn drop(&mut self) {
            LOADING.store(false, Ordering::SeqCst);
        }
    }
    let _over = Over;
    loading(theirs, asking, None, true)
}

/// The same, at an agent of the caller's choosing, and with the Keychain or
/// without it: a test loads a key of its own into an agent of its own, and
/// has no business in the person's Keychain.
fn loading(theirs: &Path, asking: Asking, socket: Option<&Path>, keychain: bool) -> Loaded {
    let before = asking_the_agent(socket);
    let nothing = |holds: Holds, why: String| Loaded {
        added: Vec::new(),
        holds,
        why_not: Some(why),
    };
    if before == Holds::NoAgent {
        return nothing(before, NO_AGENT.to_string());
    }
    let keys = keys_to_load(theirs);
    if keys.is_empty() {
        return nothing(before, NO_KEYS.to_string());
    }
    let mut files: Vec<OsString> = Vec::new();
    if keychain {
        files.push("--apple-use-keychain".into());
    }
    files.extend(keys.iter().map(|key| key.as_os_str().to_os_string()));
    let mut said = String::new();
    match asking {
        Asking::Never => {
            // Whatever the Keychain has a passphrase for, then whatever needs
            // none.
            if keychain {
                said.push_str(&ssh_add(
                    &["--apple-load-keychain".into()],
                    Path::new(DECLINES),
                    socket,
                ));
            }
            said.push_str(&ssh_add(&files, Path::new(DECLINES), socket));
        }
        Asking::InAWindow { errand } => match write_the_asker(errand) {
            Ok(asker) => {
                said.push_str(&ssh_add(&files, &asker, socket));
                let _ = std::fs::remove_file(&asker);
            }
            Err(why) => {
                return nothing(
                    before,
                    format!("Errand could not put up the passphrase window: {why}"),
                )
            }
        },
    }
    let added = added_in(&said);
    let why_not = match added.is_empty() {
        true => Some(why_none(&said)),
        false => None,
    };
    Loaded {
        added,
        holds: asking_the_agent(socket),
        why_not,
    }
}

/// Run it, with the passphrase asked for by `asker` and by nothing else. Never
/// a terminal: Errand has none, and a terminal is where `ssh-add` would
/// otherwise wait for an answer nobody can give.
fn ssh_add(args: &[OsString], asker: &Path, socket: Option<&Path>) -> String {
    let mut add = Command::new(SSH_ADD);
    add.args(args)
        .env("SSH_ASKPASS", asker)
        .env("SSH_ASKPASS_REQUIRE", "force")
        .stdin(Stdio::null());
    if let Some(socket) = socket {
        add.env("SSH_AUTH_SOCK", socket);
    }
    match add.output() {
        Ok(out) => format!(
            "{}{}",
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout)
        ),
        Err(why) => format!("ssh-add could not be run: {why}\n"),
    }
}

/// The keys `ssh-add` says it added, by the names of their files. Only the
/// name: the rest of its line is the key's comment, which is often somebody's
/// email address.
fn added_in(said: &str) -> Vec<String> {
    let mut added: Vec<String> = Vec::new();
    for line in said.lines() {
        let Some(rest) = line.strip_prefix("Identity added: ") else {
            continue;
        };
        let path = rest.split(" (").next().unwrap_or(rest).trim();
        let name = Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());
        if !added.contains(&name) {
            added.push(name);
        }
    }
    added
}

/// Why nothing was added, from what `ssh-add` said. Nothing at all is a
/// passphrase it was not given: it says nothing about one.
fn why_none(said: &str) -> String {
    match said
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("Identity added"))
    {
        None => NOT_GIVEN.to_string(),
        Some(line) => format!("No key was loaded. ssh-add said: {line}"),
    }
}

/// The passphrase window, as a program `ssh-add` can run: handed the question,
/// it prints the answer, and the answer goes to `ssh-add` and nowhere else. It
/// gives up after five minutes, so a window left open does not leave `ssh-add`
/// waiting for ever.
const ASKER: &str = r#"#!/bin/sh
exec /usr/bin/osascript \
  -e 'on run argv' \
  -e 'activate' \
  -e 'set asked to display dialog ("Errand is loading your SSH key into the key agent, so your teammates can use it without ever reading it." & return & return & (item 1 of argv)) default answer "" with hidden answer with title "Errand" buttons {"Cancel", "Load"} default button "Load" cancel button "Cancel" giving up after 300' \
  -e 'if gave up of asked then error number -128' \
  -e 'return text returned of asked' \
  -e 'end run' \
  "$1"
"#;

/// Write it where only Errand can, fresh each time.
fn write_the_asker(errand: &Path) -> std::io::Result<PathBuf> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let kept = errand.join("keys");
    std::fs::create_dir_all(&kept)?;
    std::fs::set_permissions(&kept, std::fs::Permissions::from_mode(0o700))?;
    // With a dot in its name, which no model's key file can have.
    let at = kept.join("ssh-askpass.sh");
    match std::fs::remove_file(&at) {
        Ok(()) => {}
        Err(why) if why.kind() == std::io::ErrorKind::NotFound => {}
        Err(why) => return Err(why),
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o700)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&at)?;
    std::io::Write::write_all(&mut file, ASKER.as_bytes())?;
    Ok(at)
}

const ALREADY: &str = "It is already being loaded: answer the passphrase window that is open.";

const NO_AGENT: &str = "The key agent cannot be reached from Errand, so there is nowhere to \
    load a key. Quitting Errand and opening it again usually puts that right; if not, \
    restarting the Mac does.";

const NO_KEYS: &str = "There is no SSH key in ~/.ssh to load. If yours is kept somewhere \
    else, name it with IdentityFile in ~/.ssh/config and try again.";

const NOT_GIVEN: &str = "No key was loaded: the passphrase window was closed, or left for \
    five minutes without an answer.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_the_agent_says_is_read_the_way_ssh_add_means_it() {
        assert_eq!(
            holds(Some(0), "256 SHA256:abc me@studio (ED25519)\n"),
            Holds::Keys(1)
        );
        assert_eq!(
            holds(Some(1), "The agent has no identities.\n"),
            Holds::Nothing
        );
        assert_eq!(holds(Some(2), ""), Holds::NoAgent);
        assert_eq!(holds(None, ""), Holds::NoAgent);
        assert_eq!(
            asking_the_agent(Some(Path::new("/nowhere/at/all.sock"))),
            Holds::NoAgent
        );
    }

    #[test]
    fn only_the_name_of_a_key_is_kept_from_what_ssh_add_says() {
        let said = "Identity added: /Users/me/.ssh/id_ed25519 (me@example.com)\n\
                    Identity added: /Users/me/keys/studio_key (studio)\n\
                    Identity added: /Users/me/.ssh/id_ed25519 (me@example.com)\n";
        let added = added_in(said);
        assert_eq!(added, vec!["id_ed25519", "studio_key"]);
        assert!(!added.join(" ").contains("example.com"));

        assert_eq!(why_none(""), NOT_GIVEN);
        let refused = why_none("Error loading key \"/Users/me/.ssh/id_rsa\": invalid format\n");
        assert!(refused.contains("invalid format"), "{refused}");
    }

    #[test]
    fn the_window_that_asks_is_written_where_no_errand_can_change_it() {
        use std::os::unix::fs::PermissionsExt;
        let errand = std::env::temp_dir().join(format!("errand-ssh-asker-{}", std::process::id()));
        std::fs::remove_dir_all(&errand).ok();
        let at = write_the_asker(&errand).expect("written");
        // The keys folder, which every wall denies reading and writing.
        assert_eq!(at, errand.join("keys").join("ssh-askpass.sh"));
        let mode = std::fs::metadata(&at).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        let said = std::fs::read_to_string(&at).unwrap();
        assert!(said.starts_with("#!/bin/sh\n"), "{said}");
        assert!(said.contains("with hidden answer"), "{said}");
        assert!(said.contains("giving up after"), "{said}");
        // Written again over the last, rather than refused for being there.
        assert!(write_the_asker(&errand).is_ok());
        std::fs::remove_dir_all(&errand).ok();
    }

    #[test]
    fn keys_are_loaded_into_the_agent_and_one_needing_a_passphrase_is_not_waited_on() {
        // Run rather than read: what matters is what ssh-add does with the
        // flags and the asker it is given. Every key is made here, now, and
        // loaded into an agent of the test's own.
        if !Path::new("/usr/bin/ssh-agent").exists() || !Path::new("/usr/bin/ssh-keygen").exists() {
            return;
        }
        let theirs = std::env::temp_dir().join(format!("errand-ssh-load-{}", std::process::id()));
        std::fs::remove_dir_all(&theirs).ok();
        std::fs::create_dir_all(theirs.join(".ssh")).expect("a place");
        std::fs::create_dir_all(theirs.join("keys")).expect("a place");
        let theirs = theirs.canonicalize().expect("a real path");
        let ssh = theirs.join(".ssh");
        let make = |key: &Path, phrase: &str| {
            Command::new("/usr/bin/ssh-keygen")
                .args([
                    "-q",
                    "-t",
                    "ed25519",
                    "-C",
                    "errand-ssh-test",
                    "-N",
                    phrase,
                    "-f",
                ])
                .arg(key)
                .status()
                .is_ok_and(|s| s.success())
        };
        // One with a passphrase, made up now so nothing that looks like a
        // secret is written in here.
        let phrase = format!("test-{}", std::process::id());
        if !make(&ssh.join("id_ed25519"), "")
            || !make(&theirs.join("keys").join("studio_key"), "")
            || !make(&ssh.join("id_ecdsa"), &phrase)
        {
            return;
        }
        std::fs::write(
            ssh.join("config"),
            "Host studio\n  HostName 192.0.2.10\n  IdentityFile ~/keys/studio_key\n",
        )
        .unwrap();

        let found = keys_to_load(&theirs);
        assert_eq!(
            found,
            vec![
                ssh.join("id_ed25519"),
                ssh.join("id_ecdsa"),
                theirs.join("keys").join("studio_key"),
            ]
        );

        let socket = theirs.join("agent.sock");
        let mut agent = Command::new("/usr/bin/ssh-agent")
            .arg("-D")
            .arg("-a")
            .arg(&socket)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("an agent");
        for _ in 0..50 {
            if socket.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
        assert_eq!(asking_the_agent(Some(&socket)), Holds::Nothing);

        let loaded = loading(&theirs, Asking::Never, Some(&socket), false);
        assert_eq!(loaded.added, vec!["id_ed25519", "studio_key"], "{loaded:?}");
        assert_eq!(loaded.holds, Holds::Keys(2), "{loaded:?}");
        assert_eq!(loaded.why_not, None);

        // Nothing to load is said, not shrugged at.
        let empty = theirs.join("nobody");
        std::fs::create_dir_all(&empty).unwrap();
        let none = loading(&empty, Asking::Never, Some(&socket), false);
        assert_eq!(none.why_not.as_deref(), Some(NO_KEYS));

        let _ = agent.kill();
        let _ = agent.wait();
        assert_eq!(
            loading(&theirs, Asking::Never, Some(&socket), false)
                .why_not
                .as_deref(),
            Some(NO_AGENT)
        );
        std::fs::remove_dir_all(&theirs).ok();
    }
}
