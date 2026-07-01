# API-Classic

A **standard Cloudflare Worker** — a plain `export default { fetch }` module — that
qubepods runs on **Workers for Platforms**, using **KV + SQLite + R2 with no
`wrangler.jsonc`**. It proves the platform's auto-binding: you declare storage as
logical *imports* in the manifest, and qubepods injects the real bindings into
`env` at deploy time.

Not every backend has to be a q64 component. An API-only backend can be an
ordinary JS/TS worker, deployed straight to WfP — this is that path.

## What it shows

The worker never declares a single Cloudflare binding, id, or namespace. It just
reads three names off `env`:

| `env` binding | Storage | Backed by | Manifest import |
|---|---|---|---|
| `env.KV` | Key-value | this **project's Durable Object**, via the platform KV gateway | `imports.cache` |
| `env.DB` | SQLite | a dedicated **Cloudflare D1** (`tier: "replicated"`) | `imports.database` |
| `env.BUCKET` | Object store | this **project's R2 bucket** | `imports.storage` |

qubepods reads [`qubepod.jsonc`](./qubepod.jsonc)'s `imports` block, provisions the
D1 + R2 (idempotently, once per project), and stamps the bindings into the WfP
script upload — including the `KV` service binding with the project scope in its
props, so the standard `env.KV.get/put/list` surface works from plain JS without
leaking across tenants. One store of each **per project**, under **fixed reserved
names** — which is why a second app in the same project that declares the same
imports binds the *same* stores.

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

`imports.database[].tier` picks the storage class (pinned at project creation,
immutable):

- **`light-fast`** — SQLite in the project Durable Object; zero-provision, no
  account cap. **q64-only** (SQL over the DO needs the q64 host adapter), so a
  classic JS worker can't bind it.
- **`replicated`** *(this example)* — a dedicated **Cloudflare D1**: read replicas
  + 30-day time travel. Natively bindable by a JS worker.
- **`external`** — bring-your-own Postgres/MySQL/libSQL via Hyperdrive (needs
  `engine` + a `secret:` connection ref).

## Deploy

You need a qubepods project and a **deploy-scoped project token** (mint one in the
console at `app.qubepods.com`; it's pinned to its project).

1. Set `project` in [`qubepod.jsonc`](./qubepod.jsonc) to your project slug.
2. Deploy the bundle (`qubepod.jsonc` + `src/`) to the API:

```sh
export QUBEPODS_TOKEN=qube_…          # your deploy token
./deploy.sh                            # or the raw curl below
```

```sh
# What deploy.sh does — zip the manifest + module, POST to /api/deploy:
( cd api-classic && zip -qr /tmp/api-classic.zip qubepod.jsonc src )
curl -fsS -X POST "https://api.qubepods.com/api/deploy" \
  -H "Authorization: Bearer $QUBEPODS_TOKEN" \
  -F "environment=production" \
  -F "bundle=@/tmp/api-classic.zip" -w '\nHTTP %{http_code}\n'
# 201 → { "ok": true, "hostname": "api-classic.qubepod.app", "appUrl": "https://api-classic.qubepod.app/", … }
```

Open the returned URL and the test page runs itself.

## What comes next — API-Twin

A follow-up **API-Twin** example will be a *second app in the same project*: a
front-end wasm that renders a **QView**, and a back-end wasm that binds the **same**
`env.KV` / `env.DB` / `env.BUCKET`. Because storage is one-per-project under
reserved names, both apps read and write the **same stored stats** — that's the
shared-state story this example sets up.
