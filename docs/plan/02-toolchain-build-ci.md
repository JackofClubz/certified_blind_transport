# 02. Toolchain, build and CI

Goal: one command builds everything; CI produces the release `.wasm` in a pinned container; anyone can rebuild it and get the same SHA-256.

## 1. Toolchain

### 1.1 `rust-toolchain.toml`

```toml
[toolchain]
channel = "1.95.0"            # pin an exact stable release; bump deliberately
components = ["rustfmt", "clippy"]
targets = ["wasm32-unknown-unknown"]
profile = "minimal"
```

On a normal machine, `rustup` reads this file and installs the target automatically.

**Restricted environments** (such as the sandbox this plan was written in, where `static.rust-lang.org` is blocked but conda-forge is reachable): conda-forge publishes matching `rust` and `rust-std-wasm32-unknown-unknown` packages. Install both at the same version into a dedicated prefix with micromamba, then put that prefix's `bin` first on `PATH`. Verify with `rustc --print target-list | grep wasm32` and a test build. This route still needs checking at M0.

### 1.2 Other tools

| Tool | Why | Pinning rule |
|---|---|---|
| `wasm-bindgen-cli` | Generates the JS glue for the `.wasm` | **Must equal** the `wasm-bindgen` crate version in `Cargo.lock`. Install with `cargo install wasm-bindgen-cli --version =X.Y.Z --locked` |
| `wasm-opt` (binaryen) | Size and speed optimisation of the final `.wasm` | Pin a release; run in the container |
| Node.js LTS + pnpm | Web build (Vite), Playwright | `.nvmrc` + `packageManager` field in `package.json` |
| `just` | Task runner | Any recent version |
| `wasm-pack` (optional) | Convenience wrapper | Not required: the plain `cargo build` + `wasm-bindgen` path below is more transparent and reproducible |

## 2. Cargo configuration

### 2.1 Workspace `Cargo.toml` (excerpt)

```toml
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]
edition = "2021"
license = "Apache-2.0 OR MIT"
rust-version = "1.95"

[workspace.dependencies]
# add with `cargo add -p <crate> <dep>` at M0, then hoist the versions here
serde = { version = "1", features = ["derive"] }
postcard = { version = "1", features = ["alloc"] }
rand_core = "*"           # replace * with the pinned versions at M0
rand_chacha = "*"
sha2 = "*"
merlin = "*"
curve25519-dalek = "*"    # must match the version bulletproofs depends on
bulletproofs = "*"
wasm-bindgen = "=*"       # exact pin, mirrored by wasm-bindgen-cli
proptest = "*"
criterion = "*"

[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
debug = false

[profile.release-debuginfo]      # for the "debug symbols" build visitors can step through
inherits = "release"
debug = true
strip = false
```

### 2.2 `.cargo/config.toml`

```toml
[target.wasm32-unknown-unknown]
rustflags = [
  "-C", "target-feature=+simd128",          # WebAssembly SIMD: speeds up NTT and RNS loops
  "--cfg", 'getrandom_backend="wasm_js"',   # getrandom 0.3: use crypto.getRandomValues
]
```

- If the dependency tree uses **getrandom 0.2**, enable its `js` feature instead: add `getrandom = { version = "0.2", features = ["js"] }` to `bt-wasm`. Check with `cargo tree -i getrandom --target wasm32-unknown-unknown`. Both versions can coexist; configure each.
- SIMD is supported by all current major browsers. Keep a non-SIMD build only if you need very old Safari.

### 2.3 Panics and logging in WebAssembly

- `bt-wasm` calls `console_error_panic_hook::set_once()` at start-up, so Rust panics appear readably in the console.
- Use a tiny logging shim that forwards to `console.debug` only when a `debug` flag is set. The protocol must never log secrets: enforce this with a `Secret<T>` wrapper whose `Debug` implementation prints `"<secret>"`.

## 3. Building

### 3.1 `justfile`

