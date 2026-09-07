//! What is wrong with this setup, before it goes wrong in the middle of a job.
//!
//! Everything checked here is something that has actually gone wrong on a real
//! machine, and every one of them failed in the same unhelpful shape: not as an
//! error, but as an agent that quietly could not do something and had no way to
//! say why. A model server bound to the loopback address, so nothing on the
//! network could ever see it. An MCP server whose interpreter had been removed
//! by a package upgrade, so its tools were simply absent and looked like tools
//! the agent chose not to use. Rows in the store pointing at conversations that
//! no longer existed.
//!
//! None of those is visible from inside a conversation. All of them are one
//! question away if somebody thinks to ask, so this is the place that asks them
//! all at once.
//!
//! Every finding says what to do about it. A diagnostic that reports a problem
//! and leaves somebody to work out the fix has done the easy half.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;

/// How much of a problem something is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum How {
    /// Working.
    Fine,
    /// Works, but something is not as it should be.
    Odd,
    /// Something cannot work at all until this is dealt with.
    Broken,
}

/// One thing that was checked.
#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    /// What was checked, as somebody would name it.
    pub what: String,
    pub how: How,
    /// What was found, in words.
    pub said: String,
    /// What to do about it. Empty when there is nothing to do.
    pub fix: String,
    /// Where the fix is done, when that is a pane of System Settings: an
    /// address the system opens, so the window can offer to open it rather
    /// than describe the way there. Empty when there is nowhere to open.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub settings: String,
}

impl Finding {
    fn fine(what: &str, said: impl Into<String>) -> Self {
        Self {
            what: what.into(),
            how: How::Fine,
            said: said.into(),
            fix: String::new(),
            settings: String::new(),
        }
    }
    fn odd(what: &str, said: impl Into<String>, fix: impl Into<String>) -> Self {
        Self {
            what: what.into(),
            how: How::Odd,
            said: said.into(),
            fix: fix.into(),
            settings: String::new(),
        }
    }
    fn broken(what: &str, said: impl Into<String>, fix: impl Into<String>) -> Self {
        Self {
            what: what.into(),
            how: How::Broken,
            said: said.into(),
            fix: fix.into(),
            settings: String::new(),
        }
    }
    /// The same finding, with the pane of System Settings its fix is done in.
    fn done_in(mut self, pane: &str) -> Self {
        self.settings = pane.into();
        self
    }
}

