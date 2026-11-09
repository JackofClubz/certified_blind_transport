//! RNS polynomials and approximate fast basis conversion (`docs/plan/05` §2.4–2.5).

use crate::modops::{add_mod, inv_mod_prime, mul_mod, neg_mod, sub_mod, Barrett};
use crate::ntt::NttTable;

/// An RNS basis: a list of pairwise-coprime moduli with NTT tables and conversion constants.
#[derive(Clone, Debug)]
pub struct Basis {
    pub moduli: Vec<u64>,
    pub n: usize,
    tables: Vec<NttTable>,
    barrett: Vec<Barrett>,
    /// `(B / b_i)^{-1} mod b_i`
    hat_inv: Vec<u64>,
}

impl Basis {
    pub fn new(moduli: Vec<u64>, n: usize) -> Self {
        assert!(!moduli.is_empty());
        for (i, &qi) in moduli.iter().enumerate() {
            for (j, &qj) in moduli.iter().enumerate() {
                if i != j {
                    assert_ne!(qi, qj, "moduli must be distinct");
                }
            }
        }
        let tables: Vec<NttTable> = moduli.iter().map(|&q| NttTable::new(q, n)).collect();
        let barrett: Vec<Barrett> = moduli.iter().map(|&q| Barrett::new(q)).collect();
        let r = moduli.len();
        let mut hat_inv = Vec::with_capacity(r);
        for i in 0..r {
            let mut hat = 1u64;
            for (j, &qj) in moduli.iter().enumerate() {
                if j != i {
                    hat = mul_mod(hat, qj % moduli[i], moduli[i]);
                }
            }
            hat_inv.push(inv_mod_prime(hat, moduli[i]));
        }
        Self {
            moduli,
            n,
            tables,
            barrett,
            hat_inv,
        }
    }

    pub fn len(&self) -> usize {
        self.moduli.len()
    }

    pub fn is_empty(&self) -> bool {
        self.moduli.is_empty()
    }

    pub fn table(&self, i: usize) -> &NttTable {
        &self.tables[i]
    }

    pub fn barrett(&self, i: usize) -> &Barrett {
        &self.barrett[i]
    }

    pub fn hat_inv(&self, i: usize) -> u64 {
        self.hat_inv[i]
    }

    /// `(B / b_i) mod dest_q`
    pub fn hat_mod(&self, i: usize, dest_q: u64) -> u64 {
        let mut acc = 1u64;
        for (j, &qj) in self.moduli.iter().enumerate() {
            if j != i {
                acc = mul_mod(acc, qj % dest_q, dest_q);
            }
        }
        acc
    }

    /// `B mod dest_q`
    pub fn product_mod(&self, dest_q: u64) -> u64 {
        let mut acc = 1u64;
        for &qj in &self.moduli {
            acc = mul_mod(acc, qj % dest_q, dest_q);
        }
        acc
    }
}

/// A polynomial in an RNS basis. `limbs[i][k]` is coefficient (or NTT slot) `k` modulo `q_i`.
#[derive(Clone, Debug)]
pub struct RnsPoly {
    pub limbs: Vec<Vec<u64>>,
    pub ntt_form: bool,
}

impl RnsPoly {
    pub fn zero(basis: &Basis) -> Self {
        let n = basis.n;
        Self {
            limbs: vec![vec![0u64; n]; basis.len()],
            ntt_form: false,
        }
    }

    pub fn from_i64(basis: &Basis, coeffs: &[i64]) -> Self {
        assert_eq!(coeffs.len(), basis.n);
        let mut p = Self::zero(basis);
        for (i, &q) in basis.moduli.iter().enumerate() {
            for (k, &c) in coeffs.iter().enumerate() {
                p.limbs[i][k] = signed_to_mod(c, q);
            }
        }
        p
    }

    pub fn n(&self) -> usize {
        self.limbs[0].len()
    }

    pub fn nlimbs(&self) -> usize {
        self.limbs.len()
    }

    pub fn truncate_limbs(&mut self, keep: usize) {
        self.limbs.truncate(keep);
    }

    pub fn ntt(&mut self, basis: &Basis) {
        assert!(!self.ntt_form);
        assert_eq!(self.nlimbs(), basis.len());
        for i in 0..self.nlimbs() {
            basis.table(i).forward(&mut self.limbs[i]);
        }
        self.ntt_form = true;
    }

    pub fn intt(&mut self, basis: &Basis) {
        assert!(self.ntt_form);
        assert_eq!(self.nlimbs(), basis.len());
        for i in 0..self.nlimbs() {
            basis.table(i).inverse(&mut self.limbs[i]);
        }
        self.ntt_form = false;
    }

