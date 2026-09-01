//! Whether something is worth exactly one more try, and how long to wait.
//!
//! This is the failure that ruins standing jobs and only standing jobs. A
//! briefing that meets a busy provider for ten seconds at seven in the morning
//! does not run late: it does not run, the conversation gets a red line nobody
//! is awake to read, and the next thing that happens is somebody asking at
//! lunchtime why there was no briefing. Sitting at the keyboard the same
//! failure costs a retyped sentence.
//!
//! One more try, not a loop. The clock's own guard against a hot retry is that
//! a routine is marked as having run before its turn is claimed, so a failure
//! does not immediately become another attempt; that ordering is deliberate and
//! this must not undo it. So the retry lives inside the one attempt rather than
//! around it: at most two requests where there was one, and then the failure is
//! reported the way it always was.
//!
//! What is worth trying again is narrow on purpose. A refused connection, a
//! server that is still loading a model, a 500 and a rate limit are all things
//! that are different a moment later. A bad key, an unknown model name and a
//! malformed request are not, and trying those again spends somebody's morning
//! arriving at the same answer twice.

use std::time::Duration;

/// The longest this will ever wait before trying again.
///
/// A provider asking for five minutes is asking for longer than anybody wants
/// a tool call to take, and longer than the clock's own window. Past this the
/// honest answer is to fail and say when it could be tried again.
const AT_MOST: Duration = Duration::from_secs(45);

/// How long to wait when nothing said otherwise.
///
/// Short, because the case this is for is a server that was busy for a moment
/// rather than one that is down. If it is down, one more try at three seconds
/// costs three seconds and says so.
const A_MOMENT: Duration = Duration::from_secs(3);

/// Whether this is worth one more attempt, and how long to leave it first.
///
/// Nothing when trying again would arrive at the same answer, which is most
/// errors. The whole value of this is being wrong rarely in that direction: a
/// second attempt at a bad key is a second identical failure and twice the
/// wait before somebody is told.
pub fn worth_another_go(why: &str) -> Option<Duration> {
    let lc = why.to_ascii_lowercase();

    // Said outright by the provider, and it wins over every guess below.
    // Anthropic and OpenAI both send `retry-after` with a rate limit, and a
    // number somebody has been given is better than one this file invented.
    if let Some(wait) = how_long_it_asked_for(&lc) {
        return Some(wait.min(AT_MOST));
    }

    // A rate limit is the case this exists for, and it is not in the
    // server-down list because a server answering 429 is not down at all.
    if lc.contains("error 429") || lc.contains("rate limit") || lc.contains("overloaded") {
        return Some(Duration::from_secs(20));
    }

    // A key, a model name or a request that is wrong is wrong again in three
    // seconds. Checked before the down list, because "invalid api key" from
    // some providers arrives wrapped in words that list matches.
    if nothing_will_change(&lc) {
        return None;
    }

    match crate::local::talk::is_server_down_error(why) {
        true => Some(A_MOMENT),
        false => None,
    }
}

/// What the server asked to be waited, where it asked for anything.
///
/// Read out of the error text, which is where it ends up: the request carries
/// the header through as words so that the decision can be made in one place
/// from one string rather than threaded through every transport.
fn how_long_it_asked_for(lc: &str) -> Option<Duration> {
    let at = lc.find("retry after ")? + "retry after ".len();
    let digits: String = lc[at..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    match digits.parse::<u64>() {
        Ok(secs) if secs > 0 => Some(Duration::from_secs(secs)),
        // "retry after 0" is the server saying now, which is a wait of nothing
        // rather than no wait at all: something still has to pause between two
        // requests or this is a spin.
        Ok(_) => Some(Duration::from_millis(500)),
        Err(_) => None,
    }
}

/// Errors that mean the same thing however many times they are asked.
fn nothing_will_change(lc: &str) -> bool {
    [
        "error 400",
        "error 401",
        "error 403",
        "error 404",
        "error 422",
        "invalid api key",
        "incorrect api key",
        "authentication",
        "not found",
        "unknown model",
        "does not exist",
        "context length",
        "too many tokens",
    ]
    .iter()
    .any(|n| lc.contains(n))
}

/// What to say about waiting, in the line somebody reads.
///
/// Said out loud rather than done quietly. A tool call that takes twenty
/// seconds longer than usual with nothing on screen is indistinguishable from
/// one that has hung, and somebody watching will press Stop.
pub fn in_plain_words(why: &str, waiting: Duration) -> String {
    let reason = match crate::local::talk::is_server_down_error(why) {
        true => crate::local::talk::short_server_down_reason(why),
        false => "busy",
    };
    let secs = waiting.as_secs().max(1);
    format!("The model server is {reason}. Trying once more in {secs}s.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_provider_that_is_briefly_busy_is_worth_one_more_try() {
        // The failure that ruins a briefing at seven in the morning, when
        // nobody is there to press anything.
        assert!(worth_another_go("LLM error 429: rate limit exceeded").is_some());
        assert!(worth_another_go("LLM error 503: service unavailable").is_some());
        assert!(worth_another_go("error sending request: connection refused").is_some());
        assert!(worth_another_go("the model server is loading model qwen3").is_some());
        assert!(worth_another_go("Overloaded").is_some());
    }

    #[test]
    fn a_key_that_is_wrong_is_wrong_again_three_seconds_later() {
        // Trying these again spends somebody's morning arriving at the same
        // answer twice, and delays the one line that would let them fix it.
        assert_eq!(worth_another_go("LLM error 401: invalid api key"), None);
        assert_eq!(worth_another_go("LLM error 404: unknown model gpt-9"), None);
        assert_eq!(worth_another_go("LLM error 400: bad request"), None);
        assert_eq!(
            worth_another_go("LLM error 400: context length exceeded"),
            None
        );
    }

    #[test]
    fn a_wrong_key_wrapped_in_words_that_sound_like_a_dead_server_is_still_a_wrong_key() {
        // Some providers answer a bad key with prose containing the word
        // "connect", which the server-down list matches. Checked in the right
        // order, this is refused; in the wrong order it is retried for ever.
        assert_eq!(
            worth_another_go("LLM error 401: authentication failed, could not connect your key"),
            None
        );
    }

    #[test]
    fn what_the_server_asked_to_be_waited_beats_anything_this_file_would_guess() {
        // A number somebody has been given is better than one invented here.
        assert_eq!(
            worth_another_go("LLM error 429 (retry after 12s): slow down"),
            Some(Duration::from_secs(12))
        );
        // And a provider asking for longer than anybody wants to wait is
        // capped rather than obeyed: past this the honest answer is to fail.
        assert_eq!(
            worth_another_go("LLM error 429 (retry after 600s): come back later"),
            Some(AT_MOST)
        );
        // Nought means now, which still has to be a pause rather than a spin.
        assert!(worth_another_go("LLM error 429 (retry after 0s): fine")
            .is_some_and(|d| d < Duration::from_secs(1)));
    }

    #[test]
    fn waiting_is_said_out_loud_rather_than_done_quietly() {
        // Twenty seconds of nothing on screen is indistinguishable from a
        // hang, and somebody watching one will press Stop.
        let said = in_plain_words("error sending request: connection refused", A_MOMENT);
        assert!(said.contains("unreachable"), "{said}");
        assert!(said.contains("3s"), "{said}");
        assert!(said.contains("once more"), "{said}");
    }
}
