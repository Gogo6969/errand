//! What an agent has learnt about how its own job is done.
//!
//! An Errand agent is a standing job, not a person, and that decides what is
//! worth keeping. Not "the person lives in Berlin" but how this particular job
//! is done here: where the briefing goes, which template the invoices use, the
//! flag that export needs, that the client is Acme Holdings and not Acme Ltd.
//! Those are the things somebody has to say twice today, and the reason an
//! agent feels like a stranger every morning.
//!
//! A conversation already remembers itself. Claude Code holds a transcript and
//! the local engine holds a history, so nothing here is about what was said an
//! hour ago. This is only about what should still be true next week, which is
//! why it belongs to the agent and not to any conversation.
//!
//! Two things are deliberately not built. Nothing is extracted automatically
//! from what was said, because a store that fills itself fills with "the person
//! said hello" and then crowds out the notes that matter. And nothing is
//! shared between agents: one agent's notes in front of another is the same
//! failure as this morning's briefing in front of an unrelated question.

use anyhow::{bail, Result};

use crate::store::{Memory, Store};

/// How many notes an agent starts a conversation holding.
///
/// A ceiling for the same reason the tool budget has one: the window belongs to
/// the errand somebody is actually running, and notes crowding it out is the
/// failure this number exists to prevent. That cost has already been measured
/// once on the tool side, where filling the whole allowance took an answer from
/// nineteen seconds to fifty-three, and there is no reason to learn it twice.
const SEEDED: usize = 12;

/// And never more than this many characters of them, whatever the count.
///
/// Counted rather than trusted. A note is a sentence a model wrote, and nothing
/// about a model bounds a sentence.
const SEED_CHARS: usize = 1_500;

/// How many a search hands back.
const FOUND: usize = 5;

/// The longest a handle may be, and the longest a note may be.
const HANDLE_CHARS: usize = 60;
const NOTE_CHARS: usize = 400;

/// How an agent is told these exist, appended to whatever else it is told.
///
/// Worded around the job rather than around the person, because that is what
/// decides whether the right things get written down. "Remember facts about the
/// user" produces a notebook full of pleasantries; "remember how this job is
/// done here" produces the thing somebody would otherwise have to say twice.
pub const HOW_TO_USE_IT: &str = "\
YOUR OWN NOTES

You keep notes about how your job is done here, and they outlive any one
conversation. They are yours alone: no other agent can read them.

Write one down with `remember` the moment you are told something that will
still be true next week. How this job is done here, not what happened today:
where things go, which template or account or flag to use, what somebody is
actually called, what went wrong last time and what fixed it. If you had to
ask, or had to work it out, write it down so you do not have to again.

Do not write down what the conversation already holds, or anything you were
not told. A note you invented is worse than no note, because you will believe
it later.

Use `recall` before deciding how to do something, when there is a good chance
you have been told already. Use `forget` when a note has stopped being true and
there is nothing to replace it with; to correct one, just `remember` it again
under the same handle.";

/// A handle, checked.
///
/// Short, lowercase and joined with underscores, because the handle is the key:
/// it is what a correction replaces and what forgetting names. A model left to
/// its own devices writes a sentence here, and a sentence never matches itself
/// twice, so nothing would ever be corrected and both answers would sit there
/// for ever.
pub fn a_handle(said: &str) -> Result<String> {
    let handle: String = said
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| match c {
            c if c.is_alphanumeric() => c,
            _ => '_',
        })
        .collect();
    let handle = handle.trim_matches('_').to_string();
    // Collapse the runs the mapping above creates from spaces and punctuation.
    let handle = handle
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_");

    if handle.is_empty() {
        bail!(
            "a note needs something to be about, in a word or two like `where_the_briefing_goes`"
        );
    }
    if handle.chars().count() > HANDLE_CHARS {
        bail!(
            "`{handle}` is a sentence rather than a handle. Use a word or two, like \
             `invoice_template`, so that saying it again corrects this note rather than \
             adding a second one."
        );
    }
    Ok(handle)
}

