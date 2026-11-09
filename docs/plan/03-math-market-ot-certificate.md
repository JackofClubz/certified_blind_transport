# 03. Mathematics: market model, optimal transport and the certificate

This is the specification for the `bt-market` crate and the mathematical core of `bt-cert`. Every formula here is integer-valued, so that commitments (which work over integers modulo the group order) can check it exactly.

## 1. Market model

### 1.1 Price grid (public)

- `L` price levels (demo: L = 8), tick size τ (demo: $0.10), top price P₀ (demo: $100.40).
- Level k ∈ {0, …, L−1} has price p_k = P₀ − k·τ (level 0 is the highest price).
- Integer tick index t_k = round(p_k / τ) (demo: 1004, 1003, …, 997).

### 1.2 Orders (private)

- Firm A (buyer) has bid size a_k ≥ 0 at level k. Firm B (seller) has ask size b_k ≥ 0 at level k.
- Sizes are integers in **micro-lots**: S_P = 1000 micro-lots per lot. The public per-level limit is LIMIT = 50 lots = 50,000 micro-lots.

### 1.3 Gains from trade, in integer units

A bid at level i and an ask at level j can trade when p_i ≥ p_j, that is i ≤ j. Their gain from trade is p_i − p_j ≥ 0. To make trades at zero spread strictly worthwhile, and to keep everything integer, use **quarter-tick units**:

```
B_i = 4·t_i + 1        (buyer's value, quarter-ticks)
A_j = 4·t_j − 1        (seller's cost, quarter-ticks)
w_ij = B_i − A_j = 4·(t_i − t_j) + 2
```

So w_ij ≥ 2 > 0 exactly when i ≤ j, and w_ij ≤ −2 < 0 when i > j. Incompatible pairs have negative value and are never traded in an optimal plan.

### 1.4 The matching problem (a linear programme)

```
maximise    W(T) = Σ_{i,j} w_ij · T_ij
subject to  Σ_j T_ij ≤ a_i      for every bid level i   (A's row constraint)
            Σ_i T_ij ≤ b_j      for every ask level j   (B's column constraint)
            T_ij ≥ 0
```

T_ij is the volume traded between bid level i and ask level j. This is an **optimal transport problem** with partial mass, the "unbalanced" form. It is equivalent to balanced OT with one dummy row and one dummy column of cost 0, which absorb unfilled volume; this is the form the old demo used.

## 2. Closed-form solution: one-dimensional OT

Because value depends on a single ordered quantity (price), the optimal plan has a closed form: the **monotone coupling**. Pair the best demand with the best supply, unit by unit, until the next pair would lose money.

### 2.1 Construction

Order bids best-first (index i = 0 is the highest price, as defined). Order asks best-first, lowest price first: write ã_{j'} = b_{L−1−j'}.

Cumulative volumes:

```
D_i  = Σ_{k ≤ i} a_k        D_{−1} = 0       (cumulative demand, best bid first)
S_j' = Σ_{k ≤ j'} ã_k       S_{−1} = 0       (cumulative supply, best ask first)
```

