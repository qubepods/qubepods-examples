# Adder

The **smallest q64 component** — a library qube whose entire surface is one
scalar function:

```q
pub fn add(a: i64, b: i64) -> i64 { a + b }
```

Built with the prebuilt `q64`/`qube` release binaries (see the repo README —
never build q64 from source just to run an example), it emits a WebAssembly
**component** alongside the core module:

```
qube build --component
# → target/debug/wasm64/qubepods.examples.adder.wasm            (core module)
# → target/debug/wasm64/qubepods.examples.adder.component.wasm  (component)
# → target/debug/wasm64/qubepods.examples.adder.wit             (synthesized world)
```

The synthesized WIT world is the whole contract — one export, no imports:

```wit
package qubepods-examples:adder;

world adder {
  // (none — pure surface)
  export add: func(a: s64, b: s64) -> s64;
}
```

## What it shows

- A **library qube** (`type: "library"`, no `main`): it exports a surface for
  other qubes to link, rather than being deployed as an application.
- The **canonical-ABI scalar lift**: an `i64 -> i64` function crosses the
  component boundary as `s64 -> s64` with no capability imports to lower —
  the "pure surface" case, and the simplest possible component emit.
- The core module runs anywhere wasm does. In Node, for instance:

```js
const { instance } = await WebAssembly.instantiate(
  fs.readFileSync("target/debug/wasm64/qubepods.examples.adder.wasm"), {});
instance.exports.add(2n, 3n); // 5n  (i64 ⇄ BigInt)
```
