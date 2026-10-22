//! Typing into the focused TextBox: text from the platform, the editing
//! keys, the caret and selection, copy and paste, and the keys that end
//! it (docs/behavior/gui/input.md#textbox).

use glam::Vec2;
use rook_dom::InstanceId;
use rook_types::Value;

use crate::DataModel;

/// A TextBox's text with its caret and selection, by character.
struct Edit {
    text: Vec<char>,
    caret: usize,
    /// Where the selection started, the caret being its other end.
    anchor: Option<usize>,
}

impl Edit {
    fn selected(&self) -> Option<(usize, usize)> {
        let anchor = self.anchor.filter(|&a| a != self.caret)?;
        Some((anchor.min(self.caret), anchor.max(self.caret)))
    }

    /// Delete the selection, if there is one.
    fn cut(&mut self) -> bool {
        let Some((lo, hi)) = self.selected() else {
            return false;
        };
        self.text.drain(lo..hi);
        self.caret = lo;
        self.anchor = None;
        true
    }

    fn insert(&mut self, s: &str) {
        self.cut();
        let new: Vec<char> = s.chars().collect();
        let n = new.len();
        self.text.splice(self.caret..self.caret, new);
        self.caret += n;
    }

    /// Move the caret to `to`, growing the selection with Shift held, or
    /// dropping it.
    fn move_to(&mut self, to: usize, extend: bool) {
        if extend {
            self.anchor.get_or_insert(self.caret);
        } else {
            self.anchor = None;
        }
        self.caret = to.min(self.text.len());
    }

    /// The start of the word before the caret (Ctrl with Left and
    /// Backspace).
    fn word_back(&self) -> usize {
        let mut i = self.caret;
        while i > 0 && self.text[i - 1].is_whitespace() {
            i -= 1;
        }
        while i > 0 && !self.text[i - 1].is_whitespace() {
            i -= 1;
        }
        i
    }

    /// The end of the word after the caret (Ctrl with Right and Delete).
    fn word_forward(&self) -> usize {
        let mut i = self.caret;
        let n = self.text.len();
        while i < n && !self.text[i].is_whitespace() {
            i += 1;
        }
        while i < n && self.text[i].is_whitespace() {
            i += 1;
        }
        i
    }

    /// Where the caret's line starts and ends.
    fn line(&self) -> (usize, usize) {
        let start = self.text[..self.caret]
            .iter()
            .rposition(|&c| c == '\n')
            .map_or(0, |i| i + 1);
        let end = self.text[self.caret..]
            .iter()
            .position(|&c| c == '\n')
            .map_or(self.text.len(), |i| self.caret + i);
        (start, end)
    }
}

impl DataModel {
    fn held(&self, keys: [&str; 2]) -> bool {
        let w = self.runtime.world();
        keys.iter()
            .filter_map(|k| super::key_code_value(k))
            .any(|v| w.input.keys.contains(&v))
    }

    fn read_edit(&self, id: InstanceId) -> Edit {
        let w = self.runtime.world();
        let text: Vec<char> = (w.dom.string_property(id, "Text").unwrap_or_default())
            .chars()
            .collect();
        let at = |name| {
            let n = w.dom.number_property(id, name).unwrap_or(-1.0);
            (n >= 1.0).then(|| (n as usize - 1).min(text.len()))
        };
        let caret = at("CursorPosition").unwrap_or(text.len());
        let anchor = at("SelectionStart");
        Edit {
            text,
            caret,
            anchor,
        }
    }

    /// Store an edit. Measured: `CursorPosition` moves before `Text`
    /// changes.
    fn write_edit(&self, id: InstanceId, edit: &Edit) {
        let position = |i: Option<usize>| {
            Value::Int32(i.map_or(-1, |i| i32::try_from(i + 1).unwrap_or(i32::MAX)))
        };
        self.set_property(id, "CursorPosition", position(Some(edit.caret)));
        self.set_property(id, "SelectionStart", position(edit.anchor));
        let text: String = edit.text.iter().collect();
        if self
            .runtime
            .world()
            .dom
            .string_property(id, "Text")
            .as_deref()
            != Some(text.as_str())
        {
            self.set_property(id, "Text", Value::String(text));
        }
    }

    /// A click on TextBox `id` at `at` (GUI coordinates): focus it, its
    /// caret where it was clicked unless focusing clears it.
    pub(super) fn focus_by_click(&self, id: InstanceId, at: Vec2) {
        let caret = {
            let mut w = self.runtime.world_mut();
            let w = &mut *w;
            w.gui.caret_index_at(&mut w.dom, id, at)
        };
        self.runtime.focus_text_box(id, caret);
        self.runtime.drain_deferred();
    }

