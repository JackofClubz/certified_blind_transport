//! `bt-arith`: the arithmetic foundation of the CKKS layer.
//!
//! * [`modops`]: word-sized modular arithmetic (Shoup and Barrett reduction).
//! * [`primes`]: deterministic primality testing, NTT-friendly prime search, primitive roots.
//! * [`ntt`]: the negacyclic number-theoretic transform over `Z_q[X]/(X^N + 1)`,
//!   with Harvey lazy butterflies (see `docs/M3-ntt-math.md`).
//! * [`automorphism`]: the Galois automorphisms `X -> X^g` used for CKKS slot rotations.
//! * [`rns`]: residue-number-system polynomials and approximate basis conversion.
//!
//! All moduli must satisfy `q < 2^62` (lazy values live in `[0, 4q)`) and
//! `q ≡ 1 (mod 2N)` for the NTT.

// Index loops mirror the maths (a[i], psi_rev[m+i]); `%` is kept over `is_multiple_of` for older toolchains.
#![allow(clippy::needless_range_loop, clippy::manual_is_multiple_of)]

pub mod automorphism;
pub mod modops;
pub mod ntt;
pub mod primes;
pub mod rns;

/// Deterministic SplitMix64 generator, for tests and examples only (never for secrets).
#[doc(hidden)]
pub struct SplitMix64(pub u64);
impl SplitMix64 {
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn below(&mut self, bound: u64) -> u64 {
        ((self.next() as u128 * bound as u128) >> 64) as u64
    }
}
