//! What the window was told since the last frame. Window and device
//! events only land here, in the order they came; the frame takes them all
//! at its start ([`App::apply_inbox`]). The inbox holds no window, game or
//! renderer, so an event costs no more than noting it however many come: a
//! mouse polled at 1000 Hz, or Windows moving a dragged window, sends
//! hundreds a second.

use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{DeviceEvent, ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};

use super::{App, icon, keys};
use crate::*;

/// An event whose order matters, as it came.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Noted {
    /// A key went down (with the text it typed) or up.
    Key {
        code: KeyCode,
        down: bool,
        text: Option<String>,
    },
    /// Text an input method committed.
    Text(String),
    /// A mouse button went down or up with the cursor at `at`.
    Button {
        button: MouseButton,
        down: bool,
        at: PhysicalPosition<f64>,
    },
    /// The wheel turned this many notches (up positive).
    Wheel(f32),
    /// The window lost focus.
    Unfocused,
}

#[derive(Debug, Default)]
pub(crate) struct Inbox {
    pub(crate) events: Vec<Noted>,
    /// Raw mouse movement, summed (about a pixel a unit).
    pub(crate) motion: (f64, f64),
    /// Where the cursor last was in the window, if it moved.
    pub(crate) cursor: Option<PhysicalPosition<f64>>,
    /// The window's new size, if it changed.
    pub(crate) resized: Option<PhysicalSize<u32>>,
    /// The window moved, perhaps onto another monitor.
    pub(crate) moved: bool,
    /// The window came back into focus, or its scale changed (to this),
    /// so its icons are loaded again at its size. Windows makes a window's
    /// taskbar button a moment after the window, and an icon set before
    /// then never reaches it; by the time the window has focus it exists.
    pub(crate) refocused: bool,
    pub(crate) scale: Option<f64>,
    pub(crate) close: bool,
    /// Where the cursor is now: what the next button is pressed at.
    at: PhysicalPosition<f64>,
}

impl Inbox {
    /// Note a window event; whether it asks for a frame to be drawn
    /// (`RedrawRequested`, which the caller handles).
    pub(crate) fn note(&mut self, event: WindowEvent) -> bool {
        match event {
            WindowEvent::RedrawRequested => return true,
            WindowEvent::CloseRequested => self.close = true,
            WindowEvent::Moved(_) => self.moved = true,
            WindowEvent::Resized(size) => self.resized = Some(size),
            WindowEvent::CursorMoved { position, .. } => {
                self.at = position;
                self.cursor = Some(position);
            }
            WindowEvent::Focused(false) => self.events.push(Noted::Unfocused),
            WindowEvent::Focused(true) => self.refocused = true,
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => self.scale = Some(scale_factor),
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    self.events.push(Noted::Key {
                        code,
                        down: event.state == ElementState::Pressed,
                        text: event.text.map(|t| t.to_string()),
                    });
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => self.events.push(Noted::Text(text)),
            WindowEvent::MouseInput { button, state, .. } => self.events.push(Noted::Button {
                button,
                down: state == ElementState::Pressed,
                at: self.at,
            }),
            WindowEvent::MouseWheel { delta, .. } => {
                let notches = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.events.push(Noted::Wheel(notches));
            }
            _ => {}
        }
        false
    }

    /// Note a device event: raw mouse movement.
    pub(crate) fn note_device(&mut self, event: &DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            self.motion.0 += delta.0;
            self.motion.1 += delta.1;
        }
    }

    /// Take what's noted, keeping where the cursor is.
    pub(crate) fn take(&mut self) -> Self {
        let at = self.at;
        let taken = std::mem::take(self);
        self.at = at;
        taken
    }
}