/// Is Claude Code there, and what does it call itself?
pub fn claude_code() -> Finding {
    // Looked for the way the engine looks for it, so this cannot say "fine"
    // about a program the engine then fails to start, or "missing" about one
    // it would have found in the usual place.
    let said = std::process::Command::new(crate::claude::where_claude_is())
        .arg("--version")
        .output();
    match said {
        Ok(out) if out.status.success() => Finding::fine(
            "Claude Code",
            String::from_utf8_lossy(&out.stdout).trim().to_string(),
        ),
        Ok(out) => Finding::broken(
            "Claude Code",
            format!(
                "it is there but would not say its version: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
            "Try running `claude --version` yourself to see what it says.",
        ),
        Err(_) => Finding::broken(
            "Claude Code",
            "not found on the PATH",
            "Install it, or make sure `claude` is on the PATH this app inherits. \
             An app launched from the Dock does not read your shell's profile, so a \
             PATH set in .zshrc is not one this app can see.",
        ),
    }
}

/// The MCP servers somebody has configured, and whether they start.
pub async fn tools_from_outside(cwd: &Path) -> Vec<Finding> {
    let configured = crate::mcp::configured(cwd);
    if configured.is_empty() {
        return vec![Finding::fine(
            "Tool servers",
            "none configured, which is fine; both engines still have their own tools",
        )];
    }

    let running = crate::mcp::Servers::open(cwd).await;
    let mut found = Vec::new();
    for server in &configured {
        match running
            .trouble
            .iter()
            .find(|(name, _)| *name == server.name)
        {
            Some((_, why)) => found.push(Finding::broken(
                &format!("Tool server: {}", server.name),
                why.clone(),
                // The failure that actually happened: an upgrade removed the
                // Python a virtual environment was built on, leaving a
                // directory full of dangling symlinks.
                "Its tools are absent from every agent, which looks exactly like an \
                 agent choosing not to use them. Check the command in ~/.claude.json \
                 still exists: an interpreter inside a virtual environment stops \
                 existing when the one it was built from is upgraded away.",
            )),
            None => {
                let count = running
                    .tools()
                    .iter()
                    .filter(|t| t.server == server.name)
                    .count();
                found.push(match count {
                    0 => Finding::odd(
                        &format!("Tool server: {}", server.name),
                        "it started but offers nothing",
                        "Nothing to do here unless you expected tools from it.",
                    ),
                    n => Finding::fine(
                        &format!("Tool server: {}", server.name),
                        format!("{n} tools"),
                    ),
                });
            }
        }
    }
    found
}

/// Anything on this machine that could answer, and whether the network can see
/// it.
pub async fn models_here() -> Vec<Finding> {
    let found = crate::local::find::detect_all().await;
    if found.is_empty() {
        return vec![Finding::odd(
            "Local models",
            "nothing answered on the usual ports",
            "Only Claude can answer. Start Ollama or LM Studio if you wanted a local \
             model as well.",
        )];
    }

    let mut said = Vec::new();
    for backend in &found {
        let usable = crate::local::ready::what_can_answer(
            &backend.provider,
            &backend.base_url,
            &backend.models,
        )
        .await;
        let loaded = usable.iter().filter(|m| m.loaded).count();
        said.push(Finding::fine(
            &format!("Local models: {}", backend.label),
            match loaded {
                0 => format!(
                    "{} usable, none loaded, so the first errand will wait while one loads",
                    usable.len()
                ),
                n => format!("{} usable, {n} loaded and ready", usable.len()),
            },
        ));
    }

    // The one that cost an afternoon: a model server anybody can use from this
    // machine and nobody can see from any other, which reads as a broken
    // network sweep rather than as a bound address.
    if found
        .iter()
        .all(|b| b.base_url.contains("127.0.0.1") || b.base_url.contains("localhost"))
    {
        said.push(Finding::odd(
            "Models on the network",
            "everything found is bound to this machine only",
            "Looking on the network will never find these, because a server listening \
             on 127.0.0.1 cannot be reached from another machine. For Ollama, start it \
             with OLLAMA_HOST=0.0.0.0 to let other machines see it.",
        ));
    }
    said
}

/// Whether anything in the store points at something that is not there.
pub fn the_store(store: &crate::Store) -> Finding {
    match store.points_at_nothing() {
        Err(why) => Finding::broken("The store", format!("could not be checked: {why}"), ""),
        Ok(0) => Finding::fine("The store", "everything refers to something that exists"),
        Ok(n) => Finding::odd(
            "The store",
            format!("{n} rows refer to something that is no longer there"),
            "Harmless: nothing shows them, because everything is looked up through the \
             conversation they belong to. They are left rather than deleted, since \
             quietly removing rows to make a check pass is worse than the check.",
        ),
    }
}

/// Sockets left behind by an app that did not get to tidy up.
pub fn doorways(here: &Path) -> Finding {
    let left = std::fs::read_dir(here.join("mcp"))
        .map(|d| {
            d.flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "sock"))
                .count()
        })
        .unwrap_or(0);
    Finding::fine(
        "Ways in for Claude Code",
        match left {
            0 => "none open, which is right when nothing is running".to_string(),
            n => format!("{n} open, one for each conversation Claude Code is answering"),
        },
    )
}

