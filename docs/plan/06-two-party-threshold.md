# 06. Two-party threshold CKKS (`bt-mhe`)

Firm A and Firm B jointly hold the secret key s = s_A + s_B. Neither can decrypt alone. The server holds only public material: the public key, the relinearisation key, and (in v2) rotation keys. Instead of bootstrapping, the firms help the server **refresh** ciphertexts that run out of levels, without seeing their contents.

Primary reference: Mouchet, Troncoso-Pastoriza, Bossuat, Hubaux, *Multiparty Homomorphic Encryption from Ring-Learning-with-Errors*, PETS 2021. Implementation reference: Lattigo's `multiparty/` package (`keygen_cpk.go`, `keygen_relin.go`, `keygen_gal.go`, `keyswitch_sk.go`, and the refresh protocol in `multiparty/mpckks`). Read both before coding. The equations below are what to implement, and they were checked algebraically. Still verify them against the paper during M4.

Security model for v1: **semi-honest (passive) firms and server for privacy.** Integrity of the *output* is protected separately by the certificate (`docs/08`), which holds even against malicious parties. Privacy against a firm that deviates inside these sub-protocols is not guaranteed in v1; see §8.

## 1. Session setup

### 1.1 Common reference string (CRS) by coin flipping

The protocols need public uniformly random polynomials that no single party chose.
1. Each firm samples a 32-byte seed σ_A or σ_B and publishes the commitment H(σ_A) or H(σ_B), with H = SHA-256 and a domain label.
2. After both commitments arrive, each reveals its seed. Each party checks the other's against its commitment.
3. The CRS seed is σ = H("cbt/crs" ‖ session_id ‖ σ_A ‖ σ_B).
4. Every CRS polynomial is expanded deterministically from σ with ChaCha20 and a domain-separation label, e.g. `crs(label, index)` → uniform polynomial in R_{Q_L·P}. Labels: `"ckg"`, `"rkg/g"` (one per digit g), `"rtg/k/g"` (per rotation k and digit g), `"refresh/r"` (per refresh number r).

Because CRS polynomials are derived from the seed, key shares can be sent **without** their CRS half. This roughly halves setup traffic.

### 1.2 Secret-key shares

Each firm samples s_i uniformly ternary. The joint key s = s_A + s_B has coefficients in {−2, …, 2}, which slightly increases noise (accounted for in parameter checks). Shares are fresh per session and zeroised at session end.

## 2. Collective public key (CKG, one round)

```
a = crs("ckg")                                     (in R_{Q_L})
party i:  p_i = −s_i · a + e_i                      (e_i Gaussian)
combine:  pk = (p_A + p_B, a) = (−s·a + e, a)
```

Anyone, including the server, can combine. Both firms check that pk was combined from their own shares.

## 3. Collective relinearisation key (RKG, two rounds)

Gadget values w_g are as defined in `docs/05` §5. Arithmetic is in R_{Q_L·P}.

**Round 1.** Party i samples an ephemeral ternary u_i and Gaussian errors, and for each digit g sends:

```
h0_{i,g} = −u_i · a_g + s_i · w_g + e0_{i,g}        a_g = crs("rkg/g")
h1_{i,g} =  s_i · a_g + e1_{i,g}
```

Aggregate h0_g = h0_{A,g} + h0_{B,g} and h1_g = h1_{A,g} + h1_{B,g}.

**Round 2.** Party i sends, for each digit g:

```
h0'_{i,g} = s_i · h0_g + e2_{i,g}
h1'_{i,g} = (u_i − s_i) · h1_g + e3_{i,g}
```

**Combine:** rlk_g = (Σ_i h0'_{i,g} + Σ_i h1'_{i,g}, h1_g).

**Correctness check.** With s = Σ s_i and u = Σ u_i:

```
s·h0_g + (u − s)·h1_g
  = s·(−u·a_g + s·w_g) + (u − s)·(s·a_g) + noise
  = s²·w_g − s²·a_g + noise
  = −s·h1_g + s²·w_g + noise'
```

