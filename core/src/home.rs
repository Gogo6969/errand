//! A teammate's home: a folder the person can find, read and change.
//!
//! A teammate was rows in a database. What it remembered, how it checks its
//! work and what it had been taught were real, read into every conversation
//! it had, and nowhere anybody could look except one panel at a time. Asked
//! where its things were, one said there was no such place: agents were
//! cards in the app, it said, and its folder was empty.
//!
//! So each teammate has a home: `agents/<id>` in Errand's own folder, with a
//! link by its name under `Teammates`. In it, as plain files: who it is, what
//! it remembers, how it checks its work, and each skill it has been taught.
//!
//! The database stays what is true, and the files are written from it. A
//! file somebody changes in an editor is not taken as it stands: Errand shows
//! what changed and asks first. Anything that can write the file, the person
//! or anything else running as them, would otherwise be writing the
//! teammate's instructions unseen. No teammate can write a home at all: the
//! wall keeps every one of them out, and lets each read only its own.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::store::{Agent, Store};

/// Where the homes are, inside Errand's own folder.
pub const HOMES: &str = "agents";

/// Where each is listed by name, as a link to it.
pub const INDEX: &str = "Teammates";

pub const README: &str = "README.md";
pub const CARD: &str = "card.md";
pub const MEMORY: &str = "memory.md";
pub const CHECKLIST: &str = "checklist.md";
pub const SKILLS: &str = "skills";

/// The longest a note may be when the person writes it themselves. Longer
/// than a teammate's own, which are kept short because a model left to
/// itself writes the story rather than the fact; a person writing one down
/// on purpose has said what they meant.
pub const THE_PERSONS_NOTE_CHARS: usize = 2_000;

/// A teammate's home.
pub fn of(errand: &Path, agent: &str) -> PathBuf {
    errand.join(HOMES).join(agent)
}

/// The name a home is listed under: the teammate's own, with what a file
/// name cannot hold taken out.
pub fn listed_as(label: &str) -> String {
    let named: String = label
        .chars()
        .map(|c| match c {
            '/' | ':' => '-',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let named = named.trim().trim_start_matches('.').trim().to_string();
    match named.is_empty() {
        true => "Teammate".to_string(),
        false => named.chars().take(80).collect(),
    }
}

/// A skill's file, by its name.
fn skill_file(name: &str, taken: &[String]) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| match c.is_alphanumeric() {
            true => c,
            false => '-',
        })
        .collect();
    let slug: String = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
        .chars()
        .take(60)
        .collect();
    let slug = match slug.is_empty() {
        true => "skill".to_string(),
        false => slug,
    };
    let mut file = format!("{SKILLS}/{slug}.md");
    let mut n = 2;
    while taken.contains(&file) {
        file = format!("{SKILLS}/{slug}-{n}.md");
        n += 1;
    }
    file
}

/// Every file in a home, by its path inside the home, as written from what
/// is true now.
pub fn render(store: &Store, agent: &Agent, team: &str) -> Result<BTreeMap<String, String>> {
    let name = agent.name.trim();
    let mut files = BTreeMap::new();
    files.insert(README.to_string(), readme(name));

    let mut card = format!(
        "# {name}\n\nRole: {}\nJob: {}\n",
        agent.title.as_deref().unwrap_or("").trim(),
        agent
            .about
            .as_deref()
            .unwrap_or("")
            .replace('\n', " ")
            .trim()
    );
    card.push_str(
        "\nChange the name in the first line, or the Role or Job line, and save: Errand asks \
         you before it takes the change. Nothing below this is read back.\n\n",
    );
    if !team.is_empty() {
        card.push_str(&format!("Team: {team}\n"));
    }
    card.push_str(&format!(
        "Runs on: {}\nAsks: {}\nWorks in: {}\n",
        agent.engine, agent.asks, agent.cwd
    ));
    files.insert(CARD.to_string(), card);

    let mut memory = format!(
        "# What {name} remembers\n\nEach note is a heading and the note under it. Change a \
         note, add one or delete one, and save: Errand asks you before it takes the change. \
         A heading is a short handle, like `where_reports_go`.\n"
    );
    for note in store.remembers(&agent.id, 10_000)? {
        memory.push_str(&format!("\n## {}\n\n{}\n", note.about, note.note.trim()));
    }
    files.insert(MEMORY.to_string(), memory);

    let mut checklist = format!(
        "# How {name} checks its work\n\nBefore it says a task is done, it goes through each \
         of these, and a point that fails means not done yet. One point per line, starting \
         with \"- \". Errand asks you before it takes a change.\n\n"
    );
    for point in store.checklist(&agent.id)? {
        checklist.push_str(&format!("- {point}\n"));
    }
    files.insert(CHECKLIST.to_string(), checklist);

    let mut taken: Vec<String> = Vec::new();
    for skill in store.skills(&agent.id)? {
        let file = skill_file(&skill.name, &taken);
        taken.push(file.clone());
        let made = chrono::DateTime::from_timestamp_millis(skill.made_at)
            .map(|at| {
                at.with_timezone(&chrono::Local)
                    .format("%-d %B %Y")
                    .to_string()
            })
            .unwrap_or_default();
        let mut text = format!(
            "# {}\n\nTaught on {made}. This file shows the skill and is not read back: to \
             change it, ask {name} to do the task again the new way and keep it.\n\n\
             ## What was asked\n\n{}\n\n## The steps it took\n\n",
            skill.name,
            skill.request.trim()
        );
        for (i, step) in skill.steps.iter().enumerate() {
            text.push_str(&format!("{}. {}", i + 1, step.what.trim()));
            if step.refused {
                text.push_str(" (you said no to this)");
            }
            text.push('\n');
        }
        files.insert(file, text);
    }
    Ok(files)
}

