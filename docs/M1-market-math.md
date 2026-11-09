# M1: Market model, closed-form transport and the clearing-price certificate

**Owner:** Lance Tersou · **Code:** `crates/bt-market` · **Status:** draft v1, all statements below are checked by the tests named in §11.

This document is the mathematical specification of the market layer. Every definition corresponds to a function in `bt-market`, and every theorem has a proof here and a property test there. The aim is that you can defend each line on a whiteboard without the code.

Contents
0. Notation and units
1. The model
2. The closed-form optimum (monotone coupling)
3. Linear-programming duality
4. The uniform-price certificate
5. Completeness: a zero-gap price always exists among 2L candidates
6. Soundness, rounding and the tolerances δ
7. Choosing Π: the bitmask protocol and what it leaks
8. Integer ranges and the commitment encoding
9. Worked example (the demo preset)
10. Algorithms, complexity and code map
11. Test map
12. Scope, limitations and what to be ready to defend

---

## 0. Notation and units

| Symbol | Meaning | Unit / type |
|---|---|---|
| L | number of price levels (demo: 8) | — |
| k ∈ {0,…,L−1} | level index, 0 = highest price | — |
| t_k = t_top − k | tick index of level k (demo: t_top = 1004, so $100.40 … $99.70) | ticks of $0.10 |
| a_i | Firm A's bid volume at level i | micro-lots (`u64`) |
| b_j | Firm B's ask volume at level j | micro-lots (`u64`) |
| B_i, A_j | bid value and ask cost | quarter-ticks (`i64`) |
| w_ij = B_i − A_j | gain from trading one micro-lot between bid level i and ask level j | quarter-ticks |
| T = (T_ij) | trade plan: volume between bid level i and ask level j | micro-lots |
| W(T) | total gain from trade | quarter-tick·micro-lots (`i128`) |
| Π | uniform clearing price | quarter-ticks |
| x⁺ = max(0, x), x⁻ = max(0, −x) | positive and negative parts, x = x⁺ − x⁻ | — |

One quarter-tick is $0.025, one micro-lot is 10⁻³ lot, so one unit of W is $2.5·10⁻⁵ per share-per-lot ($0.0025 with 100-share lots). Each side's volume is at most LIMIT = 50,000 micro-lots per level.

Firm A only buys, Firm B only sells. (Two-sided firms are the same mathematics with four books; §12.)

---

## 1. The model

### 1.1 Grid
The grid is public: L levels, tick index t_k = t_top − k. `Grid::DEMO = {levels: 8, top_ticks: 1004}`.

### 1.2 Books
A book is a vector of L non-negative integers. Firm A's book a = (a_0,…,a_{L−1}) holds bids, Firm B's book b holds asks. Books are private: each firm commits to its book before anything is encrypted (`docs/08` phase 1).

### 1.3 Quarter-tick valuations
A bid at tick t is modelled as willing to pay t + ¼ tick, and an ask at tick t as willing to accept t − ¼ tick:

```
B_i = 4·t_i + 1,      A_j = 4·t_j − 1,      w_ij = B_i − A_j = 4·(t_i − t_j) + 2 = 4·(j − i) + 2.
```

Why this convention:
1. **Same-price orders cross.** A bid and an ask at the same tick have w = 2 > 0, matching exchange practice.
2. **No ties.** B_i ≡ 1 and A_j ≡ 3 (mod 4), so B_i ≠ A_j always and w_ij ≠ 0 for every pair: w_ij ≥ 2 if i ≤ j and w_ij ≤ −2 if i > j. Every pair is strictly profitable or strictly unprofitable. This removes all degenerate cases from the proofs below and from the encrypted comparisons in M5. Rule: compatible ⇔ i ≤ j ⇔ w_ij > 0.
3. **Integers throughout.** Every quantity in this document is an exact integer.

The largest gain is w_max = 4(L−1) + 2 (= 30 for L = 8), at (i, j) = (0, L−1); the smallest positive gain is w_min = 2, on the diagonal.

### 1.4 The primal problem

```
(P)   maximise   W(T) = Σ_ij w_ij T_ij
      subject to Σ_j T_ij ≤ a_i   (row i),     Σ_i T_ij ≤ b_j   (column j),     T_ij ≥ 0.
```

