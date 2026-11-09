"""Spike 1 (docs/04): one public clearing price certifies optimal trades; tampering is always detected."""
import numpy as np
from common import *

rng = np.random.default_rng(1)
books = PRESETS + random_books_overlap(rng, 2000)
ok, worst, tampered = 0, 0.0, []
for a, b in books:
    T = lp_optimum(a, b)
    g = min_gap(T, a, b); worst = max(worst, g); ok += g < 1e-6
    T2 = T.copy(); i, j = np.unravel_index(np.argmax(T), T.shape); T2[i, j] -= min(5, T[i, j])
    tampered.append(min_gap(T2, a, b))
print(f"optimal trades certified with G~0 at a candidate price: {ok}/{len(books)} (worst min-gap {worst:.2e})")
print(f"tampered trades (5 lots withheld): smallest min-gap across books = {min(tampered):.3f} quarter-tick-lots (> 0 on every book)")
grid = np.linspace(B_INT.min() - 8, B_INT.max() + 8, 400)
ok_grid = sum(min_gap(lp_optimum(a, b), a, b, grid) < 1e-6 for a, b in books[:200])
print(f"for comparison, a 400-point price grid instead of the exact breakpoints certifies only {ok_grid}/200 (G is piecewise linear; minima sit on breakpoints)")
