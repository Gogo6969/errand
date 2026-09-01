//! The things on this Mac an agent can reach, once somebody says it may.
//!
//! Every connector anywhere else begins with an account and an OAuth screen:
//! sign in to Google, grant six scopes, keep a refresh token somewhere. On a
//! Mac, for the things people actually ask about -- their mail, their calendar,
//! their reminders -- none of that is necessary. The data is already here,
//! behind an interface the system has had for twenty years, and the only
//! permission needed is the one macOS asks for itself, once, out loud.
//!
//! So these connect to nothing. No account, no token, no server, nothing that
//! expires at three in the morning, and nothing of somebody's leaving this
//! machine to be connected in the first place.
//!
//! What that costs is honesty about the boundary. An agent set never to ask is
//! walled into its own folder, and these reach outside it on purpose: the app
//! runs them, not the walled engine. The wall is therefore not what protects
//! somebody's mail here. The switch is: nothing is connected until somebody
//! turns it on, one at a time, having read what it lets an agent see.
//!
//! ## What a real mailbox costs, and what that changed
//!
//! The first version of this asked Mail for `every message whose read status is
//! false` in every mailbox of every account, with no limit and no order. On the
//! Mac it was written for that is 256 mailboxes and 191,902 messages, and the
//! question "how many unread emails do I have" took eight minutes. It was asked
//! twice and took fourteen minutes between them, and the answer it eventually
//! gave was fifty, which was neither the number of unread messages nor the
//! number in any inbox: it was the cap, applied silently, on a walk through
//! archives going back to 2020.
//!
//! Three separate faults, and the measurements that settled each of them:
//!
//! | asked of Mail                              | on a 179,288-message mailbox |
//! |--------------------------------------------|------------------------------|
//! | `unread count of every mailbox` (in bulk)  | under a second               |
//! | `name of every mailbox` (in bulk)          | one second                   |
//! | `every message whose read status is false` | thirty-one seconds           |
//! | `read status of messages 1 thru 100`       | forty seconds                |
//!
//! So: ask for the counts first, because they are free and Mail keeps them
//! anyway; look only in the mailboxes that have anything, which here is about
//! ten of the 256; look in inboxes unless somebody asks for the rest, because
//! an inbox is what a person means by their unread mail; never walk by ordinal,
//! which is the slowest thing on the list; and stop at a deadline, saying where
//! it got to, because a partial answer with a sentence under it is worth more
//! than eight minutes of nothing.

use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::{json, Value};

/// One thing an agent can be let at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Connector {
    /// What it is called in the store and on the wire.
    pub id: &'static str,
    /// What somebody calls it.
    pub name: &'static str,
    /// What an agent can see through it, said plainly enough to decide on.
    pub sees: &'static str,
    /// The app it drives, so a connector can say why it is not answering.
    pub app: &'static str,
}

/// Everything Errand knows how to connect to.
///
/// Read-only, every one of them. Sending mail, moving an appointment or
/// messaging somebody are not harder to write; they are a different question,
/// and one nobody should be answered by a tool call they did not watch.
pub const KNOWN: &[Connector] = &[
    Connector {
        id: "mail",
        name: "Mail",
        sees: "Reads your mail: who wrote, when, the subject, and the first part \
               of the message. It never sends anything and never deletes anything.",
        app: "Mail",
    },
    Connector {
        id: "calendar",
        name: "Calendar",
        sees: "Reads what is in your calendars: what, when, where, and which \
               calendar. It never adds, moves or cancels anything.",
        app: "Calendar",
    },
];

/// How long an agent is kept waiting before it is given an answer anyway.
///
/// The backstop, for when one question to Mail blocks on its own for longer
/// than the script's own deadline can notice.
const PATIENCE: Duration = Duration::from_secs(50);

/// What every mailbox gets before any of them gets more.
///
/// Measured: the 12,143-message inbox on this machine answers in a second, and
/// the 179,288-message one beside it takes thirty-one seconds just to find the
/// unread ones, before a single property is read. There is no cheap way to know
/// which is which in advance -- counting the messages in a mailbox is itself
/// one of the slow questions -- so instead of guessing, every mailbox is asked
/// briefly first and the ones that did not answer are gone back to with
/// whatever time is left. Going in order without this means the big one spends
/// the whole budget and the small one, which would have answered instantly, is
/// never asked at all.
const A_QUICK_LOOK: Duration = Duration::from_secs(20);

/// The least worth giving a mailbox on the second pass. Below this it was not
/// going to finish anyway, and the time is better spent saying so.
const WORTH_ASKING: Duration = Duration::from_secs(10);

/// How long the script gives itself, so it can stop rather than be killed
/// holding what it found.
const PATIENCE_INSIDE: u64 = 16;

/// The most that will be fetched in one go, however much is asked for.
///
/// Said out loud in the answer whenever it bites, which is the whole difference
/// between a limit and a lie.
const NEVER_MORE_THAN: i64 = 200;

/// The connector this tool belongs to, and which job it is.
pub fn which(tool: &str) -> Option<&'static str> {
    let plain = tool
        .strip_prefix(&format!("mcp__{}__", crate::team::DOORWAY))
        .unwrap_or(tool);
    JOBS.iter().find(|j| *j == &plain).copied()
}

/// Every job these connectors offer, by the one name each is written under.
const JOBS: &[&str] = &["unread_mail", "search_mail", "what_is_on"];

/// Which connector has to be on for a job to answer.
pub fn needs(job: &str) -> &'static str {
    match job {
        "what_is_on" => "calendar",
        _ => "mail",
    }
}

/// What each job is doing, in words, for the line in the conversation.
pub fn in_plain_words(job: &str, args: &Value) -> String {
    let get = |k: &str| args.get(k).and_then(|v| v.as_str()).unwrap_or("");
    let everywhere = args
        .get("everywhere")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    match job {
        "unread_mail" => match everywhere {
            true => "Looking at your unread mail, everywhere".to_string(),
            false => "Looking at your unread mail".to_string(),
        },
        "search_mail" => match get("about") {
            "" => "Looking through your mail".to_string(),
            about => format!("Looking through your mail for {about}"),
        },
        _ => match get("when") {
            "" => "Looking at your calendar".to_string(),
            when => format!("Looking at your calendar for {when}"),
        },
    }
}

