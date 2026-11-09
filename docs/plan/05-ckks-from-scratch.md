# 05. RNS-CKKS from scratch in Rust (`bt-arith`, `bt-ckks`)

This document specifies a small but genuine implementation of RNS-CKKS: enough to run the v1 matching engine on real ciphertexts. It deliberately skips bootstrapping; the two firms refresh ciphertexts jointly instead (`docs/06`).

Reference implementations to read alongside, for cross-checking only (do not copy code; keep licences clean):
- **Lattigo** (Go, `github.com/tuneinsight/lattigo`): `ring/`, `ckks/`, `multiparty/`. Its multiparty package is the closest match to what we build.
- **OpenFHE** (C++): CKKS with FLEXIBLEAUTO scaling.
- **HEAAN** (the original CKKS code): the special FFT used by the encoder.

Papers: Cheon–Kim–Kim–Song (ASIACRYPT 2017), the original CKKS; Cheon–Han–Kim–Kim–Song (SAC 2018), full-RNS CKKS; Han–Ki (CT-RSA 2020), hybrid key switching; Kim–Papadimitriou–Polyakov (CT-RSA 2022), reduced approximation error (exact scale management); Bossuat et al. (EUROCRYPT 2021), hoisting.

## 1. Parameters

### 1.1 Toy parameter set `TOY-12` (the demo default)

| Parameter | Value | Notes |
|---|---|---|
| Ring degree N | 2¹² = 4096 | Slots n = N/2 = 2048 |
| Base prime q₀ | ≈ 2⁶⁰ | Holds the final value: \|m\| must stay below q₀/2 |
| Scaling primes q₁ … q₁₂ | 12 primes ≈ 2⁵⁰ | Δ ≈ 2⁵⁰ (Spike 5: 2⁴⁰ is too coarse); 12 levels between refreshes |
| Special primes p₀, p₁, p₂ | 3 primes ≈ 2⁶⁰ | Hybrid key switching modulus P ≈ 2¹⁸⁰ |
| dnum (key-switching digits) | 5 | Groups of about 3 primes ≈ 2¹⁵⁰–2¹⁶⁰ each, below P |
| log QP | ≈ 60 + 600 + 180 = 840 bits | |
| Secret distribution | Uniform ternary {−1, 0, 1} | |
| Error distribution | Discrete Gaussian, σ = 3.19 | Toy sampler (rounded continuous Gaussian), not constant-time |
| Security | **None** at N = 2¹² with log QP = 840 | Labelled "toy, insecure" everywhere |

### 1.2 Production-shaped set `PROD-15` (documented, for benchmarks)

The same modulus chain (log QP ≈ 840) at **N = 2¹⁵** is within the Homomorphic Encryption Standard's 128-bit bound for ternary secrets (log QP ≤ 881 at N = 2¹⁵). Re-check with the lattice estimator before claiming a security level. Everything in this document is parameterised by N, so `PROD-15` is the same code with 8× larger arrays.

### 1.3 Key and ciphertext sizes at `TOY-12`

- Ciphertext at the top level: 2 polynomials × 13 limbs × 4096 × 8 bytes = **0.85 MB**.
- Key-switching key: dnum (5) × 2 × (13 + 3) limbs × 4096 × 8 bytes = **5.2 MB** each.
- v1 needs only the relinearisation key (≈ 5 MB). The v2 Sinkhorn engine adds about 12 rotation keys (≈ 60 MB), held by the server worker. dnum = 5 is the choice that keeps log QP within the production bound while keeping keys manageable: fewer digits need a larger P, more digits mean larger keys.
- Rotations (§5.2) are still implemented and tested in M3: v2 needs them, and they are cheap to build once key switching works.

## 2. Modular arithmetic (`bt-arith`)

All primes are below 2⁶¹, so lazy reductions in [0, 4q) fit in a `u64`.

