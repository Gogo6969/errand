//! When a conversation runs itself.
//!
//! A routine is not a new kind of thing here. It is a conversation with a
//! schedule and something to say, which means yesterday's briefing is sitting
//! directly above today's in the same place, and "what did it say last
//! Tuesday" is scrolling rather than archaeology.
//!
//! The schedule vocabulary is deliberately tiny: a time of day, a time on
//! certain days, or an interval. Not cron. Cron is five fields of positional
//! syntax that nobody writes correctly from memory, and every one of them that
//! is wrong is wrong silently until the morning somebody notices the briefing
//! never came. What is here can be read aloud, which is the test it has to
//! pass: `daily 07:00` and `every 30m` mean what they look like.
//!
//! Everything is local time, because seven in the morning is a fact about
//! somebody's morning and not about UTC. That has a consequence worth stating:
//! the hour that does not exist on the day the clocks go forward does not
//! happen, and the hour that happens twice does not run twice. Both are handled
//! by asking for the next occurrence strictly after the last one.

use std::collections::HashSet;

use anyhow::{bail, Result};
use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, TimeZone, Timelike, Weekday};
use serde::{Deserialize, Serialize};

use crate::store::Conversation;

/// What a standing job needs in order to run, in the words every one of them
/// uses.
///
/// One phrase rather than five, because five drifted: a watch said "while
/// Errand is open", a goal said the same, a started command said "for as long
/// as Errand is open", and the day closing the window stopped being closing
/// the app all of them were wrong in the same way. The window can be closed.
/// The process is what has to be there.
pub const WHILE_RUNNING: &str = "while Errand is running, window or no window";

/// What a routine is told when it is set, about what keeps it running and what
/// does not.
///
/// Said in the confirmation rather than in a settings card nobody opens,
/// because this is the moment somebody is deciding to rely on it. Every clause
/// is a case that was actually asked about: the window, quitting, sleep, the
/// lid, logging out, a restart. The ten minutes is the clock's own patience
/// before a run calls itself late, named here so this does not promise a
/// sentence the commonest case never gets.
pub const WHAT_KEEPS_IT_RUNNING: &str = "Closing the window does not stop it: Errand keeps \
     running in the Dock. Quitting Errand does, and Cmd-Q asks first when a run is due in the \
     next fifteen minutes. A Mac that is asleep, has its lid down or is off runs nothing; a \
     run whose time passed happens the next time Errand is running and, when it is more than \
     ten minutes late, says so. With Open Errand when I log in switched on under Settings, a \
     restart brings it back.";

/// When a routine wants to run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum When {
    /// Every day at a time. `daily 07:00`
    Daily { at: NaiveTime },
    /// On certain days of the week, at a time. `weekly mon,wed 09:30`
    Weekly { days: Vec<Weekday>, at: NaiveTime },
    /// Repeatedly, measured from the last run. `every 30m`, `every 6h`
    Every { minutes: i64 },
}

/// The most often a routine can run, in minutes.
///
/// Below this it would run every time the scheduler looked, which is a busy
/// loop wearing a schedule.
pub const FEWEST_MINUTES: i64 = 1;

/// The most often a routine can run, written the way a schedule is written.
///
/// One place, read both by `read` when it refuses and by the tool text an
/// agent reads before choosing, because the two drifted: the tool showed
/// `every 30m` as its one example of an interval, a model took that for the
/// floor, and rather than set a two-minute routine it started a shell loop
/// that nothing here could see, stop or write down.
pub fn most_often() -> String {
    When::Every {
        minutes: FEWEST_MINUTES,
    }
    .written()
}

