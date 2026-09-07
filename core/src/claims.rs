//! What an answer says it wrote, checked against the disk.
//!
//! A standing job that ran every five minutes wrote one file, then twenty-four,
//! then none while saying it had; and handed its own past runs, it reported a
//! file from half an hour earlier as one it had just written. Nothing in the
//! app disagreed, because nothing in the app looked: the answer was written
//! down as said, and a person reading "Pulse written" that evening had no way
//! to know the disk said otherwise.
//!
//! This is the cheapest look there is. When an answer names a path as written,
//! saved, created or exported, the path is stat'd at the moment the answer is
//! written down. Nothing is run, nothing is read back, and the answer itself is
//! not touched. What comes out is a line of the app's own in the conversation
//! saying only what was found: no such file, or a file older than the errand.
//!
//! Only where the agent may write. A path outside its own folder and the
//! folders allowed to it is not a claim it could have made good on, so it is
//! left alone: `/etc/hosts` in an explanation is an example, not a lie.
//!
//! Silence is the safe answer. Every rule below that skips something errs that
//! way, because a check that says nothing has cost nobody anything, and a line
//! saying "there is no such file" about a file that is there is a false
//! statement in the one place a person goes to find out what happened.

use std::path::{Component, Path, PathBuf};

use crate::store::Store;

/// The words that make a line a claim about the disk.
///
/// Past forms only. "I can save it to X" is an offer and "to save it, run Y"
/// is an instruction, and neither is anything to check.
const SAYS_IT_WROTE: &[&str] = &[
    "wrote",
    "written",
    "rewrote",
    "rewritten",
    "overwrote",
    "overwritten",
    "saved",
    "created",
    "exported",
];

/// Words that make a line with one of those in it something other than a
/// claim: a failure, a hypothetical, an example, or a file it says is gone.
///
/// Any of these anywhere on the line is enough. That skips "saved to `x`, not
/// a byte lost", which is a real claim, and that is the right way round: a
/// check that stays quiet has cost nothing, and "there is no such file" said
/// about a file the agent told us it deleted is a line nobody can act on.
const BUT_NOT_REALLY: &[&str] = &[
    "not", "never", "failed", "unable", "cannot", "could", "would", "should", "might", "example",
    "e.g", "deleted", "removed", "moved", "renamed",
];

/// Two words that together mean an example is coming.
const SUCH_AS: &str = "such as";

/// What makes the inside of a code span a command rather than a path.
///
/// A path with spaces in it is a real thing here: every agent's folder is under
/// `Application Support`. So spaces alone do not settle it. A redirect, a pipe,
/// a flag or a second path do: `/usr/bin/touch /tmp/x` is a command, and the
/// path in it is a path the agent says it ran something on, not one it says it
/// wrote.
const READS_AS_A_COMMAND: &[&str] = &[
    ">", "<", "|", ";", "&&", "$(", "`", " -", " /", " ~/", " ./", " ../",
];

/// Characters that make a path a placeholder rather than a place.
const A_PLACEHOLDER: &[char] = &['<', '>', '{', '}', '*', '?', '$', '|'];

/// Endings that make a dotted name a website rather than a file.
///
/// `techcrunch.com` in backticks has a dot and three letters after it, the
/// same shape as `notes.txt`, and was looked for in the agent's folder. A
/// list rather than a rule because there is no rule: `.md` is a file and
/// `.me` is a country.
const A_WEBSITE_ENDS_IN: &[&str] = &[
    "com", "org", "net", "io", "ai", "co", "dev", "app", "uk", "de", "fr", "eu", "us", "ca", "au",
    "edu", "gov", "info", "me", "ly", "to", "tv",
];

/// The most words a line can have and still be a heading over a list.
///
/// "Files written:" speaks for the names under it. "Pulse written and
/// confirmed. The write succeeded, and the listing shows the fresh file at
/// the top:" is a sentence that happens to end in a colon, and the listing
/// under it is the three newest files on the disk, two of them from earlier
/// runs. Read as a heading it made the previous run's file a claim, every
/// five minutes, on the very routine this module exists for.
const HEADING_WORDS: usize = 6;

/// How far a file's own time may sit before the turn began and still count as
/// this turn's work.
///
/// A FAT volume keeps times to two seconds and rounds down, and a network
/// volume stamps files with its own clock, which is not this Mac's. A minute
/// covers both and still catches what this exists for, which was half an hour.
pub const CLOCKS_MAY_DISAGREE_BY: i64 = 60_000;

/// Where an agent may write.
#[derive(Debug, Clone, Copy)]
pub struct Writable<'a> {
    /// Its own folder.
    pub own: &'a Path,
    /// Folders allowed to it on top of that.
    pub also: &'a [PathBuf],
    /// What `~` stands for, where that is known. Without it a `~/` path is
    /// left alone rather than guessed at.
    pub home: Option<&'a Path>,
}

impl Writable<'_> {
    /// Whether the agent may write at this path: inside its own folder or one
    /// allowed to it. The one test every place looked at has to pass, because
    /// the module says nothing outside those folders is looked at and for a
    /// while that was not so: a folder named on the line was searched whether
    /// or not the agent could write there, and the note reported the time of a
    /// file in `/etc`.
    fn allows(&self, path: &Path) -> bool {
        std::iter::once(self.own)
            .chain(self.also.iter().map(PathBuf::as_path))
            .any(|folder| path.starts_with(folder))
    }
}

/// A path an answer says it wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    /// The path as the answer named it.
    pub as_said: String,
    /// Where that would be on disk. One place for an absolute path. Several for
    /// a bare name: the agent's own folder, every folder allowed to it, and any
    /// folder named on the same line that it may write in, since "wrote
    /// `pulse.txt` to `/Volumes/Disk/`" is one claim about one file.
    pub could_be: Vec<PathBuf>,
    /// Whether the path was picked out of running prose rather than set apart
    /// in backticks or quotes. A bare word ends at a space, and a path with a
    /// space in it does not, so such a path may be only the front of the real
    /// one: `/Users/x/Library/Application` out of `Application Support`.
    pub from_a_bare_word: bool,
}

impl Claim {
    /// Whether the answer named it without saying which folder.
    pub fn is_a_bare_name(&self) -> bool {
        !Path::new(&self.as_said).is_absolute() && !self.as_said.starts_with("~/")
    }

    /// Whether the answer named a folder, with a slash on the end.
    pub fn is_a_folder(&self) -> bool {
        self.as_said.ends_with('/')
    }
}

/// What the disk says about a claim, where it disagrees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wrong {
    /// Nothing is there, anywhere it could be.
    Missing {
        as_said: String,
        /// Named without a folder, so it was looked for in the agent's own.
        bare: bool,
        /// Where else it was looked for, when it was bare: the folders
        /// allowed to the agent and the ones the line named. Said in the
        /// note, because "no such file in its folder" about a file the answer
        /// plainly said went to the disk read as the app looking in the wrong
        /// place.
        elsewhere: Vec<PathBuf>,
    },
    /// Something is there, and it was last changed before this turn began.
    /// The exact fault this module exists for.
    Older {
        as_said: String,
        changed_at: i64,
        began_at: i64,
    },
}

