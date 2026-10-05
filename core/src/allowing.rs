//! What saying "always" actually allows.
//!
//! It allowed almost nothing. The engine suggests a rule, and for a shell
//! command the suggestion is the whole command line, so the rule stored was
//! `top -l 1 -n 15 -o mem -stats pid,command,mem` and the next `top` with one
//! flag different asked again. Somebody pressing "Always" six times in a row and
//! being asked a seventh has not been given a choice, they have been given a
//! speed bump, and the way that ends is that they stop reading the questions.
//!
//! So a simple command is narrowed to the program: say always to `top -l 1 …`
//! and any `top` is allowed afterwards. That is a real widening of what was
//! agreed to, which is exactly why the button says what it will do before it is
//! pressed. "Always · any top command" is a choice somebody can make. "Always"
//! on its own is not.
//!
//! A command that does more than one thing is never narrowed. `printf 'a' >
//! a.txt; rm -rf ~` starts with `printf`, and allowing every command that
//! starts with `printf` would be allowing the second half of that one too.

use serde::Serialize;

/// A rule, and what it allows in words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Allowing {
    /// What to store. The beginning of what is allowed, matched a word at a
    /// time.
    pub rule: String,
    /// What that covers, for the button. Never a guess: what this says and what
    /// the rule does are the same sentence.
    pub in_words: String,
}

/// Anything that makes a command line more than one command.
///
/// Deliberately blunt, and it errs towards not narrowing. A command containing
/// any of these is stored whole, so "always" means only this exact thing.
const MORE_THAN_ONE_THING: &[&str] = &["&&", "||", ";", "|", "$(", "`", "\n", ">", "<", "&"];

/// What "always" would allow for this question.
///
/// `suggested` is what the engine proposed: nothing at all when it offered
/// nothing to remember, and an empty string when it offered to allow the whole
/// tool. Those are different answers and were briefly the same one, which took
/// the button away from every question about a tool that has no finer rule than
/// itself.
pub fn what_always_means(tool: &str, suggested: Option<&str>) -> Option<Allowing> {
    let suggested = suggested?.trim();
    if suggested.is_empty() {
        return Some(the_whole_tool(tool));
    }
    // A folder is not a tool the engine asks about: it widens the wall a
    // never-asking agent runs behind. Stored as the folder itself, and said as
    // what it is, because "anything starting /Volumes/Disk" reads like a
    // command rule and this is a place.
    if is_a_folder(tool) {
        return Some(Allowing {
            rule: suggested.to_string(),
            in_words: format!("writing anywhere inside {suggested}"),
        });
    }

    // Only shell commands are narrowed. Everything else the engine suggests is
    // already the useful shape: a path glob for a file tool covers a directory,
    // an address prefix covers a site.
    if Kind::of(tool) != Some(Kind::Commands) {
        return Some(Allowing {
            rule: suggested.to_string(),
            in_words: in_words(tool, suggested),
        });
    }

    // Logging in to another machine is narrowed to that machine, never to
    // `ssh`: "any ssh command" is every machine the key opens.
    if let Some(host) = a_single_command(suggested)
        .filter(|program| program == "ssh")
        .and_then(|_| logs_in_to(suggested))
        .map(|(_, host, _)| host)
    {
        return Some(Allowing {
            rule: format!("ssh {host}"),
            in_words: format!("any ssh command to {host}"),
        });
    }

    match a_single_command(suggested) {
        Some(program) => Some(Allowing {
            rule: program.clone(),
            in_words: format!("any {program} command"),
        }),
        // Said plainly, because a button that means less than the one beside it
        // should not look the same as it.
        None => Some(Allowing {
            rule: suggested.to_string(),
            in_words: "only this exact command".to_string(),
        }),
    }
}

/// Allowing every use of a tool, which is what an empty rule means.
///
/// Said in full, because it is the widest thing any of these buttons does and
/// the word "always" hides that completely.
pub fn the_whole_tool(tool: &str) -> Allowing {
    Allowing {
        rule: String::new(),
        in_words: match Kind::of(tool) {
            Some(kind) if kind != Kind::Folder => kind.everything().to_string(),
            _ => format!("anything this agent does with {tool}"),
        },
    }
}

