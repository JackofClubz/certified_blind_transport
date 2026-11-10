# Threat model (v1)

Status: living document for the toy in-browser demo. Parameters are **insecure**.

## 1. Parties and trust

| Party | Privacy assumption (v1) | Integrity |
|---|---|---|
| Firm A, Firm B | Semi-honest for key-generation and refresh | Malicious for the *output*: the certificate rejects bad trades |
| Server | Semi-honest for privacy: the matching entrypoint takes only ciphertexts. The in-process runner still holds every party's state in one address space; that is a test harness, not a network boundary. | Untrusted for integrity |
| Verifier / public | Sees the public record only | Checks 1–7 are real; check 8 is simulated |

## 2. Assets

- Order books and residuals (micro-lots per price level)
- Secret-key shares `s_A`, `s_B`
- Commitment openings (Pedersen blindings)
- Refresh masks and decryption smudging coins

## 3. Guarantees

- **Integrity of the outcome.** If checks 1–7 pass, the published trades are feasible for the *committed* books and welfare is within `δ` of optimal, whatever the server computed. Proof: weak duality plus uniform-price dual feasibility (`docs/M1-market-math.md`).
- **Confidentiality from the server.** Matching is `eval_matching` on ciphertexts. CKKS is IND-CPA only at a real parameter set; this demo uses toy parameters. Relin is a toy Q-only switching key (no `P` gadget). Refresh is not on the matching critical path yet.
- **Confidentiality between firms.** Each learns the trades, `Π`, and the other firm's bitmask; not the other book.

## 4. Non-guarantees

- Toy parameters (`Params::toy_test` in the demo runner, `TOY-12` labelled on the page): **no cryptographic security**.
- Smudging λ is the value shown on the page (11 bits at the documented toy target; `docs/plan/06` §5.1).
- Ciphertext-to-commitment binding is **simulated** (demo oracle).
- Homomorphic multiply uses a toy key-switch (gadget = `s'`, not `P·s'`). Do not cite this as Han–Ki hybrid KS.
- The matching circuit is on ciphertexts, but the shallow `|x|` (`n_cubic = 1`) misses Theorem 7 (`κ = 2`). Honest runs abort on the gap/settle checks. That is a completeness failure, not a soundness hole. Hitting `κ` needs more comparison depth plus refresh, a larger `Δ`, or a coarser unit.
- No proofs of well-formedness of key-generation or refresh shares.
- Firm-to-firm X25519 is unauthenticated (no PKI in the demo).
- Gaussian sampler is not constant-time.
- Side channels, DoS, network anonymity, and economic order manipulation are out of scope.

## 5. Leakage

See `docs/plan/03` §7. The bitmask leaks a coarse in-the-money unfilled boundary. Trades are inherent to settlement.

## 6. Claims discipline

Allowed: “real CKKS in Rust/Wasm with toy, insecure parameters”; “matching is evaluated on ciphertexts in this build”; “the certificate proves optimality against committed orders, whatever the server did”; “binding proof is simulated”.

Not allowed until the flags say so: “the server matches without seeing the books” as a *security* claim about the in-process demo (one process still holds the books while encrypting). The *matching function* does not take books; the page must follow `HonestyReport` flags.

Not allowed: “secure encrypted trading”; “fully verifiable FHE”; “first” without the literature sweep in `docs/plan/11` §8.
