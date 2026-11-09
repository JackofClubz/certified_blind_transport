"""Spike 5 (docs/04): the closed form under emulated CKKS (noise 2^-35 per op, polynomial sign) still certifies."""
import numpy as np
from common import *

SP = 1000          # micro-lots per lot
SCALE = 2 ** 19    # public normalisation bound (> 800 lots * 1000)
NOISE = 2 ** -35   # absolute error per operation, approximating scale 2^50

def emulated(a, b, n_iter, rng):
    ck = lambda x: x + rng.normal(0, NOISE, np.shape(x))
    def absf(x):
        s = x.copy()
        for _ in range(n_iter): s = ck((3 * s - s ** 3) / 2)                 # f(x) = (3x - x^3)/2
        for _ in range(3): s = ck((15 * s - 10 * s ** 3 + 3 * s ** 5) / 8)    # h(x) = (15x - 10x^3 + 3x^5)/8
        return ck(x * s)                                                         # |x| = x * sign(x)
    an, bn = a * SP / SCALE, b * SP / SCALE
    Dc = ck(LTRI @ an); Sc = ck(LTRI @ bn[::-1])
    Dp = np.concatenate([[0], Dc[:-1]]); Sp = np.concatenate([[0], Sc[:-1]])
    mn = (Dc[:, None] + Sc[None, :] - absf(Dc[:, None] - Sc[None, :])) / 2
    mx = (Dp[:, None] + Sp[None, :] + absf(Dp[:, None] - Sp[None, :])) / 2
    z = mn - mx; t = (z + absf(z)) / 2
    return t[:, ::-1] * MASK * SCALE / SP

rng = np.random.default_rng(5)
books = PRESETS[:1] + random_books_wide(rng, 1000)
print("sign iters  ~depth per |x| layer  certified(<0.1%)  certified(<0.5%)  worst gap/welfare")
for n in [10, 16, 22, 28]:
    c01 = c05 = 0; worst = 0.0
    for a, b in books:
        T = emulated(a, b, n, rng)
        Ti = np.maximum(np.floor(np.maximum(T, 0) * SP - 2) / SP, 0)          # round down with a 2 micro-lot margin
        feasible = (Ti.sum(1) <= a + 1e-12).all() and (Ti.sum(0) <= b + 1e-12).all()
        g = min_gap(Ti, a, b); W = max(welfare(Ti), 1e-9); r = g / W if W > 1e-9 else 0.0
        c01 += feasible and r < 1e-3; c05 += feasible and r < 5e-3; worst = max(worst, r)
    print(f"{n:<11} {2 * n + 3 * 3 + 1:<21} {c01}/{len(books):<14} {c05}/{len(books):<14} {worst:.2e}")
