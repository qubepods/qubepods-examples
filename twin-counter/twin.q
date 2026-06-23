// The backend half of the twin — the other placement of counter.q's
// `@state(app) count`. It runs server-side as ONE instance for the whole
// project (a Durable-Object-backed actor), owns the app-scoped `count`,
// persists it, and fans each change out to every frontend subscribed to it.
// The frontend never writes the number directly — a tap turns into the `inc`
// command below, the twin applies it to the single shared `count`, and the new
// value is pushed to everyone.
//
// You don't normally hand-write this — `@state(app)` in counter.q is what
// designates it. It's spelled out here so both wasm are visible; the compiler
// splitting one program into the frontend + this twin automatically is being
// built (the QView POC scaffolds it for now).

state count = 0

pub fn inc() {
  count = count + 1
}
