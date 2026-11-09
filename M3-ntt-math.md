# M3: The negacyclic NTT and word-size modular arithmetic

**Owner:** Lance Tersou · **Code:** `crates/bt-arith` · **Status:** draft v1, all statements below are checked by the tests in §10.

This is the arithmetic floor under the CKKS layer (`docs/05`). Every ciphertext multiplication, rescale and key switch reduces to the operations here:
- polynomial products in R_q = Z_q[X]/(X^N + 1), computed with the NTT;
- word-size modular reduction (Shoup and Barrett);
- the Galois automorphisms X ↦ X^g used for slot rotations.

Section map (the code comments cite these):
- §1 Why the NTT
- §2 NTT-friendly primes and roots of unity (`primes.rs`)
- §3 Word-size reduction: Shoup and Barrett, with proofs (`modops.rs`)
- §4 The forward transform: merged Cooley–Tukey, derivation and index invariant (`ntt.rs`)
- §5 The inverse transform: merged Gentleman–Sande; lazy-reduction invariants for both directions
- §6 The convolution theorem
- §7 Automorphisms in both domains (`automorphism.rs`)
- §8 From one prime to RNS (link to M4)
- §9 Cost model and performance notes
- §10 Test map
- §11 What to be ready to defend

---

## 1. Why the NTT

CKKS ciphertexts are pairs of polynomials in R_Q = Z_Q[X]/(X^N + 1), with N = 2^12 for TOY-12 and 2^15 for PROD-15. A product of two polynomials computed naively costs N² coefficient multiplications: 16.7 million at N = 4096. The NTT reduces this to O(N log N).

It works because R_q, for a suitable prime q, is isomorphic to N copies of Z_q (the Chinese remainder theorem). Products become pointwise:

```
a · b  =  INTT( NTT(a) ⊙ NTT(b) ),        cost: 2 forward + 1 inverse + N pointwise products.
```

The modulus Q is a product of word-sized primes (RNS, §8), so everything below is for a single prime q < 2⁶².

---

## 2. NTT-friendly primes and roots of unity

### 2.1 Existence of ψ
**Lemma 2.1.** Let q be prime and N a power of two. Z_q contains a primitive 2N-th root of unity ψ (ψ^{2N} = 1, ψ^N = −1) iff q ≡ 1 (mod 2N).
*Proof.* Z_q^* is cyclic of order q − 1, so it has an element of order d iff d | q − 1. ∎

