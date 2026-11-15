//! Chat as a server starts it: TextChatService's configuration and default
//! channels, and the legacy chat's scripts. See docs/behavior/gui/chat.md.

pub(crate) mod legacy;
mod text_chat;

pub(crate) use text_chat::{set_up, set_up_client, set_up_local_player};
