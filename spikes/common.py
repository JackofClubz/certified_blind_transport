"""Shared helpers for the Certified Blind Transport spikes (docs/04).
Market: L price levels, tick 0.10, top price 100.40. Bid level i compatible with ask level j iff i <= j."""
import numpy as np
from scipy.optimize import linprog

L = 8; P0 = 100.40; TICK = 0.10
def price(i): return P0 - i * TICK
TICKS = np.array([round(price(i) / TICK) for i in range(L)], dtype=float)
B_INT = 4 * TICKS + 1          # buyer value, quarter-ticks (docs/03 §1.3)
A_INT = 4 * TICKS - 1          # seller cost, quarter-ticks
CANDIDATES = np.concatenate([B_INT, A_INT])   # the 2L public clearing-price candidates
PRESETS = [(np.array([0, 5, 10, 25, 30, 20, 15, 10.]), np.array([10, 15, 25, 30, 20, 8, 0, 0.])),
           (np.array([0, 0, 0, 0, 20, 30, 25, 15.]), np.array([15, 20, 25, 30, 10, 0, 0, 0.]))]

def welfare(T):
    return sum((B_INT[i] - A_INT[j]) * T[i, j] for i in range(L) for j in range(L) if i <= j)

def gap(T, a, b, Pi):
    """G(Pi) = sum U_i a_i + sum S_j b_j - W(T) with U_i = max(0, B_i - Pi), S_j = max(0, Pi - A_j)."""
    return -welfare(T) + np.maximum(0, B_INT - Pi) @ a + np.maximum(0, Pi - A_INT) @ b

def min_gap(T, a, b, cands=CANDIDATES):
    return min(gap(T, a, b, p) for p in cands)

def lp_optimum(a, b):
    """Exact welfare-maximising trades (scipy HiGHS)."""
    pairs = [(i, j) for i in range(L) for j in range(L) if i <= j]
    c = [-(B_INT[i] - A_INT[j]) for i, j in pairs]
    A_ub, b_ub = [], []
    for i in range(L): A_ub.append([1 if p[0] == i else 0 for p in pairs]); b_ub.append(a[i])
    for j in range(L): A_ub.append([1 if p[1] == j else 0 for p in pairs]); b_ub.append(b[j])
    r = linprog(c, A_ub=A_ub, b_ub=b_ub, bounds=(0, None), method="highs")
    T = np.zeros((L, L))
    for k, (i, j) in enumerate(pairs): T[i, j] = r.x[k]
    return T

def random_books_overlap(rng, n):
    """Books with a guaranteed overlap region (used in spikes 1-2)."""
    out = []
    for _ in range(n):
        mid = rng.integers(2, 6)
        a = np.array([rng.integers(5, 41) if i >= mid - 1 else 0 for i in range(L)], float)
        b = np.array([rng.integers(5, 41) if i <= mid + 1 else 0 for i in range(L)], float)
        out.append((a, b))
    return out

def random_books_wide(rng, n):
    """Sparser books including one-sided and non-crossing cases (used in spikes 4-5)."""
    out = []
    for _ in range(n):
        mid = rng.integers(1, 7)
        a = np.array([rng.integers(0, 51) if i >= mid - rng.integers(0, 3) else 0 for i in range(L)], float)
        b = np.array([rng.integers(0, 51) if i <= mid + rng.integers(0, 3) else 0 for i in range(L)], float)
        out.append((a, b))
    return out

MASK = np.array([[1.0 if i <= j else 0.0 for j in range(L)] for i in range(L)])
LTRI = np.tril(np.ones((L, L)))

def monotone_coupling(a, b):
    """Closed-form 1-D OT (docs/03 §2.1): overlap of cumulative demand and supply intervals, masked."""
    Dc = LTRI @ a; Sc = LTRI @ b[::-1]
    Dp = np.concatenate([[0], Dc[:-1]]); Sp = np.concatenate([[0], Sc[:-1]])
    Tr = np.maximum(0, np.minimum(Dc[:, None], Sc[None, :]) - np.maximum(Dp[:, None], Sp[None, :]))
    return Tr[:, ::-1] * MASK
