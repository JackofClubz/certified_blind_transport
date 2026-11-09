//! Two-party threshold CKKS (`docs/plan/06`). Toy parameters; privacy is semi-honest only.

#![allow(clippy::needless_range_loop)]

use bt_arith::rns::{Basis, RnsPoly};
use bt_ckks::poly::{clone_limbs, gaussian_poly, poly_from_i64, uniform_poly};
use bt_ckks::types::{Ciphertext, Plaintext, PublicKey, SecretKey, SwitchingKey};
use bt_ckks::{Context, Secret};
use rand_chacha::ChaCha20Rng;
use rand_core::{RngCore, SeedableRng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Role {
    A,
    B,
}

#[derive(Clone, Debug)]
pub struct Crs {
    pub seed: [u8; 32],
}

impl Crs {
    pub fn from_seeds(session_id: &[u8; 32], sigma_a: [u8; 32], sigma_b: [u8; 32]) -> Self {
        let mut h = Sha256::new();
        h.update(b"cbt/crs");
        h.update(session_id);
        h.update(sigma_a);
        h.update(sigma_b);
        Self {
            seed: h.finalize().into(),
        }
    }

    pub fn commit(seed: &[u8; 32]) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(b"cbt/crs/commit");
        h.update(seed);
        h.finalize().into()
    }

    fn rng(&self, label: &str, idx: u32) -> ChaCha20Rng {
        let mut h = Sha256::new();
        h.update(b"cbt/crs/poly");
        h.update(self.seed);
        h.update(label.as_bytes());
        h.update(idx.to_le_bytes());
        ChaCha20Rng::from_seed(h.finalize().into())
    }

    pub fn poly(&self, ctx: &Context, label: &str, idx: u32, limbs: usize) -> RnsPoly {
        let mut rng = self.rng(label, idx);
        uniform_poly(ctx, limbs, &mut rng)
    }
}

pub struct Party {
    pub role: Role,
    pub s: SecretKey,
    pub ephemeral: Option<RnsPoly>,
}

impl Party {
    pub fn new(ctx: &Context, role: Role, rng: &mut impl RngCore) -> Self {
        Self {
            role,
            s: bt_ckks::gen_secret(ctx, rng),
            ephemeral: None,
        }
    }

    pub fn ckg_share(&self, ctx: &Context, crs: &Crs, rng: &mut impl RngCore) -> CkgShare {
        let ql = ctx.q_basis.len();
        let mut a = crs.poly(ctx, "ckg", 0, ql);
        a.ntt(&ctx.q_basis);
        let mut e = gaussian_poly(ctx, ql, ctx.params.sigma(), rng);
        e.ntt(&ctx.q_basis);
        let s = clone_limbs(&self.s.rns(), ql);
        let mut p = a;
        p.mul_assign_ntt(&s, &ctx.q_basis);
        p.neg_assign(&ctx.q_basis);
        p.add_assign(&e, &ctx.q_basis);
        CkgShare {
            role: self.role,
            p: p.limbs,
            ntt: true,
        }
    }

    pub fn decrypt_share(
        &self,
        ctx: &Context,
        ct: &Ciphertext,
        lambda: u32,
        rng: &mut impl RngCore,
    ) -> DecShare {
        let ql = ctx.q_limbs(ct.level);
        let basis = ctx.basis_q_level(ct.level);
        let s = clone_limbs(&self.s.rns(), ql);
        let mut d = ct.c1.clone();
        d.mul_assign_ntt(&s, &basis);
        let sigma = ctx.params.sigma() * ((1u64 << lambda.min(16)) as f64).sqrt();
        let mut e = gaussian_poly(ctx, ql, sigma, rng);
        e.ntt(&basis);
        d.add_assign(&e, &basis);
        DecShare {
            role: self.role,
            d: d.limbs,
            ntt: true,
            level: ct.level,
        }
    }

