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
