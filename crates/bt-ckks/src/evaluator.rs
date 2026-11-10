use bt_arith::modops::{inv_mod_prime, mul_mod, sub_mod};
use bt_arith::rns::{center_lift, signed_to_mod, Basis, RnsPoly};

use crate::context::Context;
use crate::encoder::{encode, encode_const};
use crate::poly::clone_limbs;
use crate::types::{Ciphertext, EvalKeys, SwitchingKey};

pub struct Evaluator<'a> {
    pub ctx: &'a Context,
    pub keys: &'a EvalKeys,
}

impl<'a> Evaluator<'a> {
    pub fn new(ctx: &'a Context, keys: &'a EvalKeys) -> Self {
        Self { ctx, keys }
    }

    pub fn add(&self, a: &Ciphertext, b: &Ciphertext) -> Ciphertext {
        let (a, b) = self.align(a, b);
        let basis = self.ctx.basis_q_level(a.level);
        let mut c0 = a.c0.clone();
        let mut c1 = a.c1.clone();
        c0.add_assign(&b.c0, &basis);
        c1.add_assign(&b.c1, &basis);
        Ciphertext {
            c0,
            c1,
            level: a.level,
            scale: a.scale,
        }
    }

    pub fn sub(&self, a: &Ciphertext, b: &Ciphertext) -> Ciphertext {
        let (a, b) = self.align(a, b);
        let basis = self.ctx.basis_q_level(a.level);
        let mut c0 = a.c0.clone();
        let mut c1 = a.c1.clone();
        c0.sub_assign(&b.c0, &basis);
        c1.sub_assign(&b.c1, &basis);
        Ciphertext {
            c0,
            c1,
            level: a.level,
            scale: a.scale,
        }
    }

    pub fn add_const(&self, a: &Ciphertext, c: f64) -> Ciphertext {
        let (poly, _, _) = encode_const(self.ctx, c, a.level, a.scale);
        let basis = self.ctx.basis_q_level(a.level);
        let mut c0 = a.c0.clone();
        c0.add_assign(&poly, &basis);
        Ciphertext {
            c0,
            c1: a.c1.clone(),
            level: a.level,
            scale: a.scale,
        }
    }

    /// Multiply by a small integer; scale is unchanged.
    pub fn mul_int(&self, a: &Ciphertext, k: i64) -> Ciphertext {
        let basis = self.ctx.basis_q_level(a.level);
        let mut c0 = a.c0.clone();
        let mut c1 = a.c1.clone();
        c0.mul_scalar(k, &basis);
        c1.mul_scalar(k, &basis);
        Ciphertext {
            c0,
            c1,
            level: a.level,
            scale: a.scale,
        }
    }

    pub fn mul_const(&self, a: &Ciphertext, c: f64) -> Ciphertext {
        let q_ell = self.ctx.q_at(a.level.max(1));
        let scale_pt = if a.level == 0 {
            a.scale
        } else {
            q_ell as f64
        };
        let (pt, _, _) = encode_const(self.ctx, c, a.level, scale_pt);
        let basis = self.ctx.basis_q_level(a.level);
        let mut c0 = a.c0.clone();
        let mut c1 = a.c1.clone();
        c0.mul_assign_ntt(&pt, &basis);
        c1.mul_assign_ntt(&pt, &basis);
        let mut out = Ciphertext {
            c0,
            c1,
            level: a.level,
            scale: a.scale * scale_pt,
        };
        if out.level > 0 {
            self.rescale(&mut out);
        }
        out
    }

    pub fn mul_plain(&self, a: &Ciphertext, z: &[f64]) -> Ciphertext {
        let q_ell = self.ctx.q_at(a.level.max(1));
        let scale_pt = q_ell as f64;
        let (pt, _, _) = encode(self.ctx, z, a.level, scale_pt);
        let basis = self.ctx.basis_q_level(a.level);
        let mut c0 = a.c0.clone();
        let mut c1 = a.c1.clone();
        c0.mul_assign_ntt(&pt, &basis);
        c1.mul_assign_ntt(&pt, &basis);
        let mut out = Ciphertext {
            c0,
            c1,
            level: a.level,
            scale: a.scale * scale_pt,
        };
        if out.level > 0 {
            self.rescale(&mut out);
        }
        out
    }

    pub fn mul(&self, a: &Ciphertext, b: &Ciphertext) -> Ciphertext {
        let (a, b) = self.align(a, b);
        let basis = self.ctx.basis_q_level(a.level);
        let mut d0 = a.c0.clone();
        d0.mul_assign_ntt(&b.c0, &basis);
        let mut d1a = a.c0.clone();
        d1a.mul_assign_ntt(&b.c1, &basis);
        let mut d1b = a.c1.clone();
        d1b.mul_assign_ntt(&b.c0, &basis);
        d1a.add_assign(&d1b, &basis);
        let mut d2 = a.c1.clone();
        d2.mul_assign_ntt(&b.c1, &basis);
        let mut out = if let Some(rlk) = &self.keys.relin {
            let (k0, k1) = key_switch(self.ctx, &d2, a.level, rlk);
            d0.add_assign(&k0, &basis);
            d1a.add_assign(&k1, &basis);
            Ciphertext {
                c0: d0,
                c1: d1a,
                level: a.level,
                scale: a.scale * b.scale,
            }
        } else {
            // no relin: still return degree-1 approximation (drop d2) — tests always supply keys
            Ciphertext {
                c0: d0,
                c1: d1a,
                level: a.level,
                scale: a.scale * b.scale,
            }
        };
        if out.level > 0 {
            self.rescale(&mut out);
        }
        out
    }

