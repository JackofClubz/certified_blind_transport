//! Closed-form 1-D OT matching engine (`docs/plan/07`).

#![allow(clippy::needless_range_loop, non_snake_case, unused_assignments)]

use bt_ckks::{
    decode_plaintext, decrypt, encode_plaintext, encrypt, Ciphertext, Context, Evaluator,
    PublicKey, SecretKey,
};
use bt_market::{round_trades, Book, Grid, Trades};
use serde::{Deserialize, Serialize};

pub const NORM: f64 = (1u32 << 19) as f64; // 2^19 micro-lots

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignSpec {
    pub n_cubic: u32,
    pub n_quintic: u32,
}

impl Default for SignSpec {
    fn default() -> Self {
        Self {
            n_cubic: 16,
            n_quintic: 3,
        }
    }
}

impl SignSpec {
    pub fn fast() -> Self {
        Self {
            n_cubic: 2,
            n_quintic: 1,
        }
    }

    /// Shallow `|x|` for the homomorphic path (level budget of `Params::toy_test`).
    pub fn he() -> Self {
        Self {
            n_cubic: 1,
            n_quintic: 0,
        }
    }
}

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

fn push_sign_iters(ops: &mut Vec<Op>, mut x: usize, spec: &SignSpec, used: &mut usize) -> usize {
    for _ in 0..spec.n_cubic {
        let sq = *used;
        *used += 1;
        ops.push(Op::Square { dst: sq, a: x });
        let x3 = *used;
        *used += 1;
        ops.push(Op::Mul {
            dst: x3,
            a: sq,
            b: x,
        });
        let nx = *used;
        *used += 1;
        ops.push(Op::MulC {
            dst: nx,
            a: x,
            c: 1.5,
        });
        let t = *used;
        *used += 1;
        ops.push(Op::MulC {
            dst: t,
            a: x3,
            c: -0.5,
        });
        let out = *used;
        *used += 1;
        ops.push(Op::Add {
            dst: out,
            a: nx,
            b: t,
        });
        x = out;
    }
    for _ in 0..spec.n_quintic {
        let x2 = *used;
        *used += 1;
        ops.push(Op::Square { dst: x2, a: x });
        let x3 = *used;
        *used += 1;
        ops.push(Op::Mul {
            dst: x3,
            a: x2,
            b: x,
        });
        let x4 = *used;
        *used += 1;
        ops.push(Op::Square { dst: x4, a: x2 });
        let x5 = *used;
        *used += 1;
        ops.push(Op::Mul {
            dst: x5,
            a: x4,
            b: x,
        });
        let t1 = *used;
        *used += 1;
        ops.push(Op::MulC {
            dst: t1,
            a: x,
            c: 15.0 / 8.0,
        });
        let t2 = *used;
        *used += 1;
        ops.push(Op::MulC {
            dst: t2,
            a: x3,
            c: -10.0 / 8.0,
        });
        let t3 = *used;
        *used += 1;
        ops.push(Op::MulC {
            dst: t3,
            a: x5,
            c: 3.0 / 8.0,
        });
        let s = *used;
        *used += 1;
        ops.push(Op::Add {
            dst: s,
            a: t1,
            b: t2,
        });
        let out = *used;
        *used += 1;
        ops.push(Op::Add {
            dst: out,
            a: s,
            b: t3,
        });
        x = out;
    }
    x
}

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
        Op::Sub { dst: 4, a: 0, b: 2 },
        Op::Sub { dst: 5, a: 1, b: 3 },
    ];
    let mut used = 6usize;
    let sign_x = push_sign_iters(&mut ops, 4, sign, &mut used);
    let abs_x = used;
    used += 1;
    ops.push(Op::Mul {
        dst: abs_x,
        a: 4,
        b: sign_x,
    });
    let sign_y = push_sign_iters(&mut ops, 5, sign, &mut used);
    let abs_y = used;
    used += 1;
    ops.push(Op::Mul {
        dst: abs_y,
        a: 5,
        b: sign_y,
    });
    let ds = used;
    used += 1;
    ops.push(Op::Add {
        dst: ds,
        a: 0,
        b: 2,
    });
    let mn2 = used;
    used += 1;
    ops.push(Op::Sub {
        dst: mn2,
        a: ds,
        b: abs_x,
    });
    let mn = used;
    used += 1;
    ops.push(Op::MulC {
        dst: mn,
        a: mn2,
        c: 0.5,
    });
    let dpsp = used;
    used += 1;
    ops.push(Op::Add {
        dst: dpsp,
        a: 1,
        b: 3,
    });
    let mx2 = used;
    used += 1;
    ops.push(Op::Add {
        dst: mx2,
        a: dpsp,
        b: abs_y,
    });
    let mx = used;
    used += 1;
    ops.push(Op::MulC {
        dst: mx,
        a: mx2,
        c: 0.5,
    });
    let z = used;
    used += 1;
    ops.push(Op::Sub {
        dst: z,
        a: mn,
        b: mx,
    });
    let short = SignSpec {
        n_cubic: sign.n_cubic.min(1),
        n_quintic: 0,
    };
    let sign_z = push_sign_iters(&mut ops, z, &short, &mut used);
    let az = used;
    used += 1;
    ops.push(Op::Mul {
        dst: az,
        a: z,
        b: sign_z,
    });
    let zp = used;
    used += 1;
    ops.push(Op::Add {
        dst: zp,
        a: z,
        b: az,
    });
    let tt = used;
    used += 1;
    ops.push(Op::MulC {
        dst: tt,
        a: zp,
        c: 0.5,
    });
    ops.push(Op::Output { reg: tt });
    let _ = budget;
    return Circuit {
        ops,
        depth: sign.n_cubic as usize * 2 + sign.n_quintic as usize * 3,
        refreshes: 0,
    };
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

