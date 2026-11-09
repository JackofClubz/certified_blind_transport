# 10. The web UI (`web/demo`)

The real demo keeps the look and interaction model of the current simulated one (order-book ladder, "view as", cheats, step bar, cypherpunk palette), and adds the panels that make the real cryptography visible: a network log, a certificate panel and a "verify this build" panel. Everything shown comes from the real run.

## 1. Stack

| Choice | Why |
|---|---|
| **Vite + TypeScript, no UI framework** | Small bundle, no runtime magic, easy for visitors to read in developer tools. The current demo is already vanilla JS and SVG. |
| SVG for the ladder, canvas for the ciphertext visual | As today |
| CSS custom properties shared with the site | One palette; the demo inherits the site's tokens when embedded |
| Vitest for UI logic, Playwright for end-to-end | Matches `docs/11` |
| Self-hosted fonts (Martian Mono, Hanken Grotesk via fontsource packages) | No third-party requests: important for this audience |

### 1.1 Colour meaning (unchanged from the site, now enforced in code)

| Token | Meaning | Use |
|---|---|---|
| `--amber` | Plaintext: only the key holder sees it | A firm's own orders, decrypted trades |
| `--cipher` (violet) | Ciphertext: what the server sees | Encrypted values, level meter, server log |
| `--proof` (teal) | Verified | Passing checks, commitments, proof badges |
| `--fail` (red) | Failed check | Failing checks, abort reasons |
| neutral text | Public | Prices, Π, public record |

Rule: nothing is ever drawn amber unless the current "view as" party can actually read it. Make this a function `canRead(view, datum)` and test it.

## 2. Layout

```
┌───────────────────────────────────────────────────────────────────────────────┐
│ 1 Set orders │ 2 Commit and encrypt │ 3 Compute blind │ 4 Firms decrypt │ 5 Certify │ 6 Publish │
├───────────────────────────────────────────────┬───────────────────────────────┤
│ ORDER BOOKS                view as [All│A│B│Server│Public] │ THE SERVER (live)            │
│  ladder SVG (drag bars; Π line after step 5)  │  ciphertext bytes (real)     │
│                                               │  level meter (real)          │
│  Market [Overlapping][Barely crossing][Random]│  op log: Mul, Square, MulP,  │
│  Cheater [menu]                               │    Refresh #3 …              │
│  Precision [sign iterations] Tolerance ρ      │  stats: ops · refreshes · ms │
│  [Start over]                 [Next step ▸]   ├───────────────────────────────┤
│  readout (plain-language explanation)         │ CERTIFICATE                  │
│  trades table (after step 4, per view)        │  Π · G_T, G_A, G_B vs δ/3    │
├───────────────────────────────────────────────┤  checks 1–8 [REAL]/[SIM]     │
│ NETWORK (every message, real bytes)           │  proof sizes · verify times  │
│  seq  from→to  kind        size     sha256    ├───────────────────────────────┤
│  14   A→B     DecShare 🔒  64 KB    9c1e…      │ VERIFY THIS BUILD            │
│  …                     filter: [A][B][S][V]   │  wasm sha256 · commit · steps│
└───────────────────────────────────────────────┴───────────────────────────────┘
```

On phones, the panels stack in this order: steps, order books, readout, certificate, server, network, verify build.

Desktop width: the demo breaks out of the text column, as the current one does with `.bleed`.

## 3. Steps and their states

| Step | Primary button | What runs | Readout copy (plain language) |
|---|---|---|---|
| 0 (background on load) | none | Key generation (messages 1–8) | "Setting up a key that Firm A and Firm B share. Neither will ever hold the whole key." Progress with MB exchanged. |
| 1 Set orders | "Commit and encrypt" | none (editing) | "Drag the bars, or pick a market. Firm A wants to buy and Firm B wants to sell. Choose whether anyone cheats." |
| 2 Commit and encrypt | "Send to the server" | Commit proofs and input encryption (messages 9–10) | "Each firm locked its orders into a commitment and proved each order is within the 50-lot limit, without showing it. Then it encrypted the orders under the shared key." |
| 3 Compute blind | (running) | Circuit with refreshes (11–13) | "The server is matching the orders on ciphertext. When a ciphertext runs low on levels, both firms help refresh it without seeing it." |
| 4 Firms decrypt | "Certify" | Decryption shares over the private channel (14–16) | "The firms decrypted the trades together. The server never received their decryption shares; check the network log." |
| 5 Certify | "Publish" or "Start over" | Π selection and settle proofs (17–21) | Pass: "Every check passed. At the clearing price Π, no other set of trades could have created more than δ extra value, whatever the server did." Fail: plain-language reason, for example "Firm B's trades exceed what it committed to at $100.10, so its settlement proof cannot exist." |
| 6 Publish | "Start over" | Public record shown | "This is everything the public sees: commitments, proofs, trades, Π and the verdict. No order sizes." |

**Note on the order of steps 4 and 5:** the firms see the trades before certification. If certification fails, they withhold settlement (nothing is published or executed). The trades from a failed run reveal no more than an honest run would, since they are a function of the same inputs, plus whatever the cheater changed.

## 4. Panels in detail

### 4.1 Order books (existing SVG, extended)

- Keep drag-to-edit, presets and the "edit as numbers" table.
- **New view "Public"**: shows only the trades, Π and the commitments (as hash chips). No sizes.
- **After step 5**: draw Π as a horizontal neutral line across the ladder. Traded pairs straddle it, which is the visual meaning of condition (a) in `docs/03` §3.3.

