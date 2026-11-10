//! Closed-form 1-D OT matching engine (`docs/plan/07`, `docs/M1-market-math.md` §10).

#![allow(clippy::needless_range_loop, non_snake_case, unused_assignments)]

mod abs;

use bt_ckks::{
    decode_plaintext, decrypt, encode_plaintext, encrypt, Ciphertext, Context, Evaluator,
    PublicKey, SecretKey,
};
use bt_market::{round_trades, Book, Grid, Trades};
use serde::{Deserialize, Serialize};

pub use abs::{abs_approx, he_abs, AbsKind, SignSpec, HE_F3_ITERS};

pub const NORM: f64 = (1u32 << 19) as f64; // 2^19 micro-lots

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum Cheat {
    #[default]
    None,
    SkipRefresh,
    TruncateSign(u32),
    TamperOutput,
    Steer,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Op {
    Input { dst: usize, name: String },
    Add { dst: usize, a: usize, b: usize },
    Sub { dst: usize, a: usize, b: usize },
    MulC { dst: usize, a: usize, c: f64 },
    Mul { dst: usize, a: usize, b: usize },
    Square { dst: usize, a: usize },
    Refresh { reg: usize },
    Output { reg: usize },
}

#[derive(Clone, Debug)]
pub struct Circuit {
    pub ops: Vec<Op>,
    pub depth: usize,
    pub refreshes: usize,
}

fn push_abs(ops: &mut Vec<Op>, x: usize, spec: &SignSpec, used: &mut usize) -> usize {
    let mut p = x;
    for _ in 0..spec.iters {
        let x2 = *used;
        *used += 1;
        ops.push(Op::Square { dst: x2, a: p });
        let x3 = *used;
        *used += 1;
        ops.push(Op::Mul {
            dst: x3,
            a: x2,
            b: p,
        });
        match spec.kind {
            AbsKind::F1 => {
                let t = *used;
                *used += 1;
                ops.push(Op::MulC {
                    dst: t,
                    a: p,
                    c: 1.5,
                });
                let u = *used;
                *used += 1;
                ops.push(Op::MulC {
                    dst: u,
                    a: x3,
                    c: -0.5,
                });
                let out = *used;
                *used += 1;
                ops.push(Op::Add {
                    dst: out,
                    a: t,
                    b: u,
                });
                p = out;
            }
            AbsKind::F2 | AbsKind::F3 => {
                // Same skeleton as F1 in the op list; the evaluator uses he_abs.
                let out = *used;
                *used += 1;
                ops.push(Op::MulC {
                    dst: out,
                    a: x3,
                    c: 1.0,
                });
                p = out;
            }
        }
    }
    let ax = *used;
    *used += 1;
    ops.push(Op::Mul {
        dst: ax,
        a: x,
        b: p,
    });
    ax
}

/// One-layer interval circuit (`docs/M1-market-math.md` §10).
///
/// T = ½( |D−Sp| + |S−Dp| − |D−S| − |Dp−Sp| ), then the public mask.
pub fn compile_v1(sign: &SignSpec, budget: usize) -> Circuit {
    let mut ops = vec![
        Op::Input {
            dst: 0,
            name: "D".into(),
        },
        Op::Input {
            dst: 1,
            name: "Dp".into(),
        },
        Op::Input {
            dst: 2,
            name: "S".into(),
        },
        Op::Input {
            dst: 3,
            name: "Sp".into(),
        },
        Op::Sub { dst: 4, a: 0, b: 3 }, // D − Sp
        Op::Sub { dst: 5, a: 2, b: 1 }, // S − Dp
        Op::Sub { dst: 6, a: 0, b: 2 }, // D − S
        Op::Sub { dst: 7, a: 1, b: 3 }, // Dp − Sp
    ];
    let mut used = 8usize;
    let a = push_abs(&mut ops, 4, sign, &mut used);
    let b = push_abs(&mut ops, 5, sign, &mut used);
    let c = push_abs(&mut ops, 6, sign, &mut used);
    let d = push_abs(&mut ops, 7, sign, &mut used);
    let s1 = used;
    used += 1;
    ops.push(Op::Add {
        dst: s1,
        a,
        b,
    });
    let s2 = used;
    used += 1;
    ops.push(Op::Add {
        dst: s2,
        a: c,
        b: d,
    });
    let z = used;
    used += 1;
    ops.push(Op::Sub {
        dst: z,
        a: s1,
        b: s2,
    });
    let tt = used;
    ops.push(Op::MulC {
        dst: tt,
        a: z,
        c: 0.5,
    });
    ops.push(Op::Output { reg: tt });
    let _ = budget;
    Circuit {
        ops,
        depth: sign.estimated_levels(),
        refreshes: 0,
    }
}
/// Cumulative demand laid out in slots `s = 8*i + j'`. Firm A only.
pub fn layout_demand(g: &Grid, a: &Book) -> ([f64; 64], [f64; 64]) {
    let l = g.levels;
    let mut d = vec![0u64; l];
    let mut acc = 0u64;
    for i in 0..l {
        acc += a[i];
        d[i] = acc;
    }
    let mut D = [0.0; 64];
    let mut Dp = [0.0; 64];
    for i in 0..l {
        for jp in 0..l {
            let slot = 8 * i + jp;
            D[slot] = d[i] as f64 / NORM;
            Dp[slot] = if i == 0 { 0.0 } else { d[i - 1] as f64 / NORM };
        }
    }
    (D, Dp)
}

/// Cumulative supply laid out in slots `s = 8*i + j'`. Firm B only.
pub fn layout_supply(g: &Grid, b: &Book) -> ([f64; 64], [f64; 64]) {
    let l = g.levels;
    let mut s = vec![0u64; l];
    let mut acc = 0u64;
    for j in (0..l).rev() {
        acc += b[j];
        s[l - 1 - j] = acc;
    }
    let mut S = [0.0; 64];
    let mut Sp = [0.0; 64];
    for i in 0..l {
        for jp in 0..l {
            let slot = 8 * i + jp;
            S[slot] = s[jp] as f64 / NORM;
            Sp[slot] = if jp == 0 {
                0.0
            } else {
                s[jp - 1] as f64 / NORM
            };
        }
    }
    (S, Sp)
}

/// Cumulative sums laid out in slots s = 8*i + j'.
pub fn layout_cumulatives(
    g: &Grid,
    a: &Book,
    b: &Book,
) -> ([f64; 64], [f64; 64], [f64; 64], [f64; 64]) {
    let (d, dp) = layout_demand(g, a);
    let (s, sp) = layout_supply(g, b);
    (d, dp, s, sp)
}

pub fn public_mask(g: &Grid) -> [f64; 64] {
    let l = g.levels;
    let mut m = [0.0; 64];
    for i in 0..l {
        for jp in 0..l {
            let j = l - 1 - jp;
            if i <= j {
                m[8 * i + jp] = 1.0;
            }
        }
    }
    m
}

/// Overlap of [c1, d1) and [c2, d2) as a one-layer combination of absolute values.
///
/// (min(d1,d2) − max(c1,c2))⁺ = ½( |d1−c2| + |d2−c1| − |d1−d2| − |c1−c2| )
pub fn interval_overlap_plus(c1: f64, d1: f64, c2: f64, d2: f64, absf: impl Fn(f64) -> f64) -> f64 {
    0.5 * (absf(d1 - c2) + absf(d2 - c1) - absf(d1 - d2) - absf(c1 - c2))
}

/// Exact (f64, true |x|) interval form — must match `monotone_coupling`.
pub fn circuit_f64_exact(g: &Grid, a: &Book, b: &Book) -> Vec<Vec<f64>> {
    circuit_with_abs(g, a, b, |x| x.abs())
}

pub fn circuit_f64_poly(g: &Grid, a: &Book, b: &Book, spec: &SignSpec) -> Vec<Vec<f64>> {
    circuit_with_abs(g, a, b, |x| abs_approx(x, spec))
}

fn circuit_with_abs(g: &Grid, a: &Book, b: &Book, absf: impl Fn(f64) -> f64) -> Vec<Vec<f64>> {
    let (d, dp, s, sp) = layout_cumulatives(g, a, b);
    let mask = public_mask(g);
    let l = g.levels;
    let mut t = vec![vec![0.0; l]; l];
    for i in 0..l {
        for jp in 0..l {
            let k = 8 * i + jp;
            let tt = interval_overlap_plus(dp[k], d[k], sp[k], s[k], &absf);
            let j = l - 1 - jp;
            t[i][j] = (tt * mask[k]) * NORM;
        }
    }
    t
}

pub fn decode_slots_to_trades(g: &Grid, slots: &[f64], cheat: Cheat) -> Vec<Vec<f64>> {
    let l = g.levels;
    let mut t = vec![vec![0.0; l]; l];
    let mask = public_mask(g);
    for i in 0..l {
        for jp in 0..l {
            let k = 8 * i + jp;
            let j = l - 1 - jp;
            let mut v = slots.get(k).copied().unwrap_or(0.0) * mask[k] * NORM;
            match cheat {
                Cheat::TamperOutput => v += 500.0,
                Cheat::Steer if i == 0 && j == 0 && v > 0.0 => v *= 0.5,
                _ => {}
            }
            t[i][j] = v;
        }
    }
    t
}

/// Encrypt firm A's demand cumulatives. Does not take B's book.
pub fn encrypt_demand(
    ctx: &Context,
    pk: &PublicKey,
    g: &Grid,
    a: &Book,
    rng: &mut impl rand_core::RngCore,
) -> [Ciphertext; 2] {
    let (d, dp) = layout_demand(g, a);
    let scale = ctx.scale_at_level(ctx.top_level());
    let lv = ctx.top_level();
    let mut enc = |z: &[f64]| encrypt(ctx, pk, &encode_plaintext(ctx, z, lv, scale), rng);
    [enc(&d), enc(&dp)]
}

/// Encrypt firm B's supply cumulatives. Does not take A's book.
pub fn encrypt_supply(
    ctx: &Context,
    pk: &PublicKey,
    g: &Grid,
    b: &Book,
    rng: &mut impl rand_core::RngCore,
) -> [Ciphertext; 2] {
    let (s, sp) = layout_supply(g, b);
    let scale = ctx.scale_at_level(ctx.top_level());
    let lv = ctx.top_level();
    let mut enc = |z: &[f64]| encrypt(ctx, pk, &encode_plaintext(ctx, z, lv, scale), rng);
    [enc(&s), enc(&sp)]
}

/// Encrypt the four cumulative inputs under `pk`.
pub fn encrypt_inputs(
    ctx: &Context,
    pk: &PublicKey,
    g: &Grid,
    a: &Book,
    b: &Book,
    rng: &mut impl rand_core::RngCore,
) -> [Ciphertext; 4] {
    let [d, dp] = encrypt_demand(ctx, pk, g, a, rng);
    let [s, sp] = encrypt_supply(ctx, pk, g, b, rng);
    [d, dp, s, sp]
}

/// Matching circuit on ciphertexts only. The server entry point: no books, no secrets.
///
/// One |x| layer: T = ½(|D−Sp| + |S−Dp| − |D−S| − |Dp−Sp|).
pub fn eval_matching(eval: &Evaluator, inputs: [Ciphertext; 4], spec: &SignSpec) -> Ciphertext {
    let [d, dp, s, sp] = inputs;
    let d_sp = eval.sub(&d, &sp);
    let s_dp = eval.sub(&s, &dp);
    let d_s = eval.sub(&d, &s);
    let dp_sp = eval.sub(&dp, &sp);
    let a = he_abs(eval, &d_sp, spec);
    let b = he_abs(eval, &s_dp, spec);
    let c = he_abs(eval, &d_s, spec);
    let e = he_abs(eval, &dp_sp, spec);
    let sum = eval.sub(&eval.add(&a, &b), &eval.add(&c, &e));
    eval.mul_const(&sum, 0.5)
}

pub fn run_encrypted(
    ctx: &Context,
    eval: &Evaluator,
    inputs: [Ciphertext; 4],
    g: &Grid,
    spec: &SignSpec,
    sk: Option<&SecretKey>,
    cheat: Cheat,
) -> Vec<Vec<f64>> {
    let spec = match cheat {
        Cheat::TruncateSign(n) => SignSpec {
            kind: AbsKind::F1,
            iters: n,
        },
        _ => spec.clone(),
    };
    let out_ct = eval_matching(eval, inputs, &spec);
    let slots = if let Some(sk) = sk {
        decode_plaintext(ctx, &decrypt(ctx, sk, &out_ct))
    } else {
        vec![0.0; 64]
    };
    decode_slots_to_trades(g, &slots, cheat)
}

pub fn round_output(g: &Grid, t_hat: &[Vec<f64>], kappa: u64, cheat: Cheat) -> Trades {
    let mut hat = t_hat.to_vec();
    if cheat == Cheat::TamperOutput {
        if hat[0][0] > 0.0 {
            hat[0][0] += 1000.0;
        }
    }
    round_trades(g, &hat, kappa)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bt_arith::SplitMix64;
    use bt_market::{monotone_coupling, Grid};

    fn preset_books() -> (Vec<u64>, Vec<u64>) {
        (
            vec![0, 5000, 10000, 25000, 30000, 20000, 15000, 10000],
            vec![10000, 15000, 25000, 30000, 20000, 8000, 0, 0],
        )
    }

    #[test]
    fn interval_identity_on_random_pairs() {
        let mut rng = SplitMix64(7);
        for _ in 0..200_000 {
            let mut pts = [0.0; 4];
            for p in &mut pts {
                *p = rng.below(800_000) as f64;
            }
            pts.sort_by(|a, b| a.partial_cmp(b).unwrap());
            // four random endpoints, assigned as two intervals (possibly nested/disjoint)
            let (c1, d1) = if rng.below(2) == 0 {
                (pts[0], pts[2])
            } else {
                (pts[0], pts[1])
            };
            let (c2, d2) = if rng.below(2) == 0 {
                (pts[1], pts[3])
            } else {
                (pts[2], pts[3])
            };
            let naive = (d1.min(d2) - c1.max(c2)).max(0.0);
            let id = interval_overlap_plus(c1, d1, c2, d2, |x| x.abs());
            assert!(
                (naive - id).abs() < 1e-9,
                "c1={c1} d1={d1} c2={c2} d2={d2} naive={naive} id={id}"
            );
        }
    }

    #[test]
    fn exact_circuit_matches_coupling() {
        let g = Grid::DEMO;
        let (a, b) = preset_books();
        let t = monotone_coupling(&g, &a, &b);
        let hat = circuit_f64_exact(&g, &a, &b);
        for i in 0..8 {
            for j in 0..8 {
                assert!((hat[i][j] - t[i][j] as f64).abs() < 1e-6, "{i},{j}");
            }
        }
    }

    #[test]
    fn interval_circuit_matches_coupling_random_books() {
        let g = Grid::DEMO;
        let mut rng = SplitMix64(11);
        for _ in 0..20_000 {
            let a: Vec<u64> = (0..8).map(|_| rng.below(50_001)).collect();
            let b: Vec<u64> = (0..8).map(|_| rng.below(50_001)).collect();
            let t = monotone_coupling(&g, &a, &b);
            let hat = circuit_f64_exact(&g, &a, &b);
            for i in 0..8 {
                for j in 0..8 {
                    assert!(
                        (hat[i][j] - t[i][j] as f64).abs() < 1e-6,
                        "{i},{j} {} vs {}",
                        hat[i][j],
                        t[i][j]
                    );
                }
            }
        }
    }

    #[test]
    fn abs_polynomial_bench() {
        let range = 800_000.0;
        let n = 20_000usize;
        let mut rng = SplitMix64(19);
        let samples: Vec<f64> = (0..n)
            .map(|_| (rng.below(2 * 800_000 + 1) as f64) - range)
            .collect();
        let target = 1.0; // micro-lots
        eprintln!("|x| harness: {} samples on [−{range}, {range}]", n);
        eprintln!("kind  iters  max ε_|x| (µ-lots)  levels≈");
        let kinds = [AbsKind::F1, AbsKind::F2, AbsKind::F3];
        let mut he_ok = false;
        for kind in kinds {
            let _per = SignSpec { kind, iters: 1 }.levels_per_iter();
            let max_iters = match kind {
                AbsKind::F1 => 32,
                AbsKind::F2 => 22,
                AbsKind::F3 => 18,
            };
            for iters in 1..=max_iters {
                let spec = SignSpec { kind, iters };
                let mut max_e = 0.0f64;
                for &x in &samples {
                    let xn = x / NORM;
                    if xn.abs() > 1.0 {
                        continue;
                    }
                    let e = (x.abs() - abs_approx(xn, &spec) * NORM).abs();
                    max_e = max_e.max(e);
                }
                let levels = spec.estimated_levels();
                if iters == 1
                    || iters == max_iters
                    || max_e < target
                    || iters % 4 == 0
                {
                    eprintln!("{kind:?}  {iters:5}  {max_e:12.4}  {levels}");
                }
                if kind == AbsKind::F3 && iters == HE_F3_ITERS {
                    eprintln!("  HE_F3_ITERS={HE_F3_ITERS} ε={max_e:.4} (target < {target})");
                    he_ok = max_e < target;
                }
                if max_e < target {
                    break;
                }
            }
        }
        assert!(
            he_ok,
            "HE_F3_ITERS={HE_F3_ITERS} does not meet ε_|x| < 1 µ-lot; bump iters"
        );
    }

    #[test]
    fn poly_circuit_certifies_preset() {
        let g = Grid::DEMO;
        let (a, b) = preset_books();
        let spec = SignSpec::he();
        let hat = circuit_f64_poly(&g, &a, &b, &spec);
        let t = round_trades(&g, &hat, 2);
        let opt = monotone_coupling(&g, &a, &b);
        let mut max_err = 0.0f64;
        for i in 0..8 {
            for j in 0..8 {
                max_err = max_err.max((hat[i][j] - opt[i][j] as f64).abs());
            }
        }
        assert!(max_err < 2.0, "interval+|x| poly max |T̂-T*|={max_err}");
        let _ = t;
    }

    #[test]
    fn preset_json_matches_exact_circuit() {
        let raw = include_str!("../../../test-vectors/preset.json");
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        let a: Vec<u64> = v["book_a"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_u64().unwrap())
            .collect();
        let b: Vec<u64> = v["book_b"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_u64().unwrap())
            .collect();
        let g = Grid::DEMO;
        let hat = circuit_f64_exact(&g, &a, &b);
        let exact = v["exact_circuit"].as_array().unwrap();
        for i in 0..8 {
            for j in 0..8 {
                let want = exact[i][j].as_f64().unwrap();
                assert!(
                    (hat[i][j] - want).abs() < 1e-6,
                    "{i},{j}: {} vs {want}",
                    hat[i][j]
                );
            }
        }
    }

    #[test]
    fn homomorphic_matching_meets_kappa() {
        use bt_ckks::{gen_public, gen_relin_keys, gen_secret, Context, Params};
        use rand_chacha::ChaCha20Rng;
        use rand_core::SeedableRng;
        let ctx = Context::new(&Params::toy_test());
        let mut rng = ChaCha20Rng::from_seed([9u8; 32]);
        let sk = gen_secret(&ctx, &mut rng);
        let pk = gen_public(&ctx, &sk, &mut rng);
        let evk = gen_relin_keys(&ctx, &sk, &mut rng);
        let eval = Evaluator::new(&ctx, &evk);
        let g = Grid::DEMO;
        let (a, b) = preset_books();
        let cts = encrypt_inputs(&ctx, &pk, &g, &a, &b, &mut rng);
        let hat = run_encrypted(
            &ctx,
            &eval,
            cts,
            &g,
            &SignSpec::he(),
            Some(&sk),
            Cheat::None,
        );
        let opt = circuit_f64_exact(&g, &a, &b);
        let poly = circuit_f64_poly(&g, &a, &b, &SignSpec::he());
        let mut max_err = 0.0f64;
        let mut max_vs_poly = 0.0f64;
        for i in 0..8 {
            for j in 0..8 {
                max_err = max_err.max((hat[i][j] - opt[i][j]).abs());
                max_vs_poly = max_vs_poly.max((hat[i][j] - poly[i][j]).abs());
            }
        }
        eprintln!("HE matching max |T̂-T*| = {max_err} micro-lots (κ=2)");
        eprintln!("HE vs f64-poly(he) max = {max_vs_poly}");
        assert!(
            hat.iter().flatten().all(|x| x.is_finite()),
            "non-finite HE output"
        );
        assert!(
            max_err < 2.0,
            "Theorem 7: |T̂-T*|={max_err} ≥ κ=2"
        );
    }
}
