//
// app.js -- board UI for the Blackbird wasm engine.
//
// Square indexing matches the engine: a1 = 0 .. h8 = 63 (rank * 8 + file).
// The engine (in worker.js) is the single source of truth for rules; this
// file only renders state and forwards clicks.
//

const PIECE_IMG = {
  P: 'WhitePawn', N: 'WhiteKnight', B: 'WhiteBishop',
  R: 'WhiteRook', Q: 'WhiteQueen', K: 'WhiteKing',
  p: 'BlackPawn', n: 'BlackKnight', b: 'BlackBishop',
  r: 'BlackRook', q: 'BlackQueen', k: 'BlackKing',
};

const boardEl = document.getElementById('board');
const statusEl = document.getElementById('status');
const movesEl = document.getElementById('moves');
const fenEl = document.getElementById('fen');
const depthEl = document.getElementById('depth');
const promoEl = document.getElementById('promo');

const worker = new Worker('worker.js');

let state = null;       // last state object from the worker
let selected = null;    // selected square index, or null
let lastMove = null;    // {from, to} of the most recent move
let moveLog = [];
let thinking = false;
let flipped = false;    // false: white at the bottom (you play white)
                        // true: black at the bottom, engine plays white

function squareName(i) {
  return 'abcdefgh'[i % 8] + (Math.floor(i / 8) + 1);
}

// FEN piece placement -> array of 64 (a1 = 0), '' for empty.
function fenToFields(fen) {
  const fields = new Array(64).fill('');
  const placement = fen.split(' ')[0];
  let rank = 7;
  let file = 0;
  for (const ch of placement) {
    if (ch === '/') {
      rank--;
      file = 0;
    } else if (ch >= '1' && ch <= '8') {
      file += Number(ch);
    } else {
      fields[rank * 8 + file] = ch;
      file++;
    }
  }
  return fields;
}

function render() {
  const fields = fenToFields(state.fen);
  const targets = new Set(
    selected === null
      ? []
      : state.legal.filter((m) => m.from === selected).map((m) => m.to)
  );

  boardEl.innerHTML = '';
  for (let r = 0; r < 8; r++) {
    const rank = flipped ? r : 7 - r;
    for (let f = 0; f < 8; f++) {
      const file = flipped ? 7 - f : f;
      const i = rank * 8 + file;
      const sq = document.createElement('div');
      sq.className = 'square ' + ((rank + file) % 2 ? 'light' : 'dark');
      if (i === selected) sq.classList.add('selected');
      if (targets.has(i)) sq.classList.add('target');
      if (fields[i]) sq.classList.add('occupied');
      if (lastMove && (i === lastMove.from || i === lastMove.to)) {
        sq.classList.add('last');
      }
      if (fields[i]) {
        const img = document.createElement('img');
        img.src = 'img/' + PIECE_IMG[fields[i]] + '.png';
        img.alt = fields[i];
        sq.appendChild(img);
      }
      sq.addEventListener('click', () => onSquareClick(i, fields));
      boardEl.appendChild(sq);
    }
  }

  fenEl.textContent = state.fen;
  movesEl.textContent = moveLog.join('\n');
  movesEl.scrollTop = movesEl.scrollHeight;
  renderStatus();
}

function renderStatus() {
  if (thinking) {
    statusEl.textContent = 'engine is thinking…';
    return;
  }
  const side = state.whiteToMove ? 'white' : 'black';
  if (state.mate) {
    statusEl.textContent = `checkmate — ${side} loses`;
  } else if (state.stalemate) {
    statusEl.textContent = 'stalemate';
  } else if (state.inCheck) {
    statusEl.textContent = `${side} to move — check!`;
  } else {
    statusEl.textContent = `${side} to move`;
  }
}

function logMove(mover, m) {
  const promo = m.promotion ? '=' + m.promotion.toUpperCase() : '';
  moveLog.push(`${moveLog.length + 1}. ${mover} ${squareName(m.from)}-${squareName(m.to)}${promo}`);
}

function onSquareClick(i, fields) {
  if (thinking || !state || state.legal.length === 0) return;

  const movesFromSelected =
    selected === null ? [] : state.legal.filter((m) => m.from === selected && m.to === i);

  if (movesFromSelected.length > 0) {
    let promo = '';
    if (movesFromSelected.some((m) => m.promotion)) {
      promo = promoEl.value;
    }
    worker.postMessage({ type: 'userMove', from: selected, to: i, promo });
    selected = null;
    return;
  }

  // (re)select a piece that has legal moves
  if (state.legal.some((m) => m.from === i)) {
    selected = selected === i ? null : i;
  } else {
    selected = null;
  }
  render();
}

function requestEngineMove() {
  thinking = true;
  renderStatus();
  worker.postMessage({ type: 'engineMove', depth: Number(depthEl.value) });
}

// The engine owns the side facing away from the player: black normally,
// white when the board is flipped.
function maybeEngineMove() {
  if (!thinking && state && state.legal.length > 0 && state.whiteToMove === flipped) {
    requestEngineMove();
  }
}

worker.onmessage = (e) => {
  const msg = e.data;

  switch (msg.type) {
    case 'ready':
      state = msg.state;
      render();
      maybeEngineMove();
      break;

    case 'state':
      state = msg.state;
      render();
      maybeEngineMove();
      break;

    case 'moveResult': {
      if (msg.ok) {
        const mover = state.whiteToMove ? 'white' : 'black';
        state = msg.state;
        lastMove = msg.move;
        logMove(mover, msg.move);
        render();
        maybeEngineMove(); // engine answers when its side is to move
      }
      break;
    }

    case 'engineMoved':
      thinking = false;
      if (msg.move) {
        lastMove = msg.move;
        logMove(state.whiteToMove ? 'white' : 'black', msg.move);
      }
      state = msg.state;
      render();
      maybeEngineMove(); // in case the board was flipped while it thought
      break;

    case 'error':
      statusEl.textContent = 'error: ' + msg.error;
      break;
  }
};

document.getElementById('new-game').addEventListener('click', () => {
  selected = null;
  lastMove = null;
  moveLog = [];
  thinking = false;
  worker.postMessage({ type: 'new' });
});

document.getElementById('engine-move').addEventListener('click', () => {
  if (!thinking && state && state.legal.length > 0) {
    requestEngineMove();
  }
});

document.getElementById('flip').addEventListener('click', () => {
  flipped = !flipped;
  selected = null;
  if (state) render();
  maybeEngineMove(); // flipping hands the far side to the engine
});

worker.postMessage({ type: 'init' });