/// Every path the answer says it wrote, resolved to where it would be.
///
/// Pure: nothing here touches the disk, so what counts as a claim can be tested
/// without one.
pub fn claimed_written(said: &str, may: &Writable) -> Vec<Claim> {
    let mut claims: Vec<Claim> = Vec::new();
    let mut fenced = false;
    // A heading that ends in a colon speaks for the list under it: "Files
    // written:" and then three bare names, none of which repeats the verb.
    let mut the_heading_claims = false;
    for line in said.lines() {
        let line = line.trim();
        if line.starts_with("```") || line.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        // A fenced block is something shown, not something said.
        if fenced || line.is_empty() {
            continue;
        }
        let item = is_a_list_item(line);
        let says_so = says_it_wrote(line) && !but_not_really(line);
        if says_so && is_a_heading(line) {
            the_heading_claims = true;
        } else if !item {
            the_heading_claims = false;
        }
        let claiming = says_so || (item && the_heading_claims && !but_not_really(line));
        if !claiming {
            continue;
        }
        let named = paths_named_on(line);
        // The folders named on the line, for a bare name beside them to be
        // looked for in. Only the ones the agent may write in. A folder it may
        // not write in takes the bare names beside it with it: "Saved
        // `pf.conf` to `/etc/`" is about /etc, which is not looked at, and
        // looking for pf.conf in the agent's own folder instead would say
        // "no such file" about a file that may well be sitting in /etc.
        let mut folders: Vec<PathBuf> = Vec::new();
        let mut somewhere_it_may_not = false;
        for one in &named {
            if let Some(path) = absolute(&one.said, may) {
                match may.allows(&path) {
                    true => folders.push(path),
                    false => somewhere_it_may_not = true,
                }
            }
        }
        for one in &named {
            let Some(claim) = where_it_would_be(one, &folders, somewhere_it_may_not, may) else {
                continue;
            };
            match claims.iter_mut().find(|c| c.as_said == claim.as_said) {
                Some(already) => {
                    for place in claim.could_be {
                        if !already.could_be.contains(&place) {
                            already.could_be.push(place);
                        }
                    }
                    // Set apart anywhere is set apart: the backticked one is
                    // the whole path, whatever a bare word made of it.
                    already.from_a_bare_word &= claim.from_a_bare_word;
                }
                None => claims.push(claim),
            }
        }
    }
    claims
}

/// The claims the disk does not bear out.
///
/// `began_at` is when the turn started, in milliseconds since the epoch, where
/// that is known; without it only presence is checked.
pub fn not_borne_out(claims: &[Claim], began_at: Option<i64>) -> Vec<Wrong> {
    let mut wrong = Vec::new();
    for claim in claims {
        let found = claim
            .could_be
            .iter()
            .find_map(|place| std::fs::metadata(place).ok());
        match found {
            // A word cut off at a space, with the rest of the name sitting
            // right there in the folder, is not a missing file. It is the
            // front of one, and the one it is the front of may well be there.
            None if claim.from_a_bare_word && cut_off_at_a_space(&claim.could_be) => {}
            None => wrong.push(Wrong::Missing {
                as_said: claim.as_said.clone(),
                bare: claim.is_a_bare_name(),
                elsewhere: looked_in_besides_its_own(claim),
            }),
            // Only a file's time means anything. A folder's changes when a
            // name is added to it and not when a file in it is rewritten, so
            // a folder "written to" this turn can carry last week's time.
            Some(meta) if meta.is_file() => {
                let changed_at = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64);
                if let (Some(changed_at), Some(began_at)) = (changed_at, began_at) {
                    if changed_at < began_at - CLOCKS_MAY_DISAGREE_BY {
                        wrong.push(Wrong::Older {
                            as_said: claim.as_said.clone(),
                            changed_at,
                            began_at,
                        });
                    }
                }
            }
            Some(_) => {}
        }
    }
    wrong
}

/// Whether a path picked out of prose stops where the real name has a space.
///
/// One read of the folder above it, looking for a name that begins with the
/// word and a space. `/Volumes/Disk/My` is not a missing file when the folder
/// holds `My Notes`; it is the first word of one that is there.
fn cut_off_at_a_space(places: &[PathBuf]) -> bool {
    places.iter().any(|place| {
        let (Some(parent), Some(word)) = (place.parent(), place.file_name()) else {
            return false;
        };
        let front = format!("{} ", word.to_string_lossy());
        std::fs::read_dir(parent).is_ok_and(|entries| {
            entries
                .flatten()
                .any(|entry| entry.file_name().to_string_lossy().starts_with(&front))
        })
    })
}

/// The folders a bare name was looked for in, besides the agent's own.
///
/// Read back off the places rather than kept beside them: every place is a
/// folder with the name under it, so the folder is the place with the name's
/// own parts taken off the end. The first place is always the agent's own
/// folder, which the note calls by that name rather than by its path.
fn looked_in_besides_its_own(claim: &Claim) -> Vec<PathBuf> {
    if !claim.is_a_bare_name() {
        return Vec::new();
    }
    let Some(name) = tidy(Path::new(&claim.as_said)) else {
        return Vec::new();
    };
    let parts = name.components().count();
    let mut folders: Vec<PathBuf> = Vec::new();
    for place in claim.could_be.iter().skip(1) {
        if let Some(folder) = place.ancestors().nth(parts) {
            if !folders.iter().any(|f| f == folder) {
                folders.push(folder.to_path_buf());
            }
        }
    }
    folders
}

/// What to put in the conversation.
///
/// Facts, then what to do. The file's own time and the turn's are both said
/// rather than concluded from, because a disk whose clock is wrong makes the
/// conclusion wrong and leaves the two times true. And where it looked, for
/// a name the answer gave without a folder, because "no such file in its
/// folder" under an answer that said the disk read as the app having looked
/// in the wrong place.
pub fn what_to_say(wrong: &Wrong) -> String {
    let what_now = "Check before relying on that part of the answer, or ask it to do it again.";
    match wrong {
        Wrong::Missing {
            as_said,
            bare: true,
            elsewhere,
        } => {
            let what = a_file_or_a_folder(as_said);
            let places: Vec<String> = std::iter::once("in its folder".to_string())
                .chain(elsewhere.iter().map(|f| format!("in {}", f.display())))
                .collect();
            format!(
                "It says it wrote {as_said}, and there is no such {what} {}. {what_now}",
                one_or_the_other(&places)
            )
        }
        Wrong::Missing { as_said, .. } => {
            let what = a_file_or_a_folder(as_said);
            format!("It says it wrote {as_said}, and there is no such {what}. {what_now}")
        }
        Wrong::Older {
            as_said,
            changed_at,
            began_at,
        } => format!(
            "It says it wrote {as_said}, and that file was last changed at {}, before this \
             errand began at {}. {what_now}",
            at_the_time(*changed_at, *began_at),
            at_the_time(*began_at, *began_at)
        ),
    }
}