Write R_i = Σ_j T_ij and C_j = Σ_i T_ij for the row and column sums, and the **residuals** r_i = a_i − R_i (unfilled bid volume) and r'_j = b_j − C_j (unfilled ask volume). T is *feasible* iff T ≥ 0, r ≥ 0 and r' ≥ 0. Write W* for the optimal value of (P).

(P) is the Kantorovich optimal-transport problem with *partial* marginals (inequality constraints) and cost −w. Incompatible pairs need no extra constraint: w_ij < 0 there, so no optimal plan uses them. The certificate still checks T_ij = 0 for i > j explicitly (check 1 in `docs/08` §4.4); this gives a clear error message instead of a large gap.

---

## 2. The closed-form optimum (monotone coupling)

### 2.1 Quantile functions
Lay out demand as a sequence of micro-lot units, best bid first: units 0,…,a_0−1 are at level 0, the next a_1 units at level 1, and so on. Lay out supply best ask first, i.e. starting at level L−1. Define, for unit index u,

```
β(u) = B_i   where unit u of demand lies at bid level i     (non-increasing in u)
α(u) = A_j   where unit u of supply lies at ask level j     (non-decreasing in u)
```

β is defined on [0, ΣA), α on [0, ΣB). Let M = min(ΣA, ΣB). The cumulative sums D_i = a_0 + … + a_i and S'_j = b_{L−1} + … + b_j (indices of the supply run downwards) mark the level boundaries: demand level i occupies [D_{i−1}, D_i), supply level j occupies [S'_{j+1}, S'_j), with D_{−1} = S'_L = 0.

### 2.2 Definition (masked monotone coupling)
Match demand unit u with supply unit u for every u < M, and keep the match iff it is compatible. In cell form:

```
T*_ij = 1[i ≤ j] · ( min(D_i, S'_j) − max(D_{i−1}, S'_{j+1}) )⁺ .
```

This is the length of the overlap of demand level i's interval with supply level j's interval, masked. `monotone_coupling` computes exactly this.

**Lemma 2 (feasibility).** T* is feasible.
*Proof.* For fixed i, the intervals [S'_{j+1}, S'_j) are disjoint, so the overlaps of [D_{i−1}, D_i) with them sum to at most its length a_i. Masking only removes volume. The column case is symmetric. ∎

**Lemma 6 (staircase).** T* has at most 2L − 1 non-zero cells, and its support is a staircase: as u increases, the bid level i(u) never decreases and the ask level j(u) never increases.
*Proof.* Non-zero cells correspond to non-empty pieces of the common refinement of two partitions of [0, M): the demand partition has at most L − 1 interior cut points and the supply partition at most L − 1. A partition of an interval by at most 2L − 2 cut points has at most 2L − 1 pieces. Masking removes cells. Monotonicity follows because both sequences of levels are traversed in order. ∎

### 2.3 Optimality

**Theorem 1 (quantile formula).** For every feasible T,

```
W(T)  ≤  Σ_{u < M} ( β(u) − α(u) )⁺  =  W(T*).
```

So T* is optimal for (P), including against fractional plans.

*Proof.*
(i) *Upper bound.* Let Q = Σ_ij T_ij be the traded volume; Q ≤ M by feasibility. Write W(T) = Σ_i B_i R_i − Σ_j A_j C_j with Σ R_i = Σ C_j = Q.

The multiset of bid values (B_i with multiplicity a_i) has its Q largest elements equal to β(0), …, β(Q−1). Choosing R_i ≤ a_i units at each level with total Q gives Σ_i B_i R_i ≤ Σ_{u<Q} β(u). This is the rearrangement inequality: moving one unit from a lower to a higher value never decreases the sum. Symmetrically, Σ_j A_j C_j ≥ Σ_{u<Q} α(u). (For fractional T replace sums over units by integrals of the step functions β, α over [0, Q); the argument is the same.) Hence

```
W(T) ≤ F(Q) := Σ_{u<Q} ( β(u) − α(u) ).
```

The increments β(u) − α(u) are non-increasing in u, because β is non-increasing and α is non-decreasing. So F grows while the increment is positive and then falls. Its maximum over Q ∈ [0, M] is Σ_{u<M} (β(u) − α(u))⁺.

(ii) *Attainment.* Unit u < M of the coupling lies in cell (i(u), j(u)), with increment β(u) − α(u) = w_{i(u) j(u)}. The mask keeps the unit iff w > 0, i.e. iff β(u) − α(u) > 0. So W(T*) = Σ_{u<M} (β(u) − α(u))⁺. ∎

