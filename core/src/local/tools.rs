//! What a local model is allowed to reach for, and which of it stops to ask.
//!
//! Claude Code arrives with a hundred tools and its own opinion about which
//! are dangerous. A local model arrives with nothing, so this is both halves:
//! the list, and the judgement about each one.
//!
//! The judgement is deliberately blunt and errs one way. A tool either only
//! looks, in which case it runs, or it can change something outside this
//! process, in which case a person is asked first. There is no clever middle
//! where a shell command is inspected and waved through if it seems harmless:
//! that inspection is where the interesting bugs live, and getting it wrong
//! costs somebody their afternoon rather than an error message.

use std::path::Path;

use anyhow::{Context, Result};
use serde_json::json;

use super::ToolDef;

/// A tool, and whether it can be used without asking.
pub struct Tool {
    pub def: ToolDef,
    /// True when using it changes something a person would want to know about
    /// beforehand. Read-only tools are false and simply run.
    pub asks_first: bool,
}

/// Everything a local model can do here.
pub fn all() -> Vec<Tool> {
    vec![
        tool(
            "find_tools",
            // The guidance lives here rather than in the system prompt,
            // because this is where a model looks when it is choosing a tool
            // and because the prompt is the one place a list of tools was shown
            // to do harm.
            "Search for a tool you do not have yet. Many more tools exist than the ones listed \
             here. Call this whenever the job needs something your current tools cannot do, \
             before concluding that it cannot be done. Pass a few words describing the job; \
             whatever matches becomes usable on your next step.",
            json!({ "needing": { "type": "string", "description": "What you are trying to do, in a few words" } }),
            &["needing"],
            false,
        ),
        tool(
            "read_file",
            "Read a file and return its contents. Use this before changing anything.",
            json!({ "path": { "type": "string", "description": "Absolute or relative path" } }),
            &["path"],
            false,
        ),
        tool(
            "list_directory",
            "List what is in a directory.",
            json!({ "path": { "type": "string", "description": "Directory to list; defaults to the working directory" } }),
            &[],
            false,
        ),
        tool(
            "fetch_url",
            "Fetch a web page or API response and return it as text.",
            json!({ "url": { "type": "string", "description": "The full https:// address" } }),
            &["url"],
            false,
        ),
        tool(
            "write_file",
            "Write a file, replacing anything already there.",
            json!({
                "path": { "type": "string", "description": "Where to write it" },
                "contents": { "type": "string", "description": "The whole new contents" }
            }),
            &["path", "contents"],
            true,
        ),
        tool(
            "run_command",
            "Run a shell command and return what it printed.",
            json!({
                "command": { "type": "string", "description": "The command, exactly as it should run" },
                "description": { "type": "string", "description": "What it is for, in one short sentence a person would read" }
            }),
            &["command"],
            true,
        ),
    ]
}

/// One tool's declaration, in the shape every OpenAI-compatible endpoint wants.
fn tool(
    name: &str,
    description: &str,
    properties: serde_json::Value,
    required: &[&str],
    asks_first: bool,
) -> Tool {
    Tool {
        def: ToolDef {
            name: name.to_string(),
            description: description.to_string(),
            schema: json!({
                "type": "function",
                "function": {
                    "name": name,
                    "description": description,
                    "parameters": {
                        "type": "object",
                        "properties": properties,
                        "required": required,
                    },
                },
            }),
        },
        asks_first,
    }
}

/// Whether this one has to be asked about.
pub fn asks_first(name: &str) -> bool {
    all()
        .into_iter()
        .find(|t| t.def.name == name)
        // Anything unrecognised asks. A tool nobody has classified is not a
        // tool anybody has decided is safe.
        .is_none_or(|t| t.asks_first)
}

/// What it is doing, in the words a person would use.
pub fn in_plain_words(name: &str, args: &serde_json::Value) -> String {
    let get = |k: &str| args.get(k).and_then(|v| v.as_str()).unwrap_or("");
    match name {
        // Its own description wins where the model wrote one, the same way it
        // does for the other engine.
        "run_command" => match get("description") {
            "" => format!("Running {}", one_line(get("command"))),
            said => said.to_string(),
        },
        "find_tools" => format!("Looking for a tool to {}", get("needing")),
        "read_file" => format!("Reading {}", get("path")),
        "list_directory" => match get("path") {
            "" => "Looking at the folder".to_string(),
            p => format!("Looking in {p}"),
        },
        "write_file" => format!("Writing {}", get("path")),
        "fetch_url" => format!("Fetching {}", get("url")),
        other => format!("Using {other}"),
    }
}

