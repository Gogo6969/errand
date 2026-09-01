//! A model on this machine, driven by a loop of our own.
//!
//! The second engine, and the reason the protocol in `engine` is only seven
//! events wide. Claude Code arrives with tools, permissions and an agent loop
//! already built; a local model arrives with none of that and nothing but
//! tokens, so everything Claude Code does for us has to be done here instead.
//! The window is never told which of the two it is talking to.
//!
//! The talking part is not ours and is not new. `talk`, `stream` and `find` are
//! lifted from KinAI, which has been speaking to Ollama, LM Studio, vLLM,
//! llama.cpp and Open WebUI in anger for a couple of hundred releases. They are
//! kept as close to their originals as the import lines allow, because the
//! value in them is not the HTTP call. It is the accumulated knowledge of how
//! each backend misbehaves, and the comments recording it are worth more than
//! the code they sit above. Rewriting them in our own voice would mean
//! rediscovering all of it.
//!
//! What is ours is the loop on top: turning a stream of tokens and tool calls
//! into the same events Claude Code produces. That includes stopping to ask.
//! A local model running a shell command is exactly as capable of ruining your
//! afternoon as a hosted one, and it goes through the same card.

pub mod again;
pub mod anthropic;
pub mod find;
pub mod ready;
pub mod stream;
pub mod talk;
pub mod tokens;
pub mod tools;

mod loops;

pub use loops::Local;

/// The opening instructions, for looking at.
///
/// A turn has one input nothing prints, and an answer that comes back empty is
/// usually that input being wrong. This is here so it can be read.
pub fn instructions_for(home: &std::path::Path) -> String {
    loops::opening_instructions(home, &crate::mcp::Servers::default(), "", "ask")
}

use serde::{Deserialize, Serialize};

/// Where a model lives and how to talk to it.
///
/// The five fields the ported client actually reads, and no more. KinAI's
/// version of this carries another half-dozen for things that are its business
/// and not ours: which family member is allowed to use the slot, how it routes
/// pictures, the friendly addendum a household puts on its own assistant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmSettings {
    /// One of: ollama, lmstudio, vllm, llamacpp, openai-compat.
    pub provider: String,
    /// Which protocol this one speaks: `openai` or `anthropic`.
    ///
    /// Not the same question as the provider, which is about who is serving.
    /// DeepSeek serves both from the same host at different addresses, and
    /// somebody with a working setup pointed at the second one could not use
    /// it here at all, because there was only ever one answer to this.
    ///
    /// Defaults to the one almost everything speaks, so nothing stored before
    /// this existed changes meaning.
    #[serde(default = "the_usual_protocol")]
    pub wire: String,
    /// Where it answers, e.g. `http://localhost:11434`.
    pub base_url: String,
    /// Which model, e.g. `qwen2.5-coder:14b`.
    pub model: String,
    /// How much it can hold, in tokens.
    ///
    /// Defaulted, because not everything that writes these settings knows it.
    /// The picker writes what it learned from asking the server -- who is
    /// serving, where, which model, which protocol -- and nothing about
    /// sizes. Required, that made every model added from the picker unusable:
    /// the settings were stored, the agent said "Now on Qwen3.8-27B", and the
    /// first thing said to it came back "this thread has no model chosen",
    /// which was not true and pointed nowhere near the fault.
    #[serde(default = "as_much_as_most_hold")]
    pub context_window: usize,
    /// For endpoints that want one. Most local ones do not.
    pub api_key: Option<String>,
    /// What to ask for, where anybody has asked for anything.
    ///
    /// Nothing means "do not mention it", which is not the same as a default
    /// and is the only safe thing to send to a model that has opinions about
    /// it. Kimi's current models pin sampling and answer an error rather than
    /// clamping, so a client that always sends a number fails every request
    /// against them -- including the one that checks whether the key works,
    /// which then reads as a bad key.
    #[serde(default)]
    pub temperature: Option<f32>,
    /// The ceiling on one reply.
    #[serde(default = "enough_for_an_answer")]
    pub max_tokens: usize,
}

/// What almost everything speaks, and what anything stored before there was a
/// choice must go on meaning.
fn the_usual_protocol() -> String {
    "openai".to_string()
}

/// A size that suits nearly everything served locally today.
///
/// The same number `Default` uses, named so that settings written without one
/// mean what settings written with none of them mean.
fn as_much_as_most_hold() -> usize {
    32_768
}