### 4.2 The server (live)

- **Ciphertext bytes:** the first 48 bytes of the current register's c0 and c1, read from the real ciphertext (`step()` returns them). Replace the old random hex.
- **Level meter:** real level of the register being processed (0–12), with a refresh animation when a refresh completes.
- **Op log:** each executed `Op` with level after, measured microseconds and key-switch count. Refresh barriers are highlighted.
- **Stats:** ops executed, refreshes, key switches, wall time (measured).
- **Circuit view** (expandable): the compiled circuit as a list, with the current program counter highlighted.

### 4.3 Network

- A virtualised table of `Bus.log`: seq, from → to, kind, size, SHA-256 prefix, time.
- A lock icon on messages sent over the firm-to-firm channel.
- A totals row: bytes per party, bytes per phase.
- A tooltip on `DecShare`: "Encrypted between the firms. The server never receives it."

### 4.4 Certificate

- Π (price and quarter-tick value) and the 16-candidate table: G_T per candidate, both bitmasks, the chosen row.
- Three bars, G_T, G_A and G_B, each against δ/3. G_A and G_B show only "≤ δ/3 (proved)". **Never show their values**, since they are private; the bar is filled to the threshold with a "proved in zero knowledge" label.
- Checks 1–8, each with status, REAL or SIMULATED tag, proof size and verification time. Expanding a check shows exactly what was verified.

### 4.5 Verify this build

- wasm SHA-256 (from `info()`), git commit, release link, and the four shell commands from `docs/02` §5 in a copyable code block.
- "Parameters: TOY-12 (N = 4096, log QP ≈ 840): insecure, for demonstration only. PROD-15 would use N = 32768."

## 5. Controls

| Control | Effect | Notes |
|---|---|---|
| Market presets and drag | Sets both books | As today |
| Cheater menu | Sets a cheat in the relevant worker | Options from `docs/08` §7 |
| Precision | Sign-polynomial iterations (SignSpec) | Low values show "rejected: not precise enough"; teaches the depth/accuracy trade-off |
| Tolerance ρ | 0.1% / 0.5% / 1% | Shows how δ changes the certificate |
| Parameter set | TOY-12 (default) / TOY-11 (faster, for phones) | Changes trigger new key generation |

Remove the old "ε" and "rounds" controls from v1. They belong to the v2 Sinkhorn engine, which appears in an "Engine" selector marked **experimental** once it exists.

## 6. Console API (for curious engineers)

Expose `window.cbt` with:

```ts
cbt.info()                      // build and params
cbt.network()                   // array of logged messages (headers, sizes, hashes)
cbt.circuit()                   // compiled ops
cbt.record()                    // public record of the last run
cbt.encryptDemo([1,2,3])        // encrypts under a throwaway single-party key and returns {bytes, decrypt()}: a toy for exploring CKKS
```

`cbt.encryptDemo` uses a separate throwaway key, never the session keys, so curiosity cannot leak anything.

## 7. Copy for the section (draft)

**Intro:** "Two rival trading firms each run an AI agent. Both would gain by trading with each other, but neither will show its order book to a competitor, and neither trusts the server in between. Here they trade anyway. The server matches their orders while they stay encrypted, and the firms then prove, against commitments made before trading, that the result is the best possible."

**Where is the cryptography?** (an expandable box) "Three pieces, all running in your browser. Encryption: a CKKS implementation written in Rust and compiled to WebAssembly, with the key split between the two firms. Matching: optimal transport, which for a single price axis has an exact closed form that we evaluate on ciphertext. Proof: Pedersen commitments and Bulletproofs check one clearing price against the committed orders, which proves the trades are optimal. The server is never trusted: if it cuts corners, the proof fails."

**Honesty box:** "Real: the encryption, joint keys, refreshes, commitments, range proofs and optimality certificate. Simulated: the proof that each ciphertext encrypts exactly the committed orders (an open research problem; see why it matters less than you'd think). Toy parameters: not secure. Smudging noise in decryption is far below a production level."

## 8. Accessibility and performance

- Keyboard: every control reachable; the ladder is editable through the numbers table; the step button has a visible focus ring.
- Screen readers: `aria-live="polite"` on the readout; the network table has a text summary ("21 messages, 24 MB, no decryption share sent to the server").
- Reduced motion: no animations of bars or flows; progress shown as text.
- Startup: show the wasm download size and progress, and start key generation immediately in the background. Never block the main thread: all cryptography runs in workers except Bulletproof verification (milliseconds).

## 9. Embedding in the website

- The site's Blind transport section keeps its intro copy and gets `<div id="cbt-demo"></div>`.
- The site build copies the pinned demo release into `/demo/` and adds `<script type="module" src="/demo/assets/main-<hash>.js">`.
- The section's long explanatory text (beyond trading, why me, unsolved problems) stays in the site; the demo owns only the interactive part.
- Update the site's "How the demo works" and "Cost model" sections. Replace the modelled numbers with "measured live in the panels above", and keep the honesty notes.

## 10. M6 checklist

- [ ] Existing ladder ported to TypeScript, with tests for `canRead`
- [ ] Workers and bus wired; the network panel shows real traffic
- [ ] Server panel shows real bytes, levels and ops
- [ ] Certificate panel with live checks; the Π line on the ladder
- [ ] Cheats wired to workers; each produces the documented failure
- [ ] "Verify this build" shows the real hash
- [ ] Playwright suite (`docs/09` §8) green on desktop and a phone viewport
- [ ] Lighthouse: no third-party requests; CSP passes