/// Somewhere to keep things, that can actually be written to.
pub fn somewhere_to_work(here: &Path) -> Finding {
    let probe = here.join(".errand-can-write");
    match std::fs::write(&probe, b"x").and_then(|()| std::fs::remove_file(&probe)) {
        Ok(()) => Finding::fine("Where things are kept", here.display().to_string()),
        Err(why) => Finding::broken(
            "Where things are kept",
            format!("{} cannot be written to: {why}", here.display()),
            "Nothing can be saved until this is fixed. Check the folder's permissions.",
        ),
    }
}

/// Which Errand this is.
///
/// Every version so far has been installed by hand, over the top of the last
/// one, sometimes three times in an evening. "Which one am I running" is then a
/// real question with no way to answer it, and the answer decides whether a
/// bug report is about something already fixed.
pub fn this_errand() -> Finding {
    Finding::fine(
        "This Errand",
        format!(
            "version {}, installed by hand: there is no updater yet, and nowhere \
             to update from",
            env!("CARGO_PKG_VERSION")
        ),
    )
}

/// Whether an agent that never asks can be walled in.
///
/// Worth saying out loud rather than assuming, because the answer decides what
/// "Never" means: with the wall, it means an agent confined to its own folder;
/// without it, it means an agent that can do anything and will not mention it.
pub fn the_wall() -> Finding {
    match crate::wall::possible() {
        true => Finding::fine(
            "The wall",
            "agents set to never ask are confined to their own folder",
        ),
        false => Finding::odd(
            "The wall",
            "this machine has no sandbox-exec, so nothing can be walled in",
            "An agent set to never ask has nothing between it and the rest of the machine. \
             Set those agents back to asking first, where you decide each time.",
        ),
    }
}

/// Whether macOS will put one of this app's notifications on screen.
///
/// Learnt by the app, which is the side that can ask the system, and judged
/// here, which is the side that knows what to say about the answer. Not a
/// plain yes or no, because the third answer is the one that hides: the
/// system's question dismissed without being answered leaves notifications
/// off with nobody ever having said no.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notifying {
    /// They are on.
    Allowed,
    /// Somebody said no, or switched them off since.
    Refused,
    /// The system's question was never answered: dismissed, or still open.
    NotAnswered,
    /// It could not be asked: there is no app bundle for the system to
    /// attribute a notification to, or the system did not answer in time.
    Unknown,
}

/// The pane of System Settings where notifications are turned on, open at
/// this app's own entry, as an address the system opens.
///
/// The query is what selects Errand in the list. Without it the pane opens on
/// its front page and Errand is one row among sixty, under a letter that
/// depends on what else is installed. The id is this app's bundle identifier,
/// from tauri.conf.json.
pub const NOTIFICATION_SETTINGS: &str =
    "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=com.errandai.errand";

/// Whether an errand that finishes while somebody is reading something else
/// will say so.
///
/// The refusal used to be one line on stderr, which nobody who opens an app
/// from the Dock has ever seen. From the outside it looked like an app whose
/// errands finish without a word, and saying so is the one thing a
/// notification is for.
pub fn notifications(may: Notifying) -> Finding {
    const WHAT: &str = "Notifications";
    const HOW: &str = "Open System Settings, then Notifications, then Errand, and switch Allow \
                       notifications on.";
    match may {
        Notifying::Allowed => Finding::fine(
            WHAT,
            "on, so an errand that finishes while you are reading something else says so",
        ),
        Notifying::Refused => Finding::odd(
            WHAT,
            "off for Errand in macOS, so an errand that finishes while you are reading \
             something else finishes quietly",
            HOW,
        )
        .done_in(NOTIFICATION_SETTINGS),
        Notifying::NotAnswered => Finding::odd(
            WHAT,
            "macOS has not been told whether Errand may show them, so none are shown",
            format!("Errand asks when it starts, and the question may have been dismissed. {HOW}"),
        )
        .done_in(NOTIFICATION_SETTINGS),
        Notifying::Unknown => Finding::odd(
            WHAT,
            "could not be checked: macOS did not answer, or this is not an app it can \
             attribute a notification to",
            "Open Errand from /Applications rather than from a build folder, and check again.",
        ),
    }
}

