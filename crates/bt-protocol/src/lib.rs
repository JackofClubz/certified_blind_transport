//! Protocol messages, party handles, and an in-process runner.

use bt_cert::{
    commit_orders, gap_bitmask, prove_settle, verify_all, PublicRecord, Role as CertRole, Verdict,
};
use bt_ckks::{Context, Params};
use bt_engine::{circuit_f64_poly, encrypt_inputs, round_output, Cheat, SignSpec};
use bt_market::{choose_pi, thresholds, Book, Grid};
use bt_mhe::{combine_pk, Crs, Party, Role as MheRole};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use hkdf::Hkdf;
use rand_chacha::ChaCha20Rng;
use rand_core::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey as XPublic, StaticSecret};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionParams {
    pub session_id: [u8; 32],
    pub grid_levels: usize,
    pub top_ticks: i64,
    pub limit: u64,
    pub kappa: u64,
    pub smudge_lambda: u32,
    pub mask_lambda: u32,
    pub demo_mode: bool,
    pub toy: bool,
}

impl SessionParams {
    pub fn demo(session_id: [u8; 32]) -> Self {
        Self {
            session_id,
            grid_levels: 8,
            top_ticks: 1004,
            limit: 50_000,
            kappa: 2,
            smudge_lambda: 11,
            mask_lambda: 40,
            demo_mode: true,
            toy: true,
        }
    }

    pub fn hash(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(postcard::to_allocvec(self).unwrap());
        h.finalize().into()
    }

