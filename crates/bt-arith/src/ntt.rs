//! The negacyclic number-theoretic transform over `R_q = Z_q[X]/(X^N + 1)`.
//!
//! * Forward: merged Cooley–Tukey (twist by ψ folded into the twiddles), natural-order input,
//!   **bit-reversed** output. Output index `k` holds `a(ψ^(2·rev(k)+1))`.
//! * Inverse: merged Gentleman–Sande, bit-reversed input, natural-order output, including the
//!   factor `N^{-1}`.
//! * Butterflies are Harvey-lazy: intermediate values live in `[0, 4q)` (forward) or `[0, 2q)`
//!   (inverse), so we need `q < 2^62`.
//!
//! Derivation and invariants: `docs/M3-ntt-math.md` §4–§5.

use crate::modops::{inv_mod_prime, mul_mod, mul_shoup, mul_shoup_lazy, pow_mod, ShoupConst};
use crate::primes::primitive_2n_root;

/// Reverse the lowest `bits` bits of `x`.
#[inline]
pub fn bit_reverse(x: usize, bits: u32) -> usize {
    if bits == 0 {
        0
    } else {
        x.reverse_bits() >> (usize::BITS - bits)
    }
}

/// Precomputed tables for one prime `q` and ring degree `n`.
#[derive(Clone, Debug)]
pub struct NttTable {
    pub q: u64,
    pub n: usize,
    pub log_n: u32,
    pub psi: u64,
    /// `psi_rev[k] = ψ^{rev(k)}` with Shoup companions.
    psi_rev: Vec<ShoupConst>,
    /// `psi_inv_rev[k] = ψ^{-rev(k)}` with Shoup companions.
    psi_inv_rev: Vec<ShoupConst>,
    n_inv: ShoupConst,
}

impl NttTable {
    pub fn new(q: u64, n: usize) -> Self {
        assert!(
            n.is_power_of_two() && n >= 2,
            "n must be a power of two >= 2"
        );
        assert!(q < (1u64 << 62), "lazy butterflies need q < 2^62");
        let psi = primitive_2n_root(q, n);
        Self::with_root(q, n, psi)
    }

    /// Build tables from a given primitive 2n-th root (useful for cross-checking other libraries).
    pub fn with_root(q: u64, n: usize, psi: u64) -> Self {
        assert_eq!(
            pow_mod(psi, n as u64, q),
            q - 1,
            "psi must be a primitive 2n-th root"
        );
        let log_n = n.trailing_zeros();
        let psi_inv = inv_mod_prime(psi, q);
        let mut psi_rev = Vec::with_capacity(n);
        let mut psi_inv_rev = Vec::with_capacity(n);
        for k in 0..n {
            let e = bit_reverse(k, log_n) as u64;
            psi_rev.push(ShoupConst::new(pow_mod(psi, e, q), q));
            psi_inv_rev.push(ShoupConst::new(pow_mod(psi_inv, e, q), q));
        }
        let n_inv = ShoupConst::new(inv_mod_prime(n as u64 % q, q), q);
        Self {
            q,
            n,
            log_n,
            psi,
            psi_rev,
            psi_inv_rev,
            n_inv,
        }
    }

    /// In-place forward NTT. Input: coefficients in `[0, q)`, natural order.
    /// Output: evaluations in `[0, q)`, bit-reversed order.
    pub fn forward(&self, a: &mut [u64]) {
        assert_eq!(a.len(), self.n);
        let q = self.q;
        let two_q = 2 * q;
        let mut t = self.n;
        let mut m = 1;
        while m < self.n {
            t >>= 1;
            for i in 0..m {
                let w = self.psi_rev[m + i];
                let j1 = 2 * i * t;
                let (lo, hi) = a[j1..j1 + 2 * t].split_at_mut(t);
                for (x, y) in lo.iter_mut().zip(hi.iter_mut()) {
                    // invariant: x, y in [0, 4q)
                    let mut u = *x;
                    if u >= two_q {
                        u -= two_q;
                    } // u in [0, 2q)
                    let v = mul_shoup_lazy(*y, w, q); // v in [0, 2q), ≡ y·w
                    *x = u + v; // [0, 4q)
                    *y = u + two_q - v; // (0, 4q)
                }
            }
            m <<= 1;
        }
        for x in a.iter_mut() {
            if *x >= two_q {
                *x -= two_q;
            }
            if *x >= q {
                *x -= q;
            }
        }
    }

    /// In-place inverse NTT. Input: evaluations in `[0, q)`, bit-reversed order.
    /// Output: coefficients in `[0, q)`, natural order (the factor `N^{-1}` is applied).
    pub fn inverse(&self, a: &mut [u64]) {
        assert_eq!(a.len(), self.n);
        let q = self.q;
        let two_q = 2 * q;
        let mut t = 1;
        let mut m = self.n;
        while m > 1 {
            let h = m >> 1;
            for i in 0..h {
                let w = self.psi_inv_rev[h + i];
                let j1 = 2 * i * t;
                let (lo, hi) = a[j1..j1 + 2 * t].split_at_mut(t);
                for (x, y) in lo.iter_mut().zip(hi.iter_mut()) {
                    // invariant: x, y in [0, 2q)
                    let (u, v) = (*x, *y);
                    let mut s = u + v;
                    if s >= two_q {
                        s -= two_q;
                    } // [0, 2q)
                    *x = s;
                    *y = mul_shoup_lazy(u + two_q - v, w, q); // [0, 2q)
                }
            }
            t <<= 1;
            m = h;
        }
        for x in a.iter_mut() {
            *x = mul_shoup(*x, self.n_inv, q); // Shoup accepts any input < 2^64
        }
    }

