# 04. Spike findings: the experiments behind the design

These experiments were run on 27 October 2026 to test the research report's design before committing to it. All scripts are in `spikes/` (Python 3 with numpy and scipy). Run them with `python3 spikes/<name>.py`. Order books come from the demo's two presets plus random books generated with fixed seeds.

## Spike 1. Does one clearing price certify optimal trades?

Script: `spikes/certificate_uniform_price.py`

**Question.** The research report built its certificate from Sinkhorn's dual potentials, one per price level. Decrypting those reveals the whole plan, including unfilled orders. Can a single public clearing price Π serve as the certificate instead?

**Method.** Solve the matching LP exactly (scipy HiGHS) for 2,002 books. For each, compute the gap G(Π) at the 2L = 16 candidate prices {B_i} ∪ {A_j} (integer quarter-tick units) and take the minimum. Then remove 5 lots from the largest trade and repeat.

**Result.**

| Test | Outcome |
|---|---|
| Optimal trades certified with G = 0 at some candidate Π | **2,002 / 2,002** (worst min-gap exactly 0 in integer units) |
| Tampered trades (5 lots withheld): smallest min-gap across all books | **10** quarter-tick·lots (= 5 lots × the minimum spread of 2 quarter-ticks), positive on every book |
| For comparison: a 400-point price grid instead of the exact breakpoints | certifies only **16 / 200** |

The grid result is expected: G is piecewise linear in Π, so its minimum sits exactly on a breakpoint, which a grid misses. **Always search the exact candidates.**

**Decision.** Use the single-price certificate (`docs/03` §3). It leaks one price instead of 16 potentials, and every check is linear in the committed orders.

## Spike 2. Can scaling-form Sinkhorn run under CKKS at certificate-grade accuracy?

Script: `spikes/sinkhorn_scaling_range.py`

**Question.** CKKS values must stay within about ±2²⁰ (with q₀ ≈ 2⁶⁰ and Δ ≈ 2⁴⁰). How large do Sinkhorn's scaling vectors u and v get, and how good are the resulting trades?

**Method.** 301 books, 60 Sinkhorn rounds, kernel normalised so max K = 1. Measure max |u|, |v| and the certificate gap of the (slightly shaved) Sinkhorn plan as a share of the optimal welfare.

**Result.**

| ε | Median gap / welfare | Worst | max \|u\|, \|v\| | min K |
|---|---|---|---|---|
| 0.2 | 40.2% | 47.3% | 2.7·10³ | 1.9·10⁻¹¹ |
| 0.1 | 27.5% | 35.4% | 3.0·10⁵ | 3.6·10⁻²² |
| 0.05 | 12.5% | 21.5% | 3.5·10⁹ | 1.3·10⁻⁴³ |
| 0.03 | 5.1% | 12.5% | 9.3·10¹⁴ | 3.3·10⁻⁷² |
| 0.02 | 2.0% | 7.0% | 5.7·10²¹ | 6.1·10⁻¹⁰⁸ |

**Reading.** In this formulation, values stay CKKS-friendly only at ε ≥ about 0.1, where trades lose more than a quarter of the achievable welfare. Reaching about 2% loss needs ε ≈ 0.02, where values reach 10²¹ (about 2⁷²). That is the known dynamic-range problem of Sinkhorn at small ε, and it is why plaintext Sinkhorn is usually run in the log domain. The log domain does not help under CKKS, because computing it needs encrypted exp and log over the same huge range.

## Spike 3. Bounded reformulation: Sinkhorn on the plan itself

Scripts: inline experiments recorded here (not packaged; reproduce from the description if needed).

**Idea.** Run Sinkhorn as iterative proportional fitting directly on the plan P, whose entries stay in [0, 1], with ε lowered in public steps by multiplying P with a public reweighting matrix. Reciprocals of row and column sums use Newton's method.

**Findings, in order:**

