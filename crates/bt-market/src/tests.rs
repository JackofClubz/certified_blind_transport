//! Property tests for `bt-market`. Each test names the theorem in `docs/M1-market-math.md` it checks.

use super::*;
use bt_arith::SplitMix64;

const G: Grid = Grid::DEMO;
const SP: u64 = 1000; // micro-lots per lot
const LIMIT: u64 = 50 * SP;

fn lots(v: &[u64]) -> Book {
    v.iter().map(|x| x * SP).collect()
}

/// Random books of several shapes: overlapping, sparse, one-sided, non-crossing, empty.
fn random_book_pair(rng: &mut SplitMix64) -> (Book, Book) {
    let l = G.levels;
    let shape = rng.below(5);
    let mut a = vec![0u64; l];
    let mut b = vec![0u64; l];
    match shape {
        0 => {
            // overlapping with micro-lot granularity
            let mid = 1 + rng.below(6) as usize;
            for i in 0..l {
                if i + 1 >= mid {
                    a[i] = rng.below(LIMIT + 1);
                }
            }
            for j in 0..l {
                if j <= mid + 1 {
                    b[j] = rng.below(LIMIT + 1);
                }
            }
        }
        1 => {
            // sparse, whole lots
            for k in 0..l {
                if rng.below(3) == 0 {
                    a[k] = rng.below(51) * SP;
                }
                if rng.below(3) == 0 {
                    b[k] = rng.below(51) * SP;
                }
            }
        }
        2 => {
            // one-sided
            for k in 0..l {
                a[k] = rng.below(LIMIT + 1);
            }
        }
        3 => {
            // non-crossing: bids only on low prices, asks only on high prices
            let cut = 1 + rng.below(6) as usize;
            for k in 0..l {
                if k >= cut {
                    a[k] = rng.below(LIMIT + 1);
                } else {
                    b[k] = rng.below(LIMIT + 1);
                }
            }
        }
        _ => {
            // fully random including zeros
            for k in 0..l {
                a[k] = rng.below(LIMIT + 1) * rng.below(2);
                b[k] = rng.below(LIMIT + 1) * rng.below(2);
            }
        }
    }
    (a, b)
}

const N_CASES: usize = 20_000;

#[test]
fn presets_match_the_demo() {
    let (a, b) = (
        lots(&[0, 5, 10, 25, 30, 20, 15, 10]),
        lots(&[10, 15, 25, 30, 20, 8, 0, 0]),
    );
    let t = monotone_coupling(&G, &a, &b);
    let traded: u64 = t.iter().flatten().sum();
    assert_eq!(traded, 40 * SP, "the demo preset clears 40 lots");
    let pi = equilibrium_price(&G, &a, &b);
    assert_eq!(gap(&G, &a, &b, &t, pi), 0);
}

#[test]
fn coupling_is_feasible_masked_and_a_staircase() {
    // Lemma 2 (feasibility) and Lemma 6 (at most 2L-1 non-zero cells)
    let mut rng = SplitMix64(1);
    for _ in 0..N_CASES {
        let (a, b) = random_book_pair(&mut rng);
        let t = monotone_coupling(&G, &a, &b);
        let (ra, rb) = residuals(&a, &b, &t);
        assert!(ra.iter().all(|&r| r >= 0) && rb.iter().all(|&r| r >= 0));
        for i in 0..G.levels {
            for j in 0..i {
                assert_eq!(t[i][j], 0);
            }
        }
        let nnz = t.iter().flatten().filter(|&&v| v > 0).count();
        assert!(nnz < 2 * G.levels, "nnz = {nnz} exceeds 2L-1");
    }
}

#[test]
fn coupling_attains_the_quantile_bound() {
    // Theorem 1: W(T*) equals the independent bound Σ_u (β(u) - α(u))^+
    let mut rng = SplitMix64(2);
    for _ in 0..N_CASES {
        let (a, b) = random_book_pair(&mut rng);
        let t = monotone_coupling(&G, &a, &b);
        assert_eq!(welfare(&G, &t), quantile_welfare_bound(&G, &a, &b));
    }
}

