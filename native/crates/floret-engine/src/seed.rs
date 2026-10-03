//! The patch seed: one number that re-rolls every random / noise node.
//!
//! A patch carries an optional `seed` (see [`crate::patch::PatchFile`]).
//! Every node that draws randomness passes its own seed through
//! [`EvalCtx::seed`](crate::node::EvalCtx::seed) (or [`mix`] where no
//! context is at hand, e.g. inside a `scalar` closure), which mixes the
//! patch seed in. Patch seed 0 — the default, and what every older file
//! loads with — returns the node's seed untouched, so existing patches
//! draw exactly what they always drew; any other patch seed shifts every
//! node's seed by the same scrambled offset, so nodes that differed still
//! differ and the whole picture changes together ("New variation").
//!
//! Plumbing as for [`crate::host`]: `Engine::tick` installs the engine's
//! patch seed in a thread-local for the duration of the tick, which keeps
//! `EvalCtx` / `FrameCtx` literals unchanged. Outside a tick (unit tests,
//! node factories) the patch seed reads as 0.

use crate::nodes::random::hash32;
use std::cell::Cell;

thread_local! {
    static CURRENT: Cell<u32> = const { Cell::new(0) };
}

/// The patch seed of the tick running on this thread (0 outside a tick).
pub fn current() -> u32 {
    CURRENT.with(Cell::get)
}

/// A node's own seed with the current patch seed mixed in. Identity when
/// the patch seed is 0.
pub fn mix(own: f64) -> f64 {
    mix_with(own, current())
}

/// [`mix`] with an explicit patch seed.
///
/// The result stays an integer whenever `own` is one (the offset is a
/// whole number in `1..=2^20`), so nodes that floor, truncate or `| 0` their
/// seed, or step it per octave (`seed + i`), keep working the same way.
/// A non-finite `own` (an unset seed) counts as 0, so it still varies.
pub fn mix_with(own: f64, patch: u32) -> f64 {
    if patch == 0 {
        return own;
    }
    let own = if own.is_finite() { own } else { 0.0 };
    own + f64::from(offset(patch))
}

/// The scrambled shift for a patch seed: neighbouring patch seeds give
/// unrelated offsets, and none is 0.
fn offset(patch: u32) -> u32 {
    (hash32(f64::from(patch), 0x5eed as f64, 0.0) >> 12) + 1
}

/// Installs a patch seed for the current thread until the guard drops
/// (restoring the previous one, so nested engines are fine).
pub struct SeedGuard(u32);

pub fn install(seed: u32) -> SeedGuard {
    SeedGuard(CURRENT.with(|c| c.replace(seed)))
}

impl Drop for SeedGuard {
    fn drop(&mut self) {
        let prev = self.0;
        CURRENT.with(|c| c.set(prev));
    }
}

/// A fresh, non-zero patch seed for "New variation", different from
/// `previous`. Seeds stay below 1 000 000 so they read (and type) easily.
pub fn reroll(previous: u32, entropy: u64) -> u32 {
    let mut x = entropy ^ u64::from(previous).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    loop {
        // splitmix64
        x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = x;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        let seed = (z % 999_999) as u32 + 1;
        if seed != previous {
            return seed;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_zero_is_the_identity() {
        for own in [0.0, 1.0, -7.0, 3.7, 1e10, f64::NAN] {
            let got = mix_with(own, 0);
            assert!(got == own || (got.is_nan() && own.is_nan()), "{own}");
        }
        // Outside a tick the patch seed is 0.
        assert_eq!(mix(42.0), 42.0);
    }

    #[test]
    fn other_seeds_shift_every_node_alike() {
        let (a, b) = (mix_with(1.0, 7), mix_with(2.0, 7));
        assert_ne!(a, 1.0);
        assert_eq!(b - a, 1.0, "distinct node seeds stay distinct");
        assert_eq!(a.fract(), 0.0, "integer seeds stay integers");
        assert_ne!(mix_with(1.0, 7), mix_with(1.0, 8));
        assert_eq!(mix_with(f64::NAN, 7), mix_with(0.0, 7));
    }

    #[test]
    fn install_scopes_the_seed() {
        assert_eq!(current(), 0);
        {
            let _outer = install(5);
            assert_eq!(current(), 5);
            {
                let _inner = install(9);
                assert_eq!(mix(1.0), mix_with(1.0, 9));
            }
            assert_eq!(current(), 5);
        }
        assert_eq!(current(), 0);
    }

    #[test]
    fn reroll_never_repeats_or_returns_zero() {
        let mut s = 0;
        for i in 0..200 {
            let next = reroll(s, i);
            assert!(next != 0 && next != s && next < 1_000_000);
            s = next;
        }
    }
}