/// Room for an answer rather than for a book.
fn enough_for_an_answer() -> usize {
    4_096
}

impl LlmSettings {
    /// The address to hit for `what`, given whatever is stored as the base.
    pub fn reach(&self, what: &str) -> String {
        Self::reaching(&self.base_url, what)
    }

    /// Which protocol this one speaks.
    pub fn how_it_talks(&self) -> crate::local::stream::Wire {
        match self.wire.as_str() {
            "anthropic" => crate::local::stream::Wire::Anthropic,
            _ => crate::local::stream::Wire::Openai,
        }
    }

    /// The same, without needing the rest of the settings.
    ///
    /// There is no one shape here, which is the whole difficulty. Three hosted
    /// providers, three different answers: Moonshot serves at `<host>/v1`, Z.ai
    /// at `<host>/api/paas/v4`, and DeepSeek at the bare host with the endpoint
    /// straight off the root. A client that appends `/v1` to all of them is
    /// wrong about two, and one of the two fails as a 404 that reads exactly
    /// like a bad key.
    ///
    /// So: a base that already names its version is used as it stands, and one
    /// that does not gets `/v1`, which is where every OpenAI-compatible server
    /// that does not say otherwise puts it. That covers all of the above and
    /// leaves every local server already stored as a bare host working
    /// unchanged, which matters more than tidiness: those are somebody's.
    pub fn reaching(base_url: &str, what: &str) -> String {
        let base = base_url.trim_end_matches('/');
        let what = what.trim_start_matches('/');
        match names_its_version(base) {
            true => format!("{base}/{what}"),
            false => format!("{base}/v1/{what}"),
        }
    }
}

/// Whether the last part of this address is a version, as `/v1` or `/v4` are.
///
/// Only the last part. A host with `v2` somewhere in the middle of its path is
/// saying something about itself, not about where its endpoints are.
pub(crate) fn names_its_version(base: &str) -> bool {
    let last = base.rsplit('/').next().unwrap_or_default();
    let Some(rest) = last.strip_prefix('v').or_else(|| last.strip_prefix('V')) else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit() || c == '.')
}

impl Default for LlmSettings {
    fn default() -> Self {
        Self {
            provider: "ollama".into(),
            wire: the_usual_protocol(),
            base_url: "http://localhost:11434".into(),
            model: String::new(),
            context_window: 32_768,
            api_key: None,
            // Low on purpose. This is a thing running errands, where being
            // interesting is not a virtue and doing the same thing twice is.
            // Not mentioned unless somebody says otherwise. Every server has
            // a sensible default of its own and some refuse to be overruled.
            temperature: None,
            max_tokens: 4_096,
        }
    }
}

/// One turn in the conversation, in the shape the API wants it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum ChatMessage {
    System {
        content: String,
    },
    User {
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// `data:image/png;base64,…` for anything attached. Empty keeps the
        /// plain string form, which endpoints without vision need.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        image_data_urls: Vec<String>,
    },
    Assistant {
        content: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<ToolCall>,
        /// What a thinking model showed of its working, kept exactly as it
        /// arrived and handed back exactly as it arrived.
        ///
        /// Not a nicety. DeepSeek's reasoning models refuse a request with
        /// tools in it when an earlier assistant turn is missing the
        /// `reasoning_content` they sent -- not a warning, a 400 -- and this
        /// whole app is a tool loop, so it would work on the first turn and
        /// fail on the second.
        ///
        /// Kept apart from `content` rather than joined onto it, because
        /// joining them puts the model's private working into the answer
        /// somebody reads, into the notes it keeps and into anything it is
        /// asked to summarise.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning: Option<String>,
    },
    Tool {
        content: String,
        tool_call_id: String,
    },
}

impl ChatMessage {
    /// What was said, whoever said it.
    pub fn content(&self) -> &str {
        match self {
            ChatMessage::System { content } => content,
            ChatMessage::User { content, .. } => content,
            ChatMessage::Assistant { content, .. } => content,
            ChatMessage::Tool { content, .. } => content,
        }
    }
}

/// A tool call as it goes back into the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

impl ToolCall {
    pub fn new(id: String, name: String, arguments: String) -> Self {
        Self {
            id,
            kind: "function".into(),
            function: ToolCallFunction { name, arguments },
        }
    }
}

