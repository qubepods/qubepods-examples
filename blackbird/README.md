# blackbird — a chess engine, deployed as a QubePod

A full-strength chess engine you can play in the browser. The board, the
rules, and the search are all **Blackbird** — a Rust engine (magic bitboards,
transposition table, quiescence search, killer/history move ordering,
aspiration windows) — compiled to WebAssembly and shipped as a **QubePod**.

This is the example that shows the *other* shape of a qube: not a `.q`
application built on-device, but **precompiled native source (Rust) packed as a
WIT component plus a static web page**. The wasm is committed (~130 KB browser
build, ~170 KB component), so the page runs from a plain checkout — and `qube
pod deploy` ships it to a qubepods project as one URL.

## What it shows

- **A `qubepod.jsonc` deploy manifest** instead of `qube.json5`. A QubePod
  bundles a WIT **component** (`component/blackbird.wasm`, world
  `qubepods:blackbird/blackbird`) and an **assets directory** (`web/`). This is
  how you bring an existing native codebase to qubepods without rewriting it in
  q64.
- **Two builds of one engine core.** The same Rust crate compiles twice:
  - `web/engine/blackbird.wasm` — the browser build (`wasm32-unknown-unknown`,
    hand-rolled ABI, no bindgen) the page's Web Worker runs.
  - `component/blackbird.wasm` — a real WIT component (`wasm32-wasip2`,
    `set-position` / `best-move` / `legal-moves` / `apply-move` / `fen`) for
    component-model hosts.
- **The engine is the single source of truth for rules.** The page never
  re-implements chess: after every move the worker replies with the full state
  (FEN, legal moves, check/mate), so the UI just renders it.

## Run it locally

The page fetches wasm, so it needs an HTTP server (not `file://`):

```sh
python3 -m http.server -d web 8080
# open http://localhost:8080
```

You play white; the engine answers automatically. **Engine move** makes the
engine move for whichever side is to play, so you can also play black or let it
play itself.

## Deploy it to a qubepods project

With the `qube` CLI (built from [q64-lang/q64](https://github.com/q64-lang/q64)):

```sh
QUBEPODS_TOKEN=… qube pod deploy --url https://api.qubepods.com
```

`scripts/deploy.sh [environment]` is a curl fallback — it packs the same bundle
(`qubepod.jsonc` + `component/blackbird.wasm` + `web/`) and POSTs it to
`api.qubepods.com/api/deploy`.

`web/` is also a self-contained static site: any static host serves it as-is.

## Architecture

The engine's UCI boundary became a `postMessage` protocol:

```
index.html / app.js  --postMessage-->  worker.js  ----->  blackbird.wasm
   (render + clicks)                  (owns state)        (rules + search)
```

The search runs in the worker, off the main thread — depth 7 answers a
middlegame position in roughly 300 ms.

- Square indexing is a1 = 0 … h8 = 63 (`rank · 8 + file`) end to end: engine,
  wasm ABI, and board UI all agree.
- The browser build is single-threaded; the engine's Lazy SMP stays off (wasm
  has no `std::thread`). Searches are fixed-depth.

## Build / test

```sh
cargo test --workspace      # engine + wrapper tests
node scripts/smoke.mjs      # drives the shipped wasm through the worker's calls
./scripts/build-wasm.sh     # rebuild both wasm (needs the wasm32 targets)
```

The committed wasm only needs rebuilding when `engine/`, `wasm/`, or
`component/` change. One local patch lives in `engine/src/search.rs`: a
`wasm32-unknown-unknown` clock shim, because `std::time::Instant` aborts in the
browser.

## Layout

| Path | What it is |
|------|------------|
| `qubepod.jsonc` | The QubePod deploy manifest — component + `web/` assets. |
| `engine/` | The Rust engine core (magic bitboards, TT, quiescence search). |
| `wasm/` | The `bb_*` browser web-API crate → `web/engine/blackbird.wasm`. |
| `component/` | The WIT component crate + `wit/blackbird.wit` → `component/blackbird.wasm`. |
| `web/` | The deployable page: board UI (`app.js`), engine Web Worker (`worker.js`), committed wasm. |
| `training/` | NNUE weights the engine crate embeds by relative path. |
| `scripts/` | `build-wasm.sh`, `smoke.mjs`, `deploy.sh`. |

> The engine core is vendored from the Rust Blackbird engine. NNUE evaluation is
> currently dead code — search evaluates through the PST path only, and the
> linker drops the embedded weights (which is why the wasm is small).
