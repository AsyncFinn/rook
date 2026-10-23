//! TweenService, tweens and GuiObject's tween methods, written in Luau
//! over the `std.tween` helpers (`luau/tween.luau`).

/// Each source gets the engine's `std` table.
pub(super) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    Some(match (owner, name) {
        ("TweenService", "Create") => {
            "local t = (...).tween return function(self, ...) return t.create(...) end"
        }
        ("TweenService", "GetValue") => {
            "local t = (...).tween return function(self, alpha, style, direction) return t.getValue(alpha, style, direction) end"
        }
        ("TweenBase", "Play") => "local t = (...).tween return function(self) t.play(self) end",
        ("TweenBase", "Pause") => "local t = (...).tween return function(self) t.pause(self) end",
        ("TweenBase", "Cancel") => "local t = (...).tween return function(self) t.cancel(self) end",
        // The `Internal` forms are Roblox's own scripts' copies (their
        // menu slides in with one), with the same parameters.
        ("GuiObject", "TweenPosition" | "TweenPositionInternal") => {
            r#"local t = (...).tween
            return function(self, position, direction, style, time, override, callback)
                return t.gui(self, { Position = position }, direction, style, time, override, callback)
            end"#
        }
        ("GuiObject", "TweenSize" | "TweenSizeInternal") => {
            r#"local t = (...).tween
            return function(self, size, direction, style, time, override, callback)
                return t.gui(self, { Size = size }, direction, style, time, override, callback)
            end"#
        }
        ("GuiObject", "TweenSizeAndPosition" | "TweenSizeAndPositionInternal") => {
            r#"local t = (...).tween
            return function(self, size, position, direction, style, time, override, callback)
                return t.gui(self, { Size = size, Position = position }, direction, style, time, override, callback)
            end"#
        }
        _ => return None,
    })
}
