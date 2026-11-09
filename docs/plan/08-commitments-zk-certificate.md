# 08. Commitments, zero-knowledge proofs and the certificate (`bt-cert`)

This is the integrity half of the system. Everything in this document is **real** in v1 except the single item marked simulated: the proof binding ciphertexts to commitments.

## 1. Libraries

| Crate | Use | Notes |
|---|---|---|
| `bulletproofs` (dalek-cryptography) | Aggregated range proofs on Pedersen commitments | Pin at M0. Use its `PedersenGens` and `BulletproofGens`. |
| `curve25519-dalek` | `RistrettoPoint`, `Scalar` | **Use the exact version that `bulletproofs` depends on** (it may re-export it, or depend on a fork). Mismatched versions are the most likely M0 build problem. |
| `merlin` | Fiat–Shamir transcripts | Required by `bulletproofs` |
| `sha2` | Hashes for session ids, trade-list digests | |
| `rand_core` + `getrandom` | Blinding factors | WebAssembly backend configured (`docs/02`) |

**Pedersen generators:** `PedersenGens::default()` gives B (the Ristretto basepoint) and B̃ (`B_blinding`, a hash-derived point with no known discrete log relative to B). Com(v; r) = v·B + r·B̃. Commitments are perfectly hiding, computationally binding and additively homomorphic.

**Bulletproof generators:** `BulletproofGens::new(64, 16)`, enough for bit size n ≤ 64 and aggregation of up to m = 16 values. Aggregation requires m to be a power of two (pad with zero-valued commitments).

## 2. Integer encodings

From `docs/03`:
- Volumes in micro-lots: a_k, b_k ∈ [0, 50,000]; residuals in [0, 50,000].
- Prices in quarter-ticks: B_k = 4t_k + 1, A_k = 4t_k − 1; Π is one of the 16 candidates.
- Surpluses U_k = max(0, B_k − Π) and S_k = max(0, Π − A_k): small non-negative integers (≤ 30).
- All values are mapped to `Scalar::from(u64)`. Negative coefficients use `-Scalar::from(x)`.

## 3. Phase 1: commit (before any encryption)

Each firm, say A with orders a_0 … a_7:

1. Sample blindings r_k ← `Scalar::random(&mut OsRng)`.
2. Build the value list for one aggregated proof (m = 16, n = 16 bits):
   - values 0–7: a_k, blinding r_k → commitment C_k = Com(a_k; r_k);
   - values 8–15: LIMIT − a_k, blinding −r_k → commitment LIMIT·B − C_k.
3. `RangeProof::prove_multiple(&bp_gens, &pc_gens, &mut transcript, &values, &blindings, 16)` returns the proof and the 16 commitments.
4. Publish C_0 … C_7 and the proof. (The commitments for values 8–15 are recomputed by the verifier as LIMIT·B − C_k.)

**Transcript binding:** before proving, append to a fresh `Transcript::new(b"cbt/v1/commit")` the session id, the role, the public parameters hash, and LIMIT. The verifier uses an identical transcript.

**Verifier checks:** recompute the 16 commitments and verify the proof. This proves 0 ≤ a_k ≤ LIMIT for every level, **without revealing any a_k**. Real, about 1 KB, milliseconds.

Firm B does the same with b_0 … b_7 and its own blindings r'_k.

## 4. Phase 2: settle (after the trades are decrypted)

### 4.1 Public inputs

Both firms hold the same integer trade matrix T (rounded by the public rule in `docs/03` §4). They compare SHA-256(T); a mismatch aborts. They publish T (or its hash plus the matched block, per settlement policy). Π is chosen by the bitmask protocol of `docs/03` §6. The tolerance δ splits into three public parts:

```
δ_T = 0,   δ_A = δ_B = w_max · (2L − 1) · 2κ = 1800    (L = 8, κ = 2; proof in M1-market-math.md, Theorem 7)
```

### 4.2 Why three separate proofs are enough (and fully zero knowledge)

From `docs/03` §3.3, with r_k and r'_j the residuals:

```
G(Π) = G_T(Π) + G_A(Π) + G_B(Π)
G_T(Π) = Σ_{i,j} T_ij (U_i + S_j − w_ij)       public
G_A(Π) = Σ_k U_k · r_k                          Firm A's private term, ≥ 0 if r ≥ 0
G_B(Π) = Σ_j S_j · r'_j                         Firm B's private term
```