    pub fn add_assign(&mut self, other: &Self, basis: &Basis) {
        assert_eq!(self.ntt_form, other.ntt_form);
        assert_eq!(self.nlimbs(), other.nlimbs());
        for i in 0..self.nlimbs() {
            let q = basis.moduli[i];
            for k in 0..self.n() {
                self.limbs[i][k] = add_mod(self.limbs[i][k], other.limbs[i][k], q);
            }
        }
    }

    pub fn sub_assign(&mut self, other: &Self, basis: &Basis) {
        assert_eq!(self.ntt_form, other.ntt_form);
        for i in 0..self.nlimbs() {
            let q = basis.moduli[i];
            for k in 0..self.n() {
                self.limbs[i][k] = sub_mod(self.limbs[i][k], other.limbs[i][k], q);
            }
        }
    }

    pub fn neg_assign(&mut self, basis: &Basis) {
        for i in 0..self.nlimbs() {
            let q = basis.moduli[i];
            for k in 0..self.n() {
                self.limbs[i][k] = neg_mod(self.limbs[i][k], q);
            }
        }
    }

    /// Pointwise product; both operands must be in NTT form.
    pub fn mul_assign_ntt(&mut self, other: &Self, basis: &Basis) {
        assert!(self.ntt_form && other.ntt_form);
        for i in 0..self.nlimbs() {
            let q = basis.moduli[i];
            for k in 0..self.n() {
                self.limbs[i][k] = mul_mod(self.limbs[i][k], other.limbs[i][k], q);
            }
        }
    }

    pub fn mul_scalar(&mut self, s: i64, basis: &Basis) {
        for i in 0..self.nlimbs() {
            let q = basis.moduli[i];
            let sm = signed_to_mod(s, q);
            for k in 0..self.n() {
                self.limbs[i][k] = mul_mod(self.limbs[i][k], sm, q);
            }
        }
    }

    pub fn mul_u64_mod(&mut self, s: u64, basis: &Basis) {
        for i in 0..self.nlimbs() {
            let q = basis.moduli[i];
            let sm = s % q;
            for k in 0..self.n() {
                self.limbs[i][k] = mul_mod(self.limbs[i][k], sm, q);
            }
        }
    }

    /// Centre-lift the first limb (or a named limb) to `i64` coefficients. Caller must INTT first.
    pub fn center_lift_limb(&self, limb: usize, q: u64) -> Vec<i64> {
        assert!(!self.ntt_form);
        self.limbs[limb]
            .iter()
            .map(|&x| center_lift(x, q))
            .collect()
    }
}

/// Map a signed integer into `[0, q)`.
#[inline]
pub fn signed_to_mod(x: i64, q: u64) -> u64 {
    let q_i = q as i128;
    let mut r = (x as i128) % q_i;
    if r < 0 {
        r += q_i;
    }
    r as u64
}

/// Centre lift from `[0, q)` into `(-q/2, q/2]`.
#[inline]
pub fn center_lift(x: u64, q: u64) -> i64 {
    if x > q / 2 {
        x as i64 - q as i64
    } else {
        x as i64
    }
}

/// Approximate fast basis conversion (`docs/plan/05` §2.5).
///
/// `src` is in coefficient form over `from`. Returns coefficient-form residues over `to`.
/// The integer represented is `x + u·B` for a small `u` (the usual approximate conversion).
pub fn convert_approx(src: &RnsPoly, from: &Basis, to: &Basis) -> RnsPoly {
    assert!(!src.ntt_form);
    assert_eq!(src.nlimbs(), from.len());
    assert_eq!(src.n(), from.n);
    assert_eq!(from.n, to.n);
    let n = from.n;
    let r = from.len();
    let mut dst = RnsPoly::zero(to);
    for (c_idx, &c) in to.moduli.iter().enumerate() {
        let hats: Vec<u64> = (0..r).map(|i| from.hat_mod(i, c)).collect();
        for k in 0..n {
            let mut acc = 0u64;
            for i in 0..r {
                let bi = from.moduli[i];
                let xi = mul_mod(src.limbs[i][k], from.hat_inv[i], bi);
                // centre the multiplier so u stays small
                let (xi_c, centered_neg) = if xi > bi / 2 {
                    (bi - xi, true)
                } else {
                    (xi, false)
                };
                let term = mul_mod(xi_c, hats[i], c);
                if centered_neg {
                    acc = sub_mod(acc, term, c);
                } else {
                    acc = add_mod(acc, term, c);
                }
            }
            dst.limbs[c_idx][k] = acc;
        }
    }
    dst
}

/// Extend `src` (basis `from`) with extra moduli `extra`, approximating the same integer.
pub fn mod_up(src: &RnsPoly, from: &Basis, extra: &Basis) -> RnsPoly {
    let conv = convert_approx(src, from, extra);
    let mut out = RnsPoly {
        limbs: src.limbs.clone(),
        ntt_form: false,
    };
    out.limbs.extend(conv.limbs);
    out
}

