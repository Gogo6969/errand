//! A task taught once and done again by name.
//!
//! Every errand is already written down step by step: each tool call is a
//! `doing` line with the tool underneath it and, once it arrives, the outcome
//! on the same line. A skill is one of those errands kept by name. Saving one
//! keeps what the person asked and the steps that answered it. Running one
//! starts a new turn, in a conversation of its own named after the skill,
//! whose first line is that request and those steps as a plan to follow, so
//! the model does the work again and changes what has to change: the date, a
//! file that has moved, a page that looks different now.
//!
//! Nothing here runs a step. The recorded steps are words the model is handed,
//! and every one of them it takes goes through the same tools, the same
//! posture and the same permission cards as any other turn. That is the
//! difference between this and a macro: a macro replays keystrokes into a
//! world that has moved on, and this replays the intention.
//!
//! What is decided here is decided in words and nowhere else: which lines of a
//! conversation are the errand worth keeping, and what the new turn reads. The
//! store keeps the rows and the app opens the conversation.

use serde::{Deserialize, Serialize};

use crate::engine::Answer;
use crate::store::{Line, Skill, Store};
use crate::team::{which_of_ours, Ours};

/// One step of a skill, as it was written down when it was taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    /// What was done, in the sentence the window showed: "Running ls -la",
    /// "Reading notes.txt". The tool's arguments are not kept separately,
    /// because a line does not hold them; the sentence names the command, the
    /// path or the address, which is what the next run has to know.
    pub what: String,
    /// The tool underneath, by the name the engine that took the step gave it.
    pub tool: String,
    /// The first line of what came back, cut short. Kept so the next run can
    /// tell whether the world still looks the way it did, not so it can be
    /// answered from.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub outcome: String,
    /// Whether the person said no to this step when it stopped to ask.
    ///
    /// Kept in the skill rather than dropped from it, because the plan is a
    /// record of what happened and a step somebody refused is part of that.
    /// But it is not a step to take again: a plan that said "do each one
    /// again" over a refused `rm -rf` would put the same card in front of the
    /// same person on every run, which is the habit-presses-Return failure the
    /// quit question is written to avoid, and an "always" pressed on it once
    /// would do the thing unasked from then on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub refused: bool,
}

/// An errand worth keeping: what was asked, and the steps that answered it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Taught {
    pub request: String,
    pub steps: Vec<Step>,
}

/// The longest a skill's name may be.
const NAME_CHARS: usize = 60;

/// How much of a step's outcome is kept.
///
/// The first line and no more than this, because an outcome can be a whole
/// file, and a plan that carries every file it once read is a plan the model
/// answers from instead of following.
const OUTCOME_CHARS: usize = 120;

/// What `save_skill` answers when the conversation holds no steps to keep.
pub const NOTHING_TO_KEEP: &str = "There are no steps in this conversation to keep. A skill is \
    made from something already done here: do the task first, then save it.";

/// A name, checked.
///
/// One line, short, and not empty, because the name is the key: it is what
/// `run_skill` is called with and what saving again replaces. A model left to
/// itself puts the whole request here, and a request never matches itself
/// twice.
pub fn a_name(said: &str) -> anyhow::Result<String> {
    let name = said.trim();
    anyhow::ensure!(!name.is_empty(), "say what the skill is called");
    anyhow::ensure!(
        !name.contains(['\n', '\r']),
        "a skill's name is one line, not a paragraph"
    );
    anyhow::ensure!(
        name.chars().count() <= NAME_CHARS,
        "a skill's name is at most {NAME_CHARS} characters; put the detail in the request, not \
         the name"
    );
    Ok(name.to_string())
}

/// What the conversation a skill runs in is called.
pub fn called(name: &str) -> String {
    format!("Skill: {name}")
}