impl When {
    /// Read a schedule the way somebody would write one.
    pub fn read(said: &str) -> Result<Self> {
        let said = said.trim().to_lowercase();
        let mut words = said.split_whitespace();
        match words.next() {
            Some("daily") => Ok(When::Daily {
                at: clock(words.next().unwrap_or_default())?,
            }),
            Some("weekly") => {
                let days = words.next().unwrap_or_default();
                let at = clock(words.next().unwrap_or_default())?;
                let days: Result<Vec<Weekday>> = days.split(',').map(day).collect();
                let days = days?;
                if days.is_empty() {
                    bail!("weekly needs days, like `weekly mon,thu 09:00`");
                }
                Ok(When::Weekly { days, at })
            }
            Some("every") => {
                let span = words.next().unwrap_or_default();
                let (count, unit) = span.split_at(span.len().saturating_sub(1));
                let count: i64 = count
                    .parse()
                    .map_err(|_| anyhow::anyhow!("`{span}` is not a length of time"))?;
                let minutes = match unit {
                    "m" => count,
                    "h" => count * 60,
                    "d" => count * 60 * 24,
                    // With the floor named, like the refusal below it: `every
                    // 30m` as the one example was read as the floor once.
                    _ => bail!(
                        "`{span}` should end in m, h or d, like `every 2m`; `{}` is as often as \
                         it goes",
                        most_often()
                    ),
                };
                // Said with the floor in it, because this is read by a model
                // choosing how often, and "too often" alone sent one off to
                // build a loop instead.
                if minutes < FEWEST_MINUTES {
                    bail!(
                        "that is too often to be a routine; the most often is `{}`",
                        most_often()
                    );
                }
                Ok(When::Every { minutes })
            }
            _ => bail!(
                "try `daily 07:00`, `weekly mon,fri 09:30` or `every 2m`; `{}` is the most often",
                most_often()
            ),
        }
    }

    /// The first time this should run strictly after `since`.
    ///
    /// Strictly after, which is what makes a run that takes a while safe: a
    /// routine that finished at 07:00:40 must not be due again at 07:00:41
    /// because its own start time still matches.
    pub fn next_after(&self, since: DateTime<Local>) -> Option<DateTime<Local>> {
        match self {
            When::Daily { at } => {
                (0..=1).find_map(|days| on(since + Duration::days(days), *at, since))
            }
            When::Weekly { days, at } => (0..=7).find_map(|ahead| {
                let day = since + Duration::days(ahead);
                days.contains(&day.weekday())
                    .then(|| on(day, *at, since))
                    .flatten()
            }),
            When::Every { minutes } => Some(since + Duration::minutes(*minutes)),
        }
    }

    /// Said back the way it was written, so what is stored round-trips.
    pub fn written(&self) -> String {
        match self {
            When::Daily { at } => format!("daily {}", at.format("%H:%M")),
            When::Weekly { days, at } => format!(
                "weekly {} {}",
                days.iter()
                    .map(|d| d.to_string().to_lowercase()[..3].to_string())
                    .collect::<Vec<_>>()
                    .join(","),
                at.format("%H:%M")
            ),
            When::Every { minutes } => match (minutes % (60 * 24) == 0, minutes % 60 == 0) {
                (true, _) => format!("every {}d", minutes / (60 * 24)),
                (_, true) => format!("every {}h", minutes / 60),
                _ => format!("every {minutes}m"),
            },
        }
    }
}

/// That day at that time, if it is after the moment we are counting from.
fn on(day: DateTime<Local>, at: NaiveTime, after: DateTime<Local>) -> Option<DateTime<Local>> {
    let when = Local
        .with_ymd_and_hms(
            day.year(),
            day.month(),
            day.day(),
            at.hour(),
            at.minute(),
            0,
        )
        // On the morning the clocks go forward this hour does not exist, and on
        // the morning they go back it exists twice. `single()` refuses both
        // rather than guessing, and the day is skipped -- which is the honest
        // answer for an hour that did not happen.
        .single()?;
    (when > after).then_some(when)
}

fn clock(said: &str) -> Result<NaiveTime> {
    NaiveTime::parse_from_str(said, "%H:%M")
        .map_err(|_| anyhow::anyhow!("`{said}` is not a time of day; try 07:00"))
}

fn day(said: &str) -> Result<Weekday> {
    match &said.trim()[..said.trim().len().min(3)] {
        "mon" => Ok(Weekday::Mon),
        "tue" => Ok(Weekday::Tue),
        "wed" => Ok(Weekday::Wed),
        "thu" => Ok(Weekday::Thu),
        "fri" => Ok(Weekday::Fri),
        "sat" => Ok(Weekday::Sat),
        "sun" => Ok(Weekday::Sun),
        other => bail!("`{other}` is not a day"),
    }
}

