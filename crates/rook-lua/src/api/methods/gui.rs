//! GUI's methods: GuiService and PlayerGui, StyleRules, text sizes, TextBox focus and TextChatService.

use super::*;

pub(crate) mod screen;
mod styles;
mod text;
pub(crate) mod text_chat;
pub(crate) mod textbox;

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    first(
        &[
            screen::native,
            textbox::native,
            text::native,
            styles::native,
            text_chat::native,
        ],
        owner,
        name,
    )
}

pub(super) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    first(&[screen::luau_source, text_chat::luau_source], owner, name)
}