/// A note, checked.
pub fn a_note(said: &str) -> Result<String> {
    let note = said.trim();
    if note.is_empty() {
        bail!("there was nothing to write down");
    }
    if note.chars().count() > NOTE_CHARS {
        bail!(
            "that is {} characters, and a note has to fit in {NOTE_CHARS}. Keep the thing \
             that will still be true next week and leave out the story.",
            note.chars().count()
        );
    }
    if looks_like_a_secret(note) {
        bail!(
            "that looks like a key or a password, and notes are kept in plain text and read \
             back into every conversation. Put it wherever this machine keeps secrets and \
             write down where you put it instead."
        );
    }
    Ok(note.to_string())
}

/// Does this look like something that should not be written down in the clear?
///
/// A guess, and deliberately a narrow one. The cost of a false positive is a
/// refused note with a sentence explaining why, which somebody can work around.
/// The cost of a false negative is an API key sitting in plain text and read
/// back into every conversation this agent ever has.
fn looks_like_a_secret(note: &str) -> bool {
    let lower = note.to_lowercase();
    // The prefixes that are only ever the start of a credential.
    let known_shapes = [
        "sk-ant-",
        "sk-proj-",
        "sk-live-",
        "ghp_",
        "github_pat_",
        "xoxb-",
        "xoxp-",
        "aws_secret",
    ];
    if known_shapes.iter().any(|shape| lower.contains(shape)) {
        return true;
    }
    // Or a long unbroken run of key-ish characters sitting next to a word that
    // says what it is. Either alone is ordinary: a path is long and unbroken,
    // and "the password is in 1Password" is a sentence worth keeping.
    let named = [
        "password",
        "api key",
        "api_key",
        "secret",
        "token",
        "passphrase",
    ]
    .iter()
    .any(|word| lower.contains(word));
    named
        && note.split_whitespace().any(|word| {
            word.chars().count() >= 20
                && word
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                && word.chars().any(|c| c.is_ascii_digit())
        })
}

/// What an agent opens a conversation already knowing.
///
/// In front of the model rather than waiting to be searched for, because an
/// agent that has to think to go looking for what it was told is an agent that
/// mostly will not. Bounded twice, by count and by characters, because the
/// window belongs to the errand and not to the notebook.
pub fn opening(store: &Store, agent: &str) -> Result<String> {
    let kept = store.remembers(agent, SEEDED)?;
    if kept.is_empty() {
        return Ok(String::new());
    }

    let mut out = String::from(
        "What you have already been told about this job. These are your own notes from \
         earlier conversations, not something anybody has just said to you:\n",
    );
    for one in &kept {
        let line = format!("- {}: {}\n", one.about.replace('_', " "), one.note);
        if out.chars().count() + line.chars().count() > SEED_CHARS {
            break;
        }
        out.push_str(&line);
    }
    Ok(out)
}

/// What a search hands back, as the model will read it.
pub fn search(store: &Store, agent: &str, looking_for: &str) -> Result<String> {
    let found = store.recall(agent, looking_for, FOUND)?;
    if found.is_empty() {
        // A sentence rather than a blank, and never a fallback to whatever was
        // most recent: an agent handed something unrelated cannot tell what it
        // was told from what happened to be lying around, and will act on the
        // second as though it were the first.
        return Ok(
            "Nothing written down about that. You have not been told, so find out, \
                   and write it down when you do."
                .to_string(),
        );
    }
    Ok(found
        .iter()
        .map(|one| format!("- {}: {}", one.about.replace('_', " "), one.note))
        .collect::<Vec<_>>()
        .join("\n"))
}