/// Whether two routines are the same routine written twice.
///
/// Not string equality, which never catches it: somebody adding the briefing
/// again next month types "brief me on bitcoin" where they typed "give me the
/// bitcoin briefing", and gets two agents doing the same thing every morning
/// with no sign anywhere that they are.
///
/// The time has to match, because the same instruction at seven and at six is
/// two different arrangements that somebody may well want. What is said only
/// has to be mostly the same.
pub fn the_same_thing_twice(one_at: &str, one_what: &str, two_at: &str, two_what: &str) -> bool {
    let Ok(one) = When::read(one_at) else {
        return false;
    };
    let Ok(two) = When::read(two_at) else {
        return false;
    };
    if one != two {
        return false;
    }
    mostly_the_same(one_what, two_what)
}

/// Whether two instructions say mostly the same thing.
///
/// Compared as a set of the words that carry meaning, because word order and
/// politeness vary and neither changes what an agent will do.
fn mostly_the_same(one: &str, two: &str) -> bool {
    let words = |s: &str| {
        let mut all: Vec<String> = s
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 3)
            .map(str::to_string)
            .collect();
        all.sort();
        all.dedup();
        all
    };
    let (a, b) = (words(one), words(two));
    if a.is_empty() || b.is_empty() {
        return one.trim().eq_ignore_ascii_case(two.trim());
    }
    // "brief" and "briefing" are the same word for this purpose, and that is
    // the whole difficulty: nobody types it identically the second time, so
    // comparing whole words catches nothing and the duplicate goes in.
    let same_word = |x: &String, y: &String| x.starts_with(y.as_str()) || y.starts_with(x.as_str());
    let shared = a
        .iter()
        .filter(|w| b.iter().any(|o| same_word(w, o)))
        .count();
    // Most of the shorter one, so padding one out with extra words cannot hide
    // that it says the same thing.
    shared * 3 >= a.len().min(b.len()) * 2
}

/// What to say about one that already exists.
pub fn already_doing_this(who: &str, at: &str) -> String {
    format!(
        "{who} already does almost exactly this at {at}. Two agents on the same \
         job every morning is usually a mistake, and nothing else here would \
         have told you."
    )
}

/// What a run that arrives late should say about itself.
///
/// It used to say the time it was actually running, which is the one time
/// nobody needs: a briefing due at seven and opened at nine announced itself as
/// "due at 09:00 and nothing was running then", which is not late, not true,
/// and not what happened. The whole worth of the sentence is the gap between
/// the two times, and it had thrown one of them away.
///
/// The day is named as well as the time once it is not today. "Due at 07:00" on
/// a Monday morning reads as an hour ago; if the Mac was shut all weekend it
/// was three days ago, and those are different pieces of news.
pub fn arriving_late(due: DateTime<Local>, now: DateTime<Local>) -> String {
    let clock = due.format("%H:%M");
    // By calendar day rather than by hours: something due at 23:50 and run at
    // 00:10 is yesterday's, though it is twenty minutes old.
    let days = (now.date_naive() - due.date_naive()).num_days();
    let when = match days {
        ..=0 => format!("at {clock}"),
        1 => format!("at {clock} yesterday"),
        // Inside the week the name of the day is what somebody actually thinks
        // in. Past that it stops being a name and starts being a guess about
        // which week.
        2..=6 => format!("at {clock} on {}", due.format("%A")),
        _ => format!("at {clock} on {}", due.format("%-d %B")),
    };
    format!("(This is late: it was due {when} and nothing was running then.)")
}

/// The moment a routine's next run is counted from.
///
/// Its last run, or the moment the schedule was set. Not "now" -- a routine
/// whose time passed while the app was closed is overdue, and treating it as
/// though it had only just been set would quietly move it to tomorrow.
pub fn counting_from(c: &Conversation, now: DateTime<Local>) -> DateTime<Local> {
    c.ran_at
        .or(Some(c.started_at))
        .and_then(|ms| Local.timestamp_millis_opt(ms).single())
        .unwrap_or(now)
}