```rust
#[inline] fn add_mod(a: u64, b: u64, q: u64) -> u64 { let s = a + b; if s >= q { s - q } else { s } }
#[inline] fn sub_mod(a: u64, b: u64, q: u64) -> u64 { if a >= b { a - b } else { a + q - b } }
#[inline] fn mul_mod(a: u64, b: u64, q: u64) -> u64 { ((a as u128 * b as u128) % q as u128) as u64 } // reference; replace with Barrett

// Barrett with a 128-bit constant mu = floor(2^128 / q), stored as (hi, lo): standard; see Lattigo ring/modular_reduction.go
// Shoup multiplication by a fixed operand w (twiddles, constants): w' = floor(w * 2^64 / q)
#[inline] fn mul_shoup(a: u64, w: u64, w_shoup: u64, q: u64) -> u64 {
    let hi = ((a as u128 * w_shoup as u128) >> 64) as u64;
    let r = a.wrapping_mul(w).wrapping_sub(hi.wrapping_mul(q)); // in [0, 2q)
    if r >= q { r - q } else { r }
}
```

Unit tests: compare every routine against the `u128 %` reference on random inputs and on edge cases (0, q−1, values near 2⁶⁴ for lazy forms).

### 2.1 NTT-friendly primes

Every modulus must satisfy q ≡ 1 (mod 2N), so that a primitive 2N-th root of unity exists (Theorem 2.1 in your NTT paper).

```
fn find_primes(bits, count, n, exclude) -> Vec<u64>:
    step = 2n
    start from the largest k with k*step + 1 < 2^bits, then go downward (and for scaling primes alternate above/below 2^bits to keep their product close to Δ^L)
    keep q = k*step + 1 if is_prime(q) and q not in exclude
```

`is_prime`: deterministic Miller–Rabin with bases {2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37}, which is correct for all 64-bit inputs.

### 2.2 Primitive 2N-th root ψ

Draw a random x in [2, q−1] and set ψ = x^((q−1)/(2N)) mod q. Accept if ψ^N ≡ −1 (mod q), which makes it a primitive 2N-th root. Precompute the powers ψ^{bitrev(i)} and ψ^{−bitrev(i)} for i < N, each with its Shoup companion, plus N⁻¹ mod q.

### 2.3 Negacyclic NTT (Lance writes this)

Forward: merged Cooley–Tukey with the ψ twist folded into the twiddles, output in bit-reversed order. Inverse: Gentleman–Sande, then multiply by N⁻¹. The pointwise product is order-agnostic, so no reordering is needed (your paper, §2.3 and §6).

```rust
pub fn ntt_forward(a: &mut [u64], psi_rev: &[u64], psi_rev_sh: &[u64], q: u64) {
    let n = a.len(); let (mut t, mut m) = (n, 1);
    while m < n {
        t >>= 1;
        for i in 0..m {
            let (w, ws) = (psi_rev[m + i], psi_rev_sh[m + i]);
            let j1 = 2 * i * t;
            for j in j1..j1 + t {
                let u = a[j];
                let v = mul_shoup(a[j + t], w, ws, q);
                a[j] = add_mod(u, v, q);
                a[j + t] = sub_mod(u, v, q);
            }
        }
        m <<= 1;
    }
}
// ntt_inverse: Gentleman–Sande mirror with psi_inv_rev, then scale by N^{-1} (fold into the last stage).
```

Then optimise with Harvey lazy butterflies (values in [0, 4q), one correction at the end), exactly as Guideline 6.1 in your paper. Under WebAssembly SIMD the inner loop vectorises poorly, because there is no 64×64→128 multiply. Leave the scalar loop first and measure before attempting SIMD tricks.

Test: for random a and b, INTT(NTT(a) ⊙ NTT(b)) equals the schoolbook negacyclic product mod q.

### 2.4 RNS polynomials

```rust
pub struct Basis { pub moduli: Vec<u64>, /* per-modulus NTT tables, Barrett constants */ }
pub struct RnsPoly { pub limbs: Vec<Vec<u64>>, pub ntt_form: bool }   // limbs[i][k] = coefficient/evaluation k mod q_i
```

Operations are limb-wise: add, sub, negate, multiply (NTT form), multiply by a scalar, and switch between NTT and coefficient form.

### 2.5 Fast basis conversion (used in rescale, ModUp, ModDown)

For x represented in basis B = {b₀, …, b_{r−1}} (product B), its representation in another basis C:

```
y_c = Σ_i [ x_i · (B/b_i)^{-1} ]_{b_i} · (B/b_i)   mod c       for each c in C
```

