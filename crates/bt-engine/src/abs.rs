//! Cheon et al. (ASIACRYPT 2020) comparison polynomials, used as |x| ≈ x · p(x).
//!
//! The figure of merit is ε_|x| = ||x| − x·p(x)|, not |sign(x) − p(x)|.

use bt_ckks::{Ciphertext, Evaluator};
use serde::{Deserialize, Serialize};

/// Cheon f_n families (odd, map [−1,1] toward ±1).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AbsKind {
    /// f₁(x) = (3x − x³)/2. Depth ≈ 2 ciphertext muls per iteration.
    F1,
    /// f₂(x) = (15x − 10x³ + 3x⁵)/8. Depth ≈ 3.
    F2,
    /// f₃(x) = (35x − 35x³ + 21x⁵ − 5x⁷)/16. Depth ≈ 3 sequential, 5 muls.
    F3,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignSpec {
    pub kind: AbsKind,
    pub iters: u32,
}

/// Iterations of f₃ that meet ε_|x| < 1 micro-lot on [−800k, 800k] in the f64 harness.
pub const HE_F3_ITERS: u32 = 13;

impl Default for SignSpec {
    fn default() -> Self {
        Self {
            kind: AbsKind::F3,
            iters: 16,
        }
    }
}

impl SignSpec {
    pub fn fast() -> Self {
        Self {
            kind: AbsKind::F1,
            iters: 2,
        }
    }

    /// Homomorphic path: one layer of f₃, sized from `abs_polynomial_bench`.
    pub fn he() -> Self {
        Self {
            kind: AbsKind::F3,
            iters: HE_F3_ITERS,
        }
    }

    /// Levels consumed per iteration with this evaluator (rescale after each ct-mul / mul_const).
    /// f₃ is arranged so x³ and x⁴ share a layer; x⁵ and x⁷ share the next.
    pub fn levels_per_iter(&self) -> usize {
        match self.kind {
            AbsKind::F1 => 3,
            AbsKind::F2 => 4,
            AbsKind::F3 => 4,
        }
    }

    pub fn estimated_levels(&self) -> usize {
        self.iters as usize * self.levels_per_iter() + 1 /* x · p(x) */ + 1 /* ×½ */
    }
}

pub fn cheon_f1(x: f64) -> f64 {
    0.5 * (3.0 * x - x.powi(3))
}

pub fn cheon_f2(x: f64) -> f64 {
    (15.0 * x - 10.0 * x.powi(3) + 3.0 * x.powi(5)) / 8.0
}

pub fn cheon_f3(x: f64) -> f64 {
    (35.0 * x - 35.0 * x.powi(3) + 21.0 * x.powi(5) - 5.0 * x.powi(7)) / 16.0
}

fn apply_kind(x: f64, kind: AbsKind) -> f64 {
    match kind {
        AbsKind::F1 => cheon_f1(x),
        AbsKind::F2 => cheon_f2(x),
        AbsKind::F3 => cheon_f3(x),
    }
}

/// Iterate the Cheon polynomial (as a sign map). No clamp: |x| ≤ 1 is the caller's job.
pub fn iterate_p(mut x: f64, spec: &SignSpec) -> f64 {
    for _ in 0..spec.iters {
        x = apply_kind(x, spec.kind);
    }
    x
}

/// |x| ≈ x · p(x). Near zero this is small even when p is a poor sign.
pub fn abs_approx(x: f64, spec: &SignSpec) -> f64 {
    x * iterate_p(x, spec)
}

fn he_f1(eval: &Evaluator, x: &Ciphertext) -> Ciphertext {
    let x2 = eval.square(x);
    let x3 = eval.mul(&x2, x);
    let t = eval.sub(&eval.mul_int(x, 3), &x3);
    eval.mul_const(&t, 0.5)
}

fn he_f2(eval: &Evaluator, x: &Ciphertext) -> Ciphertext {
    let x2 = eval.square(x);
    let x3 = eval.mul(&x2, x);
    let x4 = eval.square(&x2);
    let x5 = eval.mul(&x4, x);
    let t = eval.add(
        &eval.sub(&eval.mul_int(x, 15), &eval.mul_int(&x3, 10)),
        &eval.mul_int(&x5, 3),
    );
    eval.mul_const(&t, 0.125)
}

fn he_f3(eval: &Evaluator, x: &Ciphertext) -> Ciphertext {
    let x2 = eval.square(x);
    let x3 = eval.mul(&x2, x);
    let x4 = eval.square(&x2);
    let x5 = eval.mul(&x4, x);
    let x7 = eval.mul(&x4, &x3);
    let t = eval.add(
        &eval.sub(&eval.mul_int(x, 35), &eval.mul_int(&x3, 35)),
        &eval.sub(&eval.mul_int(&x5, 21), &eval.mul_int(&x7, 5)),
    );
    eval.mul_const(&t, 1.0 / 16.0)
}

fn he_step(eval: &Evaluator, x: &Ciphertext, kind: AbsKind) -> Ciphertext {
    match kind {
        AbsKind::F1 => he_f1(eval, x),
        AbsKind::F2 => he_f2(eval, x),
        AbsKind::F3 => he_f3(eval, x),
    }
}

/// One |x| layer: p ∘ ⋯ ∘ p, then multiply by the original x.
pub fn he_abs(eval: &Evaluator, x: &Ciphertext, spec: &SignSpec) -> Ciphertext {
    let mut p = x.clone();
    for _ in 0..spec.iters {
        p = he_step(eval, &p, spec.kind);
    }
    eval.mul(x, &p)
}