    /// Exponent `e` such that output slot `k` of [`Self::forward`] holds `a(ψ^e)`: `e = 2·rev(k) + 1`.
    pub fn eval_exponent(&self, k: usize) -> u64 {
        2 * bit_reverse(k, self.log_n) as u64 + 1
    }

    /// Negacyclic product `a·b mod (X^N + 1, q)` via the NTT.
    pub fn multiply(&self, a: &[u64], b: &[u64]) -> Vec<u64> {
        let (mut fa, mut fb) = (a.to_vec(), b.to_vec());
        self.forward(&mut fa);
        self.forward(&mut fb);
        for (x, y) in fa.iter_mut().zip(&fb) {
            *x = mul_mod(*x, *y, self.q);
        }
        self.inverse(&mut fa);
        fa
    }
}

/// Reference schoolbook negacyclic product, O(N^2). For tests.
pub fn negacyclic_schoolbook(a: &[u64], b: &[u64], q: u64) -> Vec<u64> {
    let n = a.len();
    let mut c = vec![0u64; n];
    for i in 0..n {
        for j in 0..n {
            let p = mul_mod(a[i], b[j], q);
            let k = i + j;
            if k < n {
                c[k] = (c[k] + p) % q;
            } else {
                c[k - n] = (c[k - n] + q - p) % q; // X^N = -1
            }
        }
    }
    c
}

/// Reference evaluation `a(x) mod q` by Horner. For tests.
pub fn eval_poly(a: &[u64], x: u64, q: u64) -> u64 {
    a.iter()
        .rev()
        .fold(0, |acc, &c| (mul_mod(acc, x, q) + c) % q)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primes::ntt_primes_below;
    use crate::SplitMix64;

    fn random_poly(rng: &mut SplitMix64, n: usize, q: u64) -> Vec<u64> {
        (0..n).map(|_| rng.below(q)).collect()
    }

    #[test]
    fn roundtrip_and_convolution_small_and_large() {
        let mut rng = SplitMix64(7);
        for &(n, bits) in &[
            (8usize, 30u32),
            (64, 40),
            (1024, 55),
            (4096, 60),
            (4096, 61),
        ] {
            let q = ntt_primes_below(bits, 1, n, &[])[0];
            let tab = NttTable::new(q, n);
            let a = random_poly(&mut rng, n, q);
            let b = random_poly(&mut rng, n, q);
            // round trip
            let mut x = a.clone();
            tab.forward(&mut x);
            tab.inverse(&mut x);
            assert_eq!(x, a, "roundtrip n={n}");
            // convolution theorem (schoolbook only for moderate n)
            if n <= 1024 {
                assert_eq!(
                    tab.multiply(&a, &b),
                    negacyclic_schoolbook(&a, &b, q),
                    "convolution n={n}"
                );
            }
        }
    }

    #[test]
    fn output_slots_are_evaluations_at_odd_powers() {
        let mut rng = SplitMix64(9);
        let n = 64;
        let q = ntt_primes_below(50, 1, n, &[])[0];
        let tab = NttTable::new(q, n);
        let a = random_poly(&mut rng, n, q);
        let mut fa = a.clone();
        tab.forward(&mut fa);
        for k in 0..n {
            let x = pow_mod(tab.psi, tab.eval_exponent(k), q);
            assert_eq!(fa[k], eval_poly(&a, x, q), "slot {k}");
        }
    }

    #[test]
    fn extreme_inputs_respect_lazy_bounds() {
        // all coefficients q-1: maximises intermediate values
        let n = 4096;
        let q = ntt_primes_below(61, 1, n, &[])[0];
        let tab = NttTable::new(q, n);
        let a = vec![q - 1; n];
        let mut x = a.clone();
        tab.forward(&mut x);
        assert!(x.iter().all(|&v| v < q));
        tab.inverse(&mut x);
        assert_eq!(x, a);
    }

    #[test]
    fn linearity() {
        let mut rng = SplitMix64(11);
        let n = 256;
        let q = ntt_primes_below(45, 1, n, &[])[0];
        let tab = NttTable::new(q, n);
        let a = random_poly(&mut rng, n, q);
        let b = random_poly(&mut rng, n, q);
        let sum: Vec<u64> = a.iter().zip(&b).map(|(x, y)| (x + y) % q).collect();
        let (mut fa, mut fb, mut fs) = (a, b, sum);
        tab.forward(&mut fa);
        tab.forward(&mut fb);
        tab.forward(&mut fs);
        for k in 0..n {
            assert_eq!(fs[k], (fa[k] + fb[k]) % q);
        }
    }
}
