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
use chrono::{
    DateTime, Datelike, Days, Duration, Local, LocalResult, NaiveDate, NaiveTime, TimeZone, Weekday,
};
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

/// The longest a routine can wait between runs, in minutes: a year and a day.
///
/// A number any longer is not a schedule, and a large enough one overflowed the
/// clock's arithmetic: `every 999999999d` was accepted, and then every tick
/// stopped on it before reaching any other routine or any watch.
pub const MOST_MINUTES: i64 = 366 * 24 * 60;

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
    ///
    /// All of it or none of it. Words it did not understand used to be left
    /// out, and what was stored and shown was the text as written: `weekly
    /// mon-fri 09:00` ran on Mondays only, and `every 30m 09:00-17:00` ran
    /// round the clock, forty-eight times a day where sixteen were meant, while
    /// the panel read back exactly what the person had asked for.
    pub fn read(said: &str) -> Result<Self> {
        let said = said.trim().to_lowercase();
        let mut words = said.split_whitespace();
        let when = match words.next() {
            Some("daily") => When::Daily {
                at: clock(words.next().unwrap_or_default())?,
            },
            Some("weekly") => {
                let days = days_in(words.next().unwrap_or_default())?;
                let at = clock(words.next().unwrap_or_default())?;
                When::Weekly { days, at }
            }
            Some("every") => {
                let span = words.next().unwrap_or_default();
                // By character, not by byte: `every 1½` was cut in the middle
                // of the ½ and took the whole clock down with it.
                let mut letters: Vec<char> = span.chars().collect();
                let unit = letters.pop().unwrap_or(' ');
                let count: String = letters.into_iter().collect();
                let count: i64 = count
                    .parse()
                    .map_err(|_| anyhow::anyhow!("`{span}` is not a length of time"))?;
                let minutes = match unit {
                    'm' => Some(count),
                    'h' => count.checked_mul(60),
                    'd' => count.checked_mul(60 * 24),
                    // With the floor named, like the refusal below it: `every
                    // 30m` as the one example was read as the floor once.
                    _ => bail!(
                        "`{span}` should end in m, h or d, like `every 2m`; `{}` is as often as \
                         it goes",
                        most_often()
                    ),
                };
                let too_long = || {
                    anyhow::anyhow!(
                        "`{span}` is longer than a routine waits; the longest is `every 366d`"
                    )
                };
                let minutes = minutes.ok_or_else(too_long)?;
                // Said with the floor in it, because this is read by a model
                // choosing how often, and "too often" alone sent one off to
                // build a loop instead.
                if minutes < FEWEST_MINUTES {
                    bail!(
                        "that is too often to be a routine; the most often is `{}`",
                        most_often()
                    );
                }
                if minutes > MOST_MINUTES {
                    return Err(too_long());
                }
                When::Every { minutes }
            }
            _ => bail!(
                "try `daily 07:00`, `weekly mon,fri 09:30` or `every 2m`; `{}` is the most often",
                most_often()
            ),
        };
        if let Some(more) = words.next() {
            bail!(
                "`{more}` is more than a schedule can say. Write one of `daily 07:00`, `weekly \
                 mon-fri 09:00` or `every 2m`; something that should happen only in working \
                 hours can run on the working days, or look at the time itself when it runs."
            );
        }
        Ok(when)
    }

    /// The first time this should run strictly after `since`.
    ///
    /// Strictly after, which is what makes a run that takes a while safe: a
    /// routine that finished at 07:00:40 must not be due again at 07:00:41
    /// because its own start time still matches.
    pub fn next_after(&self, since: DateTime<Local>) -> Option<DateTime<Local>> {
        self.next_after_in(since)
    }

    /// The same, in any time zone, which is what lets the clock changes of
    /// places other than this Mac's be tested.
    ///
    /// Counted in calendar days rather than in spans of twenty-four hours. The
    /// two differ on the day the clocks change, and the difference lost days:
    /// a routine that ran at half past midnight on the day the clocks went back
    /// was looked for "twenty-four hours later", which was still the same date,
    /// and the one day ahead it looked at was then gone. Nothing was found, and
    /// a routine with nothing to find is never run again.
    pub fn next_after_in<Tz: TimeZone>(&self, since: DateTime<Tz>) -> Option<DateTime<Tz>> {
        let zone = since.timezone();
        let from = since.naive_local().date();
        let ahead = |days: u64| from.checked_add_days(Days::new(days));
        match self {
            When::Daily { at } => (0..=2).find_map(|days| on(&zone, ahead(days)?, *at, &since)),
            When::Weekly { days, at } => (0..=14).find_map(|n| {
                let day = ahead(n)?;
                days.contains(&day.weekday())
                    .then(|| on(&zone, day, *at, &since))
                    .flatten()
            }),
            When::Every { minutes } => since.checked_add_signed(Duration::minutes(*minutes)),
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
fn on<Tz: TimeZone>(
    zone: &Tz,
    day: NaiveDate,
    at: NaiveTime,
    after: &DateTime<Tz>,
) -> Option<DateTime<Tz>> {
    let when = match zone.from_local_datetime(&day.and_time(at)) {
        LocalResult::Single(one) => one,
        // The morning the clocks go back this hour happens twice. The first of
        // the two, so it runs once, when it was meant to. It used to be refused
        // as though it had not happened at all.
        LocalResult::Ambiguous(first, _) => first,
        // The morning they go forward it does not happen. That day is skipped,
        // which is the honest answer for an hour that did not exist, and the
        // days after it are still looked at.
        LocalResult::None => return None,
    };
    (when > *after).then_some(when)
}

fn clock(said: &str) -> Result<NaiveTime> {
    NaiveTime::parse_from_str(said, "%H:%M")
        .map_err(|_| anyhow::anyhow!("`{said}` is not a time of day; try 07:00"))
}

/// The days in a list like `mon,thu` or `mon-fri`.
fn days_in(said: &str) -> Result<Vec<Weekday>> {
    let mut days = Vec::new();
    for one in said.split(',').filter(|one| !one.trim().is_empty()) {
        match one.split_once('-') {
            // A run of days, which may go round the end of the week.
            Some((from, to)) => {
                let (mut at, to) = (day(from)?, day(to)?);
                loop {
                    if !days.contains(&at) {
                        days.push(at);
                    }
                    if at == to {
                        break;
                    }
                    at = at.succ();
                }
            }
            None => {
                let at = day(one)?;
                if !days.contains(&at) {
                    days.push(at);
                }
            }
        }
    }
    if days.is_empty() {
        bail!("weekly needs days, like `weekly mon,thu 09:00` or `weekly mon-fri 09:00`");
    }
    Ok(days)
}

/// One day, by its name. The whole word, because the first three letters of
/// anything were taken as a day before, and cutting a word by bytes at three
/// panicked on one written with an accent.
fn day(said: &str) -> Result<Weekday> {
    Ok(match said.trim() {
        "mon" | "monday" => Weekday::Mon,
        "tue" | "tues" | "tuesday" => Weekday::Tue,
        "wed" | "weds" | "wednesday" => Weekday::Wed,
        "thu" | "thur" | "thurs" | "thursday" => Weekday::Thu,
        "fri" | "friday" => Weekday::Fri,
        "sat" | "saturday" => Weekday::Sat,
        "sun" | "sunday" => Weekday::Sun,
        other => {
            bail!("`{other}` is not a day; days are written mon, tue, wed, thu, fri, sat and sun")
        }
    })
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
    format!(
        "(This is late: it was due {} and nothing was running then.)",
        when_it_was_due(due, now)
    )
}

/// The same, for a run held back by the one before it, which was still going.
///
/// "Nothing was running then" was said of these too, which is the one thing
/// that was not true of them.
pub fn arriving_after_the_last_run(due: DateTime<Local>, now: DateTime<Local>) -> String {
    format!(
        "(This is late: it was due {}, and the run before it was still going.)",
        when_it_was_due(due, now)
    )
}

/// When something was due, in the words somebody would use for it.
fn when_it_was_due(due: DateTime<Local>, now: DateTime<Local>) -> String {
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
    when
}

/// The moment a routine's next run is counted from.
///
/// Its last run, or the moment the schedule was set. Not "now" -- a routine
/// whose time passed while the app was closed is overdue, and treating it as
/// though it had only just been set would quietly move it to tomorrow.
pub fn counting_from(c: &Conversation, now: DateTime<Local>) -> DateTime<Local> {
    // The later of the last run and the moment it was set or switched back on.
    // When the conversation began is only a last resort: counted from there, a
    // schedule set in a conversation a week old was a week overdue the moment
    // it was saved, and ran at once saying it was late.
    let from = match (c.ran_at, c.routine_set_at) {
        (Some(ran), Some(set)) => Some(ran.max(set)),
        (ran, set) => ran.or(set),
    };
    from.or(Some(c.started_at))
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

    use chrono::Timelike;

    /// A moment in a named place, for the clock changes.
    fn in_zone(
        zone: chrono_tz::Tz,
        y: i32,
        m: u32,
        d: u32,
        h: u32,
        min: u32,
        sec: u32,
    ) -> DateTime<chrono_tz::Tz> {
        zone.with_ymd_and_hms(y, m, d, h, min, sec)
            .earliest()
            .expect("a real time")
    }

    #[test]
    fn a_schedule_set_in_an_old_conversation_counts_from_when_it_was_set() {
        // Counted from when the conversation began, a schedule saved at three
        // in the afternoon in a conversation a week old ran at once and said it
        // was late.
        let now = at("2026-09-28 15:00:00");
        let c = Conversation {
            started_at: at("2026-09-21 10:00:00").timestamp_millis(),
            routine_set_at: Some(now.timestamp_millis()),
            runs_at: Some("daily 07:00".into()),
            runs_what: Some("the briefing".into()),
            ..Default::default()
        };
        let next = When::read("daily 07:00")
            .unwrap()
            .next_after(counting_from(&c, now))
            .unwrap();
        assert_eq!(next, at("2026-09-29 07:00:00"));
        assert!(due_within(&[("Scout".to_string(), c)], &HashSet::new(), now, 15).is_none());
    }

    #[test]
    fn a_daily_routine_carries_on_through_both_clock_changes() {
        use chrono_tz::{America::New_York, Europe::Berlin};
        // New York goes back on 1 November 2026: 01:30 happens twice. It runs
        // once, at the first, and it is not lost for good.
        let when = When::read("daily 01:30").unwrap();
        let next = when
            .next_after_in(in_zone(New_York, 2026, 10, 31, 1, 30, 20))
            .expect("a next run");
        assert_eq!(next.naive_local().to_string(), "2026-11-01 01:30:00");
        let after = when
            .next_after_in(next + Duration::seconds(20))
            .expect("and one after that");
        assert_eq!(after.naive_local().to_string(), "2026-11-02 01:30:00");

        // Run at half past midnight that morning, the next one is the next day
        // and not the same date twenty-four hours on.
        let when = When::read("daily 00:30").unwrap();
        let next = when
            .next_after_in(in_zone(New_York, 2026, 11, 1, 0, 30, 20))
            .expect("a next run");
        assert_eq!(next.naive_local().to_string(), "2026-11-02 00:30:00");

        // Berlin goes forward on 29 March 2026: 02:30 does not happen. That
        // day is skipped and the routine carries on the day after.
        let when = When::read("daily 02:30").unwrap();
        let next = when
            .next_after_in(in_zone(Berlin, 2026, 3, 28, 2, 30, 20))
            .expect("a next run");
        assert_eq!(next.naive_local().to_string(), "2026-03-30 02:30:00");

        // And late in the evening of the change, the evening is not skipped.
        let when = When::read("daily 23:30").unwrap();
        let next = when
            .next_after_in(in_zone(Berlin, 2026, 3, 28, 23, 30, 20))
            .expect("a next run");
        assert_eq!(next.naive_local().to_string(), "2026-03-29 23:30:00");

        // Nor a weekly one on the day of the change.
        let when = When::read("weekly sun 23:30").unwrap();
        let next = when
            .next_after_in(in_zone(Berlin, 2026, 3, 22, 23, 30, 20))
            .expect("a next run");
        assert_eq!(next.naive_local().to_string(), "2026-03-29 23:30:00");
    }

    #[test]
    fn a_schedule_says_everything_it_means_or_is_refused() {
        // Leftover words were ignored and the text was stored as written, so
        // the panel read back what was asked while something else ran.
        assert!(
            When::read("every 30m 09:00-17:00").is_err(),
            "the hours were ignored"
        );
        assert!(
            When::read("daily 07:00 19:00").is_err(),
            "the second time was ignored"
        );
        // And the working week is five days.
        let working = When::read("weekly mon-fri 09:00").expect("a working week");
        assert_eq!(working.written(), "weekly mon,tue,wed,thu,fri 09:00");
        let round = When::read("weekly fri-mon 10:00").expect("round the end of the week");
        assert_eq!(round.written(), "weekly fri,sat,sun,mon 10:00");
        assert!(When::read("weekly monday,thursday 07:30").is_ok());
        assert!(
            When::read("weekly monkey 07:30").is_err(),
            "the first three letters of anything were a day"
        );
    }

    #[test]
    fn nothing_written_as_a_schedule_can_take_the_clock_down() {
        // Each of these panicked, the last on every tick after it was saved.
        assert!(When::read("every 1½").is_err());
        assert!(When::read("weekly mié 09:00").is_err());
        assert!(When::read("every 999999999d").is_err());
        let longest = When::read("every 366d").expect("a year and a day");
        assert!(longest.next_after(Local::now()).is_some());
    }

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