**Remark (Q\*).** Since w is never 0, the kept units are exactly u < Q* with Q* = #{u < M : β(u) > α(u)}, and Q* = Σ T*_ij. The monotone coupling is the classical fact that in one dimension the optimal transport plan for a supermodular gain is the quantile (north-west-corner) coupling. Here w_ij = 4(j − i) + 2 is supermodular in (i, −j). The masking handles the partial marginals.

`quantile_welfare_bound` computes the right-hand side without forming any coupling, by walking the two run-length sequences. The test `coupling_attains_the_quantile_bound` checks equality on 20,000 random books.

Theorem 5 below gives a second, independent proof of optimality: it produces a dual solution with zero gap. That second proof is the one the protocol actually uses, because it can be checked without seeing the books.

---

## 3. Linear-programming duality

The dual of (P) is

```
(D)   minimise   Σ_i a_i U_i + Σ_j b_j S_j
      subject to U_i + S_j ≥ w_ij   for all i, j,       U ≥ 0,  S ≥ 0.
```

Economically, U_i is the surplus per micro-lot of a buyer at level i and S_j the surplus of a seller at level j. A pair (U, S) satisfying the constraints is *dual-feasible*.

**Theorem 2 (weak duality with an exact gap identity).** For *any* trade matrix T (feasible or not) and any vectors U, S, define the gap

```
G := Σ_i a_i U_i + Σ_j b_j S_j − W(T).
```

Then

```
G = Σ_ij T_ij (U_i + S_j − w_ij)  +  Σ_i U_i r_i  +  Σ_j S_j r'_j .                    (★)
```

If T is feasible and (U, S) is dual-feasible, every term of (★) is ≥ 0. Hence G ≥ 0, and W(T) ≥ W* − G.

*Proof.* Expand: Σ_ij T_ij (U_i + S_j) = Σ_i U_i R_i + Σ_j S_j C_j. Then Σ_i U_i a_i = Σ_i U_i R_i + Σ_i U_i r_i, and likewise for S. Substituting gives (★); it is a purely algebraic identity. Non-negativity holds term by term. For the last claim apply G ≥ 0 to an optimal plan T°: W* = W(T°) ≤ Σ a U + Σ b S = W(T) + G. ∎

The point of (★) is that it *splits the gap into parts owned by different parties* (§4.2).

---

## 4. The uniform-price certificate

### 4.1 Dual solutions from a single price
For any number Π define

```
U_i(Π) = (B_i − Π)⁺,       S_j(Π) = (Π − A_j)⁺ .
```

**Lemma 3 (every price is dual-feasible).** For every Π and all i, j: U_i(Π), S_j(Π) ≥ 0 and U_i(Π) + S_j(Π) ≥ w_ij.
*Proof.* x⁺ ≥ x, so U_i + S_j ≥ (B_i − Π) + (Π − A_j) = w_ij. ∎

So one public number Π yields a full dual solution. Nobody needs to send or prove the vectors U and S: everyone recomputes them from Π.

### 4.2 Decomposition of the gap

**Theorem 4 (three owned parts).** With U = U(Π), S = S(Π), identity (★) reads G(Π) = G_T + G_A + G_B with

```
G_T(Π) = Σ_ij T_ij · ( U_i + S_j − w_ij )      depends only on T and Π: public
G_A(Π) = Σ_i U_i · r_i                          depends on Firm A's book: A proves it
G_B(Π) = Σ_j S_j · r'_j                         depends on Firm B's book: B proves it
```

For feasible T, all three are ≥ 0 (Lemma 3 and Theorem 2).

A cell's contribution to G_T has a closed form. Put x = B_i − Π and y = Π − A_j, so x + y = w_ij. Then

```
U_i + S_j − w_ij = x⁺ + y⁺ − (x + y) = x⁻ + y⁻ ,
```

which is zero iff A_j ≤ Π ≤ B_i. In words, G_T charges each traded micro-lot by how far Π lies outside that pair's acceptable price range.

**Corollary 4.1 (zero gap ⇔ competitive equilibrium).** For feasible T, G(Π) = 0 iff all three of the following hold:
1. every traded pair accepts the price: T_ij > 0 ⇒ A_j ≤ Π ≤ B_i;
2. no unfilled buyer wants to buy at Π: r_i > 0 ⇒ B_i ≤ Π;
3. no unfilled seller wants to sell at Π: r'_j > 0 ⇒ A_j ≥ Π.

