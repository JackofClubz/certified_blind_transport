use bt_arith::rns::{signed_to_mod, RnsPoly};
use rand_core::RngCore;

use crate::context::Context;

pub fn uniform_poly(ctx: &Context, limbs: usize, rng: &mut impl RngCore) -> RnsPoly {
    let n = ctx.n();
    let mut p = RnsPoly {
        limbs: vec![vec![0u64; n]; limbs],
        ntt_form: false,
    };
    for i in 0..limbs {
        let q = if i < ctx.q_moduli.len() {
            ctx.q_moduli[i]
        } else {
            ctx.p_moduli[i - ctx.q_moduli.len()]
        };
        for k in 0..n {
            p.limbs[i][k] = uniform_mod(rng, q);
        }
    }
    p
}

pub fn ternary_poly(ctx: &Context, limbs: usize, rng: &mut impl RngCore) -> RnsPoly {
    let n = ctx.n();
    let coeffs: Vec<i64> = (0..n)
        .map(|_| {
            let r = rng.next_u32() % 3;
            r as i64 - 1
        })
        .collect();
    poly_from_i64(ctx, &coeffs, limbs)
}

pub fn gaussian_poly(ctx: &Context, limbs: usize, sigma: f64, rng: &mut impl RngCore) -> RnsPoly {
    let n = ctx.n();
    let coeffs: Vec<i64> = (0..n).map(|_| sample_gaussian(rng, sigma)).collect();
    poly_from_i64(ctx, &coeffs, limbs)
}

pub fn poly_from_i64(ctx: &Context, coeffs: &[i64], limbs: usize) -> RnsPoly {
    let n = ctx.n();
    assert_eq!(coeffs.len(), n);
    let mut p = RnsPoly {
        limbs: vec![vec![0u64; n]; limbs],
        ntt_form: false,
    };
    for i in 0..limbs {
        let q = limb_mod(ctx, i);
        for k in 0..n {
            p.limbs[i][k] = signed_to_mod(coeffs[k], q);
        }
    }
    p
}

pub fn limb_mod(ctx: &Context, i: usize) -> u64 {
    if i < ctx.q_moduli.len() {
        ctx.q_moduli[i]
    } else {
        ctx.p_moduli[i - ctx.q_moduli.len()]
    }
}

pub fn uniform_mod(rng: &mut impl RngCore, q: u64) -> u64 {
    loop {
        let x = rng.next_u64();
        let t = q.wrapping_mul(u64::MAX / q);
        if x < t || t == 0 {
            return x % q;
        }
    }
}

fn sample_gaussian(rng: &mut impl RngCore, sigma: f64) -> i64 {
    let u1 = ((rng.next_u64() >> 11) as f64 / ((1u64 << 53) as f64)).max(1e-16);
    let u2 = (rng.next_u64() >> 11) as f64 / ((1u64 << 53) as f64);
    let r = sigma * (-2.0 * u1.ln()).sqrt();
    let theta = 2.0 * std::f64::consts::PI * u2;
    (r * theta.cos()).round() as i64
}

pub fn clone_limbs(p: &RnsPoly, keep: usize) -> RnsPoly {
    RnsPoly {
        limbs: p.limbs[..keep].to_vec(),
        ntt_form: p.ntt_form,
    }
}
