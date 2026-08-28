//! The window, and the threads it is showing.
//!
//! Everything the window can do is here, and it is three things: open a thread,
//! say something to it, stop it. What comes back does not come back from a
//! command -- it arrives on its own, as the agent produces it, because a
//! conversation where the answer only appears when you ask for it is not a
//! conversation.
//!
//! The window is never told which engine is answering. It receives the events
//! in `errand_core::engine` and nothing else, which is the whole reason that
//! protocol is as small as it is: on the day a local model is driving instead
//! of Claude Code, nothing in here or in the page changes.

use std::collections::HashMap;
use std::sync::Mutex;

use errand_core::{claude::Claude, Engine, Event};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

/// Every conversation this window has open.
#[derive(Default)]
struct Threads(Mutex<HashMap<String, Claude>>);

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

/// Start a conversation, or pick up one this thread had before.
#[tauri::command]
async fn open_thread(
    app: AppHandle,
    threads: State<'_, Threads>,
    id: String,
) -> Result<(), String> {
    if threads.0.lock().unwrap().contains_key(&id) {
        return Ok(()); // Already talking to it.
    }
    // Where the agent works. Its own folder per thread, so one errand cannot
    // tidy up after another, and so "the files from that thing last Tuesday"
    // are still somewhere findable.
    let home = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("threads")
        .join(&id);
    std::fs::create_dir_all(&home).map_err(|e| e.to_string())?;

    let (claude, events) = Claude::open(&id, &home).map_err(|e| e.to_string())?;
    threads.0.lock().unwrap().insert(id.clone(), claude);

    // Everything it says, forwarded as it says it. On a thread of its own
    // because this outlives the command that started it.
    std::thread::spawn(move || {
        while let Ok(event) = events.recv() {
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
async fn say(threads: State<'_, Threads>, id: String, text: String) -> Result<(), String> {
    let mut open = threads.0.lock().unwrap();
    let thread = open
        .get_mut(&id)
        .ok_or_else(|| "that conversation is not open".to_string())?;
    thread.say(&text).map_err(|e| e.to_string())
}

/// Stop it, whatever it is in the middle of.
#[tauri::command]
async fn stop(threads: State<'_, Threads>, id: String) -> Result<(), String> {
    if let Some(mut thread) = threads.0.lock().unwrap().remove(&id) {
        thread.stop().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(Threads::default())
        .invoke_handler(tauri::generate_handler![open_thread, say, stop])
        .run(tauri::generate_context!())
        .expect("running the window");
}