/// A tool as the model is told about it.
#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    /// The full OpenAI function-schema object, sent as-is.
    pub schema: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_the_picker_writes_is_enough_to_use_the_model_it_names() {
        // Exactly what the "Show in picker" button stores: who is serving,
        // where, which model, which protocol. Nothing about sizes, because
        // asking a server what it has does not tell you those.
        //
        // Required fields, that made every model added from the picker
        // unusable. The settings were stored, the header said "Now on
        // Qwen3.8-27B-Q4_K_M", and the first thing said to it came back "this
        // thread has no model chosen", which was untrue and pointed nowhere
        // near the fault.
        let written = r#"{"provider":"llamacpp","base_url":"http://192.168.1.25:8081","model":"Qwen3.8-27B-Q4_K_M","wire":"openai"}"#;
        let settings: LlmSettings = serde_json::from_str(written).expect("the picker's own shape");

        assert_eq!(settings.model, "Qwen3.8-27B-Q4_K_M");
        assert_eq!(settings.base_url, "http://192.168.1.25:8081");
        // And the two it does not write mean what they mean everywhere else.
        assert_eq!(
            settings.context_window,
            LlmSettings::default().context_window
        );
        assert_eq!(settings.max_tokens, LlmSettings::default().max_tokens);
    }

    #[test]
    fn settings_stored_before_the_protocol_was_a_choice_still_mean_what_they_meant() {
        // The oldest shape of all, from before `wire` existed.
        let old =
            r#"{"provider":"ollama","base_url":"http://localhost:11434","model":"qwen2.5:7b"}"#;
        let settings: LlmSettings = serde_json::from_str(old).expect("an older shape");
        assert_eq!(settings.wire, "openai");
        assert_eq!(settings.model, "qwen2.5:7b");
    }

    #[test]
    fn each_provider_is_reached_where_it_actually_serves() {
        // Three hosted providers, three different shapes, all documented. A
        // client that appends /v1 to every one of them is wrong about two, and
        // the failure is a 404 that reads exactly like a bad key.
        assert_eq!(
            LlmSettings::reaching("https://api.moonshot.ai/v1", "chat/completions"),
            "https://api.moonshot.ai/v1/chat/completions"
        );
        assert_eq!(
            LlmSettings::reaching("https://api.z.ai/api/paas/v4", "chat/completions"),
            "https://api.z.ai/api/paas/v4/chat/completions"
        );
        assert_eq!(
            LlmSettings::reaching("https://api.deepseek.com/v1", "models"),
            "https://api.deepseek.com/v1/models"
        );

        // And a bare host gets /v1, which is where a server that does not say
        // otherwise puts it. Every local one already stored is this shape, and
        // they are somebody's: they go on working.
        assert_eq!(
            LlmSettings::reaching("http://127.0.0.1:11434", "models"),
            "http://127.0.0.1:11434/v1/models"
        );

        // A version in the middle of a path is the host saying something about
        // itself, not about where its endpoints are.
        assert!(!names_its_version("https://api.example.com/v2/openai"));
        assert!(names_its_version("https://api.example.com/v2"));
        assert!(!names_its_version("https://api.example.com/vision"));
    }

    #[test]
    fn a_base_url_written_either_way_reaches_the_same_place() {
        // Every hosted provider documents this differently, and both forms get
        // typed. Appending to one that already ends in /v1 gives /v1/v1/models,
        // which comes back 404 and reads exactly like a wrong key.
        for base in [
            "https://api.deepseek.com",
            "https://api.deepseek.com/",
            "https://api.deepseek.com/v1",
            "https://api.deepseek.com/v1/",
        ] {
            assert_eq!(
                LlmSettings::reaching(base, "models"),
                "https://api.deepseek.com/v1/models",
                "{base}"
            );
        }
        // A trailing slash is not a path segment, whichever shape it is on.
        assert_eq!(
            LlmSettings::reaching("https://api.z.ai/api/paas/v4/", "models"),
            "https://api.z.ai/api/paas/v4/models"
        );

        // A path that merely contains v1 further up is left alone: only a
        // trailing one is the duplicate.
        assert_eq!(
            LlmSettings::reaching("http://box:8000/api/v1", "chat/completions"),
            "http://box:8000/api/v1/chat/completions"
        );
    }
}
