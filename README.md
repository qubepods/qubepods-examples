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
| [**blackbird**](./blackbird/) | A full chess engine you play in the browser, shipped as a **QubePod** (`qubepod.jsonc`) instead of a `.q` app: precompiled **Rust** source packed as a WIT **component** (`qubepods:blackbird/blackbird`) plus a static `web/` page. The committed wasm runs from a plain checkout. The pattern for bringing an existing native codebase to qubepods. |

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
