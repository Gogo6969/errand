//! The window, and the threads it is showing.
//!
//! Everything the window can do is here, and it is a short list: list the
//! threads, open one, read what was said in it, say something, stop it, forget
//! it. What comes back does not come back from a command -- it arrives on its
//! own, as the agent produces it, because a conversation where the answer only
//! appears when you ask for it is not a conversation.
//!
//! The window is never told which engine is answering. It receives the events
//! in `errand_core::engine` and nothing else, which is the whole reason that
//! protocol is as small as it is: on the day a local model is driving instead
//! of Claude Code, nothing in here or in the page changes.
//!
//! Everything said is written down as it happens rather than when the thread is
//! closed. A window can be quit, a machine can lose power, and a conversation
//! that survives only tidy exits is one nobody would trust with an errand that
//! matters.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Datelike;
use errand_core::doctor;
use errand_core::doorway;
use errand_core::goal;
use errand_core::keeping;
use errand_core::keys;
use errand_core::local::{find, LlmSettings, Local};
use errand_core::mcp;
use errand_core::memory;
use errand_core::routine;
use errand_core::routine::When;
use errand_core::store::{Settled, NOT_YET_NAMED};
use errand_core::team;
use errand_core::watch;
use errand_core::{claude::Claude, Agent, Answer, Conversation, Engine, Event, Line, Store};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

mod onscreen;

/// Everything the window is holding: the conversations that are live, and the
/// book they are all written into.
/// An agent working out who it is, and what it has said while doing so.
///
/// Its words during this are not the person's business: they were asked for by
/// the app, in the middle of somebody else's conversation, and putting them in
/// the transcript would be putting our own question there under the agent's
/// name. So while an id is in here, everything it says is collected and
/// nothing is written down or shown.
///
/// Keyed by the conversation whose engine is answering, holding the agent it is
/// answering *about* and what it has said so far. Two ids, because the question
/// goes down one conversation and the answer belongs to the agent that owns it.
type Settling = Arc<Mutex<HashMap<String, (String, String)>>>;

struct Held {
    /// Whatever is answering each open thread. Boxed rather than one concrete
    /// type because there are two engines now and the window is not told which
    /// it is talking to -- that is the whole point of the protocol, and it
    /// stops being true the moment this map knows.
    live: Mutex<HashMap<String, Box<dyn Engine + Send>>>,
    settling: Settling,
    /// Where an engine posts the things only the app can do.
    wants: tokio::sync::mpsc::UnboundedSender<team::Wants>,
    /// Routines that are still running, so the clock does not start one twice.
    ///
    /// A routine that takes longer than its own interval is ordinary, and
    /// starting it on top of itself puts two turns in one conversation racing
    /// each other. Seen the first time one was tested.
    running: Arc<Mutex<std::collections::HashSet<String>>>,
    /// Conversations somebody is waiting on the end of, by id.
    ///
    /// One agent asking another has to wait for the answer, and the events it
    /// is waiting for arrive on the other conversation's pump. Forwarding them
    /// here is more direct than listening on the window's event bus and does
    /// not depend on a window being open at all -- which matters, because a
    /// routine at seven in the morning may delegate.
    watching: Arc<Mutex<HashMap<String, tokio::sync::mpsc::UnboundedSender<Event>>>>,
    /// Turns that have just ended, for whatever goal they belong to.
    goals: tokio::sync::mpsc::UnboundedSender<(String, String)>,
    /// What each conversation is doing, for the ones that are doing something.
    ///
    /// Kept by the app rather than worked out in the window, because the window
    /// only knows about conversations somebody has opened. A routine firing at
    /// seven on an agent nobody is looking at is exactly the work worth being
    /// able to see, and it is the work the window cannot see at all.
    doing: Arc<Mutex<HashMap<String, String>>>,
    /// Watches being looked at this moment, so one slow page cannot be looked
    /// at twice at once. Held through a guard that removes it on the way out,
    /// however that happens, because the alternative leaks on every early
    /// return and a leaked entry is a watch that never looks again.
    looking: Arc<Mutex<std::collections::HashSet<String>>>,
    /// The socket each Claude conversation can reach this app's own tools on.
    ///
    /// Held here because holding it is what keeps it open: dropping one stops
    /// answering and takes the file away, so a conversation that has been
    /// closed cannot be reached by a process that outlived it.
    doorways: Mutex<HashMap<String, doorway::Doorway>>,
    /// Agents parked waiting for somebody to do something at the keyboard.
    ///
    /// Keyed by the handover rather than by the conversation, because the
    /// answer has to reach the exact call that is waiting: a second handover
    /// arriving while the first is still on screen must not be ended by the
    /// first card being pressed.
    handovers: Mutex<HashMap<String, (String, tokio::sync::oneshot::Sender<String>)>>,
    /// What is stopping errands from working, if anything is.
    ///
    /// Only the kind that goes on happening until somebody does something: a
    /// login that has expired fails identically every time, and saying so
    /// before the next errand is the difference between losing a sentence and
    /// losing a paragraph, a screenshot and a routine.
    trouble: Mutex<Option<errand_core::trouble::Trouble>>,
    /// When each model was last asked how much it holds, by mark.
    ///
    /// In memory rather than in the store: it is a thing about this run of the
    /// app, not about the model, and putting it in the settings would mean a
    /// migration and a number somebody reads in Settings that means nothing to
    /// them.
    sized: Mutex<HashMap<String, std::time::Instant>>,
    /// The run each conversation is in the middle of, for the ones started by
    /// something other than a person typing.
    ///
    /// Kept here because the two halves happen in different places: the clock
    /// starts a run and the engine's event pump is where it ends, and nothing
    /// travels between them but the conversation id.
    mid_run: Mutex<HashMap<String, i64>>,
    /// The one conversation somebody is actually looking at, if any.
    ///
    /// A notification used to be held back whenever the main window had focus,
    /// which is the right instinct pointed at the wrong thing: a routine fires
    /// in a conversation nobody is reading, and that is precisely when the
    /// window being in front should not silence it. Focus says the app is on
    /// screen; this says which of forty conversations is.
    looking_at: Mutex<Option<String>>,
    store: Arc<Store>,
}

/// One event, and which conversation it belongs to.
///
/// The id travels with it because the window shows one conversation at a time
/// and keeps several alive: somebody who starts something slow and goes to read
/// another should come back to find it finished, not paused.
#[derive(Clone, Serialize)]
struct Happened {
    conversation: String,
    /// Where this landed in the conversation, when it was written down.
    ///
    /// Carried so that a message just received can be carried on from, the
    /// same as one read back off disk. Without it "From here" would appear on
    /// everything above the fold and on nothing below it, which reads as a
    /// button that comes and goes.
    #[serde(skip_serializing_if = "Option::is_none")]
    seq: Option<i64>,
    #[serde(flatten)]
    event: Event,
}

/// Where the threads and the book they are written in live.
///
/// Named after the app rather than keyed to its bundle identifier, which is
/// the usual thing and was a mistake worth not repeating. An identifier can
/// have to change: macOS records per identifier whether an app may put
/// anything on screen, and that record cannot be argued with, only left
/// behind. When it changed here every thread moved with it, and moving a
/// thread is not a small thing, because each one is a working directory and
/// the agent's memory of that conversation is filed under exactly that path.
/// A name does not change when an identifier does.
fn where_things_live(_app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let here = beside_everything_else().ok_or("there is no home directory to keep things in")?;
    std::fs::create_dir_all(&here).map_err(|e| e.to_string())?;
    Ok(here)
}

/// The same place, worked out without an app to ask.
///
/// One definition rather than two, and the reason is the whole session's worth
/// of the same bug: something computed in two places drifts, and the day it
/// does the app and the thing talking to it disagree about where the door is,
/// which reads as "Errand is not running" while it plainly is. This is what
/// Tauri's own resolver returns on a Mac, and this app is a Mac app.
fn beside_everything_else() -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(std::path::PathBuf::from(home).join("Library/Application Support/Errand"))
}

/// Say on screen that an errand has ended, if nobody was there to see it end.
///
/// The reason to have this at all is that these jobs take minutes. Somebody
/// hands over a thing worth walking away from, walks away, and would otherwise
/// have to keep coming back to find out whether it is finished, which is most
/// of the value of having handed it over in the first place.
///
/// And the reason it is conditional is the same reason: a notification for a
/// thread you are watching finish is noise, and an app that sends those gets
/// its notifications turned off, along with the ones that mattered.
fn tell_them(app: &AppHandle, store: &Store, id: &str, event: &Event) {
    let (title, body) = match event {
        Event::Done { said, .. } => (called(store, id), gist(said)),
        // "Stopped" alone reads as something somebody was watching. A routine
        // stopping is a different fact: nobody was there, and it will not have
        // done the thing it does every morning.
        Event::Failed { why } => {
            let by_the_clock = store
                .conversation(id)
                .ok()
                .flatten()
                .and_then(|c| c.runs_at)
                .is_some();
            let what = match by_the_clock {
                true => format!("{} stopped, and it was a routine", called(store, id)),
                false => format!("{} stopped", called(store, id)),
            };
            (what, gist(why))
        }
        // An agent that has stopped to ask is the one thing here that is
        // actually waiting on somebody. Finishing can be read whenever; a
        // question holds the whole errand until it is answered, and a routine
        // at seven in the morning will sit on one until it times out with
        // nobody ever knowing it asked.
        Event::NeedsYou(ask) => (
            format!("{} needs you", called(store, id)),
            gist(&ask.asking),
        ),
        _ => return,
    };
    // Held back only for the conversation actually on screen. Both halves are
    // needed: the window can be in front with something else open, and it can
    // have this open while sitting behind a browser.
    let in_front = app
        .get_webview_window("main")
        .and_then(|w| w.is_focused().ok())
        .unwrap_or(false);
    let held: State<Held> = app.state();
    let this_one = held.looking_at.lock().unwrap().as_deref() == Some(id);
    if in_front && this_one {
        return;
    }
    onscreen::show(id, &title, &body);
    onscreen::waiting(how_many_are_waiting(&held));
}

/// How many agents are stopped waiting on somebody, for the dock icon.
///
/// The one count in this app that is a claim on somebody's attention rather
/// than a tally of things that happened, which is why it is the one worth
/// putting on the icon.
fn how_many_are_waiting(held: &Held) -> i64 {
    held.doing
        .lock()
        .unwrap()
        .values()
        .filter(|what| what.starts_with("Waiting on you"))
        .count() as i64
}

/// What a thread is called, or something honest if it is not called anything.
fn called(store: &Store, id: &str) -> String {
    // The conversation's agent, not the id as an agent. They are the same
    // string only for an agent's first conversation, so every conversation
    // opened after that was announced as "Errand" rather than by name, which
    // is the one thing a notification is for.
    store
        .conversation(id)
        .ok()
        .flatten()
        .map(|c| c.agent)
        .and_then(|agent| store.agent(&agent).ok().flatten())
        .map_or_else(|| "Errand".to_string(), |a| a.name)
}

/// The gist of what it said, in the room a notification actually has.
///
/// The first line, because that is where an answer puts its point, with any
/// heading marks taken off: what reads as a heading in the thread reads as
/// punctuation in a notification.
fn gist(said: &str) -> String {
    let line = said
        .lines()
        .map(|l| {
            l.trim()
                .trim_start_matches('#')
                .trim_start_matches("**")
                .trim()
        })
        .find(|l| !l.is_empty())
        .unwrap_or("Finished.");
    if line.chars().count() > 140 {
        let short: String = line.chars().take(139).collect();
        format!("{short}\u{2026}")
    } else {
        line.to_string()
    }
}

/// Every agent there is: pinned first, then most recently spoken to.
#[tauri::command]
async fn agents(held: State<'_, Held>) -> Result<Vec<Agent>, String> {
    held.store.agents().map_err(|e| e.to_string())
}

/// The threads with something matching in them.
#[tauri::command]
async fn matching(held: State<'_, Held>, looking_for: String) -> Result<Vec<Agent>, String> {
    held.store.matching(&looking_for).map_err(|e| e.to_string())
}

/// Everything said in one thread, in the order it was said.
#[tauri::command]
async fn lines(held: State<'_, Held>, id: String) -> Result<Vec<Line>, String> {
    held.store.lines(&id).map_err(|e| e.to_string())
}

/// One agent's conversations, most recently spoken to first.
#[tauri::command]
async fn conversations(held: State<'_, Held>, agent: String) -> Result<Vec<Conversation>, String> {
    held.store.conversations(&agent).map_err(|e| e.to_string())
}

/// Start another conversation with an agent it already has.
///
/// A fresh id every time, chosen by the window, because it becomes the engine's
/// session id and reusing one would resume something somebody meant to leave.
#[tauri::command]
async fn start_conversation(
    held: State<'_, Held>,
    id: String,
    agent: String,
    name: String,
) -> Result<(), String> {
    held.store
        .begin_conversation(&id, &agent, &name)
        .map_err(|e| e.to_string())
}

/// Give a conversation the name it is picked out by.
#[tauri::command]
async fn call_it(
    app: AppHandle,
    held: State<'_, Held>,
    id: String,
    name: String,
) -> Result<(), String> {
    write_it_down_if_new(&app, &held, &id)?;
    held.store.call_it(&id, &name).map_err(|e| e.to_string())
}

/// The rows an agent needs, for one that has never been written down.
///
/// An agent made in the window is not written down until there is something to
/// write: somebody who makes one and then thinks better of it should not leave
/// a row behind, and certainly not a process. The cost of that is this: the
/// first thing ever said to a new agent is said to something that does not
/// exist yet, and the line written for it fails on the foreign key.
///
/// Which is what somebody saw instead of an answer, on the first thing they
/// ever typed into this app: "FOREIGN KEY constraint failed". It was written
/// before anything pointed at these rows except the engine, and the engine was
/// started after the line was written, so nothing ever went first.
///
/// Here rather than in several places, because the folder an agent works in is
/// decided here and a second opinion about that is a second folder.
///
/// Called by everything that writes something about an agent, not only by
/// saying something to it. The rest were quieter versions of the same fault: a
/// routine set on a new agent updated no rows and reported success, so the
/// panel went back to saying "this runs only when you ask it to" and the only
/// way to find out was the morning it did not happen. An agent starts existing
/// the moment somebody does something to it, and every one of these is
/// somebody doing something to it.
fn write_it_down_if_new(app: &AppHandle, held: &Held, id: &str) -> Result<(), String> {
    if held
        .store
        .conversation(id)
        .map_err(|e| e.to_string())?
        .is_some()
    {
        return Ok(());
    }
    // Its own folder per thread, so one errand cannot tidy up after another,
    // and so "the files from that thing last Tuesday" are still somewhere
    // findable.
    let home = where_things_live(app)?.join("threads").join(id);
    std::fs::create_dir_all(&home).map_err(|e| e.to_string())?;
    // Resolved, because Claude Code resolves it too, and a comparison between a
    // resolved path and an unresolved one is a comparison that fails on a
    // machine with a symlink in it.
    let home = home.canonicalize().unwrap_or(home);
    held.store
        .make_sure_it_exists(id, NOT_YET_NAMED, &home)
        .map_err(|e| e.to_string())
}