```make
default: test

fmt:      cargo fmt --all -- --check
lint:     cargo clippy --workspace --all-targets -- -D warnings
test:     cargo test --workspace
bench:    cargo bench -p bt-cli

wasm:
    cargo build -p bt-wasm --release --target wasm32-unknown-unknown
    wasm-bindgen target/wasm32-unknown-unknown/release/bt_wasm.wasm \
        --out-dir web/demo/src/pkg --target web --typescript
    wasm-opt -O3 --enable-simd web/demo/src/pkg/bt_wasm_bg.wasm -o web/demo/src/pkg/bt_wasm_bg.wasm
    sha256sum web/demo/src/pkg/bt_wasm_bg.wasm | tee web/demo/src/pkg/SHA256SUMS

wasm-test:
    wasm-pack test --headless --chrome crates/bt-wasm   # or: cargo test --target wasm32-unknown-unknown with wasm-bindgen-test-runner

web: wasm
    cd web/demo && pnpm install --frozen-lockfile && pnpm build

e2e: web
    docker compose -f docker/compose.yml run --rm e2e

vectors:
    cargo run -p bt-cli --release -- gen-vectors --out test-vectors/

release-local:
    docker build -f docker/build.Dockerfile -t cbt-build .
    docker run --rm -v "$PWD":/src -w /src cbt-build just web
```

`--target web` produces an ES module that each worker imports with `await init(new URL('./pkg/bt_wasm_bg.wasm', import.meta.url))`. Vite copies the `.wasm` file into `dist/assets/` with a content hash in its name.

### 3.2 `docker/build.Dockerfile`

```dockerfile
# Pin by digest: docker pull rust:1.95-bookworm && docker inspect --format='{{index .RepoDigests 0}}'
FROM rust:1.95-bookworm@sha256:<digest> AS base

ARG WASM_BINDGEN_VERSION=<exact, from Cargo.lock>
ARG BINARYEN_VERSION=<pinned release>
ARG NODE_MAJOR=20

RUN rustup target add wasm32-unknown-unknown \
 && cargo install wasm-bindgen-cli --version "=${WASM_BINDGEN_VERSION}" --locked \
 && cargo install just --locked

RUN curl -fsSL "https://github.com/WebAssembly/binaryen/releases/download/version_${BINARYEN_VERSION}/binaryen-version_${BINARYEN_VERSION}-x86_64-linux.tar.gz" \
    | tar -xz -C /opt && ln -s /opt/binaryen-version_${BINARYEN_VERSION}/bin/wasm-opt /usr/local/bin/wasm-opt

RUN curl -fsSL https://deb.nodesource.com/setup_${NODE_MAJOR}.x | bash - \
 && apt-get install -y nodejs && corepack enable

ENV SOURCE_DATE_EPOCH=0 \
    CARGO_INCREMENTAL=0 \
    RUSTFLAGS="--remap-path-prefix=/src=. --remap-path-prefix=/usr/local/cargo=cargo"
WORKDIR /src
```

Replace each `<...>` at M0 with the pinned value and commit the result.

### 3.3 `.devcontainer/devcontainer.json`

```json
{
  "name": "cbt",
  "build": { "dockerfile": "../docker/build.Dockerfile", "context": ".." },
  "customizations": { "vscode": { "extensions": ["rust-lang.rust-analyzer", "ms-playwright.playwright"] } },
  "postCreateCommand": "cd web/demo && pnpm install"
}
```

### 3.4 `docker/compose.yml` and `docker/Caddyfile`

```yaml
services:
  preview:
    image: caddy:2
    volumes:
      - ../web/demo/dist:/srv:ro
      - ./Caddyfile:/etc/caddy/Caddyfile:ro
    ports: ["8080:80"]
  e2e:
    image: mcr.microsoft.com/playwright:<pinned tag matching @playwright/test>
    depends_on: [preview]
    working_dir: /src/web/demo
    volumes: ["..:/src"]
    environment: [BASE_URL=http://preview]
    command: npx playwright test
```

