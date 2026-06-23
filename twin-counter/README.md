# twin-counter

The **backend starter** for qubepods: a page with a button and a number,
shared by everyone — built the way qubepods backends are built, as a **twin**.

You write one screen. The number on it is `@state(app)` — app scope — so it
doesn't live in your browser, it lives in the project's shared backend. Open
the page on your phone and your laptop at once: tap on one, watch it climb on
the other.

## One program, two wasm

A twin is a single program that runs as **two** WebAssembly modules:

- a **frontend** wasm that renders the screen (WebGPU) — one per viewer, and
- a **backend** wasm — the *twin* — that owns the shared number and pushes
  every change out to all the frontends watching it.

You author one program — the screen. It runs in **two places**: a frontend
instance that renders it, and a backend **twin** instance that holds the
`@state(app)` for everyone. Same program, placed twice.

**[`counter.q`](./counter.q)** — what you write (the frontend):

```q64
screen Counter {
  @state(app) count = 0           // app scope → one shared number, kept in the twin

  draw {
    text("Everyone shares this count")
    number(count)                 // reading count SUBSCRIBES this screen to it
    button("Click me") on_press {
      count = count + 1           // writing count fans a diff out to every subscriber
    }
  }
}
```

**[`twin.q`](./twin.q)** — what the backend twin runs (the other placement):

```q64
state count = 0

pub fn inc() {
  count = count + 1
}
```

The backend half is spelled out here so you can see both wasm. In the twin
model you don't normally write it — the `@state(app)` line *is* the backend;
having the compiler split your one program into the frontend + this twin
automatically is the part being built (today the POC scaffolds it).

`state count = 0` (local, this browser) vs `@state(app) count = 0` (shared, in
the twin) is the whole distinction. The `@` means *synced*; `(app)` means *one
singleton for everyone*. No sockets, no fetch, no API to wire — reading the
name subscribes, writing it broadcasts.

## 1. Create a project with the backend enabled

In the qubepods app, create a new project and turn the **Backend** switch
**on**:

> **Backend** — provision a database, storage & key-value plus a live
> frontend↔backend connection.

That live frontend↔backend connection *is* the twin channel. A backend-enabled
project is anchored by a single backend instance, so the app-scoped twin has
exactly one home and everyone's frontend talks to it. (Chosen once at creation,
can't be flipped later.)

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

`qube run` compiles the frontend to WebAssembly on-device (the q64 compiler is
itself wasm, so it works on iPad Safari) and renders the screen live in the
**Preview** pane. The `@state(app)` count is served by the project's twin, so
the number is the same on every device pointed at the project.

### Prefer a desktop terminal?

Same thing from a laptop — `qube run` is all you need:

```sh
git clone https://github.com/qubepods/qubepods-examples.git
cd qubepods-examples/twin-counter
qube run
```

## 3. How the shared count works

The twin is an actor with one job: hold `count` and keep everyone in sync.

- **One instance per project.** The app-scoped twin has a single backend home,
  so there's exactly one `count`. Every frontend reads and writes that one
  number.
- **Read = subscribe.** Drawing `number(count)` subscribes the screen to the
  twin's `count`. When it changes — from *anyone* — that screen redraws.
- **Write = broadcast.** `count = count + 1` in `on_press` becomes the `inc`
  command to the twin; the twin applies it to the single shared `count` and
  fans the new value out to every subscriber.

So a tap on one device lights up the number on all of them — that's the twin,
and it's the same shape every qubepods backend uses.

## Files

| File | What it is |
|------|------------|
| [`counter.q`](./counter.q) | The frontend screen you write — a button, a number, one `@state(app)`. |
| [`twin.q`](./twin.q) | The backend twin, generated from `@state(app)` — owns the shared `count`. |
| [`qube.json5`](./qube.json5) | Manifest: the frontend application qube. |
