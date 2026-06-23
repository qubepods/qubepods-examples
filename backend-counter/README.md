# backend-counter

The **backend starter** for qubepods: one page, one button, one number —
shared by everyone.

Every click bumps a counter that lives in the project's backend, so the
number is the same for every visitor. Open the page on your phone and your
laptop at once: click on one, watch it move on the other.

It's about 100 lines of [q64](https://q64.dev) in
[`src/main.q`](./src/main.q) — a single `wasi:http` handler that serves its
own page *and* keeps the count. No framework, no build step, no separate
backend service to wire up.

```
GET  /            → the HTML page (button + live count)
GET  /api/count   → the current count, as plain text
POST /api/click   → add one atomically, return the new count
```

---

## What this teaches

1. **Turning on a backend for a project** — the one switch that gives a
   project persistent, shared state.
2. **A qube that paints its own UI and keeps its own state** — frontend and
   backend are the same artifact, with the count reached through one
   capability (`env.kv`) and nothing else.

---

## 1. Create a project with the backend enabled

In the qubepods app, create a new project. On the create form there's a
**Backend** switch:

> **Backend** — provision a database, storage & key-value plus a live
> frontend↔backend connection.

Turn it **on**. This is chosen once at creation and can't be flipped later —
a project either has a backend twin or it doesn't, because turning one on
later would be a data migration, not a toggle.

A backend-enabled project is anchored by a single **backend instance**. That's
the whole trick behind "everyone shares this count": there is exactly one
backend per project, it has its own durable store, and every request to the
project's endpoint lands on it.

Without a backend, a project is static/stateless — fine for a brochure site,
but two visitors would never see a shared number.

## 2. Run it — in the browser (no desktop needed)

You don't need a desktop, a local toolchain, or even a laptop. qubepods is
browser-first: a mobile or iPad is enough. The IDE — **Qubonaut** — runs at
`app.qubepods.com` as an installable PWA, and you're **already signed in**
there, so there's no login step and no token to save.

1. Open your backend-enabled project from step 1 and tap **Edit** to open it
   in **Qubonaut**.
2. In Qubonaut's terminal, clone this example into your workspace:

   ```sh
   git clone https://github.com/qubepods/qubepods-examples.git
   cd qubepods-examples/backend-counter
   ```

   Qubonaut has a built-in terminal with `git` (it talks to GitHub over the
   API, so it works on iPad too — no desktop git needed).
3. Press **Run**.

That's it. Pressing **Run** builds the qube to WebAssembly on-device (the q64
compiler itself is wasm, so it works on iPad Safari) and deploys it onto the
project. No `qube build`, no `qube pod login`, no `qube pod deploy` — those are
the desktop-CLI spellings of what the **Run** button does for you.

Your project already has its address on **`*.qubepod.app`**, shown on the
project page.

> **Public access is the part we're still building.** Pressing **Run** builds
> and deploys the qube and gives you a live preview of the page. Wiring the
> project's `*.qubepod.app` route so *anyone* on the open internet reaches your
> backend — that last hop is in active development, and **this example is the
> one we're using to get there.** When it lands, the steps above don't change:
> same Run button, same route, now public.

### Prefer a desktop terminal?

Everything above also has a CLI form, for when you're working from a laptop
instead of the dashboard. Clone the repo and build/run it against any WASIp3
host:

```sh
git clone https://github.com/qubepods/qubepods-examples.git
cd qubepods-examples/backend-counter
qube build --component
wasmtime serve target/<host>/backend-counter.component.wasm   # open http://localhost:8080
```

Run this way the count lives in the host's own store for the life of the
process — just you, and it resets when you stop the server. Deploying to a
project from the CLI (`qube pod login` then `qube pod deploy`) is the desktop
counterpart to the **Run** button, and rides the same in-progress public-route
work noted above.

## 3. How the shared count works

The handler is the entire app:

```q64
@http_handler
pub fn handle(req: Request) -> Response @kv {
    match (req.method(), req.path()) {
        ("GET",  "/")          -> Response.html(page())
        ("GET",  "/api/count") -> Response.ok("{bump(0)}")
        ("POST", "/api/click") -> Response.ok("{bump(1)}")
        _                      -> Response.not_found()
    }
}

fn bump(delta: i64) -> i64 @kv {
    match env.kv.increment(COUNTER_KEY, delta) {
        Ok(n)  -> n
        Err(_) -> 0
    }
}
```

- The count is a single key in the project's key-value store, reached through
  the `env.kv` capability.
- `env.kv.increment(key, delta)` is **atomic**. Two people clicking at the
  same moment both get counted — the host applies both bumps without losing
  one. Reading the current value is just a bump of `0`.
- The page (served by `GET /`) is plain HTML + a few lines of JavaScript: on
  load it fetches `/api/count`, the button `POST`s to `/api/click`, and it
  polls every ~1.5s so clicks from *other* people show up on your screen too.

Because there's one backend per project and one count inside it, every browser
pointed at the endpoint is reading and writing the same number.

## Status & what's evolving

This example is deliberately at the leading edge — it's both a demo and a
spec for the pieces being finished. As of now:

- **Working:** the q64 source (one `@http_handler`, `env.kv`) and component
  emission to `wasi:http/proxy`; building it **on-device in the browser** (the
  q64 compiler is itself wasm, iPad included) and serving it under a WASIp3
  host. The backend-enabled project + its single per-project backend store
  exist, and the project's `*.qubepod.app` route is shown on the project page.
- **Being wired:** the **Run → public** last hop — deploying from the browser
  and serving the project's `*.qubepod.app` route to the open internet so every
  visitor hits the same backend; and binding the component's `wasi:keyvalue`
  import to *that* project's store at boot (per-project, per-identity). Until
  those land, **Run** builds, deploys, and previews for you, and `env.kv`
  resolves to the host's default store. **This example is the thing we're using
  to finish that hop.**
- **On the roadmap:** a raw-SQL face (`env.db` → `wasi:sql`) over the same
  per-project store, for examples that want tables and queries rather than a
  single counter; and richer `wasi:http` `Request`/`Response` builders
  (`req.method()`, `req.path()`, `Response.html`) firming up as standard
  surface.

If you're reading the q64 source and a constructor looks slightly ahead of
what your toolchain ships, that's expected — this folder is one of the things
driving those surfaces to completion.

---

## Files

| File | What it is |
|------|------------|
| [`src/main.q`](./src/main.q) | The whole app — handler, counter, and page. |
| [`qube.json5`](./qube.json5) | Manifest: application type, `wasi:http/proxy` component, `@kv` capability. |