1. **Warm-starting Newton from the previous reciprocal diverged** on every book. Row sums change by up to 42× between rounds, and Newton's method for 1/d diverges once the initial guess times d exceeds 2.
2. **Firm-supplied reciprocals of their own marginals** (each firm encrypts 1/a_i alongside a_i, which costs nothing and leaks nothing) plus a deliberately small initial guess (θ/a_i with θ = 1/16) removed the divergence risk in principle. Measured row-sum to target ratios still reach 4.1 for rows and 13.4 for columns.
3. **At CKKS-like noise of 2⁻²⁴, small row sums go negative** under noise and Newton then blows up: 113–290 of 301 books diverged.
4. **At noise of 2⁻³⁵ (scale Δ ≈ 2⁵⁰) nothing diverged and overfill was tiny** (median ratio 1.6·10⁻⁷). But the trades collapsed: median gap was 73% of welfare. Rows crushed by the public reweighting never recover within a fixed Newton budget.

**Reading.** Encrypted Sinkhorn at certificate-grade accuracy is a genuine research problem: precision, dynamic range and fixed-depth division interact badly. It is not something to put on the critical path of the demo.

**Decision.** Move encrypted Sinkhorn to the v2 research track (`docs/07` §6), aimed at multi-dimensional problems where moderate ε is acceptable, such as distance estimates reported as a certified interval.

## Spike 4. The closed-form one-dimensional OT is exact

Script: `spikes/closed_form_1d_ot.py`

**Question.** Does the masked monotone coupling (`docs/03` §2) equal the LP optimum?

**Result.** On 5,002 books, including sparse and one-sided books, it was feasible and welfare-optimal on **5,002 / 5,002**, with worst welfare difference 0. The certificate gap was exactly 0 on **5,002 / 5,002**.

## Spike 5. The closed form under emulated CKKS

Script: `spikes/closed_form_fhe_emulation.py`

**Question.** Under FHE, |x| must come from a polynomial approximation of sign(x), and every operation adds CKKS noise. Do the decrypted, rounded trades still certify?

**Method.** Volumes are normalised by the public bound 2¹⁹ micro-lots. Every operation adds Gaussian noise of standard deviation 2⁻³⁵, approximating a scale Δ ≈ 2⁵⁰. sign(x) is approximated by n iterations of f(x) = (3x − x³)/2 followed by 3 iterations of (15x − 10x³ + 3x⁵)/8, and |x| = x·sign(x). Trades are rounded down with a κ = 2 micro-lot margin. The run counts as certified if it is feasible and its gap is below 0.1% of welfare.

**Result.**

| Sign iterations n | Approx. depth per \|x\| layer | Certified (< 0.1% gap, feasible) | Certified (< 0.5%) | Worst gap / welfare |
|---|---|---|---|---|
| 10 | ≈ 30 | 596 / 1,001 | 633 / 1,001 | 1.8% |
| 16 | ≈ 42 | **988 / 1,001** | **1,001 / 1,001** | 0.20% |
| 22 | ≈ 54 | 986 / 1,001 | 1,001 / 1,001 | 0.30% |
| 28 | ≈ 66 | 989 / 1,001 | 1,001 / 1,001 | 0.30% |

With the tolerance ρ = 0.5% (`docs/03` §5), **all 1,001 books certify from n = 16**. The residual gap comes from the κ rounding margin, not from approximation error, which is why it does not improve with more iterations.

**Reading and decisions.**
- v1 engine = closed form under CKKS, with about 42 levels per |x| layer and two layers (min/max, then max(0, ·)). That is about 90 levels in total, so about 8 collective refreshes with a 12-level budget between refreshes.
- The simple sign polynomials used here are far from optimal. Optimised composite polynomials (Cheon et al., ASIACRYPT 2020; Lee et al., minimax composite sign) should roughly halve the depth. Measure this in M5.
- Use scale Δ ≈ 2⁵⁰ (50-bit scaling primes), not 2⁴⁰.

## Summary of design changes caused by the spikes

| Before (research report) | After (this plan) | Evidence |
|---|---|---|
| Certificate from per-level Sinkhorn potentials, decrypted by both firms | One public clearing price Π; nothing beyond trades and Π decrypted | Spike 1, and the leakage argument in `docs/03` §7 |
| Encrypted Sinkhorn with Goldschmidt division and a warm start | Closed-form one-dimensional OT (monotone coupling) with polynomial min/max | Spikes 2–5 |
| Δ ≈ 2⁴⁰ | Δ ≈ 2⁵⁰ | Spikes 3 and 5 (noise floor) |
| Sinkhorn central to v1 | Sinkhorn as the v2 research track for multi-dimensional costs | Spike 3 |
