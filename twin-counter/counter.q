//! counter — a shared counter, built as a twin.
//!
//! You write one thing: a screen with a button and a number. The number is
//! marked `@state(app)` — "app scope" — so it doesn't live in this browser,
//! it lives in the project's shared backend **twin**. Everyone looking at the
//! screen reads the same number, and a tap from any device bumps it for
//! everyone.
//!
//! This one program becomes **two wasm**:
//!
//!   • a FRONTEND wasm that renders this screen (WebGPU), one per viewer, and
//!   • a BACKEND wasm — the twin — that owns `count` and fans every change out
//!     to all the frontends watching it.
//!
//! The frontend is what you see; the twin is what makes the count shared. See
//! `twin.q` for what the twin runs (the other placement of the `@state(app)`
//! line below), and `README.md` for how the two are wired.

screen Counter {
  @state(app) count = 0           // app scope → one shared number, kept in the twin

  draw {
    text("Everyone shares this count")
    number(count)                 // reading `count` here SUBSCRIBES this screen to it
    button("Click me") on_press {
      count = count + 1           // writing `count` fans a diff out to every subscriber
    }
  }
}
