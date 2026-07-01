//! api-classic — a classic request/response HTTP API, the whole thing in one
//! `@http_handler`.
//!
//! This is the "backend with real storage" example: it uses all three project
//! stores — key-value, SQLite, and object storage — through the ambient
//! capability faces on `env`, and serves them over HTTP. It's the
//! request/response sibling of `twin-counter` (which serves a live wRPC
//! channel); same manifest-driven model, different entry point.
//!
//! The capability boundary is the point. The handler calls `env.kv`, `env.db`,
//! and `env.blob` — but it never names a namespace, a database, or a bucket.
//! qubepods opens each store and pins it to this qube's identity (org/project/app)
//! at boot (env.md §`env.kv`/`env.db`/`env.blob`), and meters the calls. So the
//! data is the project's, can't reach another tenant, and is billed through the
//! platform — not raw Cloudflare a customer could point anywhere.
//!
//! HTTP entry point: `@http_handler` (env.md §"HTTP service entry point"). The
//! v0 handler is the **sync string shape** — `serve(method, path, body) -> str`
//! — which qubepods runs on a plain Worker (the gate hands in the request
//! strings; the qube returns the response string). Each store call returns a
//! `Result` the handler matches; routing is `method`/`path` on `==`.

@http_handler
pub fn serve(method: str, path: str, body: str) -> str {
    // env.kv — a shared visit counter. `increment` is atomic
    // (wasi:keyvalue/atomics.increment), so concurrent hits never lose one.
    if method == "POST" && path == "/visit" {
        match env.kv.increment("visits", 1) {
            Ok(_)  -> "visited"
            Err(_) -> "kv-error"
        }
    // env.db — a tiny append-and-read log. `setup` creates the table; the
    // dialect is SQLite regardless of the project's storage tier (env.md §`env.db`).
    } else if method == "POST" && path == "/setup" {
        match env.db.execute("CREATE TABLE IF NOT EXISTS notes(id INTEGER PRIMARY KEY, body TEXT)") {
            Ok(_)  -> "ready"
            Err(_) -> "db-error"
        }
    } else if method == "POST" && path == "/note" {
        match env.db.execute("INSERT INTO notes(body) VALUES('hello')") {
            Ok(_)  -> "noted"
            Err(_) -> "db-error"
        }
    } else if method == "GET" && path == "/note" {
        match env.db.query_text("SELECT body FROM notes ORDER BY id DESC LIMIT 1") {
            Ok(Some(s)) -> "note: {s}"
            Ok(None)    -> "no notes"
            Err(_)      -> "db-error"
        }
    // env.blob — stash an opaque object under this project's bucket; never named.
    } else if method == "PUT" && path == "/asset" {
        match env.blob.put("asset", body) {
            Ok(())  -> "stored"
            Err(_)  -> "blob-error"
        }
    } else if method == "GET" && path == "/asset" {
        match env.blob.get("asset") {
            Ok(Some(_)) -> "asset-present"
            Ok(None)    -> "no asset"
            Err(_)      -> "blob-error"
        }
    } else {
        "not found"
    }
}
