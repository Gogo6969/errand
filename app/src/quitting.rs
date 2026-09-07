//! Quitting, and the question before it.
//!
//! Closing the window no longer stops anything, so quitting is the one thing
//! that does, and it is one keystroke. Cmd-Q at four minutes to eight with the
//! briefing at eight is far more often a habit than a decision, and the
//! briefing then runs late at the next opening, which is honest and is also
//! the briefing not being there at eight. So the keystroke asks,
//! when and only when something is due soon: an app that asks "are you sure"
//! every time teaches people to press Return without reading, and the one time
//! it mattered is lost in the habit.
//!
//! The system's own alert, through the toolkit already compiled into this app
//! for its window, rather than a dialog plugin for one question. It has to be
//! on the main thread, which a menu event is; `osascript` is the way when it
//! somehow is not, the same route `say_it_out_loud` takes for the one message
//! that has to be shown before there is an app at all.

use chrono::{DateTime, Local};
use errand_core::routine::DueSoon;
use errand_core::watch;
use objc2_app_kit::{NSAlert, NSAlertSecondButtonReturn, NSAlertStyle};
use objc2_foundation::NSString;

/// How far ahead the question looks, in minutes.
///
/// As arbitrary as the ten minutes the clock allows before it calls a run
/// late. Chosen so that quitting an hour before the briefing is not nagged
/// and quitting right before it is.
pub const MINUTES_OF_NOTICE: i64 = 15;

/// The id of the Quit item in the menu bar, which is the only way in here.
pub const QUIT_ITEM: &str = "quit-errand";

/// The two answers, as the buttons say them.
pub const QUIT_ANYWAY: &str = "Quit anyway";
pub const KEEP_RUNNING: &str = "Keep running";

/// What the question says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub title: String,
    pub body: String,
}

/// The words, given who is due and when.
///
/// The name and the time are the whole of the title, because those are the
/// two things that decide the answer: "a routine is due soon" would send
/// somebody to the Repeat panel to find out which, and by then the habit has
/// pressed Return.
pub fn the_question(due: &DueSoon, now: DateTime<Local>) -> Question {
    let minutes = due.at.signed_duration_since(now).num_minutes();
    let how_soon = match minutes {
        // Overdue by seconds, waiting on the clock's next tick.
        ..=0 => "any moment now".to_string(),
        1 => "in a minute".to_string(),
        n => format!("in {n} minutes"),
    };
    // An agent's name is whatever its first message was, cut short, so it is
    // usually a sentence and not a name, and spliced into this one it read as
    // nonsense. `reads_as_a_name` decides, as it does everywhere a name goes
    // into prose.
    let who = match watch::reads_as_a_name(&due.who) {
        true => due.who.clone(),
        false => "A routine".to_string(),
    };
    Question {
        title: format!("{who} is due at {}, {how_soon}.", due.at.format("%H:%M")),
        // What comes back after quitting and what does not, said exactly:
        // the clock reads routines and watches off the store, and nothing
        // restarts a command that was running or picks a goal up mid-turn.
        // And a run is only called late past ten minutes, so "says so" on its
        // own promised a sentence the commonest case never gets.
        body: "Quitting stops it, and every other routine, watch, goal and started command. \
               Routines and watches carry on when Errand is opened again, and a run more \
               than ten minutes late says so; a goal part way through or a command that \
               was running has to be started again. Closing the window instead keeps \
               everything running."
            .to_string(),
    }
}

/// Ask it on screen. True means quit.
pub fn asked(question: &Question) -> bool {
    on_the_main_thread(question).unwrap_or_else(|| through_osascript(question))
}

/// The system's alert, which can only be shown from the main thread.
///
/// Nothing when this is not that thread, rather than a crash: the toolkit
/// checks, and a question that cannot be shown here is still worth asking the
/// other way.
fn on_the_main_thread(question: &Question) -> Option<bool> {
    let main = objc2::MainThreadMarker::new()?;
    let alert = NSAlert::new(main);
    alert.setMessageText(&NSString::from_str(&question.title));
    alert.setInformativeText(&NSString::from_str(&question.body));
    alert.setAlertStyle(NSAlertStyle::Warning);
    // Added first, so that Return keeps it running. The safe answer is the
    // one a reflex lands on; quitting takes a click on the other button.
    let _ = alert.addButtonWithTitle(&NSString::from_str(KEEP_RUNNING));
    let _ = alert.addButtonWithTitle(&NSString::from_str(QUIT_ANYWAY));
    Some(alert.runModal() == NSAlertSecondButtonReturn)
}

/// The same question through the system's scripting, from any thread.
///
/// A dialog that could not be shown at all answers "keep running": the person
/// pressed one key and nothing was asked, and Quit from the Dock still works.
/// The other way round, a failure that quits, is a run lost to a fault nobody
/// saw.
fn through_osascript(question: &Question) -> bool {
    let quoted = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    std::process::Command::new("osascript")
        .arg("-e")
        .arg(format!(
            "display dialog \"{}\" with title \"{}\" buttons {{\"{KEEP_RUNNING}\", \
             \"{QUIT_ANYWAY}\"}} default button \"{KEEP_RUNNING}\" cancel button \
             \"{KEEP_RUNNING}\"",
            quoted(&question.body),
            quoted(&question.title)
        ))
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).contains(QUIT_ANYWAY))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(hour: u32, minute: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 9, 5, hour, minute, 0)
            .single()
            .expect("a real local moment")
    }

    #[test]
    fn the_quit_question_names_the_routine_and_offers_to_quit_anyway() {
        // The two things that decide the answer are the name and the time;
        // "a routine is due soon" sends somebody to find out which, and by
        // then the habit has pressed Return.
        let asked = the_question(
            &DueSoon {
                who: "Trend Scout".to_string(),
                at: at(8, 0),
            },
            at(7, 56),
        );
        assert_eq!(asked.title, "Trend Scout is due at 08:00, in 4 minutes.");
        assert!(asked.body.contains("Closing the window"), "{}", asked.body);
        // The words on the buttons, which the text is written to sit beside.
        assert_eq!(QUIT_ANYWAY, "Quit anyway");
        assert_eq!(KEEP_RUNNING, "Keep running");
    }

    #[test]
    fn an_overdue_routine_is_due_any_moment_and_one_a_minute_off_is_in_a_minute() {
        // Overdue by seconds is the clock's next tick, not "in -1 minutes".
        let scout = |at| DueSoon {
            who: "Trend Scout".to_string(),
            at,
        };
        assert_eq!(
            the_question(&scout(at(8, 0)), at(8, 0)).title,
            "Trend Scout is due at 08:00, any moment now."
        );
        assert_eq!(
            the_question(&scout(at(8, 0)), at(7, 59)).title,
            "Trend Scout is due at 08:00, in a minute."
        );
    }

    #[test]
    fn a_name_that_is_really_a_sentence_is_not_spliced_into_the_question() {
        // An agent's name is its first message cut short. "Write a file called
        // hello.txt containing… is due at 08:00" is not a sentence anybody
        // can act on.
        let asked = the_question(
            &DueSoon {
                who: "Write a file called hello.txt containing\u{2026}".to_string(),
                at: at(8, 0),
            },
            at(7, 56),
        );
        assert_eq!(asked.title, "A routine is due at 08:00, in 4 minutes.");
    }
}