fn readme(name: &str) -> String {
    format!(
        "# {name}\n\n\
         This is {name}'s home in Errand: who it is, what it remembers, how it checks its \
         work, and what it has been taught.\n\n\
         - card.md: its name, role and job\n\
         - memory.md: its notes, read into every conversation it has\n\
         - checklist.md: what it goes through before it says a task is done\n\
         - skills: one file for each task it can do again by name\n\n\
         Change card.md, memory.md or checklist.md in any editor and save. Errand shows you \
         what changed and asks before it takes it, because these are the teammate's \
         instructions. {name} can read these files and cannot change them: it changes only \
         through its own notes and suggestions you agree to.\n\n\
         Its work, the files it makes, is in its working folder, named in card.md.\n"
    )
}

/// What the person changed in a file, read back into what it means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadBack {
    Card {
        name: String,
        role: String,
        job: String,
    },
    Memory(Vec<(String, String)>),
    Checklist(Vec<String>),
    /// A file that is shown and never read back.
    NotReadBack,
}

/// Read a file of a home as the person left it.
pub fn read_back(path: &str, text: &str) -> ReadBack {
    match path {
        CARD => {
            let mut name = String::new();
            let mut role = String::new();
            let mut job = String::new();
            for line in text.lines() {
                let line = line.trim();
                if name.is_empty() {
                    if let Some(rest) = line.strip_prefix("# ") {
                        name = rest.trim().to_string();
                        continue;
                    }
                }
                if let Some(rest) = line.strip_prefix("Role:") {
                    role = rest.trim().to_string();
                } else if let Some(rest) = line.strip_prefix("Job:") {
                    job = rest.trim().to_string();
                }
            }
            ReadBack::Card { name, role, job }
        }
        MEMORY => {
            let mut notes: Vec<(String, String)> = Vec::new();
            let mut heading: Option<String> = None;
            let mut body: Vec<&str> = Vec::new();
            let mut finish = |heading: &mut Option<String>, body: &mut Vec<&str>| {
                if let Some(about) = heading.take() {
                    notes.push((about, body.join("\n").trim().to_string()));
                }
                body.clear();
            };
            for line in text.lines() {
                match line.strip_prefix("## ") {
                    Some(about) => {
                        finish(&mut heading, &mut body);
                        heading = Some(about.trim().to_string());
                    }
                    None if heading.is_some() => body.push(line),
                    None => {}
                }
            }
            finish(&mut heading, &mut body);
            ReadBack::Memory(notes)
        }
        CHECKLIST => ReadBack::Checklist(
            text.lines()
                .filter_map(|line| {
                    let line = line.trim();
                    line.strip_prefix("- ")
                        .or_else(|| line.strip_prefix("* "))
                        .map(|p| p.trim().to_string())
                })
                .filter(|p| !p.is_empty())
                .collect(),
        ),
        _ => ReadBack::NotReadBack,
    }
}