/// How these are declared to an engine.
///
/// Offered whether or not anything is connected, and answered with a sentence
/// saying so when it is not. An agent that cannot see the tool cannot tell
/// somebody the thing they asked for is one switch away.
pub fn declarations() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "unread_mail",
                "description":
                    "What is unread in the person's mail: who it is from, when it arrived, the \
                     and the subject, plus the first part of each if you ask for with_text. \
                     Looks in inboxes only unless you ask for everywhere. Always says how many it listed out of how many there are, and \
                     says so plainly if it stopped early. Reading only. Answers with a sentence \
                     saying so if Mail is not connected.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "at_most": {
                            "type": "number",
                            "description": "How many to read. Ten if you do not say, 200 at the very most."
                        },
                        "everywhere": {
                            "type": "boolean",
                            "description":
                                "Look in every mailbox, not just the inboxes. Slower, and it \
                                 turns up old unread post in archives. False if you do not say."
                        },
                        "with_text": {
                            "type": "boolean",
                            "description":
                                "Also read the first 300 characters of each message. Noticeably \
                                 slower, because it pulls the whole message across. Ask for it \
                                 when the subjects are not enough. False if you do not say."
                        }
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "search_mail",
                "description":
                    "Find messages in the person's mail by a word in the subject. Searches the \
                     inboxes first, then everything else until it runs out of time, and says how \
                     far it got. Reading only.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "about": {
                            "type": "string",
                            "description": "A word or a name to look for in the subject"
                        },
                        "at_most": { "type": "number", "description": "How many, 200 at the very most" }
                    },
                    "required": ["about"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "what_is_on",
                "description":
                    "What is in the person's calendars over a stretch of days: what, when, \
                     where and which calendar. Reading only.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "when": {
                            "type": "string",
                            "description": "`today`, `tomorrow`, or `7` for the next seven days"
                        }
                    }
                }
            }
        }),
    ]
}

/// Do one of these, having been told the connector is on.
///
/// Everything goes out through `osascript` and comes back as text. Nothing here
/// writes, so the worst a wrong argument can do is ask a question nobody
/// answers.
pub fn run(job: &str, args: &Value) -> Result<String> {
    let text = |k: &str| args.get(k).and_then(|v| v.as_str()).unwrap_or("").trim();
    let number = |k: &str, or: i64| args.get(k).and_then(|v| v.as_i64()).unwrap_or(or).max(1);
    let flag = |k: &str| args.get(k).and_then(|v| v.as_bool()).unwrap_or(false);

    match job {
        "unread_mail" => unread_mail(number("at_most", 10), flag("everywhere"), flag("with_text")),
        "search_mail" => match text("about") {
            "" => bail!("say a word or a name to look for"),
            about => search_mail(about, number("at_most", 10)),
        },
        "what_is_on" => what_is_on(text("when")),
        _ => bail!("there is no {job} here"),
    }
}

/// One mailbox, and how much of it is unread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Holding {
    /// The account it belongs to, as Mail names it.
    pub account: String,
    /// The mailbox, as Mail names it.
    pub mailbox: String,
    /// How many messages in it Mail already believes are unread.
    pub unread: i64,
}

impl Holding {
    /// How to say which one this is, in an answer somebody reads.
    fn said(&self) -> String {
        format!("{} / {}", self.account, self.mailbox)
    }

    /// Whether this is where mail arrives, rather than somewhere it was filed.
    ///
    /// An inbox is what a person means by their unread mail. Everything else is
    /// archives, and archives on a machine this old hold unread post from 2020,
    /// which answers a question nobody asked.
    fn is_an_inbox(&self) -> bool {
        self.mailbox.eq_ignore_ascii_case("inbox")
    }
}

/// Ask Mail what it already knows, without looking in anything.
///
/// `unread count` is a number Mail keeps as it goes, and asking for it across
/// every mailbox of an account is one question and under a second. Everything
/// slow here is avoided by asking this first.
pub fn the_script_for_where_the_unread_is() -> String {
    r#"set out to ""
tell application "Mail"
  repeat with acc in every account
    try
      set an to name of acc
      set ns to name of every mailbox of acc
      set us to unread count of every mailbox of acc
      repeat with i from 1 to (count of ns)
        if (item i of us) > 0 then
          set out to out & an & tab & (item i of ns) & tab & (item i of us) & linefeed
        end if
      end repeat
    end try
  end repeat
end tell
return out"#
        .to_string()
}

/// Read back what Mail said about where the unread is.
///
/// Split from both ends rather than through the middle: an account name has no
/// tab in it and neither does a number, but a mailbox somebody named themselves
/// might, and splitting left to right would then read half a name as a count.
pub fn where_it_is(said: &str) -> Vec<Holding> {
    said.lines()
        .filter_map(|line| {
            let (account, rest) = line.split_once('\t')?;
            let (mailbox, unread) = rest.rsplit_once('\t')?;
            Some(Holding {
                account: account.trim().to_string(),
                mailbox: mailbox.trim().to_string(),
                unread: unread.trim().parse().ok()?,
            })
        })
        .filter(|one| one.unread > 0)
        .collect()
}

/// Which of them are worth the time, and in what order.
///
/// Inboxes first and, unless somebody asked for everywhere, inboxes only. The
/// order is the point as much as the filter: whatever the deadline cuts off is
/// then the least interesting thing rather than whatever Mail happened to
/// mention last.
pub fn worth_looking_in(all: &[Holding], everywhere: bool) -> Vec<Holding> {
    let (inboxes, filed): (Vec<_>, Vec<_>) = all.iter().cloned().partition(|one| one.is_an_inbox());
    match everywhere {
        true => inboxes.into_iter().chain(filed).collect(),
        false => inboxes,
    }
}