That is, (T, Π) is a Walrasian equilibrium, and then T is optimal (Theorem 2 with G = 0).
*Proof.* Each of the three non-negative parts must vanish term by term. Item 1 is the x⁻ + y⁻ = 0 condition above. Item 2: U_i r_i = 0 with r_i > 0 forces U_i = 0, i.e. B_i ≤ Π. Item 3 is symmetric. ∎

This is the sentence for the page: *the certificate proves that a single clearing price exists at which every executed trade is voluntary and no one left out would have wanted to trade.* Optimality then follows from LP duality.

### 4.3 Shape of G(Π)
Each U_i(Π) and S_j(Π) is convex and piecewise linear in Π, with a single kink at B_i or A_j. So G(Π) = Σ a_i U_i(Π) + Σ b_j S_j(Π) − W(T) is convex and piecewise linear, and its kinks lie in the **candidate set**

```
𝒞 = {B_0, …, B_{L−1}} ∪ {A_0, …, A_{L−1}}        (|𝒞| = 2L; demo: 16 values, 3987 … 4017).
```

The same holds for each part separately: G_T and the private parts are piecewise linear with kinks in 𝒞. G_A is non-increasing in Π and G_B is non-decreasing (for feasible T), because r, r' ≥ 0 and U_i is non-increasing, S_j non-decreasing. §7 uses this monotonicity.

---

## 5. Completeness: a zero-gap price always exists among the candidates

**Theorem 5 (constructive clearing price).** Let Q* = Σ T*_ij, with β, α as in §2.1. Drop any term below that refers to a unit that does not exist (no traded unit when Q* = 0; no unfilled demand unit when Q* = ΣA; no unfilled supply unit when Q* = ΣB). Define

```
Π_lo = max( α(Q*−1), β(Q*) ),       Π_hi = min( β(Q*−1), α(Q*) ).
```

Then Π_lo ≤ Π_hi, both lie in 𝒞, and every Π ∈ [Π_lo, Π_hi] gives G(Π) = 0 for T = T*. If both books are empty, every Π works.

*Proof.*
*Non-empty interval.* Compare each lower term with each upper term:
- α(Q*−1) < β(Q*−1), because unit Q*−1 is traded.
- α(Q*−1) ≤ α(Q*), because α is monotone.
- β(Q*) ≤ β(Q*−1), because β is monotone.
- β(Q*) < α(Q*), because unit Q* is not traded. Either it lies beyond M, in which case at least one of the two terms is dropped, or β(Q*) − α(Q*) < 0 by the Remark in §2.

*Membership in 𝒞.* Every term is some B_i or A_j.

*Zero gap.* Use Corollary 4.1 with Π ∈ [Π_lo, Π_hi].
1. A traded unit u < Q* sits in cell (i, j) with α(u) = A_j and β(u) = B_i. By monotonicity, A_j = α(u) ≤ α(Q*−1) ≤ Π ≤ β(Q*−1) ≤ β(u) = B_i.
2. If r_i > 0, bid level i contains an untraded demand unit u ≥ Q*, so B_i = β(u) ≤ β(Q*) ≤ Π.
   *Why u ≥ Q\*:* T* trades exactly the units u < Q* (the Remark in §2), so any unfilled unit has index ≥ Q*.
3. Symmetrically, if r'_j > 0 then A_j = α(u) ≥ α(Q*) ≥ Π for some u ≥ Q*. ∎

`equilibrium_price` returns Π_lo. The test `equilibrium_price_certifies_with_zero_gap` checks G(Π_lo) = 0 exactly on 20,000 random books, including one-sided, non-crossing and empty ones. It also checks that a brute-force minimum over 𝒞 is 0.

**Corollary 5.1 (second proof of optimality).** The pair (U(Π_lo), S(Π_lo)) is dual-feasible with zero gap, so T* is optimal for (P) by Theorem 2. Theorem 1 is not needed for this.

**Corollary 5.2 (candidates suffice).** For any T, min over all real Π of G(Π) equals the minimum over 𝒞. The reason: G is convex and piecewise linear with kinks in 𝒞; its slope is −ΣA < 0 to the left of 𝒞 and +ΣB > 0 to the right (or 0 if a book is empty, when G is constant there). So restricting the search to 2L prices loses nothing.

---

## 6. Soundness, rounding and the tolerances δ