/// Open a conversation, or pick up one from before.
#[tauri::command]
async fn open_thread(app: AppHandle, held: State<'_, Held>, id: String) -> Result<(), String> {
    if held.live.lock().unwrap().contains_key(&id) {
        return Ok(()); // Already talking to it.
    }
    write_it_down_if_new(&app, &held, &id)?;

    // A thread we have met before is resumed, in the directory it was started
    // in; a new one is begun there. Both facts live in the store because
    // neither can be guessed at afterwards: resuming with the wrong flag is a
    // hard error, and resuming from the wrong directory quietly starts an empty
    // conversation wearing the same name.
    // Two lookups, because the two halves live in different places now: how to
    // reach the engine belongs to the agent, and whether this particular
    // conversation has run before belongs to the conversation.
    // Both exist by now, whether they were written a moment ago or a month ago.
    let conversation = held.store.conversation(&id).map_err(|e| e.to_string())?;
    let known = match &conversation {
        Some(c) => held.store.agent(&c.agent).map_err(|e| e.to_string())?,
        None => None,
    };
    let on_engine = known
        .as_ref()
        .map_or_else(|| "claude".to_string(), |a| a.engine.clone());
    let settings = known.as_ref().and_then(|a| a.engine_settings.clone());
    let (home, again) = match &known {
        Some(a) => (
            std::path::PathBuf::from(&a.cwd),
            conversation.as_ref().is_some_and(|c| c.opened),
        ),
        // Only if the rows went missing between being written and being read,
        // which is a broken store rather than a new agent. Working in the app's
        // own folder is a poor answer and refusing to answer at all is a worse
        // one.
        None => (where_things_live(&app)?, false),
    };
    // The folders this agent may write in beyond its own, told to the wall
    // before an engine is started behind it. A sandbox profile is fixed at
    // the moment the process starts, so this has to come first.
    if let Some(a) = &known {
        let folders = held.store.folders_allowed(&a.id).unwrap_or_default();
        errand_core::wall::also_allow(&home, folders);
    }

    // What this agent has already been told about its job. Read here, in the
    // app, because the store is the app's: an engine is handed a string and
    // never a database, which is what keeps there being one idea of what a note
    // is rather than one per engine.
    let remembers = held
        .store
        .conversation(&id)
        .ok()
        .flatten()
        .map(|c| c.agent)
        .and_then(|agent| memory::opening(&held.store, &agent).ok())
        .unwrap_or_default();

    let (engine, events): (Box<dyn Engine + Send>, _) = match on_engine.as_str() {
        "local" => {
            // Everything about a local model has to be told to it. Nothing is
            // remembered by anything outside this app, which is the difference
            // between the two: Claude Code holds its own session, and this
            // holds none.
            // Said with the reason. "No model chosen" was what this reported
            // for settings that were chosen, stored and shown in the header a
            // line above, and it sent somebody looking at the picker for a
            // fault that was in what the picker had written.
            let said = settings.unwrap_or_default();
            let settings: LlmSettings = serde_json::from_str(&said).map_err(|why| {
                format!(
                    "the model set for this thread could not be read ({why}). \
                     Choose it again in the picker at the top."
                )
            })?;
            // Ask again, behind this, whether it still holds what it did. Not
            // in front of it: the asking takes seconds against a server that is
            // slow and five against one that is off, and paying that before
            // every turn to catch a setting that changes twice a year would be
            // a poor trade. So this turn uses what is written down, the answer
            // lands while it is running, and the next turn uses the truth.
            ask_again_how_much_it_holds(&app, &said);
            let asks = known.as_ref().map_or("ask", |a| a.asks.as_str());
            // What was already said here. A local model keeps no session of
            // its own, so this is the only way a conversation survives the app
            // being closed -- and until this nothing did it: reopening one
            // handed the model its instructions and nothing else while the
            // window went on showing the whole thread, so a follow-up the next
            // morning was answered by an agent that had never read what it was
            // following up on.
            let so_far = match again {
                true => held
                    .store
                    .lines(&id)
                    .map(|lines| keeping::as_turns(&lines))
                    .unwrap_or_default(),
                // Nothing has been said here yet, so there is nothing to carry.
                false => Vec::new(),
            };
            // A local model keeps no session at all, so a conversation carried
            // on from another needs what happened told to it, the same way
            // Claude does when it cannot fork its own.
            let carried = match conversation.as_ref().filter(|c| c.carries_on) {
                Some(_) => held
                    .store
                    .lines(&id)
                    .map(|lines| keeping::as_a_reminder(&lines))
                    .unwrap_or_default(),
                None => String::new(),
            };
            let remembers = match carried.is_empty() {
                true => remembers.clone(),
                false => format!("{remembers}\n\n{carried}"),
            };
            let (it, events) = Local::open(
                settings,
                home,
                asks,
                &remembers,
                so_far,
                Some((id.clone(), held.wants.clone())),
            )
            .map_err(|e| e.to_string())?;
            (Box::new(it), events)
        }
        _ => {
            let asks = known.as_ref().map_or("ask", |a| a.asks.as_str());
            // The store's flag, or the transcript itself, whichever says yes.
            // They disagree after an engine change, which clears the flag but
            // cannot clear the file, and starting as new against a session that
            // exists is a failure the agent never recovers from.
            let again = again || errand_core::claude::already_going(&id, &home);
            // Three ways in, and the third only ever happens once. A
            // conversation that carries on from another is forked from it on
            // its first launch and is an ordinary conversation for ever after.
            let carrying = conversation.as_ref().filter(|c| c.carries_on && !again);
            let pick_up = match (again, carrying.and_then(|c| c.came_from.as_deref())) {
                (true, _) => errand_core::claude::PickUp::Again,
                // Only when the point to carry on from is the end of it. Going
                // back to somewhere earlier is handled by telling it what
                // happened, below, because Claude Code will only fork from a
                // message it named and it does not name the point you chose.
                (false, Some(parent)) if carrying.is_some_and(|c| c.carries_on_at.is_none()) => {
                    errand_core::claude::PickUp::From(parent)
                }
                _ => errand_core::claude::PickUp::New,
            };
            // What was said before, when there is no session to inherit it
            // from. Appended to the steering rather than said as a first
            // message, so it never appears in the window as something somebody
            // typed.
            let carried = match carrying {
                Some(c) if c.carries_on_at.is_some() => held
                    .store
                    .lines(&id)
                    .map(|lines| keeping::as_a_reminder(&lines))
                    .unwrap_or_default(),
                _ => String::new(),
            };
            let remembers = match carried.is_empty() {
                true => remembers.clone(),
                false => format!("{remembers}\n\n{carried}"),
            };
            // Which model, if this agent was put on one. Held in the same
            // column a local engine keeps its whole settings blob in, because
            // for Claude the entire setting is one word.
            let model: Option<String> = known
                .as_ref()
                .and_then(|a| a.engine_settings.clone())
                .filter(|m| !m.trim().is_empty());
            // A socket of this conversation's own, so that the two tools the
            // local engine gets in process are reachable by an engine that
            // runs outside it. Which conversation is asking is the socket,
            // never anything said over it.
            //
            // Bound before the process that will use it exists. The doorway
            // only connects when a tool is actually called, so the order is
            // not load-bearing, but there is no reason to have a race here.
            let door = doorway::listen(
                where_things_live(&app)?
                    .join("mcp")
                    .join(format!("{}.sock", short_enough_for_a_socket(&id))),
                id.clone(),
                held.wants.clone(),
            )
            .map_err(|e| e.to_string())?;

            let (it, events) = Claude::open(
                &id,
                &home,
                pick_up,
                asks,
                Some(door.at()),
                model.as_deref(),
                &remembers,
            )
            .map_err(|e| e.to_string())?;
            held.doorways.lock().unwrap().insert(id.clone(), door);
            (Box::new(it), events)
        }
    };
    held.live.lock().unwrap().insert(id.clone(), engine);

    // Everything it says: written down, then forwarded. In that order, so that
    // a window which reloads a moment later reads the same conversation it was
    // just shown.
    let store = held.store.clone();
    let settling = held.settling.clone();
    let watching = held.watching.clone();
    std::thread::spawn(move || {
        // Steps where the agent made a schedule of its own. Kept until the step
        // finishes, because saying what a schedule means before it exists would
        // be saying it about one that may not.
        let mut its_own_schedules: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        while let Ok(event) = events.recv() {
            // An agent in the middle of settling on a name is answering us, not
            // whoever is at the window.
            if settling.lock().unwrap().contains_key(&id) {
                match &event {
                    Event::Said {
                        text,
                        settled: true,
                    } => {
                        settling
                            .lock()
                            .unwrap()
                            .entry(id.clone())
                            .and_modify(|(_, so_far)| {
                                so_far.push_str(text);
                                so_far.push('\n');
                            });
                    }
                    Event::Done { .. } | Event::Failed { .. } => {
                        let Some((agent, said)) = settling.lock().unwrap().remove(&id) else {
                            continue;
                        };
                        if let Some(on) = read_what_it_settled_on(&said) {
                            if let Err(e) = store.settled_on(&agent, &on) {
                                eprintln!("could not write down who {agent} is: {e}");
                            } else {
                                let _ = app.emit("settled", (&agent, &on));
                            }
                        }
                    }
                    _ => {}
                }
                continue;
            }

            // The first errand is finished and nobody has named this yet. Ask
            // it who it is, now that it knows what the job was.
            // The agent that owns this conversation, if it still has not worked
            // out who it is. Asked down whichever conversation just finished,
            // because that is the one with an engine attached to it.
            let unnamed = store
                .conversation(&id)
                .ok()
                .flatten()
                .and_then(|c| store.agent(&c.agent).ok().flatten())
                .filter(|a| a.name == NOT_YET_NAMED)
                .map(|a| a.id);

            if let (true, Some(agent)) = (matches!(event, Event::Done { .. }), unnamed) {
                settling
                    .lock()
                    .unwrap()
                    .insert(id.clone(), (agent, String::new()));
                let asked = {
                    let held: State<Held> = app.state();
                    let mut live = held.live.lock().unwrap();
                    live.get_mut(&id).map(|engine| engine.say(WHO_ARE_YOU, &[]))
                };
                // Nothing to ask, or it would not take the question. Either
                // way it keeps the name it has and is asked again next time.
                if !matches!(asked, Some(Ok(()))) {
                    settling.lock().unwrap().remove(&id);
                }
            }

            // A question somebody has already answered for good is answered
            // here rather than shown again. The list is ours and applies to
            // both engines, so "always" means the same thing whichever is
            // running and can be taken back in one place.
            if let Event::NeedsYou(ask) = &event {
                let known = store
                    .conversation(&id)
                    .ok()
                    .flatten()
                    .map(|c| c.agent)
                    .filter(|agent| {
                        store
                            .already_allowed(agent, &ask.tool, &ask.detail)
                            .unwrap_or(false)
                    });
                if known.is_some() {
                    let held: State<Held> = app.state();
                    let answered = held
                        .live
                        .lock()
                        .unwrap()
                        .get_mut(&id)
                        .map(|engine| engine.answer(&ask.call, Answer::Yes));
                    // Only skipped if the answer actually went. Otherwise the
                    // card is shown, which is the safe way round.
                    if matches!(answered, Some(Ok(()))) {
                        continue;
                    }
                }
            }

            // An agent asked to do something every morning reaches for the
            // engine's scheduler, because that is the tool in front of it and
            // it has no idea this app has one. The job is real and Errand knows
            // nothing about it: not in Repeat, not run here, and gone when the
            // session is. Somebody who is told "scheduled, every day at 7:02"
            // and nothing else finds that out on the morning it does not
            // happen.
            match &event {
                Event::Doing(step) if errand_core::schedules::makes_one_of_its_own(&step.tool) => {
                    its_own_schedules.insert(step.call.clone());
                }
                // Said once the schedule exists rather than once it is
                // proposed, and only where it worked.
                Event::Did { call, outcome }
                    if its_own_schedules.remove(call) && !outcome.trim().is_empty() =>
                {
                    match store.the_app_says(
                        &id,
                        "note",
                        &errand_core::schedules::what_that_means(),
                    ) {
                        Ok(line) => {
                            let _ = app.emit(
                                "noted",
                                Noted {
                                    conversation: id.clone(),
                                    seq: line.seq,
                                    kind: "note".to_string(),
                                    text: line.text,
                                },
                            );
                        }
                        Err(why) => eprintln!("could not say whose schedule that is: {why}"),
                    }
                }
                _ => {}
            }

            // What the turn cost, written down as it ends. Only where the
            // engine said: a model on this machine costs no dollars, and a row
            // of zeroes would make every total a lie about what it totals.
            if let Event::Done {
                cost: Some(cost), ..
            } = &event
            {
                if let Ok(Some(talk)) = store.conversation(&id) {
                    if let Err(why) = store.spent(
                        &talk.agent,
                        &id,
                        cost.dollars,
                        cost.turns,
                        chrono::Local::now().timestamp_millis(),
                    ) {
                        eprintln!("could not write down what {id} cost: {why}");
                    }
                }
            }

            let written = match store.happened(&id, &event) {
                Ok(line) => line.map(|l| l.seq),
                Err(e) => {
                    // Losing a line is not worth ending the conversation over,
                    // but it must not pass in silence either.
                    eprintln!("could not write down what happened in {id}: {e}");
                    None
                }
            };
            // A session that is not there is not a fault worth repeating. Left
            // alone, the next turn asks to pick up the same missing session and
            // fails in the same words, for ever. Forgetting that it was ever
            // opened is what lets the next one start.
            if let Event::Failed { why } = &event {
                if errand_core::claude::the_session_is_gone(why) {
                    if let Err(e) = store.start_it_again(&id) {
                        eprintln!("could not let {id} start again: {e}");
                    }
                }
            }

            // A routine's turn is over, so the clock may start it again.
            if event.ends_the_turn() {
                let held: State<Held> = app.state();
                held.running.lock().unwrap().remove(&id);
                held.doing.lock().unwrap().remove(&id);
                // However it ended. The mark is only about whether one was
                // going, so a failure clears it as surely as an answer does.
                let _ = store.a_turn_ended(&id);
                // And a goal decides whether there is another one. Handed on
                // rather than done here: deciding means possibly starting the
                // next turn, and starting a turn inside the handler for the end
                // of the last one is a shape that has to be untangled sooner or
                // later. This is the same arrangement the app already uses for
                // everything an engine asks it to do.
                let said = match &event {
                    Event::Done { said, .. } => said.clone(),
                    // A turn that failed is not the end of a goal by itself.
                    // Written as the thing that is left, so that the same
                    // failure twice running trips the going-in-circles rule and
                    // a passing one does not end anything.
                    Event::Failed { why } => format!("GOAL: not yet - the turn failed: {why}"),
                    _ => String::new(),
                };
                let _ = held.goals.send((id.clone(), said));
            } else {
                // What it is on, in the words the window would use. Anything
                // that is not an ending means a turn is in flight; a step says
                // what it is, and everything else is at least "thinking".
                let held: State<Held> = app.state();
                let now = match &event {
                    Event::Doing(step) => Some(step.what.clone()),
                    Event::Said { .. } | Event::Started { .. } => Some("Writing".to_string()),
                    Event::NeedsYou(ask) => Some(format!("Waiting on you: {}", ask.asking)),
                    _ => None,
                };
                if let Some(now) = now {
                    held.doing.lock().unwrap().insert(id.clone(), now);
                }
            }

            // Anybody waiting on this conversation ending -- which is another
            // agent, sitting in a tool call, expecting an answer.
            if let Some(waiting) = watching.lock().unwrap().get(&id) {
                let _ = waiting.send(event.clone());
            }

            // What the failure actually means, in the app's own words, before it
            // is written down or put on screen. The provider's sentence is
            // accurate and says nothing somebody can act on: "401 OAuth access
            // token has been revoked" is a login that has expired and a
            // terminal command away from working, and nothing in those words
            // says so.
            let event = match &event {
                Event::Failed { why } => match errand_core::trouble::what_it_means(why) {
                    Some(trouble) => {
                        // Remembered, so the next errand is warned before it is
                        // typed rather than after. The one that prompted this
                        // cost somebody a paragraph and a screenshot.
                        let held: State<Held> = app.state();
                        *held.trouble.lock().unwrap() = match trouble.until_somebody_acts {
                            true => Some(trouble.clone()),
                            false => None,
                        };
                        let _ = app.emit("trouble", trouble.clone());
                        Event::Failed {
                            why: errand_core::trouble::as_a_line(&trouble),
                        }
                    }
                    None => event.clone(),
                },
                // Anything that got through means whatever was wrong is not
                // wrong any more, so the warning goes away on its own.
                Event::Done { .. } | Event::Said { .. } => {
                    let held: State<Held> = app.state();
                    if held.trouble.lock().unwrap().take().is_some() {
                        let _ = app.emit("trouble_over", ());
                    }
                    event.clone()
                }
                _ => event.clone(),
            };

            // How this run went, for the routine's own record. Only for runs
            // something other than a person started: a conversation somebody
            // is sitting in front of has its whole history on screen.
            if let Some(outcome) = match &event {
                Event::Done { .. } => Some("done".to_string()),
                Event::Failed { why } => Some(why.clone()),
                _ => None,
            } {
                let held: State<Held> = app.state();
                let run = held.mid_run.lock().unwrap().remove(&id);
                if let Some(run) = run {
                    let _ = store.a_run_ended(run, &outcome);
                }
            }

            tell_them(&app, &store, &id, &event);
            let _ = app.emit(
                "happened",
                Happened {
                    conversation: id.clone(),
                    seq: written,
                    event,
                },
            );
        }

        // Nothing more will come, which means the engine has stopped. Left in
        // the list of conversations that are live it goes on looking open, and
        // every turn after this one writes the question down and then does
        // nothing whatever: no answer, no failure, no sign that anything was
        // asked. Taking it out is what makes the next turn open a new one.
        let held: State<Held> = app.state();
        held.live.lock().unwrap().remove(&id);
    });
    Ok(())
}

/// Say something. Safe while it is working: that is the point of the thing.
#[tauri::command]
async fn say(
    app: AppHandle,
    held: State<'_, Held>,
    id: String,
    text: String,
    // Files dropped on the window, or images pasted into it as data URLs.
    // Nothing here means the ordinary case, which is most of them.
    attached: Option<Vec<String>>,
    // Answers with where this landed, so the window can offer to carry the
    // conversation on from a message somebody has only just sent rather than
    // only from ones read back off disk.
) -> Result<Option<i64>, String> {
    let pictures = attached
        .map(|these| pictures_from(&these))
        .transpose()?
        .unwrap_or_default();

    // Before the line, because the line points at it. An agent made in the
    // window is not written down until there is something to write, and this is
    // that moment.
    write_it_down_if_new(&app, &held, &id)?;
    let written = held.store.asked(&id, &text).map_err(|e| e.to_string())?;

    // Kept, beside the store rather than in it. The bytes used to reach the
    // engine and be thrown away, and the line said "(with a picture)" -- so a
    // conversation that had been about a picture read afterwards as a
    // conversation about nothing, and you could never see the one you sent.
    //
    // Beside rather than in, because a base64 image in a transcript line is
    // read back into the window on every reopen and a store that holds a
    // conversation should not also be an album. And beside the thread's own
    // folder rather than inside it, because that folder is where the agent
    // works: a picture kept there is one an errand can overwrite or tidy away.
    if !pictures.is_empty() {
        match keep_the_pictures(&app, &id, written.seq, &pictures) {
            Ok(named) => held
                .store
                .pictures_with(&id, written.seq, &named)
                .map_err(|e| e.to_string())?,
            // Not fatal. What somebody said is still said, and the errand still
            // runs with the picture in front of the model; what is lost is
            // being able to look at it again afterwards, which is worth a line
            // on stderr rather than a refused message.
            Err(why) => eprintln!("that picture could not be kept: {why}"),
        }
    }

    // Typed while the agent was waiting for a button to be pressed. The words
    // are the answer, and they go to the call that is waiting rather than into
    // the queue behind it. The queue is read only when the handover ends, and
    // a handover nobody presses a button on ends ten minutes later: for those
    // ten minutes somebody had answered and nothing whatever had happened.
    if let Some(handover) = a_handover_waiting_in(&held, &id) {
        if let Some((_, tell)) = held.handovers.lock().unwrap().remove(&handover) {
            let _ = tell.send(format!("{SAID_INSTEAD}{text}"));
            let _ = app.emit(
                "handed_back",
                HandedBack {
                    conversation: id.clone(),
                    handover,
                    how: text.clone(),
                },
            );
            return Ok(Some(written.seq));
        }
    }

    // Started here, because saying something is the first moment there is
    // anything for an engine to do. Looking at a conversation used to start
    // one, which cost a process and a resume for every glance, and resuming
    // does not only reload a transcript: a message the engine was sent and
    // killed before finishing is queued inside its own session and is run
    // again on the next resume. So opening the window ran an errand nobody had
    // asked for that minute, over and over, until one of them was left alone
    // long enough to finish.
    if !held.live.lock().unwrap().contains_key(&id) {
        open_thread(app.clone(), held.clone(), id.clone()).await?;
    }

    // Written down as in flight before the engine is handed it. A turn cannot
    // survive the process running it, and until this nothing anywhere knew one
    // had been going: quitting Errand mid-turn left a question with no answer
    // and nothing saying why, which reads as an app still thinking about it.
    let _ = held.store.a_turn_began(&id);

    let mut live = held.live.lock().unwrap();
    let thread = live
        .get_mut(&id)
        .ok_or_else(|| "that conversation is not open".to_string())?;
    thread.say(&text, &pictures).map_err(|e| e.to_string())?;
    Ok(Some(written.seq))
}

/// Where the pictures somebody sent are kept.
///
/// Beside the thread's own folder rather than inside it. That folder is the
/// agent's working directory, and a picture kept there is one an errand can
/// overwrite, tidy away, or list back to itself as though it were its own work.
fn where_the_pictures_are(
    app: &AppHandle,
    conversation: &str,
) -> Result<std::path::PathBuf, String> {
    let at = where_things_live(app)?.join("pictures").join(conversation);
    std::fs::create_dir_all(&at).map_err(|e| e.to_string())?;
    Ok(at)
}

/// Write the pictures down, and say what they were called.
///
/// Named after the line they belong to, so that what is on disk can be read
/// back to a particular thing somebody said without a second table saying so.
fn keep_the_pictures(
    app: &AppHandle,
    conversation: &str,
    seq: i64,
    pictures: &[errand_core::Picture],
) -> Result<Vec<String>, String> {
    use base64::Engine as _;
    let at = where_the_pictures_are(app, conversation)?;
    let mut named = Vec::new();
    for (n, one) in pictures.iter().enumerate() {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&one.base64)
            .map_err(|why| why.to_string())?;
        let name = format!("{seq}-{n}.{}", ending_for(&one.kind));
        std::fs::write(at.join(&name), bytes).map_err(|e| e.to_string())?;
        named.push(name);
    }
    Ok(named)
}

/// The file ending for a kind of picture.
fn ending_for(kind: &str) -> &'static str {
    match kind {
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => "png",
    }
}