/// The script that actually reads the messages, over the mailboxes chosen.
///
/// The mailboxes are named in it one by one rather than found again, so nothing
/// is walked twice, and it carries its own deadline so that running out of time
/// hands back what it has instead of being killed holding it.
///
/// Every property read here is a round trip to Mail and costs about half a
/// second, which is why the loop stops at what was asked for rather than
/// reading everything it found and trimming afterwards. Reading the sender,
/// subject and date of all 75 unread messages in one mailbox here took 148
/// seconds; reading them for ten takes fifteen.
///
/// Asking for the property of the whole list in one go looks like the obvious
/// repair and is not available: once `every message whose ...` has been put in
/// a variable it is a list of references, and Mail answers `sender of` a list
/// with an error rather than a list of senders.
fn read_them(one: &Holding, at_most: i64, with_text: bool) -> String {
    let mut script = String::from(
        "set out to \"\"\nset found to 0\nset stopped to \"\"\n\
         set cutoff to (current date) + ",
    );
    script.push_str(&PATIENCE_INSIDE.to_string());
    script.push_str("\ntell application \"Mail\"\n");
    {
        let acc = quoted(&one.account);
        let mb = quoted(&one.mailbox);
        let which = quoted(&one.said());
        // The body is asked for only when somebody wants it. It is the one
        // property that pulls a whole message across rather than a line of it,
        // and on a mailbox this size it is the difference between an answer and
        // a timeout: it is what made the first version of this take eight
        // minutes and come back with a number that was not true.
        let body = match with_text {
            true => {
                "        try
                           set t to (content of m)
                           if (length of t) > 300 then set t to (text 1 thru 300 of t)
                           set out to out & t & linefeed
                         end try
"
            }
            false => "",
        };
        script.push_str(&format!(
            r#"if found < {at_most} and stopped is "" then
  if (current date) > cutoff then
    set stopped to {which}
  else
    try
      repeat with m in (every message of mailbox {mb} of account {acc} whose read status is false)
        if found >= {at_most} then exit repeat
        set out to out & "From: " & (sender of m) & linefeed
        set out to out & "When: " & ((date received of m) as string) & linefeed
        set out to out & "Where: " & {which} & linefeed
        set out to out & "Subject: " & (subject of m) & linefeed
{body}        set out to out & "---" & linefeed
        set found to found + 1
      end repeat
    end try
  end if
end if
"#
        ));
    }
    script.push_str(
        "end tell\n\
         return out & \"((found \" & found & \"))\" & linefeed & \
         \"((stopped \" & stopped & \"))\"",
    );
    script
}

/// The reading script for one mailbox, for looking at when a live run goes odd.
pub fn the_script_for_reading(account: &str, mailbox: &str, at_most: i64) -> String {
    read_them(
        &Holding {
            account: account.to_string(),
            mailbox: mailbox.to_string(),
            unread: 1,
        },
        at_most,
        false,
    )
}

/// What came back, pulled apart from the two markers on the end of it.
///
/// Returned as it is when the markers are missing, because a connector that
/// swallows an answer it did not expect is worse than one that hands it over.
pub fn what_it_read(said: &str) -> (String, i64, Option<String>) {
    let mut body = Vec::new();
    let mut found = 0;
    let mut stopped = None;
    for line in said.lines() {
        if let Some(rest) = line
            .strip_prefix("((found ")
            .and_then(|r| r.strip_suffix("))"))
        {
            found = rest.trim().parse().unwrap_or(0);
        } else if let Some(rest) = line
            .strip_prefix("((stopped ")
            .and_then(|r| r.strip_suffix("))"))
        {
            let rest = rest.trim();
            stopped = (!rest.is_empty()).then(|| rest.to_string());
        } else {
            body.push(line);
        }
    }
    (body.join("\n").trim().to_string(), found, stopped)
}

/// The sentence under the messages, saying what was and was not looked at.
///
/// This is the whole repair of the original fault. A list of fifty with nothing
/// under it cannot be told apart from a mailbox that holds exactly fifty, and
/// an agent given it will say "fifty unread" and be wrong. So every number that
/// shaped the list is named: how many were read, where they came from, what
/// stopped it, and what Mail believes is unread somewhere it did not look.
///
/// What is deliberately not claimed is that the two numbers agree. Mail's own
/// unread count for one inbox here is eight, and walking that same inbox for
/// messages whose read status is false turns up seventy-five. Which of those is
/// "how many unread emails do I have" is Mail's business and not this app's, so
/// the count is used to decide where to look and the walk is used to say what
/// was found, and neither is quietly presented as the other.
pub fn how_it_went(
    found: i64,
    stopped: Option<&str>,
    chosen: &[Holding],
    all: &[Holding],
    at_most: i64,
    everywhere: bool,
) -> String {
    let mut said = match found {
        // Nothing read is two different answers, and telling them apart is the
        // whole job. Either there was nothing there, or there was something
        // there and it could not be got at, and the second one has to hand over
        // Mail's own numbers rather than a word that reads like the first.
        0 => match chosen.is_empty() {
            true => match everywhere {
                true => "Nothing unread anywhere in Mail.".to_string(),
                false => "Nothing unread in any inbox.".to_string(),
            },
            false => format!("{} None of them could be read.", the_counts(chosen)),
        },
        1 => "One unread message".to_string(),
        _ => format!("{found} unread messages"),
    };
    if found > 0 {
        let places: Vec<String> = chosen.iter().map(|one| one.said()).collect();
        said.push_str(&match places.len() {
            1 => format!(", in {}.", places[0]),
            _ => format!(", across {}.", places.join(", ")),
        });
    }

    if let Some(where_it_got_to) = stopped {
        said.push_str(&format!(
            " {where_it_got_to} did not answer in time and was left unread, so there are more \
             than this. It is a large mailbox; asking again may get through it."
        ));
    } else if found >= at_most {
        said.push_str(&format!(
            " It stopped at the {at_most} asked for, so there are more. Ask for more with \
             at_most, up to {NEVER_MORE_THAN}."
        ));
    }

    said.push_str(&elsewhere(chosen, all, everywhere));
    said
}

