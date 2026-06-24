//
// blackbird.js -- loader + JS bindings for the Rust engine wasm.
//
// Hand-written replacement for the Emscripten glue the C++ build used: the
// Rust module is plain wasm32-unknown-unknown with no imports, so all this
// does is instantiate it and marshal strings across the boundary.
//
// ABI (see wasm/src/lib.rs): numbers are plain i32/u32; strings IN go
// through bb_alloc + (ptr, len); strings OUT come back as a length with the
// bytes at bb_result_ptr(). Memory views are taken fresh after every call
// because memory.grow invalidates them.
//
// Exposes createBlackbird({ locateFile }) -> Promise<{ api }>, mirroring
// the factory shape the worker already loads.
//

function createBlackbird(opts) {
  const locate = (opts && opts.locateFile) || ((f) => f);
  const url = locate('blackbird.wasm');

  const encoder = new TextEncoder();
  const decoder = new TextDecoder();

  return fetch(url)
    .then((resp) => WebAssembly.instantiateStreaming
      ? WebAssembly.instantiateStreaming(resp, {})
      : resp.arrayBuffer().then((b) => WebAssembly.instantiate(b, {})))
    .then(({ instance }) => {
      const e = instance.exports;

      function readResult(len) {
        const bytes = new Uint8Array(e.memory.buffer, e.bb_result_ptr(), len);
        return decoder.decode(bytes);
      }

      function sendString(s) {
        const bytes = encoder.encode(s);
        const ptr = e.bb_alloc(bytes.length);
        new Uint8Array(e.memory.buffer, ptr, bytes.length).set(bytes);
        return [ptr, bytes.length];
      }

      const api = {
        init: () => e.bb_init(),
        newGame: () => e.bb_new_game(),
        setFen: (fen) => {
          const [ptr, len] = sendString(fen);
          return e.bb_set_fen(ptr, len);
        },
        fen: () => readResult(e.bb_fen()),
        whiteToMove: () => e.bb_white_to_move(),
        legalMoves: () => readResult(e.bb_legal_moves()),
        applyMove: (from, to, promo) =>
          e.bb_apply_move(from, to, promo ? promo.charCodeAt(0) : 0),
        inCheck: () => e.bb_in_check(),
        isMate: () => e.bb_is_mate(),
        bestMove: (depth) => readResult(e.bb_best_move(depth)),
      };

      return { api };
    });
}

// Worker (importScripts) and Node (smoke test) contexts.
if (typeof self !== 'undefined') {
  self.createBlackbird = createBlackbird;
}
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { createBlackbird };
}