/// A routine due soon enough that quitting now would cut in front of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueSoon {
    /// Who runs it: the agent's name, as a person knows it.
    pub who: String,
    /// When it is due. Already past, for one whose tick has not come round.
    pub at: DateTime<Local>,
}

/// Which routine, if any, is due within the next `minutes`.
///
/// The judgement behind the question Cmd-Q asks, kept here and pure so it can
/// be tested without an event loop. `routines` are the conversations with a
/// schedule, each with the name of the agent that runs it; `still_running` are
/// the ids with a run going this moment.
///
/// One switched off is not a reason to stay open: Pause under Repeat sets
/// `routine_off`, and the clock walks past it. One still running is not
/// either, because the clock will not start it again on top of itself, and a
/// turn that quitting cuts short is already said so in the conversation when
/// Errand comes back. One overdue, whose time has passed and whose tick has not
/// come round, is due now and not tomorrow.
///
/// The soonest of several, because the question has room for one name, and
/// the one about to be missed is the one that matters.
pub fn due_within(
    routines: &[(String, Conversation)],
    still_running: &HashSet<String>,
    now: DateTime<Local>,
    minutes: i64,
) -> Option<DueSoon> {
    let horizon = now + Duration::minutes(minutes);
    routines
        .iter()
        .filter(|(_, c)| !c.routine_off && c.runs_what.is_some())
        .filter(|(_, c)| !still_running.contains(&c.id))
        .filter_map(|(who, c)| {
            let when = When::read(c.runs_at.as_deref()?).ok()?;
            let at = when.next_after(counting_from(c, now))?;
            (at <= horizon).then(|| DueSoon {
                who: who.clone(),
                at,
            })
        })
        .min_by_key(|due| due.at)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_late_run_names_the_time_it_was_due_and_not_the_time_it_is_now() {
        // The bug this replaces: a briefing due at seven and opened at nine
        // announced itself as due at nine, which is not late, not true, and not
        // what happened. The gap between the two times is the whole point of
        // the sentence and one of them had been thrown away.
        let due = Local
            .with_ymd_and_hms(2026, 8, 30, 7, 0, 0)
            .single()
            .expect("a time");
        let now = Local
            .with_ymd_and_hms(2026, 8, 30, 9, 12, 0)
            .single()
            .expect("a time");
        let said = arriving_late(due, now);
        assert!(said.contains("07:00"), "{said}");
        assert!(!said.contains("09:12"), "{said}");
    }

    #[test]
    fn a_run_from_another_day_says_which_day() {
        // "Due at 07:00" on a Monday reads as an hour ago. If the Mac was shut
        // all weekend it was three days ago, and those are different news.
        let due = Local
            .with_ymd_and_hms(2026, 8, 28, 7, 0, 0)
            .single()
            .expect("a time");
        assert!(arriving_late(
            due,
            Local
                .with_ymd_and_hms(2026, 8, 29, 9, 0, 0)
                .single()
                .unwrap()
        )
        .contains("yesterday"),);
        // Friday, from the following Monday.
        assert!(arriving_late(
            due,
            Local
                .with_ymd_and_hms(2026, 8, 31, 9, 0, 0)
                .single()
                .unwrap()
        )
        .contains("Friday"),);
        // Past a week the name of a day stops being a name and starts being a
        // guess about which week it was.
        let old = arriving_late(
            due,
            Local
                .with_ymd_and_hms(2026, 9, 20, 9, 0, 0)
                .single()
                .unwrap(),
        );
        assert!(old.contains("28 August"), "{old}");
    }

    #[test]
    fn a_run_late_across_midnight_belongs_to_the_day_it_was_due() {
        // Twenty minutes old and still yesterday's, which is what somebody
        // reading it at ten past midnight needs to be told.
        let due = Local
            .with_ymd_and_hms(2026, 8, 29, 23, 50, 0)
            .single()
            .expect("a time");
        let now = Local
            .with_ymd_and_hms(2026, 8, 30, 0, 10, 0)
            .single()
            .expect("a time");
        assert!(
            arriving_late(due, now).contains("yesterday"),
            "{}",
            arriving_late(due, now)
        );
    }

    #[test]
    fn the_same_job_at_the_same_time_is_noticed_however_it_was_worded() {
        // Nobody types it the same way twice, so string equality never catches
        // this and two agents end up doing the same thing every morning.
        assert!(the_same_thing_twice(
            "daily 07:00",
            "Give me the Bitcoin briefing",
            "daily 7:00",
            "Brief me on bitcoin",
        ));
    }

    #[test]
    fn the_same_job_at_a_different_time_is_two_arrangements() {
        // Somebody may well want the briefing at six and again at noon.
        assert!(!the_same_thing_twice(
            "daily 07:00",
            "Give me the Bitcoin briefing",
            "daily 12:00",
            "Give me the Bitcoin briefing",
        ));
    }

    #[test]
    fn two_different_jobs_at_the_same_time_are_not_a_duplicate() {
        assert!(!the_same_thing_twice(
            "daily 07:00",
            "Give me the Bitcoin briefing",
            "daily 07:00",
            "Check whether the backups ran overnight",
        ));
    }

    #[test]
    fn padding_one_out_does_not_hide_that_it_says_the_same_thing() {
        assert!(the_same_thing_twice(
            "daily 07:00",
            "bitcoin briefing",
            "daily 07:00",
            "please could you put together the bitcoin briefing for me thanks",
        ));
    }

    #[test]
    fn a_schedule_nobody_can_read_is_never_called_a_duplicate() {
        // Refusing to parse and calling it a match are different failures, and
        // the second one blocks something that was fine.
        assert!(!the_same_thing_twice("nonsense", "a", "daily 07:00", "a"));
        assert!(!the_same_thing_twice("daily 07:00", "a", "nonsense", "a"));
    }

    /// A local moment, because every time in here is a local time. Building
    /// these as UTC and converting was the first version, and it moved every
    /// test time by the offset -- so "07:00" became three in the morning and
    /// the assertions about which day it landed on were nonsense.
    fn at(s: &str) -> DateTime<Local> {
        let naive = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap();
        Local
            .from_local_datetime(&naive)
            .single()
            .expect("a real local moment")
    }

    #[test]
    fn a_schedule_reads_the_way_somebody_would_write_one() {
        for said in [
            "daily 07:00",
            "weekly mon,fri 09:30",
            "every 30m",
            "every 6h",
        ] {
            let when = When::read(said).unwrap_or_else(|e| panic!("{said}: {e}"));
            assert_eq!(when.written(), said, "it did not survive the round trip");
        }
    }

    #[test]
    fn something_that_is_not_a_schedule_says_so_rather_than_meaning_something_else() {
        // The failure mode this avoids is a schedule that parses into the wrong
        // time and is only noticed on the morning nothing arrives.
        for nonsense in [
            "",
            "daily",
            "daily 25:00",
            "daily 7",
            "every",
            "every 30",
            "every 0m",
            "weekly 09:00",
            "sometimes",
        ] {
            assert!(When::read(nonsense).is_err(), "{nonsense:?} was accepted");
        }
    }

    #[test]
    fn a_minute_is_the_most_often_a_routine_can_run_and_asking_for_less_names_that_floor() {
        // The floor is a minute, not the thirty in the example. A model that
        // took the example for the floor started a shell loop rather than a
        // two-minute routine, so both halves are pinned here: the shortest
        // schedule is accepted as written, and the refusal says what the
        // shortest is, in a sentence the model can act on.
        assert_eq!(most_often(), "every 1m");
        for said in ["every 1m", "every 2m"] {
            let when = When::read(said).unwrap_or_else(|e| panic!("{said}: {e}"));
            assert_eq!(when.written(), said, "it did not survive the round trip");
        }
        let refused = When::read("every 0m").expect_err("nothing below a minute is a routine");
        assert!(
            refused.to_string().contains(&most_often()),
            "the refusal does not say how often is allowed: {refused}"
        );
        // And the two refusals beside it, which are what a person typing by
        // hand reads in the Repeat panel and what a model reads when it
        // guessed at the shape, no longer show thirty minutes as the only
        // interval there is.
        for said in ["every 90s", "sometimes"] {
            let refused = When::read(said).expect_err("not a schedule");
            assert!(
                refused.to_string().contains(&most_often()),
                "{said}: the refusal does not say how often is allowed: {refused}"
            );
            assert!(
                !refused.to_string().contains("30m"),
                "{said}: thirty minutes is still the example: {refused}"
            );
        }
    }

    /// A routine as the clock sees one: an agent's name and the conversation
    /// that carries the schedule, last run at `ran`.
    fn a_routine(who: &str, runs_at: &str, ran: &str) -> (String, Conversation) {
        (
            who.to_string(),
            Conversation {
                id: who.to_lowercase().replace(' ', "-"),
                runs_at: Some(runs_at.to_string()),
                runs_what: Some("the briefing".to_string()),
                ran_at: Some(at(ran).timestamp_millis()),
                ..Default::default()
            },
        )
    }

    #[test]
    fn a_routine_due_within_the_next_while_is_named_with_the_minute_it_is_due() {
        // Cmd-Q at four minutes to eight, with the briefing at eight. The
        // question needs the name and the time, and nothing else about it.
        let scout = a_routine("Trend Scout", "daily 08:00", "2026-09-04 08:00:20");
        let due = due_within(&[scout], &HashSet::new(), at("2026-09-05 07:56:00"), 15)
            .expect("four minutes away is within fifteen");
        assert_eq!(due.who, "Trend Scout");
        assert_eq!(due.at, at("2026-09-05 08:00:00"));

        // One whose time has passed and whose tick has not come round is due
        // now, not tomorrow: quitting in that ten-second gap loses the run
        // exactly as surely as quitting a minute before it.
        let scout = a_routine("Trend Scout", "daily 08:00", "2026-09-04 08:00:20");
        let due = due_within(&[scout], &HashSet::new(), at("2026-09-05 08:00:10"), 15)
            .expect("overdue is due");
        assert_eq!(due.at, at("2026-09-05 08:00:00"));
    }

    #[test]
    fn nothing_due_soon_means_quitting_asks_nothing() {
        // An app that asks "are you sure" every time teaches people to press
        // Return without reading, and then the one time it mattered is lost in
        // the habit.
        let scout = a_routine("Trend Scout", "daily 08:00", "2026-09-04 08:00:20");
        assert_eq!(
            due_within(&[scout], &HashSet::new(), at("2026-09-05 06:00:00"), 15),
            None,
            "two hours away is not soon"
        );
        // An ordinary conversation with no schedule is not a routine at all.
        let plain = ("Scout".to_string(), Conversation::default());
        assert_eq!(
            due_within(&[plain], &HashSet::new(), at("2026-09-05 07:56:00"), 15),
            None
        );
        // And a schedule nobody can read is not one either, rather than a
        // question about a routine that will never fire.
        let (who, mut broken) = a_routine("Trend Scout", "daily 08:00", "2026-09-04 08:00:20");
        broken.runs_at = Some("sometimes".to_string());
        assert_eq!(
            due_within(
                &[(who, broken)],
                &HashSet::new(),
                at("2026-09-05 07:56:00"),
                15
            ),
            None
        );
    }

    #[test]
    fn a_routine_switched_off_or_paused_is_not_a_reason_to_stay_open() {
        // Pause under Repeat sets `routine_off`, and the clock walks past it;
        // asking somebody to stay open for a run that will not happen is the
        // nag this question is at pains not to be.
        let (who, mut scout) = a_routine("Trend Scout", "daily 08:00", "2026-09-04 08:00:20");
        scout.routine_off = true;
        assert_eq!(
            due_within(
                &[(who.clone(), scout.clone())],
                &HashSet::new(),
                at("2026-09-05 07:56:00"),
                15
            ),
            None
        );
        // One still running is not started again on top of itself, so it is
        // not due; what quitting does to the run under way is said in the
        // conversation when Errand comes back.
        scout.routine_off = false;
        let running: HashSet<String> = [scout.id.clone()].into_iter().collect();
        assert_eq!(
            due_within(&[(who, scout)], &running, at("2026-09-05 07:56:00"), 15),
            None
        );
    }

    #[test]
    fn the_soonest_of_several_due_routines_is_the_one_named() {
        // Room for one name, and the one about to be missed is the one that
        // matters. Listed later on purpose, so order in the store decides
        // nothing.
        let later = a_routine("Disk Watch", "daily 08:05", "2026-09-04 08:05:20");
        let sooner = a_routine("Trend Scout", "daily 08:00", "2026-09-04 08:00:20");
        let due = due_within(
            &[later, sooner],
            &HashSet::new(),
            at("2026-09-05 07:56:00"),
            15,
        )
        .expect("both are within fifteen minutes");
        assert_eq!(due.who, "Trend Scout");
        assert_eq!(due.at, at("2026-09-05 08:00:00"));
    }

    #[test]
    fn what_a_routine_is_told_says_what_stops_it_and_what_does_not() {
        // The cases somebody actually asks about, each named: the window,
        // quitting, sleep, the lid, being off, and a restart. "It runs while
        // Errand is open" answered none of them and was wrong about the first.
        for case in [
            "Closing the window",
            "Dock",
            "Quitting",
            "Cmd-Q",
            "asleep",
            "lid",
            "off",
            "late",
            "log in",
        ] {
            assert!(WHAT_KEEPS_IT_RUNNING.contains(case), "nothing about {case}");
        }
        assert!(!WHAT_KEEPS_IT_RUNNING.contains("while Errand is open"));
        assert_eq!(
            WHILE_RUNNING,
            "while Errand is running, window or no window"
        );
    }

    #[test]
    fn the_next_run_is_strictly_after_the_last_one() {
        // A run that finished at 07:00:40 must not be due again at 07:00:41
        // because its own start time still matches the schedule.
        let when = When::read("daily 07:00").unwrap();
        let ran = at("2026-08-28 07:00:40");
        let next = when.next_after(ran).unwrap();
        assert_eq!(next.date_naive(), at("2026-08-29 07:00:00").date_naive());
        assert_eq!((next.hour(), next.minute()), (7, 0));
    }

    #[test]
    fn a_time_later_today_is_today_and_one_already_past_is_tomorrow() {
        let when = When::read("daily 18:00").unwrap();
        let morning = when.next_after(at("2026-08-28 09:00:00")).unwrap();
        assert_eq!(morning.date_naive(), at("2026-08-28 12:00:00").date_naive());

        let evening = when.next_after(at("2026-08-28 20:00:00")).unwrap();
        assert_eq!(evening.date_naive(), at("2026-08-29 12:00:00").date_naive());
    }

    #[test]
    fn a_weekly_routine_waits_for_one_of_its_own_days() {
        let when = When::read("weekly mon 09:00").unwrap();
        // A Friday.
        let next = when.next_after(at("2026-08-28 10:00:00")).unwrap();
        assert_eq!(next.weekday(), Weekday::Mon);
        assert_eq!(next.hour(), 9);
    }

    #[test]
    fn an_interval_counts_from_when_it_last_ran_rather_than_from_the_hour() {
        // Which is the difference between "every 30 minutes" and "on the hour
        // and the half hour", and the reason this is not cron.
        let when = When::read("every 30m").unwrap();
        let ran = at("2026-08-28 09:07:00");
        assert_eq!(when.next_after(ran).unwrap(), at("2026-08-28 09:37:00"));
    }

    #[test]
    fn every_day_means_every_day_and_not_every_twenty_four_hours() {
        // The difference shows up on the two mornings a year when a day is not
        // twenty-four hours long, and it is the difference between a briefing
        // at seven and a briefing that drifts.
        let when = When::read("daily 07:00").unwrap();
        let mut from = at("2026-08-28 07:00:01");
        for _ in 0..5 {
            let next = when.next_after(from).unwrap();
            assert_eq!((next.hour(), next.minute()), (7, 0), "it drifted to {next}");
            from = next + Duration::seconds(1);
        }
    }
}
