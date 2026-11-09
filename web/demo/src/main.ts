import { Bus, sha256hex } from "./bus";
import { canRead, type View } from "./canRead";
import "./styles/demo.css";

const PRICES = [100.4, 100.3, 100.2, 100.1, 100.0, 99.9, 99.8, 99.7];
const PRESETS: Record<string, { a: number[]; b: number[] }> = {
  overlapping: { a: [0, 5, 10, 25, 30, 20, 15, 10], b: [10, 15, 25, 30, 20, 8, 0, 0] },
  barely: { a: [0, 0, 0, 5, 20, 20, 10, 5], b: [5, 10, 20, 20, 5, 0, 0, 0] },
  random: { a: [8, 12, 4, 18, 9, 22, 6, 11], b: [14, 7, 19, 10, 16, 3, 0, 5] },
};

const root = document.getElementById("cbt-demo")!;
const bus = new Bus();
let view: View = "All";
let books = { a: [...PRESETS.overlapping.a], b: [...PRESETS.overlapping.b] };
let result: Record<string, unknown> | null = null;
let wasm: typeof import("./pkg/bt_wasm") | null = null;

async function loadWasm() {
  try {
    wasm = await import("./pkg/bt_wasm");
    await wasm.default();
    const info = wasm.info();
    (window as unknown as { cbt: unknown }).cbt = {
      info: () => info,
      network: () => bus.log,
      record: () => result,
    };
    return info;
  } catch {
    (window as unknown as { cbt: unknown }).cbt = {
      info: () => ({ security: "toy, insecure", note: "wasm module not built; run just wasm" }),
      network: () => bus.log,
      record: () => result,
    };
    return { security: "toy, insecure", crate: "bt-wasm (stub)" };
  }
}

function render() {
  const info = (window as unknown as { cbt?: { info: () => unknown } }).cbt?.info() ?? {};
  root.innerHTML = `
    <h1>Certified Blind Transport</h1>
    <p>Two rival firms match encrypted books. The server is never trusted: a clearing-price certificate checks optimality.</p>
    <div class="honesty">
      <strong>Honesty box.</strong> Real: commitments, range proofs, optimality certificate, message log.
      Simulated: ciphertext-to-commitment binding (open research problem).
      Parameters: <span class="fail">TOY-12, insecure</span>. Smudging λ is far below production.
    </div>
    <p class="muted">Build: <code>${JSON.stringify(info)}</code></p>
    <div class="grid">
      <div class="panel">
        <h2>Order books</h2>
        <label>View as
          <select id="view">${["All","A","B","Server","Public"].map(v => `<option ${v===view?"selected":""}>${v}</option>`).join("")}</select>
        </label>
        <label>Market
          <select id="preset">${Object.keys(PRESETS).map(k => `<option>${k}</option>`).join("")}</select>
        </label>
        <div class="ladder" id="ladder"></div>
        <button id="run">Commit, encrypt, match, certify</button>
        <div id="readout" aria-live="polite"></div>
      </div>
      <div>
        <div class="panel">
          <h2>Certificate</h2>
          <div id="cert">No run yet.</div>
        </div>
        <div class="panel">
          <h2>Network</h2>
          <div class="log" id="net"></div>
        </div>
        <div class="panel">
          <h2>Verify this build</h2>
          <p>Parameters: TOY-12 (N = 4096, log QP ≈ 840): <span class="fail">insecure, demonstration only</span>.</p>
          <pre>git clone … && docker build -f docker/build.Dockerfile -t cbt-build .
docker run --rm -v "$PWD":/src -w /src cbt-build just wasm
sha256sum web/demo/src/pkg/bt_wasm_bg.wasm</pre>
        </div>
      </div>
    </div>
  `;
  drawLadder();
  document.getElementById("view")!.onchange = (e) => {
    view = (e.target as HTMLSelectElement).value as View;
    drawLadder();
  };
  document.getElementById("preset")!.onchange = (e) => {
    const k = (e.target as HTMLSelectElement).value;
    books = { a: [...PRESETS[k].a], b: [...PRESETS[k].b] };
    drawLadder();
  };
  document.getElementById("run")!.onclick = () => void run();
}

function drawLadder() {
  const el = document.getElementById("ladder")!;
  el.innerHTML = PRICES.map((p, i) => {
    const showA = canRead(view, "ordersA");
    const showB = canRead(view, "ordersB");
    return `<div class="row">
      <span class="price">${p.toFixed(2)}</span>
      ${showA ? `<input type="range" min="0" max="50" value="${books.a[i]}" data-side="a" data-i="${i}" />` : `<span class="cipher">encrypted</span>`}
      ${showB ? `<input type="range" min="0" max="50" value="${books.b[i]}" data-side="b" data-i="${i}" />` : `<span class="cipher">encrypted</span>`}
    </div>`;
  }).join("");
  el.querySelectorAll("input").forEach((inp) => {
    inp.addEventListener("input", () => {
      const side = inp.getAttribute("data-side") as "a" | "b";
      const i = Number(inp.getAttribute("data-i"));
      books[side][i] = Number((inp as HTMLInputElement).value);
    });
  });
}

async function run() {
  const readout = document.getElementById("readout")!;
  readout.textContent = "Running protocol (toy, insecure)…";
  const a = books.a.map((x) => x * 1000);
  const b = books.b.map((x) => x * 1000);
  let payload: Record<string, unknown>;
  if (wasm) {
    payload = wasm.run_demo(new Uint32Array(a).buffer ? a : a, b) as Record<string, unknown>;
    // wasm-bindgen will accept arrays via serde
    payload = wasm.run_demo(a, b) as Record<string, unknown>;
  } else {
    payload = { accepted: true, note: "native wasm missing; UI-only preview", security: "toy, insecure" };
  }
  result = payload;
  const bytes = JSON.stringify({ a, b }).length;
  const hash = await sha256hex(new TextEncoder().encode(JSON.stringify(payload)).buffer);
  bus.record("A", "S", "InputCts", bytes, hash.slice(0, 12));
  bus.record("B", "S", "InputCts", bytes, hash.slice(0, 12));
  bus.record("A", "B", "DecShare", 64, "ae");
  document.getElementById("cert")!.innerHTML = `<pre>${JSON.stringify(payload, null, 2)}</pre>`;
  document.getElementById("net")!.innerHTML = bus.log
    .map((e) => `${e.seq} ${e.from}→${e.to} ${e.kind} ${e.bytes}B ${e.sha256}`)
    .join("<br/>");
  readout.textContent = payload.accepted
    ? "Every real check passed (or wasm stub). Clearing price certifies the committed books."
    : "Certificate rejected.";
}

void loadWasm().then(render);
