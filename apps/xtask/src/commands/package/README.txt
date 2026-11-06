Rook runs Roblox places (.rbxl and .rbxlx files) on your own machine.
It is an independent project, not made by or connected to Roblox.

To play: drag a place onto rook.exe, or open rook.exe and pick one.

From a terminal, `rook place.rbxl` opens a place, and `rook --help` lists
the tools for looking inside place files.

Rook downloads what it needs from Roblox as places ask for it, and keeps it
in %LOCALAPPDATA%\Rook, with its logs and settings. To remove Rook, delete
this folder and that one.

Some old games have lost their music, because Roblox made those sounds
private. To bring one back, put your own copy of the song in
%LOCALAPPDATA%\Rook\overrides, named after the sound's id, such as
1234567.mp3, and start the place again. A sound that won't load prints its
id in the log; the logs are in %LOCALAPPDATA%\Rook\logs.
