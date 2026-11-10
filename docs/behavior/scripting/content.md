# Content

Checked against Studio 0.747 on 2026-11-10
(`conformance/cases/scripting/content.luau`).

`Content` is what the modern asset properties hold: `ImageContent`,
`AudioContent`, `MouseIconContent`, `IconContent` and the rest. Each has a
legacy string view (`Image`, `SoundId`, `MouseIcon`, `Icon`) that reads and
writes the same value.

## The value

- `typeof` is `Content`, `type` is `userdata`.
- Members: `SourceType` (an `Enum.ContentSourceType`: None, Uri or Object),
  `Uri` (nil unless a URI) and `Object` (nil unless an object). They are
  read-only: assigning one raises `Uri member of Content is read-only and
  cannot be assigned to`. Other names raise `Foo is not a valid member of
  Content`; a number key raises `invalid argument #2 (string expected, got
  number)`.
- `tostring` prints `Content{SourceType=Uri, Uri=rbxasset://d.png}`,
  `Content{SourceType=None}` or `Content{SourceType=Object,
  Object=EditableImage}`.
- `==` compares the values: two `fromUri` of one URI are equal but not
  `rawequal`, and a property read twice gives two userdata. Compared with
  anything else, it is false.
- An empty URI is no content: `Content.fromUri("")` and `Image = ""` read
  back as `Content{SourceType=None}`.

## The `Content` table

A frozen table holding `fromUri`, `fromAssetId`, `fromObject` and `none`
(one shared value). There is no `Content.new`.

Each constructor takes exactly one argument; any other count, a method call
(`Content:fromUri(x)`) included, raises `Invalid number of arguments.`

| Call | Gives |
|---|---|
| `fromUri("a")` | the URI as given, spaces kept |
| `fromUri(1.5)` | the number as `tostring` prints it (`1.5`) |
| `fromUri(nil)`, `fromUri(true)` | `invalid argument #1 to 'fromUri' (string expected, got nil)` |
| `fromAssetId(5)`, `fromAssetId("7")` | `rbxassetid://5`, `rbxassetid://7` |
| `fromAssetId(x)` | `x` rounded half away from zero (4.5 → 5, -0.5 → -1) |
| `fromAssetId(2^63)`, `(-2^63)` | `rbxassetid://-9223372036854775808` |
| `fromAssetId(1e19)`, `(-1e20)` | `rbxassetid://0` |
| `fromAssetId(0/0)`, `(math.huge)` | `Invalid id. Expected a finite value.` |
| `fromAssetId("abc")` | `invalid argument #1 to 'fromAssetId' (number expected, got string)` |
| `fromObject(nil)` | `Expected Object, got nil.` |
| `fromObject(part)` | `invalid argument #1 to 'fromObject' (Object expected, got Instance)` |

`fromObject` takes an `Object`, such as an EditableImage, which isn't an
Instance. Rook has no Object types yet, so it refuses everything.

Past 2^63 Rook gives 0 for any id; Studio was only seen giving it for 1e19
and beyond (Inference: a range check that lets 2^63 itself through).

## Properties

Assigning anything but a Content to a Content property raises `Unable to
assign property ImageContent. Content expected, got string`, and a Content
to its legacy view raises `Unable to assign property Image. ContentId
expected, got Content`.
