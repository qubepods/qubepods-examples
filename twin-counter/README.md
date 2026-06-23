# twin-counter

The **backend starter** for qubepods: a page with a button and a shared count,
built the way qubepods backends are built — as a **twin**.

A twin is two qubes, two wasm:

- [**`frontend/`**](./frontend) — renders the button and the number (wasm32 →
  WebGPU), and calls the backend.
- [**`backend/`**](./backend) — **you write it.** It keeps the count in a WASI
  key-value store and serves `bump` / `read` to the frontend over wRPC.

The count lives in the backend's key-value store, so it's one number for the
whole project: every frontend that calls the backend reads and writes the same
`"count"`.

## The backend you write

[`backend/backend.q`](./backend/backend.q) — the shared count, in a WASI KV
binding (`env.kv` → `wasi:keyvalue`):

```q64
pub fn bump() -> i64 @kv {
    match env.kv.increment("count", 1) {   // wasi:keyvalue/atomics.increment
        Ok(n)  -> n
        Err(_) -> 0
    }
}

pub fn read() -> i64 @kv {
    match env.kv.increment("count", 0) {   // +0 reads the current value
        Ok(n)  -> n
        Err(_) -> 0
    }
}
```

The backend names no database and no cloud — it asks for one capability,
`@kv`. qubepods binds that `wasi:keyvalue` import to the project's store at
boot. `qube audit` shows the whole surface: `wasi:keyvalue`, nothing else.
`rpc.export: true` in its [`qube.json5`](./backend/qube.json5) serves `bump` and
`read` over wRPC.

## The frontend that calls it

[`frontend/frontend.q`](./frontend/frontend.q) — renders, and calls the backend:

```q64
import counter.{bump, read}              // bound to the backend in qube.json5

state count = 0

fn main @wire {
    count = read()                       // load the shared count, then draw
    paint()
}

pub fn on_press(id: i64) @wire {
    count = bump()                       // add one on the backend; show the total
    paint()
}

fn paint {
    qview.text(40, 56, 0)
    qview.number(40, 120, count)
    qview.button(1, 40, 180, 280, 72, 1)
    qview.present()
}
```

`import counter.{…}` is wired to the backend by the frontend's
[`qube.json5`](./frontend/qube.json5) `rpc.import`. Each call crosses a wire, so
the frontend picks up the `@wire` effect — visible in `qube audit`.

## 1. Create a project with the backend enabled

In the qubepods app, create a project and turn the **Backend** switch **on**:

> **Backend** — provision a database, storage & key-value plus a live
> frontend↔backend connection.

That key-value store is what the backend's `env.kv` binds to, and the one
backend instance per project is why the count is shared. (Chosen once at
creation, can't be flipped later.)

## 2. Run it — in the browser (no desktop needed)

A mobile or iPad is enough. The IDE — **Qubonaut** — runs at `app.qubepods.com`
as an installable PWA, and you're already signed in.

1. Open your backend-enabled project and tap **Edit** to open it in **Qubonaut**.
2. In Qubonaut's terminal, clone this example into your workspace:

   ```sh
   git clone https://github.com/qubepods/qubepods-examples.git
   cd qubepods-examples/twin-counter
   ```

3. Run it:

   ```sh
   qube run
   ```

`qube run` compiles each half to WebAssembly on-device (the q64 compiler is
itself wasm, so it works on iPad Safari): the backend serves `bump`/`read`, and
the frontend renders the screen in the **Preview** pane and calls the backend.

### Prefer a desktop terminal?

Same thing from a laptop — `qube run` is all you need:

```sh
git clone https://github.com/qubepods/qubepods-examples.git
cd qubepods-examples/twin-counter
qube run
```

## 3. How the shared count works

- **The count lives in the backend, in `env.kv`.** One project = one backend
  instance = one `"count"` key. Every frontend that calls `bump`/`read` hits
  the same number.
- **`bump` is atomic.** `env.kv.increment` lowers to
  `wasi:keyvalue/atomics.increment`, so two people tapping at once both get
  counted — neither write is lost. `read` is just a bump of zero.
- **The frontend draws what it last read.** It calls `read()` on load and
  `bump()` on each tap, so it always shows the current shared total when it
  loads or when you tap. (To watch *other* people's taps land live without
  re-tapping, the frontend would subscribe to the backend over the project
  channel — a small step on top of the same two qubes.)

## Files

| File | What it is |
|------|------------|
| [`backend/backend.q`](./backend/backend.q) | The backend you write — the shared count in a WASI KV binding (`env.kv`), served over wRPC. |
| [`backend/qube.json5`](./backend/qube.json5) | Backend manifest: library, `rpc.export`, `@kv`. |
| [`frontend/frontend.q`](./frontend/frontend.q) | The frontend that renders the button + count and calls the backend. |
| [`frontend/qube.json5`](./frontend/qube.json5) | Frontend manifest: application, `rpc.import` of the backend, `@wire`. |
