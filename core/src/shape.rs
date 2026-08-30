//! Asking for an answer a script can use.
//!
//! An agent answers in prose, which is right for a person and useless to the
//! thing that called it from a shell. `errand ask ... | jq .day` gets "Sunday,
//! and the shops are shut" and jq says nothing sensible about that, so the
//! script either parses English or gives up, and both of those are worse than
//! asking properly in the first place.
//!
//! So a caller can say what shape the answer has to be, and two things happen:
//! the request carries the instruction, and the answer is checked before it is
//! printed. The check is the half that matters. Prose where a script expected
//! JSON is a failure that shows up three steps later as a wrong value, and an
//! exit code at the moment it happens costs nobody an afternoon.

use serde_json::Value;

/// What to add to a request so the answer comes back usable.
///
/// The shape is given rather than described, because a model handed an example
/// matches it and a model handed a paragraph about JSON writes a paragraph
/// about JSON. Nothing here promises the model will obey; that is what the
/// check afterwards is for.
pub fn how_to_answer(shape: Option<&str>) -> String {
    let mut asked = String::from(
        "Answer with JSON and nothing else: no sentence before it, no sentence \
         after it, and no code fence around it.",
    );
    if let Some(shape) = shape {
        asked.push_str(" It has to match this shape exactly:\n");
        asked.push_str(shape.trim());
    }
    asked
}

/// The request, with the instruction under it.
///
/// A blank line between them, because they are two different things being said
/// and a model reading one run-on paragraph treats the instruction as part of
/// the job it was given.
pub fn asked_for(request: &str, shape: Option<&str>) -> String {
    format!("{}\n\n{}", request.trim(), how_to_answer(shape))
}

/// The answer, if it is the shape that was asked for.
///
/// Fences are taken off first. Every model wraps JSON in ```json when it is
/// feeling helpful, and refusing that would be refusing a correct answer over
/// three characters, which is a rule nobody would defend out loud.
pub fn what_came_back(said: &str) -> Result<String, String> {
    let bare = unfenced(said.trim());
    match serde_json::from_str::<Value>(bare) {
        Ok(_) => Ok(bare.to_string()),
        // Said with what actually arrived, because the answer to "why is this
        // not JSON" is nearly always visible in the first line of it: a model
        // that explained itself first, or one that could not do the job and
        // said so in a sentence.
        Err(why) => Err(format!(
            "the answer was asked for as JSON and is not: {why}\n{}",
            first_of(said)
        )),
    }
}

/// Inside the fence, where there is one.
///
/// Only when the whole answer is one fenced block. A fence in the middle of a
/// sentence is somebody talking about JSON rather than answering in it, and
/// pulling it out would be inventing an answer that was not given.
fn unfenced(said: &str) -> &str {
    let Some(rest) = said.strip_prefix("```") else {
        return said;
    };
    // ```json, ```JSON, ``` on its own: everything up to the first newline is
    // the language, not the answer.
    let Some((_language, body)) = rest.split_once('\n') else {
        return said;
    };
    match body.trim_end().strip_suffix("```") {
        Some(inside) => inside.trim(),
        None => said,
    }
}

/// Enough of it to see what went wrong, and no more.
///
/// An agent that failed can produce paragraphs, and a shell that prints all of
/// them buries the exit code it should have noticed.
fn first_of(said: &str) -> String {
    const ENOUGH: usize = 300;
    let said = said.trim();
    match said.char_indices().nth(ENOUGH) {
        Some((at, _)) => format!("{}…", &said[..at]),
        None => said.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shape_asked_for_is_shown_rather_than_described() {
        // A model handed an example matches it. A model handed a paragraph
        // about JSON writes a paragraph about JSON.
        let asked = asked_for("What day is it?", Some(r#"{"day": "Sunday"}"#));
        assert!(asked.starts_with("What day is it?"), "{asked}");
        assert!(asked.contains(r#"{"day": "Sunday"}"#), "{asked}");
        // Two things being said, not one run-on paragraph.
        assert!(asked.contains("\n\n"), "{asked}");
    }

    #[test]
    fn json_can_be_asked_for_without_saying_what_is_in_it() {
        let asked = asked_for("What day is it?", None);
        assert!(asked.contains("JSON"), "{asked}");
        assert!(!asked.contains("match this shape"), "{asked}");
    }

    #[test]
    fn an_answer_in_the_shape_asked_for_comes_back_as_it_was_written() {
        // Not reserialised: a script asked for JSON, and rewriting the numbers
        // and the key order on the way past is a change nobody asked for.
        let said = r#"{"day": "Sunday", "shops": false, "temperature": 17.50}"#;
        assert_eq!(what_came_back(said).expect("JSON"), said);
    }

    #[test]
    fn a_fence_around_the_answer_is_not_a_reason_to_reject_it() {
        // Every model wraps JSON in ```json when it is feeling helpful, and
        // failing over three characters is a rule nobody would defend.
        for wrapped in [
            "```json\n{\"day\": \"Sunday\"}\n```",
            "```JSON\n{\"day\": \"Sunday\"}\n```",
            "```\n{\"day\": \"Sunday\"}\n```",
            "  ```json\n{\"day\": \"Sunday\"}\n```  ",
        ] {
            assert_eq!(
                what_came_back(wrapped).expect("JSON"),
                "{\"day\": \"Sunday\"}",
                "{wrapped}"
            );
        }
    }

    #[test]
    fn prose_is_a_failure_rather_than_something_printed_for_a_script_to_parse() {
        // The whole point. A script that gets a sentence where it expected an
        // object finds out three steps later as a wrong value; here it finds
        // out now.
        let why = what_came_back("It is Sunday, and the shops are shut.")
            .expect_err("prose is not an answer in this shape");
        assert!(why.contains("not"), "{why}");
        // With what arrived, because the reason is nearly always in it.
        assert!(why.contains("It is Sunday"), "{why}");
    }

    #[test]
    fn a_model_that_talks_about_json_has_still_not_answered_in_it() {
        // A fence in the middle of a sentence is somebody explaining, and
        // pulling it out would be inventing an answer that was not given.
        let said = "Here you go:\n```json\n{\"day\": \"Sunday\"}\n```\nHope that helps.";
        assert!(what_came_back(said).is_err(), "{said}");
    }

    #[test]
    fn a_long_failure_is_cut_short_rather_than_filling_the_terminal() {
        // An agent that failed can produce paragraphs, and printing all of
        // them buries the exit code somebody should have noticed.
        let waffle = "x".repeat(5_000);
        let why = what_came_back(&waffle).expect_err("not JSON");
        assert!(why.len() < 500, "{} characters", why.len());
        assert!(why.contains('…'), "{why}");
    }
}