/// One picture back, as the window can draw it.
///
/// Read when the line is drawn rather than handed over with the transcript, so
/// opening a conversation with forty screenshots in it does not put forty
/// screenshots into memory before a word of it is on screen.
///
/// The name is checked rather than trusted. It comes back from the store, but a
/// name is a path, and a path with `..` in it is a way to read anything on the
/// disk through a command whose whole job is handing bytes to the window.
#[tauri::command]
async fn a_picture(app: AppHandle, conversation: String, name: String) -> Result<String, String> {
    use base64::Engine as _;
    if name.contains('/') || name.contains("..") || name.is_empty() {
        return Err("that is not a picture in this conversation".to_string());
    }
    let at = where_the_pictures_are(&app, &conversation)?.join(&name);
    let bytes = std::fs::read(&at).map_err(|_| "that picture is not here any more".to_string())?;
    let kind = match at.extension().and_then(|e| e.to_str()) {
        Some("jpg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "image/png",
    };
    Ok(format!(
        "data:{kind};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// A picture an agent made or fetched, for the window to draw.
///
/// Different from `a_picture`, which hands back one somebody attached and is
/// confined to the folder those are kept in. This one takes a path, because the
/// whole point is a file the agent has just written somewhere of its own
/// choosing -- and the alternative is what happened without it: an answer
/// saying "here is the picture" followed by a path, and no picture.
///
/// Bounded three ways rather than trusted. It must be an image by its ending,
/// it must be inside somewhere this app or its agents actually work, and it
/// must be small enough to be a picture rather than a disk image somebody
/// renamed. Between them those keep a command whose job is handing bytes to
/// the window from becoming a way to read any file on the machine.
#[tauri::command]
async fn a_local_picture(app: AppHandle, path: String) -> Result<String, String> {
    use base64::Engine as _;
    let at = std::path::Path::new(&path);
    let kind = match at
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => return Err("that is not a picture".to_string()),
    };
    let real = at
        .canonicalize()
        .map_err(|_| "there is nothing there".to_string())?;
    if !somewhere_an_agent_works(&app, &real) {
        return Err("that is not somewhere an agent of yours works".to_string());
    }
    let how_big = std::fs::metadata(&real).map_err(|e| e.to_string())?.len();
    if how_big > A_PICTURE_AT_MOST {
        return Err(format!(
            "that is {}MB, which is too big to show here",
            how_big / 1024 / 1024
        ));
    }
    let bytes = std::fs::read(&real).map_err(|e| e.to_string())?;
    Ok(format!(
        "data:{kind};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// Whether a file is somewhere this app's agents actually work.
///
/// Their own folders, and the temporary places a shell puts things -- which is
/// where a downloaded file lands, and is the case this exists for. Compared
/// after resolving both sides, because a comparison between a resolved path and
/// an unresolved one fails on a machine with a symlink in it, and `/tmp` on a
/// Mac is one.
fn somewhere_an_agent_works(app: &AppHandle, real: &std::path::Path) -> bool {
    let mut allowed: Vec<std::path::PathBuf> = vec![std::env::temp_dir()];
    if let Ok(here) = where_things_live(app) {
        allowed.push(here);
    }
    allowed
        .into_iter()
        .filter_map(|one| one.canonicalize().ok())
        .any(|one| real.starts_with(one))
}

/// A picture somebody has just dropped on the window, so they can see it.
///
/// Separate from `a_local_picture`, which draws a picture an agent named and is
/// bounded to the places agents work -- a model can write any path it likes and
/// that one must not become a way to read the disk. This is the other
/// direction: a file the person chose themselves, a moment before the app reads
/// exactly the same bytes to send it. So the rules here are the ones `send`
/// already applies, and nothing wider: it has to be a picture, and it has to be
/// small enough to be one.
#[tauri::command]
async fn a_picture_to_send(path: String) -> Result<String, String> {
    use base64::Engine as _;
    let at = std::path::Path::new(&path);
    let kind = match at
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => return Err("that is not a picture".to_string()),
    };
    let how_big = std::fs::metadata(at)
        .map_err(|_| "there is nothing there".to_string())?
        .len();
    if how_big > A_PICTURE_AT_MOST {
        return Err(format!(
            "that is {}MB, and a picture has to be under {}MB",
            how_big / 1024 / 1024,
            A_PICTURE_AT_MOST / 1024 / 1024
        ));
    }
    let bytes = std::fs::read(at).map_err(|e| e.to_string())?;
    Ok(format!(
        "data:{kind};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// Show a file in Finder, without opening it.
///
/// Revealed rather than opened, and that is the whole of the difference: an
/// answer ending "I saved it to ~/Desktop/report.pdf" is a dead end without
/// this, and opening whatever a path points at would be a way to run something
/// on the strength of a line a model wrote.
#[tauri::command]
async fn show_in_finder(path: String) -> Result<(), String> {
    let at = std::path::Path::new(&path)
        .canonicalize()
        .map_err(|_| "there is nothing there any more".to_string())?;
    std::process::Command::new("open")
        .arg("-R")
        .arg(&at)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// The most an attached picture may be, before base64.
///
/// Generous for a screenshot and firm about a video somebody dragged in by
/// mistake. Both engines have their own limits and neither says so kindly: the
/// failure is a turn that dies somewhere far from the drop.
const A_PICTURE_AT_MOST: u64 = 8 * 1024 * 1024;

/// Turn what the window handed over into something an engine can be given.
///
/// Two shapes arrive, because two gestures produce them: dropping a file gives
/// a path, and pasting gives the bytes with no name at all.
fn pictures_from(these: &[String]) -> Result<Vec<errand_core::Picture>, String> {
    use base64::Engine as _;
    let mut ready = Vec::new();
    for one in these {
        if let Some(rest) = one.strip_prefix("data:") {
            let (kind, data) = rest
                .split_once(";base64,")
                .ok_or("that is not a picture this understands")?;
            ready.push(errand_core::Picture {
                kind: kind.to_string(),
                base64: data.to_string(),
            });
            continue;
        }

        let at = std::path::Path::new(one);
        let kind = match at
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase)
            .as_deref()
        {
            Some("png") => "image/png",
            Some("jpg" | "jpeg") => "image/jpeg",
            Some("gif") => "image/gif",
            Some("webp") => "image/webp",
            // Not a picture. Left alone rather than refused: a dropped file is
            // still a path the agent can open, which is what dropping one did
            // before pictures were understood at all.
            _ => continue,
        };
        let how_big = std::fs::metadata(at).map_err(|e| e.to_string())?.len();
        if how_big > A_PICTURE_AT_MOST {
            return Err(format!(
                "{} is {}MB, and a picture has to be under {}MB",
                at.file_name().unwrap_or_default().to_string_lossy(),
                how_big / 1024 / 1024,
                A_PICTURE_AT_MOST / 1024 / 1024
            ));
        }
        let bytes = std::fs::read(at).map_err(|e| e.to_string())?;
        ready.push(errand_core::Picture {
            kind: kind.to_string(),
            base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        });
    }
    Ok(ready)
}

/// Answer a question the thread stopped to ask.
///
/// Written down first, then sent, for the same reason as saying anything else:
/// what somebody decided is worth keeping even if the agent has gone. And the
/// order matters more here than there, because the moment the answer lands the
/// agent starts working again and its next line may arrive before ours.
#[tauri::command]
async fn answer(
    held: State<'_, Held>,
    id: String,
    call: String,
    step: String,
    said: String,
    tool: String,
    rule: String,
) -> Result<(), String> {
    let said = match said.as_str() {
        "yes" => Answer::Yes,
        "always" => Answer::Always,
        "no" => Answer::No,
        other => return Err(format!("no idea what \"{other}\" means")),
    };
    held.store
        .answered(&id, &step, in_a_word(said))
        .map_err(|e| e.to_string())?;

    // Remembered here rather than handed to the engine. Claude Code would file
    // an "always" in its own settings, where this app could neither show it nor
    // take it back, and the local engine would keep it in memory until the
    // conversation ended. One list, ours, either way.
    if matches!(said, Answer::Always) {
        if let Some(agent) = held
            .store
            .conversation(&id)
            .map_err(|e| e.to_string())?
            .map(|c| c.agent)
        {
            held.store
                .allow(&agent, &tool, &rule)
                .map_err(|e| e.to_string())?;
        }
    }

    let mut live = held.live.lock().unwrap();
    let thread = live
        .get_mut(&id)
        .ok_or_else(|| "that conversation is not open".to_string())?;
    thread.answer(&call, said).map_err(|e| e.to_string())
}

/// What an answer is called when it is read back later.
fn in_a_word(said: Answer) -> &'static str {
    match said {
        Answer::Yes => "You said yes",
        Answer::Always => "You said yes, and to stop asking",
        Answer::No => "You said no",
    }
}

/// One thing that could answer a thread.
#[derive(Clone, Serialize)]
struct Choice {
    /// `claude`, or `local`.
    engine: String,
    /// What it is called on screen.
    name: String,
    /// Everything a local model needs to be reached, as JSON. Nothing for
    /// Claude.
    settings: Option<String>,
}

/// What the picker shows.
///
/// A list somebody keeps, and nothing else. This used to be a question asked
/// every time the dropdown opened -- probe this machine, probe the network if
/// asked -- and that was wrong in four ways at once: slow every time, different
/// every time, mostly full of models that were not loaded and could not answer
/// without a wait, and it forgot anything anybody chose. Finding models is a
/// thing you do once, in the place for doing it, and this is only the result.
#[tauri::command]
async fn engines(held: State<'_, Held>) -> Result<Vec<Choice>, String> {
    Ok(held
        .store
        .offered()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|one| Choice {
            engine: one.engine,
            name: one.label,
            settings: one.settings,
        })
        .collect())
}

/// Somewhere models are served from, with what it is offering right now.
#[derive(Serialize)]
struct Somewhere {
    id: String,
    label: String,
    provider: String,
    base_url: String,
    /// Whether a key is kept for it. Never the key itself: it goes in one
    /// direction only, and there is no command anywhere that hands one back.
    has_key: bool,
    /// Which protocol it speaks: `openai` or `anthropic`.
    wire: String,
    /// True when this was found by looking rather than read from the store, so
    /// the screen can offer to keep it.
    found: bool,
    /// What it says it has. Empty until asked.
    models: Vec<errand_core::local::ready::Ready>,
    /// Why it could not be reached, if it could not.
    trouble: Option<String>,
}

/// Look for models, here or on the network.
///
/// Only ever from the settings screen. This is the slow thing, and the whole
/// point of the change is that it happens when somebody asks for it rather than
/// every time a dropdown opens.
#[tauri::command]
async fn look_for_models(wider: Option<bool>) -> Result<Vec<Somewhere>, String> {
    let mut found = find::detect_all().await;
    if wider.unwrap_or(false) {
        found.extend(find::scan_local_network().await);
    }
    let mut seen = std::collections::HashSet::new();
    let found: Vec<_> = found
        .into_iter()
        .filter(|b| seen.insert(b.base_url.clone()))
        .collect();

    let mut all = Vec::new();
    for one in found {
        let models =
            errand_core::local::ready::what_can_answer(&one.provider, &one.base_url, &one.models)
                .await;
        all.push(Somewhere {
            id: one.base_url.clone(),
            label: match elsewhere(&one.base_url) {
                Some(host) => format!("{} on {host}", one.label),
                None => one.label.clone(),
            },
            provider: one.provider,
            base_url: one.base_url,
            has_key: false,
            // Anything found by looking is a local server, and they all speak
            // the usual one.
            wire: "openai".to_string(),
            found: true,
            models,
            trouble: None,
        });
    }
    Ok(all)
}

/// Ask somewhere what models it has.
///
/// The endpoint is asked rather than a list being shipped, because model names
/// change under everybody and a name compiled in here is a name that is wrong
/// by the time somebody uses it. A hosted provider that adds a model tomorrow
/// shows it tomorrow without this app being rebuilt.
#[tauri::command]
async fn models_at(held: State<'_, Held>, id: String) -> Result<Somewhere, String> {
    let one = held
        .store
        .backends()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|b| b.id == id)
        .ok_or("there is nothing here by that name")?;

    let settings = LlmSettings {
        provider: one.provider.clone(),
        base_url: one.base_url.clone(),
        api_key: keys::look_up(&one.id),
        wire: one.wire.clone(),
        ..Default::default()
    };
    let (models, trouble) = match find::list_models(&settings).await {
        Ok(named) => (
            errand_core::local::ready::what_can_answer(&one.provider, &one.base_url, &named).await,
            None,
        ),
        // The reason, as a sentence. "401 Unauthorized" and "could not resolve
        // the host" are two entirely different afternoons, and a screen that
        // says "could not reach it" for both has told nobody anything.
        Err(why) => (Vec::new(), Some(format!("{why:#}"))),
    };

    Ok(Somewhere {
        id: one.id,
        label: one.label,
        provider: one.provider,
        base_url: one.base_url,
        has_key: one.has_key,
        wire: one.wire,
        found: false,
        models,
        trouble,
    })
}

/// Remember somewhere models are served from.
///
/// The key, if there is one, goes to the keychain and its presence to the
/// store. Nothing anywhere hands it back: it is written once, read by the thing
/// that makes the request, and there is no command that returns it.
#[tauri::command]
async fn remember_backend(
    held: State<'_, Held>,
    id: Option<String>,
    label: String,
    provider: String,
    base_url: String,
    api_key: Option<String>,
    wire: Option<String>,
) -> Result<Somewhere, String> {
    let base_url = base_url.trim().to_string();
    if base_url.is_empty() {
        return Err("it needs an address".into());
    }
    let label = match label.trim() {
        "" => base_url.clone(),
        named => named.to_string(),
    };
    let id = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    // Written before the store hears about it, so that a store row claiming a
    // key exists can never outlive a keychain that does not have one.
    let key = api_key
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty());
    let has_key = match &key {
        Some(k) => {
            keys::remember(&id, k).map_err(|e| format!("could not keep the key: {e:#}"))?;
            true
        }
        // Nothing typed means leave whatever is already there, which is what
        // somebody editing the address of a thing they already set up means.
        None => keys::look_up(&id).is_some(),
    };

    // Asked where it actually serves, rather than assumed. The three hosted
    // providers somebody is most likely to add here document three different
    // shapes, and two of the three fail with a 404 that reads exactly like a
    // wrong key. Settled once, here, with the key in hand, and the address that
    // answered is the one kept.
    //
    // A failure is not fatal: it is kept anyway, with the reason shown, because
    // somebody adding a machine that is switched off right now is doing a
    // reasonable thing and should not have to type it all again later.
    let key_now = keys::look_up(&id);
    let wire = wire.unwrap_or_else(|| "openai".to_string());
    let (settled, models, trouble) = match find::settle(&base_url, key_now.as_deref(), &wire).await
    {
        Ok((where_it_is, named)) => {
            let ready =
                errand_core::local::ready::what_can_answer(&provider, &where_it_is, &named).await;
            (where_it_is, ready, None)
        }
        Err(why) => (base_url.clone(), Vec::new(), Some(format!("{why:#}"))),
    };

    held.store
        .add_backend(&errand_core::store::Backend {
            id: id.clone(),
            label: label.clone(),
            provider: provider.clone(),
            base_url: settled.clone(),
            has_key,
            wire: wire.clone(),
            added_at: chrono::Local::now().timestamp_millis(),
        })
        .map_err(|e| e.to_string())?;

    // Kept either way, and the reason where there is one, in one answer. It
    // used to report a kept backend as an error, which is two different things
    // wearing the same coat: a screen cannot tell "this failed" from "this
    // worked but the machine is off right now" if both arrive as a failure.
    Ok(Somewhere {
        id,
        label,
        provider,
        base_url: settled,
        has_key,
        wire,
        found: false,
        models,
        trouble,
    })
}

/// How often a model is asked again how much it holds.
///
/// A self-hosted model's window changes when somebody restarts the server with
/// a different flag, which is a thing that happens and not a thing that happens
/// often. Often enough to catch it the same day, rare enough that the asking is
/// invisible.
const HOW_OFTEN_TO_ASK: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);

/// Find out again, in the background, how much a model holds.
///
/// The one number in a model's settings that can go stale on its own. Errand
/// asks when the model is added to the picker and wrote the answer down, and
/// until now never asked again: a server restarted with a bigger window went on
/// being sent a third of it, with the conversation dropped early and nothing
/// anywhere saying why. Not an error, which is what makes it worth chasing --
/// the opposite direction announces itself with a refused request.
///
/// Nothing waits on this. Not the turn that triggered it, and not the window.
fn ask_again_how_much_it_holds(app: &AppHandle, said: &str) {
    let Ok(kept) = serde_json::from_str::<serde_json::Value>(said) else {
        return;
    };
    let at = |k: &str| {
        kept.get(k)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let (provider, base_url, model) = (at("provider"), at("base_url"), at("model"));
    if base_url.is_empty() || model.is_empty() {
        return;
    }
    let mark = errand_core::store::what_makes_it_the_same("local", Some(said));

    {
        // Once every few hours per model, however many agents are on it and
        // however many turns they take. Kept in memory rather than in the
        // settings, so this leaves no trace in what somebody reads and nothing
        // to migrate.
        let held: State<Held> = app.state();
        let mut asked = held.sized.lock().unwrap();
        let now = std::time::Instant::now();
        if asked
            .get(&mark)
            .is_some_and(|then: &std::time::Instant| now.duration_since(*then) < HOW_OFTEN_TO_ASK)
        {
            return;
        }
        asked.insert(mark.clone(), now);
    }

    let key = kept
        .get("api_key")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Ok(caps) = errand_core::local::find::query_model_caps(
            &provider,
            &base_url,
            key.as_deref(),
            &model,
        )
        .await
        else {
            return;
        };
        let Some(holds) = caps.context_length else {
            return;
        };
        let held: State<Held> = app.state();
        match held.store.it_holds(&mark, holds as usize) {
            // Nothing to say when nothing changed, which is almost always.
            Ok(0) => {}
            Ok(rows) => {
                eprintln!(
                    "{model} holds {holds}, not what was written down; corrected {rows} rows"
                );
                // The picker is the one place this is visible, so it is the one
                // place that has to be redrawn. Somebody looking at Settings
                // while this lands should not be reading last week's number.
                let _ = app.emit("models_changed", ());
            }
            Err(why) => eprintln!("could not write down what {model} holds: {why}"),
        }
    });
}

/// The settings for a model, with its real context window in them.
///
/// Returned unchanged when the server will not say, or when somebody has
/// already put a number in by hand: a size asked for on purpose beats one
/// discovered, because the person asking is usually working around a server
/// that is lying about what it can hold.
async fn with_its_real_size(said: String) -> String {
    let Ok(mut settings) = serde_json::from_str::<serde_json::Value>(&said) else {
        return said;
    };
    if settings.get("context_window").is_some() {
        return said;
    }
    let at = |k: &str| {
        settings
            .get(k)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let (provider, base_url, model) = (at("provider"), at("base_url"), at("model"));
    if base_url.is_empty() || model.is_empty() {
        return said;
    }
    let key = settings
        .get("api_key")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let Ok(caps) =
        errand_core::local::find::query_model_caps(&provider, &base_url, key.as_deref(), &model)
            .await
    else {
        return said;
    };
    let Some(holds) = caps.context_length else {
        return said;
    };
    // Its own answer, not a guess from it. A model that says it holds 8,192 is
    // sent 8,192 and no more, and one that says 200,000 stops being treated as
    // though it held a sixth of that.
    settings["context_window"] = serde_json::json!(holds);
    if settings.get("max_tokens").is_none() {
        settings["max_tokens"] =
            serde_json::json!(errand_core::local::room_for_an_answer(holds as usize));
    }
    settings.to_string()
}

/// Forget somewhere, its key, and everything it was offering.
#[tauri::command]
async fn forget_backend(held: State<'_, Held>, id: String) -> Result<(), String> {
    // The key first. A key left behind for something nobody can see any more is
    // a secret nobody knows they still have.
    let _ = keys::forget(&id);
    held.store.forget_backend(&id).map_err(|e| e.to_string())
}

/// Put a model in the picker.
///
/// This is where a model's real size is worked out and written down. Errand
/// already knew how to ask -- `query_model_caps` reads the context length out
/// of Ollama's model info and out of GGUF metadata -- and nothing outside its
/// own tests ever called it, so every model local or hosted was assumed to hold
/// 32,768 tokens. Wrong in both directions and both hurt: a 200k model had its
/// history dropped four times sooner than it needed to be, and an 8k model was
/// sent 32k and refused the request outright, which reads as a broken model
/// rather than a wrong setting.
///
/// Asked here rather than at every turn because it takes seconds and this is
/// somebody pressing a button on the settings screen. A server that will not
/// say is left at the usual number rather than guessed at.
#[tauri::command]
async fn offer_this(
    held: State<'_, Held>,
    engine: String,
    label: String,
    settings: Option<String>,
    backend: Option<String>,
) -> Result<(), String> {
    let settings = match settings {
        Some(said) => Some(with_its_real_size(said).await),
        None => None,
    };
    let next = held
        .store
        .offered()
        .map_err(|e| e.to_string())?
        .iter()
        .map(|o| o.sort)
        .max()
        .unwrap_or(0)
        + 1;
    held.store
        .offer(&errand_core::store::Offered {
            id: uuid::Uuid::new_v4().to_string(),
            engine,
            label,
            settings,
            backend,
            sort: next,
            // Worked out by the store, which is the one place that knows what
            // makes two of these the same.
            mark: String::new(),
        })
        .map_err(|e| e.to_string())
}

/// Give a line in the picker a name somebody chose.
#[tauri::command]
async fn call_it_something(held: State<'_, Held>, id: String, label: String) -> Result<(), String> {
    let label = label.trim();
    if label.is_empty() {
        return Err("it needs a name".into());
    }
    held.store
        .call_it_something(&id, label)
        .map_err(|e| e.to_string())
}

/// Move a line up or down the picker.
#[tauri::command]
async fn move_it(held: State<'_, Held>, id: String, up: bool) -> Result<(), String> {
    held.store.move_it(&id, up).map_err(|e| e.to_string())
}

/// Take a model out of the picker.
#[tauri::command]
async fn stop_offering(held: State<'_, Held>, id: String) -> Result<(), String> {
    held.store.stop_offering(&id).map_err(|e| e.to_string())
}

/// What everything has cost, over a stretch of time.
///
/// The one question anybody running errands has, and this app threw away the
/// answer on every turn until now. Two stretches rather than one, because
/// "today" and "this month" are different questions and a single running total
/// answers neither.
#[derive(Serialize)]
struct WhatItCost {
    today: Vec<errand_core::store::Spending>,
    this_month: Vec<errand_core::store::Spending>,
    /// Nothing has ever been paid for. Said apart from an empty list, because
    /// somebody running only local models is not somebody whose spending failed
    /// to load.
    nothing_yet: bool,
}

#[tauri::command]
async fn what_it_cost(held: State<'_, Held>) -> Result<WhatItCost, String> {
    let now = chrono::Local::now();
    // From midnight rather than twenty-four hours back: somebody asking what
    // today cost means today.
    let midnight = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|t| t.and_local_timezone(*now.offset()).earliest())
        .map_or(0, |t| t.timestamp_millis());
    let month = chrono::NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .and_then(|t| t.and_local_timezone(*now.offset()).earliest())
        .map_or(0, |t| t.timestamp_millis());

    let today = held
        .store
        .spending_since(midnight)
        .map_err(|e| e.to_string())?;
    let this_month = held
        .store
        .spending_since(month)
        .map_err(|e| e.to_string())?;
    let ever = held.store.spending_since(0).map_err(|e| e.to_string())?;
    Ok(WhatItCost {
        today,
        this_month,
        nothing_yet: ever.is_empty(),
    })
}

/// Whether this conversation is live, and whether it is stopped for somebody.
///
/// The window cannot tell from the store: a question with no answer written
/// against it is either one nobody will ever answer, because the process that
/// asked it is gone, or one that is being waited on right now. Those look
/// identical on disk and want opposite things done about them, and drawing the
/// second as the first is how an errand started from outside became
/// unanswerable -- the card said the question had expired while the engine sat
/// there waiting for it.
#[tauri::command]
async fn still_going(held: State<'_, Held>, id: String) -> Result<bool, String> {
    Ok(held.live.lock().unwrap().contains_key(&id))
}

/// What changed in the version somebody is running.
///
/// `first_time` is the only part that needs the app rather than the file: every
/// copy of Errand is installed by hand over the top of the last, so "is this a
/// version I have already been told about" cannot be worked out from the notes
/// alone, and being told the same thing at every launch is how somebody learns
/// to close a panel without reading it.
#[tauri::command]
async fn what_changed(app: AppHandle) -> Result<WhatChanged, String> {
    let here = where_things_live(&app)?;
    let first_time = !errand_core::changes::already_seen(&here);
    Ok(WhatChanged {
        first_time,
        notes: errand_core::changes::this_one(),
    })
}

/// Say that these notes have been put in front of somebody.
#[tauri::command]
async fn seen_what_changed(app: AppHandle) -> Result<(), String> {
    errand_core::changes::seen(&where_things_live(&app)?);
    Ok(())
}

/// The notes, and whether this is the first look at them.
#[derive(serde::Serialize)]
struct WhatChanged {
    first_time: bool,
    /// Nothing where a version has no notes written for it, which a test in
    /// core refuses to let happen but which should still not be a crash here.
    notes: Option<errand_core::changes::Notes>,
}

/// Whether Errand comes back by itself after a restart.
///
/// Read from the file the system obeys rather than from anything remembered
/// here, so what the switch shows and what actually happens at a login cannot
/// drift apart.
#[tauri::command]
async fn opens_at_login() -> Result<errand_core::atlogin::AtLogin, String> {
    let (home, me) = at_login_needs()?;
    Ok(errand_core::atlogin::how_it_stands(&home, &me))
}

/// Start Errand at login, or stop doing that.
///
/// Turning it on writes a file naming this copy. It does not start a second
/// one now and does not ask for a password: everything about this happens in
/// the person's own folder, and dragging the app to the bin ends it, because a
/// file naming an app that is gone is a file the system quietly gives up on.
#[tauri::command]
async fn open_at_login(yes: bool) -> Result<errand_core::atlogin::AtLogin, String> {
    let (home, me) = at_login_needs()?;
    match yes {
        true => errand_core::atlogin::turn_on(&home, &me),
        false => errand_core::atlogin::turn_off(&home),
    }
    .map_err(|why| format!("{why}"))?;
    Ok(errand_core::atlogin::how_it_stands(&home, &me))
}

/// The two paths this needs, and a sentence when either is missing.
fn at_login_needs() -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    let home =
        std::env::var("HOME").map_err(|_| "there is no home folder to write to".to_string())?;
    let me = std::env::current_exe().map_err(|why| format!("{why}"))?;
    Ok((std::path::PathBuf::from(home), me))
}

/// Everything the settings screen lists, which is the picker itself.
#[tauri::command]
async fn whats_offered(held: State<'_, Held>) -> Result<Vec<errand_core::store::Offered>, String> {
    held.store.offered().map_err(|e| e.to_string())
}

/// Everywhere somebody has told this about, and what each is offering.
#[tauri::command]
async fn backends(held: State<'_, Held>) -> Result<Vec<Somewhere>, String> {
    let kept = held.store.backends().map_err(|e| e.to_string())?;
    let mut all = Vec::new();
    for one in kept {
        all.push(Somewhere {
            id: one.id,
            label: one.label,
            provider: one.provider,
            base_url: one.base_url,
            has_key: one.has_key,
            wire: one.wire,
            found: false,
            models: Vec::new(),
            trouble: None,
        });
    }
    Ok(all)
}

/// The host, if this is not the machine somebody is sitting at.
///
/// Returned as an option rather than a string, because "on 127.0.0.1" is noise
/// in a list where nearly everything is here.
fn elsewhere(base_url: &str) -> Option<String> {
    let authority = base_url.split("//").nth(1)?.split('/').next()?;
    // A port is usual but not certain, and falling back to the whole URL when
    // there is none put "on http://10.0.0.4" in the list.
    let host = authority
        .rsplit_once(':')
        .map_or(authority, |(host, _)| host);
    match host {
        "127.0.0.1" | "localhost" | "::1" | "[::1]" => None,
        _ => Some(host.to_string()),
    }
}

/// Put a thread on a different engine.
///
/// Whatever was answering it is stopped first. Two engines in one thread would
/// both be writing into it, and the second would be talking about a
/// conversation it never had.
#[tauri::command]
async fn use_engine(
    app: AppHandle,
    held: State<'_, Held>,
    id: String,
    engine: String,
    settings: Option<String>,
) -> Result<(), String> {
    // The quietest of the lot: pick a model for a brand-new agent, say
    // something to it, and be answered by the one you did not pick.
    write_it_down_if_new(&app, &held, &id)?;
    // Every conversation this agent has, not one. The map is keyed by
    // conversation and the id here is the agent's, so removing by it stopped
    // nothing at all: the old engine kept running and kept writing, which is
    // the two-engines-in-one-conversation the comment above forbids, spread
    // across an agent instead.
    let theirs: Vec<String> = held
        .store
        .conversations(&id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|c| c.id)
        .collect();
    {
        let mut live = held.live.lock().unwrap();
        let mut doors = held.doorways.lock().unwrap();
        for conversation in theirs {
            // Both, and for the same reason: an agent moved onto a local model
            // reaches these tools in process and has no use for a socket, and
            // one moved back gets a fresh doorway when it is next opened.
            doors.remove(&conversation);
            if let Some(mut was) = live.remove(&conversation) {
                let _ = was.stop();
            }
        }
    }

    held.store
        .use_engine(&id, &engine, settings.as_deref())
        .map_err(|e| e.to_string())
}

/// One watch, held while it is being looked at.
///
/// A guard rather than an insert and a remove, because the looking has half a
/// dozen ways to end early and every one of them would otherwise leave the
/// entry behind. A left-behind entry is a watch that is never looked at again,
/// silently, for the life of the process.
struct Looking {
    at: Arc<Mutex<std::collections::HashSet<String>>>,
    id: String,
}

impl Drop for Looking {
    fn drop(&mut self) {
        self.at.lock().unwrap().remove(&self.id);
    }
}

/// A turn claimed for a conversation, so the clock does not start the same
/// routine on top of itself.
///
/// Unlike looking, this is handed over rather than simply released: once a turn
/// is really in flight the engine owns it and gives it back when the turn ends.
/// What this guards is everything before that point. Starting used to be two
/// fallible steps after the claim, each with a `?`, and either one failing left
/// the conversation claimed for as long as the app stayed open. The clock then
/// skipped that routine every morning afterwards and said nothing, which is the
/// worst shape a bug can have here: a thing that quietly stops happening.
struct Turn {
    among: Arc<Mutex<std::collections::HashSet<String>>>,
    id: String,
    engine_has_it: bool,
}

impl Turn {
    fn claim(among: Arc<Mutex<std::collections::HashSet<String>>>, id: String) -> Self {
        among.lock().unwrap().insert(id.clone());
        Self {
            among,
            id,
            engine_has_it: false,
        }
    }

    /// The engine is running this turn and will release it at the end of it.
    fn handed_to_the_engine(mut self) {
        self.engine_has_it = true;
    }
}

impl Drop for Turn {
    fn drop(&mut self) {
        if !self.engine_has_it {
            self.among.lock().unwrap().remove(&self.id);
        }
    }
}

/// A name for this conversation's socket, short enough for one.
///
/// A unix socket path has a hard length limit and the folder it sits in is
/// already long, so the name is cut down. It used to be cut with a plain slice
/// at sixteen bytes, which assumed every id was a uuid: an id shorter than that
/// panicked, and the panic landed inside whichever loop had asked for the turn.
fn short_enough_for_a_socket(id: &str) -> String {
    id.replace('-', "").chars().take(16).collect()
}

/// Look at everything that is due to be looked at.
///
/// Rides on the clock that already ticks rather than bringing a second way of
/// being concurrent. Four at a time, because twenty watches coming due together
/// must not open twenty sockets, and the rest are looked at on the next tick.
async fn look_around(app: &AppHandle) -> Result<(), String> {
    const AT_ONCE: usize = 4;
    let now = chrono::Local::now();

    let due: Vec<(String, watch::Watch)> = {
        let held: State<Held> = app.state();
        held.store
            .watching()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter_map(|c| {
                let watch = watch::Watch::read(c.watches.as_deref()?).ok()?;
                // Never on top of a turn that is already going. A watch that
                // said something into a conversation mid-answer would be two
                // people talking at once.
                let held: State<Held> = app.state();
                let busy = held.running.lock().unwrap().contains(&c.id)
                    || held.doing.lock().unwrap().contains_key(&c.id)
                    || held.looking.lock().unwrap().contains(&c.id);
                let due = match c.looked_at {
                    None => true,
                    Some(then) => now.timestamp_millis() - then >= watch.every * 60 * 1000,
                };
                (due && !busy).then_some((c.id.clone(), watch))
            })
            .take(AT_ONCE)
            .collect()
    };

    for (conversation, watch) in due {
        let held: State<Held> = app.state();
        held.looking.lock().unwrap().insert(conversation.clone());
        let _guard = Looking {
            at: held.looking.clone(),
            id: conversation.clone(),
        };
        if let Err(why) = look_once(app, &conversation, &watch, now).await {
            eprintln!("looking at {conversation}: {why}");
        }
    }
    Ok(())
}

/// The most times a watch may find something odd before it stops.
///
/// A watch that can never settle, or can never be reached, is worse than no
/// watch: it looks for ever and says nothing. Roughly an hour at the page
/// floor, which is long enough not to trip over one odd answer and short
/// enough to say so the same morning.
const ODD_LOOKS_BEFORE_STOPPING: i64 = 5;

/// Look at one thing, and wake somebody if it has really changed.
async fn look_once(
    app: &AppHandle,
    conversation: &str,
    watch: &watch::Watch,
    now: chrono::DateTime<chrono::Local>,
) -> Result<(), String> {
    let was = {
        let held: State<Held> = app.state();
        held.store
            .conversation(conversation)
            .map_err(|e| e.to_string())?
            .ok_or("it went away while being looked at")?
    };

    let seen = match watch::look_again(&watch.look, was.saw.as_deref()).await {
        Ok(seen) => seen,
        Err(why) => {
            // Counted rather than retried for ever. A watch on an address that
            // has stopped answering must stop, not fail seven hundred times a
            // day in silence.
            let misses = was.misses + 1;
            let held: State<Held> = app.state();
            held.store
                .looked(
                    conversation,
                    None,
                    None,
                    was.seeing.as_deref(),
                    was.unsettled,
                    misses,
                )
                .map_err(|e| e.to_string())?;
            if misses >= ODD_LOOKS_BEFORE_STOPPING {
                let reason = format!(
                    "Stopped looking. {} could not be reached {misses} times running.                      The last thing it said was: {why}",
                    watch.written()
                );
                held.store
                    .pause_watch(conversation, &reason)
                    .map_err(|e| e.to_string())?;
                held.store
                    .noted(conversation, &reason)
                    .map_err(|e| e.to_string())?;
            }
            return Ok(());
        }
    };

    match watch::compare(was.saw.as_deref(), was.seeing.as_deref(), &seen.mark) {
        // Nothing was known. This is now what is known, and nobody is woken.
        watch::Next::FirstSight => {
            let held: State<Held> = app.state();
            held.store
                .looked(conversation, Some(&seen.mark), Some(&seen.note), None, 0, 0)
                .map_err(|e| e.to_string())
        }
        watch::Next::Same => {
            let held: State<Held> = app.state();
            held.store
                .looked(conversation, None, None, None, 0, 0)
                .map_err(|e| e.to_string())
        }
        // Different, and not yet different the same way twice.
        watch::Next::Settling => {
            let unsettled = was.unsettled + 1;
            let held: State<Held> = app.state();
            held.store
                .looked(conversation, None, None, Some(&seen.mark), unsettled, 0)
                .map_err(|e| e.to_string())?;
            if unsettled >= ODD_LOOKS_BEFORE_STOPPING {
                let reason = format!(
                    "Stopped looking. {} is different every time I look, so I cannot tell a                      real change from the parts that always change. Some pages put a new                      token in every answer. Try watching a feed or an API for the same thing                      if there is one.",
                    watch.written()
                );
                held.store
                    .pause_watch(conversation, &reason)
                    .map_err(|e| e.to_string())?;
                held.store
                    .noted(conversation, &reason)
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        }
        watch::Next::Changed => {
            // The brakes, and they are checked before anything is written.
            // Nothing is lost by refusing here: the mark still equals what was
            // being seen, so the same change is still a change on the next
            // look, and it fires as soon as the brake lets go.
            let today: i64 = now.format("%Y%m%d").to_string().parse().unwrap_or(0);
            let too_soon = was.woke_at.is_some_and(|then| {
                now.timestamp_millis() - then < watch::WAKE_NO_OFTENER_THAN * 60 * 1000
            });
            let enough_today = was.woke_on == Some(today) && was.woke_today >= watch::WAKES_A_DAY;
            if too_soon || enough_today {
                return Ok(());
            }

            let said = format!(
                "{}

{}",
                was.watches_what.clone().unwrap_or_default(),
                watch::what_to_say(
                    watch,
                    was.saw_note.as_deref(),
                    &seen.note,
                    was.woke_at
                        .map(|then| {
                            format!(
                                "at {}",
                                chrono::DateTime::from_timestamp_millis(then)
                                    .map(|t| t
                                        .with_timezone(&chrono::Local)
                                        .format("%H:%M")
                                        .to_string())
                                    .unwrap_or_default()
                            )
                        })
                        .as_deref(),
                )
            );

            // Written down before anybody is woken, for the reason a routine's
            // last run is: if starting fails it has still had its turn, rather
            // than trying again every thirty seconds for the rest of the day.
            let turn = {
                let held: State<Held> = app.state();
                held.store
                    .woke(conversation, &seen.mark, &seen.note, today)
                    .map_err(|e| e.to_string())?;
                // A watch waking somebody is a run like any other, and the same
                // question is asked of it: has this been working.
                if let Ok(run) = held.store.a_run_began(conversation, "watch") {
                    held.mid_run
                        .lock()
                        .unwrap()
                        .insert(conversation.to_string(), run);
                }
                Turn::claim(held.running.clone(), conversation.to_string())
            };
            say(
                app.clone(),
                app.state(),
                conversation.to_string(),
                said,
                None,
            )
            .await?;
            turn.handed_to_the_engine();
            Ok(())
        }
    }
}

/// A line the app itself put into a conversation, on its way to the window.
#[derive(Clone, Serialize)]
struct Noted {
    conversation: String,
    seq: i64,
    kind: String,
    text: String,
}

/// A conversation's goal, as the window shows it.
#[derive(Serialize)]
struct Aiming {
    /// What it is trying to get to. Nothing means it has no goal.
    goal: Option<String>,
    /// What that will actually do, in numbers, before anybody agrees to it.
    means: String,
    tries: i64,
    at_most: i64,
    /// What the agent last said was still to do.
    left: Option<String>,
    /// Why it ended, if it has.
    over: Option<String>,
}

/// What this conversation is trying to get to.
#[tauri::command]
async fn goal_of(held: State<'_, Held>, id: String) -> Result<Aiming, String> {
    let talk = held
        .store
        .conversation(&id)
        .map_err(|e| e.to_string())?
        .ok_or("there is no such conversation")?;
    Ok(Aiming {
        means: match talk.goal.as_deref() {
            Some(what) => goal::what_it_means(what),
            None => String::new(),
        },
        goal: talk.goal,
        tries: talk.goal_tries,
        at_most: goal::AT_MOST_TRIES,
        left: talk.goal_left,
        over: talk.goal_over,
    })
}

/// Set or change what this conversation is trying to get to.
///
/// Setting one starts it, because a goal that has to be set and then separately
/// kicked off is two steps where somebody meant one, and the second step is the
/// one that gets forgotten. Clearing it stops it where it stands.
#[tauri::command]
async fn aim_at(
    app: AppHandle,
    held: State<'_, Held>,
    id: String,
    goal: Option<String>,
) -> Result<(), String> {
    write_it_down_if_new(&app, &held, &id)?;
    let goal = goal.map(|g| g.trim().to_string()).filter(|g| !g.is_empty());
    held.store
        .aim_at(
            &id,
            goal.as_deref(),
            chrono::Local::now().timestamp_millis(),
        )
        .map_err(|e| e.to_string())?;
    let Some(what) = goal else {
        return Ok(());
    };
    say(
        app.clone(),
        app.state(),
        id,
        goal::how_to_report(&what, 0, None),
        None,
    )
    .await
    .map(|_| ())
}

/// Whether a goal has another turn in it, and starting that turn if it has.
///
/// Everything that decides this lives in `goal`; what is here is the part that
/// touches the store and sends the next message. Three of the four endings are
/// failures of a kind, and each says a different thing, because "it finished",
/// "it ran out of turns", "it went round in circles" and "it stopped saying
/// where it was" want three different things done about them and one
/// celebration.
async fn keep_at_it(app: &AppHandle, id: &str, said: &str) -> Result<(), String> {
    let talk = {
        let held: State<Held> = app.state();
        held.store
            .conversation(id)
            .map_err(|e| e.to_string())?
            .ok_or("there is no such conversation")?
    };
    // No goal, or one that has already ended. The second is the one that
    // matters: without it the message saying a goal is over would itself end a
    // turn and start the whole thing again.
    let (Some(what), None) = (talk.goal.as_deref(), talk.goal_over.as_deref()) else {
        return Ok(());
    };

    let next = goal::read(said, talk.goal_tries, talk.goal_left.as_deref());
    let over = match &next {
        goal::Next::Keep(_) => None,
        goal::Next::Done => Some("done"),
        goal::Next::Circling(_) => Some("going round"),
        goal::Next::Enough => Some("out of turns"),
        goal::Next::Silent => Some("stopped reporting"),
    };
    let left = match &next {
        goal::Next::Keep(left) | goal::Next::Circling(left) => Some(left.clone()),
        _ => None,
    };
    {
        let held: State<Held> = app.state();
        held.store
            .got_to(id, left.as_deref(), over)
            .map_err(|e| e.to_string())?;
    }

    // Written down before the next turn starts, and before anything can fail,
    // for the reason a routine's last run is: a goal that spends a turn and
    // does not record it would spend that turn again.
    if next.over() {
        let line = {
            let held: State<Held> = app.state();
            held.store
                .the_app_says(id, "goal", &next.in_plain_words(what))
                .map_err(|e| e.to_string())?
        };
        // Told to the window as well as written down. A line that only appears
        // when somebody clicks away and back is a line nobody sees at the
        // moment it is about, and the moment is the whole of it: this is what
        // says a goal has stopped and why.
        let _ = app.emit(
            "noted",
            Noted {
                conversation: id.to_string(),
                seq: line.seq,
                kind: "goal".to_string(),
                text: line.text,
            },
        );
        return Ok(());
    }

    say(
        app.clone(),
        app.state(),
        id.to_string(),
        goal::how_to_report(what, talk.goal_tries + 1, left.as_deref()),
        None,
    )
    .await
    .map(|_| ())
}

/// What an agent is asked once it has done its first errand.
///
/// After rather than before, and that is the whole design. Grok Bot asks a new
/// bot what it is for and it names itself from the answer; asking here before
/// anything has happened would mean an interview standing between somebody and
/// the thing they came to get done. So the first errand runs, and then it is
/// asked to look at what it just did and say who it is.
///
/// One line, because parsing anything an agent writes is a fight, and a fight
/// that is lost silently: a name that comes back wrapped in an apology becomes
/// the agent's name. Five fields, fixed vocabularies for the two that the
/// window has to draw, and a demand for nothing else.
const WHO_ARE_YOU: &str = "\
Before anything else: you are a standing agent, not a one-off. Somebody will \
come back to you for this kind of job again. Settle on who you are, from the \
errand you just did.\n\n\
Reply with ONE line and nothing else, five fields separated by |\n\n\
name | title | about | mark | hue\n\n\
name: two or three words somebody would call you, like a colleague. Not a \
sentence, not \"Errand\", not the request you were given.\n\
title: one word for the role. Mail, Research, Files, Money, Code, Travel.\n\
about: one sentence, what you handle, in your own words.\n\
mark: exactly one of mail clock chart search folder image code globe bag pen \
chat person list terminal helper bell spark\n\
hue: exactly one of amber blue green purple teal rose gold\n\n\
No preamble, no explanation, no quotes. Just the line.";

/// Read back what it settled on, if it answered the way it was asked to.
///
/// Strict about the two fields the window draws and forgiving about the three
/// it only shows: an unrecognised mark would be an agent with no face, while an
/// over-long name is merely an agent with an over-long name.
///
/// Returns nothing at all rather than a half-filled identity. An agent that
/// answered in prose keeps the name it had, and gets asked again next time,
/// which is a better outcome than being called "Certainly! Here is".
fn read_what_it_settled_on(said: &str) -> Option<Settled> {
    const MARKS: &[&str] = &[
        "mail", "clock", "chart", "search", "folder", "image", "code", "globe", "bag", "pen",
        "chat", "person", "list", "terminal", "helper", "bell", "spark",
    ];
    const HUES: &[&str] = &["amber", "blue", "green", "purple", "teal", "rose", "gold"];

    let line = said
        .lines()
        .map(str::trim)
        .find(|l| l.matches('|').count() >= 4)?;
    let mut fields = line.splitn(5, '|').map(str::trim);

    let name = fields
        .next()?
        .trim_matches(['"', '*', '#', ' '])
        .to_string();
    let title = fields.next()?.to_string();
    let about = fields.next()?.to_string();
    let mark = fields.next()?.to_lowercase();
    let hue = fields.next()?.to_lowercase();

    if name.is_empty() || name.len() > 60 {
        return None;
    }
    Some(Settled {
        name,
        title: title.chars().take(24).collect(),
        about: about.chars().take(200).collect(),
        // A mark it invented is not one the window can draw, so the guess from
        // the words stands instead.
        mark: MARKS.iter().find(|m| mark.contains(**m))?.to_string(),
        hue: HUES
            .iter()
            .find(|h| hue.contains(**h))
            .unwrap_or(&"amber")
            .to_string(),
    })
}

/// Put one sentence on screen, before there is a window to put it in.
///
/// Through the system rather than through a dialog plugin, which would be a
/// dependency for one message. The same route the connectors take, and the
/// only one available at the moment this is needed: this runs before the app
/// has finished being built, which is exactly when something can go wrong that
/// leaves it with no window at all.
fn say_it_out_loud(title: &str, said: &str) {
    let quoted = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let _ = std::process::Command::new("osascript")
        .arg("-e")
        .arg(format!(
            "display alert \"{}\" message \"{}\" as critical",
            quoted(title),
            quoted(said)
        ))
        .status();
}

/// Say so, in the conversations that were mid-turn when this last stopped.
///
/// A turn cannot outlive the process running it. Quitting Errand while one was
/// going killed the engine and left the transcript holding a question with no
/// answer and nothing at all saying why -- which from the window is
/// indistinguishable from an app still thinking about it, and stays that way
/// for ever.
///
/// Said rather than restarted. Running it again on its own would be an errand
/// nobody asked for that minute, which is a fault this app has had once
/// already and does not want back; the line says what happened and offers to
/// run it again, and the person decides.
fn say_what_was_cut_off(app: &AppHandle) {
    let held: State<Held> = app.state();
    let Ok(cut_off) = held.store.turns_that_were_cut_off() else {
        return;
    };
    for conversation in cut_off {
        let _ = held.store.the_app_says_about(
            &conversation,
            "ended",
            "Errand was closed while this was running, so it stopped part way. \
             Nothing already written down was lost.",
            WAS_CUT_OFF,
        );
        let _ = held.store.a_turn_ended(&conversation);
    }
}

/// What marks the line saying a turn was cut off.
///
/// On the line rather than worked out from its words, because the window puts
/// a way to run it again on this one and nothing else, and matching on a
/// sentence is how that stops working the day the sentence is reworded.
pub const WAS_CUT_OFF: &str = "cut-off";

/// Sockets left behind by an app that did not get to tidy up.
///
/// A doorway unlinks its own socket when its conversation closes, and the app
/// unlinks before binding, so this is only about the ones nothing will ever
/// bind again. Safe to do at startup because two copies of this app were never
/// supported anyway: they would share one SQLite store.
fn sweep_up_after_a_crash(here: &std::path::Path) {
    let Ok(left) = std::fs::read_dir(here.join("mcp")) else {
        return;
    };
    for one in left.flatten() {
        if one.path().extension().is_some_and(|e| e == "sock") {
            let _ = std::fs::remove_file(one.path());
        }
    }
}

/// Say so if somebody already has a server by the name we use.
///
/// Which of two servers with one name wins is not something this app decides,
/// and it is not written down anywhere either. So rather than depend on it,
/// this looks, and says what it found. A delegation that quietly reached
/// somebody else's server would be very hard to work out from the symptom.
fn say_if_the_name_is_taken(here: &std::path::Path) {
    if mcp::configured(here)
        .iter()
        .any(|server| server.name == team::DOORWAY)
    {
        eprintln!(
            "warning: an MCP server called `{}` is already configured. Handing work between \
             agents may reach that one instead of this app's own.",
            team::DOORWAY
        );
    }
}

/// The receiving end, held between setup and Ready.
struct Waiting(Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<team::Wants>>>);

/// Do the things an engine cannot do for itself.
///
/// One at a time on purpose. Two agents delegating at once is a thing that will
/// happen and a thing nobody has thought through: it means two conversations
/// running, either of which may delegate again. Serialising it makes the first
/// version something whose behaviour can be predicted, and the queue is where
/// that decision is written down rather than assumed.
/// The receiving end of turns that have ended, parked until the app is up.
///
/// Parked for the same reason the other one is: setup runs while the app is
/// still being built, and spawning work into it there is how the window stops
/// appearing at all.
struct TurnsThatEnded(Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<(String, String)>>>);

/// One task, reading the ends of turns and deciding whether a goal goes again.
fn carry_on_goals(
    app: AppHandle,
    mut ended: tokio::sync::mpsc::UnboundedReceiver<(String, String)>,
) {
    tauri::async_runtime::spawn(async move {
        while let Some((id, said)) = ended.recv().await {
            if let Err(why) = keep_at_it(&app, &id, &said).await {
                eprintln!("the goal in {id}: {why}");
            }
        }
    });
}

fn answer_what_engines_cannot(
    app: AppHandle,
    mut wants: tokio::sync::mpsc::UnboundedReceiver<team::Wants>,
) {
    tauri::async_runtime::spawn(async move {
        while let Some(asked) = wants.recv().await {
            // One task each. Handling these one at a time was the careful
            // first version and stopped being careful the moment a delegated
            // conversation could delegate: the second request waits behind the
            // first, and the first is waiting for the second. What that looked
            // like was not a hang but a lie -- the first agent was told "it did
            // not finish within ten minutes" about work that had never
            // started.
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                // Matched exhaustively on purpose. A tool declared to both
                // engines and forgotten here would be offered, called, and
                // answered with "there is no ... here" on both engines
                // identically -- a symmetric failure, so it would not even look
                // like the asymmetry this arrangement exists to prevent. The
                // enum makes forgetting one a build error instead of a bug.
                let said = match team::which_of_ours(&asked.tool) {
                    Some(team::Ours::WhoElse) => who_else(&app, &asked.from),
                    Some(team::Ours::Ask) => ask_teammate(&app, &asked).await,
                    Some(team::Ours::Remember) => write_it_down(&app, &asked),
                    Some(team::Ours::Recall) => look_it_up(&app, &asked),
                    Some(team::Ours::Forget) => take_it_back(&app, &asked),
                    Some(team::Ours::EveryDay) => set_it_running(&app, &asked),
                    Some(team::Ours::KeepAnEyeOn) => keep_an_eye_on(&app, &asked),
                    Some(team::Ours::OverToYou) => over_to_you(&app, &asked).await,
                    // Not one of the app's own, so it may be one of the things
                    // this Mac can be let at.
                    None => match errand_core::connectors::which(&asked.tool) {
                        Some(job) => reach_for_it(&app, job, &asked.args),
                        None => Err(anyhow::anyhow!("there is no {} here", asked.tool)),
                    },
                };
                let _ = asked.answer.send(said);
            });
        }
    });
}