### 6.1 Soundness
**Theorem 6 (what acceptance guarantees).** Suppose the verifier accepts (T, Π) after checking:
- (C1) T_ij ≥ 0 and T_ij = 0 for i > j;
- (C2) r ≥ 0 and r' ≥ 0;
- (C3) G_T(Π) ≤ δ_T, G_A(Π) ≤ δ_A, G_B(Π) ≤ δ_B.

Then T is feasible for the committed books, and W(T) ≥ W* − (δ_T + δ_A + δ_B).

*Proof.* (C1)–(C2) give feasibility. Lemma 3 gives dual feasibility of (U(Π), S(Π)). Theorem 2 then gives W(T) ≥ W* − G(Π), and G = G_T + G_A + G_B ≤ δ. ∎

This holds **whatever the server did**: however many homomorphic operations it ran, whatever its CKKS precision, whether or not it cheated. The certificate never trusts the encrypted computation; it only checks its output against the committed inputs. Test: `soundness_bound_holds`.

### 6.2 Why tolerances are needed at all
CKKS is approximate. The server outputs T̂ ≈ T*, decrypted with some error. The protocol needs integer trades, so a public rounding rule is fixed before anyone commits:

```
T_int[i][j] = 1[i ≤ j] · max(0, floor(T̂_ij − κ))         κ = 2 micro-lots (demo)
```

Rounding loses a few micro-lots per cell, which makes the honest gap slightly positive. δ must be large enough to accept every honest run, and small enough to bound what a cheater can get away with.

### 6.3 The rounding theorem
**Theorem 7 (honest runs certify).** Suppose |T̂_ij − T*_ij| < κ for all compatible (i, j), with κ a positive integer. Then:
1. 0 ≤ T*_ij − T_int,ij ≤ 2κ for every cell, and T_int,ij = 0 wherever T*_ij = 0. In particular T_int is feasible: no over-fill is possible.
2. At Π* = `equilibrium_price` (any Π in the interval of Theorem 5):
   - G_T(Π*) = 0 exactly;
   - G_A(Π*) ≤ w_max · (2L−1) · 2κ;
   - G_B(Π*) ≤ w_max · (2L−1) · 2κ.

Hence the thresholds

```
δ_T = 0,        δ_A = δ_B = w_max · (2L − 1) · 2κ        (= 30 · 15 · 4 = 1800 for the demo)
```

accept every honest run whose decryption error is below κ. With these values, the bitmask protocol of §7 always finds a price.

*Proof.*
1. Fix a cell and write x = T̂ − κ; then T* − 2κ < x < T*.
   - If T* = 0, then x < 0 and T_int = 0.
   - If T* ≥ 1: floor(x) ≤ x < T*, and floor(x) ≥ T* − 2κ because x > T* − 2κ is an integer bound. Hence T* − 2κ ≤ T_int ≤ T*. (Exact arithmetic would even give T_int ≤ T* − 1; allowing T_int = T* absorbs floating-point rounding in computing x.)
   Feasibility follows from T_int ≤ T* entrywise and Lemma 2.
2. Let d = T* − T_int ≥ 0. By item 1, supp(d) ⊆ supp(T*), d ≤ 2κ, and supp(T_int) ⊆ supp(T*).
   - *G_T.* By Theorem 5, G(T*, Π*) = 0, so every term of (★) for T* vanishes. In particular U_i + S_j − w_ij = 0 on supp(T*) ⊇ supp(T_int). So G_T(T_int, Π*) = 0.
   - *G_A.* The residuals of T_int are r_i = r*_i + Σ_j d_ij. Since U_i r*_i = 0 for every i (the G_A part for T* vanishes),
     ```
     G_A(T_int, Π*) = Σ_i U_i Σ_j d_ij = Σ_{(i,j) ∈ supp(T*)} U_i d_ij.
     ```
     On supp(T*), U_i + S_j = w_ij and S_j ≥ 0, so U_i ≤ w_ij ≤ w_max. With d ≤ 2κ and |supp(T*)| ≤ 2L − 1 (Lemma 6), G_A ≤ w_max (2L−1) 2κ.
   - *G_B.* Symmetric. ∎

The same argument gives the joint bound G_A + G_B = Σ_{supp T*} w_ij d_ij ≤ 2κ Σ_{supp T*} w_ij, which is usually much smaller. It is not used because supp(T*) is not public. Tests: `rounding_lemma_and_protocol_completeness` (20,000 books with uniform noise in (−κ, κ)) and the boundary check in `cheats_are_rejected`.

