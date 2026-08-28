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

use errand_core::{claude::Claude, Engine, Event, Line, Store, Thread};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

mod onscreen;

/// Everything the window is holding: the conversations that are live, and the
/// book they are all written into.
struct Held {
    live: Mutex<HashMap<String, Claude>>,
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
    match store.thread(id) {
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

/// Every thread there has ever been, most recently spoken to first.
#[tauri::command]
async fn threads(held: State<'_, Held>) -> Result<Vec<Thread>, String> {
    held.store.threads().map_err(|e| e.to_string())
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
    let known = held.store.thread(&id).map_err(|e| e.to_string())?;
    let (home, again) = match &known {
        Some(t) => (std::path::PathBuf::from(&t.cwd), t.opened),
        None => {
            // Its own folder per thread, so one errand cannot tidy up after
            // another, and so "the files from that thing last Tuesday" are
            // still somewhere findable.
            let home = app
                .path()
                .app_data_dir()
                .map_err(|e| e.to_string())?
                .join("threads")
                .join(&id);
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

    let (claude, events) = Claude::open(&id, &home, again).map_err(|e| e.to_string())?;
    held.live.lock().unwrap().insert(id.clone(), claude);

    // Everything it says: written down, then forwarded. In that order, so that
    // a window which reloads a moment later reads the same conversation it was
    // just shown.
    let store = held.store.clone();
    std::thread::spawn(move || {
        while let Ok(event) = events.recv() {
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

/// Give a thread the name it will be remembered by.
#[tauri::command]
async fn call_it(held: State<'_, Held>, id: String, name: String) -> Result<(), String> {
    held.store.call_it(&id, &name).map_err(|e| e.to_string())
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
            let data = app.path().app_data_dir()?;
            let store = Store::open(&errand_core::store::beside(&data))?;
            app.manage(Held {
                live: Mutex::new(HashMap::new()),
                store: Arc::new(store),
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            threads,
            lines,
            open_thread,
            say,
            call_it,
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
