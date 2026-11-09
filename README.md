# certified-blind-transport

Two firms match orders on encrypted books. The result comes with a zero-knowledge certificate that the matching was optimal.

**Status:** v1 workspace is built. M1 market maths and M3 NTT arithmetic are complete and tested. CKKS, two-party threshold, the certificate, the matching engine, the protocol runner, and a Vite demo are in place. Toy parameters (`TOY-12`) are **insecure** and labelled as such everywhere.

## Layout

| Path | Contents |
|---|---|
| `crates/bt-arith` | Modular arithmetic, NTT, automorphisms, RNS polynomials |
| `crates/bt-market` | 1-D OT, uniform-price certificate (integers) |
| `crates/bt-ckks` | Toy RNS-CKKS |
| `crates/bt-mhe` | Two-party threshold layer |
| `crates/bt-cert` | Pedersen + Bulletproofs commit/settle |
| `crates/bt-engine` | Closed-form matching circuit |
| `crates/bt-protocol` | Messages and in-process session runner |
| `crates/bt-wasm` | `info()`, `run_demo()`, worker handles |
| `crates/bt-cli` | Native harness (`run`, `gen-vectors`) |
| `web/demo` | Vite + TypeScript UI |
| `docs/` | Specs, plan, [threat model](docs/THREAT-MODEL.md) |

## Run it

```bash
rustup target add wasm32-unknown-unknown
cargo test --workspace --release --exclude bt-wasm
cargo run -p bt-cli --release -- run
cargo run -p bt-market --example preset
cargo build -p bt-wasm --release --target wasm32-unknown-unknown
```

Parameters are toy. Do not claim production security. Ciphertext-to-commitment binding is simulated (demo oracle). See `docs/THREAT-MODEL.md`.

## Licence

Apache-2.0 OR MIT.