/// Is this step bookkeeping rather than the errand?
///
/// Saving a skill, listing them, looking through notes and looking at who
/// else there is are things an agent does around a task and never part of
/// one. Left in, "save what we just did" would keep the saving as a step of
/// the thing saved, and a turn that only looked something up would be taken
/// for the task when the task was the turn before.
fn is_bookkeeping(tool: &str) -> bool {
    matches!(
        which_of_ours(tool),
        Some(
            Ours::SaveSkill
                | Ours::Skills
                | Ours::Recall
                | Ours::Remember
                | Ours::Forget
                | Ours::WhoElse
        )
    )
}

/// The errand worth keeping, out of everything said in a conversation.
///
/// The last turn with a step in it: a turn is what the person said and every
/// step taken before they said something else. The last rather than all of
/// them, because "save what we just did" means the thing just done, and a
/// conversation that has done three errands is not three skills. Usually that
/// is the turn before the one asking to save; it is the same turn when the
/// task and the asking came in one sentence.
pub fn from_lines(lines: &[Line]) -> Option<Taught> {
    let mut turns: Vec<Taught> = Vec::new();
    for line in lines {
        match line.kind.as_str() {
            "mine" => turns.push(Taught {
                request: line.text.trim().to_string(),
                steps: Vec::new(),
            }),
            // An `asking` line is a step that stopped for permission, and it
            // is still a step: what was decided about it is on its outcome,
            // in the words `Answer::in_a_word` writes there, and a no is
            // recognised by those exact words so the plan can say so.
            "doing" | "asking" => {
                let tool = line.tool.clone().unwrap_or_default();
                if is_bookkeeping(&tool) {
                    continue;
                }
                let outcome = line.outcome.as_deref().unwrap_or("");
                if let Some(turn) = turns.last_mut() {
                    turn.steps.push(Step {
                        what: line.text.trim().to_string(),
                        tool,
                        outcome: cut_short(outcome),
                        refused: line.kind == "asking" && outcome.trim() == Answer::No.in_a_word(),
                    });
                }
            }
            _ => {}
        }
    }
    turns
        .into_iter()
        .rev()
        .find(|turn| !turn.steps.is_empty() && !turn.request.is_empty())
}

/// The first line of something, and no more than `OUTCOME_CHARS` of it.
fn cut_short(said: &str) -> String {
    let line = said
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    match line.chars().count() > OUTCOME_CHARS {
        true => format!(
            "{}\u{2026}",
            line.chars().take(OUTCOME_CHARS - 1).collect::<String>()
        ),
        false => line.to_string(),
    }
}

/// What `save_skill` answers once the skill is kept.
pub fn kept(name: &str, taught: &Taught, replaced: bool) -> String {
    let steps = match taught.steps.len() {
        1 => "1 step".to_string(),
        n => format!("{n} steps"),
    };
    let again = match replaced {
        true => " It replaces the skill that was under that name.",
        false => "",
    };
    format!(
        "Kept as the skill `{name}`: {steps}, answering \"{}\". Run it again with run_skill.{again}",
        cut_short(&taught.request)
    )
}

/// The first line of the new turn: the request and the steps as a plan.
///
/// Written for the model that will read it, which is the same agent on
/// another day. It says to follow the steps and to change them, in that
/// order, because a plan read as a script is done wrong the first time a file
/// has moved, and a plan read as a suggestion is not done at all. And it says
/// not to answer from last time's outcomes, because they are right there and
/// a model asked "what is in the folder" will read the answer off the plan.
pub fn the_plan(skill: &Skill, differently: &str) -> String {
    let mut plan = format!(
        "Do again what was done before under the skill called {name}.\n\n\
         What was asked then:\n{request}\n",
        name = skill.name,
        request = skill.request.trim()
    );
    let differently = differently.trim();
    if !differently.is_empty() {
        plan.push_str(&format!("\nWhat is different this time:\n{differently}\n"));
    }
    plan.push_str(
        "\nThe steps taken then, in order, each with the start of what came back. Follow them \
         as a plan: do each one again where the world is the same, change it where the world \
         is different (a new date, a file that has moved, a page that has changed), leave out \
         one that has nothing to act on, and say what you did differently. The tool in brackets \
         is the one used then; use whichever of yours does the same thing. Do not answer from \
         what came back last time: take the steps and answer from what comes back now.\n\n",
    );
    for (i, step) in skill.steps.iter().enumerate() {
        plan.push_str(&format!("{}. {} ({})", i + 1, step.what, step.tool));
        // A refused step is on the list so the run knows it was there, and is
        // marked as the one thing on it not to do again. Its outcome is the
        // refusal itself, which the sentence already says.
        match step.refused {
            true => plan.push_str(SAID_NO_TO_THIS),
            false if !step.outcome.is_empty() => {
                plan.push_str(&format!(" -> {}", step.outcome));
            }
            false => {}
        }
        plan.push('\n');
    }
    plan
}