That is the form of a switching key from s² to s with public part h1_g. ✓

## 4. Collective rotation keys (RTG, one round)

For each needed rotation k (Galois element g_k = 5^k mod 2N) and digit g:

```
a_{k,g} = crs("rtg/k/g")
party i:  h_{i,k,g} = −s_i · a_{k,g} + φ_k(s_i) · w_g + e_{i,k,g}
combine:  rtk_{k,g} = (h_{A,k,g} + h_{B,k,g}, a_{k,g}) = (−s·a + φ_k(s)·w_g + e, a)
```

This is correct because the automorphism φ_k is linear: φ_k(s_A) + φ_k(s_B) = φ_k(s). **v1 needs no rotation keys** (`docs/07` §1). Implement and test RTG in M4 anyway, because the v2 Sinkhorn engine needs about 12 of them.

## 5. Threshold decryption (both firms learn the output)

Used once per run, on the masked trades ciphertext. The server must **not** learn the trades, so shares go firm-to-firm only (routed through the main-thread bus, but encrypted between the firms; see `docs/09` §5).

```
input: ct = (c0, c1), dropped to level 1 (Q₁ = q₀·q₁ ≈ 2¹¹⁰; decode with i128 CRT)
party i: d_i = s_i · c1 + E_i        E_i ← smudging noise, standard deviation σ_sm
each firm: m = c0 + d_A + d_B  (mod Q₁), centre-lift, decode
```

### 5.1 Smudging, and an honest limitation

Releasing exact CKKS decryptions of computed ciphertexts leaks information about the secret key (Li–Micciancio, EUROCRYPT 2021; key-recovery attacks by Guo et al., USENIX Security 2024). If Firm A recovered s, it could decrypt Firm B's input ciphertext. Smudging noise E_i must therefore exceed the ciphertext noise by a statistical factor 2^λ, using a **worst-case** noise bound (Guo et al. broke average-case estimates).

The tension: computation leaves a relative error of about 2⁻³⁰ (`docs/05` §8), and the output needs a precision of about 2⁻¹⁹ (one micro-lot on the normalised scale). That leaves only **λ ≈ 11 bits** for smudging at `TOY-12`, which is not a meaningful security level.

**v1 decision:** use λ = 11 with a worst-case bound, and say so on the page. **Production paths** (research track):
- (a) a larger scale and more precise sign polynomials, to buy more headroom;
- (b) switch the final, integer-valued stage to an exact scheme (BFV/BGV) before decryption, so outputs are exact integers and no CKKS noise leaks;
- (c) decrypt to additive secret shares and round inside a small two-party computation.

Record this in the threat model.

## 6. Collective refresh (instead of bootstrapping)

When a ciphertext reaches the refresh level, the server asks both firms to help refresh it.

**Requirement on the refresh level ℓ_r.** The masked value m − M must be recoverable exactly, so

```
log₂ Q_{ℓ_r} ≥ log₂ ‖m‖∞ + λ_m + 2
```

with ‖m‖ ≈ scale × max|value| ≈ 2⁵⁰ × 2¹ and masking parameter λ_m = 40: log Q ≥ 93. Level 1 (q₀q₁ ≈ 2¹¹⁰) works. **Refresh at level 1, never at level 0.**

