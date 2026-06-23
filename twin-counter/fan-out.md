# Live fan-out — design note

`twin-counter` is the live version: a tap on any device lights up the number on
all of them, with no polling. This note is the design behind the code in
`backend/` and `frontend/` — grounded in the q64 specs (citations inline), and
explicit about the two spots, marked `HOST SEAM` in the source, where the
language surface meets the platform's twin hosting and isn't fully first-class
yet.

## You don't need a listener — notify rides the RPC channel

q64 RPC is bidirectional and streaming by design, so "push the new count to
every frontend" is the same channel the frontend already talks on, not a second
subsystem:

- **Remote channels** (`rpc.md` §"Remote channels"). `Channel<Tx, Rx>` is a
  bidirectional endpoint, "sugar over a pair of WASIp3 streams — an outbound
  `stream<Tx>` and an inbound `stream<Rx>`." The frontend opens it with
  `connect<…>()`; the backend receives its end via a `@channel_handler` entry
  point (`env.md`). It's iterable: `for n in channel { … }`.
- **Streaming results** (`rpc.md` §"Async results and streaming"). A `pub fn`
  can return a `Stream<T>` directly; it lowers to `wit stream<T>` over wRPC
  "without a polling shim."

## The broadcaster is an actor (the twin)

A bare `@channel_handler` is *per-connection* — it can only talk back to its own
caller. To reach *everyone*, the set of open connections has to live in one
shared, serialized place. q64's name for that is the **actor** (`concurrency.md`
§Actors): "a task with private state and a typed message inbox," serial (one
message at a time = the consistency boundary). The actor owns the subscriber set
and broadcasts; `env.kv` is where the count is durably kept.

```q64
// The twin: owns the live subscriber set; env.kv holds the count.
actor Counter {
    state subs: Vec<Sender<i64, Unbounded>> = Vec.new()

    handle Join(tx: Sender<i64, Unbounded>) @kv {
        tx.send(read())                       // hand the newcomer the current value…
        state.subs.push(move tx)              // …then remember it
    }
    handle Bump @kv {
        let n = bump()                        // env.kv.increment("count", 1)
        for tx in state.subs { tx.send(n) }   // ← the fan-out
    }
}
```

(`actor` / `handle` / `tell`, `channel<T,P>` → `Sender`/`Receiver`,
`Sender.send`, and `env.kv.increment` are all real surfaces — `concurrency.md`
§Actors + §Channels, `env.md` §`env.kv`.)

The frontend holds one channel and renders every value that arrives — the
event-loop form from `env.md`'s channel example, not the `on_press(id)`
callback:

```q64
fn main @wire {
    let twin = connect<counter.join>()             // Channel<i64, Tap>
    spawn { for n in twin { count = n; paint() } }  // live: every broadcast redraws
    // …send a Tap up the same channel on each press…
}
```

## What `emit` puts on the wire: the stream, not the sender

`component: { emit: true }` (set on the backend) emits the qube's public RPC
surface lowered to the canonical ABI. Per `rpc.md` §"Wire encoding," an RPC
signature "may use only **value types**"; the unlowerable set "stays
**process-local**" (closures, faces-as-values, WIT resources — "meaningless on
the far side of a wire").

| Thing | Emitted into the component? |
|---|---|
| `@channel_handler pub fn join(session: Channel<i64, Tap>)` | **yes** — lowers to `stream<i64>` (out) + `stream<Tap>` (in) (`env.md`) |
| `env.kv` import (`wasi:keyvalue`) | **yes** — an import in the world |
| the actor, its `state`, its `Vec<Sender<…>>` subscriber set | **no** — process-local internals |

So the `Sender` you hold to fan out is **local plumbing, off-wire** — and that's
correct, not a gap. What crosses is the channel's send direction, lowered to
`stream<i64>`; `session.send(n)` is the call that writes into that emitted
stream. The boundary is the channel/stream, not the sender.

## The two honest seams

The primitives above are all real q64; two pieces are compositions the specs
don't yet show end-to-end, and both land on the same place — the platform's twin
hosting:

1. **"One twin per project."** For a tap on *any* frontend to reach *all* of
   them, every connection must share one actor instance. That singleton-per-
   project is the twin/DO hosting (`concurrency-model.md`: "twin = the same
   actor shape hosted remotely; subscribers receive diffs"). It's real in the
   platform — the QView POC's Durable Object is exactly this — and the
   first-class q64 sugar for it is `@state(scope)`. The explicit `actor` above is
   the in-language shape; pinning it to one-per-project is the host binding.

2. **Registering each connection's send-half + the tap source.** How the
   per-connection `Channel`'s send side gets handed to the actor's subscriber
   set, and how a qview button press becomes a `Tap` on the channel's inbound
   stream (qview's input is the `on_press` callback, not an event stream), are
   the wiring details to pin down before this compiles.

Bottom line: **yes — build notify into the channel; the actor is the
broadcaster; `emit` carries the stream and (correctly) leaves the sender
process-local.** The remaining work is the twin-hosting binding (seam 1) and the
channel/input wiring (seam 2), both of which are platform-side.
