# twin-counter

The **backend starter** for qubepods: a page with a button and a shared count,
built the way qubepods backends are built — as a **twin**.

A twin is two qubes, two wasm:

- [**`frontend/`**](./frontend) — renders the button and the number (wasm32 →
  WebGPU), and holds one live channel to the backend.
- [**`backend/`**](./backend) — **you write it.** It keeps the count in a WASI
  key-value store and fans every change out to all the frontends over a wRPC
  channel — so a tap on any device updates them all.

The count lives in the backend's key-value store, so it's one number for the
whole project; the backend is a single instance, so it can hold the set of
connected frontends and broadcast to them.

## The backend you write

[`backend/backend.q`](./backend/backend.q) — the count in a WASI KV binding
(`env.kv` → `wasi:keyvalue`), and a q64 **actor** that owns the subscriber set
and fans out:

```q64
actor Counter {
    state subs: Vec<Sender<i64, Unbounded>> = Vec.new()

    handle Join(tx: Sender<i64, Unbounded>) @kv {
        tx.send(read())                       // give the newcomer the current value…
        state.subs.push(move tx)              // …then remember it
    }
    handle Bump @kv {
        let n = bump()                        // env.kv.increment("count", 1)
        for tx in state.subs { tx.send(n) }   // ← the fan-out
    }
}

@channel_handler
pub fn join(session: Channel<i64, Tap>) @wire {
    let (tx, rx) = channel<i64, Unbounded>()
    twin.tell(Join(move tx))                  // register this frontend
    spawn { for n in rx { session.send(n) } } // push the twin's updates out to it
    for _tap in session { twin.tell(Bump) }   // forward its taps to the twin
}
```

The backend names no database and no cloud — it asks for `@kv` (the WASI
key-value store qubepods binds to the project) and `@wire` (the channel it
serves). `qube audit` shows exactly that. `component.emit: true` +
`rpc.export: true` ([`qube.json5`](./backend/qube.json5)) serve the channel; the
`Sender` set stays process-local (only the channel's `stream<i64>` crosses the
wire — see [`fan-out.md`](./fan-out.md)).

## The frontend

[`frontend/frontend.q`](./frontend/frontend.q) — renders, and holds one channel
to the twin:

```q64
import counter.{join}

state count = 0

fn main @wire {
    let twin = connect<counter.join>()              // Channel<Tap, i64>
    spawn { for n in twin { count = n; paint() } }  // live: redraw on every broadcast
    for _press in presses() { twin.send(Tap) }      // each press → a tap up the channel
}

fn paint {
    qview.text(40, 56, 0)
    qview.number(40, 120, count)
    qview.button(1, 40, 180, 280, 72, 1)
    qview.present()
}
```

`connect<counter.join>()` opens the dual end of the backend's channel (the name
`counter` is bound by the frontend's [`qube.json5`](./frontend/qube.json5)
`rpc.import`). The call carries `@wire`, visible in `qube audit`.

> Two pieces are platform-side and marked `HOST SEAM` in the source: making the
> `actor` one instance per project (the twin/Durable-Object hosting; sugar is
> `@state(scope)`), and turning a qview press into a channel `Tap`. See
> [`fan-out.md`](./fan-out.md).

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
itself wasm, so it works on iPad Safari): the backend serves its channel, and
the frontend renders the screen in the **Preview** pane and connects to it.

### Prefer a desktop terminal?

Same thing from a laptop — `qube run` is all you need:

```sh
git clone https://github.com/qubepods/qubepods-examples.git
cd qubepods-examples/twin-counter
qube run
```

## 3. How the shared count works

- **The count lives in the backend, in `env.kv`.** One project = one backend
  instance = one `"count"` key, kept in the WASI key-value store. `bump` is
  atomic (`wasi:keyvalue/atomics.increment`), so two taps at once both land.
- **The backend holds the connections and broadcasts.** Because it's a single
  instance, the `actor` can keep the set of connected frontends. On a tap it
  bumps the count and `send`s the new value down every channel — the fan-out.
- **The frontend renders every value it receives.** It opens one channel,
  `for n in twin { … }` redraws on each broadcast, and sends a `Tap` up on each
  press. So a tap on one device lights up the number on all of them, live —
  no polling, no re-tapping.

## Files

| File | What it is |
|------|------------|
| [`backend/backend.q`](./backend/backend.q) | The backend you write — count in a WASI KV binding (`env.kv`), an actor that fans out over a wRPC channel. |
| [`backend/qube.json5`](./backend/qube.json5) | Backend manifest: library, `component.emit`, `rpc.export`, `@kv` + `@wire`. |
| [`frontend/frontend.q`](./frontend/frontend.q) | The frontend — renders the button + count and holds one channel to the twin. |
| [`frontend/qube.json5`](./frontend/qube.json5) | Frontend manifest: application, `rpc.import` of the backend, `@wire`. |