/// The program a command runs, when it runs exactly one.
///
/// Nothing when the line does more than one thing, redirects, or reads a
/// variable, because in all of those the first word is not what is being
/// allowed.
fn a_single_command(command: &str) -> Option<String> {
    if MORE_THAN_ONE_THING.iter().any(|c| command.contains(c)) {
        return None;
    }
    for word in command.split_whitespace() {
        // FOO=bar in front of a command names no program.
        if word.contains('=') && !word.starts_with('-') {
            continue;
        }
        // Running something as somebody else is not the thing being allowed,
        // and allowing every `sudo` would be allowing everything.
        if word == "sudo" || word == "env" || word == "command" {
            return None;
        }
        // A path to a program is allowed as that path, not as its last part: a
        // rule of `python` should not cover `/tmp/evil/python`.
        return Some(word.to_string());
    }
    None
}

/// The programs that log in to, or copy to, another machine.
pub const REACHING: &[&str] = &["ssh", "scp", "sftp", "rsync", "ssh-copy-id"];

/// Which machine one command logs in to or copies to, and for `ssh` what it
/// runs there: `(program, host, command there)`.
///
/// The host is the name as written, less any `user@`, so `ssh me@studio df`
/// is `studio`. Nothing for a command that reaches nothing, including an `rsync`
/// between two folders on this Mac.
pub fn logs_in_to(command: &str) -> Option<(String, String, Option<String>)> {
    // The options of each that take a value, which is not the host.
    const SSH_TAKES: &str = "BbcDEeFIiJLlmOopQRSWw";
    const SCP_TAKES: &str = "cDFiJlOoPS";
    let words: Vec<&str> = command.split_whitespace().collect();
    let program = words.first()?.rsplit('/').next()?.to_string();
    if !REACHING.contains(&program.as_str()) {
        return None;
    }
    let bare = |at: &str| {
        let host = at.trim_start_matches("ssh://");
        let host = host.rsplit('@').next().unwrap_or(host);
        host.split(':').next().unwrap_or(host).to_string()
    };
    let takes = match program.as_str() {
        "ssh" | "ssh-copy-id" | "sftp" => SSH_TAKES,
        _ => SCP_TAKES,
    };
    let mut rest = words[1..].iter().enumerate();
    while let Some((at, word)) = rest.next() {
        if let Some(flag) = word.strip_prefix('-') {
            // `-p 22` takes the next word; `-p22` and `--port=22` do not.
            if flag.len() == 1 && takes.contains(flag) {
                rest.next();
            }
            continue;
        }
        let word = word.trim_matches(['"', '\'']);
        match program.as_str() {
            "ssh" | "sftp" | "ssh-copy-id" => {
                let there = words[at + 2..].join(" ");
                return Some((
                    program.clone(),
                    bare(word),
                    (program == "ssh" && !there.is_empty()).then_some(there),
                ));
            }
            // A copy names its far side as `host:path`, and a local path with
            // a colon in it has a slash before the colon.
            _ => {
                if let Some((host, _)) = word.split_once(':') {
                    if !host.is_empty() && !host.contains('/') {
                        return Some((program.clone(), bare(host), None));
                    }
                }
            }
        }
    }
    None
}

/// What a rule already granted actually covers, in words.
///
/// For the list somebody reads when deciding what to take back. A rule stored
/// as a whole command line covers only that line and nothing else, which is
/// invisible from the line itself: it looks like a permission and behaves like
/// a one-off.
pub fn in_words(tool: &str, rule: &str) -> String {
    if rule.is_empty() {
        return the_whole_tool(tool).in_words;
    }
    // A plain path is a place, and said as one. A glob the engine suggested
    // is said as the pattern it is, because "inside //Users/me/**" is not a
    // sentence anybody can check.
    let a_place = rule.starts_with('/') && !rule.starts_with("//") && !rule.contains('*');
    match Kind::of(tool) {
        Some(Kind::Folder) => format!("writing anywhere inside {rule}"),
        Some(Kind::Commands) => match a_single_command(rule) {
            // A rule that is exactly a program name covers every use of it.
            Some(program) if program == rule => format!("any {program} command"),
            // "ssh studio": every command on that one machine.
            Some(program) if program == "ssh" && rule.split_whitespace().count() == 2 => {
                format!(
                    "any ssh command to {}",
                    rule.split_whitespace().nth(1).unwrap_or_default()
                )
            }
            // Anything else is the beginning of one particular command, which
            // in practice means that command and nothing else.
            _ => "only this exact command".to_string(),
        },
        Some(Kind::Reading) if a_place => format!("reading anything inside {rule}"),
        Some(Kind::Changing) if a_place => format!("changing anything inside {rule}"),
        Some(Kind::Writing) if a_place => format!("writing anything inside {rule}"),
        Some(Kind::Fetching) if rule.contains("://") => format!("fetching anything from {rule}"),
        _ => format!("anything starting {rule}"),
    }
}