/// Mail's own count for the mailboxes that were looked in.
///
/// Said as Mail's number rather than as the number, because the two do not
/// always agree: the unread count for one inbox here is eight, and walking that
/// same inbox for messages whose read status is false turns up seventy-five.
/// Which of those somebody means is Mail's business, so both are attributed.
fn the_counts(chosen: &[Holding]) -> String {
    let here: i64 = chosen.iter().map(|one| one.unread).sum();
    let places: Vec<String> = chosen
        .iter()
        .map(|one| format!("{} in {}", one.unread, one.said()))
        .collect();
    match here {
        0 => "Mail counts nothing unread where it looked.".to_string(),
        _ => format!("Mail counts {here} unread: {}.", places.join(", ")),
    }
}

/// What is unread somewhere this did not look, from Mail's own counts.
///
/// Said whether or not anything was found, because "nothing unread" told to
/// somebody with nine hundred unread messages one folder over is believed, and
/// is wrong.
fn elsewhere(chosen: &[Holding], all: &[Holding], everywhere: bool) -> String {
    let unlooked: Vec<&Holding> = all
        .iter()
        .filter(|one| !chosen.iter().any(|c| c.said() == one.said()))
        .collect();
    let count: i64 = unlooked.iter().map(|one| one.unread).sum();
    if count == 0 {
        return String::new();
    }
    let boxes = unlooked.len();
    match everywhere {
        true => {
            format!(" Mail counts another {count} unread in {boxes} mailboxes it did not reach.")
        }
        false => format!(
            " Mail counts another {count} unread outside the inboxes, in {boxes} mailboxes such \
             as archives and junk. Ask again with everywhere set to true to include them."
        ),
    }
}

/// What can still be said when the messages themselves could not be read.
///
/// The counts come back in under a second and the messages can take half a
/// minute, so the two have to be able to fail apart. Losing the counts to a
/// slow read would throw away the part that actually answers "how many unread
/// emails do I have", which is the question that started all this.
pub fn the_counts_alone(
    chosen: &[Holding],
    all: &[Holding],
    everywhere: bool,
    why: &str,
) -> String {
    let mut said = the_counts(chosen);
    said.push_str(&format!(
        " The messages themselves could not be read: {why} So this is Mail's own count rather \
         than a list of what is in them."
    ));
    said.push_str(&elsewhere(chosen, all, everywhere));
    said
}

/// What is unread, without walking a mailbox that has nothing in it.
///
/// One cheap question decides everything: which mailboxes have anything unread
/// at all. Then every one of those is given a quick look, and only the ones
/// that did not answer in that time are gone back to. Whatever happens, Mail's
/// own counts come back, because they are the answer to the question that is
/// actually being asked most of the time.
fn unread_mail(asked_for: i64, everywhere: bool, with_text: bool) -> Result<String> {
    let at_most = asked_for.min(NEVER_MORE_THAN);
    // Free, and it decides everything after it. Given its own short patience
    // because if this is slow then Mail is not answering at all.
    let all = where_it_is(&ask_the_mac(
        "Mail",
        &the_script_for_where_the_unread_is(),
        Duration::from_secs(20),
    )?);
    let chosen = worth_looking_in(&all, everywhere);

    if chosen.is_empty() {
        return Ok(how_it_went(0, None, &chosen, &all, at_most, everywhere));
    }

    let began = Instant::now();
    let mut body = String::new();
    let mut found = 0;
    let mut slow: Vec<&Holding> = Vec::new();

    // Everybody gets a quick look first.
    for one in &chosen {
        if found >= at_most {
            break;
        }
        match read_one(one, at_most - found, with_text, A_QUICK_LOOK) {
            // It answered, but said itself that it did not get to the end.
            Some((more, got, Some(_))) => {
                push(&mut body, &more);
                found += got;
                slow.push(one);
            }
            Some((more, got, None)) => {
                push(&mut body, &more);
                found += got;
            }
            None => slow.push(one),
        }
    }

    // Then the ones that did not answer, but only if the quick look turned up
    // nothing at all. When something was found, waiting another half minute for
    // a mailbox that has already shown it will not answer buys an agent very
    // little and costs whoever asked the question a great deal: the mailbox
    // that would not answer is named below, and asking again is one sentence.
    let mut stopped = slow.first().map(|one| one.said());
    let worth_the_wait = found == 0;
    for (gone, one) in slow.iter().enumerate() {
        if found >= at_most || !worth_the_wait {
            break;
        }
        let left = PATIENCE.saturating_sub(began.elapsed());
        let share = left / (slow.len() - gone) as u32;
        if share < WORTH_ASKING {
            stopped = Some(one.said());
            break;
        }
        match read_one(one, at_most - found, with_text, share) {
            Some((more, got, gave_up)) => {
                push(&mut body, &more);
                found += got;
                stopped = match gave_up {
                    Some(_) => Some(one.said()),
                    None => slow.get(gone + 1).map(|next| next.said()),
                };
            }
            None => stopped = Some(one.said()),
        }
    }

    let under = how_it_went(
        found,
        stopped.as_deref(),
        &chosen,
        &all,
        at_most,
        everywhere,
    );
    let body = body.trim().to_string();
    Ok(match body.is_empty() {
        true => under,
        false => format!("{body}\n\n{under}"),
    })
}

/// One mailbox, within the time it was given. Nothing when it would not answer.
///
/// A mailbox that will not answer is not an error: the others still have
/// something to say, and which ones went unread is said in the sentence at the
/// bottom rather than thrown as a failure that loses all of them.
fn read_one(
    one: &Holding,
    at_most: i64,
    with_text: bool,
    patience: Duration,
) -> Option<(String, i64, Option<String>)> {
    // The third of these is the script's own note that it gave up rather than
    // finished. Dropping it was a quiet way of reporting a partial answer as a
    // whole one, which is the fault this whole file exists to stop.
    ask_one("Mail", &read_them(one, at_most, with_text), patience)
}

