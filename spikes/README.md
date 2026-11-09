# Spikes

Experiments behind the design decisions in `docs/04-spike-findings.md`.

Requirements: Python 3.10+, `numpy`, `scipy` (`pip install numpy scipy`).

| Script | Spike | Runtime |
|---|---|---|
| `certificate_uniform_price.py` | 1: one clearing price certifies optimal trades | ~1 min |
| `sinkhorn_scaling_range.py` | 2: scaling-form Sinkhorn range vs accuracy | ~1 min |
| `closed_form_1d_ot.py` | 4: closed-form 1-D OT equals the LP optimum | ~1 min |
| `closed_form_fhe_emulation.py` | 5: closed form under emulated CKKS noise and polynomial sign | ~2 min |

Spike 3 (bounded Sinkhorn reformulations) was exploratory; its findings are recorded in `docs/04` and the emulator is easy to rebuild from that description when the v2 track starts.

Shared helpers (market grid, LP oracle, certificate gap, closed form) are in `common.py`.
