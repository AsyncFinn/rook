# GUI images

Checked against Studio 0.743 on 2026-10-22.

[M8](../../roadmap/m8-gui.md) step 4. ImageLabel is in 458 archive files. Measured in
Studio 0.743 on 2026-10-22 in edit mode on Baseplate, from
`screen_capture` shots of ImageLabels on an 800 × 600 SurfaceGui (Studio's
captures leave ScreenGuis out; world GUI lays out and draws as a ScreenGui
does), at FieldOfView 20 from 44.69 studs, so a canvas pixel is a screen
pixel. The test images were made for it: a 64 × 48 one in nine colored
regions (corners 16 pixels, an orientation mark in the middle, the bottom
edge half transparent) and a 100 × 40 one striped every 5 pixels. Rook's
screenshots of the same scene differ only along edges (JPEG and rounding).

Code: `rook-gui`'s `draw/image.rs` (the pieces each `ScaleType` makes) and
`images.rs` (which images are loaded); `rook-view`'s `gui.rs` loads them
with the scene's other files; the renderer draws them through
`renderer/gui/images.rs` and `shaders/gui_paint.wgsl`.

## Loading

- `Image` is a view of `ImageContent`, as `Texture` is of `TextureContent`
  ([textures and decals](../rendering/textures-and-decals.md)). Images load
  as decals' do (the install's `rbxasset://` files, asset delivery,
  content folders); until one arrives, or if it fails, the object draws
  only its background.
- `IsLoaded` turns true once the image has loaded (an ImageLabel in
  StarterGui read true in edit mode). Rook loads the images of every
  ImageLabel and ImageButton a client lays out, shown or not, so scripts
  waiting on `IsLoaded` to show one don't wait forever; a server loads
  none, and its `IsLoaded` stays false (Inference).
- Defaults from `Instance.new`: `ImageColor3` white, `ImageTransparency`
  0, ScaleType Stretch, `SliceCenter` (0, 0, 0, 0), `SliceScale` 1,
  `TileSize` {1, 0}, {1, 0}, `ImageRectOffset` and `ImageRectSize`
  (0, 0), ResampleMode Default.

## Drawing

- The image draws over the background, tinted by `ImageColor3` at
  `ImageTransparency`, and turns with the object. ImageButtons draw as
  ImageLabels (their hover and press images come with input, step 5).
- **ResampleMode** Default blends texels (a 4 × 4 part shown 120 pixels
  wide shades smoothly from one to the next); Pixelated shows each texel's
  color, in 30-pixel squares.
- **ImageRectOffset** and **ImageRectSize** pick part of the image, in its
  pixels; a size of (0, 0) is the whole image. A rect past the image's
  edge is cut there (offset (32, 24), size 64 × 48 showed only the
  image's bottom right quarter, stretched), a negative size mirrors the
  image (offset (64, 0), size −64 × 48: left to right), and a size of 0 on
  one axis shows a line of texels stretched (size 0 × 16: the column at
  x = 0, all red).
- Where a blended rect meets the rest of the image, Studio blends in the
  texels just past it; so does Rook.

## ScaleType

- **Stretch**: the rect fills the object.
- **Fit**: the rect, keeping its shape, as big as fits, centered (64 × 48
  in 180 × 120: 160 × 120).
- **Crop**: the rect, keeping its shape, as small as covers the object,
  centered and cut (64 × 48 in 180 × 120: 7.5 pixels cut top and bottom).
- **Tile**: copies of the image `TileSize` big (its scale of the object's
  size), from the object's top left, the last ones cut. A `TileSize` of 0
  draws the image once over the object. With an ImageRect it doesn't tile
  but stretches the rect.

### Slice

Nine pieces: the corners at their own size times `SliceScale`, the edges
stretched along them and the middle stretched both ways.

- `SliceCenter` is the middle piece, from the rect's top left (a center of
  (8, 8, 40, 24) in a rect from (16, 16) drew the image's (24, 24) to
  (56, 40) as the middle); corners given the wrong way round are swapped.
- (0, 0, 0, 0), the default, is taken as it is: the whole image is the
  bottom right corner, drawn at its size, and the middle and the edges are
  the image's first row and column of texels stretched.
- Corners too big for the object shrink together, keeping their shape,
  until they fit both ways: in 40 × 30, the 16-pixel corners drew 15
  (30 / 32), leaving no middle row; in 24 × 110, 12.
- A `SliceScale` of 0 fills the object with the bottom right corner
  (Observed; negative scales are taken the same way, Inference).
