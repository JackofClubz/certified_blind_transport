//! `cargo run -p bt-market --example preset`: the demo's default books, cleared and certified in the clear.
//! Every number quoted in `docs/M1-market-math.md` §9 comes from this program.

#![allow(clippy::needless_range_loop)] // (i, j) indices mirror the maths

use bt_market::*;

fn main() {
    let g = Grid::DEMO;
    let sp = 1000u64;
    let a: Book = [0, 5, 10, 25, 30, 20, 15, 10]
        .iter()
        .map(|x| x * sp)
        .collect();
    let b: Book = [10, 15, 25, 30, 20, 8, 0, 0]
        .iter()
        .map(|x| x * sp)
        .collect();
    let price = |k: usize| format!("${:.2}", g.ticks(k) as f64 / 10.0);

    println!("level  price    bids(A)  asks(B)   B_i   A_i");
    for k in 0..g.levels {
        println!(
            "{k:>5}  {:>7}  {:>7}  {:>7}  {:>4}  {:>4}",
            price(k),
            a[k] / sp,
            b[k] / sp,
            g.b(k),
            g.a(k)
        );
    }
    let t = monotone_coupling(&g, &a, &b);
    println!("\nnon-zero trades (bid level i, ask level j): lots, gain per micro-lot w_ij");
    for i in 0..g.levels {
        for j in 0..g.levels {
            if t[i][j] > 0 {
                println!("  ({i},{j})  {:>3} lots  w={}", t[i][j] / sp, g.w(i, j));
            }
        }
    }
    let w = welfare(&g, &t);
    let q: u64 = t.iter().flatten().sum();
    println!(
        "\ntraded volume Q* = {} lots, welfare W* = {w} (quarter-tick·micro-lots)",
        q / sp
    );
    println!(
        "quantile bound          = {}",
        quantile_welfare_bound(&g, &a, &b)
    );
    let pi = equilibrium_price(&g, &a, &b);
    println!(
        "equilibrium price Π = {pi} quarter-ticks = ${:.3}",
        pi as f64 / 40.0
    );
    let parts = gap_parts(&g, &a, &b, &t, pi);
    println!(
        "gap parts at Π: G_T={} G_A={} G_B={}",
        parts.g_t, parts.g_a, parts.g_b
    );
    println!("\ngap at every candidate price:");
    for p in g.candidates() {
        let gp = gap_parts(&g, &a, &b, &t, p);
        println!(
            "  Π={p:>5} (${:.3})  G_T={:>8} G_A={:>9} G_B={:>9}  total={:>9}",
            p as f64 / 40.0,
            gp.g_t,
            gp.g_a,
            gp.g_b,
            gp.total()
        );
    }
    let thr = thresholds(&g, 2);
    println!(
        "\nthresholds (κ=2): δ_T={} δ_A={} δ_B={}",
        thr.t, thr.a, thr.b
    );
    let dollars = (thr.t + thr.a + thr.b) as f64 * 0.025 * 0.001;
    println!("worst-case certified welfare shortfall: {dollars:.4} dollar·lots ({:.2} dollars for 100-share lots)", dollars * 100.0);
    println!(
        "as a fraction of W*: {:.2e}",
        (thr.t + thr.a + thr.b) as f64 / w as f64
    );
    let bits = |m: &[bool]| {
        m.iter()
            .map(|&x| if x { '1' } else { '0' })
            .collect::<String>()
    };
    let (ma, mb) = (bitmask_a(&g, &a, &t, &thr), bitmask_b(&g, &b, &t, &thr));
    let gt_ok: Vec<bool> = g
        .candidates()
        .iter()
        .map(|&p| gap_public(&g, &t, p) <= thr.t)
        .collect();
    println!(
        "\ncandidates ascending; masks: m_A={}  m_B={}  G_T<=δ_T: {}",
        bits(&ma),
        bits(&mb),
        bits(&gt_ok)
    );
    println!("chosen Π = {:?}", choose_pi(&g, &t, &ma, &mb, &thr));
}