```caddyfile
:80 {
  root * /srv
  file_server
  header {
    Content-Security-Policy "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self' blob:; style-src 'self'; font-src 'self'; img-src 'self' data:; connect-src 'self'"
    X-Content-Type-Options nosniff
    Referrer-Policy no-referrer
  }
  @wasm path *.wasm
  header @wasm Content-Type application/wasm
}
```

## 4. Continuous integration

### 4.1 `.github/workflows/ci.yml`

```yaml
name: ci
on: [push, pull_request]
jobs:
  rust:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@master   # keep in sync with rust-toolchain.toml
        with: { toolchain: "1.95.0", targets: "wasm32-unknown-unknown", components: "rustfmt, clippy" }
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all -- --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace --release        # release: CKKS tests are slow in debug
      - run: cargo build -p bt-wasm --release --target wasm32-unknown-unknown
  wasm-and-web:
    runs-on: ubuntu-latest
    container: ghcr.io/<you>/cbt-build:<tag>        # the image from docker/build.Dockerfile, pushed by release.yml
    steps:
      - uses: actions/checkout@v4
      - run: just web
      - run: cd web/demo && pnpm test               # vitest unit tests for UI logic
      - uses: actions/upload-artifact@v4
        with: { name: demo-dist, path: web/demo/dist }
  e2e:
    needs: wasm-and-web
    runs-on: ubuntu-latest
    container: mcr.microsoft.com/playwright:<pinned>
    steps:
      - uses: actions/checkout@v4
      - uses: actions/download-artifact@v4
        with: { name: demo-dist, path: web/demo/dist }
      - run: cd web/demo && pnpm install --frozen-lockfile && pnpm exec playwright test
```

Pin third-party actions by commit SHA once the workflow stabilises.

### 4.2 `.github/workflows/release.yml` (on tag `v*`)

1. Build the image from `docker/build.Dockerfile` and push it to GHCR (tagged with the release).
2. Inside it, run `just web`.
3. Compute `SHA256SUMS` for `bt_wasm_bg.wasm` and every file in `dist/`.
4. Create a GitHub Release with `demo-dist.tar.gz`, `SHA256SUMS` and the build-image digest.
5. Attach build provenance (GitHub's artifact attestation action), so visitors can verify which workflow produced the binary.

## 5. Reproducible builds: how a visitor verifies

The README and the demo's "Verify this build" panel give these steps:

```bash
git clone https://github.com/<you>/certified-blind-transport && cd certified-blind-transport
git checkout v1.0.0
docker build -f docker/build.Dockerfile -t cbt-build .
docker run --rm -v "$PWD":/src -w /src cbt-build just wasm
sha256sum web/demo/src/pkg/bt_wasm_bg.wasm   # must equal the hash shown on the website
```

Sources of non-determinism to eliminate (check by building twice in CI and diffing):
- absolute paths: handled by `--remap-path-prefix`;
- timestamps: `SOURCE_DATE_EPOCH`;
- dependency drift: committed `Cargo.lock`, `--locked`, and a frozen pnpm lockfile;
- tool drift: everything pinned in the image digest.

## 6. M0 checklist

- [ ] Toolchain installed (rustup, or conda-forge in restricted environments); `cargo build --target wasm32-unknown-unknown` works on an empty crate
- [ ] Workspace skeleton with all crates as empty libraries; `just test` passes
- [ ] `bt-wasm` exports `info()` returning `{crate, version, commit, params, security: "toy, insecure"}`
- [ ] `getrandom` WebAssembly backend configured; a test draws random bytes inside a worker
- [ ] `curve25519-dalek` and `bulletproofs` compile together for wasm32 (one range proof created and verified in a wasm test)
- [ ] Vite app loads the module in a worker; `bt.info()` callable from the console
- [ ] CI green; release workflow produces `SHA256SUMS`; second build reproduces the same hash
