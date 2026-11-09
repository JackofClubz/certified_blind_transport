//! Galois automorphisms `φ_g : a(X) ↦ a(X^g)` of `Z_q[X]/(X^N + 1)` for odd `g`.
//!
//! CKKS rotates its slots by `k` positions with `g = 5^k mod 2N` (and conjugates with `g = 2N - 1`).
//! In the NTT domain `φ_g` is a pure permutation of the evaluation slots (`docs/M3-ntt-math.md` §7).

use crate::modops::neg_mod;
use crate::ntt::{bit_reverse, NttTable};

/// Galois element for a left rotation of the CKKS slots by `k` (negative `k` rotates right).
pub fn galois_element_for_rotation(k: i64, n: usize) -> u64 {
    let two_n = 2 * n as u64;
    let slots = (n / 2) as i64;
    let k = k.rem_euclid(slots) as u64;
    let mut g = 1u64;
    for _ in 0..k {
        g = g * 5 % two_n;
    }
    g
}

/// Apply `φ_g` in the coefficient domain: coefficient `i` moves to `i·g mod 2N`, negated if `≥ N`.
pub fn apply_coeff(a: &[u64], g: u64, q: u64) -> Vec<u64> {
    let n = a.len();
    let two_n = 2 * n as u64;
    assert!(g % 2 == 1, "g must be odd");
    let mut out = vec![0u64; n];
    for (i, &c) in a.iter().enumerate() {
        let idx = (i as u64 * g) % two_n;
        if idx < n as u64 {
            out[idx as usize] = c;
        } else {
            out[(idx - n as u64) as usize] = neg_mod(c, q);
        }
    }
    out
}

/// Permutation `perm` such that `NTT(φ_g(a))[k] = NTT(a)[perm[k]]` for the layout of [`NttTable`].
///
/// Slot `k` holds `a(ψ^{e_k})` with `e_k = 2·rev(k)+1`; `φ_g(a)(ψ^{e}) = a(ψ^{e·g})`, so
/// `perm[k]` is the slot whose exponent is `e_k·g mod 2N`.
pub fn ntt_permutation(tab: &NttTable, g: u64) -> Vec<usize> {
    let n = tab.n;
    let two_n = 2 * n as u64;
    (0..n)
        .map(|k| {
            let e = (tab.eval_exponent(k) * g) % two_n; // odd
            bit_reverse(((e - 1) / 2) as usize, tab.log_n)
        })
        .collect()
}

/// Apply `φ_g` in the NTT domain with a precomputed permutation.
pub fn apply_ntt(a_hat: &[u64], perm: &[usize]) -> Vec<u64> {
    perm.iter().map(|&p| a_hat[p]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primes::ntt_primes_below;
    use crate::SplitMix64;

    #[test]
    fn coefficient_and_ntt_domain_agree() {
        let mut rng = SplitMix64(3);
        let n = 1024;
        let q = ntt_primes_below(50, 1, n, &[])[0];
        let tab = NttTable::new(q, n);
        let a: Vec<u64> = (0..n).map(|_| rng.below(q)).collect();
        for k in [1i64, 2, 3, 7, 64, -1, -16] {
            let g = galois_element_for_rotation(k, n);
            let mut lhs = apply_coeff(&a, g, q);
            tab.forward(&mut lhs);
            let mut fa = a.clone();
            tab.forward(&mut fa);
            let rhs = apply_ntt(&fa, &ntt_permutation(&tab, g));
            assert_eq!(lhs, rhs, "rotation {k}");
        }
    }

    #[test]
    fn automorphisms_compose_and_invert() {
        let mut rng = SplitMix64(4);
        let n = 256;
        let q = ntt_primes_below(40, 1, n, &[])[0];
        let a: Vec<u64> = (0..n).map(|_| rng.below(q)).collect();
        let g1 = galois_element_for_rotation(3, n);
        let g2 = galois_element_for_rotation(5, n);
        let two_n = 2 * n as u64;
        // φ_{g1} ∘ φ_{g2} = φ_{g1·g2}
        let lhs = apply_coeff(&apply_coeff(&a, g2, q), g1, q);
        let rhs = apply_coeff(&a, g1 * g2 % two_n, q);
        assert_eq!(lhs, rhs);
        // rotation by k then by -k is the identity
        let back = apply_coeff(
            &apply_coeff(&a, galois_element_for_rotation(7, n), q),
            galois_element_for_rotation(-7, n),
            q,
        );
        assert_eq!(back, a);
    }
}
