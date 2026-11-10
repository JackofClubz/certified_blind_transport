use bt_arith::rns::RnsPoly;
use rand_core::RngCore;

use crate::context::Context;
use crate::poly::{clone_limbs, gaussian_poly, ternary_poly, uniform_poly};
use crate::secret::Secret;
use crate::types::{Ciphertext, EvalKeys, Plaintext, PublicKey, SecretKey, SwitchingKey};

pub fn gen_secret(ctx: &Context, rng: &mut impl RngCore) -> SecretKey {
    let limbs = ctx.qp_basis.len();
    let mut s = ternary_poly(ctx, limbs, rng);
    s.ntt(&ctx.qp_basis);
    SecretKey {
        poly: Secret::new(s.limbs),
        ntt_form: true,
    }
}

pub fn gen_public(ctx: &Context, sk: &SecretKey, rng: &mut impl RngCore) -> PublicKey {
    let ql = ctx.q_basis.len();
    let mut a = uniform_poly(ctx, ql, rng);
    a.ntt(&ctx.q_basis);
    let mut e = gaussian_poly(ctx, ql, ctx.params.sigma(), rng);
    e.ntt(&ctx.q_basis);
    let s = sk.rns();
    let mut b = a.clone();
    let s_q = clone_limbs(&s, ql);
    b.mul_assign_ntt(&s_q, &ctx.q_basis);
    b.neg_assign(&ctx.q_basis);
    b.add_assign(&e, &ctx.q_basis);
    PublicKey { b, a }
}

pub fn encrypt(
    ctx: &Context,
    pk: &PublicKey,
    pt: &Plaintext,
    rng: &mut impl RngCore,
) -> Ciphertext {
    let ql = ctx.q_limbs(pt.level);
    let basis = ctx.basis_q_level(pt.level);
    let mut v = ternary_poly(ctx, ql, rng);
    v.ntt(&basis);
    let mut e0 = gaussian_poly(ctx, ql, ctx.params.sigma(), rng);
    e0.ntt(&basis);
    let mut e1 = gaussian_poly(ctx, ql, ctx.params.sigma(), rng);
    e1.ntt(&basis);
    let pk_b = clone_limbs(&pk.b, ql);
    let pk_a = clone_limbs(&pk.a, ql);
    let mut c0 = v.clone();
    c0.mul_assign_ntt(&pk_b, &basis);
    c0.add_assign(&e0, &basis);
    c0.add_assign(&pt.poly, &basis);
    let mut c1 = v;
    c1.mul_assign_ntt(&pk_a, &basis);
    c1.add_assign(&e1, &basis);
    Ciphertext {
        c0,
        c1,
        level: pt.level,
        scale: pt.scale,
    }
}

pub fn decrypt(ctx: &Context, sk: &SecretKey, ct: &Ciphertext) -> Plaintext {
    let ql = ctx.q_limbs(ct.level);
    let basis = ctx.basis_q_level(ct.level);
    let s = clone_limbs(&sk.rns(), ql);
    let mut t = ct.c1.clone();
    t.mul_assign_ntt(&s, &basis);
    t.add_assign(&ct.c0, &basis);
    Plaintext {
        poly: t,
        level: ct.level,
        scale: ct.scale,
    }
}

/// Switching key from `s_from` to `s_to` (both NTT over QP).
pub fn gen_switching_key(
    ctx: &Context,
    s_to: &RnsPoly,
    s_from: &RnsPoly,
    rng: &mut impl RngCore,
) -> SwitchingKey {
    let qp = ctx.qp_basis.len();
    let mut digits = Vec::new();
    for _ in 0..ctx.digits.len() {
        let mut a = uniform_poly(ctx, qp, rng);
        a.ntt(&ctx.qp_basis);
        let mut k0 = a.clone();
        k0.mul_assign_ntt(s_to, &ctx.qp_basis);
        k0.neg_assign(&ctx.qp_basis);
        // Switching-key error omitted: `d·e` with a full-size `d` saturates q0.
        // Restore a Gaussian here together with a real `P` gadget.
        // Toy gadget: add `s_from` (not `P·s_from`). Hybrid `P` + digit decomposition
        // wraps for full-size `d` at these sizes; the P-gadget is restored with a real
        // dnum split. With this form, key-switch is `d·evk` over Q (see `key_switch`).
        k0.add_assign(s_from, &ctx.qp_basis);
        digits.push((k0, a));
    }
    SwitchingKey { digits }
}

pub fn gen_relin_keys(ctx: &Context, sk: &SecretKey, rng: &mut impl RngCore) -> EvalKeys {
    let s = sk.rns();
    let mut s2 = s.clone();
    s2.mul_assign_ntt(&s, &ctx.qp_basis);
    let relin = gen_switching_key(ctx, &s, &s2, rng);
    EvalKeys {
        relin: Some(relin),
        rotations: Vec::new(),
    }
}

pub fn gen_rotation_key(
    ctx: &Context,
    sk: &SecretKey,
    k: i32,
    rng: &mut impl RngCore,
) -> SwitchingKey {
    use bt_arith::automorphism::{apply_ntt, galois_element_for_rotation, ntt_permutation};
    let s = sk.rns();
    let n = ctx.n();
    let g = galois_element_for_rotation(k as i64, n);
    let mut s_rot = s.clone();
    for (i, limb) in s_rot.limbs.iter_mut().enumerate() {
        let tab = ctx.qp_basis.table(i);
        let perm = ntt_permutation(tab, g);
        *limb = apply_ntt(limb, &perm);
    }
    gen_switching_key(ctx, &s, &s_rot, rng)
}