/// Everything an agent knows, for a person rather than a model.
pub fn all_of_it(store: &Store, agent: &str) -> Result<Vec<Memory>> {
    store.remembers(agent, 500)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_handle_is_a_word_or_two_and_a_sentence_is_refused() {
        // The handle is the key: a sentence never matches itself twice, so
        // nothing would ever be corrected and both answers would sit there.
        assert_eq!(
            a_handle("Where the briefing goes").unwrap(),
            "where_the_briefing_goes"
        );
        assert_eq!(
            a_handle("  invoice-template!  ").unwrap(),
            "invoice_template"
        );
        assert_eq!(
            a_handle("Acme Holdings (not Ltd)").unwrap(),
            "acme_holdings_not_ltd"
        );
        assert!(a_handle("").is_err());
        assert!(a_handle("   !!!   ").is_err());

        let sentence = a_handle(&"a word ".repeat(20));
        assert!(sentence.is_err(), "a whole sentence was accepted as a key");
        assert!(format!("{:#}", sentence.unwrap_err()).contains("handle"));
    }

    #[test]
    fn a_note_that_is_really_a_key_is_refused_with_somewhere_else_to_put_it() {
        // Notes are plain text and are read back into every conversation this
        // agent ever has, so this is the one refusal worth being firm about.
        for secret in [
            "the api key is sk-ant-api03-abcdefghijklmnop",
            "token: ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "password is Hunter2xxxxxxxxxxxxxxxxxxx9",
        ] {
            let said = a_note(secret);
            assert!(said.is_err(), "{secret:?} was written down in the clear");
            assert!(
                format!("{:#}", said.unwrap_err()).contains("where you put it"),
                "it refused without saying what to do instead"
            );
        }
    }

    #[test]
    fn an_ordinary_note_is_not_mistaken_for_a_key() {
        // The false-positive side matters more than the true-positive side: a
        // refusal an agent does not understand is one it will keep hitting.
        for fine in [
            "The briefing goes to Telegram, not email",
            "The password for the export is in 1Password under Acme",
            "Use /Users/somebody/Documents/templates/invoice-acme-2026.docx",
            "The API key lives in the environment as ACME_KEY",
        ] {
            assert!(a_note(fine).is_ok(), "{fine:?} was refused");
        }
    }

    #[test]
    fn a_note_long_enough_to_be_a_story_is_refused() {
        assert!(a_note(&"word ".repeat(200)).is_err());
        assert!(a_note("").is_err());
    }

    #[test]
    fn an_agent_with_nothing_written_down_opens_with_nothing_at_all() {
        // Not an empty heading. A block that says "here is what you know" above
        // nothing reads as knowledge that has been lost.
        let store = Store::in_memory().unwrap();
        store
            .begin("a1", "One", std::path::Path::new("/tmp/one"))
            .unwrap();
        assert_eq!(opening(&store, "a1").unwrap(), "");
    }

    #[test]
    fn the_notes_an_agent_opens_with_stop_at_the_budget_however_long_they_are() {
        // A note is a sentence a model wrote, and nothing about a model bounds
        // a sentence. The window belongs to the errand, not the notebook.
        let store = Store::in_memory().unwrap();
        store
            .begin("a1", "One", std::path::Path::new("/tmp/one"))
            .unwrap();
        for n in 0..40 {
            store
                .remember("a1", &format!("thing_{n}"), &"x".repeat(NOTE_CHARS))
                .unwrap();
        }
        let said = opening(&store, "a1").unwrap();
        assert!(
            said.chars().count() <= SEED_CHARS + NOTE_CHARS,
            "the notes took {} characters",
            said.chars().count()
        );
    }

    #[test]
    fn the_opening_says_the_notes_are_its_own_and_not_something_just_said_to_it() {
        // Without that, an agent treats a note as an instruction it has just
        // been given and acts on it immediately.
        let store = Store::in_memory().unwrap();
        store
            .begin("a1", "One", std::path::Path::new("/tmp/one"))
            .unwrap();
        store.remember("a1", "where_it_goes", "Telegram").unwrap();
        let said = opening(&store, "a1").unwrap();
        assert!(said.contains("your own notes"), "{said}");
        assert!(said.contains("where it goes: Telegram"), "{said}");
    }

    #[test]
    fn a_search_that_finds_nothing_says_so_rather_than_handing_back_something_else() {
        let store = Store::in_memory().unwrap();
        store
            .begin("a1", "One", std::path::Path::new("/tmp/one"))
            .unwrap();
        store.remember("a1", "where_it_goes", "Telegram").unwrap();

        let said = search(&store, "a1", "something nobody mentioned").unwrap();
        assert!(said.contains("Nothing written down"), "{said}");
        assert!(
            !said.contains("Telegram"),
            "it handed back an unrelated note"
        );
    }
}
