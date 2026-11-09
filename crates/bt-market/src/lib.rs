//! `bt-market`: the market model, the closed-form one-dimensional optimal transport, and the
//! clearing-price optimality certificate, all in exact integers.
//!
//! The mathematics (with proofs) is in `docs/M1-market-math.md`. Section numbers in the comments
//! below refer to that document.
//!
//! Units: volumes in **micro-lots** (`u64`), prices in **quarter-ticks** (`i64`),
//! gains and gaps in quarter-tick·micro-lots (`i128`).

// Index loops mirror the maths (T[i][j], w_ij), so we keep them.
#![allow(clippy::needless_range_loop)]

use std::fmt;

/// The public price grid (§1.1). Level `k` has tick index `t_k = top_ticks - k` (level 0 = highest price).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grid {
    pub levels: usize,
    pub top_ticks: i64,
}

impl Grid {
    /// The demo grid: 8 levels, $100.40 down to $99.70 with a $0.10 tick.
    pub const DEMO: Grid = Grid {
        levels: 8,
        top_ticks: 1004,
    };

    pub fn ticks(&self, k: usize) -> i64 {
        self.top_ticks - k as i64
    }
    /// Buyer's value `B_i = 4 t_i + 1` (§1.3).
    pub fn b(&self, i: usize) -> i64 {
        4 * self.ticks(i) + 1
    }
    /// Seller's cost `A_j = 4 t_j - 1` (§1.3).
    pub fn a(&self, j: usize) -> i64 {
        4 * self.ticks(j) - 1
    }
    /// Gain from trade `w_ij = B_i - A_j = 4(t_i - t_j) + 2`; positive iff `i <= j`.
    pub fn w(&self, i: usize, j: usize) -> i64 {
        self.b(i) - self.a(j)
    }
    pub fn compatible(&self, i: usize, j: usize) -> bool {
        i <= j
    }
    /// Largest possible gain `w_max = 4(L-1) + 2`.
    pub fn w_max(&self) -> i64 {
        4 * (self.levels as i64 - 1) + 2
    }
    /// The `2L` clearing-price candidates `{B_i} ∪ {A_j}`, sorted ascending (§5).
    pub fn candidates(&self) -> Vec<i64> {
        let mut c: Vec<i64> = (0..self.levels)
            .flat_map(|k| [self.b(k), self.a(k)])
            .collect();
        c.sort_unstable();
        c.dedup();
        c
    }
}

/// Volumes per level, in micro-lots.
pub type Book = Vec<u64>;
/// `T[i][j]`: volume traded between bid level `i` and ask level `j`, in micro-lots.
pub type Trades = Vec<Vec<u64>>;

fn zeros(l: usize) -> Trades {
    vec![vec![0; l]; l]
}

/// Closed-form 1-D optimal transport: the masked monotone coupling (§2, Theorem 1).
///
/// Demand units are ordered best bid first (`i = 0, 1, …`); supply units best ask first
/// (`j = L-1, L-2, …`). Cell `(i, j)` receives the overlap of the two unit intervals, and
/// incompatible cells (`i > j`) are dropped.
pub fn monotone_coupling(g: &Grid, a: &Book, b: &Book) -> Trades {
    let l = g.levels;
    assert_eq!(a.len(), l);
    assert_eq!(b.len(), l);
    // demand intervals [D_{i-1}, D_i)
    let mut d = Vec::with_capacity(l);
    let mut acc = 0u64;
    for &x in a {
        d.push((acc, acc + x));
        acc += x;
    }
    // supply intervals, best ask (lowest price, highest level index) first
    let mut s = vec![(0u64, 0u64); l];
    let mut acc = 0u64;
    for j in (0..l).rev() {
        s[j] = (acc, acc + b[j]);
        acc += b[j];
    }
    let mut t = zeros(l);
    for i in 0..l {
        for j in i..l {
            let lo = d[i].0.max(s[j].0);
            let hi = d[i].1.min(s[j].1);
            if hi > lo {
                t[i][j] = hi - lo;
            }
        }
    }
    t
}

/// Total gain from trade `W(T) = Σ w_ij T_ij` (§1.4).
pub fn welfare(g: &Grid, t: &Trades) -> i128 {
    let mut w = 0i128;
    for (i, row) in t.iter().enumerate() {
        for (j, &v) in row.iter().enumerate() {
            w += g.w(i, j) as i128 * v as i128;
        }
    }
    w
}

