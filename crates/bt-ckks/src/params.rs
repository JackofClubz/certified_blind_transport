use serde::{Deserialize, Serialize};

/// CKKS parameter set. `TOY-12` is the demo default and is **insecure**.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Params {
    pub n_log: u32,
    pub q0_bits: u32,
    pub q_bits: u32,
    /// Number of scaling primes (levels between refreshes).
    pub levels: usize,
    pub p_count: usize,
    pub dnum: usize,
    pub sigma_bits: u32, // store as bits of a fixed point; actual sigma is 3.19
}

impl Params {
    /// Insecure demo parameters (`docs/plan/05` §1.1).
    pub fn toy12() -> Self {
        Self {
            n_log: 12,
            q0_bits: 60,
            q_bits: 50,
            levels: 12,
            p_count: 3,
            dnum: 5,
            sigma_bits: 0,
        }
    }

    /// Tiny set for unit tests and the in-browser toy run (still labelled insecure).
    /// `dnum = 1` keeps key-switching as a single Q-only gadget.
    /// ~50 scaling primes of ~40 bits cover one f₃ layer of |x| plus a public ½.
    pub fn toy_test() -> Self {
        Self {
            n_log: 7, // N = 128, 64 slots (one 8×8 market)
            q0_bits: 50,
            q_bits: 40,
            levels: 60,
            p_count: 2,
            dnum: 1,
            sigma_bits: 0,
        }
    }

    pub fn n(&self) -> usize {
        1usize << self.n_log
    }

    pub fn slots(&self) -> usize {
        self.n() / 2
    }

    pub fn sigma(&self) -> f64 {
        3.19
    }

    pub fn security_label(&self) -> &'static str {
        "toy, insecure"
    }

    pub fn id_hash(&self) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let bytes = postcard::to_allocvec(self).expect("params serialize");
        let mut h = Sha256::new();
        h.update(b"cbt/params/");
        h.update(&bytes);
        h.finalize().into()
    }
}