The coupling assigns to the pair (i, j') the overlap of the demand interval [D_{i−1}, D_i) and the supply interval [S_{j'−1}, S_{j'}):

```
T̃_{i j'} = max(0, min(D_i, S_j') − max(D_{i−1}, S_{j'−1}))
```

Map back to ask levels and remove incompatible pairs with the public mask M_ij = 1[i ≤ j]:

```
T_ij = M_ij · T̃_{i, L−1−j}
```

### 2.2 Why it is optimal (proof sketch, to write up fully in M1)

Think of total volume as a line. The q-th unit of demand (best first) has value B(q), non-increasing in q. The q-th unit of supply (best first) has cost A(q), non-decreasing in q. The monotone coupling pairs unit q with unit q, earning v(q) = B(q) − A(q), which is non-increasing.

- **Upper bound:** for any feasible plan, sort its traded units; an exchange argument (the rearrangement inequality) shows its welfare is at most ∫₀^{Q} v(q) dq over the volume Q it trades, which in turn is at most ∫₀^{Q*} v(q) dq with Q* = sup{q : v(q) > 0}.
- **Attainment:** because v is non-increasing, the units with v(q) > 0 form an initial segment [0, Q*), and every unit in that segment lies in a pair with i ≤ j. Masking incompatible pairs removes exactly the units beyond Q*. So the masked coupling earns exactly ∫₀^{Q*} v(q) dq.

**Empirical check** (`spikes/closed_form_1d_ot.py`): on 5,002 order books (the two presets plus 5,000 random), the masked monotone coupling was feasible and matched the LP optimum exactly every time, and its certificate gap (Section 3) was exactly 0 every time.

### 2.3 Why this matters for encryption

Every operation is FHE-friendly:
- cumulative sums are a **public** lower-triangular matrix applied to the encrypted vector (linear);
- `min`, `max` and `max(0, ·)` can be written with an absolute value: min(x, y) = (x + y − |x − y|)/2, max(x, y) = (x + y + |x − y|)/2, max(0, z) = (z + |z|)/2;
- the mask is a public 0/1 plaintext multiplication.

There are no iterations, no division and no data-dependent control flow. The only non-linear operation is |x|, computed by a polynomial approximation of sign(x) (`docs/07`).

### 2.4 Relationship to Sinkhorn

Entropic OT (Sinkhorn) at regularisation ε converges to this plan as ε → 0. For one-dimensional costs the closed form is both exact and cheaper, so v1 uses it. Sinkhorn is needed for multi-dimensional costs, where no closed form exists: multi-asset baskets, delivery dates and venues, or dataset distances on 2-D grids. That is the v2 track (`docs/07` §6), where your thesis on separable (Kronecker) kernels applies directly.

## 3. The optimality certificate

### 3.1 Dual problem

The dual of the LP in §1.4:

```
minimise    Σ_i U_i · a_i + Σ_j S_j · b_j
subject to  U_i + S_j ≥ w_ij     for all i, j
            U_i ≥ 0, S_j ≥ 0
```

U_i is the buyer's surplus per unit at level i; S_j is the seller's surplus per unit at level j. These are the OT dual potentials, read as economic quantities.

**Weak duality.** For any primal-feasible T and dual-feasible (U, S):

```
W(T) ≤ W* ≤ Σ U_i a_i + Σ S_j b_j
```

Define the **gap**

```
G(T, U, S) = Σ_i U_i a_i + Σ_j S_j b_j − W(T) ≥ 0
```

If G ≤ δ then W(T) ≥ W* − δ: the trades are provably within δ of the best possible welfare. If G = 0, they are exactly optimal.

### 3.2 A single clearing price is enough

Pick one integer price Π, in quarter-ticks, and set

```
U_i(Π) = max(0, B_i − Π)        S_j(Π) = max(0, Π − A_j)
```

**Dual feasibility holds for every Π:** U_i + S_j ≥ (B_i − Π) + (Π − A_j) = w_ij, and both are ≥ 0. So nobody needs to trust how Π was chosen; feasibility is automatic and publicly checkable.

### 3.3 Decomposing the gap

Let r_i = a_i − Σ_j T_ij (Firm A's unfilled volume at level i) and r'_j = b_j − Σ_i T_ij (Firm B's). Substituting a_i = Σ_j T_ij + r_i and b_j = Σ_i T_ij + r'_j:

```
G(Π) = Σ_{i,j} T_ij · (U_i + S_j − w_ij)      ← G_T(Π): depends only on the public trades
     + Σ_i U_i · r_i                           ← G_A(Π): known only to Firm A
     + Σ_j S_j · r'_j                          ← G_B(Π): known only to Firm B
```

All three terms are ≥ 0. G(Π) = 0 exactly when:
- (a) every traded pair straddles the price: A_j ≤ Π ≤ B_i;
- (b) every bid level with unfilled volume is priced at or below Π (B_i ≤ Π);
- (c) every ask level with unfilled volume is priced at or above Π (A_j ≥ Π).

These are the **competitive-equilibrium conditions**. The certificate says: "at this clearing price, nobody left an order unfilled that they would have wanted filled, and every trade made both sides better off."

### 3.4 Completeness: an honest result always has a certifying price

A single-good market with divisible volume always has a Walrasian equilibrium price, and LP duality ties it to the optimal trades. G(Π) is piecewise linear in Π with breakpoints only at {B_i} ∪ {A_j}, so its minimum over all real Π is attained at one of these **2L public candidates**.

**Empirical check** (`spikes/certificate_uniform_price.py`): on 2,002 books, the optimal trades certified with G exactly 0 (integer units) at a candidate price every time. Removing 5 lots from the largest trade made G at least 10 quarter-tick·lots at every candidate price on every book.

### 3.5 Soundness: what an accepted certificate guarantees

If the verifier confirms:
1. T_ij ≥ 0 and T_ij = 0 wherever i > j (public, checked in the clear);
2. r_i ≥ 0 and r'_j ≥ 0 (range proofs on commitments, `docs/08`);
3. G(Π) ≤ δ = δ_T + δ_A + δ_B. This is realised as three bounded parts: G_T ≤ δ_T checked publicly, and G_A ≤ δ_A and G_B ≤ δ_B each proved in zero knowledge by the firm that owns it, using homomorphic combinations of its commitments (`docs/08` §4). No joint opening is needed;

then T is feasible for the committed orders, and W(T) ≥ W* − δ. This holds whatever the server did: how many operations it ran, its CKKS precision, or whether it cheated. The proof is just weak duality (§3.1) plus feasibility of (U(Π), S(Π)) (§3.2).

### 3.6 Integer bounds (for choosing scalar encodings)

- w_ij ∈ [−30, 30] quarter-ticks for L = 8.
- Volumes ≤ 8 × 50,000 = 400,000 micro-lots per side.
- |G| ≤ 2 × 30 × 400,000 = 2.4·10⁷ < 2²⁵. All values fit easily in the curve25519 scalar field (≈ 2²⁵²), so no wrap-around issues as long as negative coefficients are encoded as field negatives.

## 4. From decrypted trades to certified integer trades

The FHE engine returns T̂, an approximation of T in micro-lots, with error from CKKS noise and polynomial approximation. A **public deterministic rounding rule**, fixed in the session parameters before anyone commits:

```
T_int[i][j] = M_ij · max(0, floor(T̂[i][j] − κ))        κ = 2 micro-lots
```

**Rounding lemma** (proof: `certified-blind-transport/docs/M1-market-math.md` §6). If every entry of the decrypted T̂ is within κ of the exact optimum T* (|T̂ − T*| < κ entrywise), then 0 ≤ T* − T_int ≤ 2κ entrywise and T_int is zero wherever T* is. So T_int is always feasible (no over-fill is possible), and at the equilibrium price G_T = 0 exactly.

Each firm still checks its own residuals privately. A negative residual means the approximation error exceeded κ: the firm aborts with "precision failure". That is a completeness failure (the run is retried with better precision), never a soundness failure.

## 5. Tolerance δ

Honest runs have G > 0 only because of the κ margin and approximation error. Earlier drafts used a relative tolerance (0.5% of welfare, from the spike in `docs/04`). It is replaced by **rigorous constants** that are proved sufficient for every honest run whose decryption error is below κ (Theorem 7 in `M1-market-math.md`):

```
δ_T = 0
δ_A = δ_B = w_max · (2L − 1) · 2κ        = 30 · 15 · 4 = 1800   for L = 8, κ = 2
```

Why: the optimal plan has at most 2L − 1 non-zero cells (a staircase), rounding moves each by at most 2κ micro-lots, each unfilled micro-lot costs a firm at most w_max in its gap part, and rounding never creates a trade outside the optimal support, so the public part stays exactly 0.

**What δ means in money and volume.** δ = 3600 quarter-tick·micro-lots = $0.09 per share-per-lot ($9.00 with 100-share lots). A cheating server could therefore withhold at most δ / w_min = 1800 micro-lots (1.8 lots) at the marginal cell (w = 2) without detection, and less at better-priced cells. This is the price of tolerating CKKS noise. It shrinks linearly with κ (better precision) or with finer volume units. State it on the page.

## 6. Choosing Π without revealing more than necessary

Who picks Π? It must make G small, and G depends on private residuals.

**v1 protocol (simple, small leak):**
1. The verifier computes G_T(Π_c) for each of the 2L candidates Π_c (public).
2. Firm A computes G_A(Π_c) privately and publishes a bitmask m_A[c] = 1[G_A(Π_c) ≤ δ_A]; Firm B likewise publishes m_B.
3. The verifier picks the smallest Π_c with G_T(Π_c) ≤ δ_T and m_A[c] = m_B[c] = 1. If there is none, the certificate fails.
4. The real check (§3.5, step 3) then runs on commitments. A firm that lies in its bitmask can only cause a rejection; it cannot make a bad result pass.

**Leakage of the bitmask:** roughly, the price boundary of each firm's in-the-money unfilled volume. **v2 option:** a private set intersection over the 16 candidates, so only the chosen Π is revealed.

## 7. What each party learns (leakage statement for v1)

| Party | Learns | Does not learn |
|---|---|---|
| Firm A | Its own book; the trades T_int; Π; G_T; Firm B's bitmask | Firm B's unfilled orders, except what the trades, Π and the bitmask imply |
| Firm B | Symmetric | Symmetric |
| Server | Ciphertexts and their sizes; the public parameters; the number of refreshes | Any order size or trade (CKKS IND-CPA, plus smudging in decryption shares) |
| Verifier / public | Commitments, proofs, T_int, Π, G_T, both bitmasks, verdict | Order sizes, residuals and the private gap parts G_A, G_B (hidden by Pedersen; range proofs are zero knowledge) |

Note: revealing T_int to both firms is inherent to trading, since both parties to a trade see it. A further "private settlement" variant, where each firm sees only its own fills, is possible but not in v1.

## 8. Optional: entropic "anti-steering" certificate (stretch)

When several optimal plans tie, the LP certificate cannot tell them apart, so a server could steer tied volume toward one firm. The monotone coupling has no such freedom (it is a deterministic function of the inputs). So in v1 the check is simply that the server's output equals the closed form within tolerance, which the gap plus feasibility already enforce up to ties of equal welfare. If v2 uses Sinkhorn, the research report's entropic certificate ε·KL(P‖P*_ε) ≤ F_ε(P) − D_ε(f, g) applies. It is not linear, so it would run in auditor mode.

## 9. `bt-market` API sketch

```rust
pub struct Grid { pub levels: usize, pub top_ticks: i64 }          // t_k = top_ticks - k
impl Grid {
    pub fn b(&self, i: usize) -> i64;                                // 4 t_i + 1
    pub fn a(&self, j: usize) -> i64;                                // 4 t_j - 1
    pub fn w(&self, i: usize, j: usize) -> i64;
    pub fn candidates(&self) -> Vec<i64>;                            // {B_i} ∪ {A_j}, sorted, deduplicated
}
pub type Book = Vec<u64>;                                            // micro-lots per level
pub type Trades = Vec<Vec<u64>>;                                     // T[i][j], micro-lots

pub fn monotone_coupling(g: &Grid, a: &Book, b: &Book) -> Trades;    // §2.1, exact integers
pub fn welfare(g: &Grid, t: &Trades) -> i128;
pub fn residuals(a: &Book, b: &Book, t: &Trades) -> (Vec<i128>, Vec<i128>);
pub fn gap_parts(g: &Grid, a: &Book, b: &Book, t: &Trades, pi: i64) -> GapParts; // G_T, G_A, G_B
pub fn round_trades(g: &Grid, t_hat: &[Vec<f64>], kappa: u64) -> Trades;          // §4
pub fn thresholds(g: &Grid, kappa: u64) -> Thresholds;                            // §5: δ_T, δ_A, δ_B
pub fn choose_pi(g: &Grid, t: &Trades, mask_a: &[bool], mask_b: &[bool], thr: &Thresholds) -> Option<i64>; // §6
// implemented: certified-blind-transport/crates/bt-market (also equilibrium_price, certify_plain, quantile_welfare_bound)
// v2 / explanations:
pub fn sinkhorn_log(g: &Grid, a: &Book, b: &Book, eps_schedule: &[f64], rounds: usize) -> Vec<Vec<f64>>;
```

## 10. Tests for M1 (`proptest`)

1. `monotone_coupling` is feasible: row sums ≤ a, column sums ≤ b, zero on i > j.
2. Optimality: for random books, there exists a candidate Π with `gap == 0` (exact integers). This is itself a proof of optimality, so no LP solver is needed in Rust. Cross-check against `spikes/` once, by exporting vectors.
3. Tampering: removing or moving any positive amount of volume from an optimal T makes `min_Π gap > 0`.
4. Rounding: for T̂ = T + noise with |noise| < κ, the residuals after `round_trades` are ≥ 0, G_T = 0 at the equilibrium price, G_A ≤ δ_A and G_B ≤ δ_B.
5. Dual feasibility: for every Π and every compatible pair, U_i + S_j ≥ w_ij.
