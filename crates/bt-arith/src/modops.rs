//! Word-sized modular arithmetic.
//!
//! Conventions: unless stated otherwise, inputs are reduced (`< q`) and `q < 2^62`.
//! Proofs of the reduction bounds are in `docs/M3-ntt-math.md` §3.

/// `(a + b) mod q` for `a, b < q`.
#[inline(always)]
pub fn add_mod(a: u64, b: u64, q: u64) -> u64 {
    let s = a + b;
    if s >= q {
        s - q
    } else {
        s
    }
}

/// `(a - b) mod q` for `a, b < q`.
#[inline(always)]
pub fn sub_mod(a: u64, b: u64, q: u64) -> u64 {
    if a >= b {
        a - b
    } else {
        a + q - b
    }
}

/// `(-a) mod q` for `a < q`.
#[inline(always)]
pub fn neg_mod(a: u64, q: u64) -> u64 {
    if a == 0 {
        0
    } else {
        q - a
    }
}

/// Reference multiplication `(a * b) mod q` via 128-bit remainder. Slow but obviously correct.
#[inline(always)]
pub fn mul_mod(a: u64, b: u64, q: u64) -> u64 {
    ((a as u128 * b as u128) % q as u128) as u64
}

/// `base^exp mod q` by square-and-multiply.
pub fn pow_mod(mut base: u64, mut exp: u64, q: u64) -> u64 {
    let mut acc = 1 % q;
    base %= q;
    while exp > 0 {
        if exp & 1 == 1 {
            acc = mul_mod(acc, base, q);
        }
        base = mul_mod(base, base, q);
        exp >>= 1;
    }
    acc
}

/// Inverse modulo a **prime** `q` (Fermat): `a^(q-2)`. Panics on `a ≡ 0`.
pub fn inv_mod_prime(a: u64, q: u64) -> u64 {
    assert!(a % q != 0, "zero has no inverse");
    pow_mod(a, q - 2, q)
}

/// A constant `w < q` together with its Shoup companion `w' = floor(w * 2^64 / q)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShoupConst {
    pub w: u64,
    pub w_shoup: u64,
}

impl ShoupConst {
    pub fn new(w: u64, q: u64) -> Self {
        debug_assert!(w < q);
        Self {
            w,
            w_shoup: (((w as u128) << 64) / q as u128) as u64,
        }
    }
}

/// Shoup multiplication, lazy: returns `r ≡ a·w (mod q)` with `r ∈ [0, 2q)`, for **any** `a < 2^64`.
#[inline(always)]
pub fn mul_shoup_lazy(a: u64, c: ShoupConst, q: u64) -> u64 {
    let qhat = ((a as u128 * c.w_shoup as u128) >> 64) as u64;
    a.wrapping_mul(c.w).wrapping_sub(qhat.wrapping_mul(q))
}

/// Shoup multiplication, fully reduced: `a·w mod q` in `[0, q)`.
#[inline(always)]
pub fn mul_shoup(a: u64, c: ShoupConst, q: u64) -> u64 {
    let r = mul_shoup_lazy(a, c, q);
    if r >= q {
        r - q
    } else {
        r
    }
}

/// Barrett reduction for products of two reduced values (HAC 14.42 with base 2).
///
/// With `s = bitlen(q)` and `mu = floor(2^(2s) / q)`: for `x < q^2`,
/// `qhat = floor(floor(x / 2^(s-1)) * mu / 2^(s+1))` satisfies `0 <= x - qhat*q < 3q`.
#[derive(Clone, Copy, Debug)]
pub struct Barrett {
    pub q: u64,
    s: u32,
    mu: u128,
}

impl Barrett {
    pub fn new(q: u64) -> Self {
        assert!(
            (2..(1u64 << 62)).contains(&q),
            "Barrett: need 2 <= q < 2^62"
        );
        let s = 64 - q.leading_zeros();
        Self {
            q,
            s,
            mu: (1u128 << (2 * s)) / q as u128,
        }
    }

    /// Reduce `x < q^2` to `[0, q)`.
    #[inline(always)]
    pub fn reduce(&self, x: u128) -> u64 {
        let qhat = ((x >> (self.s - 1)) * self.mu) >> (self.s + 1);
        let mut r = (x - qhat * self.q as u128) as u64; // < 3q < 2^64
        if r >= self.q {
            r -= self.q;
        }
        if r >= self.q {
            r -= self.q;
        }
        r
    }

    /// `(a * b) mod q` for `a, b < q`.
    #[inline(always)]
    pub fn mul(&self, a: u64, b: u64) -> u64 {
        self.reduce(a as u128 * b as u128)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SplitMix64;

    const QS: [u64; 4] = [
        65537,
        1_152_921_504_606_830_593,
        (1 << 61) - 1,
        4_611_686_018_427_322_369,
    ];

    #[test]
    fn shoup_matches_reference_for_any_a() {
        let mut rng = SplitMix64(1);
        for &q in &QS {
            for _ in 0..20_000 {
                let w = rng.below(q);
                let a = rng.next(); // full 64-bit input: the lazy bound must hold for any a
                let c = ShoupConst::new(w, q);
                let lazy = mul_shoup_lazy(a, c, q);
                assert!(lazy < 2 * q);
                assert_eq!(lazy % q, ((a as u128 * w as u128) % q as u128) as u64);
                assert_eq!(mul_shoup(a % q, c, q), mul_mod(a % q, w, q));
            }
        }
    }

    #[test]
    fn barrett_matches_reference() {
        let mut rng = SplitMix64(2);
        for &q in &QS {
            let br = Barrett::new(q);
            for _ in 0..20_000 {
                let (a, b) = (rng.below(q), rng.below(q));
                assert_eq!(br.mul(a, b), mul_mod(a, b, q));
            }
            assert_eq!(br.mul(q - 1, q - 1), 1); // (-1)^2
        }
    }

    #[test]
    fn inverse_and_pow() {
        for q in [65537u64, (1 << 61) - 1] {
            for a in [1u64, 2, 3, 12345, q - 1] {
                assert_eq!(mul_mod(a, inv_mod_prime(a, q), q), 1);
            }
            assert_eq!(pow_mod(3, q - 1, q), 1); // Fermat
        }
    }
}