#[test]
fn gap_identity_and_decomposition() {
    // Theorem 2 (definition) equals Theorem 4 (decomposition) for arbitrary T and Π
    let mut rng = SplitMix64(3);
    for _ in 0..N_CASES / 4 {
        let (a, b) = random_book_pair(&mut rng);
        let t = monotone_coupling(&G, &a, &b);
        for &p in &G.candidates() {
            assert_eq!(gap(&G, &a, &b, &t, p), gap_parts(&G, &a, &b, &t, p).total());
            let parts = gap_parts(&G, &a, &b, &t, p);
            assert!(
                parts.g_t >= 0 && parts.g_a >= 0 && parts.g_b >= 0,
                "each part non-negative for feasible T"
            );
        }
    }
}

#[test]
fn every_candidate_price_is_dual_feasible() {
    // Lemma 3
    for p in G
        .candidates()
        .into_iter()
        .chain([0, i64::from(u16::MAX), 4000, 4100])
    {
        assert!(dual_feasible(&G, &surplus(&G, p)), "Π = {p}");
    }
}

#[test]
fn equilibrium_price_certifies_with_zero_gap() {
    // Theorem 5 (completeness) and, through weak duality, optimality of T*
    let mut rng = SplitMix64(4);
    for _ in 0..N_CASES {
        let (a, b) = random_book_pair(&mut rng);
        let t = monotone_coupling(&G, &a, &b);
        let pi = equilibrium_price(&G, &a, &b);
        assert!(G.candidates().contains(&pi));
        assert_eq!(gap(&G, &a, &b, &t, pi), 0, "a={a:?} b={b:?} Π={pi}");
        // and the brute-force minimum over candidates agrees
        let best = G
            .candidates()
            .iter()
            .map(|&p| gap(&G, &a, &b, &t, p))
            .min()
            .unwrap();
        assert_eq!(best, 0);
    }
}

#[test]
fn any_suboptimal_plan_has_positive_gap_at_every_price() {
    // Corollary of Theorem 2: removing volume or moving it to a worse pair is always detected
    let mut rng = SplitMix64(5);
    let mut tested = 0;
    for _ in 0..N_CASES {
        let (a, b) = random_book_pair(&mut rng);
        let t = monotone_coupling(&G, &a, &b);
        let cells: Vec<(usize, usize)> = (0..G.levels)
            .flat_map(|i| (0..G.levels).map(move |j| (i, j)))
            .filter(|&(i, j)| t[i][j] > 0)
            .collect();
        if cells.is_empty() {
            continue;
        }
        tested += 1;
        // (1) withhold one micro-lot
        let (i, j) = cells[rng.below(cells.len() as u64) as usize];
        let mut t1 = t.clone();
        t1[i][j] -= 1;
        let min1 = G
            .candidates()
            .iter()
            .map(|&p| gap(&G, &a, &b, &t1, p))
            .min()
            .unwrap();
        assert!(
            min1 >= 2,
            "withholding 1 micro-lot must cost at least w_min = 2, got {min1}"
        );
        // (2) re-route volume between two traded cells (keeps every marginal), if it lowers welfare
        if cells.len() >= 2 {
            let (i1, j1) = cells[0];
            let (i2, j2) = cells[cells.len() - 1];
            if i1 != i2 && j1 != j2 && G.compatible(i1, j2) && G.compatible(i2, j1) {
                let m = t[i1][j1].min(t[i2][j2]).min(1000);
                let mut t2 = t.clone();
                t2[i1][j1] -= m;
                t2[i2][j2] -= m;
                t2[i1][j2] += m;
                t2[i2][j1] += m;
                let dw = welfare(&G, &t) - welfare(&G, &t2);
                let min2 = G
                    .candidates()
                    .iter()
                    .map(|&p| gap(&G, &a, &b, &t2, p))
                    .min()
                    .unwrap();
                assert!(
                    dw >= 0 && min2 >= dw,
                    "gap must be at least the welfare lost"
                );
            }
        }
    }
    assert!(tested > N_CASES / 2);
}