/// Add to what has been read so far, keeping the blank line between messages.
fn push(body: &mut String, more: &str) {
    if more.is_empty() {
        return;
    }
    if !body.is_empty() {
        body.push('\n');
    }
    body.push_str(more);
}

/// Every mailbox there is, in the order worth searching them.
fn every_mailbox_script() -> String {
    // No `try` around the whole thing. Swallowing the error here is what turns
    // "Mail would not answer" into "you have no mailboxes", which is a sentence
    // an agent will repeat to somebody with three accounts.
    r#"set out to ""
tell application "Mail"
  repeat with acc in every account
    set an to name of acc
    set ns to name of every mailbox of acc
    repeat with i from 1 to (count of ns)
      set out to out & an & tab & (item i of ns) & tab & "1" & linefeed
    end repeat
  end repeat
end tell
return out"#
        .to_string()
}

/// The script that looks for a word in the subjects, over the mailboxes given.
///
/// Stops at what was asked for the same way, and for the same reason.
fn look_for(chosen: &[Holding], about: &str, at_most: i64) -> String {
    let mut script =
        String::from("set out to \"\"\nset found to 0\nset stopped to \"\"\nset needle to ");
    script.push_str(&quoted(about));
    script.push_str("\nset cutoff to (current date) + ");
    script.push_str(&PATIENCE_INSIDE.to_string());
    script.push_str("\ntell application \"Mail\"\n");
    for one in chosen {
        let acc = quoted(&one.account);
        let mb = quoted(&one.mailbox);
        let which = quoted(&one.said());
        script.push_str(&format!(
            r#"if found < {at_most} and stopped is "" then
  if (current date) > cutoff then
    set stopped to {which}
  else
    try
      repeat with m in (every message of mailbox {mb} of account {acc} whose subject contains needle)
        if found >= {at_most} then exit repeat
        set out to out & "From: " & (sender of m) & linefeed
        set out to out & "When: " & ((date received of m) as string) & linefeed
        set out to out & "Where: " & {which} & linefeed
        set out to out & "Subject: " & (subject of m) & linefeed & "---" & linefeed
        set found to found + 1
      end repeat
    end try
  end if
end if
"#
        ));
    }
    script.push_str(
        "end tell\n\
         return out & \"((found \" & found & \"))\" & linefeed & \
         \"((stopped \" & stopped & \"))\"",
    );
    script
}

/// What a search found, and how much of the post it got through.
///
/// Nothing here can be known in advance the way an unread count can, so the
/// honest thing is different: say where it looked and say where it stopped, so
/// that "nothing found" and "nothing found yet" are told apart.
pub fn how_the_search_went(
    found: i64,
    stopped: Option<&str>,
    looked_in: usize,
    at_most: i64,
) -> String {
    let mut said = match found {
        0 => format!("Nothing in the subjects of {looked_in} mailboxes matches that."),
        1 => "One message matches.".to_string(),
        _ => format!("{found} messages match."),
    };
    if let Some(where_it_got_to) = stopped {
        said.push_str(&format!(
            " It searched {looked_in} mailboxes and stopped after {PATIENCE_INSIDE} seconds, part \
             way through {where_it_got_to}, so there may be more."
        ));
    } else if found >= at_most {
        said.push_str(&format!(
            " It stopped at the {at_most} asked for, so there may be more."
        ));
    }
    said
}

/// Look for a word in the subjects, inboxes first.
fn search_mail(about: &str, asked_for: i64) -> Result<String> {
    let at_most = asked_for.min(NEVER_MORE_THAN);
    let every = where_it_is(&ask_the_mac(
        "Mail",
        &every_mailbox_script(),
        Duration::from_secs(20),
    )?);
    let chosen = worth_looking_in(&every, true);
    if chosen.is_empty() {
        bail!(
            "Mail listed no mailboxes to search. If there are accounts in Mail, it was busy \
             rather than empty, and asking again in a moment should work."
        );
    }

    let said = ask_the_mac("Mail", &look_for(&chosen, about, at_most), PATIENCE)?;
    let (body, found, stopped) = what_it_read(&said);
    let under = how_the_search_went(found, stopped.as_deref(), chosen.len(), at_most);
    Ok(match body.is_empty() {
        true => under,
        false => format!("{body}\n\n{under}"),
    })
}

/// How many days a stretch covers, from the word for it.
fn days(when: &str) -> i64 {
    match when.trim().to_lowercase().as_str() {
        "" | "today" => 1,
        "tomorrow" => 2,
        "week" | "this week" => 7,
        other => other.parse::<i64>().unwrap_or(1).clamp(1, 31),
    }
}

/// Every calendar there is, which Calendar answers instantly.
fn every_calendar_script() -> String {
    r#"set out to ""
tell application "Calendar"
  set ns to name of every calendar
  repeat with i from 1 to (count of ns)
    set out to out & (item i of ns) & linefeed
  end repeat
end tell
return out"#
        .to_string()
}

/// Ask Calendar what is on, over the calendars named.
///
/// Measured the same way the mail side was, and it needed the same repair. One
/// calendar on this machine answers in a second and most in two, but asking all
/// twenty-five in one breath took four minutes and forty-three seconds, and a
/// deadline checked between them does not help because at least one of them
/// blocks for longer than the whole budget on its own.
///
/// So they are asked in handfuls, and a handful that does not come back is
/// tried again one at a time. That way a single bad calendar costs itself and
/// not the four next to it, and the common case is still five questions rather
/// than twenty-five.
fn what_is_on_script(over: i64, calendars: &[String]) -> String {
    let mut script = format!(
        "set out to \"\"\nset found to 0\nset stopped to \"\"\n\
         set cutoff to (current date) + {PATIENCE_INSIDE}\n\
         set the_start to (current date) - (time of (current date))\n\
         set the_end to the_start + ({over} * days)\n\
         tell application \"Calendar\"\n"
    );
    for name in calendars {
        let it = quoted(name);
        script.push_str(&format!(
            r#"if stopped is "" then
  if (current date) > cutoff then
    set stopped to {it}
  else
    try
      repeat with e in (every event of calendar {it} whose start date is greater than or equal to the_start and start date is less than the_end)
        set out to out & (summary of e) & linefeed
        set out to out & "  " & ((start date of e) as string) & linefeed
        try
          if (location of e) is not missing value and (location of e) is not "" then
            set out to out & "  at " & (location of e) & linefeed
          end if
        end try
        set out to out & "  in " & {it} & linefeed
        set found to found + 1
      end repeat
    end try
  end if
end if
"#
        ));
    }
    script.push_str(
        "end tell\n\
         return out & \"((found \" & found & \"))\" & linefeed & \
         \"((stopped \" & stopped & \"))\"",
    );
    script
}