The result equals x + u·B for a small integer 0 ≤ u < r ("approximate" conversion). This is acceptable in ModUp, where the extra multiple of B lands in the noise after ModDown. Precompute (B/b_i)^{−1} mod b_i and (B/b_i) mod c for all i and c.

## 3. Encoding (`bt-ckks::encoder`)

Slots j = 0 … n−1 correspond to the primitive 2N-th roots ζ_j = ζ^{5^j mod 2N}, with ζ = e^{iπ/N}. A real polynomial m(X) is determined by its values at the ζ_j and their conjugates.

**Encode** z ∈ ℂⁿ (real vectors in our use) at scale Δ:
1. Coefficients of the interpolating polynomial: m_k = (2/N) · Σ_j Re(z_j · ζ_j^{−k}) for k = 0 … N−1. This O(N·n) formula is for tests; production uses the O(N log N) "special FFT" (HEAAN `fftSpecialInv`, Lattigo `ckks/encoder.go`).
2. Scale and round: M_k = round(Δ · m_k), an `i64`. Keep |z| ≤ 2⁹ so |M_k| < 2⁶⁰ at Δ = 2⁵⁰.
3. Reduce M_k into each RNS limb (signed to unsigned mod q_i), then NTT each limb.

**Decode:** INTT, centre-lift, divide by the scale, then evaluate at the ζ_j with the special FFT.

**Decrypting at level 0 is enough.** The decryption result needs only the q₀ limb when |m| < q₀/2, so drop all other limbs first. This avoids multi-precision CRT reconstruction entirely.

## 4. Keys, encryption, decryption

Single-party versions are used in tests; the two-party versions are in `docs/06`.

- **Secret key** s: uniform ternary, kept in NTT form over Q_L ∪ P.
- **Public key**: pk = (−a·s + e, a) mod Q_L, with a uniform and e Gaussian.
- **Encrypt** m (a plaintext at level L): sample v ternary and e₀, e₁ Gaussian; ct = (v·pk₀ + e₀ + m, v·pk₁ + e₁).
- **Decrypt**: m' = c₀ + c₁·s (mod q₀ after dropping limbs).

Ciphertext metadata: `level` (number of scaling primes still present) and `scale: f64`.

## 5. Hybrid key switching (relinearisation and rotations)

**Goal.** Given d ∈ R_{Q_ℓ} and a key for "s′ → s", output (k₀, k₁) with k₀ + k₁·s ≈ d·s′.

**Digits.** Partition the primes of Q_L into dnum groups Q⁽⁰⁾, …, Q⁽ᵈⁿᵘᵐ⁻¹⁾. The gadget element for digit g is

```
w_g = P · (Q_L / Q^{(g)}) · [ (Q_L / Q^{(g)})^{-1} ]_{Q^{(g)}}       (mod Q_L·P)
```

**Key generation** (single-party; two-party in `docs/06`):

```
ksk_g = ( −a_g · s + e_g + w_g · s′ ,  a_g )        mod Q_L·P,   for g = 0 … dnum−1
```

**Switching at level ℓ:**
1. For each digit g that intersects Q_ℓ, take the residues of d in that digit's primes (restricted to those present at level ℓ). This is d_g = [d]_{Q^{(g)}}, exact.
2. **ModUp:** extend d_g to all primes of Q_ℓ ∪ P with fast basis conversion (§2.5).
3. Inner product in NTT form: (u₀, u₁) = Σ_g d_g · (ksk_g restricted to Q_ℓ ∪ P).
4. **ModDown:** (k₀, k₁) = ((u − Conv_{P→Q_ℓ}([u]_P)) · P⁻¹) mod Q_ℓ, applied to u₀ and u₁.

**Why one top-level key works at every level.** Modulo any q_j in Q_ℓ, the gadget values w_g reduce to P·1 for the digit containing q_j and to 0 for the others, so Σ_g d_g·w_g ≡ P·d (mod q_j). The CRT identity holds at every level.

**Noise after switching** ≈ dnum · N · σ · max_g Q⁽ᵍ⁾ / P plus a rounding term. With P ≳ max digit modulus, this is about 2¹⁶ on a scale of 2⁵⁰, so about 34 bits of precision survive.

