//! Primality testing, NTT-friendly prime search and primitive 2N-th roots of unity.

use crate::modops::{mul_mod, pow_mod};

/// Deterministic Miller–Rabin for all `n < 2^64`.
///
/// The first twelve primes (2..=37) as bases are a proven deterministic witness set for
/// `n < ψ_12 = 318_665_857_834_031_151_167_461 ≈ 3.2·10^23` (Sorenson and Webster, 2015),
/// which covers all of `u64` (`2^64 ≈ 1.8·10^19`).
pub fn is_prime(n: u64) -> bool {
    const BASES: [u64; 12] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];
    if n < 2 {
        return false;
    }
    for &p in &BASES {
        if n % p == 0 {
            return n == p;
        }
    }
    let r = (n - 1).trailing_zeros();
    let d = (n - 1) >> r;
    'witness: for &a in &BASES {
        let mut x = pow_mod(a, d, n);
        if x == 1 || x == n - 1 {
            continue;
        }
        for _ in 1..r {
            x = mul_mod(x, x, n);
            if x == n - 1 {
                continue 'witness;
            }
        }
        return false;
    }
    true
}

/// The `count` largest primes `q < 2^bits` with `q ≡ 1 (mod 2n)`, skipping `exclude`, in descending order.
pub fn ntt_primes_below(bits: u32, count: usize, n: usize, exclude: &[u64]) -> Vec<u64> {
    assert!((2..=62).contains(&bits), "bits must be in 2..=62");
    let step = 2 * n as u64;
    let mut k = ((1u64 << bits) - 1) / step;
    let mut out = Vec::with_capacity(count);
    while out.len() < count {
        assert!(k > 0, "ran out of candidates");
        let q = k * step + 1;
        if q < (1u64 << bits) && is_prime(q) && !exclude.contains(&q) {
            out.push(q);
        }
        k -= 1;
    }
    out
}

/// `count` NTT-friendly primes **close to `2^bits`**, alternating just above and just below,
/// so that their product stays close to `2^(bits*count)`. This keeps the CKKS scale stable
/// across rescalings (`docs/05` §7).
pub fn ntt_primes_near(bits: u32, count: usize, n: usize, exclude: &[u64]) -> Vec<u64> {
    assert!((2..62).contains(&bits));
    let step = 2 * n as u64;
    let center = 1u64 << bits;
    let (mut up, mut down) = (center / step + 1, center / step);
    let mut out = Vec::with_capacity(count);
    let mut take_up = true;
    while out.len() < count {
        let cand = if take_up {
            loop {
                let q = up * step + 1;
                up += 1;
                if is_prime(q) && !exclude.contains(&q) && !out.contains(&q) {
                    break q;
                }
            }
        } else {
            loop {
                assert!(down > 0);
                let q = down * step + 1;
                down -= 1;
                if is_prime(q) && !exclude.contains(&q) && !out.contains(&q) {
                    break q;
                }
            }
        };
        out.push(cand);
        take_up = !take_up;
    }
    out
}

/// A primitive `2n`-th root of unity modulo the prime `q` (requires `q ≡ 1 mod 2n`, `n` a power of two).
///
/// Deterministic: tries `x = 2, 3, …` and returns the first `psi = x^((q-1)/(2n))` with
/// `psi^n ≡ -1`. Since the order of `psi` divides `2n` (a power of two) and `psi^n ≠ 1`,
/// its order is exactly `2n` (`docs/M3-ntt-math.md` §2).
pub fn primitive_2n_root(q: u64, n: usize) -> u64 {
    assert!(n.is_power_of_two());
    let two_n = 2 * n as u64;
    assert_eq!(q % two_n, 1, "q must be 1 mod 2n");
    for x in 2..q {
        let psi = pow_mod(x, (q - 1) / two_n, q);
        if pow_mod(psi, n as u64, q) == q - 1 {
            return psi;
        }
    }
    unreachable!("a prime q ≡ 1 mod 2n always has a primitive 2n-th root")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn miller_rabin_matches_sieve() {
        const LIMIT: usize = 200_000;
        let mut sieve = vec![true; LIMIT];
        sieve[0] = false;
        sieve[1] = false;
        for i in 2..LIMIT {
            if sieve[i] {
                let mut j = i * i;
                while j < LIMIT {
                    sieve[j] = false;
                    j += i;
                }
            }
        }
        for (n, &p) in sieve.iter().enumerate() {
            assert_eq!(is_prime(n as u64), p, "n = {n}");
        }
    }

    #[test]
    fn miller_rabin_known_values() {
        // strong pseudoprimes to several small bases must be rejected
        for c in [
            3_215_031_751u64,
            2_152_302_898_747,
            3_474_749_660_383,
            341_550_071_728_321,
            3_825_123_056_546_413_051,
        ] {
            assert!(!is_prime(c), "{c}");
        }
        for p in [(1u64 << 61) - 1, 18_446_744_073_709_551_557] {
            assert!(is_prime(p), "{p}");
        }
    }

    #[test]
    fn ntt_primes_are_ntt_friendly() {
        let n = 4096;
        let qs = ntt_primes_below(60, 4, n, &[]);
        let ps = ntt_primes_near(50, 12, n, &qs);
        for &q in qs.iter().chain(&ps) {
            assert!(is_prime(q));
            assert_eq!(q % (2 * n as u64), 1);
        }
        let mut all = qs.clone();
        all.extend(&ps);
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 16, "all distinct");
        // product of the near-2^50 primes stays within a tiny relative distance of 2^600
        let log2: f64 = ps.iter().map(|&q| (q as f64).log2()).sum();
        assert!((log2 - 600.0).abs() < 1e-3, "log2 product = {log2}");
    }

    #[test]
    fn primitive_root_has_exact_order() {
        let n = 4096;
        for q in ntt_primes_below(60, 3, n, &[]) {
            let psi = primitive_2n_root(q, n);
            assert_eq!(pow_mod(psi, 2 * n as u64, q), 1);
            assert_eq!(pow_mod(psi, n as u64, q), q - 1);
        }
    }
}
