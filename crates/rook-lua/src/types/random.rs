//! `Random`: Luau's own PCG32, so a seed gives the sequences it does in
//! Roblox (measured in Studio 0.746: `docs/behavior/scripting/random.md`).

use mlua::{Lua, MetaMethod, UserData, UserDataFields, UserDataMethods, Value as LuaValue, Vector};

use crate::ctx::args::type_name;

const MULTIPLIER: u64 = 6_364_136_223_846_793_005;
const INCREMENT: u64 = 105;
const WORD: f64 = 4_294_967_296.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LRandom {
    state: u64,
}

impl LRandom {
    pub(crate) fn new(seed: u64) -> Self {
        let mut r = Self { state: 0 };
        r.next_u32();
        r.state = r.state.wrapping_add(seed);
        r.next_u32();
        r
    }

    fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(MULTIPLIER).wrapping_add(INCREMENT);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        xorshifted.rotate_right((old >> 59) as u32)
    }

    /// Uniform in [0, 1).
    pub(crate) fn next_f64(&mut self) -> f64 {
        let low = f64::from(self.next_u32());
        let high = f64::from(self.next_u32());
        (high + low / WORD) / WORD
    }

    /// A whole number below `range`, which fits a word.
    fn below(&mut self, range: u64) -> u64 {
        (u64::from(self.next_u32()) * range) >> 32
    }

    /// Between `a` and `b`, either way round.
    fn next_between(&mut self, a: f64, b: f64) -> f64 {
        a.min(b) + self.next_f64() * (a - b).abs()
    }

    /// Whole numbers from `a` to `b`, either way round, fractions cut off.
    /// A range too wide for a word comes from [`Self::next_f64`] instead.
    fn next_integer(&mut self, a: f64, b: f64) -> f64 {
        let (a, b) = (a.trunc(), b.trunc());
        let (low, high) = (a.min(b), a.max(b));
        let range = high - low + 1.0;
        if range < WORD {
            return low + self.below(range as u64) as f64;
        }
        // Fitted to Studio's numbers: a wide range leaves out its last value
        // unless it holds a power of two of them.
        let scale = if range.log2().fract() == 0.0 {
            range
        } else {
            range - 1.0
        };
        low + (self.next_f64() * scale).floor()
    }

    fn next_unit_vector(&mut self) -> Vector {
        let low = f64::from(self.next_u32());
        let high = f64::from(self.next_u32());
        let angle = std::f64::consts::TAU * (low / WORD);
        let z = 2.0 * ((high + low / WORD) / WORD) - 1.0;
        let r = (1.0 - z * z).sqrt();
        Vector::new((r * angle.cos()) as f32, (r * angle.sin()) as f32, z as f32)
    }

    fn shuffle(&mut self, t: &mlua::Table) -> mlua::Result<()> {
        if t.is_readonly() {
            return Err(mlua::Error::runtime("attempt to modify a readonly table"));
        }
        let n = t.raw_len();
        for i in 1..=n {
            if t.raw_get::<LuaValue>(i)?.is_nil() {
                return Err(mlua::Error::runtime(
                    "Attempted to shuffle table containing nil values. \
                     This is not allowed because the table length may change.",
                ));
            }
        }
        for i in (2..=n).rev() {
            let j = 1 + self.below(i as u64) as usize;
            let (a, b): (LuaValue, LuaValue) = (t.raw_get(i)?, t.raw_get(j)?);
            t.raw_set(i, b)?;
            t.raw_set(j, a)?;
        }
        Ok(())
    }
}

impl UserData for LRandom {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "Random");
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_method_mut("NextNumber", |_, t, (a, b): (Option<f64>, Option<f64>)| {
            Ok(t.next_between(a.unwrap_or(0.0), b.unwrap_or(1.0)))
        });
        m.add_method_mut("NextInteger", |_, t, (a, b): (f64, f64)| {
            Ok(t.next_integer(a, b))
        });
        m.add_method_mut("NextUnitVector", |_, t, ()| Ok(t.next_unit_vector()));
        m.add_method_mut("Shuffle", |_, t, table: mlua::Table| t.shuffle(&table));
        m.add_method("Clone", |_, t, ()| Ok(t.clone()));
        m.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("Random"));
        m.add_meta_method(MetaMethod::Eq, |_, a, b: mlua::UserDataRef<Self>| {
            Ok(*a == *b)
        });
    }
}

/// `Random.new`: no seed is a seed of its own; a seed loses its fraction.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Luau calls it through mlua, which passes arguments by value"
)]
pub(crate) fn random_new(lua: &Lua, seed: LuaValue) -> mlua::Result<LRandom> {
    if seed.is_nil() {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
        return Ok(LRandom::new(now.map_or(0, |d| d.as_nanos() as u64)));
    }
    let number = match &seed {
        LuaValue::String(_) => lua.coerce_number(seed.clone())?,
        LuaValue::Integer(i) => Some(*i as f64),
        LuaValue::Number(n) => Some(*n),
        _ => None,
    };
    match number {
        Some(n) => Ok(LRandom::new(n as i64 as u64)),
        None => Err(mlua::Error::runtime(format!(
            "invalid argument #1 to 'new' (number expected, got {})",
            type_name(&seed)
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Numbers read from Studio 0.746.
    #[test]
    fn a_seed_gives_studios_sequence() {
        let mut r = LRandom::new(0);
        for want in [
            0.551_455_690_474_907_4,
            0.017_231_024_361_130_14,
            0.883_376_920_906_403,
            0.920_948_367_966_525_6,
        ] {
            assert!((r.next_f64() - want).abs() < 1e-15);
        }
        assert_eq!(
            LRandom::new(7).next_integer(1.0, 2_147_483_648.0),
            2_066_166_747.0
        );
    }

    #[test]
    fn unit_vectors_are_studios() {
        let v = LRandom::new(8).next_unit_vector();
        assert!((v.x() - 0.891_154_35).abs() < 1e-6, "{v:?}");
        assert!((v.y() - 0.252_350_12).abs() < 1e-6);
        assert!((v.z() + 0.377_045_45).abs() < 1e-6);
        let v = LRandom::new(9).next_unit_vector();
        assert!((v.x() + 0.731_460_33).abs() < 1e-6, "{v:?}");
        assert!((v.y() + 0.651_293_1).abs() < 1e-6);
        assert!((v.z() - 0.201_947_93).abs() < 1e-6);
    }

    #[test]
    fn integers_swap_their_bounds_and_cut_fractions() {
        let word = |seed| LRandom::new(seed).next_integer(0.0, 9.0);
        assert_eq!(LRandom::new(3).next_integer(9.0, 0.0), word(3));
        assert_eq!(LRandom::new(3).next_integer(0.9, 9.9), word(3));
        // One value still takes a word.
        let mut r = LRandom::new(3);
        assert_eq!(r.next_integer(0.0, 0.0), 0.0);
        assert_ne!(r, LRandom::new(3));
    }
}
