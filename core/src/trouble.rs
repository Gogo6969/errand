//! What a failure actually means, and what to do about it.
//!
//! Engines report what their provider told them, which is the right thing to
//! keep and the wrong thing to show. "Failed to authenticate. API Error: 401
//! OAuth access token has been revoked" is a complete and accurate description
//! of what happened, and it does not say the one thing somebody needs, which is
//! that their Claude Code login has expired and they can fix it in a terminal
//! in ten seconds.
//!
//! It is worse than unhelpful when it arrives late. The failure that prompted
//! this came after somebody had typed a paragraph, attached a screenshot of an
//! order confirmation, and asked for a daily errand: everything they had done
//! was already spent by the time the app admitted it could not sign in. So this
//! is also what the app remembers, so it can say it before the next one rather
//! than after.

/// Something that went wrong, said so a person can act on it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Trouble {
    /// What is wrong, in one sentence, in the app's own words.
    pub said: String,
    /// What to do about it. Never empty: a named problem with no way out is
    /// only a better-worded dead end.
    pub fix: String,
    /// Whether this will go on happening until somebody does something. These
    /// are the ones worth saying before the next errand rather than after it.
    pub until_somebody_acts: bool,
}

/// What this failure really means, where it is one this app recognises.
///
/// Nothing for the ones it does not, and that is deliberate: a guess dressed as
/// an explanation is worse than the provider's own words, because the provider
/// at least was there.
pub fn what_it_means(why: &str) -> Option<Trouble> {
    let lc = why.to_ascii_lowercase();

    // The one that started this. Claude Code holds its own login and it
    // expires; every turn fails identically until somebody signs in again.
    if lc.contains("oauth") || (lc.contains("401") && lc.contains("authenticat")) {
        return Some(Trouble {
            said: "Claude Code is signed out. Its login has expired or been revoked, so \
                   nothing can be asked of it until it is signed in again."
                .to_string(),
            fix: "Run `claude` in a terminal and sign in. Then come back and ask again \
                  -- nothing you have written here is lost."
                .to_string(),
            until_somebody_acts: true,
        });
    }

    if lc.contains("invalid api key")
        || lc.contains("incorrect api key")
        || lc.contains("error 401")
    {
        return Some(Trouble {
            said: "The key for this model is not being accepted.".to_string(),
            fix: "Check it under Settings, beside the model in the picker. A key that \
                  worked before has usually been rolled or has run out."
                .to_string(),
            until_somebody_acts: true,
        });
    }

    if lc.contains("error 403") || lc.contains("permission denied by the provider") {
        return Some(Trouble {
            said: "The provider refused this account rather than this request.".to_string(),
            fix: "Check the account has this model enabled, and that it is not out of \
                  credit."
                .to_string(),
            until_somebody_acts: true,
        });
    }

    if lc.contains("error 404") && lc.contains("model") || lc.contains("unknown model") {
        return Some(Trouble {
            said: "The model this agent is set to is not there any more.".to_string(),
            fix: "Choose another in the picker at the top. A model can be withdrawn by \
                  its provider, or renamed."
                .to_string(),
            until_somebody_acts: true,
        });
    }

    // Something that will very likely work in a minute. Named so it does not
    // read as somebody's fault, and deliberately not marked as needing action.
    if crate::local::talk::is_server_down_error(why) {
        return Some(Trouble {
            said: format!(
                "The model server is {}.",
                crate::local::talk::short_server_down_reason(why)
            ),
            fix: "It was tried twice. If it is a machine of yours, check it is running; \
                  otherwise this usually clears on its own."
                .to_string(),
            until_somebody_acts: false,
        });
    }
    None
}

/// The whole thing, as one piece of text for a line in a conversation.
pub fn as_a_line(trouble: &Trouble) -> String {
    format!("{} {}", trouble.said, trouble.fix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_expired_login_says_it_is_a_login_and_how_to_fix_it() {
        // The exact string that arrived, on a message somebody had spent a
        // paragraph and a screenshot writing.
        let real = "Failed to authenticate. API Error: 401 OAuth access token has been revoked.";
        let said = what_it_means(real).expect("this one is recognised");
        assert!(said.said.contains("signed out"), "{said:?}");
        assert!(said.fix.contains("claude"), "{said:?}");
        // And it is the kind worth saying before the next errand rather than
        // after: it will fail identically every time until somebody acts.
        assert!(said.until_somebody_acts);
    }

    #[test]
    fn a_busy_server_is_not_somebodys_fault_and_needs_nothing_done() {
        // The distinction that decides whether the app nags: a rate limit
        // clears itself, a revoked token does not.
        let said = what_it_means("error sending request: connection refused")
            .expect("this one is recognised");
        assert!(!said.until_somebody_acts, "{said:?}");
        assert!(said.said.contains("server"), "{said:?}");
    }

    #[test]
    fn a_failure_this_app_does_not_recognise_is_left_in_the_provider_s_own_words() {
        // A guess dressed as an explanation is worse than the raw message,
        // because the provider at least was there.
        assert_eq!(what_it_means("something nobody has seen before"), None);
        assert_eq!(what_it_means(""), None);
    }

    #[test]
    fn every_named_trouble_says_what_to_do() {
        // A named problem with no way out is a better-worded dead end.
        for why in [
            "401 OAuth token revoked",
            "LLM error 401: invalid api key",
            "LLM error 403: forbidden",
            "LLM error 404: unknown model gpt-9",
            "error sending request: connection refused",
        ] {
            let said = what_it_means(why).unwrap_or_else(|| panic!("{why} was not recognised"));
            assert!(!said.fix.trim().is_empty(), "{why}: {said:?}");
            assert!(!said.said.trim().is_empty(), "{why}: {said:?}");
        }
    }
}