### 6.4 What δ means and what it costs
The total tolerance δ = δ_T + δ_A + δ_B = 3600 units.
- **In money:** $0.09 per share-per-lot, i.e. $9.00 for 100-share lots. In the demo preset that is about 1.2% of W*.
- **In volume:** by Theorem 6 a dishonest server can deviate from the optimum by at most δ in welfare. At the marginal cell (w = 2) that is up to 1800 micro-lots (1.8 lots) of withheld volume. In practice the per-firm threshold binds first: for a single withheld cell, the affected firm's part grows by U_i per micro-lot, so the boundary in the preset is 900 micro-lots (tested exactly: 900 certifies, 901 is rejected).
- **How to shrink it:** δ scales linearly with κ, the CKKS error bound in micro-lots. Better precision, or finer volume units at the same relative precision, shrinks it proportionally. The page should state the number rather than hide it.

The previous plan used a relative tolerance (0.5% of welfare, from the spike). That number was empirical. The constants above are proved, and only need the engine to meet the precision hypothesis |T̂ − T*| < κ. If the engine misses it, the run fails *completeness* (a firm sees a negative residual and aborts), never soundness.

---

## 7. Choosing Π: the bitmask protocol and what it leaks

Π must make all three parts small, but G_A and G_B depend on private residuals. v1 protocol:

1. Everybody computes G_T(Π_c) for each candidate Π_c ∈ 𝒞 (public).
2. Firm A publishes m_A[c] = 1[G_A(Π_c) ≤ δ_A]; Firm B publishes m_B[c] = 1[G_B(Π_c) ≤ δ_B].
3. The verifier takes the smallest Π_c with G_T(Π_c) ≤ δ_T and m_A[c] = m_B[c] = 1. If there is none, it rejects.
4. Each firm then proves its own inequality at that Π_c in zero knowledge (`docs/08` §4.3).

**Correctness.** *Completeness:* by Theorem 7, Π* ∈ 𝒞 passes all three tests, so step 3 succeeds. *Soundness:* step 4 enforces the inequalities whatever the masks say. A lying mask can only cause rejection (Theorem 6 holds for every Π).

**Leakage.** For feasible T, G_A(Π) is non-increasing in Π (§4.3). So an honest m_A is a *threshold vector* 0…01…1, and it reveals one number: the smallest candidate price at which A's in-the-money unfilled volume, weighted by distance, is at most δ_A. Roughly, this is A's best unfilled bid, up to the tolerance. That is at most log₂(2L+1) ≈ 4.1 bits. Symmetrically, m_B is 1…10…0 and reveals roughly B's best unfilled ask.

Is this new information? Not in essence. In a lit market, the best unfilled bid and ask after a cross are the top of the book, which is normally public. State it anyway. The v2 option of private set intersection over 𝒞 would reveal only the chosen Π.

`bitmask_a`, `bitmask_b`, `choose_pi`.

---

## 8. Integer ranges and the commitment encoding

For L = 8 and LIMIT = 50,000:

| Quantity | Range | Bits |
|---|---|---|
| a_i, b_j, r_i, r'_j, T_ij | [0, 50,000] (residuals ≥ 0 only if feasible) | 16 |
| Σ volumes | ≤ 400,000 | 19 |
| B_i, A_j, Π | ≈ 3987 … 4017 | 12 |
| U_i, S_j at Π ∈ 𝒞 | [0, 30] | 5 |
| w_ij | [−30, 30] | 6 (signed) |
| W(T), G and its parts | ≤ 30 · 400,000 · 2 = 2.4·10⁷ | 25 |
| δ_A − G_A, δ_B − G_B (when accepted) | [0, 1800] | 11 |

The code uses `u64` volumes, `i64` prices and `i128` sums: overflow is impossible by a wide margin.

**In the commitments (M2).** Pedersen commitments over Ristretto live in Z_ℓ with ℓ ≈ 2²⁵². Firm A commits to each a_i. G_A is a linear function of the a_i with public coefficients:

```
G_A(Π) = Σ_i U_i a_i − Σ_i U_i R_i
⇒  Com(δ_A − G_A) = δ_A·B − Σ_i U_i·Com(a_i) + (Σ_i U_i R_i)·B,      blinding −Σ U_i ρ_i
```

