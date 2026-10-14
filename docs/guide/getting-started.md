# Getting started

## Building

Rook needs a recent stable Rust toolchain (1.94 or later).

```sh
git clone https://github.com/AsyncFinn/rook
cd rook
cargo build --release
```

This builds two programs in `target/release`: `rook-player` (the windowed player) and `rook` (the command-line tool, see [tools.md](tools.md)).

## Getting a place to open

Use places you made or have the rights to, for example saved from Roblox Studio with *File → Save to File*. Rook opens `.rbxl`, `.rbxm`, `.rbxlx` and `.rbxmx` files. It ships no Roblox content.

## Playing a place

```sh
rook-player place.rbxl                                    # play as a local player
rook-player place.rbxl --run                              # show the server alone (Studio's Run mode)
rook-player place.rbxl --screenshot out.png               # render one frame to a PNG
rook-player place.rbxl --camera 100,80,100 --look-at 0,0,0 --screenshot out.png
rook-player place.rbxl --camera 0,500,0.01 --look-at 0,0,0 --fov 20 --screenshot top.png  # narrow field of view
rook-player place.rbxl --seconds 5 --screenshot out.png   # run scripts for 5 s first
rook-player place.rbxl --script extra.luau                # also run a Luau file as a server script
rook-player place.rbxl --uncapped                         # draw as fast as possible, not at the monitor's refresh rate
rook-player place.rbxl --frame-times                      # print frame and step times every second, and a summary at exit
rook-player place.rbxl --seconds 60 --bench 600           # run 600 frames offscreen and print where the time goes
```

By default the player starts a session with one local player and draws what that player's client sees, as Studio's Play mode does. The server runs inside the player, on its own thread, so nothing needs to be online and a slow server frame doesn't hold up drawing. Output is tagged by the side (server or client) that printed it.

### Frame times

The window title shows, for the last second: frames per second, the median frame time, the slowest 1% of frames (their average), the longest frame, how many frames took longer than 16.7 ms (a 60 Hz frame), and the median and longest server and client steps. `--frame-times` prints the same line to the terminal every second and a summary of the whole run when the window closes. Scripts always run at 60 steps a second, as on Roblox; the frame rate is separate. Without `--uncapped` the window presents at the monitor's refresh rate.

`--bench N` runs N frames offscreen with the server and client in lockstep (the same every run) and prints the time per stage, then the median, slowest 1% and longest server step, client step and both together.

| Control | Action |
|---|---|
| Right mouse button (hold) | Look around |
| W A S D | Move |
| Q / E | Down / up |
| Shift | Move faster |
| Mouse wheel | Change speed |

## Command bar

When the player runs in an interactive terminal, that terminal shows a `>` prompt, like Studio's command bar with the feel of Luau's own REPL. Each entry runs as Luau in the open place; an expression prints its value, and globals you set stay set. An unfinished statement continues on a `>>` line. The arrow keys edit the line and recall earlier entries, and output prints above the prompt without disturbing what you're typing.

```text
> workspace.Gravity
196.2
> for _, p in workspace:GetChildren() do
>>     if p:IsA("BasePart") then p.Anchored = false end
>> end
> :client
```

`:server` and `:client` choose where entries run (the server by default), and `:help` lists the commands. Ctrl+C clears the line; on an empty line it, or Ctrl+D, ends input. `rook run-headless place.rbxl --repl` gives the same prompt without a window, after `--seconds`, until input ends. `--no-repl` turns the prompt off.

The prompt is the `repl` Cargo feature, on by default. Build with `--no-default-features` to leave it out of a build you hand to others. It reads only the local terminal, never the network.

## Next

- [tools.md](tools.md): inspect files, run places without a window, scan an archive, check Rook against Studio.
- [features.md](features.md): everything Rook supports today.
- If a place doesn't load or a script behaves differently from Roblox, please [report it](../../CONTRIBUTING.md#reporting-a-problem).