/// What changed between what Errand wrote and what is there now, in words,
/// for the person to judge. Nothing is applied here.
pub fn what_changed(path: &str, written: &str, now: &str) -> Vec<String> {
    match (read_back(path, written), read_back(path, now)) {
        (
            ReadBack::Card {
                name: n0,
                role: r0,
                job: j0,
            },
            ReadBack::Card {
                name: n1,
                role: r1,
                job: j1,
            },
        ) => {
            let mut said = Vec::new();
            for (what, was, is) in [("Name", n0, n1), ("Role", r0, r1), ("Job", j0, j1)] {
                if was != is {
                    said.push(format!("{what}: \"{was}\" becomes \"{is}\""));
                }
            }
            said
        }
        (ReadBack::Memory(was), ReadBack::Memory(is)) => {
            let was: HashMap<String, String> = was.into_iter().collect();
            let mut said = Vec::new();
            let mut seen = Vec::new();
            for (about, note) in &is {
                seen.push(about.clone());
                match was.get(about) {
                    None => said.push(format!("Adds the note \"{about}\": {note}")),
                    Some(before) if before != note => {
                        said.push(format!("Changes the note \"{about}\" to: {note}"))
                    }
                    Some(_) => {}
                }
            }
            let mut gone: Vec<&String> = was.keys().filter(|about| !seen.contains(about)).collect();
            gone.sort();
            for about in gone {
                said.push(format!("Takes away the note \"{about}\""));
            }
            said
        }
        (ReadBack::Checklist(was), ReadBack::Checklist(is)) => {
            let mut said: Vec<String> = is
                .iter()
                .filter(|p| !was.contains(p))
                .map(|p| format!("Adds the point: {p}"))
                .collect();
            said.extend(
                was.iter()
                    .filter(|p| !is.contains(p))
                    .map(|p| format!("Takes away the point: {p}")),
            );
            if said.is_empty() && was != is {
                said.push("Puts the points in a new order".to_string());
            }
            said
        }
        _ => vec!["This file shows what is true and is not read back.".to_string()],
    }
}

/// Take what the person changed in a file. Says what could not be taken, and
/// why, entry by entry; everything else is taken.
pub fn take(store: &Store, agent: &str, path: &str, now: &str, at: i64) -> Result<Vec<String>> {
    let mut refused = Vec::new();
    match read_back(path, now) {
        ReadBack::Card { name, role, job } => {
            if name.trim().is_empty() {
                refused.push("A teammate needs a name, so the name was kept.".to_string());
                let was = store.agent(agent)?.map(|a| a.name).unwrap_or_default();
                store.rename(agent, &was, &role, &job)?;
            } else {
                store.rename(agent, &name, &role, &job)?;
            }
        }
        ReadBack::Memory(notes) => {
            let had: Vec<String> = store
                .remembers(agent, 10_000)?
                .into_iter()
                .map(|m| m.about)
                .collect();
            let mut keeping: Vec<String> = Vec::new();
            for (about, note) in notes {
                let handle = match crate::memory::a_handle(&about) {
                    Ok(handle) => handle,
                    Err(why) => {
                        refused.push(format!("\"{about}\": {why}"));
                        continue;
                    }
                };
                match a_note_from_the_person(&note) {
                    Ok(note) => {
                        // Taken only if it says something new: a note the
                        // person did not touch keeps its place and its count.
                        let same = store
                            .remembers(agent, 10_000)?
                            .into_iter()
                            .any(|m| m.about == handle && m.note.trim() == note);
                        if !same {
                            store.remember(agent, &handle, &note)?;
                        }
                        keeping.push(handle);
                    }
                    Err(why) => {
                        refused.push(format!("\"{about}\": {why}"));
                        // Kept as it was rather than lost with the edit.
                        keeping.push(handle);
                    }
                }
            }
            for about in had.iter().filter(|about| !keeping.contains(about)) {
                store.forget_note(agent, about)?;
            }
        }
        ReadBack::Checklist(points) => {
            let kept = store.set_checklist(agent, &points, at)?;
            let dropped = points.len().saturating_sub(kept.len());
            if dropped > 0 {
                refused.push(format!(
                    "{dropped} point{} left out: repeated, empty, or more than {} in all.",
                    if dropped == 1 { " was" } else { "s were" },
                    crate::checklist::AT_MOST
                ));
            }
        }
        ReadBack::NotReadBack => {
            refused.push("This file is not read back, so nothing was taken from it.".to_string())
        }
    }
    Ok(refused)
}

