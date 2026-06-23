//! backend — the twin you write.
//!
//! It keeps the shared count in a WASI key-value store and serves two calls to
//! the frontend over wRPC: `bump` and `read`.
//!
//! `env.kv` is the WASI key-value capability (it lowers to `wasi:keyvalue`).
//! qubepods binds it to the project's key-value store at boot, so `"count"` is
//! one shared key for the whole project — every frontend that calls this
//! backend reads and writes the same number.
//!
//! `rpc.export: true` in qube.json5 serves the public functions below over
//! wRPC, so the frontend can call them like ordinary functions.

// Add one to the shared count and return the new total. `increment` lowers to
// wasi:keyvalue/atomics.increment, so two taps at the same instant both land —
// the store applies them atomically and neither is lost.
pub fn bump() -> i64 @kv {
    match env.kv.increment("count", 1) {
        Ok(n)  -> n
        Err(_) -> 0
    }
}

// Read the current total without changing it — a bump of zero. A key that was
// never set reads as 0.
pub fn read() -> i64 @kv {
    match env.kv.increment("count", 0) {
        Ok(n)  -> n
        Err(_) -> 0
    }
}