/// The one kind of allowance that is a place rather than a tool.
pub const A_FOLDER: &str = "folder";

/// Whether an allowance is for a folder the agent may write in.
pub fn is_a_folder(tool: &str) -> bool {
    tool.eq_ignore_ascii_case(A_FOLDER)
}

/// What can be allowed ahead, by what it is for rather than by which engine's
/// tool asks for it.
///
/// Each engine asks under names of its own: Claude's `Bash` is a local model's
/// `run_command` and `start_command`. A rule written ahead was kept under the
/// name the window sent, which was always Claude's, so for a teammate on a
/// local model it never answered a single thing it asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Commands,
    Writing,
    Changing,
    Reading,
    Fetching,
    Folder,
}

impl Kind {
    /// Every kind, in the order the window offers them.
    pub const ALL: [Kind; 6] = [
        Kind::Commands,
        Kind::Writing,
        Kind::Changing,
        Kind::Reading,
        Kind::Fetching,
        Kind::Folder,
    ];

    /// Which kind a tool is, whichever engine's name it goes by.
    pub fn of(tool: &str) -> Option<Kind> {
        match tool.to_ascii_lowercase().as_str() {
            "bash" | "run_command" | "start_command" => Some(Kind::Commands),
            "read" | "read_file" => Some(Kind::Reading),
            "edit" | "notebookedit" | "change_file" => Some(Kind::Changing),
            "write" | "write_file" => Some(Kind::Writing),
            "webfetch" | "fetch_url" => Some(Kind::Fetching),
            A_FOLDER => Some(Kind::Folder),
            _ => None,
        }
    }

    /// A kind by the name the window sends, or by any tool's name.
    pub fn called(name: &str) -> Option<Kind> {
        match name {
            "commands" => Some(Kind::Commands),
            "writing" => Some(Kind::Writing),
            "changing" => Some(Kind::Changing),
            "reading" => Some(Kind::Reading),
            "fetching" => Some(Kind::Fetching),
            _ => Kind::of(name),
        }
    }

    /// The name a rule of this kind is kept under: the one it always had, so a
    /// rule kept before there were kinds reads the same as one kept after.
    pub fn kept_as(self) -> &'static str {
        match self {
            Kind::Commands => "Bash",
            Kind::Writing => "Write",
            Kind::Changing => "Edit",
            Kind::Reading => "Read",
            Kind::Fetching => "WebFetch",
            Kind::Folder => A_FOLDER,
        }
    }

    /// What it is called in the window, after "Using".
    pub fn using(self) -> &'static str {
        match self {
            Kind::Commands => "running commands",
            Kind::Writing => "writing files",
            Kind::Changing => "changing files",
            Kind::Reading => "reading files",
            Kind::Fetching => "fetching web pages",
            Kind::Folder => "a folder it may write in",
        }
    }

    /// What to type after "Let it", when none of the choices is the one.
    pub fn to_type(self) -> &'static str {
        match self {
            Kind::Commands => "a program, like curl, or a whole command",
            Kind::Fetching => "the start of a web address, like https://github.com",
            Kind::Writing | Kind::Changing | Kind::Reading | Kind::Folder => {
                "a folder's whole path, starting with /"
            }
        }
    }

    /// Everything of this kind, in words: what a rule with nothing in it
    /// allows.
    pub fn everything(self) -> &'static str {
        match self {
            Kind::Commands => "running any command at all",
            Kind::Writing => "writing any file",
            Kind::Changing => "changing any file",
            Kind::Reading => "reading any file",
            Kind::Fetching => "fetching any web page",
            Kind::Folder => "writing anywhere",
        }
    }

    /// Whether it can be allowed whole, with nothing in the rule. A folder
    /// cannot: the wall with no edge is not a wall.
    pub fn can_be_whole(self) -> bool {
        self != Kind::Folder
    }
}

