//! Re-export shim for `ThreadedMessage` / `ThreadedMessageFollowup`. The
//! canonical home is [`crate::components::ui::thread_message`]; this module
//! keeps the legacy `crate::components::threaded_message::ThreadedMessage`
//! import path working for integration consumers (Drive, Gmail, Linear,
//! GitHub, Slack).

#[allow(unused_imports)]
pub use crate::components::ui::thread_message::{ThreadedMessage, ThreadedMessageFollowup};