pub fn row_sums(t: &Trades) -> Vec<u64> {
    t.iter().map(|r| r.iter().sum()).collect()
}
pub fn col_sums(t: &Trades) -> Vec<u64> {
    let l = t.len();
    (0..l).map(|j| t.iter().map(|r| r[j]).sum()).collect()
}

/// Residuals `r_i = a_i - Σ_j T_ij` and `r'_j = b_j - Σ_i T_ij` (may be negative for infeasible T).
pub fn residuals(a: &Book, b: &Book, t: &Trades) -> (Vec<i128>, Vec<i128>) {
    let rs = row_sums(t);
    let cs = col_sums(t);
    (
        a.iter()
            .zip(&rs)
            .map(|(&x, &y)| x as i128 - y as i128)
            .collect(),
        b.iter()
            .zip(&cs)
            .map(|(&x, &y)| x as i128 - y as i128)
            .collect(),
    )
}

/// Uniform-price dual solution (§4): `U_i = max(0, B_i - Π)`, `S_j = max(0, Π - A_j)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Surplus {
    pub u: Vec<i64>,
    pub s: Vec<i64>,
}

pub fn surplus(g: &Grid, pi: i64) -> Surplus {
    Surplus {
        u: (0..g.levels).map(|i| (g.b(i) - pi).max(0)).collect(),
        s: (0..g.levels).map(|j| (pi - g.a(j)).max(0)).collect(),
    }
}

/// Check dual feasibility `U_i + S_j >= w_ij`, `U, S >= 0` for every pair (Lemma 3; always true).
pub fn dual_feasible(g: &Grid, sp: &Surplus) -> bool {
    sp.u.iter().all(|&x| x >= 0)
        && sp.s.iter().all(|&x| x >= 0)
        && (0..g.levels).all(|i| (0..g.levels).all(|j| sp.u[i] + sp.s[j] >= g.w(i, j)))
}

/// The three parts of the duality gap (§4, Theorem 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GapParts {
    /// `G_T = Σ T_ij (U_i + S_j - w_ij)`: depends only on public data.
    pub g_t: i128,
    /// `G_A = Σ U_i r_i`: Firm A's private term.
    pub g_a: i128,
    /// `G_B = Σ S_j r'_j`: Firm B's private term.
    pub g_b: i128,
}
impl GapParts {
    pub fn total(&self) -> i128 {
        self.g_t + self.g_a + self.g_b
    }
}

/// Public term `G_T(Π)`, computable by anyone from the trades.
pub fn gap_public(g: &Grid, t: &Trades, pi: i64) -> i128 {
    let sp = surplus(g, pi);
    let mut gt = 0i128;
    for (i, row) in t.iter().enumerate() {
        for (j, &v) in row.iter().enumerate() {
            if v > 0 {
                gt += v as i128 * (sp.u[i] + sp.s[j] - g.w(i, j)) as i128;
            }
        }
    }
    gt
}

/// Firm A's private term `G_A(Π) = Σ U_i r_i` (computed inside Firm A only).
pub fn gap_firm_a(g: &Grid, a: &Book, t: &Trades, pi: i64) -> i128 {
    let sp = surplus(g, pi);
    let rs = row_sums(t);
    (0..g.levels)
        .map(|i| sp.u[i] as i128 * (a[i] as i128 - rs[i] as i128))
        .sum()
}

/// Firm B's private term `G_B(Π) = Σ S_j r'_j` (computed inside Firm B only).
pub fn gap_firm_b(g: &Grid, b: &Book, t: &Trades, pi: i64) -> i128 {
    let sp = surplus(g, pi);
    let cs = col_sums(t);
    (0..g.levels)
        .map(|j| sp.s[j] as i128 * (b[j] as i128 - cs[j] as i128))
        .sum()
}

pub fn gap_parts(g: &Grid, a: &Book, b: &Book, t: &Trades, pi: i64) -> GapParts {
    GapParts {
        g_t: gap_public(g, t, pi),
        g_a: gap_firm_a(g, a, t, pi),
        g_b: gap_firm_b(g, b, t, pi),
    }
}