### 5.1 Relinearisation

ct × ct gives (d₀, d₁, d₂) = (c₀c₀′, c₀c₁′ + c₁c₀′, c₁c₁′). Key-switch d₂ from s² to s using the relinearisation key (s′ = s²), and add the result to (d₀, d₁).

### 5.2 Rotations

Rotating the slots by k positions applies the automorphism φ_g : X ↦ X^g with g = 5^k mod 2N.
- In coefficient form, coefficient i moves to index (i·g mod 2N); if that index is ≥ N, subtract N and negate.
- In NTT form, φ_g permutes the bit-reversed evaluation points: precompute the permutation table per g.
- Then key-switch φ_g(c₁) from φ_g(s) to s with the rotation key for g (s′ = φ_g(s)).

**Hoisting** (optional in v1): when several rotations of the same ciphertext are needed, do ModUp once and reuse the digits (Halevi–Shoup; Bossuat et al. double hoisting). This roughly halves the cost of the engine's rotate-and-sum steps.

## 6. Evaluation operations

| Operation | Level cost | Notes |
|---|---|---|
| `add`, `sub` | 0 | Operands must share level and scale; drop limbs of the higher one first |
| `add_plain`, `add_const` | 0 | Encode the constant at the ciphertext's exact scale |
| `mul_plain` (vector), `mul_const` | 1 after rescale | Encode at scale q_ℓ (see §7) so the scale is preserved after rescale |
| `mul` (ct × ct) + relinearise | 1 after rescale | |
| `square` | 1 | Saves one multiplication in the tensor |
| `rotate(k)` | 0 | One key switch |
| `rescale` | consumes 1 | c_j ← (c_j − [c_ℓ]_{q_j}) · q_ℓ⁻¹ mod q_j for j < ℓ, where [c_ℓ]_{q_j} is the centred lift of the last limb (INTT that limb, lift, NTT in q_j). Add ⌊q_ℓ/2⌋ before lifting to round to nearest. |
| `drop_level` | free | Discard top limbs without dividing; used to align levels |

## 7. Scale management

Use **exact scale tracking** (Kim–Papadimitriou–Polyakov, implemented in OpenFHE as FLEXIBLEAUTO):
- Each ciphertext carries an `f64` scale. After `mul` and `rescale` at level ℓ, the new scale is Δ_{ℓ−1} = Δ_ℓ² / q_ℓ.
- Define the canonical scale per level by this recursion from Δ_L = q_L. Whenever two ciphertexts must be added, they are at the same level and so share the canonical scale.
- Plaintext constants multiplied into a level-ℓ ciphertext are encoded at scale q_ℓ, so that after rescale the result's scale equals the ciphertext's scale.
- Assert in debug builds that |scale₁/scale₂ − 1| < 2⁻⁴⁰ before every addition.

## 8. Precision targets

Measured by the M3 tests, comparing decrypted values against plaintext f64:

| Operation | Target max absolute error (values normalised to ≤ 1) |
|---|---|
| Fresh encrypt and decrypt | ≤ 2⁻⁴⁰ |
| One ct × ct + rescale | ≤ 2⁻³⁶ |
| One rotation | ≤ 2⁻³⁶ |
| Public 8×8 matrix × encrypted vector | ≤ 2⁻³⁴ |
| Composite sign polynomial (depth ≈ 42) on inputs with \|x\| ≥ 2⁻¹⁹ | Correct sign; \|x\|·sign error ≤ 2⁻²⁰ |

## 9. API sketch (`bt-ckks`)