/// How many calendars are asked for at once.
///
/// Small enough that one that will not answer costs only a handful of others a
/// retry, large enough that twenty-five calendars are five questions and not
/// twenty-five processes.
const A_HANDFUL: usize = 5;

/// How long a handful of calendars gets before they are tried one at a time.
const EACH_HANDFUL: Duration = Duration::from_secs(12);

/// And how long one calendar gets on its own, having already been slow once.
const EACH_CALENDAR: Duration = Duration::from_secs(4);

/// What is on, and how much of the diary was actually looked at.
pub fn how_the_diary_went(found: i64, missed: &[String], over: i64) -> String {
    let stretch = match over {
        1 => "today".to_string(),
        2 => "today and tomorrow".to_string(),
        _ => format!("the next {over} days"),
    };
    let mut said = match found {
        0 => format!("Nothing in {stretch}."),
        1 => format!("One thing in {stretch}."),
        _ => format!("{found} things in {stretch}."),
    };
    if !missed.is_empty() {
        // Named, not counted. "Nothing on Thursday" read off a look that never
        // reached the work calendar is the way somebody misses a meeting.
        said.push_str(&format!(
            " These calendars would not answer in time and were not looked at, so there may be \
             more: {}.",
            missed.join(", ")
        ));
    }
    said
}

/// Everything in the diary over a stretch of days.
fn what_is_on(when: &str) -> Result<String> {
    let over = days(when);
    let calendars: Vec<String> = ask_the_mac(
        "Calendar",
        &every_calendar_script(),
        Duration::from_secs(20),
    )?
    .lines()
    .map(|line| line.trim().to_string())
    .filter(|line| !line.is_empty())
    .collect();
    if calendars.is_empty() {
        bail!("Calendar listed no calendars, which usually means it was busy rather than empty");
    }

    let began = Instant::now();
    let mut body = String::new();
    let mut found = 0;
    let mut missed: Vec<String> = Vec::new();

    for handful in calendars.chunks(A_HANDFUL) {
        let left = PATIENCE.saturating_sub(began.elapsed());
        if left < EACH_CALENDAR {
            missed.extend(handful.iter().cloned());
            continue;
        }
        match ask_one(
            "Calendar",
            &what_is_on_script(over, handful),
            left.min(EACH_HANDFUL),
        ) {
            Some((more, got, gave_up)) => {
                push(&mut body, &more);
                found += got;
                // It answered, but said it stopped part way through. Everything
                // from there on in this handful went unlooked at.
                if let Some(at) = gave_up {
                    let from = handful.iter().position(|c| *c == at).unwrap_or(0);
                    missed.extend(handful[from..].iter().cloned());
                }
            }
            // The handful would not come back at all, so try them singly: it is
            // usually one calendar spoiling it for the others.
            None => {
                for one in handful {
                    let left = PATIENCE.saturating_sub(began.elapsed());
                    if left < EACH_CALENDAR {
                        missed.push(one.clone());
                        continue;
                    }
                    match ask_one(
                        "Calendar",
                        &what_is_on_script(over, std::slice::from_ref(one)),
                        EACH_CALENDAR,
                    ) {
                        Some((more, got, None)) => {
                            push(&mut body, &more);
                            found += got;
                        }
                        _ => missed.push(one.clone()),
                    }
                }
            }
        }
    }

    let under = how_the_diary_went(found, &missed, over);
    let body = body.trim().to_string();
    Ok(match body.is_empty() {
        true => under,
        false => format!("{body}\n\n{under}"),
    })
}

/// One question, within the time it was given. Nothing when it would not answer.
fn ask_one(app: &str, script: &str, patience: Duration) -> Option<(String, i64, Option<String>)> {
    Some(what_it_read(&ask_the_mac(app, script, patience).ok()?))
}

