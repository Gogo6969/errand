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

use errand_core::doctor;
use errand_core::doorway;
use errand_core::keeping;
use errand_core::local::{find, LlmSettings, Local};
use errand_core::mcp;
use errand_core::memory;
use errand_core::routine::When;
use errand_core::store::Allowance;
use errand_core::store::{Settled, NOT_YET_NAMED};
use errand_core::team;
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
    /// What each conversation is doing, for the ones that are doing something.
    ///
    /// Kept by the app rather than worked out in the window, because the window
    /// only knows about conversations somebody has opened. A routine firing at
    /// seven on an agent nobody is looking at is exactly the work worth being
    /// able to see, and it is the work the window cannot see at all.
    doing: Arc<Mutex<HashMap<String, String>>>,
    /// The socket each Claude conversation can reach this app's own tools on.
    ///
    /// Held here because holding it is what keeps it open: dropping one stops
    /// answering and takes the file away, so a conversation that has been
    /// closed cannot be reached by a process that outlived it.
    doorways: Mutex<HashMap<String, doorway::Doorway>>,
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
fn where_things_live(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let here = app
        .path()
        .data_dir()
        .map_err(|e| e.to_string())?
        .join("Errand");
    std::fs::create_dir_all(&here).map_err(|e| e.to_string())?;
    Ok(here)
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
        Event::Done { said } => (called(store, id), gist(said)),
        Event::Failed { why } => (format!("{} stopped", called(store, id)), gist(why)),
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
    let watching = app
        .get_webview_window("main")
        .and_then(|w| w.is_focused().ok())
        .unwrap_or(false);
    if watching {
        return;
    }
    onscreen::show(id, &title, &body);
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
async fn call_it(held: State<'_, Held>, id: String, name: String) -> Result<(), String> {
    held.store.call_it(&id, &name).map_err(|e| e.to_string())
}

/// Open a conversation, or pick up one from before.
#[tauri::command]
async fn open_thread(app: AppHandle, held: State<'_, Held>, id: String) -> Result<(), String> {
    if held.live.lock().unwrap().contains_key(&id) {
        return Ok(()); // Already talking to it.
    }

    // A thread we have met before is resumed, in the directory it was started
    // in; a new one is begun there. Both facts live in the store because
    // neither can be guessed at afterwards: resuming with the wrong flag is a
    // hard error, and resuming from the wrong directory quietly starts an empty
    // conversation wearing the same name.
    // Two lookups, because the two halves live in different places now: how to
    // reach the engine belongs to the agent, and whether this particular
    // conversation has run before belongs to the conversation.
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
        None => {
            // Its own folder per thread, so one errand cannot tidy up after
            // another, and so "the files from that thing last Tuesday" are
            // still somewhere findable.
            let home = where_things_live(&app)?.join("threads").join(&id);
            std::fs::create_dir_all(&home).map_err(|e| e.to_string())?;
            // Resolved, because Claude Code resolves it too, and a comparison
            // between a resolved path and an unresolved one is a comparison
            // that fails on a machine with a symlink in it.
            let home = home.canonicalize().unwrap_or(home);
            held.store
                .begin(&id, NOT_YET_NAMED, &home)
                .map_err(|e| e.to_string())?;
            // Its first conversation shares the agent's id, which is what the
            // migration did for every agent that existed before conversations
            // did. Keeping the two the same for a first conversation means
            // there is one rule rather than two.
            held.store
                .begin_conversation(&id, &id, "First")
                .map_err(|e| e.to_string())?;
            (home, false)
        }
    };

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
            let settings: LlmSettings = serde_json::from_str(&settings.unwrap_or_default())
                .map_err(|_| "this thread has no model chosen".to_string())?;
            let asks = known.as_ref().map_or("ask", |a| a.asks.as_str());
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
                    .join(format!("{}.sock", &id.replace('-', "")[..16])),
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

            let written = match store.happened(&id, &event) {
                Ok(line) => line.map(|l| l.seq),
                Err(e) => {
                    // Losing a line is not worth ending the conversation over,
                    // but it must not pass in silence either.
                    eprintln!("could not write down what happened in {id}: {e}");
                    None
                }
            };
            // A routine's turn is over, so the clock may start it again.
            if event.ends_the_turn() {
                let held: State<Held> = app.state();
                held.running.lock().unwrap().remove(&id);
                held.doing.lock().unwrap().remove(&id);
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
    });
    Ok(())
}