**Protocol** (refresh number r; a' = crs("refresh/r") over Q_L):

```
server → firms:  c1 (at level 1), r
party i:  sample mask M_i, coefficients uniform in [−2^{log‖m‖ + λ_m}, 2^{log‖m‖ + λ_m}]
          h0_i = s_i · c1 − M_i + e0_i      (mod Q₁; e0_i smudging noise)
          h1_i = M_i − s_i · a' + e1_i      (mod Q_L; lift M_i to Q_L as a signed integer)
firms → server:  h0_i, h1_i
server:   c0' = c0 + h0_A + h0_B           (mod Q₁)  =  m − (M_A + M_B) + e   (small, so lift it exactly)
          lift c0' to Q_L (centred), then
          ct' = (c0' + h1_A + h1_B, a')    (mod Q_L)  =  (m − s·a' + e', a')
```

ct' decrypts to m + e' under s, and is now at the top level L. The server sees only m − M, statistically hidden by the masks. The firms see only c1, which is pseudorandom. **Scale:** ct' keeps the input's scale value; the engine's scale bookkeeping must accept non-canonical scales after a refresh (`docs/05` §7 assert relaxed to the tracked value).

**Cost per refresh, per firm:** one multiplication at 2 limbs and one at 13 limbs, plus sampling. Each firm sends about 64 KB (h0) + 425 KB (h1). With about 8 refreshes per run, that is about 4 MB per firm. Show it in the network log.

## 7. API sketch (`bt-mhe`)

```rust
pub struct Crs { seed: [u8; 32] }
impl Crs {
    pub fn poly(&self, ctx: &Context, label: &str, idx: u32, basis: BasisId) -> RnsPoly;
}

pub struct Party { role: Role, s: Secret<RnsPoly>, ctx: Arc<Context>, crs: Crs }
impl Party {
    pub fn ckg_share(&self, rng) -> CkgShare;
    pub fn rkg_round1(&mut self, rng) -> RkgShare1;                  // keeps u_i internally
    pub fn rkg_round2(&self, agg1: &RkgAgg1, rng) -> RkgShare2;
    pub fn rtg_share(&self, k: i32, rng) -> RtgShare;
    pub fn decrypt_share(&self, ct: &Ciphertext, lambda: u32, rng) -> DecShare;      // smudged
    pub fn refresh_share(&self, c1: &RnsPoly, r: u32, lambda_m: u32, rng) -> RefreshShare;
}
pub fn combine_pk(ctx, crs, shares: &[CkgShare]) -> PublicKey;
pub fn combine_rlk(ctx, crs, r1: &[RkgShare1], r2: &[RkgShare2]) -> SwitchingKey;
pub fn combine_rtk(ctx, crs, k: i32, shares: &[RtgShare]) -> SwitchingKey;
pub fn combine_decrypt(ctx, ct: &Ciphertext, shares: &[DecShare]) -> Plaintext;
pub fn combine_refresh(ctx, crs, ct: &Ciphertext, r: u32, shares: &[RefreshShare]) -> Ciphertext;
```

Each share type is `Serialize` + `Deserialize` with a header {session_id, role, protocol, round, params_id}. The combiner rejects shares with mismatched headers.

## 8. Security notes and v2 hardening

- **Passive security only for privacy.** A malicious firm could send malformed shares. Output integrity is still protected (the certificate rejects wrong trades), but malformed shares could in principle leak information or bias keys. v2: attach lattice zero-knowledge proofs of share well-formedness (LaZer, Greco, Hint-MLWE; see the research report).
- **Smudging λ is small at toy parameters** (§5.1). State this on the page.
- **Fresh keys every session**, zeroised on drop. Never reuse key shares across sessions.
- **Ordering of reveals:** commit-then-reveal for CRS seeds prevents either firm biasing the CRS.
- **Liveness:** if a firm stops responding, the run aborts. Nothing is revealed, because no decryption happens without both shares.

## 9. M4 test plan

1. Two parties' CKG gives a pk under which single-party decryption with s = s_A + s_B works (a test-only helper combines the shares).
2. The RKG output relinearises correctly: mul then decrypt equals the plaintext product, within the bound.
3. RTG keys rotate correctly for every k in the engine's set.
4. Threshold decryption equals single-key decryption up to the smudging noise. Measure the noise empirically against the bound.
5. Refresh: a level-1 ciphertext refreshes to level L, decrypts to the same message within the bound, and allows a further L−1 multiplications.
6. Negative tests: mismatched headers rejected; a refresh at level 0 rejected (insufficient modulus for masking).
7. Leakage sanity check (statistical): over 1,000 refreshes of the same message, the server's view c0' shows no correlation with m beyond the masking bound.