/// The gap in its defining form `G = Σ U_i a_i + Σ S_j b_j - W(T)` (Theorem 2).
pub fn gap(g: &Grid, a: &Book, b: &Book, t: &Trades, pi: i64) -> i128 {
    let sp = surplus(g, pi);
    let dual: i128 = (0..g.levels)
        .map(|k| sp.u[k] as i128 * a[k] as i128 + sp.s[k] as i128 * b[k] as i128)
        .sum();
    dual - welfare(g, t)
}

/// A clearing price certifying the optimal trades with gap exactly 0 (Theorem 5, constructive).
///
/// With `Q*` the optimally traded volume, `β` the demand value curve and `α` the supply cost curve
/// (both indexed by unit), any `Π` in
/// `[ max(α(Q*-1), β(Q*)), min(β(Q*-1), α(Q*)) ]` works, where terms that do not exist
/// (no traded unit, no unfilled unit) are dropped. Both endpoints are candidates.
pub fn equilibrium_price(g: &Grid, a: &Book, b: &Book) -> i64 {
    let l = g.levels;
    let total_a: u64 = a.iter().sum();
    let total_b: u64 = b.iter().sum();
    let t = monotone_coupling(g, a, b);
    let q_star: u64 = t.iter().flatten().sum();
    // β(u): value of the u-th demand unit (best first); α(u): cost of the u-th supply unit (best first)
    let beta = |u: u64| -> i64 {
        let mut acc = 0;
        for i in 0..l {
            acc += a[i];
            if u < acc {
                return g.b(i);
            }
        }
        unreachable!()
    };
    let alpha = |u: u64| -> i64 {
        let mut acc = 0;
        for j in (0..l).rev() {
            acc += b[j];
            if u < acc {
                return g.a(j);
            }
        }
        unreachable!()
    };
    let mut lo: Option<i64> = None;
    let mut hi: Option<i64> = None;
    let mut raise = |x: i64| lo = Some(lo.map_or(x, |y: i64| y.max(x)));
    if q_star > 0 {
        raise(alpha(q_star - 1));
    } // every traded ask must be at or below Π
    if q_star < total_a {
        raise(beta(q_star));
    } // every unfilled bid must be at or below Π
    let mut lower = |x: i64| hi = Some(hi.map_or(x, |y: i64| y.min(x)));
    if q_star > 0 {
        lower(beta(q_star - 1));
    } // every traded bid must be at or above Π
    if q_star < total_b {
        lower(alpha(q_star));
    } // every unfilled ask must be at or above Π
    match (lo, hi) {
        (Some(x), Some(y)) => {
            debug_assert!(x <= y, "empty clearing interval");
            x
        }
        (Some(x), None) => x,
        (None, Some(y)) => y,
        (None, None) => g.candidates()[0], // both books empty: any price certifies
    }
}

/// Public rounding rule (§6): `T_int = mask · max(0, floor(T̂ - κ))`, for decrypted trades `T̂` in micro-lots.
pub fn round_trades(g: &Grid, t_hat: &[Vec<f64>], kappa: u64) -> Trades {
    let l = g.levels;
    let mut t = zeros(l);
    for i in 0..l {
        for j in 0..l {
            if g.compatible(i, j) {
                let v = (t_hat[i][j] - kappa as f64).floor();
                if v > 0.0 {
                    t[i][j] = v as u64;
                }
            }
        }
    }
    t
}

/// Tolerances for the three gap parts (§6, Theorem 7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Thresholds {
    pub t: i128,
    pub a: i128,
    pub b: i128,
}

/// Rigorous tolerances for honest runs whose decryption error is below `κ` (Theorem 7):
/// `δ_T = 0` and `δ_A = δ_B = w_max · (2L - 1) · 2κ`.
pub fn thresholds(g: &Grid, kappa: u64) -> Thresholds {
    let part = g.w_max() as i128 * (2 * g.levels as i128 - 1) * (2 * kappa as i128);
    Thresholds {
        t: 0,
        a: part,
        b: part,
    }
}

/// A firm's bitmask over the candidates: `m[c] = (its private gap part at Π_c) <= its threshold` (§7).
pub fn bitmask_a(g: &Grid, a: &Book, t: &Trades, thr: &Thresholds) -> Vec<bool> {
    g.candidates()
        .iter()
        .map(|&p| gap_firm_a(g, a, t, p) <= thr.a)
        .collect()
}
pub fn bitmask_b(g: &Grid, b: &Book, t: &Trades, thr: &Thresholds) -> Vec<bool> {
    g.candidates()
        .iter()
        .map(|&p| gap_firm_b(g, b, t, p) <= thr.b)
        .collect()
}

