use bt_engine::Cheat;
use bt_protocol::{decode_msg, honesty_report, run_session, RunConfig, SessionParams};
use rand_core::{CryptoRng, RngCore};
use wasm_bindgen::prelude::*;

/// OS / browser CSPRNG. Demo crypto must not use a seeded test RNG.
struct OsFill;

impl CryptoRng for OsFill {}

impl RngCore for OsFill {
    fn next_u32(&mut self) -> u32 {
        let mut b = [0u8; 4];
        self.fill_bytes(&mut b);
        u32::from_le_bytes(b)
    }
    fn next_u64(&mut self) -> u64 {
        let mut b = [0u8; 8];
        self.fill_bytes(&mut b);
        u64::from_le_bytes(b)
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        getrandom::getrandom(dest).expect("OS/browser RNG");
    }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
        getrandom::getrandom(dest).map_err(|e| {
            let code = std::num::NonZeroU32::new(1).unwrap();
            let _ = e;
            rand_core::Error::from(code)
        })
    }
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub fn info() -> JsValue {
    let params = bt_ckks::Params::toy12();
    let v = serde_json::json!({
        "crate": "bt-wasm",
        "version": env!("CARGO_PKG_VERSION"),
        "commit": option_env!("GIT_COMMIT").unwrap_or("unknown"),
        "params": {
            "name": "TOY-12",
            "n": params.n(),
            "levels": params.levels,
            "log_qp_approx": 840,
        },
        "security": params.security_label(),
        "honesty": honesty_report(11),
        "smudge_lambda": 11,
    });
    serde_wasm_bindgen::to_value(&v).unwrap_or(JsValue::NULL)
}

#[wasm_bindgen]
pub struct FirmHandle {
    role: String,
    book: Vec<u64>,
}

#[wasm_bindgen]
impl FirmHandle {
    #[wasm_bindgen(constructor)]
    pub fn new(role: &str, params: &[u8]) -> FirmHandle {
        let _ = postcard::from_bytes::<SessionParams>(params);
        FirmHandle {
            role: role.to_string(),
            book: vec![0; 8],
        }
    }

    pub fn set_book(&mut self, book: &[u64]) {
        self.book = book.to_vec();
    }

    pub fn set_cheat(&mut self, _cheat: &str) {}

    pub fn start(&mut self) -> js_sys::Array {
        let _ = &self.role;
        js_sys::Array::new()
    }

    pub fn on_message(&mut self, msg: &[u8]) -> js_sys::Array {
        let _ = decode_msg(msg);
        js_sys::Array::new()
    }
}

#[wasm_bindgen]
pub struct ServerHandle {
    cheat: String,
}

#[wasm_bindgen]
impl ServerHandle {
    #[wasm_bindgen(constructor)]
    pub fn new(params: &[u8]) -> ServerHandle {
        let _ = postcard::from_bytes::<SessionParams>(params);
        ServerHandle {
            cheat: String::new(),
        }
    }

    pub fn set_cheat(&mut self, cheat: &str) {
        self.cheat = cheat.to_string();
    }

    pub fn on_message(&mut self, msg: &[u8]) -> js_sys::Array {
        let _ = decode_msg(msg);
        let _ = &self.cheat;
        js_sys::Array::new()
    }

    pub fn step(&mut self) -> JsValue {
        serde_wasm_bindgen::to_value(&serde_json::json!({
            "op": "idle",
            "level": 12,
            "micros": 0,
            "keyswitches": 0,
            "done": true
        }))
        .unwrap_or(JsValue::NULL)
    }
}

#[wasm_bindgen]
pub struct VerifierHandle {}

#[wasm_bindgen]
impl VerifierHandle {
    #[wasm_bindgen(constructor)]
    pub fn new(params: &[u8]) -> VerifierHandle {
        let _ = postcard::from_bytes::<SessionParams>(params);
        VerifierHandle {}
    }

    pub fn on_message(&mut self, msg: &[u8]) -> js_sys::Array {
        let _ = decode_msg(msg);
        js_sys::Array::new()
    }

    pub fn checks(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&Vec::<bt_cert::CheckResult>::new()).unwrap_or(JsValue::NULL)
    }
}

#[wasm_bindgen]
pub fn decode_header(msg: &[u8]) -> JsValue {
    match decode_msg(msg) {
        Ok(env) => serde_wasm_bindgen::to_value(&env.header).unwrap_or(JsValue::NULL),
        Err(_) => JsValue::NULL,
    }
}

#[wasm_bindgen]
pub fn honesty() -> JsValue {
    serde_wasm_bindgen::to_value(&honesty_report(11)).unwrap_or(JsValue::NULL)
}

#[wasm_bindgen]
pub fn run_demo(book_a: &[u64], book_b: &[u64]) -> JsValue {
    let mut rng = OsFill;
    let params = SessionParams::demo([7u8; 32]);
    let a = if book_a.len() == 8 {
        book_a.to_vec()
    } else {
        vec![0, 5000, 10000, 25000, 30000, 20000, 15000, 10000]
    };
    let b = if book_b.len() == 8 {
        book_b.to_vec()
    } else {
        vec![10000, 15000, 25000, 30000, 20000, 8000, 0, 0]
    };
    let res = run_session(&params, &a, &b, &RunConfig::default(), &mut rng);
    let out = serde_json::json!({
        "accepted": res.verdict.accepted,
        "pi": res.verdict.pi,
        "g_t": res.verdict.g_t,
        "welfare": res.verdict.welfare,
        "checks": res.verdict.checks,
        "trades": res.trades,
        "messages": res.messages.len(),
        "bytes": res.bytes_total,
        "security": "toy, insecure",
        "honesty": res.honesty,
        "timings_us": res.timings_us,
        "smudge_lambda": params.smudge_lambda,
        "toy": true,
    });
    serde_wasm_bindgen::to_value(&out).unwrap_or(JsValue::NULL)
}

#[wasm_bindgen]
pub fn run_demo_cheat(book_a: &[u64], book_b: &[u64], cheat: &str) -> JsValue {
    let mut rng = OsFill;
    let params = SessionParams::demo([7u8; 32]);
    let a = book_a.to_vec();
    let b = book_b.to_vec();
    let mut cfg = RunConfig::default();
    match cheat {
        "steer" => cfg.cheat_a = Cheat::Steer,
        "tamper" => cfg.cheat_server = Cheat::TamperOutput,
        "truncate" => cfg.cheat_server = Cheat::TruncateSign(0),
        _ => {}
    }
    let res = run_session(&params, &a, &b, &cfg, &mut rng);
    serde_wasm_bindgen::to_value(&serde_json::json!({
        "accepted": res.verdict.accepted,
        "checks": res.verdict.checks,
        "security": "toy, insecure",
        "honesty": res.honesty,
    }))
    .unwrap_or(JsValue::NULL)
}

#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_tests {
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn getrandom_browser_backend() {
        let mut b = [0u8; 32];
        getrandom::getrandom(&mut b).expect("getrandom");
        assert!(b.iter().any(|&x| x != 0), "OS RNG returned zeros");
    }

    #[wasm_bindgen_test]
    fn u128_lowering_mul() {
        let a: u128 = (1u128 << 64) + 7;
        let b: u128 = (1u128 << 64) + 3;
        let p = a.wrapping_mul(b);
        assert_ne!(p, 0);
    }
}
