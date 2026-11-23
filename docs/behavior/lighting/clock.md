# The clock

Checked against Studio 0.748 on 2026-11-23.

Lighting's time of day, how scripts set and read it, and where it puts
the sun and the moon. Recorded by the conformance case `lighting/clock`
(`conformance/cases/lighting/clock.luau`); everything below marked
Confirmed is in its expectation.

## Whole seconds after midnight

Roblox keeps the time as whole seconds after midnight (Confirmed).
`ClockTime` 13.999 reads back 13.9989 and `TimeOfDay` `13:59:56`, and
23.9999 reads 23.9997 and `23:59:59`: the seconds are cut off, never
rounded up. `GetMinutesAfterMidnight()` is the seconds over 60
(839.93 at 13:59:56), and `ClockTime` the seconds over 3600. A change
of less than a second stores the same time, so nothing reports a change.

There are three ways to set it, and each takes odd values its own way
(all Confirmed):

- **`ClockTime`** isn't wrapped: anything outside 0 to 24 is midnight.
  24, 25.5, 48.25, -1, -0.25, 1e9 and both infinities all read 0 and
  `00:00:00`. NaN passes that check and lands on 20:45:52 (`ClockTime`
  20.7644), as `SetMinutesAfterMidnight(NaN)` does. A string is taken as
  its number (`"7.5"` is 7:30), and one that isn't a number as 0.
- **`SetMinutesAfterMidnight(minutes)`** wraps into the day: 1500 is
  01:00:00, -30 is 23:30:00, 1e7 is 10:40:00. NaN is 20:45:52, which is
  exactly where -2³¹ seconds wraps to: the seconds go through a 32-bit
  integer, whose conversion turns NaN into its lowest value (Inference;
  minutes out of the 32-bit range are taken to land there too,
  unmeasured). Fractions of a second are cut off towards zero. A string
  is taken as its number; nil raises `Argument 1 missing or nil`.
- **`TimeOfDay`** is read as Boost.Date_Time's `duration_from_string`
  reads a duration (the errors are Boost's), then wrapped into the day,
  and what's stored reads back as `HH:MM:SS`:
  - Fields are split at any of `:`, `-`, `,` and `.`, and empty ones are
    skipped. The first is the hours (any whole number), the next two the
    minutes and the seconds (0 to 65535 each), the fourth fractional
    seconds, which are read and then dropped; any after that are ignored.
    `6:30` is `06:30:00`, `7` is `07:00:00`, `12:30:45.5` and
    `12:30:45.6` are `12:30:45`, `12:61:00` is `13:01:00`, `12:00:75` is
    `12:01:15`, `1:2:3:4` is `01:02:03`.
  - A leading `-` makes the whole time negative, and it wraps: `-1:00:00`
    is `23:00:00`. So do times past a day: `25:00:00` is `01:00:00`,
    `24:00:00` is `00:00:00`, `99999:00:00` is `15:00:00`.
  - A field that isn't digits (`" 08:00:00 "`, `08:00:00am`, `8h`, `abc`)
    raises `bad lexical cast: source type value could not be interpreted
    as target`, and an empty string `invalid string position`. Either way
    the old time stays.
  - A number is taken as its string: 9 is `09:00:00`.

Boost also takes a field's leading `+`, and reads only the first six
digits of the fractional seconds (Inference: Boost's rules, not
measured).

Setting any of the three reports a change of both `ClockTime` and
`TimeOfDay`, on `Changed` and on both properties' `GetPropertyChangedSignal`
(Confirmed). Which of the two fires first wasn't isolated; Rook fires
`ClockTime` first (Inference).

`GeographicLatitude` takes any number and reads back as set: 200, -400,
1e9, NaN and infinity (Confirmed).

## The sun and the moon

`GetSunDirection()` turns the sun around the Z axis once a day, rising
at +X (at 6:00 it's (0.917, 0, -0.399) at latitude 0), on a circle tilted
towards +Z by the latitude less 23.5°, taken as an angle whatever its
size: at hour angle h = (hours − 12) / 24 × 2π and tilt
t = latitude − 23.5°, the direction is (−sin h cos t, cos h cos t, sin t)
(Confirmed at eight times for latitudes 0, 41.733 and -30, and at 14:00
for latitudes 23.5, ±90, 120, 200, -400 and 1e9). Studio works in 32-bit
floats, the tilt as (latitude − 23.5) × π / 180 in that order: at
latitude 1e9 its sun is (-0.498, 0.863, 0.084), which only that rounding
gives. A NaN or infinite latitude gives a NaN direction.

`GetMoonDirection()` is the sun's direction with X and Y negated: the
moon is where the sun is half a day later (Confirmed at every time and
latitude above). `GetMoonPhase()` read 0.75 at midnight and at noon
(Confirmed); Lighting has no date for it to follow, so Rook keeps it
there (Inference).

## In Rook

`rook_types::Clock` (`crates/rook-types/src/clock.rs`) holds the time
and the rules for each way in; `TimeOfDay` stays the stored property, as
files save it, and `ClockTime` and `TimeOfDay` read through it
(`crates/rook-lua/src/api/properties/clock.rs`), so a saved string reads
back normalized too, and one that doesn't read is the default 14:00:00
(Inference). The renderer lights by the same `sun_direction` and
`moon_direction` the methods return (`crates/rook-types/src/clock/sky.rs`),
through [the sun](sun.md)'s light.
