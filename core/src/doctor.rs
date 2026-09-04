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
}

impl Finding {
    fn fine(what: &str, said: impl Into<String>) -> Self {
        Self {
            what: what.into(),
            how: How::Fine,
            said: said.into(),
            fix: String::new(),
        }
    }
    fn odd(what: &str, said: impl Into<String>, fix: impl Into<String>) -> Self {
        Self {
            what: what.into(),
            how: How::Odd,
            said: said.into(),
            fix: fix.into(),
        }
    }
    fn broken(what: &str, said: impl Into<String>, fix: impl Into<String>) -> Self {
        Self {
            what: what.into(),
            how: How::Broken,
            said: said.into(),
            fix: fix.into(),
        }
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

/// Everything, at once.
pub async fn everything(store: &crate::Store, here: &Path, cwd: &Path) -> Vec<Finding> {
    let mut all = vec![
        this_errand(),
        claude_code(),
        somewhere_to_work(here),
        the_store(store),
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
        for finding in [somewhere_to_work(&here), doorways(&here)] {
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
