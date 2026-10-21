# Resources

Files about Rook itself rather than any one crate: its icon, used by the apps and this repository's README.

## Icon

`rook.svg` is Rook's icon and the only copy of it: a white rook, tilted 10°, on an orange-to-red tile with a soft shadow. Everything else is made from it at build time.

### How it reaches the apps

`apps/build.rs` is the build script of both apps (`rook-player` and `rook-tools`, through `build = "../build.rs"` in their `Cargo.toml`). For Windows targets it:

1. draws `rook.svg` with [resvg](https://github.com/linebender/resvg) at every size Windows shows an icon at: the title bar's and the taskbar's at 100-300% display scaling, plus 256 pixels for large views;
2. packs those images into an `.ico` in the build's output folder;
3. embeds it in the executable as icon resource 1 with [embed-resource](https://github.com/nabijaczleweli/rust-embed-resource), so Explorer, shortcuts and the taskbar show it.

The player's window loads the same resource (`apps/rook-player/src/app/icon.rs`). Windows keeps two icons per window, a small one for the title bar and Alt+Tab and a big one for the taskbar, and the player loads each at its exact size for the display's scale. Windows then shows an image drawn for that size, rather than shrinking a larger one into a soft one. It reloads them when the window gains focus, because the taskbar button only exists by then, and when the window moves to a display with another scale.

On other platforms the build does nothing and windows get the system's default icon for now.

### Changing it

Edit `rook.svg` and rebuild; there are no generated files to update. Keep it to shapes, gradients and filters: resvg is built without text or embedded images. Check the result at 16 pixels, the title bar's size, where thin details blur first.