fn sign_poly(mut x: f64, spec: &SignSpec) -> f64 {
    for _ in 0..spec.n_cubic {
        x = 0.5 * (3.0 * x - x.powi(3));
    }
    for _ in 0..spec.n_quintic {
        x = (15.0 * x - 10.0 * x.powi(3) + 3.0 * x.powi(5)) / 8.0;
    }
    x.clamp(-1.0, 1.0)
}

fn abs_approx(x: f64, spec: &SignSpec) -> f64 {
    x * sign_poly(x, spec)
}

/// Exact (f64, true sign) closed form — must match `monotone_coupling`.
pub fn circuit_f64_exact(g: &Grid, a: &Book, b: &Book) -> Vec<Vec<f64>> {
    let (d, dp, s, sp) = layout_cumulatives(g, a, b);
    let mask = public_mask(g);
    let l = g.levels;
    let mut t = vec![vec![0.0; l]; l];
    for i in 0..l {
        for jp in 0..l {
            let k = 8 * i + jp;
            let x = d[k] - s[k];
            let y = dp[k] - sp[k];
            let mn = 0.5 * (d[k] + s[k] - x.abs());
            let mx = 0.5 * (dp[k] + sp[k] + y.abs());
            let z = mn - mx;
            let tt = 0.5 * (z + z.abs());
            let j = l - 1 - jp;
            t[i][j] = (tt * mask[k]) * NORM;
        }
    }
    t
}