/// Whose notebook this is.
///
/// Loud where `who_else` is tolerant, and the difference is deliberate: "who
/// else is there" is still an answerable question without knowing who is
/// asking, and a note with no agent behind it is not a smaller note, it is a
/// note in somebody else's book. The row always exists by the time an engine
/// can call a tool, because opening a conversation writes it before either
/// engine starts, so this failing means something is wrong that guessing would
/// hide.
fn whose_notebook(app: &AppHandle, from: &str) -> anyhow::Result<String> {
    let held: State<Held> = app.state();
    held.store
        .conversation(from)?
        .map(|c| c.agent)
        .ok_or_else(|| {
            anyhow::anyhow!("this conversation has no agent, so there is no notebook to write in")
        })
}

/// Write something down, or correct what was written before.
fn write_it_down(app: &AppHandle, asked: &team::Wants) -> anyhow::Result<String> {
    let agent = whose_notebook(app, &asked.from)?;
    let said = |k: &str| asked.args.get(k).and_then(|v| v.as_str()).unwrap_or("");
    let about = memory::a_handle(said("about"))?;
    let note = memory::a_note(said("note"))?;

    let held: State<Held> = app.state();
    held.store.remember(&agent, &about, &note)?;
    Ok(format!(
        "Written down under `{about}`. Saying that handle again will replace it."
    ))
}