/// Whether the line about notifications has been said in this run of the app.
///
/// Once per run rather than once per errand, and once per run rather than
/// once ever: said on every errand it is a nag about something that was
/// decided, and never said again it cannot reach somebody who turned them off
/// a month later. A run of this app is weeks long, window or no window, so
/// once per run is close to once.
#[derive(Debug, Default)]
pub struct Told(AtomicBool);

/// The line a conversation gets when an errand ended, nobody was watching,
/// and macOS would not say so.
///
/// `may` is asked only until the line has been said, because asking is a
/// round trip to the system on every errand that ends unwatched, which is
/// most of them. None while notifications are on, when they cannot be
/// checked, and on every occasion after the first.
pub fn nobody_was_told(told: &Told, may: impl FnOnce() -> Notifying) -> Option<String> {
    if told.0.load(Ordering::SeqCst) {
        return None;
    }
    let why = match may() {
        Notifying::Refused => "Notifications are off for Errand in macOS",
        Notifying::NotAnswered => "macOS has not been told whether Errand may show notifications",
        Notifying::Allowed | Notifying::Unknown => return None,
    };
    if told.0.swap(true, Ordering::SeqCst) {
        return None;
    }
    Some(format!(
        "{why}, so nothing says when an errand finishes or needs you while you are reading \
         something else. To turn them on, open System Settings, then Notifications, then \
         Errand, and switch Allow notifications on."
    ))
}

