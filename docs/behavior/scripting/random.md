# Random

Checked against Studio 0.746 on 2026-11-06.

`Random` is Luau's PCG32, so a seed gives the same numbers in Rook as in
Roblox (`rook_lua::types::random`; `conformance/cases/scripting/random.luau`).

- *The generator.* 64-bit state, multiplier 6364136223846793005, increment
  105, PCG's RXS output. Seeding: state 0, one step, add the seed, one step.
- *The seed.* A number's whole part as a 64-bit integer: 1.5 seeds as 1, -1
  and 2^32 + 5 are taken whole. A numeric string coerces. No seed, or nil,
  picks one. Anything else errors:
  `invalid argument #1 to 'new' (number expected, got string)`.
- *`NextNumber`.* Two words a and b: `(b + a / 2^32) / 2^32`, from 0 up to 1.
  `NextNumber(min, max)` is `low + u * (high - low)`, whichever way round the
  bounds come.
- *`NextInteger`.* Both arguments lose their fractions, and the larger is the
  top. A range under 2^32 takes one word, `(word * range >> 32) + min`, even
  when it holds one value. A wider one is `floor(NextNumber() * scale) + min`,
  where the scale is the range's size, or one less unless that's a power of
  two (fitted to 2^32 to 2^52, so Inference past what was fitted).
- *`NextUnitVector`.* Two words a and b: the angle is `2π * a / 2^32`, z is
  `2 * ((b + a / 2^32) / 2^32) - 1`, and x and y are `sqrt(1 - z²)` times the
  angle's cosine and sine.
- *`Shuffle`.* Fisher-Yates from the end: for i from n down to 2, swap with
  `1 + (word * i >> 32)`, so n - 1 words. A hole errors, `Attempted to shuffle
  table containing nil values. This is not allowed because the table length
  may change.`; a frozen table errors `attempt to modify a readonly table`; a
  dictionary has no length and stays as it is.
- *Comparing.* `==` compares state: `Random.new(1) == Random.new(1)`, until
  one draws. `tostring` gives `Random`.
