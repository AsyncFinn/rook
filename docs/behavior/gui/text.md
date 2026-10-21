# GUI text

Checked against Studio 0.743 on 2026-10-21.

[M8](../../roadmap/m8-gui.md) step 2. TextLabel is in 550 archive files, TextButton in 456;
Hint in 145 and Message in 26. Measured in Studio 0.743 on 2026-10-21,
mostly in edit mode on Baseplate, by reading `TextBounds` and `TextFits`
of TextLabels in a ScreenGui under StarterGui. The conformance case
`gui/text` holds the decisive values; Rook passes it.

Code: `rook-gui`'s `text` module (fonts, shaping, lines, rich text,
glyph masks and distance fields) and `draw/text.rs`; the glyph atlas in
`rook-render/src/renderer/gui/atlas.rs`; `Font` in Luau in
`rook-lua/src/types/font.rs`; the `Font` and `FontSize` views in
`rook-lua/src/api/properties/text.rs`.

## Fonts

- A text object's font is its `FontFace`: a family file (JSON listing
  faces by weight and style, each a `rbxasset://` file in the install or
  an `rbxassetid://` asset), a weight and a style. Rook picks the face
  nearest the weight in the right style by CSS's rules (Inference).
  Install files load at once; asset ids load through the engine's
  content loader, and their text waits in Source Sans until then.
- **`Enum.Font`** items are `FontFace`s: `Font.fromEnum` gave the family,
  weight and style of every item (`rook_types::LEGACY_FONTS`). Setting
  `Font` sets `FontFace`; reading it gives the item whose face it is, or
  `Unknown`.
- **Stand-ins**: the install has no `Arial.json` or `GothamSSm.json`.
  Arial's `TextBounds` equal Arimo's and Gotham's equal Montserrat's at
  every size tried, so Rook draws them with those.
- **Legacy** (the font of every TextLabel made with `Instance.new`, and
  of old places) is Arimo drawn half as big again: its `TextBounds` at
  `TextSize` 24 are Arial's at 36, at every size from 1 to 100, and a
  line is 1.5 × `TextSize` tall (19 at 13: the height is floored).
- **`TextSize`** is kept within 1 to 100 (500 read 100, −3 read 1). The
  default is 8 for a TextLabel, TextButton or TextBox made with
  `Instance.new`.
- **`FontSize`** is a view of `TextSize`: setting `Size36` makes it 36,
  and 13 reads as `Size14`, the smallest item no smaller. Files from
  before `TextSize` save only `FontSize`; loading takes `TextSize` from
  it (Inference: newer files save both, agreeing).

## Widths

- A line of text is `TextSize` tall (Legacy: 1.5 ×), and the font is
  scaled so its hhea ascent to descent is that height. Source Sans at
  100 is 79.55 pixels to the em, Arimo 89.5.
- **Every glyph's advance is a whole number of pixels**: ten H's are
  always ten times one H, at every size from 8 to 100. Kerning applies:
  Roblox shapes with HarfBuzz ([research](../../design/research.md)), and
  Rook with HarfRust, its Rust port.
- The advance is close to the scaled advance rounded up. Fitting five
  letters at every size 8 to 100 in Source Sans and Arial, rounding up
  matches about nine glyphs in ten; the rest are a pixel off either way,
  by a pattern that depends on the glyph (hinting). Neither skrifa's
  TrueType interpreter nor its autohinter, at whole or fractional sizes,
  reproduces it, so Rook rounds up (Approximation): strings come out
  within a few pixels of Studio's, exact at most sizes above 40.
- `TextBounds` counts spaces at the end of a line (`"a   "` is 45 wide
  at Legacy 24, 18 for the `a`).

## Lines

- A newline starts a line, also at the end (`"A\n"` is two lines tall);
  empty text is one line tall and 0 wide.
- **`LineHeight`**: n lines of height h are h + (n − 1) × h ×
  `LineHeight` tall (`A\nB` at Legacy 24 and 1.5: 90).
- **`TextWrapped`** breaks lines before a word that would pass the
  width; a word wider than the whole width is split between letters, and
  then `TextFits` is false. Only the lines that fit the object's height
  count: a word split over seven lines in a label five lines tall reports
  five lines' bounds. Without wrapping, text runs past the object and
  `TextFits` is false.