```rust
pub struct Params { pub n_log: u32, pub q0_bits: u32, pub q_bits: u32, pub levels: usize, pub p_count: usize, pub dnum: usize, pub sigma: f64 }
pub struct Context { /* bases, NTT tables, encoder FFT tables, automorphism tables, gadget constants */ }
impl Context { pub fn new(p: &Params) -> Self; pub fn slots(&self) -> usize; }

pub struct Plaintext { pub poly: RnsPoly, pub level: usize, pub scale: f64 }
pub struct Ciphertext { pub c: [RnsPoly; 2], pub level: usize, pub scale: f64 }
pub struct SecretKey(Secret<RnsPoly>);
pub struct PublicKey { pub b: RnsPoly, pub a: RnsPoly }
pub struct SwitchingKey { pub digits: Vec<[RnsPoly; 2]> }
pub struct EvalKeys { pub relin: SwitchingKey, pub rotations: BTreeMap<i32, SwitchingKey> }

impl Context {
    pub fn encode(&self, z: &[f64], level: usize, scale: f64) -> Plaintext;
    pub fn decode(&self, pt: &Plaintext) -> Vec<f64>;
    pub fn encrypt(&self, pk: &PublicKey, pt: &Plaintext, rng: &mut impl CryptoRngCore) -> Ciphertext;
    pub fn decrypt(&self, sk: &SecretKey, ct: &Ciphertext) -> Plaintext;          // single-party, tests only
}
pub struct Evaluator<'a> { ctx: &'a Context, keys: &'a EvalKeys }
impl Evaluator<'_> {
    pub fn add(&self, a: &Ciphertext, b: &Ciphertext) -> Ciphertext;
    pub fn sub(&self, a: &Ciphertext, b: &Ciphertext) -> Ciphertext;
    pub fn add_const(&self, a: &Ciphertext, c: f64) -> Ciphertext;
    pub fn mul_plain(&self, a: &Ciphertext, z: &[f64]) -> Ciphertext;              // encodes at scale q_level, rescales
    pub fn mul_const(&self, a: &Ciphertext, c: f64) -> Ciphertext;
    pub fn mul(&self, a: &Ciphertext, b: &Ciphertext) -> Ciphertext;               // tensor, relinearise, rescale
    pub fn square(&self, a: &Ciphertext) -> Ciphertext;
    pub fn rotate(&self, a: &Ciphertext, k: i32) -> Ciphertext;
    pub fn rotate_hoisted(&self, a: &Ciphertext, ks: &[i32]) -> Vec<Ciphertext>;   // optional
    pub fn drop_to(&self, a: &Ciphertext, level: usize) -> Ciphertext;
}
```

`Secret<T>` zeroises on drop (`zeroize` crate) and prints `<secret>` in `Debug`.

## 10. Serialisation

- `postcard` with a version byte and a parameter-set identifier (a hash of `Params`). Reject mismatches.
- Ciphertext wire format: header {version, params_id, level, scale} followed by raw little-endian `u64` limbs in NTT form.
- The router logs byte length and SHA-256 per message; the UI shows "c0 7f3a…", the first bytes of the real limbs.

## 11. M3 test plan

1. **Arithmetic:** Barrett and Shoup against the u128 reference; Miller–Rabin against a sieve for small values and known primes and pseudoprimes.
2. **NTT:** negacyclic convolution property; NTT(INTT(x)) = x; the lazy version equals the strict version.
3. **Encoder:** decode(encode(z)) ≈ z within 2⁻⁴⁰; the special FFT equals the naive formula.
4. **Encrypt/decrypt:** error statistics over 1,000 trials within §8 targets.
5. **Homomorphic properties:** add, mul, rotate and mul_plain against plaintext f64 on random vectors, at every level.
6. **Key switching:** switching s′ → s for random s′; noise growth within the predicted bound.
7. **Cross-check** (optional but valuable): a small Go program using Lattigo with the same parameters and seeds produces test vectors for the encoder and NTT. Encryption is randomised, so compare decryption results only.
8. **Benchmarks** (`criterion`, native; `performance.now()` in wasm): NTT, mul+relinearise, rotate, rescale at `TOY-12`. Record them in `docs/11`.

## 12. Security notes for this layer

- Toy parameters are insecure by design and are labelled in the UI, `info()` and the README.
- The Gaussian sampler is not constant-time. That is acceptable for a demo; replace it (with a CDT sampler using constant-time selection) before any real use.
- Never expose raw decryptions of computed ciphertexts without the smudging noise of `docs/06` (the IND-CPA-D attacks of Li–Micciancio and Guo et al.).
- Seed every random stream from `getrandom` (the browser's `crypto.getRandomValues`). Deterministic seeds are allowed only in tests and are marked as such.