/// What the plan says beside a step the person said no to.
pub const SAID_NO_TO_THIS: &str =
    ": the person said no to this last time. Leave it out unless they say to do it now.";

/// What `skills` answers.
pub fn listed(skills: &[Skill]) -> String {
    if skills.is_empty() {
        return "No skills have been taught here yet. Do a task once, then save it with \
                save_skill and a name, and it can be run again with run_skill."
            .to_string();
    }
    let each: Vec<String> = skills
        .iter()
        .map(|skill| {
            let steps = match skill.steps.len() {
                1 => "1 step".to_string(),
                n => format!("{n} steps"),
            };
            let made = chrono::DateTime::from_timestamp_millis(skill.made_at)
                .map(|at| {
                    at.with_timezone(&chrono::Local)
                        .format("%-d %B %Y")
                        .to_string()
                })
                .unwrap_or_default();
            format!(
                "  {} -- {}; {steps}, saved {made}",
                skill.name,
                cut_short(&skill.request)
            )
        })
        .collect();
    format!(
        "Skills taught here, each run again with run_skill and its name:\n{}",
        each.join("\n")
    )
}

/// What `run_skill` answers when there is no skill by that name.
pub fn nobody_taught(name: &str, skills: &[Skill]) -> String {
    match skills.is_empty() {
        true => format!(
            "There is no skill called `{name}`, and none has been taught here yet. Do the task \
             once, then save it with save_skill."
        ),
        false => format!(
            "There is no skill called `{name}`. The ones taught here are: {}.",
            skills
                .iter()
                .map(|s| format!("`{}`", s.name))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// What `run_skill` answers when the skill is being run from inside itself.
pub fn goes_round_in_circles(name: &str) -> String {
    format!(
        "This conversation is already a run of the skill `{name}`, so running it again from \
         here would go round for ever. Take the steps yourself instead."
    )
}

/// Which skill a conversation is a run of, if it is one.
///
/// Read off the name `called` gives a run, which is the one thing a run
/// carries that nothing else does.
pub fn is_a_run(conversation_name: &str) -> Option<&str> {
    conversation_name.strip_prefix("Skill: ")
}

/// What to say when a run of a skill tries to save one.
///
/// What actually happened: a skill was taught from a request that ended "then
/// save what you just did as a skill called echo-check". Running it, the model
/// followed those words to the letter, called save_skill from inside the run,
/// and the run's own plan became the skill's request. Every run after that
/// would have started from the plan of the plan, and the task somebody taught
/// was gone. A run is a use of a skill, never a lesson.
pub fn a_run_cannot_teach_itself(name: &str) -> String {
    format!(
        "This conversation is a run of the skill `{name}`, and a run cannot be saved as a \
         skill: it would replace `{name}` with its own plan. Do the task once in a \
         conversation of its own and save it from there."
    )
}

/// Is this conversation already inside a run of the skill?
///
/// Followed up the chain of hand-offs rather than checked on this one
/// conversation, because a run may hand part of itself to another agent with
/// `ask`, and that agent may be told to run the same skill: two conversations
/// down is still going round, and every turn of it is a conversation, a
/// process and ten minutes. Bounded the way `who_is_waiting` is, because
/// following rows is the shape of thing that loops if a row is ever wrong.
pub fn already_running(store: &Store, from: &str, name: &str) -> anyhow::Result<bool> {
    const DEEP_ENOUGH: usize = 12;
    let a_run = called(name);
    let mut at = Some(from.to_string());
    let mut followed = 0;
    while let Some(id) = at.take() {
        if followed >= DEEP_ENOUGH {
            break;
        }
        followed += 1;
        let Some(talk) = store.conversation(&id)? else {
            break;
        };
        if talk.name.eq_ignore_ascii_case(&a_run) {
            return Ok(true);
        }
        at = talk.asked_by;
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_of_a_skill_is_known_by_its_name_and_nothing_else_is() {
        assert_eq!(is_a_run(&called("echo-check")), Some("echo-check"));
        assert_eq!(is_a_run("Asked from the terminal"), None);
        assert_eq!(is_a_run("First"), None);
    }

    #[test]
    fn a_run_saying_save_does_not_overwrite_the_skill_it_is_running() {
        // A taught request that ended "then save this as a skill" made the
        // run re-save itself, and the plan became the request. The refusal
        // names the skill and says where a lesson belongs.
        let said = a_run_cannot_teach_itself("echo-check");
        assert!(said.contains("`echo-check`"), "{said}");
        assert!(said.contains("cannot be saved"), "{said}");
        assert!(said.contains("conversation of its own"), "{said}");
    }

    fn mine(seq: i64, text: &str) -> Line {
        Line {
            seq,
            at: seq * 1000,
            kind: "mine".into(),
            text: text.into(),
            call: None,
            tool: None,
            outcome: None,
            anchor: None,
            pictures: Vec::new(),
            said_by: None,
        }
    }

    fn did(seq: i64, tool: &str, what: &str, outcome: Option<&str>) -> Line {
        Line {
            seq,
            at: seq * 1000,
            kind: "doing".into(),
            text: what.into(),
            call: Some(format!("call_{seq}")),
            tool: Some(tool.into()),
            outcome: outcome.map(str::to_string),
            anchor: None,
            pictures: Vec::new(),
            said_by: None,
        }
    }

    fn said(seq: i64, text: &str) -> Line {
        Line {
            kind: "said".into(),
            ..mine(seq, text)
        }
    }

    #[test]
    fn the_errand_kept_is_the_last_turn_with_steps_in_it_and_not_the_turn_asking_to_save() {
        // "Save what we just did" arrives as its own turn, after the work.
        // The turn asking has no steps but the saving itself, which is not
        // part of what it saves.
        let lines = vec![
            mine(1, "Tidy the Downloads folder"),
            did(
                2,
                "run_command",
                "Running ls ~/Downloads",
                Some("a.pdf\nb.pdf"),
            ),
            did(
                3,
                "run_command",
                "Running mv ~/Downloads/*.pdf ~/Papers",
                Some(""),
            ),
            said(4, "Moved two PDFs to Papers."),
            mine(5, "save what we just did as a skill called tidy"),
            did(6, "save_skill", "Keeping this as a skill called tidy", None),
        ];
        let taught = from_lines(&lines).expect("there is an errand to keep");
        assert_eq!(taught.request, "Tidy the Downloads folder");
        assert_eq!(
            taught.steps,
            [
                Step {
                    what: "Running ls ~/Downloads".into(),
                    tool: "run_command".into(),
                    outcome: "a.pdf".into(),
                    refused: false,
                },
                Step {
                    what: "Running mv ~/Downloads/*.pdf ~/Papers".into(),
                    tool: "run_command".into(),
                    outcome: String::new(),
                    refused: false,
                },
            ]
        );
    }

    #[test]
    fn a_task_and_the_asking_in_one_sentence_keep_that_turns_own_steps() {
        let lines = vec![
            mine(
                1,
                "Count the files here and save that as a skill called count",
            ),
            did(2, "run_command", "Running ls | wc -l", Some("12")),
            did(
                3,
                "mcp__errand__save_skill",
                "Keeping this as a skill",
                None,
            ),
        ];
        let taught = from_lines(&lines).unwrap();
        assert_eq!(
            taught.request,
            "Count the files here and save that as a skill called count"
        );
        assert_eq!(taught.steps.len(), 1);
        assert_eq!(taught.steps[0].what, "Running ls | wc -l");
    }

    #[test]
    fn a_turn_that_only_looked_through_notes_is_not_the_task() {
        // The model checks its notes and the list of skills before saving.
        // Those are things done around a task, and a turn made of nothing
        // else is passed over for the turn that did the work.
        let lines = vec![
            mine(1, "Read the report"),
            did(2, "read_file", "Reading report.md", Some("# Report")),
            mine(3, "save that as a skill called report"),
            did(
                4,
                "recall",
                "Looking up what it knows about report",
                Some("Nothing"),
            ),
            did(5, "skills", "Looking at the skills taught here", None),
            did(6, "who_else", "Looking for somebody to hand this to", None),
        ];
        let taught = from_lines(&lines).unwrap();
        assert_eq!(taught.request, "Read the report");
        assert_eq!(taught.steps.len(), 1);
        assert_eq!(taught.steps[0].tool, "read_file");
    }

    #[test]
    fn a_conversation_with_no_steps_in_it_has_nothing_to_keep() {
        assert_eq!(from_lines(&[]), None);
        let only_talk = vec![mine(1, "hello"), said(2, "Hello."), mine(3, "save it")];
        assert_eq!(from_lines(&only_talk), None);
        let only_bookkeeping = vec![
            mine(1, "what do you know?"),
            did(
                2,
                "recall",
                "Looking through its own notes",
                Some("Nothing yet"),
            ),
        ];
        assert_eq!(from_lines(&only_bookkeeping), None);
    }

    fn asked(seq: i64, tool: &str, what: &str, decided: Answer) -> Line {
        // The words the app really writes on an answered `asking` line, not a
        // fixture's idea of them: a fixture that said "Not allowed" passed
        // while the store held "You said no".
        Line {
            kind: "asking".into(),
            ..did(seq, tool, what, Some(decided.in_a_word()))
        }
    }

    #[test]
    fn a_step_that_stopped_to_ask_is_still_a_step_and_carries_what_was_decided() {
        let lines = vec![
            mine(1, "Clean the build"),
            asked(2, "Bash", "Running rm -rf build", Answer::Yes),
        ];
        let taught = from_lines(&lines).unwrap();
        assert_eq!(taught.steps[0].outcome, "You said yes");
        assert!(!taught.steps[0].refused);
    }

    #[test]
    fn a_step_the_person_said_no_to_is_not_in_the_plan_as_something_to_do_again() {
        // Kept, so the run knows the step was there; marked, so the same card
        // is not put in front of the same person on every run of the skill.
        let lines = vec![
            mine(1, "Clean the build"),
            did(2, "Bash", "Running ls build", Some("a.o\nb.o")),
            asked(3, "Bash", "Running rm -rf build", Answer::No),
        ];
        let taught = from_lines(&lines).unwrap();
        assert_eq!(taught.steps.len(), 2);
        assert!(taught.steps[1].refused);
        let skill = Skill {
            name: "clean".into(),
            request: taught.request.clone(),
            steps: taught.steps.clone(),
            made_at: 1_700_000_000_000,
        };
        let plan = the_plan(&skill, "");
        assert!(
            plan.contains("1. Running ls build (Bash) -> a.o\n"),
            "{plan}"
        );
        assert!(
            plan.contains(
                "2. Running rm -rf build (Bash): the person said no to this last time. Leave it \
                 out unless they say to do it now.\n"
            ),
            "{plan}"
        );
        assert!(!plan.contains("-> You said no"), "{plan}");
        // A yes, or a yes-and-stop-asking, is an ordinary step.
        for decided in [Answer::Yes, Answer::Always] {
            let lines = vec![
                mine(1, "Clean the build"),
                asked(2, "Bash", "Running rm -rf build", decided),
            ];
            assert!(!from_lines(&lines).unwrap().steps[0].refused);
        }
        // And the same words on a `doing` line are an outcome, not a refusal.
        let lines = vec![
            mine(1, "Read it"),
            did(2, "read_file", "Reading no.txt", Some("You said no")),
        ];
        assert!(!from_lines(&lines).unwrap().steps[0].refused);
    }

    #[test]
    fn an_outcome_is_kept_as_its_first_line_and_cut_short() {
        let long = "x".repeat(300);
        let lines = vec![
            mine(1, "Read it"),
            did(
                2,
                "read_file",
                "Reading big.txt",
                Some(&format!("\n\n  {long}\nmore")),
            ),
        ];
        let taught = from_lines(&lines).unwrap();
        assert_eq!(taught.steps[0].outcome.chars().count(), OUTCOME_CHARS);
        assert!(taught.steps[0].outcome.ends_with('\u{2026}'));
    }

    #[test]
    fn a_name_is_one_short_line_or_it_is_refused_in_a_sentence_the_model_can_act_on() {
        assert_eq!(a_name("  tidy downloads ").unwrap(), "tidy downloads");
        assert!(a_name("   ")
            .unwrap_err()
            .to_string()
            .contains("what the skill is called"));
        assert!(a_name("one\ntwo")
            .unwrap_err()
            .to_string()
            .contains("one line"));
        assert!(a_name(&"n".repeat(61))
            .unwrap_err()
            .to_string()
            .contains("at most 60 characters"));
        assert!(a_name(&"n".repeat(60)).is_ok());
    }

    fn a_skill() -> Skill {
        Skill {
            name: "tidy".into(),
            request: "Tidy the Downloads folder".into(),
            steps: vec![
                Step {
                    what: "Running ls ~/Downloads".into(),
                    tool: "run_command".into(),
                    outcome: "a.pdf".into(),
                    refused: false,
                },
                Step {
                    what: "Running mv ~/Downloads/*.pdf ~/Papers".into(),
                    tool: "run_command".into(),
                    outcome: String::new(),
                    refused: false,
                },
            ],
            made_at: 1_700_000_000_000,
        }
    }

    #[test]
    fn the_plan_carries_the_request_and_every_step_in_order_and_says_to_adapt() {
        let plan = the_plan(&a_skill(), "");
        assert!(plan.starts_with("Do again what was done before under the skill called tidy."));
        assert!(plan.contains("What was asked then:\nTidy the Downloads folder\n"));
        let first = plan
            .find("1. Running ls ~/Downloads (run_command) -> a.pdf")
            .unwrap();
        let second = plan
            .find("2. Running mv ~/Downloads/*.pdf ~/Papers (run_command)\n")
            .unwrap();
        assert!(first < second, "the steps are out of order");
        assert!(
            plan.contains("change it where the world is different"),
            "the plan does not say to adapt"
        );
        assert!(
            plan.contains("Do not answer from what came back last time"),
            "the plan does not say to take the steps rather than read the old answers"
        );
        assert!(!plan.contains("What is different this time"));
    }

    #[test]
    fn what_is_different_this_time_is_said_before_the_steps() {
        let plan = the_plan(&a_skill(), " use ~/Desktop instead of ~/Downloads ");
        let different = plan
            .find("What is different this time:\nuse ~/Desktop instead of ~/Downloads\n")
            .expect("it says what is different");
        let steps = plan.find("1. Running").unwrap();
        assert!(different < steps);
    }

    #[test]
    fn the_list_names_each_skill_with_what_it_answers_and_says_how_to_run_one() {
        assert!(listed(&[]).contains("No skills have been taught here yet"));
        let list = listed(&[a_skill()]);
        assert!(list.contains("run_skill"));
        assert!(list.contains("  tidy -- Tidy the Downloads folder; 2 steps, saved "));
    }

    #[test]
    fn a_skill_nobody_taught_is_answered_with_the_ones_that_were() {
        assert!(nobody_taught("x", &[]).contains("none has been taught here yet"));
        let said = nobody_taught("x", &[a_skill()]);
        assert!(said.contains("There is no skill called `x`"));
        assert!(said.contains("`tidy`"));
    }

    #[test]
    fn a_skill_is_taken_from_the_lines_the_store_wrote_and_read_back_as_a_plan() {
        // The whole path through the real store: the steps an engine's
        // events wrote down, the outcome that arrived later against the same
        // call, the skill kept and read back, and the plan the next run is
        // handed. Each piece is tested on its own above; this is the joins.
        use crate::engine::{Event, Step as Taken};
        let s = Store::in_memory().unwrap();
        s.begin("a1", "Scout", std::path::Path::new("/tmp/one"))
            .unwrap();
        s.begin_conversation_for("c1", "a1", "Tidy", None).unwrap();
        s.asked("c1", "Tidy the Downloads folder").unwrap();
        s.happened(
            "c1",
            &Event::Doing(Taken {
                what: "Running ls ~/Downloads".into(),
                tool: "run_command".into(),
                call: "call_1".into(),
            }),
        )
        .unwrap();
        s.happened(
            "c1",
            &Event::Did {
                call: "call_1".into(),
                outcome: "a.pdf\nb.pdf".into(),
            },
        )
        .unwrap();
        s.happened(
            "c1",
            &Event::Said {
                text: "Two PDFs.".into(),
                settled: true,
            },
        )
        .unwrap();
        s.asked("c1", "save that as a skill called tidy").unwrap();
        s.happened(
            "c1",
            &Event::Doing(Taken {
                what: "Keeping this as a skill called tidy".into(),
                tool: "mcp__errand__save_skill".into(),
                call: "call_2".into(),
            }),
        )
        .unwrap();

        let taught = from_lines(&s.lines("c1").unwrap()).expect("there is an errand to keep");
        assert_eq!(taught.request, "Tidy the Downloads folder");
        assert_eq!(taught.steps.len(), 1);
        assert_eq!(taught.steps[0].outcome, "a.pdf");

        let name = a_name("tidy").unwrap();
        assert!(!s
            .keep_skill("a1", &name, &taught.request, &taught.steps)
            .unwrap());
        let kept = s
            .skill("a1", "TIDY")
            .unwrap()
            .expect("kept, whatever the case");
        assert_eq!(kept.steps, taught.steps);
        let plan = the_plan(&kept, "");
        assert!(plan.contains("1. Running ls ~/Downloads (run_command) -> a.pdf"));
        assert!(plan.contains("What was asked then:\nTidy the Downloads folder"));
    }

    #[test]
    fn a_run_of_a_skill_is_known_from_anywhere_down_its_chain_of_hand_offs() {
        // The run itself, and a conversation the run opened by asking another
        // agent, are both inside it. An unrelated conversation is not, and
        // nor is a run of a different skill.
        let s = Store::in_memory().unwrap();
        s.begin("a1", "Scout", std::path::Path::new("/tmp/one"))
            .unwrap();
        s.begin("a2", "Scribe", std::path::Path::new("/tmp/two"))
            .unwrap();
        s.begin_conversation_for("plain", "a1", "Tidy", None)
            .unwrap();
        s.begin_conversation_for("run", "a1", &called("tidy"), Some("plain"))
            .unwrap();
        s.begin_conversation_for("handed", "a2", "Asked by Scout", Some("run"))
            .unwrap();
        s.begin_conversation_for("other", "a1", &called("report"), Some("plain"))
            .unwrap();

        assert!(already_running(&s, "run", "tidy").unwrap());
        assert!(
            already_running(&s, "run", "Tidy").unwrap(),
            "case decided it"
        );
        assert!(already_running(&s, "handed", "tidy").unwrap());
        assert!(!already_running(&s, "plain", "tidy").unwrap());
        assert!(!already_running(&s, "other", "tidy").unwrap());
        assert!(!already_running(&s, "nowhere", "tidy").unwrap());
    }

    #[test]
    fn saving_says_how_many_steps_were_kept_and_whether_one_was_replaced() {
        let taught = Taught {
            request: a_skill().request,
            steps: a_skill().steps,
        };
        let fresh = kept("tidy", &taught, false);
        assert!(fresh.contains("Kept as the skill `tidy`: 2 steps"));
        assert!(!fresh.contains("replaces"));
        assert!(
            kept("tidy", &taught, true).contains("It replaces the skill that was under that name.")
        );
    }
}
