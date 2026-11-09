"""Spike 4 (docs/04): the masked monotone coupling (closed-form 1-D OT) equals the LP optimum and certifies with gap 0."""
import numpy as np
from common import *

rng = np.random.default_rng(11)
books = PRESETS + random_books_wide(rng, 5000)
ok = cert = 0; worst = 0.0
for a, b in books:
    T = monotone_coupling(a, b); W = welfare(T); Wopt = welfare(lp_optimum(a, b))
    feasible = (T.sum(1) <= a + 1e-9).all() and (T.sum(0) <= b + 1e-9).all()
    ok += feasible and abs(W - Wopt) <= 1e-6 * max(1, Wopt); worst = max(worst, abs(W - Wopt))
    cert += min_gap(T, a, b) <= 1e-6
print(f"{len(books)} books: feasible and welfare-optimal {ok}; worst welfare difference {worst:.2e}; certificate gap 0 in {cert}")