- Inference: alignment puts lines on whole pixels, and spaces where a
  line wrapped don't count to its width.

## TextScaled

The text gets the biggest whole-pixel line height up to 100 at which it,
wrapped to the object's width, fits without splitting a word: "Hi" in a
200 × 50 Legacy label is 50 tall, in 600 × 400 it stops at 100 (Legacy's
`TextSize` would then be 66.7). It wraps whether or not `TextWrapped` is
on ("Hello, world!" in 100 × 200 is two 41-pixel lines; Rook's rounding
makes "world!" two pixels too wide there and picks 40).

## AutomaticSize

A text object grows to its `TextBounds` inside its padding, along with
its children: "Hello, world!" at Source Sans 24 with `AutomaticSize` XY
and no size is 107 × 24. Wrapped text growing on Y wraps at the object's
width (150 wide, five Legacy lines: 150 × 180). Inference: growing on X
lays text out unwrapped.

## RichText

The tags of creator-docs' rich text page: `b`, `i`, `u`, `s`, `font`
(`color` as `#RRGGBB` or `rgb()`, `size`, `face`, `family`, `weight`,
`transparency`), `stroke`, `mark`, `br`, `uppercase`/`uc`,
`smallcaps`/`sc`, comments, and the escapes `&lt;` `&gt;` `&quot;`
`&apos;` `&amp;`. Measured: `<b>` widens the run, `<font size>` makes
its line as tall as its biggest text (`A<font size="40">B</font>` at
Legacy 24: 58 × 60), escapes measure as their character, and with
`RichText` off the tags measure as text. `ContentText` is the text
without its markup. Inference: markup that doesn't parse shows as it is;
small caps are drawn as capitals (Approximation).

## Drawing

- **SDF or bitmap**: Studio's flags (`StudioAppSettings.json`, 0.743)
  turn signed distance field fonts on (`FFlagUseSDFForFonts3`,
  `DFFlagEnableSDFFontUsage2`, `DFFlagSDFTextSurfaceGui`) with
  `DFIntSDFBitmapFallbackThreshold` 34, `DFIntSDFSpread` 25, two field
  sizes (`DFFlagTwoSDFBuckets`) and a deny list of RobotoMono at weight
  100; its UI pixel shader has a bitmap mode and a distance field mode
  ([world GUI](world.md#drawing)). Inference: screen text 34 pixels or
  bigger is drawn from distance fields, smaller from bitmaps. Rook does
  so, by the size drawn (UIScales count); world GUI text always uses
  fields. Not yet: the deny list.
- Bitmap glyphs are drawn from their outlines (unhinted) into an 8-bit
  atlas at their size, on whole pixels unless the object is turned;
  field glyphs are stretched from one field per glyph. Either is drawn
  over the object's background and clipped with it, in `TextColor3` at
  `TextTransparency`.
- Inference: `TextStrokeTransparency` below 1 outlines each glyph a pixel
  wide in `TextStrokeColor3` (the glyph drawn again under it, moved each
  way), fading with the text; underlines and strikethroughs are a
  sixteenth of the line thick, at least a pixel.
- Inference: an empty TextBox shows `PlaceholderText` in
  `PlaceholderColor3`.

## Hint and Message

Studio's captures leave GUI out, and the engine draws these itself
(nothing in CoreGui holds their text), so they can't be measured by
script. Observed by eye: a Hint is an opaque black bar across the top of
the window, ignoring the GUI inset (it covers the top bar's strip), with
white text; an empty Hint still shows its bar, while a Message with no
text shows nothing, not even its gray. Hints and Messages overlap by
when they were made, the newest on top. Inference: the bar is 20 pixels tall with Legacy 12 text; a
Message in Workspace or the local player covers the screen in gray at
half transparency with its text in white Legacy 18, wrapped and
centered. Both draw over all GUI.

## Not yet

`TextTruncate`, `TextDirection`, `OpenTypeFeatures`, localization, and
fonts' own fallbacks for characters a face lacks (emoji, CJK).
