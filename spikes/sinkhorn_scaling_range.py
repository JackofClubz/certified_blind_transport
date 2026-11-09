"""Spike 2 (docs/04): scaling-form Sinkhorn needs tiny eps for certificate-grade trades, where values explode beyond CKKS range."""
import numpy as np
from common import *

def costs():
    C = np.zeros((L + 1, L + 1))
    for i in range(L):
        for j in range(L): C[i, j] = -(B_INT[i] - A_INT[j]) / (4 * L) if i <= j else 4.0
    return C

def sinkhorn_scaling(a, b, eps, iters):
    C = costs(); K = np.exp(-(C - C.min()) / eps)
    al = np.append(a, b.sum()); be = np.append(b, a.sum()); u = np.ones(L + 1); v = np.ones(L + 1)
    umax, umin = 0.0, np.inf
    for _ in range(iters):
        u = al / (K @ v); v = be / (K.T @ u)
        umax = max(umax, u.max(), v.max()); umin = min(umin, u[u > 0].min(), v[v > 0].min())
    P = u[:, None] * K * v[None, :]
    return P[:L, :L], umax, umin, K[K > 0].min()

rng = np.random.default_rng(3)
books = PRESETS[:1] + random_books_overlap(rng, 300)
opt = [welfare(lp_optimum(a, b)) for a, b in books]
print("eps    median gap/welfare   p95     worst   max|u,v|    min u,v     min K")
for eps in [0.2, 0.1, 0.05, 0.03, 0.02]:
    rel = []; um = 0; un = np.inf; km = 1
    for (a, b), W in zip(books, opt):
        T, umax, umin, kmin = sinkhorn_scaling(a, b, eps, 60); um = max(um, umax); un = min(un, umin); km = min(km, kmin)
        sh = T * (1 - 1e-3)
        if (sh.sum(1) - a).max() > 1e-9 or (sh.sum(0) - b).max() > 1e-9: rel.append(np.inf); continue
        rel.append(min_gap(sh, a, b) / max(W, 1e-9))
    rel = np.array(rel)
    print(f"{eps:<6} {np.median(rel):.4f}               {np.percentile(rel, 95):.4f}  {rel.max():.4f}  {um:.2e}    {un:.2e}    {km:.1e}")