/// Everything wrong with what an answer claims, ready to be written down.
///
/// The agent's own folder and the folders allowed to it come from the store,
/// and so does when the turn began: the last thing somebody, or the clock, said
/// in the conversation.
pub fn what_is_not_so(store: &Store, conversation: &str, said: &str) -> Vec<String> {
    let Ok(Some(talk)) = store.conversation(conversation) else {
        return Vec::new();
    };
    let Ok(Some(agent)) = store.agent(&talk.agent) else {
        return Vec::new();
    };
    // Every path starts with the empty path, so an agent whose folder is not
    // known would be "allowed" everywhere. It has nowhere it may write, so
    // there is nothing to check.
    let own = Path::new(&agent.cwd);
    if !own.is_absolute() {
        return Vec::new();
    }
    let also: Vec<PathBuf> = store
        .folders_allowed(&agent.id)
        .unwrap_or_default()
        .into_iter()
        .filter(|folder| folder.is_absolute())
        .collect();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let may = Writable {
        own,
        also: &also,
        home: home.as_deref(),
    };
    let began_at = store.when_the_turn_began(conversation).ok().flatten();
    not_borne_out(&claimed_written(said, &may), began_at)
        .iter()
        .map(what_to_say)
        .collect()
}

/// "file", or "folder" for a path the answer wrote with a slash on the end.
fn a_file_or_a_folder(as_said: &str) -> &'static str {
    match as_said.ends_with('/') {
        true => "folder",
        false => "file",
    }
}

/// Places the way somebody would list them: "in A", "in A or in B", "in A,
/// in B or in C".
fn one_or_the_other(places: &[String]) -> String {
    match places {
        [] => String::new(),
        [one] => one.clone(),
        [front @ .., last] => format!("{} or {last}", front.join(", ")),
    }
}

/// A time as somebody would say it: the hour and minute, and the day only when
/// it is not the day being talked about.
fn at_the_time(ms: i64, on_the_day_of: i64) -> String {
    use chrono::{Datelike, Local, TimeZone};
    let (Some(then), Some(day)) = (
        Local.timestamp_millis_opt(ms).single(),
        Local.timestamp_millis_opt(on_the_day_of).single(),
    ) else {
        return format!("{ms} ms since 1970");
    };
    match then.date_naive() == day.date_naive() {
        true => then.format("%H:%M").to_string(),
        false => format!(
            "{} on {} {}",
            then.format("%H:%M"),
            then.day(),
            then.format("%B")
        ),
    }
}

/// Whether the line has one of the writing words on it, as a word.
fn says_it_wrote(line: &str) -> bool {
    words(line)
        .iter()
        .any(|w| SAYS_IT_WROTE.contains(&w.as_str()))
}

/// Whether the line takes its own claim back, or never made one.
fn but_not_really(line: &str) -> bool {
    line.to_lowercase().contains(SUCH_AS)
        || words(line)
            .iter()
            .any(|w| BUT_NOT_REALLY.contains(&w.as_str()) || w.ends_with("n't"))
}

/// The words on a line, lowercased and stripped of punctuation, with curly
/// apostrophes made straight so "wasn’t" is "wasn't".
fn words(line: &str) -> Vec<String> {
    line.split_whitespace()
        .map(|w| {
            w.to_lowercase()
                .replace('\u{2019}', "'")
                .trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '.')
                .trim_end_matches('.')
                .to_string()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

fn is_a_list_item(line: &str) -> bool {
    let mut chars = line.chars();
    match chars.next() {
        Some('-' | '*' | '+') => chars.next().is_some_and(char::is_whitespace),
        Some(d) if d.is_ascii_digit() => {
            let rest = line.trim_start_matches(|c: char| c.is_ascii_digit());
            rest.starts_with(". ") || rest.starts_with(") ")
        }
        _ => false,
    }
}

/// Whether the line is a heading over a list rather than a sentence: it ends
/// in a colon once markdown's bold and italic marks are taken off, so
/// "**Files written:**" counts, and it is short and has no sentence ending
/// inside it, so "Pulse written and confirmed. The listing shows the fresh
/// file at the top:" does not.
fn is_a_heading(line: &str) -> bool {
    line.trim_end_matches(['*', '_', ' ']).ends_with(':')
        && words(line).len() <= HEADING_WORDS
        && !line.contains(". ")
        && !line.contains("! ")
        && !line.contains("? ")
}

/// A path as the answer wrote it, and how it was written.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Named {
    said: String,
    /// Picked out of prose, where it ended at the first space.
    a_bare_word: bool,
}

/// Every path-shaped thing on a line, as the answer wrote it.
///
/// Code spans and quoted strings are taken out before bare words are looked
/// at, or a path inside a command the agent ran would be found again as a
/// bare word, and a quoted path with spaces in it would be found in pieces.
fn paths_named_on(line: &str) -> Vec<Named> {
    let mut named = Vec::new();
    let mut rest = String::new();
    let set_apart = |said: String| Named {
        said,
        a_bare_word: false,
    };
    // In a code span, spaces mean a command unless the span starts like a
    // path: `cat notes.txt` is not a file called that. In quotes they mean a
    // name, which is what quotes are for: "Weekly notes.md" is one file.
    let (spans, left) = split_out(line, '`', '`');
    for span in spans {
        if looks_like_a_path(&span, false) {
            named.push(set_apart(span));
        }
    }
    for piece in left {
        let (quoted, left) = split_out(&piece, '"', '"');
        for one in quoted {
            if looks_like_a_path(&one, true) {
                named.push(set_apart(one));
            }
        }
        let (curly, left) = split_out(&left.join(" "), '\u{201c}', '\u{201d}');
        for one in curly {
            if looks_like_a_path(&one, true) {
                named.push(set_apart(one));
            }
        }
        rest.push_str(&left.join(" "));
        rest.push(' ');
    }
    for word in rest.split_whitespace() {
        let word = word
            .trim_start_matches(['(', '[', '"', '\'', '*', '_'])
            .trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']', '"', '\'', '*', '_']);
        // Only a path that says where it starts. A bare name without its
        // backticks is indistinguishable from a version number or a website.
        if (word.starts_with('/') || word.starts_with("~/")) && looks_like_a_path(word, false) {
            named.push(Named {
                said: word.to_string(),
                a_bare_word: true,
            });
        }
    }
    named
}

/// The pieces between pairs of `open` and `close`, and what is left outside
/// them. An unmatched opener is left where it is.
fn split_out(line: &str, open: char, close: char) -> (Vec<String>, Vec<String>) {
    let mut inside = Vec::new();
    let mut outside = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find(open) {
        let after = &rest[start + open.len_utf8()..];
        let Some(end) = after.find(close) else {
            break;
        };
        outside.push(rest[..start].to_string());
        inside.push(after[..end].trim().to_string());
        rest = &after[end + close.len_utf8()..];
    }
    outside.push(rest.to_string());
    (inside, outside)
}

