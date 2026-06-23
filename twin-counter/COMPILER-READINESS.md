# twin-counter — compiler readiness & the path forward

`backend.q` is written against the **target** q64 language. Not all of it
compiles through the q64 codegen yet. This note is the honest map: what runs
today, what landed recently, and the concrete path to compile the backend
*as written* — verified against `q64 emit` + the wasmtime host, not aspirational.

It answers one question in particular: **how does the frontend↔backend wire
actually get built?** (Short version: not a `connect()` builtin — q64's
cross-qube story is the component model + wRPC, driven by the `rpc` block in
`qube.json5`. See `spec/rpc.md` in the q64 repo.)

## Status legend

- ✅ **runs** — compiles through `q64 emit` and executes on the host.
- 🟡 **partial** — a narrower form runs; the twin uses a richer one.
- ⬜ **gap** — not yet lowered by codegen.

## The backend, construct by construct

| `backend.q` uses | Maps to | Status |
|---|---|---|
| `match env.kv.increment(…) { Ok(n) -> …, Err(_) -> … }` | `env.kv` → `wasi:keyvalue`, returns `Result<i64>` | 🟡 **keyless** `env.kv.increment(delta)` runs; the **keyed** `("count", 1)` form is a gap |
| `Vec.new()` + `.push` + `for tx in …` | growable Vec + iteration | ✅ runs (the empty growable + fan-out loop) |
| `spawn { … }` (fire-and-forget) | cooperative task, eager v0 | ✅ runs (main / `for` / `if` bodies) |
| method-style actor (`var c = C{}`, `c.m()`) | actor = self-taking fns | ✅ runs |
| `Vec<Sender<…>>` (a Vec of senders) | Vec of a non-i64 element | 🟡 i64 Vec runs; non-scalar element types are the v0 boundary |
| `actor Counter { state subs … handle Join(tx) handle Bump }` | **message-style** actor: typed payload messages + `tell` + `C.spawn()` | ⬜ gap (method-style is the working alternative) |
| `@channel_handler pub fn join(session: Channel<i64, Tap>)` | rpc channel entry point | ⬜ gap (the channel-handler attribute + `Channel<S,R>` param) |
| `channel<i64, Unbounded>()`, `tx.send`, `rx.recv` | in-process channel | ✅ runs |
| `for n in rx` / `for _tap in session` | for-in over a channel/stream | 🟡 for-in over a Vec runs; over a live channel/stream is a gap |
| `move tx` | move semantics | ⬜ gap |

### What landed recently (q64-lang/q64)

Four codegen slices, each tested end-to-end on the wasmtime host:

1. **`env.kv.increment`** → an `env.kv_increment` host import (`wasi:keyvalue`),
   marks the fn `@kv`. Returns a boxed **`Result<i64,i64>`**, so the twin's
   `match … { Ok(n) -> n, Err(_) -> 0 }` shape compiles and runs.
2. **`Vec.new()`** — the empty growable, so a subscriber set is built at runtime
   (`var subs = Vec.new(); subs.push(x); for tx in subs { … }`).
3. **fire-and-forget `spawn { … }`** — the body runs eagerly for effect, in
   `main`, a `for` body, or an `if` body.
4. (`env.kv` + `Vec.new` together cover the backend's persistence + fan-out
   *data flow*; the actor/channel/rpc *control flow* is the remaining work.)

## The cross-qube wire — the actual mechanism

There is **no `connect()` builtin** (it resolves to `NameNotFound`), and a bare
`import some.qube` is `UnknownModule`. q64 qubes talk over the **component model
+ wRPC**, configured in `qube.json5` — exactly as `examples/rpc-server` +
`examples/rpc-client` in the q64 repo demonstrate today:

**Export side (the backend, `type: "library"`):**

```jsonc
// backend/qube.json5  (already correct)
component: { emit: true, world: "counter-backend" },
rpc:       { export: true },
```

A qube's `pub fn` surface **is** its served world. `q64 show world --qube
backend.q` already synthesizes the WIT for a value-typed `pub fn` — e.g. a
plain `pub fn` over `i64`/`str`/`Result<value, value>` lowers to the canonical
ABI and travels on the wire unchanged (`spec/rpc.md` §"Wire encoding"). The
constraint that matters: **RPC signatures use value types only** — no `ref T`,
no resources, no closures (`RPC010`). The counter's payloads (`i64`, `Tap`,
streams of `i64`) are all value types, so they qualify.

**Import side (the frontend, `type: "application"`):**

```jsonc
// frontend/qube.json5  (already correct)
rpc: { import: { counter: "wrpc://counter-backend" } },
```

```q64
import counter.{join}        // the remote's re-exported surface
fn main @wire { … }          // any call into it carries @wire
```

The imported world is the *same artifact* the backend built with
`component.emit: true` — no IDL authored on either side. Every call across it
picks up the **`@wire`** effect (propagates up to `main`, implies `@io`,
forbidden under `@realtime`). On qubepods the endpoint doubles as a wRPC server:
a `--component` qube deployed with `rpc.export: true` is reachable as both
`wasi:http` and wRPC at the same address (`spec/rpc.md` §"Addressing").

**Two seams are runtime, not compiler** (the `HOST SEAM` markers in
`backend.q`, and `fan-out.md`):

- the **transport** byte-mover lives in `runtime/<host>/` (WebTransport in the
  browser, QUIC/TCP native) — the same place WASI lowering lives;
- the **per-project singleton twin** (`let twin = Counter.spawn()`) is a host
  concern (a qubepods Durable Object, one instance per project); the planned
  first-class spelling is `@state(scope)`.

## Path forward — ordered by leverage

To compile `backend.q` *as written*, in dependency order:

1. **Keyed `env.kv.increment(key, delta)`** — extend the v0 keyless import to
   take a `str` key (ptr,len) + i64 delta. Small: the host import gains two
   args; the host store becomes a `map<str,i64>`. Unblocks the twin's exact
   `env.kv` calls.
2. **Message-style actors** — `handle Msg(payload) { … }` + `actor.tell(Msg(x))`
   + `Actor.spawn()`, lowering the message enum + a serial dispatch. `check.zig`
   already validates this surface (`handle Get -> i64`); codegen is the gap.
   This is the largest language piece.
3. **`@channel_handler` + `Channel<S, R>`** — the rpc channel entry point: a
   `pub fn` taking a bidirectional channel, served over wRPC. Rides #2 and the
   component/wRPC export already wired in the manifest.
4. **for-in over a live channel/stream** (`for n in rx`) and **`move`** — the
   remaining surface the handler body uses.

A **compiles-today** backend (keyless `env.kv`, method-style actor, i64 Vec,
fire-and-forget `spawn`) already exercises the persistence + fan-out data flow
end-to-end on the host; it's the honest floor under the target `backend.q`
above. Items 1–4 close the distance to the file as written.
