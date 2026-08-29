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

use errand_core::local::{find, LlmSettings, Local};
use errand_core::mcp;
use errand_core::store::{Settled, NOT_YET_NAMED};
use errand_core::{claude::Claude, Agent, Answer, Engine, Event, Line, Store};
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
type Settling = Arc<Mutex<HashMap<String, String>>>;

struct Held {
    /// Whatever is answering each open thread. Boxed rather than one concrete
    /// type because there are two engines now and the window is not told which
    /// it is talking to -- that is the whole point of the protocol, and it
    /// stops being true the moment this map knows.
    live: Mutex<HashMap<String, Box<dyn Engine + Send>>>,
    settling: Settling,
    store: Arc<Store>,
}

/// One event, and which conversation it belongs to.
///
/// The thread's id travels with it because the window shows one thread at a
/// time but keeps several alive: a person who starts something slow and goes to
/// read another thread should come back to find it finished, not paused.
#[derive(Clone, Serialize)]
struct Happened {
    thread: String,
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
    match store.agent(id) {
        Ok(Some(t)) => t.name,
        _ => "Errand".to_string(),
    }
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

/// Start a conversation, or pick up one from before.
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
    let known = held.store.agent(&id).map_err(|e| e.to_string())?;
    let on_engine = known
        .as_ref()
        .map_or_else(|| "claude".to_string(), |t| t.engine.clone());
    let settings = known.as_ref().and_then(|t| t.engine_settings.clone());
    let (home, again) = match &known {
        Some(t) => (std::path::PathBuf::from(&t.cwd), t.opened),
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
                .begin(&id, "New errand", &home)
                .map_err(|e| e.to_string())?;
            (home, false)
        }
    };

    let (engine, events): (Box<dyn Engine + Send>, _) = match on_engine.as_str() {
        "local" => {
            // Everything about a local model has to be told to it. Nothing is
            // remembered by anything outside this app, which is the difference
            // between the two: Claude Code holds its own session, and this
            // holds none.
            let settings: LlmSettings = serde_json::from_str(&settings.unwrap_or_default())
                .map_err(|_| "this thread has no model chosen".to_string())?;
            let (it, events) = Local::open(settings, home).map_err(|e| e.to_string())?;
            (Box::new(it), events)
        }
        _ => {
            let (it, events) = Claude::open(&id, &home, again).map_err(|e| e.to_string())?;
            (Box::new(it), events)
        }
    };
    held.live.lock().unwrap().insert(id.clone(), engine);

    // Everything it says: written down, then forwarded. In that order, so that
    // a window which reloads a moment later reads the same conversation it was
    // just shown.
    let store = held.store.clone();
    let settling = held.settling.clone();
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
                            .and_modify(|so_far| {
                                so_far.push_str(text);
                                so_far.push('\n');
                            });
                    }
                    Event::Done { .. } | Event::Failed { .. } => {
                        let said = settling.lock().unwrap().remove(&id).unwrap_or_default();
                        if let Some(on) = read_what_it_settled_on(&said) {
                            if let Err(e) = store.settled_on(&id, &on) {
                                eprintln!("could not write down who {id} is: {e}");
                            } else {
                                let _ = app.emit("settled", (&id, &on));
                            }
                        }
                    }
                    _ => {}
                }
                continue;
            }

            // The first errand is finished and nobody has named this yet. Ask
            // it who it is, now that it knows what the job was.
            if matches!(event, Event::Done { .. })
                && store
                    .agent(&id)
                    .ok()
                    .flatten()
                    .is_some_and(|a| a.name == NOT_YET_NAMED)
            {
                settling.lock().unwrap().insert(id.clone(), String::new());
                let asked = {
                    let held: State<Held> = app.state();
                    let mut live = held.live.lock().unwrap();
                    live.get_mut(&id).map(|engine| engine.say(WHO_ARE_YOU))
                };
                // Nothing to ask, or it would not take the question. Either
                // way it keeps the name it has and is asked again next time.
                if !matches!(asked, Some(Ok(()))) {
                    settling.lock().unwrap().remove(&id);
                }
            }

            if let Err(e) = store.happened(&id, &event) {
                // Losing a line is not worth ending the conversation over, but
                // it must not pass in silence either.
                eprintln!("could not write down what happened in {id}: {e}");
            }
            tell_them(&app, &store, &id, &event);
            let _ = app.emit(
                "happened",
                Happened {
                    thread: id.clone(),
                    event,
                },
            );
        }
    });
    Ok(())
}

/// Say something. Safe while it is working: that is the point of the thing.
#[tauri::command]
async fn say(held: State<'_, Held>, id: String, text: String) -> Result<(), String> {
    // Written down first. If the agent cannot be reached, what was said is
    // still what was said, and it will be there when the thread is reopened.
    held.store.asked(&id, &text).map_err(|e| e.to_string())?;

    let mut live = held.live.lock().unwrap();
    let thread = live
        .get_mut(&id)
        .ok_or_else(|| "that conversation is not open".to_string())?;
    thread.say(&text).map_err(|e| e.to_string())
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
async fn engines() -> Result<Vec<Choice>, String> {
    let mut all = vec![Choice {
        engine: "claude".into(),
        name: "Claude".into(),
        settings: None,
    }];
    for found in find::detect_all().await {
        for model in found.models {
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
                name: format!("{model} · {}", found.label),
                settings: serde_json::to_string(&settings).ok(),
            });
        }
    }
    Ok(all)
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
    if let Some(mut was) = held.live.lock().unwrap().remove(&id) {
        let _ = was.stop();
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

/// Open a link somewhere that is not this window.
///
/// A link followed inside the webview replaces the app with a web page and
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
    let home = held
        .store
        .agent(&id)
        .map_err(|e| e.to_string())?
        .map(|t| std::path::PathBuf::from(t.cwd))
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
    if let Some(mut thread) = held.live.lock().unwrap().remove(&id) {
        thread.stop().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Forget a thread and everything said in it.
#[tauri::command]
async fn forget(held: State<'_, Held>, id: String) -> Result<(), String> {
    if let Some(mut thread) = held.live.lock().unwrap().remove(&id) {
        let _ = thread.stop();
    }
    held.store.forget(&id).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let here = where_things_live(&app.handle().clone())?;
            let store = Store::open(&errand_core::store::beside(&here))?;
            app.manage(Held {
                live: Mutex::new(HashMap::new()),
                settling: Settling::default(),
                store: Arc::new(store),
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            agents,
            matching,
            lines,
            open_thread,
            say,
            answer,
            engines,
            use_engine,
            outside,
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
        .run(|_app, event| {
            if matches!(event, tauri::RunEvent::Ready) {
                onscreen::ask();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

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
