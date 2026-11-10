use bt_arith::rns::{center_lift, RnsPoly};

use crate::context::Context;
use crate::poly::poly_from_i64;

/// Encode a real vector into a plaintext polynomial at the given scale.
pub fn encode(ctx: &Context, z: &[f64], level: usize, scale: f64) -> (RnsPoly, usize, f64) {
    let n = ctx.n();
    let slots = ctx.slots();
    assert!(z.len() <= slots);
    let mut zpad = vec![0.0; slots];
    zpad[..z.len()].copy_from_slice(z);
    let mut m = vec![0.0; n];
    for k in 0..n {
        let mut re = 0.0;
        let mut g = 1u64;
        for slot in zpad.iter() {
            let (c, _) = zeta_pow(-(k as i64) * g as i64, n);
            re += slot * c;
            g = (g * 5) % (2 * n as u64);
        }
        m[k] = 2.0 / n as f64 * re;
    }
    let coeffs: Vec<i64> = m.iter().map(|&x| (x * scale).round() as i64).collect();
    let limbs = ctx.q_limbs(level);
    let mut p = poly_from_i64(ctx, &coeffs, limbs);
    let basis = ctx.basis_q_level(level);
    p.ntt(&basis);
    (p, level, scale)
}

/// Constant polynomial `m(X) = round(c·Δ)`. Evaluates to `c` in every slot.
pub fn encode_const(ctx: &Context, c: f64, level: usize, scale: f64) -> (RnsPoly, usize, f64) {
    let n = ctx.n();
    let mut coeffs = vec![0i64; n];
    coeffs[0] = (c * scale).round() as i64;
    let limbs = ctx.q_limbs(level);
    let mut p = poly_from_i64(ctx, &coeffs, limbs);
    let basis = ctx.basis_q_level(level);
    p.ntt(&basis);
    (p, level, scale)
}

pub fn decode(ctx: &Context, poly: &RnsPoly, level: usize, scale: f64) -> Vec<f64> {
    let n = ctx.n();
    let slots = ctx.slots();
    let basis = ctx.basis_q_level(level);
    let mut p = poly.clone();
    if p.ntt_form {
        p.intt(&basis);
    }
    let q0 = ctx.q_moduli[0];
    let coeffs: Vec<f64> = p.limbs[0]
        .iter()
        .map(|&x| center_lift(x, q0) as f64 / scale)
        .collect();
    let mut z = vec![0.0; slots];
    for j in 0..slots {
        let g = pow_mod_u64(5, j as u64, 2 * n as u64);
        let mut re = 0.0;
        for (k, &mk) in coeffs.iter().enumerate() {
            let (c, _) = zeta_pow(k as i64 * g as i64, n);
            re += mk * c;
        }
        z[j] = re;
    }
    z
}

fn pow_mod_u64(mut base: u64, mut exp: u64, m: u64) -> u64 {
    let mut acc = 1 % m;
    base %= m;
    while exp > 0 {
        if exp & 1 == 1 {
            acc = acc.wrapping_mul(base) % m;
        }
        base = base.wrapping_mul(base) % m;
        exp >>= 1;
    }
    acc
}

fn zeta_pow(exp: i64, n: usize) -> (f64, f64) {
    let two_n = 2 * n as i64;
    let e = exp.rem_euclid(two_n);
    let ang = std::f64::consts::PI * (e as f64) / (n as f64);
    (ang.cos(), ang.sin())
}