/// Whether something the answer set apart reads as a path and not as a
/// command, an address or a placeholder.
///
/// `in_quotes` is for a name in quotes, where a space is part of the name and
/// a slash is more often prose than a folder: an "on/off" toggle, a
/// "write/delete pulse loop". So a quoted name has to carry an extension. In
/// backticks a bare name with a space in it is a command, and a slash is a
/// folder.
fn looks_like_a_path(said: &str, in_quotes: bool) -> bool {
    if said.is_empty()
        || said.contains(char::is_control)
        || said.contains(A_PLACEHOLDER)
        || said.contains("://")
        || said.contains('@')
    {
        return false;
    }
    if starts_like_a_path(said) {
        return !READS_AS_A_COMMAND.iter().any(|sign| said.contains(sign));
    }
    if is_a_website(said) {
        return false;
    }
    // A bare name, with a folder in it or an extension on it.
    match in_quotes {
        true => has_an_extension(said),
        false => {
            !said.contains(char::is_whitespace)
                && !READS_AS_A_COMMAND.iter().any(|sign| said.contains(sign))
                && (said.contains('/') || has_an_extension(said))
        }
    }
}

/// Whether a bare name is a website: its first part ends the way a domain
/// does. `techcrunch.com` and `example.com/x` are addresses, and an address
/// is not a file the agent wrote.
fn is_a_website(said: &str) -> bool {
    let host = said.split('/').next().unwrap_or(said);
    host.rsplit_once('.').is_some_and(|(_, ending)| {
        A_WEBSITE_ENDS_IN.contains(&ending.to_ascii_lowercase().as_str())
    })
}

fn starts_like_a_path(said: &str) -> bool {
    said.starts_with('/')
        || said.starts_with("~/")
        || said.starts_with("./")
        || said.starts_with("../")
}

/// A dot with one to eight letters or digits after it, at the end of the last
/// part of the name. `notes.txt` and `.env` have one; `e.g.`, `v1` and `v1.2`
/// do not, and neither does `techcrunch.com`: digits alone are a version, and
/// a domain's ending is a domain's.
fn has_an_extension(name: &str) -> bool {
    let last = name.rsplit('/').next().unwrap_or(name);
    let Some((_, ext)) = last.rsplit_once('.') else {
        return false;
    };
    !ext.is_empty()
        && ext.chars().count() <= 8
        && ext.chars().all(char::is_alphanumeric)
        && !ext.chars().all(|c| c.is_ascii_digit())
        && !A_WEBSITE_ENDS_IN.contains(&ext.to_ascii_lowercase().as_str())
}

/// The absolute path this names, tidied, whoever may write there.
fn absolute(said: &str, may: &Writable) -> Option<PathBuf> {
    let path = match said.strip_prefix("~/") {
        Some(under) => may.home?.join(under),
        None => PathBuf::from(said),
    };
    match path.is_absolute() {
        true => tidy(&path),
        false => None,
    }
}

/// Where a named path would be, if it is one the agent may write.
///
/// `somewhere_it_may_not` says the line also named a folder the agent may not
/// write in, which makes a bare name on it no claim at all: the name belongs
/// to that folder, and that folder is not looked at.
fn where_it_would_be(
    named: &Named,
    folders_on_the_line: &[PathBuf],
    somewhere_it_may_not: bool,
    may: &Writable,
) -> Option<Claim> {
    let said = named.said.as_str();
    if let Some(path) = absolute(said, may) {
        return may.allows(&path).then(|| Claim {
            as_said: said.to_string(),
            could_be: vec![path],
            from_a_bare_word: named.a_bare_word,
        });
    }
    // Something like `~x` or a name that is not a path at all.
    if said.starts_with('~') || somewhere_it_may_not {
        return None;
    }
    // A bare name. Not one that climbs out: `../x` is somewhere it may not
    // write, and a relative path that reaches over the top is not a claim
    // about its folder.
    let rel = tidy(Path::new(said))?;
    let mut could_be = vec![may.own.join(&rel)];
    for folder in may.also {
        could_be.push(folder.join(&rel));
    }
    for folder in folders_on_the_line {
        could_be.push(folder.join(&rel));
        // "To /Volumes/Disk/notes" could name the folder or a file in it, so
        // the name is looked for beside it too. Only where that is still
        // somewhere it may write: beside an allowed folder is usually not.
        if let Some(up) = folder.parent() {
            could_be.push(up.join(&rel));
        }
    }
    could_be.retain(|place| may.allows(place));
    could_be.dedup();
    Some(Claim {
        as_said: said.to_string(),
        could_be,
        from_a_bare_word: named.a_bare_word,
    })
}

