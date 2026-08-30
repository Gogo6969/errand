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

use anyhow::{bail, Result};
use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, TimeZone, Timelike, Weekday};
use serde::{Deserialize, Serialize};

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
                    _ => bail!("`{span}` should end in m, h or d, like `every 30m`"),
                };
                // Below a minute it would run every time the scheduler looked,
                // which is a busy loop wearing a schedule.
                if minutes < 1 {
                    bail!("that is too often to be a routine");
                }
                Ok(When::Every { minutes })
            }
            _ => bail!("try `daily 07:00`, `weekly mon,fri 09:30` or `every 30m`"),
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

#[cfg(test)]
mod tests {
    use super::*;

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