/// Set this conversation to run itself, because it was asked to.
///
/// The gap this closes is the one anybody comparing Errand with anything else
/// notices first. Somebody says "check this every day and tell me when it
/// ships", and an agent that cannot set a schedule has two answers, both bad:
/// ask questions until somebody sets one by hand, or use the engine's own
/// scheduler, which this app then has to apologise for because it does not run
/// it and cannot show it.
///
/// The schedule is this conversation's, never another's. An agent that could
/// put a standing job into somebody else's conversation could put one anywhere,
/// and the only conversation it has any business changing is the one it is in.
fn set_it_running(app: &AppHandle, asked: &team::Wants) -> anyhow::Result<String> {
    let said = |k: &str| {
        asked
            .args
            .get(k)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
    };
    let (when, what) = (said("when"), said("what"));
    if what.is_empty() {
        anyhow::bail!("say what it should do each time, or there is nothing to run");
    }
    // Read before it is stored, so an unreadable schedule is refused here, in a
    // sentence the model can act on, rather than at seven in the morning by not
    // happening.
    let read = When::read(when)?;

    let held: State<Held> = app.state();
    held.store.runs(&asked.from, Some(when), Some(what))?;

    let next = read
        .next_after(chrono::Local::now())
        .map(|at| at.format("%A %-d %B at %H:%M").to_string())
        .unwrap_or_else(|| "at its next turn".to_string());
    Ok(format!(
        "Set. This conversation now runs {}, next on {next}, and says: {what}\n\n\
         It is under Repeat, where it can be changed or stopped. It runs while Errand is \
         open; a Mac that is asleep is still asleep.",
        read.written()
    ))
}

/// Do one of the things this Mac can be let at, if it has been.
///
/// The switch is the whole of the permission here, and it has to be, because
/// this runs in the app rather than in the walled engine: an agent set never to
/// ask is confined to its own folder, and this reaches outside it on purpose.
/// So an unconnected one is refused in a sentence rather than answered, and the
/// sentence says where the switch is.
fn reach_for_it(
    app: &AppHandle,
    job: &'static str,
    args: &serde_json::Value,
) -> anyhow::Result<String> {
    let wanted = errand_core::connectors::needs(job);
    let on = {
        let held: State<Held> = app.state();
        held.store.connected()?.iter().any(|one| one == wanted)
    };
    if !on {
        let named = errand_core::connectors::KNOWN
            .iter()
            .find(|c| c.id == wanted)
            .map_or(wanted, |c| c.name);
        anyhow::bail!(
            "{named} is not connected, so there is nothing to read. Say so: they can turn it \
             on under Settings, and it takes effect at once."
        );
    }
    errand_core::connectors::run(job, args)
}

/// Where some words were actually found, line by line.
///
/// The other half of a search. `matching` narrows the list of agents, which is
/// the right first answer to "which errand was that"; this says which line in
/// which conversation, so going there means arriving at it rather than at
/// whichever of that agent's conversations spoke most recently.
#[tauri::command]
async fn hits(
    held: State<'_, Held>,
    looking_for: String,
) -> Result<Vec<errand_core::store::Hit>, String> {
    held.store.hits(&looking_for).map_err(|e| e.to_string())
}

/// Switch a routine off, or back on, without throwing it away.
#[tauri::command]
async fn routine_off(held: State<'_, Held>, id: String, off: bool) -> Result<(), String> {
    held.store.routine_off(&id, off).map_err(|e| e.to_string())
}

/// How a routine has actually been going.
///
/// The question people ask about a standing job is not when it is next but
/// whether it has been working, and until this there was nothing in the app
/// that could answer it: three failed mornings left a conversation looking
/// merely quiet.
#[tauri::command]
async fn how_it_has_been_going(
    held: State<'_, Held>,
    id: String,
) -> Result<Vec<errand_core::store::Run>, String> {
    held.store
        .how_it_has_been_going(&id, 20)
        .map_err(|e| e.to_string())
}

/// Which agent a conversation belongs to.
///
/// For arriving at one from outside the window: a notification names the
/// conversation, and opening it means opening its agent first, which the window
/// cannot know for an agent it has never loaded.
#[tauri::command]
async fn conversation_agent(held: State<'_, Held>, id: String) -> Result<Option<String>, String> {
    held.store
        .conversation(&id)
        .map(|c| c.map(|c| c.agent))
        .map_err(|e| e.to_string())
}

/// What is stopping errands from working, if anything is.
///
/// Asked by the window when it opens and whenever a conversation is shown, so
/// that somebody about to type a paragraph is told first rather than after.
#[tauri::command]
async fn whats_wrong(
    held: State<'_, Held>,
) -> Result<Option<errand_core::trouble::Trouble>, String> {
    Ok(held.trouble.lock().unwrap().clone())
}

/// Say which conversation is on screen, or that none is.
///
/// So that a notification can be held back for the one being read and shown for
/// the thirty-nine that are not. The app cannot work this out: it knows what is
/// running, and the window knows what somebody is looking at.
#[tauri::command]
async fn looking_at(held: State<'_, Held>, id: Option<String>) -> Result<(), String> {
    *held.looking_at.lock().unwrap() = id;
    Ok(())
}

/// Forget one conversation, and everything said in it.
///
/// Never the last one an agent has: the store refuses, and the refusal says
/// what to do instead rather than only that it would not.
#[tauri::command]
async fn forget_conversation(held: State<'_, Held>, id: String) -> Result<(), String> {
    held.store
        .forget_conversation(&id)
        .map_err(|e| e.to_string())
}

/// What each agent has said that nobody has read.
///
/// Asked for by the window whenever the list is drawn, rather than pushed at
/// it, because the answer changes for reasons the window is not present for:
/// a routine firing, a watch waking something, a delegated errand coming back.
#[tauri::command]
async fn what_is_new(
    held: State<'_, Held>,
) -> Result<std::collections::HashMap<String, errand_core::store::Fresh>, String> {
    held.store.what_is_new().map_err(|e| e.to_string())
}

/// Say that a conversation has now been read.
///
/// Separate from opening it. Opening is what the window does to draw a thread,
/// including for its own reasons, and marking read is a claim that somebody
/// looked -- which is only true when the window is actually in front of them.
#[tauri::command]
async fn seen(held: State<'_, Held>, conversation: String) -> Result<(), String> {
    held.store.seen(&conversation).map_err(|e| e.to_string())
}

/// Everything this Mac can be let at, and what is switched on.
#[tauri::command]
async fn connectors(held: State<'_, Held>) -> Result<Vec<Connected>, String> {
    let on = held.store.connected().map_err(|e| e.to_string())?;
    Ok(errand_core::connectors::KNOWN
        .iter()
        .map(|one| Connected {
            id: one.id.to_string(),
            name: one.name.to_string(),
            sees: one.sees.to_string(),
            on: on.iter().any(|which| which == one.id),
        })
        .collect())
}

/// One thing agents can be let at, and whether they are.
#[derive(Serialize)]
struct Connected {
    id: String,
    name: String,
    sees: String,
    on: bool,
}

/// Let agents reach one, or stop letting them.
#[tauri::command]
async fn connect(held: State<'_, Held>, id: String, on: bool) -> Result<(), String> {
    // Only something Errand knows how to reach. A row in that table naming
    // anything else would be a switch for a thing that does not exist.
    if !errand_core::connectors::KNOWN.iter().any(|c| c.id == id) {
        return Err(format!("there is nothing called {id} to connect to"));
    }
    held.store.connect(&id, on).map_err(|e| e.to_string())
}

/// Hand the keyboard over for one step, and wait to be handed it back.
///
/// The end of the road that is not really the end of one. An agent that hits a
/// sign-in, a two-factor code or a card number has to stop, and until now
/// stopping was all it could do: it wrote a sentence about what somebody would
/// have to go and do, the errand ended, and whatever it had set up half way
/// through was left half way through. What was missing is not the ability to
/// sign in -- it must never have that -- but the ability to stop, be helped,
/// and carry on in the same breath.
///
/// The page is opened in the person's own browser, signed into with their own
/// hands, in a window this app is not driving and cannot read. Errand never
/// sees the password, the code, or the session. What it gets back is one word:
/// they are done, or they are not going to.
async fn over_to_you(app: &AppHandle, asked: &team::Wants) -> anyhow::Result<String> {
    let said = |k: &str| {
        asked
            .args
            .get(k)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
    };
    let (what, where_at, why) = (said("what"), said("where"), said("why"));
    if what.is_empty() {
        anyhow::bail!("say what they need to do, or there is nothing to hand over");
    }
    // Only an address, and only one that goes to a browser. A tool that opens
    // whatever it is handed is a tool that opens `file:///` and worse, at the
    // asking of a model reading somebody else's web page.
    //
    // Not opening it is the refusal. Refusing the whole handover was, and it
    // was wrong: the commonest handover there is has no page at all -- "go into
    // System Settings and switch this on" -- and a model that writes where it
    // has to be done rather than a URL had the entire request thrown back at
    // it, so the person was never asked. What is dropped is the opening; what
    // is said is still said.
    // Web pages, and the one other destination this app repeatedly has to send
    // somebody to: a pane of System Settings. Those cannot be reached any other
    // way -- no path to open, no page to visit -- so without this the
    // instruction is "go and find it", and for Automation that is four levels
    // down a screen most people have never opened. The scheme opens Settings at
    // a pane and can do nothing else.
    let can_be_opened = where_at.starts_with("https://")
        || where_at.starts_with("http://")
        || where_at.starts_with("x-apple.systempreferences:");
    let where_at = match can_be_opened {
        true => where_at,
        false => "",
    };
    let nothing_to_open = !can_be_opened && !said("where").is_empty();

    let handover = uuid::Uuid::new_v4().to_string();
    let (tell_me, answered) = tokio::sync::oneshot::channel();
    {
        let held: State<Held> = app.state();
        held.handovers
            .lock()
            .unwrap()
            .insert(handover.clone(), (asked.from.clone(), tell_me));
    }

    // Opened before the card is shown, so that by the time somebody reads what
    // to do, the thing to do it in is already in front of them.
    if !where_at.is_empty() {
        let _ = std::process::Command::new("open").arg(where_at).spawn();
    }

    {
        let held: State<Held> = app.state();
        let line = held.store.the_app_says_about(
            &asked.from,
            "over_to_you",
            &match (why.is_empty(), where_at.is_empty()) {
                (true, true) => what.to_string(),
                (true, false) => format!("{what}\n{where_at}"),
                (false, true) => format!("{what}\n{why}"),
                (false, false) => format!("{what}\n{why}\n{where_at}"),
            },
            &handover,
        )?;
        let _ = app.emit(
            "handing_over",
            HandingOver {
                conversation: asked.from.clone(),
                seq: line.seq,
                handover: handover.clone(),
                what: what.to_string(),
                why: why.to_string(),
                where_at: where_at.to_string(),
            },
        );
    }

    // Ten minutes, the same as everything else here waits, and for the same
    // reason: an errand that ran at seven in the morning with nobody at the
    // window should end saying so rather than sitting open for ever.
    let back = tokio::time::timeout(std::time::Duration::from_secs(600), answered).await;
    {
        let held: State<Held> = app.state();
        held.handovers.lock().unwrap().remove(&handover);
    }
    match back {
        Ok(Ok(word)) if word == "done" => Ok(format!(
            "{}They say they have done it: {what}. Whatever they signed into is signed into \
             now, so try again rather than asking them how it went.",
            match nothing_to_open {
                true =>
                    "(`where` was not a web address, so nothing was opened; they were \
                         asked anyway.) ",
                false => "",
            }
        )),
        // They typed instead of pressing a button. Their words are the answer.
        Ok(Ok(word)) if word.starts_with(SAID_INSTEAD) => Ok(format!(
            "They answered in words rather than pressing a button: \"{}\". Take that as \
             their answer about: {what}. Carry on from it, and do not ask them to press \
             anything.",
            &word[SAID_INSTEAD.len()..]
        )),
        Ok(Ok(_)) => Ok(format!(
            "They have skipped it: {what}. Do not ask again. Carry on with whatever can be \
             done without it, and say plainly what cannot."
        )),
        // The window went away, or nobody was there.
        _ => Ok(format!(
            "Nobody came back about it: {what}. Say what is still waiting on them and stop \
             there."
        )),
    }
}

/// What a handover's answer starts with when it was typed rather than pressed.
const SAID_INSTEAD: &str = "said:";

/// A handover answered by typing, so the card on screen can close.
#[derive(Clone, Serialize)]
struct HandedBack {
    conversation: String,
    handover: String,
    how: String,
}

/// The handover this conversation is parked on, if it is parked on one.
fn a_handover_waiting_in(held: &Held, conversation: &str) -> Option<String> {
    held.handovers
        .lock()
        .unwrap()
        .iter()
        .find(|(_, (whose, _))| whose == conversation)
        .map(|(handover, _)| handover.clone())
}

/// Somebody is being asked to come and do something.
#[derive(Clone, Serialize)]
struct HandingOver {
    conversation: String,
    seq: i64,
    /// Which handover, so the answer reaches the call that is waiting.
    handover: String,
    what: String,
    why: String,
    #[serde(rename = "where")]
    where_at: String,
}

/// Which handovers are still being waited on.
///
/// Asked when a conversation is read back, because a line on disk cannot say
/// whether the agent that wrote it is still sitting there. Without this,
/// switching away from a conversation and back turned a question somebody was
/// being asked into a note about a question, and left the agent waiting with
/// nothing on screen to answer it.
#[tauri::command]
async fn waiting_on_you(held: State<'_, Held>) -> Result<Vec<String>, String> {
    Ok(held.handovers.lock().unwrap().keys().cloned().collect())
}

/// Say they have done it, or that they are not going to.
#[tauri::command]
async fn handed_back(held: State<'_, Held>, handover: String, how: String) -> Result<(), String> {
    let waiting = held.handovers.lock().unwrap().remove(&handover);
    match waiting {
        Some((_, tell)) => {
            let _ = tell.send(how);
            Ok(())
        }
        // Answered twice, or answered after it gave up. Neither is worth an
        // error in front of somebody who just pressed a button.
        None => Ok(()),
    }
}

/// Wake this conversation when something changes, because it was asked to.
fn keep_an_eye_on(app: &AppHandle, asked: &team::Wants) -> anyhow::Result<String> {
    let said = |k: &str| {
        asked
            .args
            .get(k)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
    };
    let (watch, often, what) = (said("watch"), said("how_often"), said("what"));
    if what.is_empty() {
        anyhow::bail!("say what to do when it changes, or there is nothing to wake for");
    }
    // The one line the app stores, put together from the two halves the tool
    // asks for separately, because "~/Downloads every 10m" is a sentence a
    // model gets subtly wrong and a person should never have to type either.
    let together = format!("{watch} every {often}");
    let read = watch::Watch::read(&together)?;

    let held: State<Held> = app.state();
    held.store.watch(&asked.from, Some(&together), Some(what))?;
    Ok(format!(
        "Set. {}\n\nIt is under Watch, where it can be changed or stopped.",
        read.in_plain_words("this conversation")
    ))
}

/// What this agent already knows about something.
fn look_it_up(app: &AppHandle, asked: &team::Wants) -> anyhow::Result<String> {
    let agent = whose_notebook(app, &asked.from)?;
    let about = asked
        .args
        .get("about")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let held: State<Held> = app.state();
    memory::search(&held.store, &agent, about)
}

