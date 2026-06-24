//
// smoke.mjs -- drives the shipped glue + wasm (web/engine/) through the
// same call sequence the worker makes, under Node. Run from the repo root:
//
//     node scripts/smoke.mjs
//
// Node has no fetch for local files, so fetch is shimmed to read from disk.
//
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';

globalThis.fetch = async (path) =>
  new Response(await readFile(new URL(path, import.meta.url)), {
    headers: { 'content-type': 'application/wasm' },
  });

const require = createRequire(import.meta.url);
const { createBlackbird } = require('../web/engine/blackbird.js');

const { api } = await createBlackbird({
  locateFile: (f) => '../web/engine/' + f,
});

function fail(msg) {
  console.error('FAIL:', msg);
  process.exit(1);
}

api.init();

// Start position sanity.
let legal = JSON.parse(api.legalMoves());
if (legal.length !== 20) fail(`startpos legal moves: ${legal.length}`);
if (api.whiteToMove() !== 1) fail('white should move first');
if (!api.fen().startsWith('rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w'))
  fail('bad start FEN: ' + api.fen());

// User plays e2e4 (12 -> 28), like a board click.
if (api.applyMove(12, 28, '') !== 1) fail('e2e4 rejected');

// Engine answers at each UI depth; every reply must be legal.
for (const depth of [1, 3, 5, 6]) {
  const t0 = Date.now();
  const move = JSON.parse(api.bestMove(depth));
  const ms = Date.now() - t0;
  if (!move) fail(`no best move at depth ${depth}`);
  legal = JSON.parse(api.legalMoves());
  const ok = legal.some(
    (m) => m.from === move.from && m.to === move.to &&
      (m.promotion || '') === (move.promotion || ''),
  );
  if (!ok) fail(`illegal engine move at depth ${depth}: ${JSON.stringify(move)}`);
  console.log(`depth ${depth}: ${JSON.stringify(move)} in ${ms} ms`);
}

// Play a quick engine-vs-engine game to shake out apply/undo-free state.
api.newGame();
let moves = 0;
while (moves < 60) {
  const move = JSON.parse(api.bestMove(4));
  if (!move) break; // mate or stalemate
  if (api.applyMove(move.from, move.to, move.promotion || '') !== 1)
    fail(`engine produced illegal move: ${JSON.stringify(move)}`);
  moves++;
}
const mate = api.isMate() === 1;
console.log(`self-play: ${moves} plies, ` +
  (moves < 60 ? (mate ? 'checkmate' : 'stalemate/draw') : 'cut off') +
  `, final FEN: ${api.fen()}`);

// FEN in, mate detection (fool's mate).
if (api.setFen('rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3') !== 1)
  fail('setFen rejected a valid FEN');
if (api.isMate() !== 1) fail('fool\'s mate not detected');
if (JSON.parse(api.bestMove(3)) !== null) fail('bestMove should be null in mate');
if (api.setFen('not a fen') !== 0) fail('setFen accepted garbage');

console.log('smoke: all checks passed');