so A proves δ_A − G_A ∈ [0, 2³²) with one range proof, and nothing else is revealed.

**No wrap-around.** Every true integer value here has |x| < 2²⁶. A range proof that x mod ℓ ∈ [0, 2³²) therefore implies the integer statement 0 ≤ x < 2³². A negative x would appear as ℓ − |x| > 2³². The same reasoning covers r_i ∈ [0, 2³²). Commit-time range proofs a_i ∈ [0, 2³²) are also needed, so that a firm cannot commit to a "negative" book.

---

## 9. Worked example (the demo preset)

`cargo run -p bt-market --example preset` prints all of this.

| Level | Price | Bids a (lots) | Asks b (lots) | B_i | A_i |
|---|---|---|---|---|---|
| 0 | $100.40 | 0 | 10 | 4017 | 4015 |
| 1 | $100.30 | 5 | 15 | 4013 | 4011 |
| 2 | $100.20 | 10 | 25 | 4009 | 4007 |
| 3 | $100.10 | 25 | 30 | 4005 | 4003 |
| 4 | $100.00 | 30 | 20 | 4001 | 3999 |
| 5 | $99.90 | 20 | 8 | 3997 | 3995 |
| 6 | $99.80 | 15 | 0 | 3993 | 3991 |
| 7 | $99.70 | 10 | 0 | 3989 | 3987 |

**Monotone coupling.** Demand is laid out best first: 5 at level 1, then 10 at level 2, then 25 at level 3, … Supply is laid out best first: 8 at level 5, then 20 at level 4, then 30 at level 3, … The overlaps give:

| Cell (i, j) | Lots | w_ij |
|---|---|---|
| (1, 5) | 5 | 18 |
| (2, 5) | 3 | 14 |
| (2, 4) | 7 | 10 |
| (3, 4) | 13 | 6 |
| (3, 3) | 12 | 2 |

The next demand unit is a bid at level 4 ($100.00), facing an ask at level 3 ($100.10): w = −2, so it stops there.
- Q* = 40 lots, W* = 304,000, equal to the quantile bound.
- The staircase has 5 cells (≤ 2L − 1 = 15).

**Clearing price.**
- The traded side gives α(Q*−1) = A_3 = 4003 and β(Q*−1) = B_3 = 4005.
- The unfilled side gives β(Q*) = B_4 = 4001 and α(Q*) = A_3 = 4003.
- So Π_lo = max(4003, 4001) = 4003 and Π_hi = min(4005, 4003) = 4003. The interval is a single point: Π* = 4003 quarter-ticks = **$100.075**.
- Reading: the marginal trade is a bid and an ask both at $100.10, and the ask level is only partly filled, so the price must sit exactly at the ask's cost.

**Gap profile over the 16 candidates** (it is convex):

| Π | 3999 | 4001 | **4003** | 4005 | 4007 |
|---|---|---|---|---|---|
| G_T | 48,000 | 24,000 | **0** | 0 | 50,000 |
| G_A | 60,000 | 0 | **0** | 0 | 0 |
| G_B | 0 | 0 | **0** | 36,000 | 72,000 |

- At 4005, G_B = 36,000 = S_3 · r'_3 = 2 · 18,000: B's 18 unfilled lots at $100.10 would want to sell at $100.125.
- At 4001, G_T = 24,000 = 2 · 12,000: the (3, 3) trades would be at a price below the seller's cost.

**Masks with δ_A = δ_B = 1800:**
Candidates are listed in ascending order, 3987 … 4017:
- m_A = 0000000111111111 (cutoff at 4001);
- m_B = 1111111110000000 (cutoff at 4003);
- G_T ≤ 0 holds at 0000000011000000, i.e. {4003, 4005}.
- The intersection is {4003}, so the protocol picks Π = 4003.

---

## 10. Algorithms, complexity and code map

| Function | Computes | Cost |
|---|---|---|
| `Grid::{b,a,w,candidates,w_max}` | §1.3, 𝒞 | O(1), O(L log L) |
| `monotone_coupling` | T* (§2.2) | O(L²); O(L) by merging two run lists, not needed at L = 8 |
| `quantile_welfare_bound` | right side of Theorem 1 | O(L) |
| `welfare`, `residuals`, `row_sums`, `col_sums` | W, r, r' | O(L²) |
| `surplus`, `dual_feasible` | U(Π), S(Π), Lemma 3 | O(L), O(L²) |
| `gap`, `gap_parts`, `gap_public`, `gap_firm_a`, `gap_firm_b` | Theorem 2, Theorem 4 | O(L²) each |
| `equilibrium_price` | Π_lo of Theorem 5 | O(L²) (dominated by the coupling) |
| `round_trades` | §6.2 | O(L²) |
| `thresholds` | δ of Theorem 7 | O(1) |
| `bitmask_a`, `bitmask_b`, `choose_pi` | §7 | O(L³) total (2L candidates × O(L²)) |
| `certify_plain` | checks C1–C3 in the clear (test oracle for M2) | O(L²) |

