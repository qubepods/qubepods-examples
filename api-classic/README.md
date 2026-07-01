# API-Classic

A **classic HTTP API worker** — a plain `export default { fetch }` module (no wasm
on the server) that qubepods runs and gives a per-qube HTTPS endpoint. It uses
**KV + SQLite + R2** with **no ids, no wrangler, no config** — the bindings come
from the manifest.

Not every backend has to be a q64 component. An API-only backend can be an
ordinary JS/TS worker — this is that path. (The companion **twin** example is the
other shape: a wasm client + a wasm backend.)

## Run and deploy — the same two commands as every example

From the qubepods shell (the Qubonaut terminal), in this directory:

```
qube run       # serve it locally with the three stores bound
qube deploy    # ship it to qubepods → https://<name>.qubepod.app
```

That's the whole interface. There is no `qubepod.jsonc`, no `wrangler.jsonc`, no
deploy script — just [`qube.json5`](./qube.json5) and [`src/index.js`](./src/index.js).

## What it shows

The worker never declares a single binding, id, or namespace. It just reads three
names off `env`; the manifest's `env` block is what wires them up:

| `env` binding | Storage | Backed by | Manifest |
|---|---|---|---|
| `env.KV` | Key-value | this **project's Durable Object**, via the KV gateway | `env.kv` |
| `env.DB` | SQLite | the **same project Durable Object's SQLite** (light-fast), via the SQL gateway | `env.db` |
| `env.BUCKET` | Object store | this **project's R2 bucket** | `env.blob` |

qubepods reads the [`qube.json5`](./qube.json5) `env` block, opens each store pinned
to this qube's identity, and injects it under a fixed reserved name — including the
`env.KV`/`env.DB` gateways with the project scope in their props, so the standard
`env.KV.get/put/list` and `env.DB.exec/query/first` surfaces work from plain JS
without leaking across tenants. One store of each **per project**, under **fixed
reserved names** — which is why the twin app in the same project binds the *same*
stores and shares this exact data.

## The test page

`GET /` is served **by the worker** (from the API, not as a static asset). It's a
small dashboard that calls the JSON endpoints and shows, per binding, whether it
was injected and whether a real round-trip works:

- `GET /api/health` — which bindings the platform injected.
- `GET /api/tests` — a real round-trip per binding:
  - **KV** — bump a shared `visits` counter, then `put`/`get`/`list` a scratch key.
  - **DB** — `CREATE TABLE IF NOT EXISTS`, insert a row, `SELECT count(*)`.
  - **R2** — `put`/`get` the shared `stats/last-run.json`, `list` the prefix.
- `GET /api/stats` — the shared stats (KV visits, DB event count, R2 last-run).

A missing binding is reported as *skipped*, so the page is honest about exactly
what the platform wired up.

## The SQLite tiers

`env.db.tier` picks the storage class (pinned at project creation, immutable):

- **`light-fast`** *(this example)* — SQLite in the project Durable Object; the
  SAME durable that backs `env.KV`, reached via the SQL gateway. Zero-provision,
  no account cap, and **shared with the twin**: a q64 component and this classic
  worker in the same project read and write one database.
- **`replicated`** — a dedicated **Cloudflare D1**: read replicas + time travel.
- **`external`** — bring-your-own Postgres/MySQL via Hyperdrive (needs `engine` +
  a `secret:` connection ref).

## The shared data — with the twin

Storage is one-per-project under reserved names, so the **twin** example (a
second app in the same project) binds the **same** `env.KV` / `env.DB` /
`env.BUCKET`. This classic worker writes the stats over plain HTTPS; the twin
reads and renders them live. Same project, same durable, same tables — that's the
shared-state story this example sets up.