/// Verifier: the smallest candidate with `G_T <= δ_T` and both firms' bits set (§7).
pub fn choose_pi(
    g: &Grid,
    t: &Trades,
    mask_a: &[bool],
    mask_b: &[bool],
    thr: &Thresholds,
) -> Option<i64> {
    g.candidates()
        .iter()
        .enumerate()
        .find(|&(c, &p)| mask_a[c] && mask_b[c] && gap_public(g, t, p) <= thr.t)
        .map(|(_, &p)| p)
}

/// Why a certificate check failed (plaintext oracle mirroring `docs/08` §4.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CertFailure {
    IncompatibleTrade { i: usize, j: usize },
    NegativeResidualA { level: usize, residual: i128 },
    NegativeResidualB { level: usize, residual: i128 },
    DualInfeasible,
    PublicGapTooLarge { g_t: i128, limit: i128 },
    FirmAGapTooLarge { g_a: i128, limit: i128 },
    FirmBGapTooLarge { g_b: i128, limit: i128 },
}

impl fmt::Display for CertFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Plaintext certificate check with full knowledge (test oracle). In the real protocol, checks on
/// residuals and private gap parts are zero-knowledge range proofs on commitments (`docs/08`).
pub fn certify_plain(
    g: &Grid,
    a: &Book,
    b: &Book,
    t: &Trades,
    pi: i64,
    thr: &Thresholds,
) -> Result<GapParts, CertFailure> {
    for (i, row) in t.iter().enumerate() {
        for (j, &v) in row.iter().enumerate() {
            if v > 0 && !g.compatible(i, j) {
                return Err(CertFailure::IncompatibleTrade { i, j });
            }
        }
    }
    let (ra, rb) = residuals(a, b, t);
    if let Some((level, &r)) = ra.iter().enumerate().find(|(_, &r)| r < 0) {
        return Err(CertFailure::NegativeResidualA { level, residual: r });
    }
    if let Some((level, &r)) = rb.iter().enumerate().find(|(_, &r)| r < 0) {
        return Err(CertFailure::NegativeResidualB { level, residual: r });
    }
    if !dual_feasible(g, &surplus(g, pi)) {
        return Err(CertFailure::DualInfeasible);
    }
    let parts = gap_parts(g, a, b, t, pi);
    if parts.g_t > thr.t {
        return Err(CertFailure::PublicGapTooLarge {
            g_t: parts.g_t,
            limit: thr.t,
        });
    }
    if parts.g_a > thr.a {
        return Err(CertFailure::FirmAGapTooLarge {
            g_a: parts.g_a,
            limit: thr.a,
        });
    }
    if parts.g_b > thr.b {
        return Err(CertFailure::FirmBGapTooLarge {
            g_b: parts.g_b,
            limit: thr.b,
        });
    }
    Ok(parts)
}

/// Independent upper bound on welfare (proof of Theorem 1): `Σ_u max(0, β(u) - α(u))` over the
/// first `min(ΣA, ΣB)` units, computed level-run by level-run (no coupling involved).
pub fn quantile_welfare_bound(g: &Grid, a: &Book, b: &Book) -> i128 {
    let l = g.levels;
    let mut runs_d: Vec<(u64, i64)> = (0..l)
        .filter(|&i| a[i] > 0)
        .map(|i| (a[i], g.b(i)))
        .collect();
    let mut runs_s: Vec<(u64, i64)> = (0..l)
        .rev()
        .filter(|&j| b[j] > 0)
        .map(|j| (b[j], g.a(j)))
        .collect();
    runs_d.reverse(); // use as stacks: pop from the end = best first
    runs_s.reverse();
    let mut total = 0i128;
    while let (Some(&(nd, vd)), Some(&(ns, vs))) = (runs_d.last(), runs_s.last()) {
        let take = nd.min(ns);
        total += take as i128 * (vd - vs).max(0) as i128;
        if nd == take {
            runs_d.pop();
        } else {
            runs_d.last_mut().unwrap().0 -= take;
        }
        if ns == take {
            runs_s.pop();
        } else {
            runs_s.last_mut().unwrap().0 -= take;
        }
    }
    total
}

#[cfg(test)]
mod tests;