/// A string AppleScript will read as one string, whatever is in it.
fn quoted(said: &str) -> String {
    format!("\"{}\"", said.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Run one script and bring back what it said, or give up out loud.
///
/// The pipes are drained on threads of their own from the moment the process
/// starts. Waiting on the process while its output sits unread in a pipe is a
/// deadlock as soon as an answer grows past the buffer, and an answer here is
/// somebody's mail, which grows.
///
/// The first call to each of these makes macOS ask whether Errand may drive
/// that app, in its own dialog, in front of whoever is there. A refusal comes
/// back here as an error, and is said as a sentence rather than as a number.
fn ask_the_mac(app: &str, script: &str, patience: Duration) -> Result<String> {
    let mut child = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut out = child.stdout.take().expect("asked for a pipe");
    let mut err = child.stderr.take().expect("asked for a pipe");
    let reading = thread::spawn(move || {
        let mut got = String::new();
        let _ = out.read_to_string(&mut got);
        got
    });
    let complaining = thread::spawn(move || {
        let mut got = String::new();
        let _ = err.read_to_string(&mut got);
        got
    });

    let began = Instant::now();
    let ended = loop {
        match child.try_wait()? {
            Some(status) => break Some(status),
            None if began.elapsed() >= patience => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            None => thread::sleep(Duration::from_millis(40)),
        }
    };

    let said = reading.join().unwrap_or_default().trim().to_string();
    let why = complaining.join().unwrap_or_default();

    let Some(status) = ended else {
        bail!(
            "{app} did not answer within {} seconds, so it was left alone.",
            patience.as_secs()
        );
    };
    if status.success() {
        return Ok(match said.is_empty() {
            true => "Nothing came back.".to_string(),
            false => said,
        });
    }
    if why.contains("-1743") || why.to_lowercase().contains("not authorized") {
        bail!(
            "macOS has not been told Errand may do this. It asks once, in a dialog of its own: \
             say yes there, or turn it on under Privacy and Security, Automation."
        );
    }
    bail!("{}", why.trim());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holding(account: &str, mailbox: &str, unread: i64) -> Holding {
        Holding {
            account: account.to_string(),
            mailbox: mailbox.to_string(),
            unread,
        }
    }

    #[test]
    fn every_job_offered_is_a_job_that_can_be_asked_for() {
        // A tool declared and not handled is offered, called, and answered with
        // "there is no such thing here", which reads as the app being broken
        // rather than as a tool that was never wired up.
        let declared = declarations();
        let named: Vec<&str> = declared
            .iter()
            .filter_map(|d| d.pointer("/function/name")?.as_str())
            .collect();
        assert_eq!(named, JOBS);
        for job in JOBS {
            assert!(which(job).is_some(), "{job} is not recognised");
            assert!(
                KNOWN.iter().any(|c| c.id == needs(job)),
                "{job} needs a connector that does not exist"
            );
        }
    }

    #[test]
    fn a_tool_that_is_not_one_of_these_is_not_claimed() {
        // Claiming somebody else's tool would answer it here instead of calling
        // it, which is the quiet kind of broken.
        assert_eq!(which("Bash"), None);
        assert_eq!(which("mcp__something_else__search_mail"), None);
        // And the same job is the same job whichever engine named it.
        assert_eq!(which("mcp__errand__search_mail"), which("search_mail"));
    }

    #[test]
    fn a_word_with_a_quote_in_it_does_not_end_the_script_early() {
        // A subject with a quotation mark in it is ordinary, and unescaped it
        // ends the string and turns the rest of the search into syntax.
        let script = look_for(&[holding("me", "INBOX", 1)], "the \"big\" one", 5);
        assert!(script.contains("\\\"big\\\""), "{script}");
    }

    #[test]
    fn a_mailbox_somebody_named_themselves_cannot_break_the_script_either() {
        // Mailboxes are named by hand, and a quotation mark in one would
        // otherwise close the string and leave the rest as syntax.
        let script = read_them(&holding("me", "Bills \"old\"", 3), 5, false);
        assert!(script.contains("\\\"old\\\""), "{script}");
    }

    #[test]
    fn a_stretch_of_days_is_read_the_way_somebody_would_say_it() {
        assert_eq!(days(""), 1);
        assert_eq!(days("today"), 1);
        assert_eq!(days("Tomorrow"), 2);
        assert_eq!(days("this week"), 7);
        assert_eq!(days("14"), 14);
        // Nonsense is today rather than an error: the question was about a
        // calendar, and answering today is a better reply than refusing.
        assert_eq!(days("whenever"), 1);
        // And nothing asks for a year of somebody's diary in one go.
        assert_eq!(days("400"), 31);
    }

    #[test]
    fn what_each_connector_lets_an_agent_see_is_said_before_it_is_turned_on() {
        // The switch is what protects somebody's mail here, not the wall: these
        // are run by the app rather than by the walled engine. So the sentence
        // beside the switch has to be enough to decide on.
        for one in KNOWN {
            assert!(one.sees.len() > 60, "{}: {}", one.id, one.sees);
            assert!(
                one.sees.contains("never"),
                "{} does not say what it will not do: {}",
                one.id,
                one.sees
            );
        }
    }

    #[test]
    fn a_mailbox_with_a_tab_in_its_name_is_still_read_back_correctly() {
        // Splitting left to right would read the first half of such a name as
        // the mailbox and the second half as a count, and the count would then
        // be nothing, and the mailbox would be quietly dropped.
        let said = "iCloud\tOdd\tName\t4\nwork\tINBOX\t9\n";
        let got = where_it_is(said);
        assert_eq!(got.len(), 2, "{got:?}");
        assert_eq!(got[0], holding("iCloud", "Odd\tName", 4));
        assert_eq!(got[1], holding("work", "INBOX", 9));
    }

    #[test]
    fn only_the_inboxes_are_looked_in_unless_somebody_asks_for_the_rest() {
        // An archive on a machine this old holds unread post from 2020, and
        // walking it answers a question nobody asked, slowly.
        let all = vec![
            holding("work", "Archive", 41),
            holding("work", "INBOX", 8),
            holding("home", "INBOX", 2),
            holding("home", "Junk", 900),
        ];
        let just_inboxes = worth_looking_in(&all, false);
        assert_eq!(just_inboxes.len(), 2);
        assert!(
            just_inboxes.iter().all(|one| one.is_an_inbox()),
            "{just_inboxes:?}"
        );

        // And when the rest are wanted, the inboxes still come first, so that
        // whatever the deadline cuts off is the least interesting thing.
        let the_lot = worth_looking_in(&all, true);
        assert_eq!(the_lot.len(), 4);
        assert!(
            the_lot[0].is_an_inbox() && the_lot[1].is_an_inbox(),
            "{the_lot:?}"
        );
    }

    #[test]
    fn a_mailbox_with_nothing_unread_in_it_is_never_opened() {
        // The whole speed-up. Asking Mail for a count is free; opening a
        // mailbox to find out there was nothing in it costs thirty seconds.
        let said = "work\tINBOX\t0\nwork\tArchive\t3\n";
        let got = where_it_is(said);
        assert_eq!(got, vec![holding("work", "Archive", 3)]);
    }

    #[test]
    fn asking_for_more_than_will_be_fetched_is_said_out_loud() {
        // The original fault. Fifty came back for at_most 50 and for at_most
        // 500, identically, and nothing in the answer said which of those was
        // the truth, so the only honest thing left to say was "I cannot tell".
        let chosen = vec![holding("work", "INBOX", 137)];
        let said = how_it_went(10, None, &chosen, &chosen, 10, false);
        assert!(said.contains("10 unread messages"), "{said}");
        assert!(said.contains("stopped at the 10 asked for"), "{said}");
        assert!(said.contains("at_most"), "{said}");
        assert!(said.contains(&NEVER_MORE_THAN.to_string()), "{said}");
    }

    #[test]
    fn a_list_that_is_all_there_is_does_not_pretend_there_is_more() {
        let chosen = vec![holding("work", "INBOX", 8)];
        let said = how_it_went(8, None, &chosen, &chosen, 10, false);
        assert!(said.contains("8 unread messages"), "{said}");
        assert!(!said.contains("at_most"), "{said}");
        assert!(said.contains("work / INBOX"), "{said}");
    }

    #[test]
    fn an_answer_that_ran_out_of_time_says_where_it_got_to() {
        // Eight minutes of nothing is worse than twenty-five seconds and a
        // sentence, and the sentence is what stops the number being believed.
        let chosen = vec![holding("work", "INBOX", 137), holding("home", "INBOX", 2)];
        let said = how_it_went(4, Some("work / INBOX"), &chosen, &chosen, 10, false);
        assert!(said.contains("did not answer in time"), "{said}");
        assert!(said.contains("work / INBOX"), "{said}");
        assert!(said.contains("there are more"), "{said}");
    }

    #[test]
    fn an_empty_inbox_still_says_what_is_unread_everywhere_else() {
        // Otherwise "nothing unread" is said to somebody with nine hundred
        // unread messages one folder over, and they believe it.
        let all = vec![holding("work", "Archive", 41), holding("home", "Junk", 900)];
        let chosen = worth_looking_in(&all, false);
        assert!(chosen.is_empty());
        let said = how_it_went(0, None, &chosen, &all, 10, false);
        assert!(said.contains("Nothing unread in any inbox"), "{said}");
        assert!(said.contains("941"), "{said}");
        assert!(said.contains("everywhere"), "{said}");
    }

    #[test]
    fn the_markers_on_the_end_of_an_answer_are_read_off_and_not_shown() {
        // They are how the script says what it did. Leaving them in the body
        // would put "((found 2))" in front of somebody as though it were mail.
        let said = "From: a\nSubject: b\n---\n((found 2))\n((stopped work / INBOX))";
        let (body, found, stopped) = what_it_read(said);
        assert_eq!(body, "From: a\nSubject: b\n---");
        assert_eq!(found, 2);
        assert_eq!(stopped.as_deref(), Some("work / INBOX"));
    }

    #[test]
    fn an_answer_that_did_not_stop_early_says_nothing_about_stopping() {
        let (_, found, stopped) = what_it_read("x\n((found 5))\n((stopped ))");
        assert_eq!(found, 5);
        assert_eq!(stopped, None);
    }

    #[test]
    fn a_diary_that_ran_out_of_time_says_which_calendar_it_reached() {
        // Twenty-five calendars asked in one breath took four minutes and
        // forty-three seconds here. Cut short, the danger is that "nothing on
        // Thursday" is read as an empty day rather than as an unfinished look.
        let all_of_it = how_the_diary_went(3, &[], 7);
        assert!(
            all_of_it.contains("3 things in the next 7 days"),
            "{all_of_it}"
        );
        assert!(!all_of_it.contains("may be more"), "{all_of_it}");

        // Named rather than counted: "nothing on Thursday", read off a look
        // that never reached the work calendar, is how somebody misses a
        // meeting.
        let cut_short = how_the_diary_went(0, &["Work".to_string(), "Büro KS".to_string()], 1);
        assert!(cut_short.contains("Nothing in today"), "{cut_short}");
        assert!(cut_short.contains("Work, Büro KS"), "{cut_short}");
        assert!(cut_short.contains("may be more"), "{cut_short}");
    }

    #[test]
    fn the_diary_carries_the_same_deadline_the_mail_does() {
        let script = what_is_on_script(7, &["Home".to_string(), "Work".to_string()]);
        assert!(
            script.contains(&format!("set cutoff to (current date) + {PATIENCE_INSIDE}")),
            "{script}"
        );
        assert!(script.contains("((found "), "{script}");
        assert!(script.contains("((stopped "), "{script}");
    }

    #[test]
    fn a_search_that_found_nothing_says_how_much_of_the_post_it_read() {
        // "Nothing found" and "nothing found yet" are different answers, and
        // only one of them means the thing is not there.
        let all_of_it = how_the_search_went(0, None, 256, 10);
        assert!(all_of_it.contains("256 mailboxes"), "{all_of_it}");
        assert!(!all_of_it.contains("may be more"), "{all_of_it}");

        let cut_short = how_the_search_went(0, Some("work / Archive"), 256, 10);
        assert!(cut_short.contains("may be more"), "{cut_short}");
        assert!(cut_short.contains("work / Archive"), "{cut_short}");
    }

    #[test]
    fn the_script_names_every_chosen_mailbox_and_no_others() {
        // Nothing is walked twice: the mailboxes were found by the cheap
        // question, and the expensive one is told exactly where to look.
        let script = read_them(&holding("work", "INBOX", 8), 10, false);
        assert!(
            script.contains("mailbox \"INBOX\" of account \"work\""),
            "{script}"
        );
        assert_eq!(
            script.matches("whose read status is false").count(),
            1,
            "{script}"
        );
        // And it carries the deadline that lets it hand back what it has.
        assert!(
            script.contains(&format!("set cutoff to (current date) + {PATIENCE_INSIDE}")),
            "{script}"
        );
    }

    #[test]
    fn the_text_of_a_message_is_only_fetched_when_somebody_asks_for_it() {
        // It is the one property that pulls a whole message across instead of a
        // line of it, and it is what turned this into an eight minute wait.
        assert!(!read_them(&holding("work", "INBOX", 8), 10, false).contains("content of m"));
        assert!(read_them(&holding("work", "INBOX", 8), 10, true).contains("content of m"));
    }
}