/// Take a note back.
fn take_it_back(app: &AppHandle, asked: &team::Wants) -> anyhow::Result<String> {
    let agent = whose_notebook(app, &asked.from)?;
    let about = memory::a_handle(
        asked
            .args
            .get("about")
            .and_then(|v| v.as_str())
            .unwrap_or_default(),
    )?;
    let held: State<Held> = app.state();
    Ok(match held.store.forget_note(&agent, &about)? {
        true => format!("Forgotten. You no longer know anything about {about}."),
        // Said plainly rather than as a failure, because trying to forget
        // something twice is not a mistake worth stopping over.
        false => format!("There was no note about {about} to take back."),
    })
}

/// Everybody else there is, and what each handles.
fn who_else(app: &AppHandle, from: &str) -> anyhow::Result<String> {
    let held: State<Held> = app.state();
    let mine = held.store.conversation(from)?.map(|c| c.agent);
    let others: Vec<String> = held
        .store
        .agents()?
        .into_iter()
        .filter(|a| Some(&a.id) != mine.as_ref() && a.name != NOT_YET_NAMED)
        .map(|a| {
            format!(
                "  {} ({}) -- {}",
                a.name,
                a.title.unwrap_or_else(|| "no role".into()),
                a.about
                    .unwrap_or_else(|| "has not said what it handles".into())
            )
        })
        .collect();

    Ok(match others.is_empty() {
        true => "There is nobody else yet.".to_string(),
        false => format!("You can hand work to:\n{}", others.join("\n")),
    })
}

/// Hand a request to another agent and wait for what it says.
///
/// In a conversation of its own, named after who asked, so the exchange is
/// readable afterwards exactly like any other -- which is the whole of what
/// makes this feel like a team rather than a function call.
async fn ask_teammate(app: &AppHandle, asked: &team::Wants) -> anyhow::Result<String> {
    let named = asked
        .args
        .get("agent")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let request = asked
        .args
        .get("request")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    anyhow::ensure!(
        !request.trim().is_empty(),
        "there was no request to pass on"
    );

    let (them, mine) = {
        let held: State<Held> = app.state();
        let mine = held.store.conversation(&asked.from)?.map(|c| c.agent);
        let them = held
            .store
            .agents()?
            .into_iter()
            .find(|a| a.name.eq_ignore_ascii_case(named.trim()))
            .ok_or_else(|| anyhow::anyhow!("there is nobody here called {named}"))?;
        (them, mine)
    };
    anyhow::ensure!(
        Some(&them.id) != mine.as_ref(),
        "that is you; ask somebody else or do it yourself"
    );

    // Not just you: anybody already waiting further up the chain. Two agents
    // that each think the other should handle a job will hand it back and forth
    // for ever, and every round costs a conversation, a process and ten minutes
    // of somebody's money. Handling one hand-off at a time used to hide this by
    // making the second wait for the first; running them at once is what turns
    // it into a real loop.
    {
        let held: State<Held> = app.state();
        let waiting = held.store.who_is_waiting(&asked.from)?;
        if waiting.contains(&them.id) {
            anyhow::bail!(
                "{} is already waiting on this job, so handing it back would go round in \
                 circles. Do it yourself, or ask somebody who is not already involved.",
                them.name
            );
        }
    }

    // Its own conversation, so the delegated work does not land in the middle
    // of whatever else that agent was doing.
    let talk = uuid::Uuid::new_v4().to_string();
    let asked_by = {
        let held: State<Held> = app.state();
        let who = mine
            .as_deref()
            .and_then(|a| held.store.agent(a).ok().flatten())
            .map_or_else(|| "something outside".to_string(), |a| a.name);
        held.store.begin_conversation_for(
            &talk,
            &them.id,
            &format!("Asked by {who}"),
            // The conversation that asked, so the next hand-off can be
            // followed back past this one. Nothing when the request came from
            // outside the app: there is no conversation to point at, and
            // pointing at one that does not exist is the thing the store checks
            // for on every change to its shape.
            Some(asked.from.as_str()).filter(|from| !from.is_empty()),
        )?;
        who
    };

    open_thread(app.clone(), app.state(), talk.clone())
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Registered before it is asked, or a fast answer arrives before anybody
    // is waiting for it.
    let (finished, done) = tokio::sync::mpsc::unbounded_channel();
    {
        let held: State<Held> = app.state();
        held.watching.lock().unwrap().insert(talk.clone(), finished);
    }

    let said = match say(
        app.clone(),
        app.state(),
        talk.clone(),
        format!("{asked_by} asks: {request}"),
        // An agent asking another sends words and nothing else.
        None,
    )
    .await
    {
        // Where it landed is of no interest to a delegated errand.
        Ok(_) => wait_for_the_answer(done, asked.along_the_way.clone()).await,
        Err(why) => Err(anyhow::anyhow!("{why}")),
    };

    {
        let held: State<Held> = app.state();
        held.watching.lock().unwrap().remove(&talk);
    }
    said
}

/// Collect what the other agent said, until its turn ends.
///
/// Waiting on a task rather than on a thread, and that is not a tidiness
/// preference. The old version parked an OS worker in a blocking receive for up
/// to ten minutes, and the task that reads an engine's output lives on the same
/// runtime: park enough workers and nothing is left to produce the very events
/// this is waiting for. The symptom of that is silence, which reads exactly
/// like an agent with nothing to say.
async fn wait_for_the_answer(
    mut done: tokio::sync::mpsc::UnboundedReceiver<Event>,
    // Where to say what is happening, for a caller watching rather than
    // waiting. Nothing for an engine: a model handed a commentary on somebody
    // else's work puts all of it in its context and none of it is the answer.
    along_the_way: Option<tokio::sync::mpsc::UnboundedSender<errand_core::team::Meanwhile>>,
) -> anyhow::Result<String> {
    use std::time::Duration;
    // Shared with the collecting task, so that giving up still reports what was
    // said before the deadline. An agent that worked for nine minutes and then
    // ran out of time has usually produced something worth passing back, and
    // "it did not finish" on its own throws that away.
    let said = Arc::new(Mutex::new(String::new()));
    let filling = said.clone();

    let collect = async move {
        let push = |text: &str| {
            let mut said = filling.lock().unwrap();
            said.push_str(text);
            said.push('\n');
        };
        let so_far = || filling.lock().unwrap().clone();

        while let Some(event) = done.recv().await {
            // Said as it happens, in the same words the window uses. What a
            // step is called is already written for a person to read, so there
            // is nothing to invent here.
            if let Some(telling) = &along_the_way {
                use errand_core::team::Meanwhile;
                let _ = match &event {
                    Event::Doing(step) => telling.send(Meanwhile::Step(step.what.clone())),
                    // The answer as it is written, which is what somebody stood
                    // at a terminal is actually waiting for. Steps alone say
                    // that it is working; only this says what it is coming to,
                    // and over a long errand the difference is minutes of
                    // reading rather than minutes of watching a spinner.
                    Event::Said {
                        text,
                        settled: false,
                    } => telling.send(Meanwhile::Saying(text.clone())),
                    // Where to answer it, because the answer is not here. The
                    // question is a card in the window, and somebody not told
                    // that waits at a terminal for something that will never
                    // arrive there.
                    Event::NeedsYou(ask) => telling.send(Meanwhile::Step(format!(
                        "Waiting on you: {} \u{2014} answer it in the Errand window",
                        ask.asking
                    ))),
                    Event::Failed { why } => {
                        telling.send(Meanwhile::Step(format!("It could not: {why}")))
                    }
                    _ => Ok(()),
                };
            }
            match event {
                Event::Said {
                    text,
                    settled: true,
                } => push(&text),
                // A question in a delegated conversation has nobody at the
                // keyboard for it, and saying so beats waiting out the ten
                // minutes.
                //
                // Unless somebody is watching, which is the whole difference
                // between an engine asking and a person asking from a terminal.
                // The question is a card in the window either way; giving up on
                // it while somebody is sitting there throws the errand away and
                // tells them nobody could answer, which is not true.
                Event::NeedsYou(ask) if along_the_way.is_none() => {
                    return format!(
                        "It stopped to ask permission to {} and there was nobody to answer, so \
                         it did not finish. What it got to: {}",
                        ask.asking,
                        so_far()
                    )
                }
                Event::Done { .. } => return so_far().trim().to_string(),
                Event::Failed { why } => return format!("It could not: {why}"),
                _ => {}
            }
        }
        // The conversation's pump has gone, which is not an answer either.
        format!(
            "It stopped before it finished. What it got to: {}",
            so_far()
        )
    };

    Ok(
        match tokio::time::timeout(Duration::from_secs(600), collect).await {
            Ok(answer) => answer,
            Err(_) => format!(
                "It did not finish within ten minutes. What it got to: {}",
                said.lock().unwrap()
            ),
        },
    )
}

/// Watch the clock, and run what is due.
///
/// In the app rather than in launchd, and the difference is worth being honest
/// about rather than discovering. Grok Bot keeps "every morning at 7am" because
/// every bot has a machine in the cloud. This runs on your Mac, so a routine
/// happens when Errand is running and the machine is awake, and does not
/// happen otherwise.
///
/// What it does instead of pretending is notice. A routine whose time passed
/// while the app was shut is overdue rather than skipped: it runs when the app
/// comes back and says in the conversation that it is late. Silently running
/// yesterday's briefing as though it were today's would be worse than either.
///
/// Every thirty seconds, because a minute-accurate schedule needs to be looked
/// at more often than once a minute or it drifts by up to a minute, and looking
/// twice a minute costs one query against a table with a handful of rows in it.
fn watch_the_clock(app: AppHandle) {
    // Tauri's own spawn, not tokio's. `tokio::spawn` needs to be called from
    // inside a runtime and this is called from Tauri's event loop, which is not
    // one -- so the task was never started and every routine simply never ran,
    // with nothing anywhere saying so.
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            // Each tick on a task of its own. A loop that can die is a loop
            // that eventually does, and an app whose clock has quietly stopped
            // looks exactly like an app that was never asked to do anything:
            // nothing runs, nothing is written down, nothing says why.
            if tauri::async_runtime::spawn(one_tick(app.clone()))
                .await
                .is_err()
            {
                eprintln!("the clock: a tick stopped in a way it could not report");
            }
        }
    });
}

/// Everything the clock does once, which is the part allowed to go wrong.
async fn one_tick(app: AppHandle) {
    if let Err(why) = run_what_is_due(&app).await {
        eprintln!("the clock: {why}");
    }
    if let Err(why) = look_around(&app).await {
        eprintln!("the looking: {why}");
    }
}

/// Anything whose time has come, started once each.
async fn run_what_is_due(app: &AppHandle) -> Result<(), String> {
    let now = chrono::Local::now();
    let due: Vec<(String, String, Option<chrono::DateTime<chrono::Local>>)> = {
        let held: State<Held> = app.state();
        held.store
            .routines()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter_map(|c| {
                let when = When::read(c.runs_at.as_deref()?).ok()?;
                let next = when.next_after(counting_from(&c, now))?;
                // Late by more than a schedule's own patience is worth saying
                // out loud. Ten minutes is arbitrary and only decides whether
                // the run announces itself as late.
                // Late by more than a schedule's own patience carries the time
                // it was actually due, which is the only part of this worth
                // reading.
                let late = (now.signed_duration_since(next).num_minutes() > 10).then_some(next);
                let what = c.runs_what.clone()?;
                // Not if the last run is still going. A routine that takes
                // longer than its own interval is ordinary, and starting it on
                // top of itself puts two turns in one conversation racing.
                let busy = held.running.lock().unwrap().contains(&c.id);
                (next <= now && !busy).then_some((c.id.clone(), what, late))
            })
            .collect()
    };

    for (conversation, what, late) in due {
        // Written down before it is started. If starting fails, the routine has
        // still had its turn and will not spend the rest of the day retrying
        // every thirty seconds.
        {
            let held: State<Held> = app.state();
            held.store
                .ran(&conversation, now.timestamp_millis())
                .map_err(|e| e.to_string())?;
            // And a row of its own, so "has it been working" has an answer.
            // One column holding only the last run cannot tell three good
            // mornings from three failed ones.
            if let Ok(run) = held.store.a_run_began(&conversation, "clock") {
                held.mid_run
                    .lock()
                    .unwrap()
                    .insert(conversation.clone(), run);
            }
        }

        // Each turn on a task of its own, so that one routine going wrong
        // cannot take the clock with it. It used to be started right here,
        // inside the clock's own loop: a panic anywhere in a turn killed that
        // loop for the life of the app, and from then on no routine ever ran
        // again, with nothing on screen or in the database saying why. That is
        // the shape of "it worked this morning and then it just stopped".
        let running = tauri::async_runtime::spawn(a_routines_turn(
            app.clone(),
            conversation.clone(),
            what,
            late,
            now,
        ));
        // A turn that never reached the engine has to be closed here, or its
        // run stays open forever and its history reads as still going.
        let trouble = match running.await {
            Ok(Ok(())) => None,
            Ok(Err(why)) => Some(why),
            Err(_) => Some("the run stopped in a way it could not report".to_string()),
        };
        if let Some(why) = trouble {
            eprintln!("the routine {conversation}: {why}");
            let held: State<Held> = app.state();
            let run = held.mid_run.lock().unwrap().remove(&conversation);
            if let Some(run) = run {
                let _ = held.store.a_run_ended(run, &why);
            }
        }
    }
    Ok(())
}

/// One routine's turn, from opening the conversation to handing it over.
async fn a_routines_turn(
    app: AppHandle,
    conversation: String,
    what: String,
    late: Option<chrono::DateTime<chrono::Local>>,
    now: chrono::DateTime<chrono::Local>,
) -> Result<(), String> {
    let turn = {
        let held: State<Held> = app.state();
        Turn::claim(held.running.clone(), conversation.clone())
    };
    open_thread(app.clone(), app.state(), conversation.clone()).await?;
    let said = match late {
        None => what,
        Some(due) => format!(
            "{what}\n\n{}",
            errand_core::routine::arriving_late(due, now)
        ),
    };
    // A routine says what it was set to say, and nothing else.
    say(app.clone(), app.state(), conversation, said, None).await?;
    turn.handed_to_the_engine();
    Ok(())
}

/// What the engine allows without being asked, which is not this app's to grant.
///
/// Beside Errand's own list rather than instead of it, because the panel that
/// says what an agent may do without asking was showing half the answer: the
/// engine reads permission rules of its own out of its settings files, and on
/// the machine this was written on there were nineteen of them, none of them
/// visible anywhere in this app.
///
/// Read from the agent's own folder as well as the person's, because a folder
/// has rules of its own and those are the ones nobody remembers agreeing to.
#[tauri::command]
async fn also_allowed(held: State<'_, Held>, agent: String) -> Result<AlsoAllowed, String> {
    let home = std::env::var("HOME").map_err(|_| "there is no home folder".to_string())?;
    let known = held.store.agent(&agent).map_err(|e| e.to_string())?;

    // These are Claude Code's files. An agent answering on a model running on
    // this machine reads none of them, and showing somebody a list of rules
    // that decide nothing about the agent they are looking at is the same fault
    // as not showing them at all, pointing the other way.
    if known.as_ref().is_some_and(|a| a.engine != "claude") {
        return Ok(AlsoAllowed::default());
    }

    // Where this agent actually works, since a folder's own settings are in
    // force for the errands run in it.
    let working_in = known
        .as_ref()
        .map(|a| std::path::PathBuf::from(&a.cwd))
        .unwrap_or_else(|| std::path::PathBuf::from(&home));
    let theirs = errand_core::elsewhere::read(&errand_core::elsewhere::where_they_live(
        std::path::Path::new(&home),
        &working_in,
    ));
    // Said here, where this agent's posture is known. Errand puts that posture
    // on the command line every time it starts the engine, and a command-line
    // argument outranks the same setting in every one of these files except the
    // administrator's, so a sentence about the mode written without it is a
    // sentence about a file that is being overruled.
    let asks = known.as_ref().map_or("ask", |a| a.asks.as_str());
    Ok(AlsoAllowed {
        mode_says: theirs.what_the_mode_means(asks),
        theirs,
    })
}

/// The engine's own rules, and what its session-wide mode means for this agent.
#[derive(Default, serde::Serialize)]
struct AlsoAllowed {
    #[serde(flatten)]
    theirs: errand_core::elsewhere::Theirs,
    /// Nothing where the mode changes nothing, which is most of the time. One
    /// sentence, written in one place: it was written twice, once here and once
    /// in the window, and the two would have drifted the first time either
    /// changed.
    mode_says: Option<String>,
}

/// Allow something before being asked about it.
///
/// Every rule in this app cost an interruption to create: `allow` had exactly
/// one caller, inside the answer to a question that had already stopped the
/// work. So somebody who knows perfectly well their research agent should be
/// free to run `curl` had no way to say so until it had interrupted them twice.
///
/// Said back in the same words the button on a card would use, so a rule
/// written here and a rule granted there are visibly the same kind of thing.
#[tauri::command]
async fn allow_in_advance(
    held: State<'_, Held>,
    agent: String,
    tool: String,
    rule: String,
) -> Result<String, String> {
    let tool = tool.trim();
    let rule = rule.trim();
    if tool.is_empty() {
        return Err("say which tool this is about".into());
    }
    // Narrowed the same way pressing Always narrows it, so that writing
    // `curl -s https://x` here means what pressing Always on that command
    // would have meant, and not something quietly different.
    let allowing = errand_core::allowing::what_always_means(tool, Some(rule))
        .ok_or_else(|| "there is nothing to remember in that".to_string())?;
    // A folder has to be one. A path that is not absolute is a guess about
    // the working directory, and a folder that is not there is a typo, and
    // both would sit in the list looking like a permission that never works.
    if errand_core::allowing::is_a_folder(tool) {
        let folder = std::path::Path::new(&allowing.rule);
        if !folder.is_absolute() {
            return Err("give the folder's whole path, starting with /".into());
        }
        if !folder.is_dir() {
            return Err(format!("{} is not a folder on this Mac", folder.display()));
        }
    }
    held.store
        .allow(&agent, tool, &allowing.rule)
        .map_err(|e| e.to_string())?;
    let_the_wall_know(&held, &agent);
    Ok(allowing.in_words)
}

/// Bring the wall up to date with what this agent may write in.
///
/// An engine already running behind the old wall keeps it until it is next
/// started: the profile is fixed when the process is. One sitting idle is
/// closed here, so the next message opens it behind the new wall; one in the
/// middle of a turn is left alone and gets the new wall after that turn.
fn let_the_wall_know(held: &Held, agent: &str) {
    let Ok(Some(a)) = held.store.agent(agent) else {
        return;
    };
    let folders = held.store.folders_allowed(agent).unwrap_or_default();
    errand_core::wall::also_allow(std::path::Path::new(&a.cwd), folders);
    let open: Vec<String> = held.live.lock().unwrap().keys().cloned().collect();
    for id in open {
        let theirs = matches!(held.store.conversation(&id), Ok(Some(c)) if c.agent == agent);
        let busy = held.running.lock().unwrap().contains(&id);
        if theirs && !busy {
            if let Some(mut thread) = held.live.lock().unwrap().remove(&id) {
                let _ = thread.stop();
            }
        }
    }
}

/// Everything an agent may do without being asked again.
#[tauri::command]
async fn allowances(held: State<'_, Held>, agent: String) -> Result<Vec<Allowed>, String> {
    Ok(held
        .store
        .allowances(&agent)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|one| Allowed {
            covers: errand_core::allowing::in_words(&one.tool, &one.rule),
            id: one.id,
            tool: one.tool,
            rule: one.rule,
        })
        .collect())
}

/// One thing this agent may do without being asked, and how much that is.
///
/// The words matter more than the rule. A rule stored as a whole command line
/// covers that line and nothing else, which looks like a permission and behaves
/// like a one-off, and nothing about the line itself says which it is.
#[derive(Serialize)]
struct Allowed {
    id: String,
    tool: String,
    rule: String,
    covers: String,
}

/// Stop a command that was left running.
///
/// Separate from stopping a conversation, because they are separate things: a
/// command outlives the turn that started it, so stopping the turn must not
/// stop the command and stopping the command must not stop the turn.
#[tauri::command]
async fn stop_a_command(handle: String) -> Result<bool, String> {
    Ok(errand_core::jobs::stop(&handle))
}

/// Take one back.
#[tauri::command]
async fn revoke(held: State<'_, Held>, id: String) -> Result<(), String> {
    let whose = held.store.whose_allowance(&id).map_err(|e| e.to_string())?;
    held.store.revoke(&id).map_err(|e| e.to_string())?;
    if let Some(agent) = whose {
        let_the_wall_know(&held, &agent);
    }
    Ok(())
}

/// How much an agent asks before acting.
#[tauri::command]
async fn asks(held: State<'_, Held>, id: String, how: String) -> Result<(), String> {
    if !["ask", "edits", "auto"].contains(&how.as_str()) {
        return Err(format!("there is no `{how}` way of asking"));
    }
    held.store.asks(&id, &how).map_err(|e| e.to_string())
}

/// Give a conversation a schedule, or take one away.
#[tauri::command]
async fn runs(
    app: AppHandle,
    held: State<'_, Held>,
    id: String,
    at: Option<String>,
    what: Option<String>,
) -> Result<(), String> {
    write_it_down_if_new(&app, &held, &id)?;
    // Read before it is stored, so a schedule nobody can parse is refused here
    // and not at seven in the morning by not happening.
    if let Some(at) = at.as_deref() {
        When::read(at).map_err(|e| e.to_string())?;
    }
    held.store
        .runs(&id, at.as_deref(), what.as_deref())
        .map_err(|e| e.to_string())
}

