# 11. Testing, security review and release

## 1. Test pyramid

| Layer | Tool | Where | What |
|---|---|---|---|
| Unit | `cargo test` | every crate | Arithmetic, NTT, encoder, key switching, protocol shares, commitments, rounding |
| Property | `proptest` | `bt-arith`, `bt-market`, `bt-cert`, `bt-engine` | Random books: optimality, certificate, cheats rejected; random polynomials: NTT laws |
| Statistical | `cargo test --release -- --ignored` | `bt-ckks`, `bt-mhe` | Noise and error distributions against bounds; masking and smudging statistics |
| Cross-implementation | Python spikes; optional Lattigo vectors | `spikes/`, `test-vectors/` | Market maths against scipy's LP; encoder and NTT against Lattigo |
| End-to-end native | `bt-cli run --parties all` | `bt-cli` | Full protocol in one process, 200 random books, with and without each cheat |
| WebAssembly | `wasm-bindgen-test` (headless Chrome) | `bt-wasm` | The same vectors pass in the browser engine |
| Browser e2e | Playwright | `web/demo/tests` | `docs/09` §8 and `docs/10` §10 |
| Fuzz | `cargo-fuzz` | `bt-protocol` | Every message decoder and state machine with arbitrary bytes: no panics, only clean aborts |

**Test vectors.** `bt-cli gen-vectors` writes JSON: books, expected trades, Π, G parts, commitments with fixed test seeds, and CKKS encode/NTT vectors. Native and wasm tests read the same files. Regenerate only intentionally and review the diff.

## 2. What "done" means for correctness

- **Market maths:** on 10,000 random books, the closed form is certified with G = 0 (exact integers), and every tampering is rejected.
- **CKKS:** all precision targets in `docs/05` §8 met at `TOY-12`.
- **Threshold layer:** every test in `docs/06` §9 passes; refresh statistics show no correlation with the message.
- **Engine:** at least 99.9% certification at ρ = 0.5% on 10,000 books in f64 emulation, and 100% on 200 books under real two-party CKKS.
- **Certificate:** every cheat in `docs/08` §7 is rejected on every tested book.
- **Browser:** all Playwright scenarios pass on desktop Chrome, Firefox and Safari, and on a phone viewport.

## 3. Security review checklist (before showing anyone)

### Secrets and randomness
- [ ] All secret material (key shares, ephemeral u_i, masks, commitment blindings) is wrapped in `Secret<T>` (zeroise on drop; `Debug` prints `<secret>`).
- [ ] No secret ever leaves its worker except as protocol output (shares, proofs). Test: instrument the bus to fail if any message decodes to a known secret.
- [ ] RNG: `getrandom` (browser `crypto.getRandomValues`) everywhere. Deterministic seeds only under `#[cfg(test)]`. `Math.random` banned in `web/demo/src` by lint.

### Protocol
- [ ] Every transcript (Bulletproofs, CRS hash, refresh labels) includes the session id, role and phase (domain separation).
- [ ] Every message header is bound to the session params hash; mismatches abort.
- [ ] CRS seeds use commit-then-reveal; reveals are checked against commitments.
- [ ] No decryption share is ever sent to the server (Playwright test 2), and only after both firms reach `Decrypting`.
- [ ] Refresh happens only at level 1 or above (masking requirement), enforced in code.

### CKKS-specific
- [ ] Smudging uses a **worst-case** noise bound (Guo et al. 2024 broke average-case estimates).
- [ ] The page states that toy parameters and the small smudging λ are insecure (`docs/06` §5.1).
- [ ] Decoded outputs are only ever the masked trade block (the mask zeroes everything else before decryption).

### Web and supply chain
- [ ] CSP as in `docs/01` §6; no third-party requests (self-hosted fonts).
- [ ] `cargo audit` and `cargo deny` in CI (advisories, licences, duplicate crypto crates).
- [ ] pnpm lockfile frozen; `pnpm audit` reviewed.
- [ ] GitHub Actions pinned by commit SHA; release provenance attached.
- [ ] Reproducible build verified by a second, clean build in CI.