/// Say something. Safe while it is working: that is the point of the thing.
#[tauri::command]
async fn say(
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

    // Written down first. If the agent cannot be reached, what was said is
    // still what was said, and it will be there when the thread is reopened.
    // The pictures are not written down: the store holds a conversation, not
    // an album, and a base64 image in a transcript line would be read back
    // into the window on every reopen.
    let said = match pictures.len() {
        0 => text.clone(),
        1 => format!("{text}\n\n(with a picture)"),
        n => format!("{text}\n\n(with {n} pictures)"),
    };
    let written = held.store.asked(&id, &said).map_err(|e| e.to_string())?;

    let mut live = held.live.lock().unwrap();
    let thread = live
        .get_mut(&id)
        .ok_or_else(|| "that conversation is not open".to_string())?;
    thread.say(&text, &pictures).map_err(|e| e.to_string())?;
    Ok(Some(written.seq))
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

/// Everything that could answer a thread on this machine.
///
/// Claude is always there; the rest is whatever is actually running and
/// answering right now, found by asking the usual ports rather than by keeping
/// a list somebody has to maintain. A model that was there yesterday and is not
/// there today should not be offered, because choosing it would fail later and
/// somewhere less obvious.
#[tauri::command]
async fn engines(wider: Option<bool>) -> Result<Vec<Choice>, String> {
    // Claude, and then Claude with a model named. The first is whatever this
    // person's own Claude Code is set to, which stays the default because it
    // is their CLI and their account. The rest exist because "Claude" on its
    // own told nobody what was actually answering, and the difference between
    // Opus and Haiku is the difference between an errand that works and one
    // that is cheap.
    let mut all = vec![Choice {
        engine: "claude".into(),
        name: "Claude · your default".into(),
        settings: None,
    }];
    for (alias, shown) in errand_core::claude::MODELS {
        all.push(Choice {
            engine: "claude".into(),
            name: format!("Claude · {shown}"),
            settings: Some((*alias).to_string()),
        });
    }

    // Two different questions, and only one of them is cheap. The usual ports
    // on this machine answer in under a second, so that is what opening the
    // picker asks. Every machine on the network is thousands of probes and
    // most of a minute, so it is asked for by name and never on a hunch.
    //
    // The wider sweep still includes this machine: a model bound to 127.0.0.1
    // is invisible from the network address the sweep walks, and "look wider"
    // finding fewer models than looking here would be a nonsense.
    let mut seen = std::collections::HashSet::new();
    let mut found = find::detect_all().await;
    if wider.unwrap_or(false) {
        found.extend(find::scan_local_network().await);
    }
    let found: Vec<_> = found
        .into_iter()
        .filter(|b| seen.insert(b.base_url.clone()))
        .collect();

    for found in found {
        // What it lists and what it can answer with are not the same thing. A
        // server names everything it has downloaded, which on a network with a
        // few machines on it is twenty entries, most unloaded and at least one
        // an embedding model that cannot hold a conversation at all.
        let usable = errand_core::local::ready::what_can_answer(
            &found.provider,
            &found.base_url,
            &found.models,
        )
        .await;

        for ready in usable {
            let model = ready.model;
            // Asked rather than assumed. The window is what every budget in the
            // engine is worked out from -- how much conversation fits, how many
            // tools are worth putting in front of it -- and a default guess of
            // 32k is wrong in both directions: it starves a model with 128k and
            // overfills one with 8k. Where the server will not say, the default
            // stands, which is the only honest thing left to do.
            let asked = find::query_model_caps(&found.provider, &found.base_url, None, &model)
                .await
                .ok()
                .and_then(|caps| caps.context_length)
                .map(|n| n as usize);

            let settings = LlmSettings {
                provider: found.provider.clone(),
                base_url: found.base_url.clone(),
                model: model.clone(),
                context_window: asked.unwrap_or(LlmSettings::default().context_window),
                ..Default::default()
            };
            all.push(Choice {
                engine: "local".into(),
                // Where it is, when it is not here. Two machines on a network
                // running the same model are the same line otherwise, and
                // choosing between them becomes guesswork.
                name: {
                    let where_it_is = match elsewhere(&found.base_url) {
                        Some(host) => format!("{} on {host}", found.label),
                        None => found.label.clone(),
                    };
                    match ready.loaded {
                        true => format!("{model} · {where_it_is}"),
                        // Said rather than hidden. Both servers load on demand,
                        // so this one works; it just keeps you waiting the
                        // first time, and that is worth knowing before you
                        // choose it rather than after.
                        false => format!("{model} · {where_it_is} · needs loading"),
                    }
                },
                settings: serde_json::to_string(&settings).ok(),
            });
        }
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
    held: State<'_, Held>,
    id: String,
    engine: String,
    settings: Option<String>,
) -> Result<(), String> {
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
                    None => Err(anyhow::anyhow!("there is no {} here", asked.tool)),
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
            .map_or_else(|| "another agent".to_string(), |a| a.name);
        held.store.begin_conversation_for(
            &talk,
            &them.id,
            &format!("Asked by {who}"),
            // The conversation that asked, so the next hand-off can be
            // followed back past this one.
            Some(&asked.from),
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
        app.state(),
        talk.clone(),
        format!("{asked_by} asks: {request}"),
        // An agent asking another sends words and nothing else.
        None,
    )
    .await
    {
        // Where it landed is of no interest to a delegated errand.
        Ok(_) => wait_for_the_answer(done).await,
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
            match event {
                Event::Said {
                    text,
                    settled: true,
                } => push(&text),
                // A question in a delegated conversation has nobody at the
                // keyboard for it, and saying so beats waiting out the ten
                // minutes.
                Event::NeedsYou(ask) => {
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
            if let Err(why) = run_what_is_due(&app).await {
                eprintln!("the clock: {why}");
            }
        }
    });
}

/// Anything whose time has come, started once each.
async fn run_what_is_due(app: &AppHandle) -> Result<(), String> {
    let now = chrono::Local::now();
    let due: Vec<(String, String, bool)> = {
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
                let late = now.signed_duration_since(next).num_minutes() > 10;
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
        }

        {
            let held: State<Held> = app.state();
            held.running.lock().unwrap().insert(conversation.clone());
        }
        open_thread(app.clone(), app.state(), conversation.clone()).await?;
        let said = match late {
            false => what,
            true => format!(
                "{what}\n\n(This is late: it was due at {} and nothing was running then.)",
                now.format("%H:%M")
            ),
        };
        // A routine says what it was set to say, and nothing else.
        say(app.state(), conversation, said, None).await?;
    }
    Ok(())
}

/// Everything an agent may do without being asked again.
#[tauri::command]
async fn allowances(held: State<'_, Held>, agent: String) -> Result<Vec<Allowance>, String> {
    held.store.allowances(&agent).map_err(|e| e.to_string())
}

/// Take one back.
#[tauri::command]
async fn revoke(held: State<'_, Held>, id: String) -> Result<(), String> {
    held.store.revoke(&id).map_err(|e| e.to_string())
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
    held: State<'_, Held>,
    id: String,
    at: Option<String>,
    what: Option<String>,
) -> Result<(), String> {
    // Read before it is stored, so a schedule nobody can parse is refused here
    // and not at seven in the morning by not happening.
    if let Some(at) = at.as_deref() {
        When::read(at).map_err(|e| e.to_string())?;
    }
    held.store
        .runs(&id, at.as_deref(), what.as_deref())
        .map_err(|e| e.to_string())
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
}

#[tauri::command]
async fn routines(held: State<'_, Held>) -> Result<Vec<Routine>, String> {
    let now = chrono::Local::now();
    Ok(held
        .store
        .routines()
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
            conversation,
            agent: talk.agent,
            who,
            talk: talk.name,
            what,
        });
    }
    // Anything stopped for somebody first: it is the only kind that will not
    // finish on its own.
    going.sort_by_key(|w| (!w.waiting, w.who.clone()));
    Ok(going)
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
    held: State<'_, Held>,
    id: String,
    name: String,
    title: String,
    about: String,
) -> Result<(), String> {
    held.store
        .rename(&id, &name, &title, &about)
        .map_err(|e| e.to_string())
}

