//! Toy RNS-CKKS. Parameters are insecure and labelled `"toy, insecure"`.

#![allow(clippy::needless_range_loop)]

pub mod context;
pub mod encoder;
pub mod evaluator;
pub mod keys;
pub mod params;
pub mod poly;
pub mod secret;
pub mod types;

pub use context::Context;
pub use encoder::{decode, encode, encode_const};
pub use evaluator::{rotate, Evaluator};
pub use keys::{
    decrypt, encrypt, gen_public, gen_relin_keys, gen_rotation_key, gen_secret, gen_switching_key,
};
pub use params::Params;
pub use secret::Secret;
pub use types::{
    Ciphertext, CiphertextWire, EvalKeys, Plaintext, PublicKey, SecretKey, SwitchingKey,
};

pub fn encode_plaintext(ctx: &Context, z: &[f64], level: usize, scale: f64) -> Plaintext {
    let (poly, level, scale) = encode(ctx, z, level, scale);
    Plaintext { poly, level, scale }
}

pub fn decode_plaintext(ctx: &Context, pt: &Plaintext) -> Vec<f64> {
    decode(ctx, &pt.poly, pt.level, pt.scale)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::ChaCha20Rng;
    use rand_core::SeedableRng;

    fn setup() -> (Context, ChaCha20Rng) {
        let ctx = Context::new(&Params::toy_test());
        let rng = ChaCha20Rng::from_seed([7u8; 32]);
        (ctx, rng)
    }

    #[test]
    fn encode_decode_roundtrip() {
        let (ctx, _) = setup();
        let z = [0.25, -0.5, 0.125, 0.0];
        let scale = ctx.scale_at_level(ctx.top_level());
        let pt = encode_plaintext(&ctx, &z, ctx.top_level(), scale);
        let out = decode_plaintext(&ctx, &pt);
        for (a, b) in z.iter().zip(&out) {
            assert!((a - b).abs() < 1e-6, "{a} vs {b}");
        }
    }

    #[test]
    fn encrypt_decrypt() {
        let (ctx, mut rng) = setup();
        let sk = gen_secret(&ctx, &mut rng);
        let pk = gen_public(&ctx, &sk, &mut rng);
        let z = [0.1, 0.2, -0.05];
        let scale = ctx.scale_at_level(ctx.top_level());
        let pt = encode_plaintext(&ctx, &z, ctx.top_level(), scale);
        let ct = encrypt(&ctx, &pk, &pt, &mut rng);
        let pt2 = decrypt(&ctx, &sk, &ct);
        let out = decode_plaintext(&ctx, &pt2);
        for (a, b) in z.iter().zip(&out) {
            assert!((a - b).abs() < 1e-4, "{a} vs {b}");
        }
    }

    #[test]
    fn homomorphic_add_and_mul() {
        let (ctx, mut rng) = setup();
        let sk = gen_secret(&ctx, &mut rng);
        let pk = gen_public(&ctx, &sk, &mut rng);
        let evk = gen_relin_keys(&ctx, &sk, &mut rng);
        let eval = Evaluator::new(&ctx, &evk);
        let scale = ctx.scale_at_level(ctx.top_level());
        let z1 = [0.2f64, 0.1];
        let z2 = [0.15f64, -0.05];
        let e1 = encrypt(
            &ctx,
            &pk,
            &encode_plaintext(&ctx, &z1, ctx.top_level(), scale),
            &mut rng,
        );
        let e2 = encrypt(
            &ctx,
            &pk,
            &encode_plaintext(&ctx, &z2, ctx.top_level(), scale),
            &mut rng,
        );
        let sum = eval.add(&e1, &e2);
        let out = decode_plaintext(&ctx, &decrypt(&ctx, &sk, &sum));
        assert!((out[0] - 0.35).abs() < 1e-3, "got {}", out[0]);

        let scaled = eval.mul_const(&e1, 0.5);
        let out = decode_plaintext(&ctx, &decrypt(&ctx, &sk, &scaled));
        assert!(
            (out[0] - 0.1).abs() < 1e-2,
            "mul_const 0.5: got {} want 0.1 (level {}, scale {})",
            out[0],
            scaled.level,
            scaled.scale
        );

        let prod = eval.mul(&e1, &e2);
        let out = decode_plaintext(&ctx, &decrypt(&ctx, &sk, &prod));
        assert!(
            (out[0] - 0.03).abs() < 5e-3,
            "ct×ct: got {} want 0.03 (level {}, scale {})",
            out[0],
            prod.level,
            prod.scale
        );
        assert!(
            (out[1] - (-0.005)).abs() < 5e-3,
            "ct×ct slot1: got {} want -0.005",
            out[1]
        );
    }

    #[test]
    fn mul_error_per_remaining_level() {
        let (ctx, mut rng) = setup();
        let sk = gen_secret(&ctx, &mut rng);
        let pk = gen_public(&ctx, &sk, &mut rng);
        let evk = gen_relin_keys(&ctx, &sk, &mut rng);
        let eval = Evaluator::new(&ctx, &evk);
        for start in [ctx.top_level(), 8usize, 4] {
            if start == 0 {
                continue;
            }
            let scale = ctx.scale_at_level(start);
            let z1 = [0.2f64, 0.1];
            let z2 = [0.15f64, -0.05];
            let e1 = encrypt(
                &ctx,
                &pk,
                &encode_plaintext(&ctx, &z1, start, scale),
                &mut rng,
            );
            let e2 = encrypt(
                &ctx,
                &pk,
                &encode_plaintext(&ctx, &z2, start, scale),
                &mut rng,
            );
            let prod = eval.mul(&e1, &e2);
            let out = decode_plaintext(&ctx, &decrypt(&ctx, &sk, &prod));
            let err = (out[0] - 0.03).abs();
            eprintln!("level {start} -> {}  err={err:.3e}  scale={}", prod.level, prod.scale);
            assert!(err < 0.02, "level {start}: err {err}");
        }
    }
}