/// Whether something else already does this, and who.
///
/// Asked as somebody types rather than when they save, so it is a thing they
/// know before they decide rather than an argument afterwards. Nothing stops
/// them: two agents on the same job every morning is usually a mistake and
/// occasionally exactly what somebody wants, and the app is not in a position
/// to know which.
#[tauri::command]
async fn already_runs(
    held: State<'_, Held>,
    id: String,
    at: String,
    what: String,
) -> Result<Option<String>, String> {
    // Only the ones that already run, which is the whole question.
    let all = held.store.routines().map_err(|e| e.to_string())?;
    for one in all {
        if one.id == id {
            continue;
        }
        let (Some(theirs_at), Some(theirs_what)) = (&one.runs_at, &one.runs_what) else {
            continue;
        };
        if routine::the_same_thing_twice(&at, &what, theirs_at, theirs_what) {
            let who = held
                .store
                .agent(&one.agent)
                .ok()
                .flatten()
                .map_or_else(|| one.name.clone(), |a| a.name);
            return Ok(Some(routine::already_doing_this(&who, theirs_at)));
        }
    }
    Ok(None)
}

/// Every conversation that runs itself, and when each is next due.
#[derive(Clone, Serialize)]
struct Routine {
    conversation: String,
    agent: String,
    name: String,
    at: String,
    what: String,
    /// Unix millis, or nothing when the schedule cannot say.
    due: Option<i64>,
    ran: Option<i64>,
    /// Switched off without being thrown away. The clock walks past it.
    off: bool,
}

#[tauri::command]
async fn routines(held: State<'_, Held>) -> Result<Vec<Routine>, String> {
    let now = chrono::Local::now();
    // Switched-off ones included. The clock must not see them; the panel must,
    // or a paused routine reads as no routine and there is nowhere to switch it
    // back on from.
    Ok(held
        .store
        .every_routine()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|c| {
            let at = c.runs_at.clone()?;
            let when = When::read(&at).ok()?;
            Some(Routine {
                due: when
                    .next_after(counting_from(&c, now))
                    .map(|d| d.timestamp_millis()),
                conversation: c.id,
                agent: c.agent,
                name: c.name,
                at,
                what: c.runs_what.unwrap_or_default(),
                ran: c.ran_at,
                off: c.routine_off,
            })
        })
        .collect())
}

/// The moment a routine's next run is counted from.
///
/// Its last run, or the moment the schedule was set. Not "now" -- a routine
/// whose time passed while the app was closed is overdue, and treating it as
/// though it had only just been set would quietly move it to tomorrow.
fn counting_from(
    c: &errand_core::Conversation,
    now: chrono::DateTime<chrono::Local>,
) -> chrono::DateTime<chrono::Local> {
    use chrono::TimeZone;
    c.ran_at
        .or(Some(c.started_at))
        .and_then(|ms| chrono::Local.timestamp_millis_opt(ms).single())
        .unwrap_or(now)
}

/// Open a link somewhere that is not this window.
///
/// A link followed inside the webview replaces the app with a web page and
/// Carry a conversation on somewhere else, from a point in it.
///
/// Nothing is removed from where it came from. That is the whole safety of it:
/// going back to an earlier point makes a second conversation rather than
/// shortening the first, so being wrong about where to go back to costs a
/// conversation nobody uses and never a word of work.
#[tauri::command]
async fn carry_on(
    held: State<'_, Held>,
    id: String,
    from: String,
    // The last line to keep. Nothing means all of it, which is the ordinary
    // case: carry this on somewhere new and leave this one alone.
    up_to: Option<i64>,
) -> Result<(), String> {
    let parent = held
        .store
        .conversation(&from)
        .map_err(|e| e.to_string())?
        .ok_or("there is no such conversation")?;
    let lines = held.store.lines(&from).map_err(|e| e.to_string())?;
    let last = lines.last().map_or(0, |l| l.seq);
    let up_to = up_to.unwrap_or(last);

    // Whether the engine can fork its own session or has to be told what
    // happened. The end of a conversation is the only point Claude Code will
    // fork from without being handed a name for the message to stop at, and it
    // does not name the point somebody clicked.
    let from_the_end = up_to >= last;
    let name = held
        .store
        .a_name_like(&parent.agent, &format!("{}, again", parent.name))
        .map_err(|e| e.to_string())?;

    held.store
        .carry_on(
            &id,
            &from,
            up_to,
            &name,
            match from_the_end {
                true => None,
                false => Some("earlier"),
            },
        )
        .map_err(|e| e.to_string())
}

/// One conversation that is doing something, as the window shows it.
#[derive(Clone, Serialize)]
struct Working {
    conversation: String,
    agent: String,
    /// The agent's name, so a list of these reads without a second lookup.
    who: String,
    /// What the conversation is called, since an agent may have several.
    talk: String,
    /// What it is on, in the same words the timeline uses.
    what: String,
    /// Whether it is stopped waiting for somebody rather than working.
    waiting: bool,
    /// Set when this is a command left running rather than a turn, in which
    /// case it can be stopped on its own without stopping the conversation.
    command: Option<String>,
    /// The last few lines a running command has printed.
    ///
    /// Until this, only the model could see what a long command was doing --
    /// it reaches the kept output through check_command and nothing else did --
    /// which is the wrong way round for the one person who can decide to stop
    /// it. Read without taking, so watching a build does not steal the output
    /// the model is about to be given.
    tail: Option<String>,
}

/// Everything that is working right now, across every agent.
///
/// The answer to a question the window cannot answer for itself: it knows only
/// about conversations somebody has opened, and the work worth being able to
/// see is exactly the work happening somewhere nobody is looking.
#[tauri::command]
async fn whats_running(held: State<'_, Held>) -> Result<Vec<Working>, String> {
    let doing = held.doing.lock().unwrap().clone();
    let mut going = Vec::new();
    for (conversation, what) in doing {
        let Ok(Some(talk)) = held.store.conversation(&conversation) else {
            continue;
        };
        let who = held
            .store
            .agent(&talk.agent)
            .ok()
            .flatten()
            .map_or_else(|| "an agent".to_string(), |a| a.name);
        going.push(Working {
            waiting: what.starts_with("Waiting on you"),
            // A turn is not a command, so it has nothing printing.
            tail: None,
            conversation,
            agent: talk.agent,
            who,
            talk: talk.name,
            what,
            command: None,
        });
    }

    // Commands left running belong here too. They are the one kind of work that
    // outlives the turn that started it, so a list of what is happening that
    // leaves them out is a list that is wrong precisely when it matters.
    for job in errand_core::jobs::running() {
        let (who, talk) = match held.store.conversation(&job.conversation) {
            Ok(Some(c)) => (
                held.store
                    .agent(&c.agent)
                    .ok()
                    .flatten()
                    .map_or_else(|| "an agent".to_string(), |a| a.name),
                c.name,
            ),
            _ => ("an agent".to_string(), String::new()),
        };
        going.push(Working {
            conversation: job.conversation.clone(),
            agent: String::new(),
            who,
            talk,
            what: job.what.clone(),
            waiting: false,
            command: Some(job.handle.clone()),
            tail: (!job.tail.trim().is_empty()).then(|| job.tail.clone()),
        });
    }
    // Anything stopped for somebody first: it is the only kind that will not
    // finish on its own.
    going.sort_by_key(|w| (!w.waiting, w.who.clone()));
    Ok(going)
}

/// One watch, as the window shows it.
#[derive(Clone, Serialize)]
struct Watching {
    watches: Option<String>,
    what: Option<String>,
    /// What it will do, in numbers, so nobody agrees to a rate they never
    /// pictured.
    means: String,
    looked_at: Option<i64>,
    woke_at: Option<i64>,
    woke_today: i64,
    /// How many looks in a row have failed, so that a watch quietly failing is
    /// visible before it has failed enough times to stop.
    misses: i64,
    /// Why it stopped, if it has.
    paused: Option<String>,
}

/// Set or clear what a conversation watches.
///
/// Read before it is stored, so something unreadable is refused at the
/// keyboard rather than at ten past the hour by not happening.
#[tauri::command]
async fn watch_it(
    app: AppHandle,
    held: State<'_, Held>,
    id: String,
    watches: Option<String>,
    what: Option<String>,
) -> Result<(), String> {
    write_it_down_if_new(&app, &held, &id)?;
    if let Some(said) = watches.as_deref() {
        let watch = watch::Watch::read(said).map_err(|e| format!("{e:#}"))?;

        // A watch on a folder the agent writes into is a loop that feeds
        // itself, and it is the easiest mistake here to make. Refused in both
        // directions rather than warned about.
        if let watch::Look::Here(at) = &watch.look {
            let home = held
                .store
                .conversation(&id)
                .map_err(|e| e.to_string())?
                .and_then(|c| held.store.agent(&c.agent).ok().flatten())
                .map(|a| std::path::PathBuf::from(a.cwd));
            if let Some(home) = home {
                if at.starts_with(&home) || home.starts_with(at) {
                    return Err(format!(
                        "{} is where this agent works, so watching it would wake it up with                          its own work and never stop. Watch somewhere else.",
                        at.display()
                    ));
                }
            }
        }

        let already = held.store.watchers().map_err(|e| e.to_string())?;
        if already.len() >= watch::AT_MOST_WATCHES && !already.iter().any(|c| c.id == id) {
            return Err(format!(
                "There are already {} watches, which is as many as this keeps track of.                  Stop one first.",
                watch::AT_MOST_WATCHES
            ));
        }
    }
    held.store
        .watch(&id, watches.as_deref(), what.as_deref())
        .map_err(|e| e.to_string())
}

/// What this conversation watches, if anything.
#[tauri::command]
async fn watches(held: State<'_, Held>, id: String) -> Result<Watching, String> {
    let talk = held
        .store
        .conversation(&id)
        .map_err(|e| e.to_string())?
        .ok_or("there is no such conversation")?;
    let who = held
        .store
        .agent(&talk.agent)
        .ok()
        .flatten()
        .map_or_else(|| "this agent".to_string(), |a| a.name);

    Ok(Watching {
        means: match talk.watches.as_deref().map(watch::Watch::read) {
            Some(Ok(watch)) => watch.in_plain_words(&who),
            // Stored and no longer readable, which is worth saying rather than
            // showing an empty box that looks like no watch at all.
            Some(Err(why)) => format!("This watch can no longer be read: {why:#}"),
            None => String::new(),
        },
        watches: talk.watches,
        what: talk.watches_what,
        looked_at: talk.looked_at,
        woke_at: talk.woke_at,
        woke_today: talk.woke_today,
        misses: talk.misses,
        paused: talk.paused,
    })
}

/// Start a stopped watch looking again.
#[tauri::command]
async fn look_again(held: State<'_, Held>, id: String) -> Result<(), String> {
    held.store.look_again(&id).map_err(|e| e.to_string())
}

/// What is wrong with this setup, before it goes wrong in the middle of a job.
///
/// Everything it checks is something that has actually gone wrong here, and
/// every one of them failed the same unhelpful way: not as an error, but as an
/// agent that quietly could not do something and had no way to say why.
#[tauri::command]
async fn checkup(app: AppHandle, held: State<'_, Held>) -> Result<Vec<doctor::Finding>, String> {
    let here = where_things_live(&app)?;
    Ok(doctor::everything(&held.store, &here, &here).await)
}

/// Write a conversation out as Markdown and show it in the Finder.
///
/// To the Desktop rather than to a folder chosen in a dialog. A file picker is
/// four clicks and a decision about where things live, for a thing whose whole
/// purpose is to end up somewhere you can see it. Revealing it afterwards means
/// nobody has to be told where it went.
///
/// Markdown rather than the app's own shape, because the point of taking
/// something out is that it can be read somewhere else. A file only this app
/// can open is not an export, it is a second copy of the problem.
#[tauri::command]
async fn export_conversation(held: State<'_, Held>, id: String) -> Result<String, String> {
    let talk = held
        .store
        .conversation(&id)
        .map_err(|e| e.to_string())?
        .ok_or("there is no such conversation")?;
    let agent = held
        .store
        .agent(&talk.agent)
        .map_err(|e| e.to_string())?
        .ok_or("that conversation has no agent")?;
    let lines = held.store.lines(&id).map_err(|e| e.to_string())?;

    let written = keeping::as_markdown(&agent, &talk, &lines);
    let onto = std::path::PathBuf::from(std::env::var("HOME").map_err(|e| e.to_string())?)
        .join("Desktop")
        .join(keeping::as_filename(&agent.name, &talk.name));
    std::fs::write(&onto, written).map_err(|e| e.to_string())?;

    // Revealed rather than opened. Opening hands the conversation to whatever
    // owns .md files on this machine, which may be something nobody wanted
    // launched.
    let _ = std::process::Command::new("open")
        .args(["-R".as_ref(), onto.as_os_str()])
        .spawn();
    Ok(onto.to_string_lossy().to_string())
}

/// there is no way back to the thread, so every one is handed to the browser
/// instead.
///
/// The scheme is checked here as well as in the page. The page's check is the
/// one that runs, and this is the one that still runs if somebody later finds a
/// way past it: the text these links come from was written by a model, which
/// read it off a web page, which anybody can write.
#[tauri::command]
async fn show_in_browser(url: String) -> Result<(), String> {
    if !worth_opening(&url) {
        return Err("that is not a kind of link this opens".into());
    }
    std::process::Command::new("open")
        .arg(&url)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Is this a kind of address worth handing to the system?
///
/// Three schemes, and the reason for a list rather than a blocklist is that a
/// blocklist has to be right about every scheme anybody will ever invent.
/// Leading space is trimmed because `  javascript:...` is the oldest trick
/// there is.
fn worth_opening(url: &str) -> bool {
    let url = url.trim().to_lowercase();
    ["http://", "https://", "mailto:"]
        .iter()
        .any(|s| url.starts_with(s))
}

/// What the engine answering this conversation turned up with.
///
/// Beside the tool servers rather than in a panel of its own, because it is
/// the same question: what can this reach. Empty for a local model, which is
/// the honest answer rather than a gap.
#[tauri::command]
async fn brought(held: State<'_, Held>, id: String) -> Result<errand_core::Brought, String> {
    Ok(held
        .live
        .lock()
        .unwrap()
        .get(&id)
        .map(|engine| engine.brought())
        .unwrap_or_default())
}

/// One server of tools, as the window shows it.
#[derive(Clone, Serialize)]
struct Outside {
    name: String,
    /// Where it was configured, so a tool that appears in one folder and not
    /// another has a visible reason rather than looking like a fault.
    from: String,
    /// What it offers, or nothing when it did not start.
    tools: Vec<String>,
    /// Why not, in words, when it did not.
    trouble: Option<String>,
}

/// The tools this thread can reach, whichever engine is answering it.
///
/// Read from the same file Claude Code reads, so this is the truth for both:
/// Claude Code connects to these servers itself, and the local engine connects
/// through ours. One list, one place it comes from, no drift between them.
///
/// This starts the servers to ask what they offer, which is the only way to
/// know. It is why the panel is opened rather than always on screen.
#[tauri::command]
async fn outside(held: State<'_, Held>, id: String) -> Result<Vec<Outside>, String> {
    // The window shows a conversation, so that is what arrives here, and the
    // working directory belongs to the agent it is under. Looking the id up as
    // an agent worked for exactly as long as an agent had one conversation
    // sharing its id: every conversation opened after that found nothing and
    // quietly fell back to ".", which reads a different project's servers.
    let home = held
        .store
        .conversation(&id)
        .map_err(|e| e.to_string())?
        .map(|c| c.agent)
        .and_then(|agent| held.store.agent(&agent).ok().flatten())
        .map(|a| std::path::PathBuf::from(a.cwd))
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let configured = mcp::configured(&home);
    let running = mcp::Servers::open(&home).await;

    Ok(configured
        .into_iter()
        .map(|server| {
            let trouble = running
                .trouble
                .iter()
                .find(|(name, _)| *name == server.name)
                .map(|(_, why)| why.clone());
            Outside {
                tools: running
                    .tools()
                    .iter()
                    .filter(|t| t.server == server.name)
                    .map(|t| t.own_name.clone())
                    .collect(),
                name: server.name,
                from: server.from,
                trouble,
            }
        })
        .collect())
}

/// Change an agent's identity by hand, whatever it settled on.
///
/// It named itself and it can be overruled: it is somebody's agent, not its
/// own. Nothing here is required, and an empty field simply becomes empty.
#[tauri::command]
async fn rename(
    app: AppHandle,
    held: State<'_, Held>,
    id: String,
    name: String,
    title: String,
    about: String,
) -> Result<(), String> {
    write_it_down_if_new(&app, &held, &id)?;
    held.store
        .rename(&id, &name, &title, &about)
        .map_err(|e| e.to_string())
}

/// Keep an agent at the top of the list, or stop.
#[tauri::command]
async fn pin(
    app: AppHandle,
    held: State<'_, Held>,
    id: String,
    pinned: bool,
) -> Result<(), String> {
    write_it_down_if_new(&app, &held, &id)?;
    held.store.pin(&id, pinned).map_err(|e| e.to_string())
}

/// Take an agent out of the list without stopping it.
///
/// Different from forgetting one, and the difference matters: a hidden agent
/// still holds its conversation and still runs whatever it runs. It is out of
/// the way, not gone.
#[tauri::command]
async fn hide(
    app: AppHandle,
    held: State<'_, Held>,
    id: String,
    hidden: bool,
) -> Result<(), String> {
    write_it_down_if_new(&app, &held, &id)?;
    held.store.hide(&id, hidden).map_err(|e| e.to_string())
}

/// Stop it, whatever it is in the middle of. The thread itself is kept.
#[tauri::command]
async fn stop(held: State<'_, Held>, id: String) -> Result<(), String> {
    // The doorway goes with it. A socket that outlives the conversation behind
    // it is a way in to something that is no longer there.
    held.doorways.lock().unwrap().remove(&id);
    // Nothing else will say the turn is over. The engine releases a turn when
    // it reaches an ending, and a killed process never reaches one, so without
    // this a stopped conversation stays "working" in the window forever and the
    // clock quietly skips it every morning after.
    held.running.lock().unwrap().remove(&id);
    held.doing.lock().unwrap().remove(&id);
    if let Some(mut thread) = held.live.lock().unwrap().remove(&id) {
        thread.stop().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Forget a thread and everything said in it.
#[tauri::command]
async fn forget(held: State<'_, Held>, id: String) -> Result<(), String> {
    held.doorways.lock().unwrap().remove(&id);
    held.running.lock().unwrap().remove(&id);
    held.doing.lock().unwrap().remove(&id);
    if let Some(mut thread) = held.live.lock().unwrap().remove(&id) {
        let _ = thread.stop();
    }
    held.store.forget(&id).map_err(|e| e.to_string())
}

/// How to reach a running Errand from a script or a terminal.
const FROM_OUTSIDE: &str = "ask";

/// What to say when somebody runs this with nothing it understands.
const HOW_TO_ASK: &str = "\
Usage: Errand ask [--json | --shape <example>] <agent> <request>
       Errand ask --who

Hands a job to one of your agents in the running app and prints what it says.
Errand has to be open: this talks to it, it does not start it.

What it is doing goes to stderr as it happens, along with the answer as it is
written, so the answer on stdout is still just the answer and arrives once.
Set ERRAND_QUIET to leave that out.

--json asks for the answer as JSON, and --shape asks for it as JSON matching
an example you give. Either way the answer is checked before it is printed:
prose where a script expected an object exits 3 rather than being piped on.";

/// Ask a running Errand something from a terminal.
///
/// Everything here is deliberately plain: one request, one answer, an exit code
/// that means what exit codes mean. The thing on the other end of this is a
/// shell script or a person, and neither wants a protocol.
fn from_a_terminal(args: Vec<String>) -> i32 {
    let Some(here) = beside_everything_else() else {
        eprintln!("could not work out where Errand keeps things");
        return 2;
    };
    let door = doorway::front_door(&here);

    // What shape the answer has to be, taken off the front before anything
    // else is read. Nothing means prose, which is what a person wants and what
    // this did for its whole life until now.
    let (shape, args) = match args.split_first() {
        Some((flag, rest)) if flag == "--json" => (Some(None), rest.to_vec()),
        Some((flag, rest)) if flag == "--shape" => match rest.split_first() {
            Some((example, rest)) => (Some(Some(example.clone())), rest.to_vec()),
            // A flag that quietly means nothing is worse than a flag that
            // fails: a script would run for a minute and then be handed prose.
            None => {
                eprintln!("--shape needs an example of the answer you want");
                return 2;
            }
        },
        _ => (None, args),
    };
    let wanted = shape.as_ref().map(|s| s.as_deref());

    let (tool, args) = match args.split_first() {
        Some((one, rest)) if one == "--who" && rest.is_empty() => {
            ("mcp__errand__who_else", serde_json::json!({}))
        }
        Some((agent, rest)) if !rest.is_empty() => (
            "mcp__errand__ask",
            serde_json::json!({
                "agent": agent,
                "request": match wanted {
                    Some(shape) => errand_core::shape::asked_for(&rest.join(" "), shape),
                    None => rest.join(" "),
                },
            }),
        ),
        _ => {
            eprintln!("{HOW_TO_ASK}");
            return 2;
        }
    };

    // Said as it happens, on stderr, so that piping the answer somewhere still
    // gets the answer and nothing else. An errand takes minutes, and minutes of
    // silence is indistinguishable from a crash.
    // The prose is written without a prefix and without a line of its own,
    // because it is one sentence arriving in pieces rather than a list of
    // things that happened. `mid_sentence` is what closes that line before the
    // next step is written under it, or the two run together on one line.
    let mut mid_sentence = false;
    let mut telling = |said: errand_core::team::Meanwhile| {
        use errand_core::team::Meanwhile;
        match said {
            Meanwhile::Step(step) => {
                if std::mem::take(&mut mid_sentence) {
                    eprintln!();
                }
                eprintln!("· {step}");
            }
            Meanwhile::Saying(text) => {
                mid_sentence = true;
                eprint!("{text}");
                // Written through, because a word held in a buffer until the
                // sentence ends is a word that arrives with the answer, which
                // is the thing this exists to stop.
                let _ = std::io::Write::flush(&mut std::io::stderr());
            }
        }
    };
    let watching = std::env::var("ERRAND_QUIET").is_err();
    let outcome = doorway::ask_from_outside(
        &door,
        tool,
        args,
        watching.then_some(&mut telling as &mut dyn FnMut(errand_core::team::Meanwhile)),
    );
    // The last thing streamed is nearly always a fragment of prose, and a
    // fragment does not end a line. Left open, the answer was printed onto the
    // end of it: "It is Sunday.It is Sunday." on one line, with nothing to say
    // where the commentary stopped and the answer began. Even redirected apart,
    // the log's last line was unterminated.
    if mid_sentence {
        eprintln!();
    }

    match outcome {
        Ok(said) if wanted.is_none() => {
            println!("{said}");
            // A turn that failed says so in its first words, and a script
            // reading the exit code has to be told as well: this returned 0
            // for "It could not: ..." and a pipeline carried on as though the
            // errand had been done.
            if said.starts_with("It could not:") {
                return 1;
            }
            0
        }
        // Asked for in a shape, so checked before it is printed. A script that
        // is handed prose where it expected an object finds out three steps
        // later as a wrong value; this is the moment it can find out cheaply.
        //
        // Its own exit code, because "the agent could not be reached" and "the
        // agent answered, but not in the shape you asked for" are different
        // problems with different fixes, and a script that retries the first
        // should not retry the second.
        Ok(said) => match errand_core::shape::what_came_back(&said) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(why) => {
                eprintln!("{why}");
                3
            }
        },
        // On stderr and non-zero, so a script can tell the difference between
        // an agent that answered and an agent that could not be reached. That
        // difference is the whole reason this is worth having a protocol for.
        Err(why) => {
            eprintln!("{why:#}");
            1
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Before anything Tauri touches. Claude Code starts this same binary to
    // reach the app's own two tools, and that process has to be a program on a
    // pipe and nothing else: build a window here and every delegated errand
    // puts a second Errand in the dock.
    //
    // The same binary rather than one bundled beside it, because
    // `current_exe()` is right in a signed .app and right under `cargo run`,
    // and a second executable would have to be built, copied, signed and kept
    // in step for no gain.
    let mut args = std::env::args().skip(1);
    let first = args.next();
    if first.as_deref() == Some(doorway::IN_ARGV) {
        match args.next() {
            Some(socket) => doorway::serve_blocking(std::path::Path::new(&socket)),
            // Nothing to serve. Said on stderr, which is the only pipe here
            // that is not somebody else's protocol.
            None => {
                eprintln!("{} needs the socket to answer on", doorway::IN_ARGV);
                std::process::exit(2)
            }
        }
    }

    // Driving Errand from outside it, which until now only its own engines
    // could do. Same binary again, for the same reason: a second one would have
    // to be built, copied, signed and kept in step for no gain.
    if first.as_deref() == Some(FROM_OUTSIDE) {
        std::process::exit(from_a_terminal(args.collect()));
    }

    tauri::Builder::default()
        .setup(|app| {
            let here = where_things_live(&app.handle().clone())?;
            // Before the store is opened and before anything is swept up. Two
            // copies of this app share one SQLite store and both run the
            // routines in it, and the sweep below deletes MCP sockets that a
            // copy already running is using this second. Errand is installed by
            // hand over the top of the last one, which makes a second copy far
            // likelier here than in an app that updates itself.
            match errand_core::only_one::take(&here) {
                Ok(lock) => {
                    app.manage(lock);
                }
                Err(why) => {
                    // Said where somebody will see it. A second copy that exits
                    // in silence looks exactly like one that crashed, and the
                    // next thing anybody tries is opening it again.
                    say_it_out_loud("Errand is already open", &why);
                    eprintln!("{why}");
                    std::process::exit(0);
                }
            }
            let store = Store::open(&errand_core::store::beside(&here))?;
            let (wants, asked) = tokio::sync::mpsc::unbounded_channel();
            let (goals, ended) = tokio::sync::mpsc::unbounded_channel();
            app.manage(Held {
                live: Mutex::new(HashMap::new()),
                settling: Settling::default(),
                wants,
                goals,
                running: Arc::default(),
                watching: Arc::default(),
                doing: Arc::default(),
                looking: Arc::default(),
                doorways: Mutex::new(HashMap::new()),
                handovers: Mutex::new(HashMap::new()),
                trouble: Mutex::new(None),
                sized: Mutex::new(HashMap::new()),
                mid_run: Mutex::new(HashMap::new()),
                looking_at: Mutex::new(None),
                store: Arc::new(store),
            });
            // The receiving end is parked here and started on Ready, for the
            // same reason the clock is: setup runs while the app is still
            // being built, and spawning work into it there is how the window
            // stops appearing at all.
            app.manage(Waiting(Mutex::new(Some(asked))));
            app.manage(TurnsThatEnded(Mutex::new(Some(ended))));

            say_what_was_cut_off(app.handle());
            sweep_up_after_a_crash(&here);
            say_if_the_name_is_taken(&here);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            agents,
            matching,
            lines,
            open_thread,
            conversations,
            start_conversation,
            call_it,
            forget_conversation,
            looking_at,
            whats_wrong,
            conversation_agent,
            a_picture,
            a_local_picture,
            a_picture_to_send,
            show_in_finder,
            hits,
            routine_off,
            how_it_has_been_going,
            say,
            answer,
            engines,
            use_engine,
            runs,
            routines,
            allowances,
            allow_in_advance,
            also_allowed,
            revoke,
            asks,
            outside,
            carry_on,
            checkup,
            whats_running,
            stop_a_command,
            look_for_models,
            backends,
            models_at,
            remember_backend,
            forget_backend,
            offer_this,
            stop_offering,
            call_it_something,
            move_it,
            whats_offered,
            opens_at_login,
            what_changed,
            handed_back,
            waiting_on_you,
            connectors,
            what_is_new,
            seen,
            connect,
            seen_what_changed,
            open_at_login,
            still_going,
            already_runs,
            what_it_cost,
            watch_it,
            watches,
            aim_at,
            goal_of,
            look_again,
            brought,
            export_conversation,
            show_in_browser,
            rename,
            pin,
            hide,
            stop,
            forget
        ])
        .build(tauri::generate_context!())
        .expect("building the window")
        // Asked at the start rather than at the moment the first errand lands.
        // The system's question arrives whenever it is first asked, and hours
        // later beside a finished job it is both a worse moment to answer and a
        // worse question, because nobody knows what it is about. The answer is
        // remembered by the system rather than by us, so it is asked once ever.
        //
        // On `Ready` specifically, and not in `setup`: setup runs before the
        // app has finished launching, and asking that early is refused outright
        // with "notifications are not allowed for this application", which
        // sounds like a decision somebody made and is really just a question
        // asked too soon.
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Ready) {
                // Everything here runs inside a callback the system makes
                // across a C boundary, and a panic cannot cross one of those:
                // it aborts the whole process instead. So the app died at
                // launch, with no window, no message and nothing in it that
                // looked like a reason -- over a socket that failed to bind.
                //
                // Caught here so that a fault in any one of these is a line on
                // stderr and one thing not working, rather than an app that
                // will not open. None of them is load-bearing enough to be
                // worth the whole app.
                if let Some(said) =
                    nothing_here_is_worth_the_whole_app(|| everything_that_waits_for_the_app(app))
                {
                    eprintln!(
                        "something failed while the app was starting, and the rest of it \
                         carried on: {said}"
                    );
                }
            }
            // A command left running outlives the errand that started it, and
            // that is the point of it. It must not outlive the only thing that
            // knows it exists: a process nobody can see or stop is worse than
            // one that ended early, and the tool says so before it starts one.
            if matches!(event, tauri::RunEvent::Exit) {
                errand_core::jobs::stop_everything();
            }
        });
}

/// Run something that must not be able to take the app with it, and say what it
/// said if it tried.
///
/// The message rather than a shrug, because a panic message is the only thing
/// in the wreckage that says what to fix, and this one happens on a machine
/// nobody is debugging on.
fn nothing_here_is_worth_the_whole_app(doing: impl FnOnce()) -> Option<String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(doing))
        .err()
        .map(|it| {
            // Two shapes, because `panic!("a")` is a `&str` and
            // `panic!("a {b}")` is a `String`, and only ever looking for one of
            // them loses the message exactly when it was worth having.
            it.downcast_ref::<&str>()
                .map(|said| (*said).to_string())
                .or_else(|| it.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "it did not say".to_string())
        })
}