/// A note the person wrote, checked: longer than a teammate's may be, and
/// never a secret.
fn a_note_from_the_person(said: &str) -> Result<String> {
    let note = said.trim();
    anyhow::ensure!(
        !note.is_empty(),
        "a note with nothing under it was left out"
    );
    anyhow::ensure!(
        note.chars().count() <= THE_PERSONS_NOTE_CHARS,
        "that is {} characters, and a note fits in {THE_PERSONS_NOTE_CHARS}",
        note.chars().count()
    );
    anyhow::ensure!(
        !crate::memory::looks_like_a_secret(note),
        "that looks like a key or a password, and notes are read back into every \
         conversation in plain text"
    );
    Ok(note.to_string())
}

/// A file the person changed and Errand has not taken yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changed {
    pub path: String,
    /// What it says now.
    pub now: String,
    /// What Errand last wrote there.
    pub written: String,
}

/// Bring a home in line with what is true, and say which files the person
/// has changed since Errand last wrote them. Those are left exactly as they
/// are until the person says what to do with them.
///
/// For each file: one the person has not touched is written again when what
/// is true has changed; one they changed is reported and left alone; one that
/// is missing is written. A link where a file should be is removed rather
/// than followed. A skill file for a skill there is no more goes, unless the
/// person changed it.
pub fn keep(store: &Store, errand: &Path, agent: &Agent, team: &str) -> Result<Vec<Changed>> {
    let home = of(errand, &agent.id);
    make_a_folder(&home)?;
    make_a_folder(&home.join(SKILLS))?;
    let ours = render(store, agent, team)?;
    let written = store.home_written(&agent.id)?;
    let mut changed = Vec::new();
    for (path, text) in &ours {
        let at = home.join(path);
        match read_plain(&at) {
            None => {
                write_plain(&at, text)?;
                store.set_home_written(&agent.id, path, text)?;
            }
            Some(on_disk) if &on_disk == text => {
                if written.get(path) != Some(text) {
                    store.set_home_written(&agent.id, path, text)?;
                }
            }
            Some(on_disk) => match written.get(path) {
                Some(before) if *before == on_disk => {
                    write_plain(&at, text)?;
                    store.set_home_written(&agent.id, path, text)?;
                }
                before => changed.push(Changed {
                    path: path.clone(),
                    now: on_disk,
                    written: before.cloned().unwrap_or_default(),
                }),
            },
        }
    }
    // Skill files for skills there are no more.
    for (path, before) in &written {
        if ours.contains_key(path) {
            continue;
        }
        let at = home.join(path);
        match read_plain(&at) {
            Some(on_disk) if on_disk != *before => {}
            _ => {
                std::fs::remove_file(&at).ok();
                store.forget_home_written(&agent.id, path)?;
            }
        }
    }
    Ok(changed)
}

/// List a home under the teammate's name, and no other.
pub fn list(errand: &Path, agent: &str, label: &str) -> Result<()> {
    let index = errand.join(INDEX);
    make_a_folder(&index)?;
    let home = of(errand, agent);
    let named = index.join(listed_as(label));
    if let Ok(found) = std::fs::read_dir(&index) {
        for one in found.flatten() {
            let link = one.path();
            let ours = std::fs::read_link(&link).is_ok_and(|to| to == home);
            if ours && link != named {
                std::fs::remove_file(&link).ok();
            }
        }
    }
    match std::fs::read_link(&named) {
        Ok(to) if to == home => Ok(()),
        // The name belongs to another teammate's home: that one keeps it.
        Ok(_) => Ok(()),
        Err(_) => {
            if std::fs::symlink_metadata(&named).is_err() {
                std::os::unix::fs::symlink(&home, &named)?;
            }
            Ok(())
        }
    }
}

/// Take away a home and its listing, for a teammate that is gone.
pub fn take_away(errand: &Path, agent: &str) {
    let home = of(errand, agent);
    let index = errand.join(INDEX);
    if let Ok(found) = std::fs::read_dir(&index) {
        for one in found.flatten() {
            if std::fs::read_link(one.path()).is_ok_and(|to| to == home) {
                std::fs::remove_file(one.path()).ok();
            }
        }
    }
    std::fs::remove_dir_all(&home).ok();
}