**In the encrypted engine (M5)** the server evaluates the same closed form on ciphertexts. Each firm encrypts its own cumulative sums D, S′ (no rotations needed). The server computes the masked overlap (min(D_i, S′_j) − max(D_{i−1}, S′_{j+1}))⁺ with polynomial approximations of |x| (min(x, y) = (x + y − |x − y|)/2, and x⁺ = (x + |x|)/2). The mask is public, so it costs nothing. This document is what the encrypted circuit must approximate within κ; `bt-market` is its plaintext oracle.

---

## 11. Test map

| Test (`crates/bt-market/src/tests.rs`) | Checks |
|---|---|
| `presets_match_the_demo` | demo numbers (§9) |
| `coupling_is_feasible_masked_and_a_staircase` | Lemma 2, Lemma 6 |
| `coupling_attains_the_quantile_bound` | Theorem 1 |
| `gap_identity_and_decomposition` | Theorem 2 (★) = Theorem 4, parts ≥ 0 |
| `every_candidate_price_is_dual_feasible` | Lemma 3 |
| `equilibrium_price_certifies_with_zero_gap` | Theorem 5, Corollary 5.1–5.2 |
| `any_suboptimal_plan_has_positive_gap_at_every_price` | Theorem 2 lower bound: min_Π G ≥ W* − W(T) |
| `rounding_lemma_and_protocol_completeness` | Theorem 7, §7 completeness |
| `soundness_bound_holds` | Theorem 6 |
| `cheats_are_rejected` | over-fill, incompatible trade, withholding; the exact 900/901 tolerance boundary (§6.4) |

Random books come from five generators: overlapping with micro-lot granularity, sparse whole lots, one-sided, non-crossing, and arbitrary with zeros. All use the deterministic `SplitMix64` from `bt-arith`, so failures are reproducible. Run with `cargo test -p bt-market --release` (about 0.2 s).

---

## 12. Scope, limitations and what to be ready to defend

**Likely questions, with answers you should be able to give from memory:**
- *"Why not Sinkhorn?"* Entropic OT is what M1 replaced. On this cost structure, scaling-form Sinkhorn either loses 27–40% of welfare (ε ≥ 0.1) or blows up numerically (values ≈ 10²¹ at ε = 0.02); see `docs/04`. In 1-D the exact optimum has a closed form (Theorem 1), so there is nothing to approximate except |x|. Your Sinkhorn expertise remains the v2 track (multi-asset, or a dataset-distance certificate).
- *"Is it optimal or approximately optimal?"* Exactly optimal in the plaintext model (Theorem 1 / Corollary 5.1). The certified output is within δ of optimal, where δ is a proved function of the CKKS error bound κ (Theorem 7) and the loss is quantified in §6.4.
- *"Why should I trust the server?"* You don't need to: Theorem 6 is unconditional.
- *"What leaks?"* The executed trades T and Π (that is the outcome), each firm's ≈ 4-bit mask (§7), and nothing else from the certificate. The FHE layer's leakage is analysed in `docs/06`.
- *"What's new here?"* Each ingredient is classical: 1-D OT quantile coupling, LP duality, Walrasian equilibrium, Pedersen and Bulletproofs. What we claim is the combination: an encrypted matching whose output is certified optimal by a uniform-price dual, split so that each firm proves its own share in zero knowledge, with proved tolerance constants for approximate FHE. Do not say "first" until the literature sweep in `docs/11` is done.

**Model limits (state them on the page):**
- One asset, one buyer firm and one seller firm.
- Two-sided firms: each firm has a bid book and an ask book. The same LP has four marginals and the same certificate applies, but the coupling now runs over the pooled books; that is a v1.1 change in `bt-market` only.
- Price-time priority within a level is out of scope: a level is one pooled order.
- Fees and minimum lot sizes are not modelled.