/// Everything that has to wait until the app is actually up.
///
/// Its own function so that the whole of it sits inside one `catch_unwind` up
/// there, and so that adding to it later cannot accidentally add to the part
/// that is outside the net.
fn everything_that_waits_for_the_app(app: &AppHandle) {
    onscreen::ask();
    // A notification that only brings the app forward is a dead end: you read
    // it, then go and find the conversation yourself, which is the errand you
    // were trying not to run.
    onscreen::listen();
    let go = app.clone();
    onscreen::on_click(move |conversation| {
        if let Some(window) = go.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
        }
        let _ = go.emit("go_to", conversation);
    });
    // Started here rather than in setup, so neither looks at the
    // store before it is in place, and neither is spawned into an
    // app that is still being built.
    watch_the_clock(app.clone());
    let waiting = {
        let parked: State<Waiting> = app.state();
        let taken = parked.0.lock().unwrap().take();
        taken
    };
    if let Some(asked) = waiting {
        answer_what_engines_cannot(app.clone(), asked);
    }
    let endings = {
        let parked: State<TurnsThatEnded> = app.state();
        let taken = parked.0.lock().unwrap().take();
        taken
    };
    if let Some(ended) = endings {
        carry_on_goals(app.clone(), ended);
    }
    // The front door. One socket, no conversation behind it, open
    // for as long as the app is, so something outside can hand an
    // agent a job the way another agent does. Opened here rather
    // than in setup for the same reason as everything else here:
    // there is no runtime yet in setup.
    if let Ok(here) = where_things_live(app) {
        let wants = {
            let held: State<Held> = app.state();
            held.wants.clone()
        };
        let opening = app.clone();
        // Opened from inside a runtime, not from here. Binding a
        // socket registers it with the reactor, and this callback
        // is Tauri's event loop, which is not one. What that looked
        // like was not an error: the file appeared, because the
        // system call that makes it succeeds before the
        // registration that fails, and then nothing was listening
        // on a door that was plainly there. Third time this shape
        // has come up in this app and the first time it was not an
        // outright crash.
        tauri::async_runtime::spawn(async move {
            match doorway::listen(doorway::front_door(&here), String::new(), wants) {
                // Kept in the app's own state so it lives as long
                // as the app and is unlinked when it goes.
                Ok(door) => {
                    let held: State<Held> = opening.state();
                    held.doorways.lock().unwrap().insert(String::new(), door);
                }
                // Said rather than swallowed. Everything else works
                // without it, and somebody whose script cannot
                // connect deserves to find the reason somewhere.
                Err(why) => eprintln!("the front door did not open: {why:#}"),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_socket_name_is_made_from_an_id_that_is_shorter_than_the_cut() {
        // What actually happened: the name was cut with a byte slice that
        // assumed every id was a uuid. A shorter id panicked, and because the
        // turn had been started inside the clock's own loop, the panic killed
        // the clock and no routine ran again until the app was restarted.
        assert_eq!(short_enough_for_a_socket("clock-probe"), "clockprobe");
        assert_eq!(short_enough_for_a_socket(""), "");
        assert_eq!(
            short_enough_for_a_socket("0cfe08f1-04d2-496a-80cc-3fc368499dc2"),
            "0cfe08f104d2496a"
        );
    }

    #[test]
    fn a_fault_while_the_app_is_starting_does_not_take_the_app_with_it() {
        // What actually happened: a socket that could not be bound panicked
        // inside the callback the system makes when the app finishes
        // launching. A panic cannot cross that boundary, so it aborted the
        // process instead, and the app died at launch with no window, no
        // message, and nothing that looked like a reason.
        let quietly = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));

        assert_eq!(nothing_here_is_worth_the_whole_app(|| {}), None);
        assert_eq!(
            nothing_here_is_worth_the_whole_app(|| panic!("the front door did not open"))
                .as_deref(),
            Some("the front door did not open")
        );
        // A panic with anything interpolated into it arrives as a String
        // rather than a &str, and looking for only one of the two loses the
        // message in exactly the cases that had something to say.
        let tries = 7;
        assert_eq!(
            nothing_here_is_worth_the_whole_app(|| panic!("gave up after {tries}")).as_deref(),
            Some("gave up after 7")
        );

        std::panic::set_hook(quietly);
    }

    #[test]
    fn a_routine_that_fails_to_start_can_still_be_started_tomorrow() {
        // The claim used to be made by hand and released only by the engine, so
        // the two fallible steps between them each had a `?` that walked out
        // holding it. The routine was then skipped every morning afterwards,
        // silently, until the app was restarted.
        let among: Arc<Mutex<std::collections::HashSet<String>>> = Arc::default();

        let gave_up = Turn::claim(among.clone(), "morning-briefing".into());
        assert!(among.lock().unwrap().contains("morning-briefing"));
        drop(gave_up);
        assert!(
            among.lock().unwrap().is_empty(),
            "a turn that never started is still claimed"
        );

        // A turn that really started belongs to the engine until the engine
        // says otherwise, so dropping the guard must not release it: that would
        // let the clock start the same routine on top of itself.
        let started = Turn::claim(among.clone(), "morning-briefing".into());
        started.handed_to_the_engine();
        assert!(
            among.lock().unwrap().contains("morning-briefing"),
            "a turn in flight was released by the wrong thing"
        );
    }

    #[test]
    fn a_pasted_picture_and_a_dropped_one_arrive_the_same_way() {
        // Two gestures, two shapes: dropping a file gives a path, and pasting
        // gives the bytes with no name at all. Both have to end up as the one
        // thing an engine can be handed.
        let pasted = pictures_from(&["data:image/png;base64,iVBORw0KGgo=".to_string()])
            .expect("a pasted picture");
        assert_eq!(pasted.len(), 1);
        assert_eq!(pasted[0].kind, "image/png");
        assert_eq!(pasted[0].base64, "iVBORw0KGgo=");
        assert_eq!(
            pasted[0].as_data_url(),
            "data:image/png;base64,iVBORw0KGgo=",
            "it did not survive the round trip a local model needs"
        );
    }

    #[test]
    fn a_dropped_file_that_is_not_a_picture_is_left_alone_rather_than_refused() {
        // Dropping a spreadsheet used to put its path in the box, which is
        // still the right thing: the agent can open it. Refusing it now would
        // take away something that worked.
        let mixed = pictures_from(&["/tmp/accounts.xlsx".to_string(), "/tmp/notes".to_string()])
            .expect("nothing to complain about");
        assert!(mixed.is_empty());
    }

    #[test]
    fn something_that_says_it_is_a_picture_and_is_not_says_so() {
        assert!(pictures_from(&["data:image/png,notbase64".to_string()]).is_err());
    }

    #[test]
    fn a_picture_too_big_to_send_says_which_one_and_how_big() {
        // Both engines have their own limit and neither says so kindly: the
        // failure is a turn that dies somewhere far from the drop.
        let big = std::env::temp_dir().join("errand-too-big.png");
        std::fs::write(&big, vec![0u8; (A_PICTURE_AT_MOST + 1) as usize]).expect("a big file");
        let said = pictures_from(&[big.to_string_lossy().to_string()]);
        let _ = std::fs::remove_file(&big);

        let why = said.expect_err("it accepted a picture over the limit");
        assert!(why.contains("errand-too-big.png"), "{why}");
        assert!(why.contains("MB"), "{why}");
    }

    #[test]
    fn a_model_on_this_machine_is_named_without_a_host_and_one_elsewhere_with_it() {
        // The picker is a list of one-line names. "on 127.0.0.1" after every
        // one of them is noise, and no host at all on a model that lives on
        // another machine is a choice somebody cannot make.
        assert_eq!(elsewhere("http://127.0.0.1:11434"), None);
        assert_eq!(elsewhere("http://localhost:1234"), None);
        assert_eq!(elsewhere("http://[::1]:8080"), None);
        assert_eq!(
            elsewhere("http://192.168.1.42:11434"),
            Some("192.168.1.42".to_string())
        );
        assert_eq!(
            elsewhere("https://box.local:8080/v1"),
            Some("box.local".to_string())
        );
    }

    #[test]
    fn a_model_reached_without_a_port_is_still_named_by_its_host() {
        // Falling back to the whole URL when there is no port put
        // "on http://10.0.0.4" in the list, which is not a host.
        assert_eq!(elsewhere("http://10.0.0.4"), Some("10.0.0.4".to_string()));
        assert_eq!(
            elsewhere("http://10.0.0.4/v1"),
            Some("10.0.0.4".to_string())
        );
    }

    #[test]
    fn an_agent_that_answered_the_way_it_was_asked_to_gets_the_identity_it_chose() {
        let on = read_what_it_settled_on(
            "Scribe | Mail | I read your inbox and draft the replies. | mail | blue",
        )
        .expect("a clean answer");
        assert_eq!(on.name, "Scribe");
        assert_eq!(on.title, "Mail");
        assert_eq!(on.mark, "mail");
        assert_eq!(on.hue, "blue");
    }

    #[test]
    fn the_line_is_found_among_whatever_else_it_decided_to_say() {
        // It was asked for one line and nothing else. It will not always
        // comply, and the compliance is not the point: the line is.
        let on = read_what_it_settled_on(
            "Sure! Here is my identity:\n\n             Ledger | Money | I keep an eye on the accounts. | chart | green\n\n             Let me know if you would like anything changed.",
        )
        .expect("the line is in there");
        assert_eq!(on.name, "Ledger");
        assert_eq!(on.mark, "chart");
    }

    #[test]
    fn an_agent_that_answered_in_prose_keeps_the_name_it_had() {
        // Rather than being called "Certainly! I would be happy to". Nothing
        // back means nothing written, and it is asked again next time.
        assert!(read_what_it_settled_on("Certainly! I would be happy to help.").is_none());
        assert!(read_what_it_settled_on("").is_none());
        assert!(
            read_what_it_settled_on("Scribe | Mail").is_none(),
            "half an answer"
        );
    }

    #[test]
    fn a_mark_the_window_cannot_draw_is_refused_and_a_hue_it_cannot_is_not() {
        // An unrecognised mark would leave an agent with no face, so the guess
        // from the words stands instead. A colour is only a colour.
        assert!(
            read_what_it_settled_on("Atlas | Travel | Flights. | aeroplane | blue").is_none(),
            "there is no aeroplane to draw"
        );
        let on = read_what_it_settled_on("Atlas | Travel | Flights. | bag | chartreuse")
            .expect("the mark is fine");
        assert_eq!(
            on.hue, "amber",
            "an unknown colour falls back rather than failing"
        );
    }

    #[test]
    fn a_name_wrapped_in_the_decoration_models_like_is_unwrapped() {
        let on = read_what_it_settled_on("**Scribe** | Mail | Inbox. | mail | blue").expect("fine");
        assert_eq!(on.name, "Scribe");
        let quoted =
            read_what_it_settled_on("\"Scribe\" | Mail | Inbox. | mail | blue").expect("fine");
        assert_eq!(quoted.name, "Scribe");
    }

    #[test]
    fn a_name_long_enough_to_be_a_sentence_is_not_a_name() {
        let rambling = format!("{} | Mail | Inbox. | mail | blue", "word ".repeat(30));
        assert!(read_what_it_settled_on(&rambling).is_none());
    }

    #[test]
    fn an_ordinary_link_is_handed_to_the_browser() {
        assert!(worth_opening("https://coindesk.com/price/bitcoin"));
        assert!(worth_opening("http://localhost:11434/v1/models"));
        assert!(worth_opening("mailto:somebody@example.com"));
        assert!(worth_opening("  https://example.com  "), "trimmed first");
        assert!(worth_opening("HTTPS://EXAMPLE.COM"), "however it is cased");
    }

    #[test]
    fn anything_that_is_not_a_link_a_person_would_recognise_is_refused() {
        // The page refuses these too. This is the check that still runs if
        // somebody later finds a way past that one, and the text these come
        // from was written by a model that read it off a page an hour ago.
        for bad in [
            "javascript:alert(1)",
            "  javascript:alert(1)",
            "JavaScript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,<script>alert(1)</script>",
            "vscode://file/Users/somebody/.ssh/id_rsa",
            "",
            "not a url at all",
        ] {
            assert!(!worth_opening(bad), "would have opened {bad:?}");
        }
    }

    #[test]
    fn the_gist_of_an_answer_is_the_first_line_of_it_without_its_punctuation() {
        assert_eq!(gist("## Bitcoin\n\nA line."), "Bitcoin");
        assert_eq!(gist("**Done.** And so on."), "Done.** And so on.");
        assert_eq!(gist("   \n\nAfter the blanks."), "After the blanks.");
        assert_eq!(gist(""), "Finished.");
        assert_eq!(
            gist(&"x".repeat(400)).chars().count(),
            140,
            "139 and the mark"
        );
    }
}
