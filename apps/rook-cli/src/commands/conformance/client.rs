//! A client case's `--! client input ...` line: the mouse and keyboard
//! steps played while it runs, 0.2 s apart, through Studio's MCP and as
//! Rook's `InputEvent`s.
//!
//! Steps: `x,y` moves the mouse there (GUI coordinates, below the top
//! bar, as the MCP takes them); `down`, `up`, `down2` and `up2` press and
//! release button 1 or 2; `wheelup` and `wheeldown` turn the wheel a
//! notch; `key:Return` presses and releases a key; `text:abc` types text
//! (no spaces); `wait` does nothing for a step.

use rook_engine::InputEvent;
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Step {
    Move(f32, f32),
    Button {
        button: u8,
        down: bool,
    },
    /// Notches; positive is forward (up).
    Wheel(f32),
    Key(String),
    Text(String),
    Wait,
}

/// The steps of a `--! client input ...` line, if the case has one.
pub(super) fn steps(source: &str) -> anyhow::Result<Option<Vec<Step>>> {
    let Some(rest) = source
        .lines()
        .find_map(|l| l.trim().strip_prefix("--! client"))
    else {
        return Ok(None);
    };
    let rest = rest.trim();
    let rest = rest.strip_prefix("input").unwrap_or(rest);
    let steps = rest
        .split_whitespace()
        .map(|token| {
            Ok(match token {
                "down" | "up" => Step::Button {
                    button: 1,
                    down: token == "down",
                },
                "down2" | "up2" => Step::Button {
                    button: 2,
                    down: token == "down2",
                },
                "wheelup" => Step::Wheel(1.0),
                "wheeldown" => Step::Wheel(-1.0),
                "wait" => Step::Wait,
                _ => {
                    if let Some(key) = token.strip_prefix("key:") {
                        Step::Key(key.to_owned())
                    } else if let Some(text) = token.strip_prefix("text:") {
                        Step::Text(text.to_owned())
                    } else {
                        let (x, y) = token
                            .split_once(',')
                            .ok_or_else(|| anyhow::anyhow!("bad input step {token:?}"))?;
                        Step::Move(x.parse()?, y.parse()?)
                    }
                }
            })
        })
        .collect::<anyhow::Result<_>>()?;
    Ok(Some(steps))
}

/// What Rook's window would send for a step. Points get the top bar
/// added back: Rook's window events are in window pixels.
pub(super) fn events(step: &Step) -> Vec<InputEvent> {
    match step {
        Step::Move(x, y) => vec![InputEvent::MouseMove {
            x: *x,
            y: y + rook_gui::GUI_INSET,
        }],
        Step::Button { button, down } => vec![InputEvent::MouseButton {
            button: *button,
            down: *down,
        }],
        Step::Wheel(delta) => vec![InputEvent::Wheel { delta: *delta }],
        Step::Key(code) => [true, false]
            .map(|down| InputEvent::Key {
                code: code.clone(),
                down,
            })
            .to_vec(),
        Step::Text(text) => vec![InputEvent::Text { text: text.clone() }],
        Step::Wait => Vec::new(),
    }
}

/// The MCP calls that play `steps`: runs of mouse steps go to
/// `user_mouse_input` and runs of keyboard steps to `user_keyboard_input`,
/// each step followed by a 0.2 s wait. Mouse buttons and the wheel act
/// where the mouse last moved.
pub(super) fn studio_calls(steps: &[Step]) -> Vec<(&'static str, Vec<Value>)> {
    let wait = json!({ "action": "wait", "wait_time_ms": 200 });
    let mut calls: Vec<(&'static str, Vec<Value>)> = Vec::new();
    let mut at: Option<(f32, f32)> = None;
    for step in steps {
        let (tool, mut actions) = match step {
            Step::Move(x, y) => {
                at = Some((*x, *y));
                let moved = json!({ "action": "moveTo", "x": x, "y": y });
                ("user_mouse_input", vec![moved])
            }
            Step::Button { button, down } => {
                let action = if *down {
                    "mouseButtonDown"
                } else {
                    "mouseButtonUp"
                };
                let side = if *button == 1 { "left" } else { "right" };
                let mut press = json!({ "action": action, "mouse_button": side });
                if let Some((x, y)) = at {
                    press["x"] = json!(x);
                    press["y"] = json!(y);
                }
                ("user_mouse_input", vec![press])
            }
            Step::Wheel(delta) => {
                let action = if *delta > 0.0 {
                    "scrollUp"
                } else {
                    "scrollDown"
                };
                let mut turn = json!({ "action": action });
                if let Some((x, y)) = at {
                    turn["x"] = json!(x);
                    turn["y"] = json!(y);
                }
                ("user_mouse_input", vec![turn])
            }
            Step::Key(code) => (
                "user_keyboard_input",
                vec![json!({ "action": "keyPress", "key_code": code })],
            ),
            Step::Text(text) => (
                "user_keyboard_input",
                vec![json!({ "action": "textInput", "text_inputs": text })],
            ),
            Step::Wait => match calls.last() {
                Some((tool, _)) => (*tool, Vec::new()),
                None => ("user_mouse_input", Vec::new()),
            },
        };
        actions.push(wait.clone());
        match calls.last_mut() {
            Some((last, run)) if *last == tool => run.append(&mut actions),
            _ => calls.push((tool, actions)),
        }
    }
    calls
}