/// Whether a rule kept for one tool answers a question asked by another: the
/// same tool, or the same kind of thing asked by the other engine.
pub fn same_thing(kept: &str, asked: &str) -> bool {
    kept == asked || matches!((Kind::of(kept), Kind::of(asked)), (Some(a), Some(b)) if a == b)
}

/// Whether a yes to the whole of one of these covers the other: handing one
/// part to a teammate and handing several out at once are one permission,
/// and a lead told to hand parts out together would otherwise stop at a card
/// on the morning run that its "always" to ask was meant to spare.
pub fn both_hand_work_on(kept: &str, asked: &str) -> bool {
    matches!((kept, asked), ("ask", "hand_out") | ("hand_out", "ask"))
}

/// What is worth offering a teammate, and why some things are not.
///
/// Only what it would ever ask about, or be walled from. One that never asks
/// has nothing to allow but somewhere else to write; a local model reads files
/// and fetches pages without asking; and a teammate on Claude that asks is not
/// walled, so a folder would change nothing for it.
pub fn worth_offering(local: bool, asks: &str) -> (Vec<Kind>, Option<&'static str>) {
    use Kind::*;
    match (asks, local) {
        ("auto", _) => (
            vec![Folder],
            Some(
                "It never asks before doing anything, so the one thing to allow is somewhere \
                 else to write.",
            ),
        ),
        ("edits", true) => (
            vec![Commands, Folder],
            Some(
                "It writes and changes files without asking, and on a local model it reads \
                 files and fetches pages without asking too.",
            ),
        ),
        ("edits", false) => (
            vec![Commands, Reading, Fetching],
            Some("It writes and changes files without asking."),
        ),
        (_, true) => (
            vec![Commands, Writing, Changing, Folder],
            Some("On a local model it reads files and fetches pages without asking."),
        ),
        (_, false) => (vec![Commands, Writing, Changing, Reading, Fetching], None),
    }
}