## 4. Benchmarks to record (fill in during M3–M6)

| Measurement | Native (laptop) | WebAssembly (Chrome, desktop) | WebAssembly (phone) |
|---|---|---|---|
| NTT, N = 4096, one limb | | | |
| ct × ct + relinearise + rescale (level 12) | | | |
| Rescale | | | |
| Collective refresh (each firm's share) | | | |
| Full key generation (each firm) | | | |
| Full engine run (v1, chosen sign spec) | | | |
| Commit proof: prove / verify | | | |
| Settle proof: prove / verify | | | |
| Total bytes on the wire per run | | | |

These numbers are also your talk material: "the full protocol runs in X seconds in a browser tab, with Y MB of traffic".

## 5. Threat model document (write in M7: `docs/THREAT-MODEL.md`)

Outline:
1. **Parties and trust:** firms semi-honest for privacy (v1); server semi-honest for privacy and malicious for integrity; verifier public.
2. **Assets:** order books and residuals; key shares.
3. **Guarantees:** integrity of the outcome (certificate, holds against malicious parties); confidentiality from the server (CKKS IND-CPA plus refresh masking); confidentiality between firms (limited to trades, Π and bitmasks).
4. **Non-guarantees and limitations:** toy parameters; small smudging λ; no proof binding ciphertexts to commitments (simulated); no protection against a firm deviating in key-generation or refresh sub-protocols (v2: lattice zero-knowledge proofs); unauthenticated firm-to-firm key exchange; side channels (non-constant-time sampler).
5. **Leakage table:** from `docs/03` §7.
6. **Out of scope:** network-level anonymity, denial of service, economic manipulation of order placement.

## 6. Claims discipline (what the website and talks may say)

| Allowed | Not allowed |
|---|---|
| "Real CKKS in Rust/WebAssembly with toy, insecure parameters" | "Secure encrypted trading" |
| "The certificate proves optimality against the committed orders, whatever the server did" | "Fully verifiable FHE" |
| "Ciphertext-to-commitment binding is simulated; it is an open problem" | Hiding or omitting the simulated check |
| "Novel combination, to our knowledge: OT duality certificates for FHE-computed matching" (after the literature sweep in §8) | "First ever" without a completed literature check |
| Measured timings and sizes | Modelled numbers presented as measured |

## 7. Release process

1. Version bump (SemVer). `CHANGELOG.md` entry.
2. `git tag vX.Y.Z` triggers `release.yml` (`docs/02` §4.2): container build, `SHA256SUMS`, GitHub Release, provenance.
3. Verify locally: rebuild in the container and compare hashes.
4. Update the site repo's `demo.lock` (version + SHA-256) and redeploy the site.
5. Announce with the hash.

## 8. Before claiming novelty

The research report's novelty finding ("no published FHE Sinkhorn, no zero-knowledge OT certificates") came from partial searches. Before any public "first" claim:
- sweep DBLP, Google Scholar and IACR ePrint for: "optimal transport" with "homomorphic", "secure matching" with "certificate", "verifiable" with "double auction", and "dark pool" with "optimality";
- check the 2026 "SoK: Privacy-Preserving Order Matching" (arXiv 2610.12535) reference list;
- ask the external reviewer.

Frame the contribution as an extension of the "prove optimality, not computation" paradigm (de Hoogh–Schoenmakers–Veeningen 2016; Otti, USENIX Security 2022; Lorünser et al., CCSW 2022) to FHE-computed transport, with the uniform-price certificate and the per-firm zero-knowledge gap split as the specific new pieces.

## 9. After v1

1. v2 engine: certified encrypted Sinkhorn on Kronecker kernels (`docs/07` §6), with a "dataset distance" tab.
2. Remote GPU server (M9): the same engine natively, with a CUDA NTT backend, over WebSocket.
3. Lattice zero-knowledge proofs for key-generation and refresh shares, and for ciphertext-to-commitment binding (the research track).
4. An exact integer output stage (BFV) to remove the smudging limitation.
5. Private Π selection (private set intersection over the 16 candidates).
6. Batching: 32 instruments per run in one ciphertext.