    pub fn refresh_share(
        &self,
        ctx: &Context,
        c1: &RnsPoly,
        r: u32,
        lambda_m: u32,
        rng: &mut impl RngCore,
        crs: &Crs,
    ) -> RefreshShare {
        let ql = c1.nlimbs();
        let level = ql - 1;
        let basis = ctx.basis_q_level(level);
        let bound = 1i64 << lambda_m.min(20).max(8);
        let n = ctx.n();
        let mask: Vec<i64> = (0..n)
            .map(|_| {
                let u = rng.next_u32() as i64;
                (u % (2 * bound + 1)) - bound
            })
            .collect();
        let mut m_poly = poly_from_i64(ctx, &mask, ql);
        m_poly.ntt(&basis);
        let s = clone_limbs(&self.s.rns(), ql);
        let mut h0 = c1.clone();
        if !h0.ntt_form {
            h0.ntt(&basis);
        }
        h0.mul_assign_ntt(&s, &basis);
        h0.sub_assign(&m_poly, &basis);
        let mut e0 = gaussian_poly(ctx, ql, ctx.params.sigma(), rng);
        e0.ntt(&basis);
        h0.add_assign(&e0, &basis);

        let qp = ctx.qp_basis.len();
        let mut a = crs.poly(ctx, "refresh/r", r, qp);
        a.ntt(&ctx.qp_basis);
        let mut m_qp = poly_from_i64(ctx, &mask, qp);
        m_qp.ntt(&ctx.qp_basis);
        let s_qp = self.s.rns();
        let mut h1 = a;
        h1.mul_assign_ntt(&s_qp, &ctx.qp_basis);
        h1.neg_assign(&ctx.qp_basis);
        h1.add_assign(&m_qp, &ctx.qp_basis);
        let mut e1 = gaussian_poly(ctx, qp, ctx.params.sigma(), rng);
        e1.ntt(&ctx.qp_basis);
        h1.add_assign(&e1, &ctx.qp_basis);
        RefreshShare {
            role: self.role,
            round: r,
            h0: h0.limbs,
            h1: h1.limbs,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CkgShare {
    pub role: Role,
    pub p: Vec<Vec<u64>>,
    pub ntt: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecShare {
    pub role: Role,
    pub d: Vec<Vec<u64>>,
    pub ntt: bool,
    pub level: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RefreshShare {
    pub role: Role,
    pub round: u32,
    pub h0: Vec<Vec<u64>>,
    pub h1: Vec<Vec<u64>>,
}

pub fn combine_pk(ctx: &Context, crs: &Crs, shares: &[CkgShare]) -> PublicKey {
    let ql = ctx.q_basis.len();
    let mut a = crs.poly(ctx, "ckg", 0, ql);
    a.ntt(&ctx.q_basis);
    let mut b = RnsPoly {
        limbs: shares[0].p.clone(),
        ntt_form: true,
    };
    for s in shares.iter().skip(1) {
        let other = RnsPoly {
            limbs: s.p.clone(),
            ntt_form: true,
        };
        b.add_assign(&other, &ctx.q_basis);
    }
    PublicKey { b, a }
}

pub fn combine_decrypt(ctx: &Context, ct: &Ciphertext, shares: &[DecShare]) -> Plaintext {
    let basis = ctx.basis_q_level(ct.level);
    let mut acc = ct.c0.clone();
    for s in shares {
        let d = RnsPoly {
            limbs: s.d.clone(),
            ntt_form: true,
        };
        acc.add_assign(&d, &basis);
    }
    Plaintext {
        poly: acc,
        level: ct.level,
        scale: ct.scale,
    }
}

pub fn combine_refresh(
    ctx: &Context,
    crs: &Crs,
    ct: &Ciphertext,
    r: u32,
    shares: &[RefreshShare],
) -> Ciphertext {
    let ql = ctx.q_limbs(ct.level);
    let basis = ctx.basis_q_level(ct.level);
    let mut c0 = ct.c0.clone();
    for s in shares {
        let h0 = RnsPoly {
            limbs: s.h0.clone(),
            ntt_form: true,
        };
        c0.add_assign(&h0, &basis);
    }
    let qp = ctx.qp_basis.len();
    let mut a = crs.poly(ctx, "refresh/r", r, qp);
    a.ntt(&ctx.qp_basis);
    let mut c0_qp = {
        // lift Q-level c0 to QP by ModUp
        let mut tmp = c0.clone();
        if tmp.ntt_form {
            tmp.intt(&basis);
        }
        let up = bt_arith::rns::mod_up(&tmp, &basis, &ctx.p_basis);
        let _ = up.nlimbs();
        // pad missing Q limbs to full Q
        while up.nlimbs() < qp {
            // already Q_level + P; need remaining upper Q zeros then P already appended
            break;
        }
        // Rebuild: full Q zeros + convert
        let mut full = poly_from_i64(ctx, &vec![0i64; ctx.n()], qp);
        let q_all = ctx.q_basis.len();
        for i in 0..ql {
            full.limbs[i] = tmp.limbs[i].clone();
        }
        let extra = bt_arith::rns::convert_approx(&tmp, &basis, &ctx.p_basis);
        for i in 0..ctx.p_moduli.len() {
            full.limbs[q_all + i] = extra.limbs[i].clone();
        }
        // also fill unused Q limbs via convert to remaining q's
        if ql < q_all {
            let rest = Basis::new(ctx.q_moduli[ql..].to_vec(), ctx.n());
            let conv = bt_arith::rns::convert_approx(&tmp, &basis, &rest);
            for i in 0..rest.len() {
                full.limbs[ql + i] = conv.limbs[i].clone();
            }
        }
        full.ntt(&ctx.qp_basis);
        full
    };
    for s in shares {
        let h1 = RnsPoly {
            limbs: s.h1.clone(),
            ntt_form: true,
        };
        c0_qp.add_assign(&h1, &ctx.qp_basis);
    }
    let keep = ctx.q_basis.len();
    Ciphertext {
        c0: clone_limbs(&c0_qp, keep),
        c1: clone_limbs(&a, keep),
        level: ctx.top_level(),
        scale: ct.scale,
    }
}

/// Combine two secret shares (tests only).
pub fn combine_secrets(ctx: &Context, a: &SecretKey, b: &SecretKey) -> SecretKey {
    let mut s = a.rns();
    s.add_assign(&b.rns(), &ctx.qp_basis);
    SecretKey {
        poly: Secret::new(s.limbs),
        ntt_form: true,
    }
}

pub fn gen_collective_relin(
    ctx: &Context,
    a: &Party,
    b: &Party,
    rng: &mut impl RngCore,
) -> SwitchingKey {
    // Test helper: relin for s = sA+sB using the combined secret.
    let sk = combine_secrets(ctx, &a.s, &b.s);
    bt_ckks::gen_relin_keys(ctx, &sk, rng).relin.expect("relin")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bt_ckks::{decode_plaintext, encode_plaintext, encrypt, Params};

    #[test]
    fn two_party_encrypt_decrypt() {
        let ctx = Context::new(&Params::toy_test());
        let mut rng = ChaCha20Rng::from_seed([3u8; 32]);
        let sid = [1u8; 32];
        let sa = [2u8; 32];
        let sb = [4u8; 32];
        assert_eq!(Crs::commit(&sa).len(), 32);
        let crs = Crs::from_seeds(&sid, sa, sb);
        let pa = Party::new(&ctx, Role::A, &mut rng);
        let pb = Party::new(&ctx, Role::B, &mut rng);
        let sh_a = pa.ckg_share(&ctx, &crs, &mut rng);
        let sh_b = pb.ckg_share(&ctx, &crs, &mut rng);
        let pk = combine_pk(&ctx, &crs, &[sh_a, sh_b]);
        let z = [0.2];
        let scale = ctx.scale_at_level(ctx.top_level());
        let pt = encode_plaintext(&ctx, &z, ctx.top_level(), scale);
        let ct = encrypt(&ctx, &pk, &pt, &mut rng);
        let d0 = pa.decrypt_share(&ctx, &ct, 8, &mut rng);
        let d1 = pb.decrypt_share(&ctx, &ct, 8, &mut rng);
        let rec = combine_decrypt(&ctx, &ct, &[d0, d1]);
        let out = decode_plaintext(&ctx, &rec);
        assert!((out[0] - 0.2).abs() < 0.05, "{}", out[0]);
    }
}
