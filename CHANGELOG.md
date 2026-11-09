# Changelog

## 0.1.0

- Workspace skeleton (M0): pinned Rust 1.90, wasm target, CI, Docker build image, Vite demo.
- `bt-arith` RNS polynomials and approximate basis conversion.
- `bt-cert` Pedersen/Bulletproofs commit and settle proofs.
- `bt-ckks` toy RNS-CKKS (insecure TOY-12 / toy_test parameters).
- `bt-mhe` two-party CRS, CKG, decrypt shares, refresh.
- `bt-engine` closed-form 1-D OT circuit (exact and polynomial-sign f64 paths).
- `bt-protocol` in-process session with firm-to-firm AEAD for decryption shares.
- `bt-wasm` `info()` and `run_demo()`; `bt-cli` native harness.
- Threat model: `docs/THREAT-MODEL.md`.