/// Whether something already allowed covers what is being asked.
///
/// A word at a time, not a character at a time. `starts_with` alone means a
/// rule of `top` covers `topple-everything`, which nobody agreed to.
pub fn covers(rule: &str, doing: &str) -> bool {
    if rule.is_empty() {
        return true;
    }
    if doing == rule {
        return true;
    }
    // The boundary matters: a rule that ends mid-word is a rule that leaks.
    doing.starts_with(rule)
        && doing[rule.len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_whitespace() || c == '/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_said_as_a_place_to_write_and_not_as_a_command_rule() {
        // "anything starting /Volumes/Disk" reads like a command. It is a
        // place, and the words have to say which.
        let said = what_always_means("folder", Some("/Volumes/Disk")).unwrap();
        assert_eq!(said.rule, "/Volumes/Disk");
        assert_eq!(said.in_words, "writing anywhere inside /Volumes/Disk");
        assert_eq!(in_words("folder", "/Volumes/Disk"), said.in_words);
    }

    #[test]
    fn saying_always_to_a_plain_command_allows_that_command_again() {
        // The whole complaint: six "always" answers and a seventh question,
        // because the rule stored was the entire line and the next line had one
        // flag different.
        let said = what_always_means("Bash", Some("top -l 1 -n 15 -o mem -stats pid,command,mem"))
            .expect("something to remember");
        assert_eq!(said.rule, "top");
        assert_eq!(said.in_words, "any top command");
        assert!(covers(&said.rule, "top -l 2"));
        assert!(covers(&said.rule, "top"));
    }

    #[test]
    fn a_command_that_does_more_than_one_thing_is_never_narrowed() {
        // `printf 'a' > a.txt; rm -rf ~` starts with printf, and allowing
        // everything that starts with printf would allow the rest of it.
        for dangerous in [
            "printf 'a' > a.txt; ls -l",
            "cd /tmp && rm -rf everything",
            "cat notes | mail somebody",
            "echo $(whoami)",
            "ls > /etc/passwd",
        ] {
            let said = what_always_means("Bash", Some(dangerous)).expect("something");
            assert_eq!(said.rule, dangerous, "{dangerous} was narrowed");
            assert_eq!(said.in_words, "only this exact command", "{dangerous}");
        }
    }

    #[test]
    fn running_something_as_somebody_else_is_not_narrowed_either() {
        // Allowing every `sudo` would be allowing everything there is.
        for it in ["sudo rm -rf /", "env FOO=1 rm -rf /", "command rm -rf /"] {
            let said = what_always_means("Bash", Some(it)).expect("something");
            assert_eq!(said.in_words, "only this exact command", "{it}");
        }
    }

    #[test]
    fn a_variable_in_front_of_a_command_is_not_the_command() {
        let said = what_always_means("Bash", Some("RUST_LOG=debug cargo test")).expect("something");
        assert_eq!(said.rule, "cargo");
    }

    #[test]
    fn a_rule_never_covers_a_word_it_only_begins() {
        // `starts_with` on its own says a rule of `top` covers
        // `topple-everything`, which nobody agreed to.
        assert!(!covers("top", "topple-everything"));
        assert!(covers("top", "top -l 1"));
        assert!(covers("top", "top"));

        // A path rule covers what is under it, and not a sibling that merely
        // begins the same way.
        assert!(covers("/Users/me/notes", "/Users/me/notes/today.txt"));
        assert!(!covers("/Users/me/notes", "/Users/me/notes-private"));
    }

    #[test]
    fn everything_that_is_not_a_shell_command_keeps_the_shape_the_engine_asked_for() {
        // A path glob for a file tool already covers a directory, and an
        // address prefix already covers a site. Narrowing those would be
        // inventing a rule nobody suggested.
        let said = what_always_means("Edit", Some("//Users/me/project/**")).expect("something");
        assert_eq!(said.rule, "//Users/me/project/**");
        assert!(said.in_words.contains("//Users/me/project/**"));
    }

    #[test]
    fn what_was_already_granted_says_how_much_it_really_covers() {
        // The list somebody reads to decide what to take back. A whole command
        // line stored as a rule looks like a permission and behaves like a
        // one-off, and nothing on the line says which.
        assert_eq!(
            in_words("Bash", "top -l 1 -n 15 -o mem"),
            "only this exact command"
        );
        assert_eq!(in_words("Bash", "top"), "any top command");
        assert_eq!(in_words("Bash", "vm_stat"), "any vm_stat command");
        assert_eq!(
            in_words("Bash", "printf 'a' > a.txt; ls -l"),
            "only this exact command"
        );
        assert_eq!(in_words("ask", ""), "anything this agent does with ask");
        assert_eq!(
            in_words("Edit", "//Users/me/**"),
            "anything starting //Users/me/**"
        );
    }

    #[test]
    fn a_rule_written_for_one_engine_answers_the_other() {
        // Written ahead in the window, a rule was kept as `Bash`, and a
        // teammate on a local model asks as `run_command`: nothing it asked
        // was ever answered by it.
        assert!(same_thing("Bash", "run_command"));
        assert!(same_thing("Bash", "start_command"));
        assert!(same_thing("run_command", "Bash"));
        assert!(same_thing("Write", "write_file"));
        assert!(same_thing("Edit", "change_file"));
        assert!(same_thing("WebFetch", "fetch_url"));
        // Different kinds stay different, and a tool that is none of these
        // answers only itself.
        assert!(!same_thing("Bash", "write_file"));
        assert!(!same_thing("Read", "Write"));
        assert!(same_thing("ask", "ask"));
        assert!(!same_thing("ask", "mcp__errand__ask"));
    }

    #[test]
    fn a_kind_is_found_by_the_windows_name_or_by_any_tools() {
        assert_eq!(Kind::called("commands"), Some(Kind::Commands));
        assert_eq!(Kind::called("folder"), Some(Kind::Folder));
        assert_eq!(Kind::called("Bash"), Some(Kind::Commands));
        assert_eq!(Kind::called("change_file"), Some(Kind::Changing));
        assert_eq!(Kind::called("ask"), None);
        // Kept under the names rules always had, so the old ones still read.
        for kind in Kind::ALL {
            assert_eq!(Kind::of(kind.kept_as()), Some(kind), "{kind:?}");
        }
    }

    #[test]
    fn what_a_kind_allows_is_said_as_what_it_is() {
        assert_eq!(in_words("Bash", ""), "running any command at all");
        assert_eq!(in_words("run_command", ""), "running any command at all");
        assert_eq!(in_words("run_command", "curl"), "any curl command");
        assert_eq!(
            in_words("Write", "/Users/me/Downloads"),
            "writing anything inside /Users/me/Downloads"
        );
        assert_eq!(
            in_words("Read", "/Volumes/Disk"),
            "reading anything inside /Volumes/Disk"
        );
        assert_eq!(
            in_words("WebFetch", "https://github.com"),
            "fetching anything from https://github.com"
        );
        assert_eq!(in_words("WebFetch", ""), "fetching any web page");
        assert!(!Kind::Folder.can_be_whole());
    }

    #[test]
    fn a_teammate_is_offered_only_what_it_would_ever_ask_about() {
        // Never asks: nothing to allow but somewhere else to write.
        let (kinds, why) = worth_offering(true, "auto");
        assert_eq!(kinds, vec![Kind::Folder]);
        assert!(why.is_some());
        assert_eq!(worth_offering(false, "auto").0, vec![Kind::Folder]);
        // A local model reads and fetches without asking, and is walled.
        let (kinds, _) = worth_offering(true, "ask");
        assert!(kinds.contains(&Kind::Commands) && kinds.contains(&Kind::Folder));
        assert!(!kinds.contains(&Kind::Reading) && !kinds.contains(&Kind::Fetching));
        // Claude asking is not walled, so a folder would change nothing.
        let (kinds, why) = worth_offering(false, "ask");
        assert!(!kinds.contains(&Kind::Folder));
        assert!(kinds.contains(&Kind::Fetching));
        assert!(why.is_none());
        // Writing without asking leaves writing out.
        assert!(!worth_offering(false, "edits").0.contains(&Kind::Writing));
    }

    #[test]
    fn logging_in_to_another_machine_is_found_and_allowed_one_machine_at_a_time() {
        let host = logs_in_to;
        assert_eq!(
            host("ssh studio df -h"),
            Some(("ssh".into(), "studio".into(), Some("df -h".into())))
        );
        assert_eq!(
            host("ssh -o BatchMode=yes -p 22 me@studio uptime").map(|h| h.1),
            Some("studio".into())
        );
        assert_eq!(host("ssh studio").map(|h| h.2), Some(None));
        assert_eq!(
            host("scp notes.txt studio:/tmp/").map(|h| h.1),
            Some("studio".into())
        );
        assert_eq!(
            host("rsync -a ./a/ me@studio:~/b/").map(|h| h.1),
            Some("studio".into())
        );
        assert_eq!(host("rsync -a ./a/ ./b/"), None);
        assert_eq!(host("df -h"), None);

        // Always, for a login, is that machine and not every machine.
        let said = what_always_means("Bash", Some("ssh studio df -h")).unwrap();
        assert_eq!(said.rule, "ssh studio");
        assert_eq!(said.in_words, "any ssh command to studio");
        assert_eq!(in_words("Bash", "ssh studio"), "any ssh command to studio");
        assert!(covers("ssh studio", "ssh studio df -h"));
        assert!(!covers("ssh studio", "ssh other df -h"));
        assert!(!covers("ssh studio", "ssh studiox df -h"));
    }

    #[test]
    fn nothing_to_remember_means_no_button() {
        // A button that quietly does nothing is worse than no button.
        assert_eq!(what_always_means("Bash", None), None);
    }

    #[test]
    fn a_tool_with_no_finer_rule_than_itself_can_still_be_allowed() {
        // The engine offers to allow the whole tool by suggesting a rule with
        // nothing in it, which is not the same as offering nothing -- and
        // briefly was, which took the button off every question of this kind.
        let said = what_always_means("ask", Some("")).expect("something to remember");
        assert_eq!(said.rule, "");
        assert_eq!(said.in_words, "anything this agent does with ask");
        // An empty rule covers everything, which is the point of it.
        assert!(covers(&said.rule, "whatever it likes"));
    }
}