**Lemma 2.2 (the constructor is correct).** Let q ≡ 1 (mod 2N) and x ∈ Z_q^*. Put ψ = x^{(q−1)/2N}. Then ψ has order exactly 2N iff x is a quadratic non-residue.
*Proof.* ψ^{2N} = x^{q−1} = 1, so ord(ψ) divides 2N, which is a power of two. So ord(ψ) = 2N iff ψ^N ≠ 1. In the field Z_q, ψ^N squares to 1, so ψ^N = ±1. Finally ψ^N = x^{(q−1)/2} = (x/q), the Legendre symbol (Euler's criterion). ∎

`primitive_2n_root` tries x = 2, 3, … and returns the first ψ with ψ^N ≡ −1. Half of all x are non-residues, and the smallest one is tiny in practice (O(log² q) under GRH), so the loop ends after a few steps. The output is deterministic, which matters: both firms must derive identical tables from the public parameters.

### 2.2 The CRT isomorphism
**Lemma 2.3.**

```
X^N + 1 = Π_{k=0}^{N−1} (X − ψ^{2k+1})
```

and the evaluation map

```
R_q → Z_q^N,     a ↦ ( a(ψ^1), a(ψ^3), …, a(ψ^{2N−1}) )
```

is a ring isomorphism.
*Proof.* (ψ^{2k+1})^N = (ψ^N)^{2k+1} = (−1)^{2k+1} = −1, so each ψ^{2k+1} is a root. The N odd powers are distinct because ord(ψ) = 2N. A degree-N polynomial with N distinct roots over a field factors as stated. The factors are pairwise coprime, so the CRT gives R_q ≅ Π Z_q[X]/(X − ψ^{2k+1}) ≅ Z_q^N, and reduction modulo (X − c) is evaluation at c. ∎

The NTT is a fast algorithm for this evaluation map, with the outputs in a particular order (§4.3).

### 2.3 Primality: deterministic Miller–Rabin
Write n − 1 = 2^r d with d odd. If n is prime, then for every base a ∈ [2, n−2], either a^d ≡ 1 or a^{2^s d} ≡ −1 for some s < r. The reason: the sequence a^d, a^{2d}, …, a^{2^r d} = 1 (Fermat) ends at 1, and in a field the only square roots of 1 are ±1.

A composite n that passes for base a is a *strong pseudoprime to base a*. Sorenson and Webster (2015) computed the smallest composite that is a strong pseudoprime to all twelve bases 2, 3, …, 37:

```
ψ_12 = 318,665,857,834,031,151,167,461 ≈ 3.2·10²³ > 2⁶⁴ ≈ 1.8·10¹⁹.
```

So for every n < 2⁶⁴, testing these twelve bases is a *proof* of primality, not a probabilistic test. `is_prime` first trial-divides by the bases, which covers n equal to a base or divisible by one. Tests: agreement with a sieve below 200,000; rejection of known strong pseudoprimes; acceptance of 2⁶¹ − 1 and 2⁶⁴ − 59.

### 2.4 How many NTT primes are there, and how to pick them
By Dirichlet's theorem (in its prime-number-theorem form for arithmetic progressions), primes are equidistributed over the φ(2N) = N invertible residue classes mod 2N. So among integers ≡ 1 (mod 2N) near x, a fraction of about (2N / φ(2N)) / ln x = 2 / ln x is prime: about 1 in 17 near 2⁵⁰ and about 1 in 21 near 2⁶⁰. Candidates are spaced 2N apart, so plenty of primes exist close to any target.

Two selection functions:
- `ntt_primes_below(bits, count, n, exclude)` returns the largest primes below 2^bits. Use it for q_0 ≈ 2⁶⁰ and the special primes P.
- `ntt_primes_near(bits, count, n, exclude)` alternates just above and just below 2^bits. Use it for the scaling primes q_1 … q_L ≈ Δ = 2⁵⁰.

**Why "near" matters.** Rescaling at level ℓ divides the scale by q_ℓ, mapping Δ² to Δ²/q_ℓ. If q_ℓ = Δ(1 + ε_ℓ), the new scale is Δ(1 − ε_ℓ + O(ε²)). With |ε| ≈ (number of candidates skipped) · 2N / Δ ≈ 2⁻³⁰, the drift is negligible, and alternating signs keeps the products of consecutive primes even closer to Δ^k. CKKS still tracks the exact scale as a rational number (`docs/05` §7); closeness only keeps that bookkeeping benign.

Measured on the actual TOY-12 chain (`cargo run -p bt-arith --example toy12_chain`): max |q_i / 2⁵⁰ − 1| = 6.8·10⁻¹⁰ ≈ 2⁻³⁰·⁵; log₂ Π q_i − 600 = −1.7·10⁻⁹ bits; log₂ QP = 840.00, as planned in `docs/05`. The example also prints each prime's ψ. The test `ntt_primes_are_ntt_friendly` asserts the product within 10⁻³ bits.

---

## 3. Word-size reduction

Every value is a u64 in [0, q) unless stated, with q < 2⁶². The 64 × 64 → 128-bit product is `u128` multiplication. On x86-64 and aarch64 that is one instruction for the high word; on wasm32 it lowers to a short helper (§9).

### 3.1 Shoup multiplication by a fixed constant
The twiddle factors are constants known in advance. For a constant w < q, precompute

```
w' = floor( w · 2⁶⁴ / q ).
```

To multiply any a < 2⁶⁴ by w:

```
q̂ = floor( a · w' / 2⁶⁴ )               (one high-half multiply)
r = a·w − q̂·q   (mod 2⁶⁴)               (two low-half multiplies and a subtraction)
```

**Theorem 3.1 (Shoup bound).** For every a < 2⁶⁴ and w < q < 2⁶³: 0 ≤ a·w − q̂·q < 2q, and the wrapping computation returns this value exactly.

*Proof.* Write w·2⁶⁴ = w'·q + e with 0 ≤ e < q, and a·w' = q̂·2⁶⁴ + f with 0 ≤ f < 2⁶⁴. Then

```
a·w − q̂·q = ( a·w'·q + a·e ) / 2⁶⁴ − q̂·q
          = ( q̂·2⁶⁴·q + f·q + a·e ) / 2⁶⁴ − q̂·q
          = ( f·q + a·e ) / 2⁶⁴.
```

This is ≥ 0, and since f < 2⁶⁴ and a·e < 2⁶⁴·q, it is < (2⁶⁴q + 2⁶⁴q)/2⁶⁴ = 2q. Because 2q < 2⁶⁴, the true value equals its residue mod 2⁶⁴, which is what wrapping arithmetic computes. ∎

- `mul_shoup_lazy` returns r ∈ [0, 2q).
- `mul_shoup` adds one conditional subtraction to reach [0, q).

The input a can be *any* 64-bit value, not just a reduced one. That is what makes lazy butterflies work (§5.3). Test: 80,000 random (a, w) with a drawn from the full 64-bit range, over four primes up to ≈ 2⁶².

**Why Shoup rather than Montgomery here.** For a fixed operand, Shoup costs one high multiply and two low multiplies, needs no conversion into and out of Montgomery form, and its output is a plain residue. Montgomery is the better choice for products of two variables on hardware without a fast high multiply. Barrett (next) is our variable × variable route.

### 3.2 Barrett reduction for variable products
For a product x = a·b with a, b < q (so x < q² < 2^{2s}, where s = bitlen(q)), precompute μ = floor(2^{2s} / q) and compute (HAC Algorithm 14.42 with base 2):

```
q̂ = floor( floor(x / 2^{s−1}) · μ / 2^{s+1} ),       r = x − q̂·q,       then subtract q at most twice.
```

**Theorem 3.2 (Barrett bound).** With Q = floor(x / q): Q − 2 ≤ q̂ ≤ Q, so 0 ≤ r < 3q.

*Proof.* Let α = x / 2^{s−1} and β = 2^{2s} / q, so that x/q = αβ / 2^{s+1}.

*Upper bound.* Flooring only decreases: q̂ ≤ αβ / 2^{s+1} = x/q, so q̂ ≤ Q.

*Lower bound.* floor(α) > α − 1 and μ = floor(β) > β − 1, so

```
floor(α)·μ / 2^{s+1}  >  ( αβ − α − β + 1 ) / 2^{s+1}  =  x/q − (α + β − 1) / 2^{s+1}.
```

Now α < 2^{2s} / 2^{s−1} = 2^{s+1}, and β ≤ 2^{2s} / 2^{s−1} = 2^{s+1} because q ≥ 2^{s−1}. So (α + β − 1)/2^{s+1} < 2, and q̂ > x/q − 3 ≥ Q − 3, i.e. q̂ ≥ Q − 2.

*Remainder.* r = (x − Qq) + (Q − q̂)q < q + 2q.

*Overflow.* The intermediate floor(α)·μ < 2^{s+1} · 2^{s+1} = 2^{2s+2} ≤ 2^{126} fits in a u128. ∎

Test: Barrett equals the u128 `%` reference on random products for each test prime.

### 3.3 Why q < 2⁶²
The lazy forward butterfly keeps values in [0, 4q) (§5.3), and 4q must fit in a u64. Shoup needs only q < 2⁶³. The TOY-12 and PROD-15 chains use q ≤ 2⁶⁰ for the base and special primes, so there is ample headroom.

---

## 4. The forward transform (merged Cooley–Tukey)

### 4.1 One splitting step
For any c ∈ Z_q, X^{2t} − c² = (X^t − c)(X^t + c), and the two factors are coprime when c ≠ 0 and q is odd. Write a polynomial of degree < 2t as p = p_lo + X^t·p_hi. Using X^t ≡ ±c in the two quotients:

```
p mod (X^t − c) = p_lo + c·p_hi,          p mod (X^t + c) = p_lo − c·p_hi.
```

Coefficient-wise this is the **Cooley–Tukey butterfly** (x, y) ↦ (x + c·y, x − c·y), applied to the t pairs (p_lo[j], p_hi[j]).

### 4.2 The recursion and the index invariant
Start from X^N + 1 = X^N − ψ^N: this is the case 2t = N, c = ψ^{N/2}. Split repeatedly. Number blocks the way the code does, with rev(·) the reversal of log₂N bits:

> **Invariant 𝓘(m).** Before the stage with m blocks (m = 1, 2, 4, …, N/2) and half-length t = N/(2m), block i (positions [2it, 2it + 2t)) holds a mod (X^{2t} − ζ_{m,i}), where
> ```
> ζ_{m,i} = w_{m,i}²,     w_{m,i} = ψ^{rev(m + i)} = psi_rev[m + i].
> ```

*Base case.* m = 1, i = 0: rev(1) = N/2, so ζ = ψ^N = −1 and the modulus is X^N + 1. The initial array is a itself. ✓

*Step.* The stage applies the butterfly with c = w_{m,i} to block i. By §4.1 the lower half then holds a mod (X^t − w_{m,i}) and the upper half holds a mod (X^t + w_{m,i}). These are blocks 2i and 2i+1 of the next stage (2m blocks, half-length t/2). So it suffices to show

```
w_{2m,2i}² = w_{m,i}       and       w_{2m,2i+1}² = −w_{m,i}.
```

Put k = m + i, so 2m + 2i = 2k and 2m + 2i + 1 = 2k + 1. Since this is not the last stage, k < N/2, so the top bit of k is 0 and rev(k) is even. Shifting k left by one shifts its reversal right by one, so rev(2k) = rev(k)/2, and setting the low bit adds N/2: rev(2k+1) = rev(2k) + N/2. Hence

```
ψ^{2·rev(2k)} = ψ^{rev(k)},       ψ^{2·rev(2k+1)} = ψ^{rev(k)} · ψ^N = −ψ^{rev(k)}. ∎
```

The "merged" in the name: classic negacyclic NTTs first twist a_j ↦ ψ^j a_j and then run a cyclic NTT with ω = ψ². Here the twist is folded into the twiddles, because the recursion starts directly from X^N + 1. That saves N multiplications and one pass over memory per transform (Pöppelmann–Oder–Güneysu; Longa–Naehrig).

### 4.3 Output order
**Theorem 4.1.** After the last stage (m = N/2, t = 1), slot k holds a(ψ^{e_k}) with e_k = 2·rev(k) + 1.

*Proof.* At the last stage, block i has length 2 and holds a mod (X² − w²), with w = ψ^{rev(N/2 + i)}. The butterfly writes a(w) to slot 2i and a(−w) to slot 2i + 1. For i < N/2:
- rev(N/2 + i) = 1 + rev(i), where rev(i) is even because i's top bit is 0;
- rev(2i) = rev(i)/2.

So rev(N/2 + i) = 2·rev(2i) + 1 = e_{2i}. For the odd slot, rev(2i+1) = rev(2i) + N/2, so e_{2i+1} = e_{2i} + N, and ψ^{e_{2i+1}} = −w. ∎

So the forward NTT outputs the evaluations of Lemma 2.3 in bit-reversed order of the odd exponents. `NttTable::eval_exponent(k)` returns e_k. The test `output_slots_are_evaluations_at_odd_powers` compares every slot with Horner evaluation at ψ^{e_k}. No reordering pass is needed: pointwise products do not care about order, and the inverse consumes this order directly.

### 4.4 The code, line by line
```
t = N, m = 1
while m < N:
    t = t/2
    for i in 0..m:   w = psi_rev[m+i]                        # w_{m,i}
        for j in 2it .. 2it+t:   (x, y) = (a[j], a[j+t])
            a[j], a[j+t] = x + w·y, x − w·y                  # §4.1 with c = w
    m = 2m
```
The pass has log₂N stages of N/2 butterflies, so N/2 · log₂N Shoup multiplications in total (24,576 at N = 4096).

---

## 5. The inverse transform and lazy reduction

### 5.1 Gentleman–Sande butterfly
Inverting (x', y') = (x + w·y, x − w·y) gives x' + y' = 2x and (x' − y')·w⁻¹ = 2y. The **GS butterfly** (u, v) ↦ (u + v, (u − v)·w⁻¹) therefore undoes one CT butterfly up to a factor 2.

### 5.2 Order and scaling
The inverse runs the stages in reverse: h = N/2, N/4, …, 1 blocks, with t = 1, 2, … . Block i at the stage with h blocks uses psi_inv_rev[h + i] = ψ^{−rev(h + i)} = w_{h,i}⁻¹, the inverse of the twiddle that the forward stage with m = h used on that same block. Each of the log₂N stages contributes a factor 2, so the result is N·a. The final pass multiplies by N⁻¹ mod q, using a Shoup constant. Input: bit-reversed evaluations. Output: natural-order coefficients.

### 5.3 Lazy-reduction invariants (Harvey)
Reducing after every addition wastes work. Harvey's trick keeps values in a slightly larger range and corrects only when needed.

**Forward.** Invariant: entering each stage, every entry is in [0, 4q).

```
u ← x;  if u ≥ 2q: u ← u − 2q              u ∈ [0, 2q)
v ← mul_shoup_lazy(y, w)                    v ∈ [0, 2q), valid since y < 4q < 2⁶⁴   (Theorem 3.1)
x ← u + v                                   ∈ [0, 4q)
y ← u + 2q − v                              ∈ (0, 4q)
```

The inputs lie in [0, q) ⊂ [0, 4q), so the invariant holds initially and is preserved by each stage. A final pass with two conditional subtractions (−2q, then −q) maps [0, 4q) to [0, q). Each butterfly costs one comparison instead of three.

**Inverse.** Invariant: entries in [0, 2q).

```
s ← u + v;  if s ≥ 2q: s ← s − 2q          s ∈ [0, 2q)
y ← mul_shoup_lazy(u + 2q − v, w⁻¹)        the argument is in (0, 4q) ⊂ [0, 2⁶⁴), so y ∈ [0, 2q)
```

The final `mul_shoup` by N⁻¹ returns fully reduced values. Theorem 3.1 allows any 64-bit input, so no extra correction is needed before it.

**Overflow budget.** Every intermediate value is below 4q ≤ 2⁶⁴ exactly when q ≤ 2⁶². `NttTable::new` asserts q < 2⁶². The test `extreme_inputs_respect_lazy_bounds` uses all-(q−1) inputs, the worst case for intermediate growth, with q just below 2⁶¹.

---

## 6. The convolution theorem

**Theorem 6.1.** For a, b ∈ R_q: INTT( NTT(a) ⊙ NTT(b) ) = a·b mod (X^N + 1, q).
*Proof.* NTT is the isomorphism of Lemma 2.3 (composed with a fixed permutation of the slots, §4.3), and multiplication in Z_q^N is pointwise. ∎

`NttTable::multiply` implements this. The test `roundtrip_and_convolution_small_and_large` compares it with the O(N²) schoolbook product `negacyclic_schoolbook` (which applies X^N = −1 explicitly) for N up to 1024. It also checks round-trips up to N = 4096 with 60- and 61-bit primes. Linearity is tested separately.

The pointwise step currently uses the u128 `%` reference (`mul_mod`). For M4, switch it to `Barrett::mul`, or keep operands in NTT form and fuse the product into key switching.

---

## 7. Galois automorphisms (slot rotations)

### 7.1 Definition
For odd g, the map φ_g : a(X) ↦ a(X^g) is a well-defined ring automorphism of R_q. It respects the relation X^N = −1 because (X^g)^N + 1 = (X^N)^g + 1 ≡ (−1)^g + 1 = 0 when g is odd. The φ_g compose as φ_g ∘ φ_h = φ_{gh mod 2N}.

**Coefficient form** (`apply_coeff`): X^i ↦ X^{ig}. Reduce the exponent e = ig mod 2N; if e ≥ N, use X^e = −X^{e−N}. So coefficient i moves to position e or e − N, with a sign flip in the second case.

### 7.2 Which g rotate CKKS slots
For N ≥ 2, (Z/2NZ)^* ≅ ⟨−1⟩ × ⟨5⟩, where 5 has order N/2.
- In the canonical CKKS encoding, slot j sits at the root ζ^{5^j}, so φ_5 shifts all N/2 slots by one position.
- φ_{5^k} shifts by k; this is `galois_element_for_rotation(k, N)`. Negative k means a right rotation, reduced mod N/2.
- φ_{2N−1} = φ_{−1} is complex conjugation.

v1 of the engine needs no rotations, because firms precompute their own cumulative sums (`docs/07`). Rotation keys and this module are for the v2 track and for tests.

### 7.3 Automorphisms in the NTT domain
**Theorem 7.1.** NTT(φ_g(a))[k] = NTT(a)[π_g(k)], where

```
π_g(k) = rev( (e_k · g mod 2N − 1) / 2 ),       e_k = 2·rev(k) + 1.
```

*Proof.* Slot k of NTT(φ_g(a)) is φ_g(a)(ψ^{e_k}) = a(ψ^{e_k g}). The exponent e' = e_k·g mod 2N is odd (a product of odd numbers, reduced modulo an even number). By Theorem 4.1, the slot holding a(ψ^{e'}) is the k' with 2·rev(k') + 1 = e', i.e. k' = rev((e' − 1)/2), since rev is an involution. ∎

So in the evaluation domain, an automorphism is a free permutation of slots: no multiplications at all. CKKS rotations are usually performed there, before the key switch. `ntt_permutation` and `apply_ntt` implement this. The test `coefficient_and_ntt_domain_agree` checks both routes for rotations {1, 2, 3, 7, 64, −1, −16} at N = 1024. The test `automorphisms_compose_and_invert` checks composition and φ_{5^k} ∘ φ_{5^{−k}} = id.

---

## 8. From one prime to RNS (the interface to M4)

CKKS uses Q = q_0 q_1 ⋯ q_L (and P = p_0 ⋯ p_{K−1} for key switching), with every prime NTT-friendly for the same N. By the CRT, R_Q ≅ Π_i R_{q_i}. A polynomial is stored as L+1 rows of N residues ("limbs"), and every ring operation is done limb by limb with that limb's `NttTable`.

Operations that do *not* act limb by limb are:
- rescaling (divide by q_ℓ and round);
- the basis extensions in hybrid key switching (`docs/05` §6).

Both need the CRT reconstruction coefficients (Q/q_i)⁻¹ mod q_i and (Q/q_i) mod p_j. Precompute them with `inv_mod_prime` and `mul_mod` as Shoup constants. They need care with lazy ranges, but no new mathematics beyond §3.

M4 needs from this crate: one `NttTable` per prime, `Barrett` for the pointwise products, and `ntt_primes_near` / `ntt_primes_below` to build the TOY-12 chain (q_0 ≈ 2⁶⁰, 12 primes ≈ 2⁵⁰, 3 special primes ≈ 2⁶⁰, all ≡ 1 mod 2¹³).

---

## 9. Cost model and performance notes

| Operation (one limb, N = 4096) | Work |
|---|---|
| forward or inverse NTT | 24,576 butterflies (one Shoup multiply each) + one final pass |
| pointwise product | 4096 Barrett multiplies |
| automorphism (NTT domain) | 4096 loads, no arithmetic |
| full TOY-12 polynomial at the top level (16 limbs) | ≈ 393k butterflies per transform |

Notes for the browser target:
1. **wasm32 has no 64 × 64 → 128 high multiply.** A `u128` product lowers to a helper built from 32-bit pieces, so the Shoup high half costs several machine multiplies. Measure with `criterion` natively and in wasm before optimising. If it dominates, there are two options: 32-bit primes with more limbs (`wasm simd128` then gives 4-lane `i32x4` arithmetic), or a 52-bit floating-point NTT. Both are known trade-offs; decide from measurements, not assumptions.
2. **Memory layout.** The twiddle tables are stored in the order the stages read them (psi_rev[m+i] is contiguous within a stage), which keeps reads sequential.
3. **This is your home ground.** Your published GPU NTT work, the IEEE paper and the GTC talk, covers the same transform at scale. In interviews, connect the two directly: the same invariants (§4.2, §5.3) govern the GPU kernel and this scalar reference. Quote only numbers that appear in the published versions.

Benchmarks are not in this draft. Add `benches/ntt.rs` with criterion for N ∈ {2¹², 2¹⁵} natively, and a wasm timing harness in M6.

---

## 10. Test map (`cargo test -p bt-arith --release`, about 0.05 s)

| Test | Checks |
|---|---|
| `modops::shoup_matches_reference_for_any_a` | Theorem 3.1 over the full 64-bit input range |
| `modops::barrett_matches_reference` | Theorem 3.2 |
| `modops::inverse_and_pow` | Fermat inverse, square-and-multiply |
| `primes::miller_rabin_matches_sieve` | §2.3 below 200,000 |
| `primes::miller_rabin_known_values` | strong pseudoprimes rejected; 2⁶¹−1, 2⁶⁴−59 accepted |
| `primes::ntt_primes_are_ntt_friendly` | q ≡ 1 (mod 2N), primality, distinctness, §2.4 product of near primes |
| `primes::primitive_root_has_exact_order` | Lemma 2.2 |
| `ntt::roundtrip_and_convolution_small_and_large` | §5.2 round trip, Theorem 6.1 |
| `ntt::output_slots_are_evaluations_at_odd_powers` | Theorem 4.1 |
| `ntt::extreme_inputs_respect_lazy_bounds` | §5.3 |
| `ntt::linearity` | NTT is Z_q-linear |
| `automorphism::coefficient_and_ntt_domain_agree` | Theorem 7.1 |
| `automorphism::automorphisms_compose_and_invert` | §7.1 group law |

Planned before M4 ships: proptest versions of the above; cross-checking `NttTable::with_root` outputs against an independent implementation (e.g. a small Python model, or OpenFHE's tables for the same q and ψ); and a cross-check of the full TOY-12 prime chain.

---

## 11. What to be ready to defend

- *"Why is ψ^N = −1 the right test?"* Lemma 2.2: the order divides a power of two, so the only way to fall short of 2N is ψ^N = 1.
- *"Prove your butterfly never overflows."* §5.3 plus Theorem 3.1, in two lines each.
- *"Where does the bit-reversed order come from, and why don't you undo it?"* Theorem 4.1. Pointwise products don't care about order, and the GS inverse consumes it directly.
- *"How do rotations work in the NTT domain?"* Theorem 7.1: a permutation computed once per g.
- *"Is your primality test probabilistic?"* No. For n < 2⁶⁴, the twelve fixed bases are a proof (§2.3).
- *"Why not Montgomery?"* §3.1: for fixed twiddles, Shoup is cheaper and needs no form conversion. Barrett handles variable × variable.