impl App {
    /// Act on what the window was told since the last frame, in order.
    pub(crate) fn apply_inbox(&mut self, event_loop: &ActiveEventLoop) {
        let inbox = self.inbox.take();
        if inbox.close {
            event_loop.exit();
        }
        if let Some(size) = inbox.resized
            && let Some(gfx) = &mut self.gfx
            && gfx.resize(size)
        {
            self.game.set_viewport(size.width, size.height);
        }
        if let Some(gfx) = &self.gfx
            && (inbox.refocused || inbox.scale.is_some())
        {
            let scale = inbox.scale.unwrap_or_else(|| gfx.window.scale_factor());
            icon::set(&gfx.window, scale);
        }
        if inbox.moved {
            // It may be on another monitor now, refreshing at another rate.
            self.read_refresh();
        }
        self.mouse_motion(inbox.motion);
        for event in inbox.events {
            match event {
                Noted::Key { code, down, text } => self.key(event_loop, code, down, text),
                Noted::Text(text) => self.game.input(InputEvent::Text { text }),
                Noted::Button { button, down, at } => {
                    // Pressed where the cursor was then.
                    self.cursor_to(at);
                    self.button(button, down);
                }
                Noted::Wheel(notches) => {
                    self.game.input(InputEvent::Wheel { delta: notches });
                    // A wheel that scrolled GUI doesn't change the speed.
                    if self.flying && !self.game.input_processed() {
                        let speed = self.camera.speed * 1.2f32.powf(notches);
                        self.camera.speed = speed.clamp(1.0, 2000.0);
                    }
                }
                Noted::Unfocused if self.looking => self.look(false),
                Noted::Unfocused => {}
            }
        }
        if let Some(at) = inbox.cursor {
            self.cursor_to(at);
        }
    }

    /// The cursor is at `at`: the game hears it, unless the cursor is held
    /// (its moves, and the warps back, aren't the player's).
    fn cursor_to(&mut self, at: PhysicalPosition<f64>) {
        if self.mouse_held() || at == self.cursor {
            return;
        }
        self.cursor = at;
        let (x, y) = (at.x as f32, at.y as f32);
        self.game.input(InputEvent::MouseMove { x, y });
    }

    fn key(
        &mut self,
        event_loop: &ActiveEventLoop,
        code: KeyCode,
        down: bool,
        text: Option<String>,
    ) {
        // Escape ends typing in a TextBox, or opens Roblox's menu, rather
        // than closing the player.
        let typing = self.game.typing();
        // Repeats only edit a TextBox; the engine drops the rest.
        if let Some(name) = keys::roblox_key(code) {
            let code = name.to_owned();
            self.game.input(InputEvent::Key { code, down });
        }
        if down {
            if code == KeyCode::Escape && !typing && !self.game.core_menu {
                event_loop.exit();
            }
            self.keys.insert(code);
        } else {
            self.keys.remove(&code);
        }
        if down
            && self.game.typing()
            && let Some(text) = text
        {
            self.game.input(InputEvent::Text { text });
        }
    }

    fn button(&mut self, button: MouseButton, down: bool) {
        let number = match button {
            MouseButton::Left => Some(1),
            MouseButton::Right => Some(2),
            MouseButton::Middle => Some(3),
            _ => None,
        };
        if let Some(button) = number {
            self.game.input(InputEvent::MouseButton { button, down });
        }
        // The right button turns the flying camera; the game's camera turns
        // with it on its own.
        if button == MouseButton::Right && down == !self.looking && (self.flying || !down) {
            self.look(down);
        }
    }
}

#[cfg(test)]
mod tests {
    use winit::event::DeviceId;

    use super::*;

    /// Events are only noted, in order: a button carries where the cursor
    /// was when it went down, moves and raw motion are summed, and taking
    /// empties the inbox but keeps where the cursor is.
    #[test]
    fn events_are_noted_in_order() {
        let device_id = DeviceId::dummy();
        let mut inbox = Inbox::default();
        let at = PhysicalPosition::new(10.0, 20.0);
        assert!(!inbox.note(WindowEvent::CursorMoved {
            device_id,
            position: at
        }));
        let state = ElementState::Pressed;
        let button = MouseButton::Right;
        inbox.note(WindowEvent::MouseInput {
            device_id,
            state,
            button,
        });
        inbox.note(WindowEvent::Moved(PhysicalPosition::new(0, 0)));
        for _ in 0..3 {
            inbox.note_device(&DeviceEvent::MouseMotion { delta: (1.0, -2.0) });
        }
        assert!(inbox.note(WindowEvent::RedrawRequested));
        let taken = inbox.take();
        assert_eq!(
            taken.events,
            [Noted::Button {
                button,
                down: true,
                at
            }]
        );
        assert_eq!(taken.motion, (3.0, -6.0));
        assert!(taken.moved);
        assert!(inbox.events.is_empty() && !inbox.moved && inbox.cursor.is_none());
        let state = ElementState::Released;
        inbox.note(WindowEvent::MouseInput {
            device_id,
            state,
            button,
        });
        assert_eq!(
            inbox.events,
            [Noted::Button {
                button,
                down: false,
                at
            }]
        );
    }
}