#[test]
fn rounding_lemma_and_protocol_completeness() {
    // Theorem 7: if |T̂ - T*| < κ entrywise, the rounded trades are feasible, G_T = 0 at the
    // equilibrium price, the private parts are within δ_A, δ_B, and the bitmask protocol finds a Π.
    let mut rng = SplitMix64(6);
    let kappa = 2u64;
    let thr = thresholds(&G, kappa);
    for _ in 0..N_CASES {
        let (a, b) = random_book_pair(&mut rng);
        let t = monotone_coupling(&G, &a, &b);
        let t_hat: Vec<Vec<f64>> = t
            .iter()
            .map(|row| {
                row.iter()
                    .map(|&v| {
                        let noise = (rng.next() as f64 / u64::MAX as f64 * 2.0 - 1.0)
                            * (kappa as f64 - 1e-6);
                        v as f64 + noise
                    })
                    .collect()
            })
            .collect();
        let ti = round_trades(&G, &t_hat, kappa);
        let pi_eq = equilibrium_price(&G, &a, &b);
        let parts = certify_plain(&G, &a, &b, &ti, pi_eq, &thr)
            .unwrap_or_else(|e| panic!("{e} for a={a:?} b={b:?}"));
        assert_eq!(parts.g_t, 0);
        let ma = bitmask_a(&G, &a, &ti, &thr);
        let mb = bitmask_b(&G, &b, &ti, &thr);
        let pi = choose_pi(&G, &ti, &ma, &mb, &thr).expect("protocol finds a clearing price");
        certify_plain(&G, &a, &b, &ti, pi, &thr).expect("chosen Π certifies");
    }
}

#[test]
fn soundness_bound_holds() {
    // Theorem 6: any (T, Π) passing the checks has W(T) >= W* - (δ_T + δ_A + δ_B)
    let mut rng = SplitMix64(7);
    let thr = thresholds(&G, 2);
    let delta = thr.t + thr.a + thr.b;
    for _ in 0..N_CASES / 2 {
        let (a, b) = random_book_pair(&mut rng);
        let opt = welfare(&G, &monotone_coupling(&G, &a, &b));
        // random feasible plan: scale the optimum down and perturb within the mask
        let mut t = monotone_coupling(&G, &a, &b);
        for row in t.iter_mut() {
            for v in row.iter_mut() {
                *v -= rng.below(*v / 50 + 2).min(*v);
            }
        }
        for &p in &G.candidates() {
            if certify_plain(&G, &a, &b, &t, p, &thr).is_ok() {
                assert!(welfare(&G, &t) >= opt - delta);
            }
        }
    }
}

#[test]
fn cheats_are_rejected() {
    let (a, b) = (
        lots(&[0, 5, 10, 25, 30, 20, 15, 10]),
        lots(&[10, 15, 25, 30, 20, 8, 0, 0]),
    );
    let thr = thresholds(&G, 2);
    let t = monotone_coupling(&G, &a, &b);
    let pi = equilibrium_price(&G, &a, &b);
    // over-fill Firm A at its best level
    let mut t1 = t.clone();
    t1[1][4] += 5 * SP;
    assert!(matches!(
        certify_plain(&G, &a, &b, &t1, pi, &thr),
        Err(CertFailure::NegativeResidualA { .. } | CertFailure::NegativeResidualB { .. })
    ));
    // trade an incompatible pair
    let mut t2 = t.clone();
    t2[7][0] = 1;
    assert!(matches!(
        certify_plain(&G, &a, &b, &t2, pi, &thr),
        Err(CertFailure::IncompatibleTrade { .. })
    ));
    // withhold 5 lots at the marginal cell (3,3): welfare loss 5000·w_33 = 10_000 > δ_T+δ_A+δ_B = 3_600,
    // so by Theorem 6 no candidate price can certify it
    assert_eq!(t[3][3], 12 * SP, "preset: the marginal cell trades 12 lots");
    let mut t3 = t.clone();
    t3[3][3] -= 5 * SP;
    assert!(G
        .candidates()
        .iter()
        .all(|&p| certify_plain(&G, &a, &b, &t3, p, &thr).is_err()));
    // withholding up to δ_B / w_33 = 900 micro-lots at the marginal cell is *within tolerance*:
    // that is the price of tolerating CKKS noise (docs/M1-market-math.md §6.4)
    let mut t4 = t.clone();
    t4[3][3] -= 900;
    assert!(G
        .candidates()
        .iter()
        .any(|&p| certify_plain(&G, &a, &b, &t4, p, &thr).is_ok()));
    let mut t5 = t.clone();
    t5[3][3] -= 901;
    assert!(G
        .candidates()
        .iter()
        .all(|&p| certify_plain(&G, &a, &b, &t5, p, &thr).is_err()));
}