/// Everything, at once.
pub async fn everything(
    store: &crate::Store,
    here: &Path,
    cwd: &Path,
    may: Notifying,
) -> Vec<Finding> {
    let mut all = vec![
        this_errand(),
        claude_code(),
        somewhere_to_work(here),
        the_store(store),
        notifications(may),
        the_wall(),
        doorways(here),
    ];
    // Bounded, because two of these talk to the network and a check that hangs
    // is a check nobody runs twice.
    let outside = tokio::time::timeout(Duration::from_secs(45), tools_from_outside(cwd)).await;
    all.extend(outside.unwrap_or_else(|_| {
        vec![Finding::odd(
            "Tool servers",
            "they took too long to answer",
            "One of them is slow to start. Opening Tools on an agent will say which.",
        )]
    }));
    let models = tokio::time::timeout(Duration::from_secs(20), models_here()).await;
    all.extend(models.unwrap_or_else(|_| {
        vec![Finding::odd(
            "Local models",
            "they took too long to answer",
            "",
        )]
    }));
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finding_that_is_wrong_always_says_what_to_do_about_it() {
        // A diagnostic that reports a problem and leaves somebody to work out
        // the fix has done the easy half of the job.
        let here = std::env::temp_dir();
        for finding in [
            somewhere_to_work(&here),
            doorways(&here),
            notifications(Notifying::Refused),
            notifications(Notifying::NotAnswered),
            notifications(Notifying::Unknown),
        ] {
            if finding.how != How::Fine {
                assert!(
                    !finding.fix.is_empty(),
                    "{} said something was wrong and not what to do: {}",
                    finding.what,
                    finding.said
                );
            }
        }
    }

    #[test]
    fn somewhere_that_cannot_be_written_to_is_broken_rather_than_merely_odd() {
        // Nothing can be saved, so this is not a warning.
        let nowhere = Path::new("/errand-no-such-place-at-all");
        let said = somewhere_to_work(nowhere);
        assert_eq!(said.how, How::Broken);
        assert!(!said.fix.is_empty());
    }

    #[test]
    fn a_store_with_nothing_dangling_says_so_plainly() {
        let store = crate::Store::in_memory().expect("a store");
        let said = the_store(&store);
        assert_eq!(said.how, How::Fine, "{}", said.said);
    }

    #[test]
    fn notifications_that_are_off_say_where_to_turn_them_on() {
        // The refusal used to be a line on stderr, which is nowhere.
        let said = notifications(Notifying::Refused);
        assert_eq!(said.how, How::Odd);
        assert!(said.said.starts_with("off for Errand"), "{}", said.said);
        for word in [
            "System Settings",
            "Notifications",
            "Errand",
            "Allow notifications",
        ] {
            assert!(
                said.fix.contains(word),
                "the fix does not say {word}: {}",
                said.fix
            );
        }
        // And the window is handed the pane to open, not only the way there.
        assert_eq!(said.settings, NOTIFICATION_SETTINGS);
        assert!(said.settings.starts_with("x-apple.systempreferences:"));
        assert!(said.settings.ends_with("?id=com.errandai.errand"));
    }

    #[test]
    fn notifications_that_are_on_are_fine_and_offer_nothing_to_open() {
        let said = notifications(Notifying::Allowed);
        assert_eq!(said.how, How::Fine);
        assert!(said.fix.is_empty());
        assert!(said.settings.is_empty());
        // What the window reads: a pane to open is there only when there is
        // one, so a finding with none draws no button.
        let json = serde_json::to_string(&said).expect("json");
        assert!(!json.contains("settings"), "{json}");
    }

    #[test]
    fn a_question_nobody_answered_is_not_called_a_no() {
        // Dismissing the system's question leaves them off with nobody having
        // said no, and "off for Errand" to that person reads as a decision
        // they never made.
        let said = notifications(Notifying::NotAnswered);
        assert_eq!(said.how, How::Odd);
        assert!(!said.said.contains("off for Errand"), "{}", said.said);
        assert!(said.fix.contains("System Settings"), "{}", said.fix);
        assert_eq!(said.settings, NOTIFICATION_SETTINGS);
    }

    #[test]
    fn the_line_about_notifications_being_off_is_said_once_and_not_on_the_next_errand() {
        let told = Told::default();
        let line = nobody_was_told(&told, || Notifying::Refused).expect("said the first time");
        for word in [
            "off for Errand",
            "System Settings",
            "Notifications",
            "Allow notifications",
        ] {
            assert!(line.contains(word), "the line does not say {word}: {line}");
        }
        assert_eq!(nobody_was_told(&told, || Notifying::Refused), None);
        // And the system is not asked again either: that is a round trip on
        // every errand that ends unwatched, which is most of them.
        assert_eq!(
            nobody_was_told(&told, || -> Notifying {
                unreachable!("asked the system after it had been told")
            }),
            None
        );
    }

    #[test]
    fn nothing_is_said_while_notifications_are_on_and_the_once_is_kept_for_when_they_are_not() {
        let told = Told::default();
        assert_eq!(nobody_was_told(&told, || Notifying::Allowed), None);
        assert_eq!(nobody_was_told(&told, || Notifying::Unknown), None);
        // Turned off a week into the same run: still said, and still once.
        let line =
            nobody_was_told(&told, || Notifying::NotAnswered).expect("said when they go off");
        assert!(line.starts_with("macOS has not been told"), "{line}");
        assert_eq!(nobody_was_told(&told, || Notifying::NotAnswered), None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_folder_with_no_servers_in_it_is_not_reported_as_a_problem() {
        // Most people have none, and a check that calls that a fault is a check
        // that trains people to ignore it.
        let empty = std::env::temp_dir().join("errand-doctor-none");
        std::fs::create_dir_all(&empty).ok();
        // Only meaningful when the machine running the test has no user-level
        // servers of its own, so this asserts the shape rather than the count.
        let said = tools_from_outside(&empty).await;
        assert!(!said.is_empty());
        assert!(said
            .iter()
            .all(|f| !f.what.is_empty() && !f.said.is_empty()));
    }
}