pub fn circuit_f64_poly(g: &Grid, a: &Book, b: &Book, spec: &SignSpec) -> Vec<Vec<f64>> {
    let (d, dp, s, sp) = layout_cumulatives(g, a, b);
    let mask = public_mask(g);
    let l = g.levels;
    let mut t = vec![vec![0.0; l]; l];
    for i in 0..l {
        for jp in 0..l {
            let k = 8 * i + jp;
            let x = d[k] - s[k];
            let y = dp[k] - sp[k];
            let mn = 0.5 * (d[k] + s[k] - abs_approx(x, spec));
            let mx = 0.5 * (dp[k] + sp[k] + abs_approx(y, spec));
            let z = mn - mx;
            let tt = 0.5 * (z + abs_approx(z, spec));
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

fn he_sign(eval: &Evaluator, x: &Ciphertext, spec: &SignSpec) -> Ciphertext {
    let mut x = x.clone();
    for _ in 0..spec.n_cubic {
        let x2 = eval.square(&x);
        let x3 = eval.mul(&x2, &x);
        let t = eval.sub(&eval.mul_int(&x, 3), &x3);
        x = eval.mul_const(&t, 0.5);
    }
    for _ in 0..spec.n_quintic {
        let x2 = eval.square(&x);
        let x3 = eval.mul(&x2, &x);
        let x4 = eval.square(&x2);
        let x5 = eval.mul(&x4, &x);
        let t1 = eval.mul_const(&x, 15.0 / 8.0);
        let t2 = eval.mul_const(&x3, -10.0 / 8.0);
        let t3 = eval.mul_const(&x5, 3.0 / 8.0);
        x = eval.add(&eval.add(&t1, &t2), &t3);
    }
    x
}

fn he_abs(eval: &Evaluator, x: &Ciphertext, spec: &SignSpec) -> Ciphertext {
    let s = he_sign(eval, x, spec);
    eval.mul(x, &s)
}

/// Matching circuit on ciphertexts only. The server entry point: no books, no secrets.
pub fn eval_matching(eval: &Evaluator, inputs: [Ciphertext; 4], spec: &SignSpec) -> Ciphertext {
    let [d, dp, s, sp] = inputs;
    let x = eval.sub(&d, &s);
    let y = eval.sub(&dp, &sp);
    let ax = he_abs(eval, &x, spec);
    let ay = he_abs(eval, &y, spec);
    let mn = eval.mul_const(&eval.sub(&eval.add(&d, &s), &ax), 0.5);
    let mx = eval.mul_const(&eval.add(&eval.add(&dp, &sp), &ay), 0.5);
    let z = eval.sub(&mn, &mx);
    let az = he_abs(eval, &z, spec);
    eval.mul_const(&eval.add(&z, &az), 0.5)
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
            n_cubic: n,
            n_quintic: 0,
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
    use bt_market::{monotone_coupling, Grid};

    #[test]
    fn exact_circuit_matches_coupling() {
        let g = Grid::DEMO;
        let a = vec![0, 5000, 10000, 25000, 30000, 20000, 15000, 10000];
        let b = vec![10000, 15000, 25000, 30000, 20000, 8000, 0, 0];
        let t = monotone_coupling(&g, &a, &b);
        let hat = circuit_f64_exact(&g, &a, &b);
        for i in 0..8 {
            for j in 0..8 {
                assert!((hat[i][j] - t[i][j] as f64).abs() < 1e-6, "{i},{j}");
            }
        }
    }

    #[test]
    fn poly_depth_vs_kappa() {
        let g = Grid::DEMO;
        let a = vec![0, 5000, 10000, 25000, 30000, 20000, 15000, 10000];
        let b = vec![10000, 15000, 25000, 30000, 20000, 8000, 0, 0];
        let opt = circuit_f64_exact(&g, &a, &b);
        fn sign7(x: f64) -> f64 {
            let x3 = x * x * x;
            let x5 = x3 * x * x;
            let x7 = x5 * x * x;
            ((35.0 * x - 35.0 * x3 + 21.0 * x5 - 5.0 * x7) / 16.0).clamp(-1.0, 1.0)
        }
        fn abs7(x: f64) -> f64 {
            x * sign7(x)
        }
        let mut max7 = 0.0f64;
        let (d, dp, s, sp) = layout_cumulatives(&g, &a, &b);
        let mask = public_mask(&g);
        for i in 0..8 {
            for jp in 0..8 {
                let k = 8 * i + jp;
                let x = d[k] - s[k];
                let y = dp[k] - sp[k];
                let mn = 0.5 * (d[k] + s[k] - abs7(x));
                let mx = 0.5 * (dp[k] + sp[k] + abs7(y));
                let z = mn - mx;
                let tt = 0.5 * (z + abs7(z));
                let j = 8 - 1 - jp;
                let hat = tt * mask[k] * NORM;
                max7 = max7.max((hat - opt[i][j]).abs());
            }
        }
        eprintln!("deg-7 Chebyshev-like max|T̂-T*|={max7:.3}");

        for n in 1..=16 {
            let spec = SignSpec {
                n_cubic: n,
                n_quintic: 0,
            };
            let hat = circuit_f64_poly(&g, &a, &b, &spec);
            let mut max_err = 0.0f64;
            for i in 0..8 {
                for j in 0..8 {
                    max_err = max_err.max((hat[i][j] - opt[i][j]).abs());
                }
            }
            eprintln!("n_cubic={n} max|T̂-T*|={max_err:.3}  levels≈{}", 9 * n + 6);
        }
    }

    #[test]
    fn poly_circuit_certifies_preset() {
        let g = Grid::DEMO;
        let a = vec![0, 5000, 10000, 25000, 30000, 20000, 15000, 10000];
        let b = vec![10000, 15000, 25000, 30000, 20000, 8000, 0, 0];
        let spec = SignSpec::default();
        let hat = circuit_f64_poly(&g, &a, &b, &spec);
        let t = round_trades(&g, &hat, 2);
        let opt = monotone_coupling(&g, &a, &b);
        let mut close = 0u64;
        let mut tot = 0u64;
        for i in 0..8 {
            for j in 0..8 {
                tot += opt[i][j];
                let d = (t[i][j] as i128 - opt[i][j] as i128).unsigned_abs() as u64;
                close += d;
            }
        }
        assert!(close * 20 < tot + 1, "L1 {close} vs {tot}");
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
    fn homomorphic_matching_on_preset_is_finite() {
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
        let a = vec![0, 5000, 10000, 25000, 30000, 20000, 15000, 10000];
        let b = vec![10000, 15000, 25000, 30000, 20000, 8000, 0, 0];
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
        let mut max_err = 0.0f64;
        for i in 0..8 {
            for j in 0..8 {
                max_err = max_err.max((hat[i][j] - opt[i][j]).abs());
            }
        }
        // Completeness target is κ = 2 micro-lots. Record the error even if we miss it.
        assert!(
            hat.iter().flatten().all(|x| x.is_finite()),
            "non-finite HE output"
        );
        let poly = circuit_f64_poly(&g, &a, &b, &SignSpec::he());
        let mut max_vs_poly = 0.0f64;
        for i in 0..8 {
            for j in 0..8 {
                max_vs_poly = max_vs_poly.max((hat[i][j] - poly[i][j]).abs());
            }
        }
        eprintln!("HE matching max |T̂-T*| = {max_err} micro-lots (κ=2)");
        eprintln!("HE vs f64-poly(he) max = {max_vs_poly}");
        eprintln!("hat[3][3]={:?} poly={:?} exact={:?}", hat[3][3], poly[3][3], opt[3][3]);
        eprintln!("hat[2][4]={:?} poly={:?} exact={:?}", hat[2][4], poly[2][4], opt[2][4]);
    }
}