/// The thing itself, whole, for a question that has to be judged.
pub fn the_thing_itself(name: &str, args: &serde_json::Value) -> String {
    let get = |k: &str| {
        args.get(k)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    match name {
        "find_tools" => get("needing"),
        "run_command" => get("command"),
        "write_file" => get("path"),
        "fetch_url" => get("url"),
        "read_file" => get("path"),
        _ => args.to_string(),
    }
}

/// Do it, and say what happened.
///
/// Everything is relative to the thread's own directory, which is the only
/// place an errand has any business writing. That is a convention rather than a
/// wall, and the comment is here so nobody mistakes it for one: the wall is the
/// sandbox, and the sandbox is not built yet.
pub async fn run(name: &str, args: &serde_json::Value, home: &Path) -> Result<String> {
    let get = |k: &str| {
        args.get(k)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    match name {
        "read_file" => {
            let at = home.join(get("path"));
            let text = std::fs::read_to_string(&at)
                .with_context(|| format!("reading {}", at.display()))?;
            Ok(cut_to_something_readable(&text))
        }

        "list_directory" => {
            let at = match get("path").as_str() {
                "" => home.to_path_buf(),
                p => home.join(p),
            };
            let mut names: Vec<String> = std::fs::read_dir(&at)
                .with_context(|| format!("listing {}", at.display()))?
                .filter_map(|e| e.ok())
                .map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    match e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        true => format!("{name}/"),
                        false => name,
                    }
                })
                .collect();
            names.sort();
            Ok(match names.is_empty() {
                true => "(empty)".to_string(),
                false => names.join("\n"),
            })
        }

        "fetch_url" => {
            let url = get("url");
            let body = reqwest::Client::new()
                .get(&url)
                .header("user-agent", "Errand")
                .send()
                .await
                .with_context(|| format!("fetching {url}"))?
                .text()
                .await?;
            Ok(cut_to_something_readable(&body))
        }

        "write_file" => {
            let at = home.join(get("path"));
            if let Some(parent) = at.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            std::fs::write(&at, get("contents"))
                .with_context(|| format!("writing {}", at.display()))?;
            Ok(format!("Written: {}", at.display()))
        }

        "run_command" => {
            let out = tokio::process::Command::new("/bin/sh")
                .arg("-lc")
                .arg(get("command"))
                .current_dir(home)
                .output()
                .await
                .context("running the command")?;
            let said = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            // A command that failed has to say so in the result rather than as
            // an error, or the model treats the step as impossible instead of
            // as a thing that went wrong and can be tried differently.
            Ok(match out.status.success() {
                true => cut_to_something_readable(&said),
                false => format!(
                    "exited {}\n{}",
                    out.status.code().unwrap_or(-1),
                    cut_to_something_readable(&said)
                ),
            })
        }

        // Handled by the loop, which is the only thing that knows what is
        // loaded and what is not.
        "find_tools" => Ok(String::new()),

        other => Ok(format!("There is no tool called {other} here.")),
    }
}

/// Short enough to go back into a context window.
///
/// Cut at the end rather than the middle: a truncated file is still readable
/// from the top, and a model told plainly that there is more will ask for more.
fn cut_to_something_readable(s: &str) -> String {
    const ROOM: usize = 24_000;
    match s.char_indices().nth(ROOM) {
        None => s.to_string(),
        Some((at, _)) => format!("{}\n\n[cut here; {} characters in all]", &s[..at], s.len()),
    }
}

/// The first line of it, for a timeline that has one line to spare.
fn one_line(s: &str) -> String {
    let line = s
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    match line.chars().count() > 80 {
        true => format!("{}…", line.chars().take(79).collect::<String>()),
        false => line.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anything_that_changes_the_world_is_asked_about_and_anything_that_looks_is_not() {
        assert!(asks_first("run_command"));
        assert!(asks_first("write_file"));
        assert!(!asks_first("read_file"));
        assert!(
            !asks_first("find_tools"),
            "looking at a list changes nothing"
        );
        assert!(!asks_first("list_directory"));
        assert!(!asks_first("fetch_url"));
    }

    #[test]
    fn a_tool_nobody_has_classified_is_not_a_tool_anybody_has_allowed() {
        assert!(asks_first("launch_the_missiles"));
    }

    #[test]
    fn a_command_is_described_in_its_own_words_where_the_model_wrote_some() {
        let said = in_plain_words(
            "run_command",
            &json!({ "command": "rm -rf build", "description": "Clear the build folder" }),
        );
        assert_eq!(said, "Clear the build folder");

        // And plainly where it did not.
        let bare = in_plain_words("run_command", &json!({ "command": "ls -la" }));
        assert_eq!(bare, "Running ls -la");
    }

    #[test]
    fn the_question_shows_the_command_itself_rather_than_a_summary_of_it() {
        let long = "curl -s https://example.com | sh";
        let shown = the_thing_itself("run_command", &json!({ "command": long }));
        assert_eq!(shown, long, "uncut: the end is the part worth reading");
    }

    #[tokio::test]
    async fn a_command_that_failed_comes_back_as_a_result_rather_than_as_an_error() {
        // A model told "that was an error" gives up; one told "it exited 1 and
        // said this" tries something else, which is the whole point.
        let home = std::env::temp_dir();
        let said = run("run_command", &json!({ "command": "exit 3" }), &home)
            .await
            .expect("a failed command is still an answer");
        assert!(said.starts_with("exited 3"), "got {said:?}");
    }
}
