# Benchmarks (v0.1, fill in on your hardware)

Toy parameters. These are **not** security claims.

| Measurement | Native (this repo, `--release`) | Wasm | Phone |
|---|---|---|---|
| `bt-arith` NTT tests (includes N=4096) | < 0.5 s for the suite | not measured | — |
| Encode/decrypt (`toy_test`, N=128) | ~20 ms | not measured | — |
| Two-party encrypt/decrypt | ~20 ms | not measured | — |
| Commit + settle Bulletproofs (honest preset) | ~2 s | not measured | — |
| Full `bt-protocol` honest run | ~2 s | `run_demo` in worker | — |
| Closed-form f64 circuit vs coupling | < 1 ms | — | — |

Wire sizes at TOY-12 remain as estimated in `docs/plan/01` (~0.85 MB ciphertext). The in-process runner currently encrypts `toy_test` keys and logs message counts, not full TOY-12 traffic.
