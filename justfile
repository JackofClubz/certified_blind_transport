default: test

fmt:
    cargo fmt --all -- --check

lint:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace --release --offline --exclude bt-wasm

bench:
    cargo run -p bt-cli --release -- run

wasm:
    cargo build -p bt-wasm --release --target wasm32-unknown-unknown
    wasm-bindgen target/wasm32-unknown-unknown/release/bt_wasm.wasm --out-dir web/demo/src/pkg --target web --typescript

web: wasm
    cd web/demo && npm install && npm run build

vectors:
    cargo run -p bt-cli --release -- gen-vectors --out test-vectors/
