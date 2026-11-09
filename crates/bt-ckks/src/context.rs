use bt_arith::primes::{ntt_primes_below, ntt_primes_near};
use bt_arith::rns::Basis;
use sha2::{Digest, Sha256};

use crate::params::Params;

/// Precomputed moduli, RNS bases and gadget grouping for one parameter set.
#[derive(Clone, Debug)]
pub struct Context {
    pub params: Params,
    pub q_moduli: Vec<u64>,
    pub p_moduli: Vec<u64>,
    pub q_basis: Basis,
    pub p_basis: Basis,
    pub qp_basis: Basis,
    /// Groups of Q-prime indices for hybrid key switching.
    pub digits: Vec<Vec<usize>>,
    pub params_id: [u8; 32],
}

impl Context {
    pub fn new(p: &Params) -> Self {
        let n = p.n();
        let q0 = ntt_primes_below(p.q0_bits, 1, n, &[])[0];
        let specials = ntt_primes_below(p.q0_bits, p.p_count + 1, n, &[])[1..].to_vec();
        let mut taken = vec![q0];
        taken.extend(&specials);
        let scaling = ntt_primes_near(p.q_bits, p.levels, n, &taken);
        let mut q_moduli = vec![q0];
        q_moduli.extend(scaling);
        let p_moduli = specials;
        let mut qp = q_moduli.clone();
        qp.extend(&p_moduli);
        let q_basis = Basis::new(q_moduli.clone(), n);
        let p_basis = Basis::new(p_moduli.clone(), n);
        let qp_basis = Basis::new(qp, n);
        let digits = partition_digits(q_moduli.len(), p.dnum);
        let params_id = {
            let mut h = Sha256::new();
            h.update(b"cbt/ctx/");
            h.update(p.id_hash());
            for &q in q_moduli.iter().chain(&p_moduli) {
                h.update(q.to_le_bytes());
            }
            h.finalize().into()
        };
        Self {
            params: p.clone(),
            q_moduli,
            p_moduli,
            q_basis,
            p_basis,
            qp_basis,
            digits,
            params_id,
        }
    }

    pub fn n(&self) -> usize {
        self.params.n()
    }

    pub fn slots(&self) -> usize {
        self.params.slots()
    }

    pub fn top_level(&self) -> usize {
        self.params.levels
    }

    /// Number of Q limbs present at `level` (q0 plus `level` scaling primes).
    pub fn q_limbs(&self, level: usize) -> usize {
        level + 1
    }

    pub fn q_at(&self, level: usize) -> u64 {
        self.q_moduli[level]
    }

    /// Canonical starting scale = last scaling prime (≈ Δ).
    pub fn scale_at_level(&self, level: usize) -> f64 {
        if level == 0 {
            self.q_moduli[0] as f64
        } else {
            self.q_moduli[level] as f64
        }
    }

    pub fn basis_q_level(&self, level: usize) -> Basis {
        Basis::new(self.q_moduli[..=level].to_vec(), self.n())
    }
}

fn partition_digits(nq: usize, dnum: usize) -> Vec<Vec<usize>> {
    let dnum = dnum.min(nq).max(1);
    let mut digits = vec![Vec::new(); dnum];
    for i in 0..nq {
        digits[i % dnum].push(i);
    }
    digits.retain(|d| !d.is_empty());
    digits
}
