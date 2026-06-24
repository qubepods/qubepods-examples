![qubepods examples — pull a Qube, deploy it, share one URL](./assets/banner.svg)

# qubepods-examples

Example [Qubes](https://qubepods.com) you can read, pull, and deploy.

Each folder is a self-contained [q64](https://q64.dev) Qube with its own
README. They start small and stay honest: every example is real source you
can build with `qube` and deploy to a qubepods project.

## Examples

| Example | What it shows |
|---------|---------------|
| [**twin-counter**](./twin-counter/) | The backend starter. A button and a shared count, built as a **twin** — a frontend wasm that renders, and a backend wasm you write that holds the count in a WASI key-value store (`env.kv`) and serves it over wRPC. |
| [**scene-overlay**](./scene-overlay/) | A QView form floating over a **3D scene** (`scene` viewport, kind 21): a turning cube drawn by the quine engine behind a frosted card with a live counter. No backend — local `state` and an `on_5` press handler. Also **links a second qube** (`color/`) for the swatch colour. The base for QView-widgets-over-3D. |
| [**blackbird**](./blackbird/) | A full chess game you play in the browser, shipped as a **static-asset qube**: `qube.json5` declares `static: { dir: "web" }` and `qube deploy` serves the `web/` folder as-is — no q64 compile, no component, no backend. The engine is a **high-performance Rust chess engine compiled to WebAssembly** (~1500 Elo) that runs **in the browser at near-native speed** (no server), loaded by the page as an *asset* wasm — deployed through the same `qube` tooling as a q64 qube. Shows qubepods is **wasm-native and language-agnostic**: bring any wasm, ship it unchanged. |

## Using an example

Each example is a normal qube. Open your project in **Qubonaut**
(`app.qubepods.com`), clone this repo in its terminal, and from the example's
folder:

```sh
qube run
```

See the example's own README for what it does and which kind of project it
needs (some need a project with the **Backend** switch turned on).

## License

MIT — see [LICENSE](./LICENSE).