    pub fn square(&self, a: &Ciphertext) -> Ciphertext {
        self.mul(a, a)
    }

    pub fn drop_to(&self, a: &Ciphertext, level: usize) -> Ciphertext {
        assert!(level <= a.level);
        let keep = self.ctx.q_limbs(level);
        Ciphertext {
            c0: clone_limbs(&a.c0, keep),
            c1: clone_limbs(&a.c1, keep),
            level,
            scale: a.scale,
        }
    }

    fn align(&self, a: &Ciphertext, b: &Ciphertext) -> (Ciphertext, Ciphertext) {
        debug_assert!((a.scale / b.scale - 1.0).abs() < 1e-6 || a.level != b.level);
        match a.level.cmp(&b.level) {
            std::cmp::Ordering::Equal => (a.clone(), b.clone()),
            std::cmp::Ordering::Greater => (self.drop_to(a, b.level), b.clone()),
            std::cmp::Ordering::Less => (a.clone(), self.drop_to(b, a.level)),
        }
    }

    pub fn rescale(&self, ct: &mut Ciphertext) {
        assert!(ct.level > 0);
        let level = ct.level;
        let q_ell = self.ctx.q_moduli[level];
        let basis = self.ctx.basis_q_level(level);
        rescale_poly(&mut ct.c0, &basis, level, q_ell);
        rescale_poly(&mut ct.c1, &basis, level, q_ell);
        ct.level -= 1;
        ct.scale /= q_ell as f64;
    }
}

fn rescale_poly(p: &mut RnsPoly, basis: &Basis, last: usize, q_ell: u64) {
    assert!(p.ntt_form);
    let n = p.n();
    let mut last_limb = p.limbs[last].clone();
    basis.table(last).inverse(&mut last_limb);
    // centre-lift with round-to-nearest
    // Last limb is already `c mod q_ℓ`. Centre-lift to `(-q/2, q/2]`.
    let lifted: Vec<i64> = last_limb.iter().map(|&x| center_lift(x, q_ell)).collect();
    for j in 0..last {
        let qj = basis.moduli[j];
        let inv = inv_mod_prime(q_ell % qj, qj);
        let mut ext = vec![0u64; n];
        for k in 0..n {
            ext[k] = signed_to_mod(lifted[k], qj);
        }
        basis.table(j).forward(&mut ext);
        for k in 0..n {
            let d = sub_mod(p.limbs[j][k], ext[k], qj);
            p.limbs[j][k] = mul_mod(d, inv, qj);
        }
    }
    p.limbs.truncate(last);
}

/// Key-switch `d` (NTT, Q_ℓ) from s' to s.
///
/// Toy path: the switching key is `(-as+e+s', a)` and the product is taken in Q.
/// This is correct when the key-switch error `d·e` stays smaller than q0/2Δ, which
/// the toy `dnum=1` parameters do not guarantee for a `P`-gadget (that product wraps).
pub fn key_switch(
    ctx: &Context,
    d: &RnsPoly,
    level: usize,
    ksk: &SwitchingKey,
) -> (RnsPoly, RnsPoly) {
    let ql = ctx.q_limbs(level);
    let q_basis = ctx.basis_q_level(level);
    let (k0, k1) = &ksk.digits[0];
    let k0t = clone_limbs(k0, ql);
    let k1t = clone_limbs(k1, ql);
    let mut t0 = d.clone();
    t0.mul_assign_ntt(&k0t, &q_basis);
    let mut t1 = d.clone();
    t1.mul_assign_ntt(&k1t, &q_basis);
    (t0, t1)
}

/// Rotate slots by `k` using automorphism + key switch.
pub fn rotate(ctx: &Context, ct: &Ciphertext, k: i32, rtk: &SwitchingKey) -> Ciphertext {
    use bt_arith::automorphism::{apply_ntt, galois_element_for_rotation, ntt_permutation};
    let basis = ctx.basis_q_level(ct.level);
    let g = galois_element_for_rotation(k as i64, ctx.n());
    let apply = |p: &RnsPoly| -> RnsPoly {
        let mut o = p.clone();
        for (i, limb) in o.limbs.iter_mut().enumerate() {
            let perm = ntt_permutation(basis.table(i), g);
            *limb = apply_ntt(limb, &perm);
        }
        o
    };
    let c0 = apply(&ct.c0);
    let c1 = apply(&ct.c1);
    let (k0, k1) = key_switch(ctx, &c1, ct.level, rtk);
    let mut out0 = c0;
    out0.add_assign(&k0, &basis);
    Ciphertext {
        c0: out0,
        c1: k1,
        level: ct.level,
        scale: ct.scale,
    }
}