/// A folder that is the person's alone, and never a link.
fn make_a_folder(at: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if std::fs::symlink_metadata(at).is_ok_and(|m| !m.file_type().is_dir()) {
        std::fs::remove_file(at)?;
    }
    std::fs::create_dir_all(at)?;
    std::fs::set_permissions(at, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

/// A file's text, if it is a plain file; never through a link.
fn read_plain(at: &Path) -> Option<String> {
    let meta = std::fs::symlink_metadata(at).ok()?;
    if !meta.file_type().is_file() || meta.len() > 1_000_000 {
        return None;
    }
    std::fs::read_to_string(at).ok()
}

/// Write a file whole: to a file beside it, then in its place, so an editor
/// never reads half of it. Whatever was there that is not a plain file goes.
fn write_plain(at: &Path, text: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if std::fs::symlink_metadata(at).is_ok_and(|m| !m.file_type().is_file()) {
        std::fs::remove_file(at).ok();
    }
    let beside = at.with_extension("md.writing");
    std::fs::write(&beside, text)?;
    std::fs::set_permissions(&beside, std::fs::Permissions::from_mode(0o600))?;
    std::fs::rename(&beside, at)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_store() -> (Store, Agent) {
        let store = Store::in_memory().unwrap();
        store
            .begin("a1", crate::store::NOT_YET_NAMED, Path::new("/tmp/a1"))
            .unwrap();
        store
            .rename("a1", "Bug Hunter", "QA", "Tries to break what was built")
            .unwrap();
        store
            .remember("a1", "where_reports_go", "The shared folder")
            .unwrap();
        store
            .set_checklist("a1", &["I tried the main path".into()], 1)
            .unwrap();
        let agent = store.agent("a1").unwrap().unwrap();
        (store, agent)
    }

    fn a_place(tag: &str) -> PathBuf {
        let at = std::env::temp_dir().join(format!("errand-home-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&at).ok();
        std::fs::create_dir_all(&at).unwrap();
        at
    }

    #[test]
    fn a_home_holds_who_it_is_what_it_remembers_and_how_it_checks_its_work() {
        let (store, agent) = a_store();
        let files = render(&store, &agent, "on Build crew, led by Ship Lead").unwrap();
        assert!(files[CARD].starts_with("# Bug Hunter\n\nRole: QA\nJob: Tries to break"));
        assert!(files[CARD].contains("Team: on Build crew"));
        assert!(files[MEMORY].contains("## where_reports_go\n\nThe shared folder"));
        assert!(files[CHECKLIST].contains("- I tried the main path"));
        assert!(files[README].contains("cannot change them"));
    }

    #[test]
    fn what_the_person_changed_is_read_back_and_said_in_words_before_anything_is_taken() {
        let (store, agent) = a_store();
        let files = render(&store, &agent, "").unwrap();
        let edited = files[MEMORY].replace("The shared folder", "The team drive")
            + "\n## invoices\n\nThey come on the first of the month.\n";
        let said = what_changed(MEMORY, &files[MEMORY], &edited);
        assert!(
            said.iter()
                .any(|s| s.starts_with("Changes the note \"where_reports_go\"")),
            "{said:?}"
        );
        assert!(
            said.iter()
                .any(|s| s.starts_with("Adds the note \"invoices\"")),
            "{said:?}"
        );
        // Nothing has changed in the store yet.
        assert_eq!(store.remembers("a1", 10).unwrap().len(), 1);

        let refused = take(&store, "a1", MEMORY, &edited, 2).unwrap();
        assert!(refused.is_empty(), "{refused:?}");
        let notes = store.remembers("a1", 10).unwrap();
        assert!(notes.iter().any(|m| m.about == "invoices"));
        assert!(notes.iter().any(|m| m.note == "The team drive"));

        // Taking a note away is taking it away.
        let files = render(&store, &agent, "").unwrap();
        let fewer =
            files[MEMORY].replace("## invoices\n\nThey come on the first of the month.\n", "");
        assert!(what_changed(MEMORY, &files[MEMORY], &fewer)
            .iter()
            .any(|s| s == "Takes away the note \"invoices\""));
        take(&store, "a1", MEMORY, &fewer, 3).unwrap();
        assert!(!store
            .remembers("a1", 10)
            .unwrap()
            .iter()
            .any(|m| m.about == "invoices"));
    }

    #[test]
    fn what_cannot_be_taken_is_said_and_kept_as_it_was() {
        let (store, agent) = a_store();
        let files = render(&store, &agent, "").unwrap();
        // Made here rather than written out, so nothing scanning the source
        // for keys mistakes it for one.
        let fake = ["s", "k-", "an", "t-"].concat() + &"q".repeat(40);
        let edited = files[MEMORY].clone()
            + &format!("\n## api\n\n{fake}\n")
            + &format!("\n## long\n\n{}\n", "x".repeat(THE_PERSONS_NOTE_CHARS + 1));
        let refused = take(&store, "a1", MEMORY, &edited, 2).unwrap();
        assert_eq!(refused.len(), 2, "{refused:?}");
        assert!(store
            .remembers("a1", 10)
            .unwrap()
            .iter()
            .any(|m| m.about == "where_reports_go"));
        // A note longer than a teammate's own, written by the person, is fine.
        let longer = files[MEMORY].clone() + &format!("\n## longer\n\n{}\n", "y".repeat(900));
        assert!(take(&store, "a1", MEMORY, &longer, 3).unwrap().is_empty());
        // The card: a name cannot be taken away.
        let card = files[CARD]
            .replace("# Bug Hunter", "# ")
            .replace("Role: QA", "Role: Testing");
        let refused = take(&store, "a1", CARD, &card, 4).unwrap();
        assert_eq!(refused.len(), 1);
        let now = store.agent("a1").unwrap().unwrap();
        assert_eq!(now.name, "Bug Hunter");
        assert_eq!(now.title.as_deref(), Some("Testing"));
    }

    #[test]
    fn keeping_a_home_writes_what_is_true_and_never_overwrites_what_the_person_changed() {
        let (store, agent) = a_store();
        let errand = a_place("keep");
        assert!(keep(&store, &errand, &agent, "").unwrap().is_empty());
        let home = of(&errand, "a1");
        let memory = home.join(MEMORY);
        assert!(std::fs::read_to_string(&memory)
            .unwrap()
            .contains("where_reports_go"));

        // What is true changes, and the file the person did not touch follows.
        store.remember("a1", "tea", "Green, no sugar").unwrap();
        assert!(keep(&store, &errand, &agent, "").unwrap().is_empty());
        assert!(std::fs::read_to_string(&memory).unwrap().contains("## tea"));

        // The person changes it: reported, and left exactly as they left it,
        // even when what is true changes again.
        let mine = std::fs::read_to_string(&memory)
            .unwrap()
            .replace("Green", "Black");
        std::fs::write(&memory, &mine).unwrap();
        store.remember("a1", "lunch", "At one").unwrap();
        let changed = keep(&store, &errand, &agent, "").unwrap();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].path, MEMORY);
        assert_eq!(std::fs::read_to_string(&memory).unwrap(), mine);

        // A link where a file should be is never followed or written through.
        let outside = errand.join("outside.md");
        std::fs::write(&outside, "keep me").unwrap();
        std::fs::remove_file(home.join(CHECKLIST)).unwrap();
        std::os::unix::fs::symlink(&outside, home.join(CHECKLIST)).unwrap();
        keep(&store, &errand, &agent, "").unwrap();
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), "keep me");
        assert!(!std::fs::symlink_metadata(home.join(CHECKLIST))
            .unwrap()
            .file_type()
            .is_symlink());

        // Listed by name, and moved when the name changes.
        list(&errand, "a1", "Bug Hunter").unwrap();
        assert!(errand.join(INDEX).join("Bug Hunter").exists());
        list(&errand, "a1", "Bug Finder").unwrap();
        assert!(!errand.join(INDEX).join("Bug Hunter").exists());
        assert_eq!(
            std::fs::read_link(errand.join(INDEX).join("Bug Finder")).unwrap(),
            home
        );
        take_away(&errand, "a1");
        assert!(!home.exists() && !errand.join(INDEX).join("Bug Finder").exists());
        std::fs::remove_dir_all(&errand).ok();
    }

    #[test]
    fn a_name_is_listed_as_something_a_folder_can_be_called() {
        assert_eq!(listed_as("Ship/Lead: v2"), "Ship-Lead- v2");
        assert_eq!(listed_as("..hidden"), "hidden");
        assert_eq!(listed_as("  "), "Teammate");
    }
}