    /// Text typed into the focused TextBox. Control characters (Enter,
    /// Tab, Backspace as text) are left to their keys.
    pub(super) fn type_text(&self, text: &str) -> bool {
        let Some(id) = self.focused_text_box() else {
            return false;
        };
        let typed: String = text.chars().filter(|c| !c.is_control()).collect();
        if typed.is_empty() || !self.editable(id) {
            return true;
        }
        let mut edit = self.read_edit(id);
        edit.insert(&typed);
        self.write_edit(id, &edit);
        true
    }

    fn editable(&self, id: InstanceId) -> bool {
        self.runtime.world().dom.bool_property(id, "TextEditable") != Some(false)
    }

    /// A key pressed with a TextBox focused, before scripts hear of it.
    /// Measured: Enter ends it with `enterPressed` true (a newline in a
    /// `MultiLine` box), Escape ends it with false, Tab does nothing; the
    /// text stays as typed.
    pub(super) fn edit_key(&mut self, code: &str, obj: InstanceId) {
        let Some(id) = self.focused_text_box() else {
            return;
        };
        let shift = self.held(["LeftShift", "RightShift"]);
        let ctrl = self.held(["LeftControl", "RightControl"]);
        let multi_line = self.runtime.world().dom.bool_property(id, "MultiLine") == Some(true);
        match code {
            "Return" | "KeypadEnter" if multi_line => {
                self.type_line_break(id);
                return;
            }
            "Return" | "KeypadEnter" => {
                self.runtime.release_text_box(true, Some(obj));
                self.runtime.drain_deferred();
                return;
            }
            "Escape" => {
                self.input.escape_ended_box = true;
                self.runtime.release_text_box(false, Some(obj));
                self.runtime.drain_deferred();
                return;
            }
            _ => {}
        }
        let editable = self.editable(id);
        let mut edit = self.read_edit(id);
        let selected: String = edit
            .selected()
            .map(|(lo, hi)| edit.text[lo..hi].iter().collect())
            .unwrap_or_default();
        match code {
            "Left" => {
                let to = match edit.selected() {
                    Some((lo, _)) if !shift => lo,
                    _ if ctrl => edit.word_back(),
                    _ => edit.caret.saturating_sub(1),
                };
                edit.move_to(to, shift);
            }
            "Right" => {
                let to = match edit.selected() {
                    Some((_, hi)) if !shift => hi,
                    _ if ctrl => edit.word_forward(),
                    _ => edit.caret + 1,
                };
                edit.move_to(to, shift);
            }
            "Home" => {
                let to = if ctrl { 0 } else { edit.line().0 };
                edit.move_to(to, shift);
            }
            "End" => {
                let to = if ctrl { edit.text.len() } else { edit.line().1 };
                edit.move_to(to, shift);
            }
            "Backspace" if editable && !edit.cut() => {
                let from = if ctrl {
                    edit.word_back()
                } else {
                    edit.caret.saturating_sub(1)
                };
                edit.text.drain(from..edit.caret);
                edit.caret = from;
            }
            "Delete" if editable && !edit.cut() => {
                let to = if ctrl {
                    edit.word_forward()
                } else {
                    (edit.caret + 1).min(edit.text.len())
                };
                edit.text.drain(edit.caret..to);
            }
            "A" if ctrl => {
                edit.anchor = Some(0);
                edit.caret = edit.text.len();
            }
            "C" if ctrl && !selected.is_empty() => {
                rook_platform::get().clipboard.set_text(&selected);
                return;
            }
            "X" if ctrl && editable && !selected.is_empty() => {
                rook_platform::get().clipboard.set_text(&selected);
                edit.cut();
            }
            "V" if ctrl && editable => {
                let Some(pasted) = rook_platform::get().clipboard.text() else {
                    return;
                };
                // Inference: a single-line box takes a paste's lines joined.
                let pasted: String = if multi_line {
                    pasted.replace("\r\n", "\n")
                } else {
                    pasted.replace("\r\n", " ").replace('\n', " ")
                };
                let pasted: String = pasted
                    .chars()
                    .filter(|&c| c == '\n' || !c.is_control())
                    .collect();
                edit.insert(&pasted);
            }
            _ => return,
        }
        self.write_edit(id, &edit);
    }

    fn type_line_break(&self, id: InstanceId) {
        if !self.editable(id) {
            return;
        }
        let mut edit = self.read_edit(id);
        edit.insert("\n");
        self.write_edit(id, &edit);
    }
}
