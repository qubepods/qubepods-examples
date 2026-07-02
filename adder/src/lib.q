//! qubepods.examples.adder — the smallest q64 component.
//!
//! One scalar export (i64 -> s64), so the component lift wraps it with no
//! capability imports to lower:
//!
//!   qube build --component
//!   # → target/debug/wasm64/qubepods.examples.adder.{wasm,component.wasm}

pub fn add(a: i64, b: i64) -> i64 { a + b }
