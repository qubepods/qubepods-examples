//! frontend — the twin half that renders.
//!
//! Draws a button and the count, and calls the backend over wRPC to read and
//! bump the shared number. Compiles to wasm32, renders via WebGPU (qview).
//!
//! `import counter.{bump, read}` binds to the backend declared in qube.json5's
//! `rpc.import`. Every call into it carries `@wire` — a remote call, disclosed
//! in the manifest and in `qube audit`.

import counter.{bump, read}

state count = 0          // what to draw; the real number lives in the backend

fn main @wire {
    count = read()       // load the current shared count, then draw
    paint()
}

pub fn on_press(id: i64) @wire {
    count = bump()       // ask the backend to add one; show the new total
    paint()
}

// Label ids index the host glyph catalog (0: heading, 1: button label).
fn paint {
    qview.text(40, 56, 0)
    qview.number(40, 120, count)
    qview.button(1, 40, 180, 280, 72, 1)
    qview.present()
}
