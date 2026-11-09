use bt_arith::rns::RnsPoly;
use serde::{Deserialize, Serialize};

use crate::secret::Secret;

#[derive(Clone, Debug)]
pub struct Plaintext {
    pub poly: RnsPoly,
    pub level: usize,
    pub scale: f64,
}

#[derive(Clone, Debug)]
pub struct Ciphertext {
    pub c0: RnsPoly,
    pub c1: RnsPoly,
    pub level: usize,
    pub scale: f64,
}

#[derive(Clone, Debug)]
pub struct SecretKey {
    pub poly: Secret<Vec<Vec<u64>>>,
    pub ntt_form: bool,
}

impl SecretKey {
    pub fn rns(&self) -> RnsPoly {
        RnsPoly {
            limbs: self.poly.expose().clone(),
            ntt_form: self.ntt_form,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PublicKey {
    pub b: RnsPoly,
    pub a: RnsPoly,
}

/// Hybrid switching key: one (k0, k1) pair per digit, over QP.
#[derive(Clone, Debug)]
pub struct SwitchingKey {
    pub digits: Vec<(RnsPoly, RnsPoly)>,
}

#[derive(Clone, Debug, Default)]
pub struct EvalKeys {
    pub relin: Option<SwitchingKey>,
    pub rotations: Vec<(i32, SwitchingKey)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CiphertextWire {
    pub level: usize,
    pub scale: f64,
    pub params_id: [u8; 32],
    pub c0: Vec<Vec<u64>>,
    pub c1: Vec<Vec<u64>>,
    pub ntt_form: bool,
}

impl Ciphertext {
    pub fn to_wire(&self, params_id: [u8; 32]) -> CiphertextWire {
        CiphertextWire {
            level: self.level,
            scale: self.scale,
            params_id,
            c0: self.c0.limbs.clone(),
            c1: self.c1.limbs.clone(),
            ntt_form: self.c0.ntt_form,
        }
    }

    pub fn from_wire(w: CiphertextWire) -> Self {
        Ciphertext {
            c0: RnsPoly {
                limbs: w.c0,
                ntt_form: w.ntt_form,
            },
            c1: RnsPoly {
                limbs: w.c1,
                ntt_form: w.ntt_form,
            },
            level: w.level,
            scale: w.scale,
        }
    }
}

impl zeroize::Zeroize for SecretKey {
    fn zeroize(&mut self) {
        self.poly.zeroize();
    }
}
