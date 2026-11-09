//! `cargo run -p bt-arith --example toy12_chain`: the TOY-12 modulus chain (insecure demo parameters,
//! see docs/05) with each prime's primitive 2N-th root and the drift of the scaling primes from Δ = 2^50.

use bt_arith::modops::pow_mod;
use bt_arith::primes::{ntt_primes_below, ntt_primes_near, primitive_2n_root};

fn main() {
    let n = 1usize << 12;
    let q0 = ntt_primes_below(60, 1, n, &[]);
    let specials = ntt_primes_below(60, 4, n, &[])[1..].to_vec(); // next three below 2^60
    let mut taken = q0.clone();
    taken.extend(&specials);
    let scaling = ntt_primes_near(50, 12, n, &taken);
    let show = |name: &str, q: u64| {
        let psi = primitive_2n_root(q, n);
        assert_eq!(pow_mod(psi, n as u64, q), q - 1);
        println!(
            "{name:<6} q = {q:>20}  log2 = {:>12.9}  psi = {psi}",
            (q as f64).log2()
        );
    };
    println!("N = {n}, all q ≡ 1 mod 2N = {}", 2 * n);
    show("q0", q0[0]);
    for (i, &q) in scaling.iter().enumerate() {
        show(&format!("q{}", i + 1), q);
    }
    for (i, &q) in specials.iter().enumerate() {
        show(&format!("p{i}"), q);
    }
    let rel: Vec<f64> = scaling
        .iter()
        .map(|&q| (q as f64 - (1u64 << 50) as f64) / (1u64 << 50) as f64)
        .collect();
    let max_eps = rel.iter().fold(0f64, |m, &e| m.max(e.abs()));
    let log2_prod: f64 = scaling.iter().map(|&q| (q as f64).log2()).sum();
    println!(
        "max |q_i/2^50 - 1| = {max_eps:.3e}   log2(Π q_i) - 600 = {:.3e} bits",
        log2_prod - 600.0
    );
    let log_qp: f64 = q0
        .iter()
        .chain(&scaling)
        .chain(&specials)
        .map(|&q| (q as f64).log2())
        .sum();
    println!("log2(QP) = {log_qp:.2}");
}