/// Drop the last `p_count` limbs after converting them down: `(x - Conv_P(x_P)) * P^{-1}` over Q.
pub fn mod_down(src: &RnsPoly, q_basis: &Basis, p_basis: &Basis) -> RnsPoly {
    assert!(!src.ntt_form);
    assert_eq!(src.nlimbs(), q_basis.len() + p_basis.len());
    let mut x_p = RnsPoly::zero(p_basis);
    let qn = q_basis.len();
    for i in 0..p_basis.len() {
        x_p.limbs[i] = src.limbs[qn + i].clone();
    }
    let conv = convert_approx(&x_p, p_basis, q_basis);
    let mut out = RnsPoly::zero(q_basis);
    for i in 0..q_basis.len() {
        let q = q_basis.moduli[i];
        let p_inv = inv_mod_prime(p_basis.product_mod(q), q);
        for k in 0..q_basis.n {
            let d = sub_mod(src.limbs[i][k], conv.limbs[i][k], q);
            out.limbs[i][k] = mul_mod(d, p_inv, q);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primes::ntt_primes_below;
    use crate::SplitMix64;

    #[test]
    fn rns_add_mul_matches_single_modulus() {
        let n = 32;
        let qs = ntt_primes_below(40, 3, n, &[]);
        let basis = Basis::new(qs.clone(), n);
        let mut rng = SplitMix64(21);
        let a: Vec<i64> = (0..n).map(|_| rng.below(100) as i64 - 50).collect();
        let b: Vec<i64> = (0..n).map(|_| rng.below(100) as i64 - 50).collect();
        let mut pa = RnsPoly::from_i64(&basis, &a);
        let pb = RnsPoly::from_i64(&basis, &b);
        pa.add_assign(&pb, &basis);
        for i in 0..3 {
            let q = qs[i];
            for k in 0..n {
                let expect = signed_to_mod(a[k] + b[k], q);
                assert_eq!(pa.limbs[i][k], expect);
            }
        }
        let mut pa = RnsPoly::from_i64(&basis, &a);
        pa.ntt(&basis);
        let mut pb = RnsPoly::from_i64(&basis, &b);
        pb.ntt(&basis);
        pa.mul_assign_ntt(&pb, &basis);
        pa.intt(&basis);
        for i in 0..3 {
            let prod = basis.table(i).multiply(
                &RnsPoly::from_i64(&basis, &a).limbs[i],
                &RnsPoly::from_i64(&basis, &b).limbs[i],
            );
            assert_eq!(pa.limbs[i], prod);
        }
    }

    #[test]
    fn convert_approx_preserves_small_integers() {
        let n = 16;
        let from_q = ntt_primes_below(30, 2, n, &[]);
        let to_q = ntt_primes_below(35, 2, n, &from_q);
        let from = Basis::new(from_q, n);
        let to = Basis::new(to_q.clone(), n);
        let mut rng = SplitMix64(3);
        let coeffs: Vec<i64> = (0..n).map(|_| rng.below(20) as i64 - 10).collect();
        let src = RnsPoly::from_i64(&from, &coeffs);
        let dst = convert_approx(&src, &from, &to);
        for i in 0..to.len() {
            for k in 0..n {
                assert_eq!(dst.limbs[i][k], signed_to_mod(coeffs[k], to_q[i]));
            }
        }
    }

    #[test]
    fn mod_down_divides_out_p_times_x() {
        let n = 16;
        let q = ntt_primes_below(40, 2, n, &[]);
        let p = ntt_primes_below(45, 1, n, &q);
        let qb = Basis::new(q.clone(), n);
        let pb = Basis::new(p.clone(), n);
        let coeffs: Vec<i64> = (0..n).map(|i| (i as i64 % 7) - 3).collect();
        let src = RnsPoly::from_i64(&qb, &coeffs);
        let mut up = mod_up(&src, &qb, &pb);
        // Form P·x in QP: Q limbs get ×P, P limbs become 0.
        for i in 0..qb.len() {
            let qi = q[i];
            let pmod = pb.product_mod(qi);
            for k in 0..n {
                up.limbs[i][k] = mul_mod(up.limbs[i][k], pmod, qi);
            }
        }
        for i in 0..pb.len() {
            up.limbs[qb.len() + i].fill(0);
        }
        let down = mod_down(&up, &qb, &pb);
        for i in 0..qb.len() {
            for k in 0..n {
                assert_eq!(down.limbs[i][k], src.limbs[i][k], "limb {i} coeff {k}");
            }
        }
    }
}
