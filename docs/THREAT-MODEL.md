# Threat model (v1)

Status: living document for the toy in-browser demo. Parameters are **insecure**.

## 1. Parties and trust

| Party | Privacy assumption (v1) | Integrity |
|---|---|---|
| Firm A, Firm B | Semi-honest for key-generation and refresh | Malicious for the *output*: the certificate rejects bad trades |
| Server | Semi-honest for privacy (sees only ciphertexts) | Untrusted for integrity |
| Verifier / public | Sees the public record only | Checks 1–7 are real; check 8 is simulated |

## 2. Assets

- Order books and residuals (micro-lots per price level)
- Secret-key shares `s_A`, `s_B`
- Commitment openings (Pedersen blindings)
- Refresh masks and decryption smudging coins

## 3. Guarantees

- **Integrity of the outcome.** If checks 1–7 pass, the published trades are feasible for the *committed* books and welfare is within `δ` of optimal, whatever the server computed. Proof: weak duality plus uniform-price dual feasibility (`docs/M1-market-math.md`).
- **Confidentiality from the server.** CKKS IND-CPA on toy parameters (not a real security level) plus refresh masking.
- **Confidentiality between firms.** Each learns the trades, `Π`, and the other firm's bitmask; not the other book.

## 4. Non-guarantees

- Toy parameters (`TOY-12`, N = 4096, log QP ≈ 840): **no cryptographic security**.
- Smudging λ ≈ 11 bits at toy parameters (`docs/plan/06` §5.1).
- Ciphertext-to-commitment binding is **simulated** (demo oracle).
- No proofs of well-formedness of key-generation or refresh shares.
- Firm-to-firm X25519 is unauthenticated (no PKI in the demo).
- Gaussian sampler is not constant-time.
- Side channels, DoS, network anonymity, and economic order manipulation are out of scope.

## 5. Leakage

See `docs/plan/03` §7. The bitmask leaks a coarse in-the-money unfilled boundary. Trades are inherent to settlement.

## 6. Claims discipline

Allowed: “real CKKS in Rust/Wasm with toy, insecure parameters”; “the certificate proves optimality against committed orders, whatever the server did”; “binding proof is simulated”.

Not allowed: “secure encrypted trading”; “fully verifiable FHE”; “first” without the literature sweep in `docs/plan/11` §8.