    pub fn grid(&self) -> Grid {
        Grid {
            levels: self.grid_levels,
            top_ticks: self.top_ticks,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Header {
    pub version: u8,
    pub session_id: [u8; 32],
    pub seq: u32,
    pub from: PartyId,
    pub to: Vec<PartyId>,
    pub kind: MsgKind,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PartyId {
    A,
    B,
    S,
    V,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MsgKind {
    Params,
    ChannelHello,
    CrsCommit,
    CrsReveal,
    CkgShare,
    PublicKey,
    Commit,
    InputCts,
    OutputCt,
    DecShare,
    Trades,
    Bitmask,
    Settle,
    Verdict,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Envelope {
    pub header: Header,
    pub payload: Vec<u8>,
}

pub fn encode_msg(env: &Envelope) -> Vec<u8> {
    postcard::to_allocvec(env).expect("msg")
}

pub fn decode_msg(bytes: &[u8]) -> Result<Envelope, postcard::Error> {
    postcard::from_bytes(bytes)
}

#[derive(Clone, Debug)]
pub struct RunResult {
    pub verdict: Verdict,
    pub trades: bt_market::Trades,
    pub messages: Vec<Envelope>,
    pub bytes_total: usize,
}

#[derive(Clone, Debug, Default)]
pub struct RunConfig {
    pub cheat_a: Cheat,
    pub cheat_b: Cheat,
    pub cheat_server: Cheat,
    pub sign: SignSpec,
}

pub fn run_session(
    params: &SessionParams,
    book_a: &Book,
    book_b: &Book,
    cfg: &RunConfig,
    rng: &mut ChaCha20Rng,
) -> RunResult {
    let mut messages = Vec::new();
    let mut seq = 0u32;
    let push = |from: PartyId,
                to: Vec<PartyId>,
                kind: MsgKind,
                payload: Vec<u8>,
                messages: &mut Vec<Envelope>,
                seq: &mut u32| {
        let env = Envelope {
            header: Header {
                version: 1,
                session_id: params.session_id,
                seq: *seq,
                from,
                to,
                kind,
            },
            payload,
        };
        *seq += 1;
        messages.push(env);
    };

    let ckks = Context::new(&Params::toy_test());
    let mut seed_a = [0u8; 32];
    let mut seed_b = [0u8; 32];
    rng.fill_bytes(&mut seed_a);
    rng.fill_bytes(&mut seed_b);
    push(
        PartyId::A,
        vec![PartyId::V],
        MsgKind::CrsCommit,
        Crs::commit(&seed_a).to_vec(),
        &mut messages,
        &mut seq,
    );
    push(
        PartyId::B,
        vec![PartyId::V],
        MsgKind::CrsCommit,
        Crs::commit(&seed_b).to_vec(),
        &mut messages,
        &mut seq,
    );
    push(
        PartyId::A,
        vec![PartyId::V],
        MsgKind::CrsReveal,
        seed_a.to_vec(),
        &mut messages,
        &mut seq,
    );
    push(
        PartyId::B,
        vec![PartyId::V],
        MsgKind::CrsReveal,
        seed_b.to_vec(),
        &mut messages,
        &mut seq,
    );
    let crs = Crs::from_seeds(&params.session_id, seed_a, seed_b);

    let pa = Party::new(&ckks, MheRole::A, rng);
    let pb = Party::new(&ckks, MheRole::B, rng);
    let sha = pa.ckg_share(&ckks, &crs, rng);
    let shb = pb.ckg_share(&ckks, &crs, rng);
    push(
        PartyId::A,
        vec![PartyId::S],
        MsgKind::CkgShare,
        postcard::to_allocvec(&sha).unwrap(),
        &mut messages,
        &mut seq,
    );
    push(
        PartyId::B,
        vec![PartyId::S],
        MsgKind::CkgShare,
        postcard::to_allocvec(&shb).unwrap(),
        &mut messages,
        &mut seq,
    );
    let pk = combine_pk(&ckks, &crs, &[sha, shb]);
    push(
        PartyId::S,
        vec![PartyId::A, PartyId::B],
        MsgKind::PublicKey,
        vec![0u8; 32],
        &mut messages,
        &mut seq,
    );

    // X25519 firm channel (decryption shares never go to the server)
    let mut sea = [0u8; 32];
    let mut seb = [0u8; 32];
    rng.fill_bytes(&mut sea);
    rng.fill_bytes(&mut seb);
    let xa = StaticSecret::from(sea);
    let xb = StaticSecret::from(seb);
    let pa_pub = XPublic::from(&xa);
    let pb_pub = XPublic::from(&xb);
    push(
        PartyId::A,
        vec![PartyId::B],
        MsgKind::ChannelHello,
        pa_pub.as_bytes().to_vec(),
        &mut messages,
        &mut seq,
    );
    push(
        PartyId::B,
        vec![PartyId::A],
        MsgKind::ChannelHello,
        pb_pub.as_bytes().to_vec(),
        &mut messages,
        &mut seq,
    );
    let shared = xa.diffie_hellman(&pb_pub);
    let hk = Hkdf::<Sha256>::new(None, shared.as_bytes());
    let mut aead_key = [0u8; 32];
    hk.expand(b"cbt/firm-channel", &mut aead_key).unwrap();
    let aead = ChaCha20Poly1305::new_from_slice(&aead_key).unwrap();

    let book_a = book_a.clone();
    let book_b = book_b.clone();
    let commit_book_a = if matches!(cfg.cheat_a, Cheat::Steer) {
        let mut c = book_a.clone();
        c[0] = params.limit;
        c
    } else {
        book_a.clone()
    };

    let (ca, oa) = commit_orders(
        CertRole::A,
        &params.session_id,
        &commit_book_a,
        params.limit,
        rng,
    );
    let (cb, ob) = commit_orders(CertRole::B, &params.session_id, &book_b, params.limit, rng);
    push(
        PartyId::A,
        vec![PartyId::V],
        MsgKind::Commit,
        postcard::to_allocvec(&ca).unwrap(),
        &mut messages,
        &mut seq,
    );
    push(
        PartyId::B,
        vec![PartyId::V],
        MsgKind::Commit,
        postcard::to_allocvec(&cb).unwrap(),
        &mut messages,
        &mut seq,
    );

    let enc_book_a = if matches!(cfg.cheat_b, Cheat::SkipRefresh) {
        book_a.clone()
    } else if matches!(cfg.cheat_a, Cheat::TruncateSign(_)) {
        book_a.clone()
    } else {
        book_a.clone()
    };
    let cts = encrypt_inputs(&ckks, &pk, &params.grid(), &enc_book_a, &book_b, rng);
    for ct in &cts {
        let wire = ct.to_wire(ckks.params_id);
        push(
            PartyId::A,
            vec![PartyId::S],
            MsgKind::InputCts,
            postcard::to_allocvec(&wire).unwrap(),
            &mut messages,
            &mut seq,
        );
    }

    let spec = match cfg.cheat_server {
        Cheat::TruncateSign(n) => SignSpec {
            n_cubic: n,
            n_quintic: 0,
        },
        _ => cfg.sign.clone(),
    };
    let mut hat = circuit_f64_poly(&params.grid(), &enc_book_a, &book_b, &spec);
    if cfg.cheat_server == Cheat::Steer {
        // move volume between compatible cells
        if hat[0][7] > 10.0 && hat[0][6] > 0.0 {
            hat[0][7] -= 10.0;
            hat[0][6] += 10.0;
        }
    }
    let trades = round_output(&params.grid(), &hat, params.kappa, cfg.cheat_server);
    push(
        PartyId::S,
        vec![PartyId::A, PartyId::B],
        MsgKind::OutputCt,
        vec![0u8; 64],
        &mut messages,
        &mut seq,
    );

    let nonce = Nonce::from_slice(&[0u8; 12]);
    let dec_bytes = postcard::to_allocvec(&trades).unwrap();
    let sealed = aead.encrypt(nonce, dec_bytes.as_ref()).unwrap();
    push(
        PartyId::A,
        vec![PartyId::B],
        MsgKind::DecShare,
        sealed.clone(),
        &mut messages,
        &mut seq,
    );
    push(
        PartyId::B,
        vec![PartyId::A],
        MsgKind::DecShare,
        sealed,
        &mut messages,
        &mut seq,
    );
    push(
        PartyId::A,
        vec![PartyId::V],
        MsgKind::Trades,
        postcard::to_allocvec(&trades).unwrap(),
        &mut messages,
        &mut seq,
    );

    let thr = thresholds(&params.grid(), params.kappa);
    let mut mask_a = gap_bitmask(CertRole::A, &params.grid(), &commit_book_a, &trades, thr.a);
    let mask_b = gap_bitmask(CertRole::B, &params.grid(), &book_b, &trades, thr.b);
    if matches!(cfg.cheat_a, Cheat::TamperOutput) {
        for b in mask_a.iter_mut() {
            *b = !*b;
        }
    }
    push(
        PartyId::A,
        vec![PartyId::V],
        MsgKind::Bitmask,
        postcard::to_allocvec(&mask_a).unwrap(),
        &mut messages,
        &mut seq,
    );
    push(
        PartyId::B,
        vec![PartyId::V],
        MsgKind::Bitmask,
        postcard::to_allocvec(&mask_b).unwrap(),
        &mut messages,
        &mut seq,
    );

    let pi = choose_pi(&params.grid(), &trades, &mask_a, &mask_b, &thr).unwrap_or(0);
    let settle_a = prove_settle(
        CertRole::A,
        &params.session_id,
        &params.grid(),
        &commit_book_a,
        &oa,
        &trades,
        pi,
        thr.a,
        rng,
    );
    let settle_b = prove_settle(
        CertRole::B,
        &params.session_id,
        &params.grid(),
        &book_b,
        &ob,
        &trades,
        pi,
        thr.b,
        rng,
    );
    push(
        PartyId::A,
        vec![PartyId::V],
        MsgKind::Settle,
        postcard::to_allocvec(&settle_a).unwrap(),
        &mut messages,
        &mut seq,
    );
    push(
        PartyId::B,
        vec![PartyId::V],
        MsgKind::Settle,
        postcard::to_allocvec(&settle_b).unwrap(),
        &mut messages,
        &mut seq,
    );

    let rec = PublicRecord {
        sid: params.session_id,
        grid: params.grid(),
        limit: params.limit,
        kappa: params.kappa,
        commit_a: ca,
        commit_b: cb,
        trades: trades.clone(),
        mask_a,
        mask_b,
        settle_a: Some(settle_a),
        settle_b: Some(settle_b),
        simulated_bind_ok: enc_book_a == commit_book_a && book_b == *book_b,
    };
    let verdict = verify_all(&rec);
    push(
        PartyId::V,
        vec![PartyId::A, PartyId::B, PartyId::S],
        MsgKind::Verdict,
        postcard::to_allocvec(&verdict.accepted).unwrap(),
        &mut messages,
        &mut seq,
    );

    let bytes_total = messages.iter().map(|m| m.payload.len() + 64).sum();
    let _ = (&pk, &pa, &pb);
    RunResult {
        verdict,
        trades,
        messages,
        bytes_total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::SeedableRng;

    #[test]
    fn honest_run_accepts() {
        let mut rng = ChaCha20Rng::from_seed([5u8; 32]);
        let params = SessionParams::demo([7u8; 32]);
        let a = vec![0, 5000, 10000, 25000, 30000, 20000, 15000, 10000];
        let b = vec![10000, 15000, 25000, 30000, 20000, 8000, 0, 0];
        let res = run_session(&params, &a, &b, &RunConfig::default(), &mut rng);
        assert!(res.verdict.accepted, "{:?}", res.verdict.checks);
        assert!(res
            .messages
            .iter()
            .any(|m| m.header.kind == MsgKind::DecShare && m.header.to == vec![PartyId::B]));
        assert!(!res
            .messages
            .iter()
            .any(|m| m.header.kind == MsgKind::DecShare && m.header.to.contains(&PartyId::S)));
    }
}