The verifier needs:
- **(i)** r_k ≥ 0 and r'_j ≥ 0 (feasibility: no firm traded more than it committed);
- **(ii)** G_T ≤ δ_T, G_A ≤ δ_A, G_B ≤ δ_B.

Then G ≤ δ, so welfare(T) ≥ W* − δ by weak duality. **Each firm can prove its own part alone**, so no joint opening is needed and nothing beyond the bitmask is revealed. (This improves on the research report's single joint gap opening.)

### 4.3 Firm A's settle proof (one aggregated Bulletproof, m = 16, n = 32)

Anyone can compute the residual commitments homomorphically from public T:

```
R_k = C_k − (Σ_j T_kj) · B                      = Com(r_k; r_k_blind)        where r_k_blind = r_k (the original blinding)
Γ_A = δ_A · B − Σ_k U_k · R_k                    = Com(δ_A − G_A; −Σ_k U_k · r_k_blind)
```

Firm A proves, in one aggregated proof:
- values 0–7: r_k with blinding r_k_blind (commitments must equal R_k);
- value 8: δ_A − G_A with blinding −Σ U_k r_k_blind (commitment must equal Γ_A);
- values 9–15: zero with fresh random blindings (padding; their commitments are published).

The verifier recomputes R_k and Γ_A from the public data and A's phase-1 commitments, checks that they equal the proof's commitments, and verifies the proof. A 32-bit range is used for all values, because aggregation needs a uniform bit size: residuals reach 400,000 > 2^16, while δ_A − G_A ∈ [0, 1800] fits easily.

Firm B does the same with S_j, C'_j and δ_B.

**Transcript:** `Transcript::new(b"cbt/v1/settle")` plus session id, role, SHA-256(T), Π and δ parts.

### 4.4 Verifier's full check list

| # | Check | Real or simulated | Cost |
|---|---|---|---|
| 1 | Firm A's commit proof: 0 ≤ a_k ≤ LIMIT | Real (Bulletproof) | ms |
| 2 | Firm B's commit proof | Real | ms |
| 3 | Trades well-formed: T ≥ 0, T = 0 where bid price < ask price, both firms' T hashes equal | Real (public) | µs |
| 4 | Dual feasibility: U(Π), S(Π) recomputed from Π; U_i + S_j ≥ w_ij for all pairs | Real (public; holds by construction, re-checked anyway) | µs |
| 5 | G_T(Π) ≤ δ_T | Real (public) | µs |
| 6 | Firm A's settle proof: residuals ≥ 0 and G_A ≤ δ_A | Real (Bulletproof + homomorphic commitments) | ms |
| 7 | Firm B's settle proof | Real | ms |
| 8 | Each ciphertext encrypts exactly the committed orders | **Simulated** (§6) | — |

**Accept** if 1–7 pass. The UI shows check 8 with its SIMULATED tag and an explanation.

**Soundness statement shown on the page:** "If checks 1–7 pass, the trades are feasible for the orders each firm committed to before trading, and no other set of trades could have produced more than δ additional total gain. This holds whatever the server did."

## 5. The clearing price Π (selection protocol)

As in `docs/03` §6, with the split thresholds:
1. The verifier publishes G_T(Π_c) for all 16 candidates.
2. Firm A publishes its bitmask m_A[c] = 1[G_A(Π_c) ≤ δ_A]; Firm B publishes m_B.
3. Π = the smallest candidate with G_T ≤ δ_T, m_A = 1 and m_B = 1. If there is none, the certificate fails (reject).
4. A lying bitmask cannot break soundness: a firm whose bit is set wrongly cannot produce the settle proof of §4.3.

## 6. The simulated check: binding ciphertexts to commitments

**What a real version needs.** A zero-knowledge proof that each firm's CKKS input ciphertexts encrypt the cumulative sums of the values inside its Pedersen commitments. That is a proof relating a lattice ciphertext to an elliptic-curve commitment, which is an open engineering and research problem. Starting points: Greco (ePrint 2024/594) for proofs of well-formed RLWE ciphertexts; Hint-MLWE (ePrint 2023/623); LaZer (CCS 2024) for a usable lattice zero-knowledge library. A cross-group equality proof is the missing piece.

**How the demo simulates it, honestly.** The visitor sets both books in the UI, so the main thread knows them (the god's-eye view). In demo mode, each firm worker also reports the plaintext it encrypted. The verifier compares those with the committed values (whose openings the firms also disclose to the demo oracle) and marks the check "SIMULATED: demo oracle". The UI says plainly that a real deployment cannot do this.

**Why the certificate makes this check much less critical.** Every check that matters is made against the **commitments**, not the ciphertexts:
- If a firm encrypts **more** volume than it committed, the trades can exceed its commitment, so its residual range proof (check 6 or 7) cannot be produced.
- If a firm encrypts **less** than it committed, its unfilled in-the-money volume rises, G_A or G_B exceeds its bound, and again its settle proof cannot be produced.
- An inconsistency that changes nothing about the optimal trades is harmless to the outcome.

So **any lie that would change the result is caught by real checks**. The simulated binding proof matters only for privacy and accountability: proving *who* lied, rather than just rejecting.

## 7. Cheat matrix (what the demo offers, and what catches it)

| Cheat (UI menu) | Implemented in | Caught by | Real? |
|---|---|---|---|
| Firm A claims more than its limit | `bt-protocol` firm, commit phase: sets a_k = 75,000 | Check 1 (commit range proof cannot be produced; the demo forces a bad proof and shows it failing) | Real |
| Firm B changes its orders after committing | Firm B encrypts different cumulative sums from its committed book | Checks 6/7 when the outcome changes; check 8 always | Real (6/7) and simulated (8) |
| Firm A lies in its bitmask | Firm A flips bits | No Π found, or check 6 fails | Real |
| The server skips a refresh | `bt-engine` hook `skip_refresh` | Checks 6/7 or 5 (garbage trades) | Real |
| The server cuts the sign approximation to save cost | `truncate_sign(n)` | Checks 5–7 | Real |
| The server steers volume to a favoured firm | `steer` | Check 5 or 6/7 (G > δ) | Real |
| The server tampers with the output | `tamper_output` | Checks 5–7 | Real |
| Too little precision (UI setting, not a cheat) | Fewer sign iterations or a smaller scale | Same checks; shown as "rejected: not precise enough" | Real |

## 8. API sketch (`bt-cert`)

```rust
pub struct CommitPhase { pub commitments: Vec<CompressedRistretto>, pub proof: RangeProof }
pub struct Openings(Secret<Vec<Scalar>>);                    // stays inside the firm's worker

pub fn commit_orders(role: Role, sid: &[u8; 32], book: &Book, limit: u64, rng) -> (CommitPhase, Openings);
pub fn verify_commit(role: Role, sid: &[u8; 32], limit: u64, c: &CommitPhase) -> Result<(), CertError>;

pub struct SettleProof { pub pad_commitments: Vec<CompressedRistretto>, pub proof: RangeProof }
pub fn gap_bitmask(role: Role, grid: &Grid, book: &Book, t: &Trades, delta_part: i128) -> Vec<bool>;
pub fn prove_settle(role: Role, sid: &[u8; 32], grid: &Grid, book: &Book, open: &Openings,
                    t: &Trades, pi: i64, delta_part: i128, rng) -> SettleProof;
pub fn verify_settle(role: Role, sid: &[u8; 32], grid: &Grid, commits: &CommitPhase,
                     t: &Trades, pi: i64, delta_part: i128, p: &SettleProof) -> Result<(), CertError>;

pub struct Verdict { pub accepted: bool, pub checks: Vec<CheckResult>, pub pi: i64, pub g_t: i128, pub welfare: i128 }
pub fn verify_all(public: &PublicRecord) -> Verdict;          // runs checks 1–7 in order
```

`CheckResult { id, label, status: Pass | Fail(reason) | Simulated(note), micros }`. The UI renders it directly, including the measured verification time.

## 9. M2 test plan

1. Commit/verify round trip for random books; a value of 50,001 fails; a tampered commitment fails.
2. Settle proofs for honest optimal trades (from `bt-market`) verify on 10,000 random books, with Π found by the bitmask protocol.
3. For every cheat in §7, the relevant check fails on every tested book.
4. Transcript separation: a proof made for session X does not verify for session Y; a proof for role A does not verify as role B.
5. Encoding edge cases: zero orders, all-zero book (rejected earlier by the UI), maximum values, negative coefficients in the homomorphic combination.
6. WebAssembly: the same tests pass under `wasm-bindgen-test` in headless Chrome (proves the curve and Bulletproofs crates work in the browser).
