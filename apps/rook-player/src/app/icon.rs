//! The window's title bar and taskbar icons, loaded from the icon the build
//! embeds in the executable (resources/README.md).

use winit::event::WindowEvent;
use winit::window::Window;

/// Reload the window's icons when it gains focus or moves to a display with
/// another scale.
///
/// Windows makes a window's taskbar button a moment after the window, and an
/// icon set before then never reaches it; by the time the window has focus
/// the button exists.
pub(crate) fn on_event(window: &Window, event: &WindowEvent) {
    match *event {
        WindowEvent::Focused(true) => set(window, window.scale_factor()),
        WindowEvent::ScaleFactorChanged { scale_factor, .. } => set(window, scale_factor),
        _ => {}
    }
}

/// Load each icon at its exact size for `scale`, so Windows shows the
/// embedded image drawn for that size instead of shrinking a bigger one
/// (which looks soft).
#[cfg(windows)]
pub(crate) fn set(window: &Window, scale: f64) {
    use winit::dpi::PhysicalSize;
    use winit::platform::windows::{IconExtWindows, WindowExtWindows};
    use winit::window::Icon;

    let icon = |side: f64| {
        let side = (side * scale).round() as u32;
        Icon::from_resource(1, Some(PhysicalSize::new(side, side))).ok()
    };
    // Big first: Windows makes its own small icon from the big one, which
    // the small one then replaces.
    window.set_taskbar_icon(icon(32.0));
    window.set_window_icon(icon(16.0));
}

#[cfg(not(windows))]
pub(crate) fn set(_: &Window, _: f64) {}