/// Keep an agent at the top of the list, or stop.
#[tauri::command]
async fn pin(held: State<'_, Held>, id: String, pinned: bool) -> Result<(), String> {
    held.store.pin(&id, pinned).map_err(|e| e.to_string())
}

/// Take an agent out of the list without stopping it.
///
/// Different from forgetting one, and the difference matters: a hidden agent
/// still holds its conversation and still runs whatever it runs. It is out of
/// the way, not gone.
#[tauri::command]
async fn hide(held: State<'_, Held>, id: String, hidden: bool) -> Result<(), String> {
    held.store.hide(&id, hidden).map_err(|e| e.to_string())
}

/// Stop it, whatever it is in the middle of. The thread itself is kept.
#[tauri::command]
async fn stop(held: State<'_, Held>, id: String) -> Result<(), String> {
    // The doorway goes with it. A socket that outlives the conversation behind
    // it is a way in to something that is no longer there.
    held.doorways.lock().unwrap().remove(&id);
    if let Some(mut thread) = held.live.lock().unwrap().remove(&id) {
        thread.stop().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Forget a thread and everything said in it.
#[tauri::command]
async fn forget(held: State<'_, Held>, id: String) -> Result<(), String> {
    held.doorways.lock().unwrap().remove(&id);
    if let Some(mut thread) = held.live.lock().unwrap().remove(&id) {
        let _ = thread.stop();
    }
    held.store.forget(&id).map_err(|e| e.to_string())
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
    if args.next().as_deref() == Some(doorway::IN_ARGV) {
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

    tauri::Builder::default()
        .setup(|app| {
            let here = where_things_live(&app.handle().clone())?;
            let store = Store::open(&errand_core::store::beside(&here))?;
            let (wants, asked) = tokio::sync::mpsc::unbounded_channel();
            app.manage(Held {
                live: Mutex::new(HashMap::new()),
                settling: Settling::default(),
                wants,
                running: Arc::default(),
                watching: Arc::default(),
                doing: Arc::default(),
                doorways: Mutex::new(HashMap::new()),
                store: Arc::new(store),
            });
            // The receiving end is parked here and started on Ready, for the
            // same reason the clock is: setup runs while the app is still
            // being built, and spawning work into it there is how the window
            // stops appearing at all.
            app.manage(Waiting(Mutex::new(Some(asked))));

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
            say,
            answer,
            engines,
            use_engine,
            runs,
            routines,
            allowances,
            revoke,
            asks,
            outside,
            carry_on,
            checkup,
            whats_running,
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
                onscreen::ask();
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
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

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
