//! Deterministic randomness.
//!
//! Every procedural choice derives from the world seed through a *path*:
//! `world.child("city").child(3).child("lot").child(17)`. A plan reads only
//! from its own path, so adding a random call in one place never changes what
//! another place generates, and any piece of the world can be regenerated on
//! its own without generating its neighbours first.

/// A 64-bit seed. Cheap to copy; derive children instead of sharing an [`Rng`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Seed(pub u64);

/// A value that can extend a seed path.
pub trait SeedKey {
    fn key_hash(&self) -> u64;
}

impl SeedKey for &str {
    fn key_hash(&self) -> u64 {
        hash_bytes(self.as_bytes())
    }
}

impl SeedKey for String {
    fn key_hash(&self) -> u64 {
        hash_bytes(self.as_bytes())
    }
}

macro_rules! int_key {
    ($($t:ty),*) => {$(
        impl SeedKey for $t {
            fn key_hash(&self) -> u64 {
                // Offset so that integer keys never collide with short strings.
                mix64((*self as i64 as u64) ^ 0x5851_f42d_4c95_7f2d)
            }
        }
    )*};
}
int_key!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize);

impl Seed {
    /// The seed one step further down the path.
    pub fn child(self, key: impl SeedKey) -> Seed {
        Seed(combine(self.0, key.key_hash()))
    }

    /// The seed for a grid cell, for things placed on lattices.
    pub fn cell(self, x: i64, y: i64) -> Seed {
        self.child(x).child(y)
    }

    /// A random stream for this seed.
    pub fn rng(self) -> Rng {
        Rng::new(self)
    }

    /// One uniform float in `[0, 1)` straight from the seed.
    pub fn unit(self) -> f64 {
        to_unit(mix64(self.0))
    }
}

/// Avalanching 64-bit mix (the SplitMix64 finaliser).
pub const fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Combines a parent hash and a child key, order-sensitive.
pub const fn combine(parent: u64, key: u64) -> u64 {
    mix64(parent ^ mix64(key.wrapping_add(0x9e37_79b9_7f4a_7c15)).rotate_left(17))
}

/// Stable byte hash (FNV-1a 64 followed by an avalanche).
pub const fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut i = 0;
    while i < bytes.len() {
        h ^= bytes[i] as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
        i += 1;
    }
    mix64(h)
}

fn to_unit(x: u64) -> f64 {
    (x >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

/// A small, fast PRNG (xoshiro256**). Use one per plan step, seeded from that
/// step's path; never keep one across unrelated choices.
#[derive(Clone, Debug)]
pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    pub fn new(seed: Seed) -> Self {
        let mut z = seed.0;
        let mut next = || {
            z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
            mix64(z)
        };
        Self {
            s: [next(), next(), next(), next()],
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        to_unit(self.next_u64())
    }

    /// Uniform in `[lo, hi)`.
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    /// Uniform integer in `[lo, hi)`. Panics if the range is empty.
    pub fn range_int(&mut self, lo: i64, hi: i64) -> i64 {
        assert!(lo < hi, "empty range {lo}..{hi}");
        let span = (hi - lo) as u64;
        // Multiply-shift: unbiased enough for generation and branch-free.
        lo + ((self.next_u64() as u128 * span as u128) >> 64) as i64
    }

    /// Uniform index into a slice of length `len`. Panics if `len` is 0.
    pub fn index(&mut self, len: usize) -> usize {
        self.range_int(0, len as i64) as usize
    }

    /// True with probability `p`.
    pub fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }

    /// A uniformly chosen element. Panics on an empty slice.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.index(items.len())]
    }

    /// An index chosen with probability proportional to its weight.
    /// Panics if no weight is positive.
    pub fn weighted(&mut self, weights: &[f64]) -> usize {
        let total: f64 = weights.iter().filter(|w| **w > 0.0).sum();
        assert!(total > 0.0, "no positive weight");
        let mut roll = self.unit() * total;
        for (i, w) in weights.iter().enumerate() {
            if *w > 0.0 {
                if roll < *w {
                    return i;
                }
                roll -= w;
            }
        }
        weights.iter().rposition(|w| *w > 0.0).unwrap_or(0)
    }

    /// Fisher-Yates shuffle.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.index(i + 1);
            items.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_path_same_seed() {
        let a = Seed(42).child("city").child(3u32);
        let b = Seed(42).child("city").child(3u32);
        assert_eq!(a, b);
    }

    #[test]
    fn paths_are_order_sensitive_and_distinct() {
        let w = Seed(42);
        assert_ne!(w.child("a").child("b"), w.child("b").child("a"));
        assert_ne!(w.child("city"), w.child("City"));
        assert_ne!(w.child(1u32), w.child(2u32));
        assert_ne!(w.child(1u32), w.child("1"));
        assert_ne!(w.cell(1, 2), w.cell(2, 1));
        assert_ne!(Seed(1).child("x"), Seed(2).child("x"));
    }

    #[test]
    fn integer_key_width_does_not_matter() {
        assert_eq!(Seed(7).child(5u8), Seed(7).child(5u64));
        assert_eq!(Seed(7).child(-3i32), Seed(7).child(-3i64));
    }

    /// Golden values: if these change, every existing world changes. Bump
    /// WORLD_VERSION and update them only on purpose.
    #[test]
    fn golden_values() {
        let s = Seed(1234).child("city").child(7u32);
        let mut rng = s.rng();
        let got = (s.0, rng.next_u64(), rng.next_u64());
        let expected = (
            0xd5f1_b99b_b725_9020,
            0x90a2_488d_3a44_9be3,
            0xa56c_8ce5_5a99_adc7,
        );
        assert_eq!(got, expected);
        assert_eq!(hash_bytes(b""), mix64(0xcbf2_9ce4_8422_2325));
    }

    #[test]
    fn rng_is_reproducible() {
        let mut a = Seed(9).rng();
        let mut b = Seed(9).rng();
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn unit_stays_in_range_and_is_roughly_uniform() {
        let mut rng = Seed(5).rng();
        let n = 100_000;
        let mut buckets = [0u32; 10];
        for _ in 0..n {
            let u = rng.unit();
            assert!((0.0..1.0).contains(&u));
            buckets[(u * 10.0) as usize] += 1;
        }
        for b in buckets {
            assert!((9_000..11_000).contains(&b), "bucket {b}");
        }
    }

    #[test]
    fn range_int_covers_bounds() {
        let mut rng = Seed(3).rng();
        let mut seen = [false; 5];
        for _ in 0..1000 {
            let v = rng.range_int(-2, 3);
            assert!((-2..3).contains(&v));
            seen[(v + 2) as usize] = true;
        }
        assert!(seen.iter().all(|s| *s));
    }

    #[test]
    fn weighted_respects_zero_weights() {
        let mut rng = Seed(11).rng();
        for _ in 0..1000 {
            assert_ne!(rng.weighted(&[1.0, 0.0, 2.0]), 1);
        }
    }

    #[test]
    fn shuffle_is_a_permutation() {
        let mut v: Vec<u32> = (0..50).collect();
        Seed(8).rng().shuffle(&mut v);
        let mut sorted = v.clone();
        sorted.sort();
        assert_eq!(sorted, (0..50).collect::<Vec<_>>());
        assert_ne!(v, sorted);
    }
}
