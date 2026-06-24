//
// worker.js -- runs the Blackbird wasm engine off the main thread.
//
// The UCI boundary of the desktop app becomes a postMessage protocol here:
// the page sends { type, ... } commands, the worker answers with the full
// game state after every operation so the UI never has to mirror rules.
//

let api = null;

function state() {
  const legal = JSON.parse(api.legalMoves());
  const inCheck = api.inCheck() === 1;
  return {
    fen: api.fen(),
    whiteToMove: api.whiteToMove() === 1,
    legal,
    inCheck,
    mate: legal.length === 0 && inCheck,
    stalemate: legal.length === 0 && !inCheck,
  };
}

self.onmessage = async (e) => {
  const msg = e.data;

  if (msg.type === 'init') {
    try {
      importScripts('engine/blackbird.js');
      // The loader fetches blackbird.wasm relative to the *worker's* URL
      // (self.location), not engine/blackbird.js -- point it back at
      // engine/ explicitly.
      const Module = await createBlackbird({
        locateFile: (file) => 'engine/' + file,
      });
      api = Module.api;
      api.init();
      postMessage({ type: 'ready', state: state() });
    } catch (err) {
      postMessage({ type: 'error', error: 'engine failed to load: ' + err });
    }
    return;
  }

  if (!api) {
    postMessage({ type: 'error', error: 'engine not initialized' });
    return;
  }

  switch (msg.type) {
    case 'new':
      api.newGame();
      postMessage({ type: 'state', state: state() });
      break;

    case 'setFen': {
      api.setFen(msg.fen);
      postMessage({ type: 'state', state: state() });
      break;
    }

    case 'userMove': {
      const ok = api.applyMove(msg.from, msg.to, msg.promo || '') === 1;
      const move = { from: msg.from, to: msg.to };
      if (msg.promo) {
        move.promotion = msg.promo;
      }
      postMessage({ type: 'moveResult', ok, move, state: state() });
      break;
    }

    case 'engineMove': {
      const t0 = performance.now();
      const move = JSON.parse(api.bestMove(msg.depth || 3));
      if (move) {
        api.applyMove(move.from, move.to, move.promotion || '');
      }
      postMessage({
        type: 'engineMoved',
        move,
        millis: Math.round(performance.now() - t0),
        state: state(),
      });
      break;
    }
  }
};
