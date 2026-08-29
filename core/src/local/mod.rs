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
    loops::opening_instructions(home, &crate::mcp::Servers::default(), "")
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
    /// Where it answers, e.g. `http://localhost:11434`.
    pub base_url: String,
    /// Which model, e.g. `qwen2.5-coder:14b`.
    pub model: String,
    /// How much it can hold, in tokens.
    pub context_window: usize,
    /// For endpoints that want one. Most local ones do not.
    pub api_key: Option<String>,
    pub temperature: f32,
    /// The ceiling on one reply.
    pub max_tokens: usize,
}

impl Default for LlmSettings {
    fn default() -> Self {
        Self {
            provider: "ollama".into(),
            base_url: "http://localhost:11434".into(),
            model: String::new(),
            context_window: 32_768,
            api_key: None,
            // Low on purpose. This is a thing running errands, where being
            // interesting is not a virtue and doing the same thing twice is.
            temperature: 0.2,
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
