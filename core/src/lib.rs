//! Errand: an errand you talk into existence, and then leave running.
//!
//! The first Errand made you write what you wanted into a box, blind, and hope.
//! It ran once and told you what it could not do. Every failure it produced was
//! a wall described well: not on the list of sites, macOS will not allow it, a
//! permission prompt is waiting. It was built to refuse accurately.
//!
//! This one is a conversation. You say what you want, it tries, it says what it
//! is doing as it does it, and when a door is locked it looks for another one
//! before it looks for you. When it does need you, it says so where the
//! conversation already is, and hands you the controls rather than a paragraph
//! about why it stopped.
//!
//! Two things do the work, and the rest of the program is not allowed to know
//! which: Claude Code, driven as a process on a pipe, and a local model driven
//! by a loop of our own. See `engine` for the protocol they both speak.

pub mod claude;
pub mod engine;
pub mod store;

pub mod local;
pub mod mcp;

pub use engine::{Answer, Engine, Event, NeedsYou, Step};
pub use store::{Line, Store, Thread};