/// A path with `.` and `..` folded away, lexically. Nothing if a relative one
/// climbs above where it started.
fn tidy(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    let mut depth: usize = 0;
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if depth == 0 {
                    return None;
                }
                out.pop();
                depth -= 1;
            }
            Component::RootDir | Component::Prefix(_) => out.push(part),
            Component::Normal(name) => {
                out.push(name);
                depth += 1;
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn may<'a>(own: &'a Path, also: &'a [PathBuf]) -> Writable<'a> {
        Writable {
            own,
            also,
            home: Some(Path::new("/Users/somebody")),
        }
    }

    fn one_path(said: &str, own: &Path, also: &[PathBuf]) -> Vec<String> {
        claimed_written(said, &may(own, also))
            .into_iter()
            .map(|c| c.as_said)
            .collect()
    }

    fn scratch(named: &str) -> PathBuf {
        let at = std::env::temp_dir().join(format!("errand-claims-{named}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&at);
        std::fs::create_dir_all(&at).unwrap();
        at
    }

    fn set_the_time(file: &Path, ms: i64) {
        let at = std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms as u64);
        let f = std::fs::File::options().write(true).open(file).unwrap();
        f.set_modified(at).unwrap();
    }

    #[test]
    fn a_path_in_a_code_span_on_a_line_that_says_it_wrote_it_is_a_claim() {
        let own = Path::new("/Users/somebody/Library/Application Support/Errand/threads/one");
        let said = "Done. I wrote the summary to `/Users/somebody/Library/Application Support/Errand/threads/one/summary.md` and it is 2 KB.";
        let claims = claimed_written(said, &may(own, &[]));
        assert_eq!(claims.len(), 1, "{claims:?}");
        assert_eq!(claims[0].could_be, vec![own.join("summary.md")]);
        // The spaces in the agent's own folder are the ordinary case, not an
        // edge: every agent lives under Application Support.
        assert!(!claims[0].is_a_bare_name());
    }

    #[test]
    fn trailing_punctuation_belongs_to_the_sentence_and_not_to_the_path() {
        let own = Path::new("/work");
        for said in [
            "Saved to /work/out/report.pdf.",
            "Saved to /work/out/report.pdf, as asked.",
            "Saved (at /work/out/report.pdf).",
            "Saved to `/work/out/report.pdf`!",
            "Exported: **/work/out/report.pdf**",
            "I created \"/work/out/report.pdf\" for you.",
            "Written to \u{201c}/work/out/report.pdf\u{201d}.",
        ] {
            let claims = claimed_written(said, &may(own, &[]));
            assert_eq!(claims.len(), 1, "{said:?} gave {claims:?}");
            assert_eq!(
                claims[0].could_be,
                vec![PathBuf::from("/work/out/report.pdf")],
                "{said:?}"
            );
        }
    }

    #[test]
    fn a_path_with_spaces_inside_a_code_span_is_one_path() {
        let own = Path::new("/Users/somebody/My Errands/one");
        let said = "Saved as `/Users/somebody/My Errands/one/Weekly notes.md`.";
        let claims = claimed_written(said, &may(own, &[]));
        assert_eq!(
            claims,
            vec![Claim {
                as_said: "/Users/somebody/My Errands/one/Weekly notes.md".to_string(),
                could_be: vec![own.join("Weekly notes.md")],
                from_a_bare_word: false,
            }]
        );
        // And in quotes, which is the other way a path with spaces survives.
        let said = "Saved as \"Weekly notes.md\" in your folder.";
        let claims = claimed_written(said, &may(own, &[]));
        assert_eq!(claims.len(), 1, "{claims:?}");
        assert_eq!(claims[0].could_be[0], own.join("Weekly notes.md"));
        // In backticks, a bare name with a space in it is a command, not a
        // file called that.
        assert!(claimed_written("Saved with `cat notes.txt`.", &may(own, &[])).is_empty());
    }

    #[test]
    fn a_bare_name_is_looked_for_in_its_folder_and_everywhere_else_the_line_names() {
        let own = Path::new("/threads/one");
        let also = vec![PathBuf::from("/Volumes/Disk")];
        // The real shape of a routine's answer: the name in one span and the
        // folder in another, with a trailing slash.
        let said = "- Wrote: `errand-pulse-20260905-162041.txt` (51 bytes) to `/Volumes/Disk/`";
        let claims = claimed_written(said, &may(own, &also));
        let file = claims
            .iter()
            .find(|c| c.as_said == "errand-pulse-20260905-162041.txt")
            .expect("the bare name is a claim");
        assert!(file.is_a_bare_name());
        assert!(file.could_be.contains(&PathBuf::from(
            "/threads/one/errand-pulse-20260905-162041.txt"
        )));
        assert!(file.could_be.contains(&PathBuf::from(
            "/Volumes/Disk/errand-pulse-20260905-162041.txt"
        )));
        // The folder itself is a claim too, and one about a folder it may
        // write in.
        assert!(claims.iter().any(|c| c.as_said == "/Volumes/Disk/"));

        // Without the trailing slash the folder is still the folder, and a
        // name "in" it is looked for both in it and beside it, because "to
        // /Volumes/Disk/notes" could name the folder or a file in it.
        let said = "Saved `today.md` to `/Volumes/Disk/notes`.";
        let claims = claimed_written(said, &may(own, &also));
        let file = claims.iter().find(|c| c.as_said == "today.md").unwrap();
        assert!(file
            .could_be
            .contains(&PathBuf::from("/Volumes/Disk/notes/today.md")));
        assert!(file
            .could_be
            .contains(&PathBuf::from("/Volumes/Disk/today.md")));

        // Relative paths with folders in them, and `./`, resolve the same way.
        assert_eq!(
            claimed_written("Created `./notes/today.md`.", &may(own, &[]))[0].could_be,
            vec![PathBuf::from("/threads/one/notes/today.md")]
        );
    }

    #[test]
    fn a_path_that_only_appears_inside_a_command_it_ran_is_not_a_claim() {
        let own = Path::new("/threads/one");
        for said in [
            "I ran `echo pulse > /threads/one/pulse.txt` and it wrote the file.",
            "Written with `/usr/bin/touch /threads/one/pulse.txt`.",
            "Saved by running `cp -p notes.md /threads/one/backup.md`.",
            "Created with `tee /threads/one/pulse.txt | cat`.",
            "Wrote it like this:\n```sh\nprintf hi > /threads/one/pulse.txt\n```\nDone.",
            "Exported with `/threads/one/export.sh --all`.",
        ] {
            let claims = claimed_written(said, &may(own, &[]));
            assert!(claims.is_empty(), "{said:?} gave {claims:?}");
        }
    }

    #[test]
    fn a_path_outside_where_it_may_write_is_not_a_claim() {
        let own = Path::new("/threads/one");
        let also = vec![PathBuf::from("/Volumes/Disk")];
        for said in [
            "Saved to /Users/somebody/Desktop/report.pdf.",
            "Created `/etc/hosts` the way you would.",
            "Wrote `/Volumes/Other/x.txt`.",
            "Saved it under `~/Documents/x.txt`.",
            "Written to `/threads/one/../two/x.txt`.",
            "Wrote `../two/x.txt`.",
        ] {
            let claims = claimed_written(said, &may(own, &also));
            assert!(claims.is_empty(), "{said:?} gave {claims:?}");
        }
        // Inside the allowed folder, through the same door, it is one.
        assert_eq!(
            one_path("Saved to `/Volumes/Disk/x.txt`.", own, &also),
            vec!["/Volumes/Disk/x.txt"]
        );
        // And a tilde path is the home folder, so it can be inside.
        let home_own = Path::new("/Users/somebody/errands/one");
        assert_eq!(
            claimed_written("Saved to `~/errands/one/x.txt`.", &may(home_own, &[]))[0].could_be,
            vec![home_own.join("x.txt")]
        );
    }

    #[test]
    fn an_offer_an_instruction_or_a_failure_is_not_a_claim() {
        let own = Path::new("/threads/one");
        for said in [
            "I can save it to `/threads/one/x.txt` if you like.",
            "To save it, write `/threads/one/x.txt` yourself.",
            "That would have created `/threads/one/x.txt`.",
            "It should be saved as `/threads/one/x.txt` next run.",
            "Failed to write `/threads/one/x.txt`: Operation not permitted.",
            "I couldn't save `/threads/one/x.txt`.",
            "I didn\u{2019}t write `/threads/one/x.txt` this time.",
            "The file was not written to `/threads/one/x.txt`.",
            "Created `/threads/one/tmp.txt`, used it, and deleted it.",
            "A file such as `/threads/one/x.txt` would be created.",
            "For example, `/threads/one/x.txt` gets created.",
            "e.g. `/threads/one/x.txt` is created.",
            "The file at `/threads/one/x.txt` is the newest one.",
            "Save it as `/threads/one/x.txt`.",
        ] {
            let claims = claimed_written(said, &may(own, &[]));
            assert!(claims.is_empty(), "{said:?} gave {claims:?}");
        }
    }

    #[test]
    fn a_heading_that_ends_in_a_colon_speaks_for_the_list_under_it() {
        let own = Path::new("/threads/one");
        let said = "**Files written:**\n\n- `a.txt`\n- `notes/b.md`\n* `/threads/one/c.txt`\n\nThe rest was left as it was: `d.txt`.";
        assert_eq!(
            one_path(said, own, &[]),
            vec!["a.txt", "notes/b.md", "/threads/one/c.txt"]
        );
        // A list whose heading does not claim anything claims nothing.
        assert!(one_path("Files read:\n- `a.txt`\n- `b.txt`", own, &[]).is_empty());
        // A numbered list is a list.
        assert_eq!(
            one_path("Saved:\n1. `a.txt`\n2. `b.txt`", own, &[]),
            vec!["a.txt", "b.txt"]
        );
        // And an item that takes its own claim back is left out on its own.
        assert_eq!(
            one_path("Created:\n- `a.txt`\n- `b.txt`, not yet", own, &[]),
            vec!["a.txt"]
        );
        // A heading is a label, not a sentence. A sentence with a writing
        // word in it that happens to end in a colon introduces whatever comes
        // next, which here is a listing and not a list of what was written.
        for said in [
            "Pulse written and confirmed. The listing shows the fresh file at the top:\n- `a.txt`\n- `b.txt`",
            "I wrote the file and the folder now looks like this:\n- `a.txt`\n- `b.txt`",
            "Written! Here is the folder:\n- `a.txt`",
        ] {
            assert!(one_path(said, own, &[]).is_empty(), "{said:?}");
        }
        // Short and unbroken, it is a heading however it is dressed.
        assert_eq!(
            one_path("Files I created for you:\n- `a.txt`", own, &[]),
            vec!["a.txt"]
        );
    }

    #[test]
    fn a_sentence_that_happens_to_end_in_a_colon_is_not_a_heading_over_the_list_under_it() {
        // Pulse Keeper's real answer at 06:41 on 7 September, with the disk
        // standing in for /Volumes/870EVO: it wrote the newest file, listed the
        // three newest, and the check said the oldest of the three was one it
        // claimed to have written and was from before the turn. Every five
        // minutes, on the routine this module exists for.
        let own = scratch("colon-own");
        let disk = scratch("colon-disk");
        let began_at = 1_788_777_714_511;
        for (name, minutes_before) in [
            ("errand-pulse-20260907-064201.txt", 0),
            ("errand-pulse-20260907-064157.txt", 0),
            ("errand-pulse-20260907-063657.txt", 5),
        ] {
            let file = disk.join(name);
            std::fs::write(&file, "pulse").unwrap();
            set_the_time(&file, began_at - minutes_before * 60 * 1000 + 10_000);
        }
        let said = "Pulse written and confirmed. The write succeeded with no permission error, and the listing shows the fresh file at the top:\n\n- `errand-pulse-20260907-064201.txt` (51 bytes, 06:42)\n- `errand-pulse-20260907-064157.txt` (51 bytes, 06:41)\n- `errand-pulse-20260907-063657.txt` (51 bytes, 06:36)\n\nThe volume is accessible and files are landing on the usual ~5-minute cadence, each ~51 bytes.";
        let also = vec![disk.clone()];
        let claims = claimed_written(said, &may(&own, &also));
        assert!(claims.is_empty(), "{claims:?}");
        assert!(not_borne_out(&claims, Some(began_at)).is_empty());
        for at in [own, disk] {
            let _ = std::fs::remove_dir_all(at);
        }
    }

    #[test]
    fn a_folder_named_on_the_line_that_it_may_not_write_in_is_not_looked_in() {
        // "Saved `pf.conf` to `/etc/`" used to look in /etc and its parent,
        // and the note then carried /etc/pf.conf's real modification time.
        // That folder is not somewhere the agent may write, so the line is
        // about something this cannot check, and it says nothing: looking in
        // the agent's own folder instead would say "no such file" about a
        // file that may well be in /etc.
        let own = Path::new("/threads/one");
        let also = vec![PathBuf::from("/Volumes/Disk")];
        for said in [
            "Saved `pf.conf` to `/etc/`.",
            "Saved `x.txt` to `~/Desktop/`.",
            "Wrote `config.json` to `/Users/somebody/.ssh/`.",
        ] {
            let claims = claimed_written(said, &may(own, &also));
            assert!(claims.is_empty(), "{said:?} gave {claims:?}");
        }
        // Named beside a folder it may write in, it is looked for there and
        // nowhere outside: not beside the disk, which is /Volumes.
        let claims = claimed_written("Saved `x.txt` to `/Volumes/Disk/`.", &may(own, &also));
        let file = claims.iter().find(|c| c.as_said == "x.txt").unwrap();
        assert!(
            file.could_be
                .iter()
                .all(|place| place.starts_with(own) || place.starts_with("/Volumes/Disk")),
            "{:?}",
            file.could_be
        );
        assert!(!file.could_be.contains(&PathBuf::from("/Volumes/x.txt")));
    }

    #[test]
    fn a_bare_path_with_a_space_in_it_is_not_reported_missing_at_the_space() {
        // Every agent's folder is under "Application Support", and a path
        // written without backticks ends at that space. The front half is not
        // a missing file; it is the first word of a folder that is there.
        let root = scratch("space");
        let own = root.join("Application Support/Errand/threads/x");
        std::fs::create_dir_all(&own).unwrap();
        std::fs::write(own.join("out.txt"), "x").unwrap();
        let also = vec![root.clone()];
        let said = format!("Saved to {}/out.txt.", own.display());
        let claims = claimed_written(&said, &may(&own, &also));
        assert_eq!(claims.len(), 1, "{claims:?}");
        assert!(claims[0].from_a_bare_word);
        assert_eq!(claims[0].as_said, format!("{}/Application", root.display()));
        assert!(not_borne_out(&claims, None).is_empty());
        // The same path in backticks is whole, and is the file.
        let said = format!("Saved to `{}/out.txt`.", own.display());
        let claims = claimed_written(&said, &may(&own, &also));
        assert_eq!(claims[0].could_be, vec![own.join("out.txt")]);
        assert!(!claims[0].from_a_bare_word);
        // A bare word that really is missing is still missing.
        let said = format!("Saved to {}/gone.txt.", root.display());
        let claims = claimed_written(&said, &may(&own, &also));
        assert_eq!(not_borne_out(&claims, None).len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_quoted_phrase_with_a_slash_in_it_is_not_a_file() {
        // Pulse Keeper's own words, at seq 17 of its first conversation: the
        // check said there was no such file as "write/delete pulse loop".
        let own = Path::new("/threads/one");
        let said = "This is an established \"write/delete pulse loop\" \u{2014} a prior run wrote pulse files to the volume every ~5 minutes and left receipts (last one 16:55).";
        assert!(one_path(said, own, &[]).is_empty());
        assert!(one_path("Wrote the \"on/off\" toggle into settings.", own, &[]).is_empty());
        // A quoted name with an extension is still a file, slash or not.
        assert_eq!(
            one_path(
                "Wrote \"notes/today.md\" and \"Weekly notes.md\".",
                own,
                &[]
            ),
            vec!["notes/today.md", "Weekly notes.md"]
        );
    }

    #[test]
    fn a_website_or_a_version_number_in_backticks_is_not_a_file() {
        let own = scratch("website");
        std::fs::write(own.join("article.md"), "x").unwrap();
        for said in [
            "I saved the article from `techcrunch.com` as `article.md`.",
            "Saved `v1.2` of the notes as `article.md`.",
            "Exported `example.com/x` to `article.md`.",
        ] {
            assert_eq!(one_path(said, &own, &[]), vec!["article.md"], "{said:?}");
        }
        let _ = std::fs::remove_dir_all(&own);
    }

    #[test]
    fn the_note_about_a_missing_file_says_where_it_looked_and_calls_a_folder_a_folder() {
        let own = scratch("where-own");
        let disk = scratch("where-disk");
        let also = vec![disk.clone()];
        // Named beside the disk, it was looked for in the agent's folder, on
        // the disk, and nowhere else the sentence can name.
        let said = format!("Wrote `pulse.txt` to `{}/`.", disk.display());
        let claims = claimed_written(&said, &may(&own, &also));
        let wrong = not_borne_out(&claims, None);
        let about_the_file = wrong
            .iter()
            .map(what_to_say)
            .find(|line| line.contains("pulse.txt"))
            .expect("the file is missing");
        assert_eq!(
            about_the_file,
            format!(
                "It says it wrote pulse.txt, and there is no such file in its folder or in {}. \
                 Check before relying on that part of the answer, or ask it to do it again.",
                disk.display()
            )
        );
        // Two more places read as a list.
        let other = scratch("where-other");
        let also = vec![disk.clone(), other.clone()];
        let claims = claimed_written("Wrote `pulse.txt`.", &may(&own, &also));
        let said = what_to_say(&not_borne_out(&claims, None)[0]);
        assert!(
            said.contains(&format!(
                "no such file in its folder, in {} or in {}.",
                disk.display(),
                other.display()
            )),
            "{said}"
        );
        // A folder is called a folder.
        let gone = own.join("out/");
        let claims = claimed_written(
            &format!("Saved everything to `{}`.", gone.display()),
            &may(&own, &[]),
        );
        assert_eq!(
            what_to_say(&not_borne_out(&claims, None)[0]),
            format!(
                "It says it wrote {}, and there is no such folder. Check before relying on that \
                 part of the answer, or ask it to do it again.",
                gone.display()
            )
        );
        for at in [own, disk, other] {
            let _ = std::fs::remove_dir_all(at);
        }
    }

    #[test]
    fn a_placeholder_a_version_or_an_address_is_not_a_path() {
        let own = Path::new("/threads/one");
        for said in [
            "Saved to `/threads/one/<name>.txt`.",
            "Created `/threads/one/{date}.md`.",
            "Wrote `/threads/one/*.txt`.",
            "Saved to https://example.com/threads/one/x.txt.",
            "Created v1.2 and saved it.",
            "Exported to example.com/x.",
            "Wrote to `x`.",
            "Saved `README`.",
        ] {
            let claims = claimed_written(said, &may(own, &[]));
            assert!(claims.is_empty(), "{said:?} gave {claims:?}");
        }
        // But a bare name with an extension, in backticks, is one.
        assert_eq!(one_path("Saved `.env`.", own, &[]), vec![".env"]);
        assert_eq!(one_path("Saved `notes.txt`.", own, &[]), vec!["notes.txt"]);
    }

    #[test]
    fn the_same_path_named_twice_is_one_claim() {
        let own = Path::new("/threads/one");
        let said = "Wrote `/threads/one/x.txt`.\n\nSaved `/threads/one/x.txt` again after the fix.";
        assert_eq!(one_path(said, own, &[]), vec!["/threads/one/x.txt"]);
    }

    #[test]
    fn a_missing_file_is_missing_and_a_present_one_is_not() {
        let own = scratch("missing");
        std::fs::write(own.join("there.txt"), "x").unwrap();
        let said = "Saved `there.txt` and `gone.txt`.";
        let claims = claimed_written(said, &may(&own, &[]));
        assert_eq!(claims.len(), 2);
        let wrong = not_borne_out(&claims, None);
        assert_eq!(
            wrong,
            vec![Wrong::Missing {
                as_said: "gone.txt".to_string(),
                bare: true,
                elsewhere: Vec::new(),
            }]
        );
        assert_eq!(
            what_to_say(&wrong[0]),
            "It says it wrote gone.txt, and there is no such file in its folder. Check before \
             relying on that part of the answer, or ask it to do it again."
        );
        let absolute = own.join("gone.txt");
        let claims = claimed_written(&format!("Saved `{}`.", absolute.display()), &may(&own, &[]));
        let wrong = not_borne_out(&claims, None);
        assert_eq!(
            what_to_say(&wrong[0]),
            format!(
                "It says it wrote {}, and there is no such file. Check before relying on that \
                 part of the answer, or ask it to do it again.",
                absolute.display()
            )
        );
        let _ = std::fs::remove_dir_all(&own);
    }

    #[test]
    fn a_file_older_than_the_turn_is_said_to_be_older_with_both_times() {
        // The fault this exists for: a routine handed its own past runs read
        // them as this run and reported a file from half an hour earlier as
        // one it had just written.
        let own = scratch("older");
        let file = own.join("pulse.txt");
        std::fs::write(&file, "pulse").unwrap();
        let began_at: i64 = 1_800_000_000_000;
        set_the_time(&file, began_at - 30 * 60 * 1000);
        let claims = claimed_written("Pulse written to `pulse.txt`.", &may(&own, &[]));
        let wrong = not_borne_out(&claims, Some(began_at));
        assert_eq!(
            wrong,
            vec![Wrong::Older {
                as_said: "pulse.txt".to_string(),
                changed_at: began_at - 30 * 60 * 1000,
                began_at,
            }]
        );
        let said = what_to_say(&wrong[0]);
        let expect = format!(
            "It says it wrote pulse.txt, and that file was last changed at {}, before this \
             errand began at {}. Check before relying on that part of the answer, or ask it \
             to do it again.",
            at_the_time(began_at - 30 * 60 * 1000, began_at),
            at_the_time(began_at, began_at)
        );
        assert_eq!(said, expect);
        // Both times are on the line, so it is a statement of fact whatever
        // the disk's clock says.
        assert!(said.contains("last changed at"), "{said}");

        // A file written this turn passes, and so does one a few seconds
        // "before", because a FAT volume rounds times down.
        set_the_time(&file, began_at - 10_000);
        assert!(not_borne_out(&claims, Some(began_at)).is_empty());
        set_the_time(&file, began_at + 5_000);
        assert!(not_borne_out(&claims, Some(began_at)).is_empty());
        // And with no idea when the turn began, presence is all that is asked.
        set_the_time(&file, began_at - 30 * 60 * 1000);
        assert!(not_borne_out(&claims, None).is_empty());
        let _ = std::fs::remove_dir_all(&own);
    }

    #[test]
    fn a_folder_is_only_checked_for_being_there() {
        // A folder's time changes when a name is added and not when a file in
        // it is rewritten, so an old time on a folder written to says nothing.
        let own = scratch("folder");
        let sub = own.join("out");
        std::fs::create_dir_all(&sub).unwrap();
        let began_at: i64 = 1_800_000_000_000;
        let dir = std::fs::File::open(&sub).unwrap();
        dir.set_modified(
            std::time::UNIX_EPOCH + std::time::Duration::from_millis((began_at - 3_600_000) as u64),
        )
        .unwrap();
        let claims = claimed_written(
            &format!("Saved everything to `{}/`.", sub.display()),
            &may(&own, &[]),
        );
        assert_eq!(claims.len(), 1, "{claims:?}");
        assert!(not_borne_out(&claims, Some(began_at)).is_empty());
        let _ = std::fs::remove_dir_all(&own);
    }

    #[test]
    fn a_bare_name_found_in_an_allowed_folder_is_not_missing() {
        let own = scratch("own");
        let disk = scratch("disk");
        std::fs::write(disk.join("pulse.txt"), "x").unwrap();
        let also = vec![disk.clone()];
        let claims = claimed_written("Wrote `pulse.txt` to the disk.", &may(&own, &also));
        assert!(not_borne_out(&claims, None).is_empty());
        // Named beside a folder the agent may not write in, the name is that
        // folder's and not a claim about its own: the point is not to say "no
        // such file" about a file that is there, and that folder is not
        // looked in.
        let elsewhere = scratch("elsewhere");
        std::fs::write(elsewhere.join("other.txt"), "x").unwrap();
        let said = format!("Wrote `other.txt` to `{}`.", elsewhere.display());
        let claims = claimed_written(&said, &may(&own, &[]));
        assert!(claims.is_empty(), "{claims:?}");
        for at in [own, disk, elsewhere] {
            let _ = std::fs::remove_dir_all(at);
        }
    }

    #[test]
    fn a_days_time_is_the_hour_and_another_days_time_says_the_day() {
        use chrono::{Datelike, Local, TimeZone};
        let began_at: i64 = 1_800_000_000_000;
        let same_day = at_the_time(began_at - 30 * 60 * 1000, began_at);
        assert_eq!(same_day.len(), 5, "{same_day}");
        assert!(!same_day.contains(" on "), "{same_day}");
        let yesterday = at_the_time(began_at - 24 * 60 * 60 * 1000, began_at);
        let then = Local
            .timestamp_millis_opt(began_at - 24 * 60 * 60 * 1000)
            .unwrap();
        assert_eq!(
            yesterday,
            format!(
                "{} on {} {}",
                then.format("%H:%M"),
                then.day(),
                then.format("%B")
            )
        );
    }

    #[test]
    fn checked_against_the_store_it_uses_the_agents_folder_and_when_the_turn_began() {
        let own = scratch("store");
        let store = Store::in_memory().unwrap();
        store.begin("agent", "Pulse Keeper", &own).unwrap();
        store.begin_conversation("talk", "agent", "First").unwrap();
        // A file from a run half an hour before this one.
        let old = own.join("errand-pulse-old.txt");
        std::fs::write(&old, "pulse").unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let earlier = now - 30 * 60 * 1000;
        set_the_time(&old, earlier);
        store.asked("talk", "Write the pulse file.").unwrap();
        // And one this turn really wrote.
        std::fs::write(own.join("errand-pulse-new.txt"), "pulse").unwrap();

        let said = format!(
            "Pulse written. The new file landed at `{}` (5 bytes).",
            own.join("errand-pulse-new.txt").display()
        );
        assert!(what_is_not_so(&store, "talk", &said).is_empty());

        let said = format!(
            "Pulse written. The new file landed at `{}` (5 bytes).",
            old.display()
        );
        let lines = what_is_not_so(&store, "talk", &said);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with(&format!(
                "It says it wrote {}, and that file was last changed at",
                old.display()
            )),
            "{}",
            lines[0]
        );

        let said = "Pulse written to `errand-pulse-never.txt`.";
        let lines = what_is_not_so(&store, "talk", said);
        assert_eq!(
            lines,
            vec![
                "It says it wrote errand-pulse-never.txt, and there is no such file in its \
                 folder. Check before relying on that part of the answer, or ask it to do it \
                 again."
                    .to_string()
            ]
        );
        // A conversation the store does not know says nothing at all.
        assert!(what_is_not_so(&store, "nobody", said).is_empty());
        let _ = std::fs::remove_dir_all(&own);
    }

    #[test]
    fn a_real_answer_that_tells_the_truth_raises_nothing() {
        // The shapes a routine actually answered in, with the disk standing in
        // for /Volumes/870EVO. Every one of these is true, so every one of
        // these has to pass in silence.
        let own = scratch("truth-own");
        let disk = scratch("truth-disk");
        std::fs::write(
            disk.join("errand-pulse-20260905-162041.txt"),
            "clawdbot pulse",
        )
        .unwrap();
        let also = vec![disk.clone()];
        let d = disk.display();
        for said in [
            format!("Pulse written and confirmed.\n\n- Wrote: `errand-pulse-20260905-162041.txt` (51 bytes) to `{d}/`\n- `ls` shows it as the newest entry\n- Write succeeded cleanly."),
            format!("Pulse written. The new file landed at `{d}/errand-pulse-20260905-162041.txt` (51 bytes, 16:20:41 UTC), confirmed by the directory listing."),
            format!("Pulse written and confirmed.\n\n- **File:** `{d}/errand-pulse-20260905-162041.txt`\n- **Contents:** `clawdbot pulse 2026-09-05T16:20:41Z`\n- **Confirm:** `ls -lat` shows it as the newest of the three."),
            "Pulse landed cleanly. `errand-pulse-20260905-162041.txt` is the newest file on the volume.".to_string(),
            // 7 September, 06:41: the newest file written, the three newest
            // listed under a sentence that ends in a colon.
            "Pulse written and confirmed. The write succeeded with no permission error, and the listing shows the fresh file at the top:\n\n- `errand-pulse-20260907-064201.txt` (51 bytes, 06:42)\n- `errand-pulse-20260907-064157.txt` (51 bytes, 06:41)\n- `errand-pulse-20260907-063657.txt` (51 bytes, 06:36)\n\nThe volume is accessible and files are landing on the usual ~5-minute cadence, each ~51 bytes.".to_string(),
            // 4 September, seq 17: a quoted phrase with a slash in it.
            "This is an established \"write/delete pulse loop\" \u{2014} a prior run wrote pulse files to the volume every ~5 minutes and left receipts (last one 16:55). My job is to keep that going every 5 minutes.".to_string(),
        ] {
            let claims = claimed_written(&said, &may(&own, &also));
            let wrong = not_borne_out(&claims, None);
            assert!(wrong.is_empty(), "{said:?} gave {wrong:?}");
        }
        for at in [own, disk] {
            let _ = std::fs::remove_dir_all(at);
        }
    }
}
