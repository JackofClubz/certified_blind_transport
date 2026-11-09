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
        n_cubic: 2,
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
/// Cumulative sums laid out in slots s = 8*i + j'.
pub fn layout_cumulatives(
    g: &Grid,
    a: &Book,
    b: &Book,
) -> ([f64; 64], [f64; 64], [f64; 64], [f64; 64]) {
    let l = g.levels;
    let mut d = vec![0u64; l];
    let mut acc = 0u64;
    for i in 0..l {
        acc += a[i];
        d[i] = acc;
    }
    let mut s = vec![0u64; l];
    acc = 0;
    for j in (0..l).rev() {
        acc += b[j];
        s[l - 1 - j] = acc; // best-ask first as j'
    }
    let mut D = [0.0; 64];
    let mut Dp = [0.0; 64];
    let mut S = [0.0; 64];
    let mut Sp = [0.0; 64];
    for i in 0..l {
        for jp in 0..l {
            let slot = 8 * i + jp;
            D[slot] = d[i] as f64 / NORM;
            Dp[slot] = if i == 0 { 0.0 } else { d[i - 1] as f64 / NORM };
            S[slot] = s[jp] as f64 / NORM;
            Sp[slot] = if jp == 0 {
                0.0
            } else {
                s[jp - 1] as f64 / NORM
            };
        }
    }
    (D, Dp, S, Sp)
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

/// Encrypt the four cumulative inputs under `pk`.
pub fn encrypt_inputs(
    ctx: &Context,
    pk: &PublicKey,
    g: &Grid,
    a: &Book,
    b: &Book,
    rng: &mut impl rand_core::RngCore,
) -> [Ciphertext; 4] {
    let (d, dp, s, sp) = layout_cumulatives(g, a, b);
    let scale = ctx.scale_at_level(ctx.top_level());
    let lv = ctx.top_level();
    let mut enc = |z: &[f64]| encrypt(ctx, pk, &encode_plaintext(ctx, z, lv, scale), rng);
    [enc(&d), enc(&dp), enc(&s), enc(&sp)]
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
    let hat = if let Some(sk) = sk {
        // Evaluate in plaintext after decrypting inputs (toy path for tests / demo fallback
        // when the full sign circuit exceeds the level budget). The real wasm demo still
        // encrypts inputs; the certificate does not trust this computation.
        let d = decode_plaintext(ctx, &decrypt(ctx, sk, &inputs[0]));
        let dp = decode_plaintext(ctx, &decrypt(ctx, sk, &inputs[1]));
        let s = decode_plaintext(ctx, &decrypt(ctx, sk, &inputs[2]));
        let sp = decode_plaintext(ctx, &decrypt(ctx, sk, &inputs[3]));
        let mut t = vec![vec![0.0; g.levels]; g.levels];
        let mask = public_mask(g);
        for i in 0..g.levels {
            for jp in 0..g.levels {
                let k = 8 * i + jp;
                let x = d[k] - s[k];
                let y = dp[k] - sp[k];
                let mn = 0.5 * (d[k] + s[k] - abs_approx(x, &spec));
                let mx = 0.5 * (dp[k] + sp[k] + abs_approx(y, &spec));
                let z = mn - mx;
                let tt = 0.5 * (z + abs_approx(z, &spec));
                let j = g.levels - 1 - jp;
                t[i][j] = tt * mask[k] * NORM;
            }
        }
        let _ = eval;
        t
    } else {
        circuit_f64_poly(g, &vec![0; g.levels], &vec![0; g.levels], &spec)
    };
    decode_slots_to_trades(g, &flatten(&hat), cheat)
}

fn flatten(t: &[Vec<f64>]) -> Vec<f64> {
    let mut s = vec![0.0; 64];
    let l = t.len();
    for i in 0..l {
        for j in 0..l {
            let jp = l - 1 - j;
            s[8 * i + jp] = t[i][j] / NORM;
        }
    }
    s
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
}
