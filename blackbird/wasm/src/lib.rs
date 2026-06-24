//! bb_* API over the chess-engine core, for the browser build.
//!
//! Stateful, like the wrapper it replaces: one game board, the search runs
//! from the current position. The worker (web/worker.js) is the only caller.
//!
//! ABI, kept trivial on purpose (no bindgen, no tool-generated glue):
//!   - numbers cross as plain wasm i32/u32 values;
//!   - strings IN: the caller gets a buffer from `bb_alloc`, writes UTF-8,
//!     and passes (ptr, len) — the callee frees it;
//!   - strings OUT: the call returns the byte length, the bytes sit in a
//!     static result buffer at `bb_result_ptr()` until the next call.
//!
//! Square indexing matches the engine and the web UI: a1 = 0 .. h8 = 63
//! (rank * 8 + file). Moves cross as JSON: {"from":12,"to":28} plus an
//! optional "promotion":"q|r|b|n".

use std::alloc::{alloc, dealloc, Layout};
use std::sync::Mutex;

use chess_engine::board::Board;
use chess_engine::movegen::{is_in_check, legal_moves};
use chess_engine::moves::{make_move, Move, MoveKind};
use chess_engine::board::PieceKind;
use chess_engine::search::SearchState;

const TT_MB: usize = 16;
const SEARCH_THREADS: usize = 1; // wasm has no std::thread

struct Game {
    board: Board,
    search: SearchState,
    /// Zobrist hashes of every position reached, for repetition detection.
    history: Vec<u64>,
}

impl Game {
    fn new() -> Self {
        let board = Board::new();
        Self {
            history: vec![board.hash],
            board,
            search: SearchState::new(TT_MB, SEARCH_THREADS),
        }
    }
}

static GAME: Mutex<Option<Game>> = Mutex::new(None);
static RESULT: Mutex<Vec<u8>> = Mutex::new(Vec::new());

fn with_game<T>(f: impl FnOnce(&mut Game) -> T) -> Option<T> {
    let mut lock = GAME.lock().ok()?;
    lock.as_mut().map(f)
}

fn put_result(s: String) -> i32 {
    let mut r = RESULT.lock().unwrap();
    *r = s.into_bytes();
    r.len() as i32
}

fn promo_letter(kind: PieceKind) -> Option<char> {
    match kind {
        PieceKind::Queen => Some('q'),
        PieceKind::Rook => Some('r'),
        PieceKind::Bishop => Some('b'),
        PieceKind::Knight => Some('n'),
        _ => None,
    }
}

fn move_json(mv: &Move) -> String {
    let mut s = format!("{{\"from\":{},\"to\":{}", mv.from.index(), mv.to.index());
    if let MoveKind::Promotion(kind) = mv.kind {
        if let Some(c) = promo_letter(kind) {
            s.push_str(&format!(",\"promotion\":\"{}\"", c));
        }
    }
    s.push('}');
    s
}

/// Pointer to the bytes of the last string-returning call.
#[no_mangle]
pub extern "C" fn bb_result_ptr() -> *const u8 {
    RESULT.lock().unwrap().as_ptr()
}

/// Caller-side buffer for passing strings in. Freed by the callee.
#[no_mangle]
pub extern "C" fn bb_alloc(len: usize) -> *mut u8 {
    let layout = Layout::from_size_align(len.max(1), 1).unwrap();
    unsafe { alloc(layout) }
}

fn take_string(ptr: *mut u8, len: usize) -> Option<String> {
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec();
    unsafe { dealloc(ptr, Layout::from_size_align(len.max(1), 1).unwrap()) };
    String::from_utf8(bytes).ok()
}

/// Initialize: starting position, fresh search state.
#[no_mangle]
pub extern "C" fn bb_init() {
    chess_engine::zobrist::init();
    *GAME.lock().unwrap() = Some(Game::new());
}

/// Reset to the starting position (search state kept, table cleared).
#[no_mangle]
pub extern "C" fn bb_new_game() {
    with_game(|g| {
        g.board = Board::new();
        g.history = vec![g.board.hash];
        g.search.clear();
    });
}

/// Set position from FEN. Returns 1 on success, 0 on failure.
#[no_mangle]
pub extern "C" fn bb_set_fen(ptr: *mut u8, len: usize) -> i32 {
    let Some(fen) = take_string(ptr, len) else {
        return 0;
    };
    match Board::from_fen(&fen) {
        Ok(board) => with_game(|g| {
            g.history = vec![board.hash];
            g.board = board;
            1
        })
        .unwrap_or(0),
        Err(_) => 0,
    }
}

/// Current position as FEN (string out).
#[no_mangle]
pub extern "C" fn bb_fen() -> i32 {
    put_result(with_game(|g| g.board.to_fen()).unwrap_or_default())
}

/// 1 if white to move, 0 if black, -1 if uninitialized.
#[no_mangle]
pub extern "C" fn bb_white_to_move() -> i32 {
    with_game(|g| match g.board.side_to_move {
        chess_engine::board::Color::White => 1,
        chess_engine::board::Color::Black => 0,
    })
    .unwrap_or(-1)
}

/// Legal moves as a JSON array (string out).
#[no_mangle]
pub extern "C" fn bb_legal_moves() -> i32 {
    let json = with_game(|g| {
        let moves = legal_moves(&g.board);
        let items: Vec<String> = moves.iter().map(move_json).collect();
        format!("[{}]", items.join(","))
    })
    .unwrap_or_else(|| "[]".to_string());
    put_result(json)
}

/// Apply a move given from/to square indexes and a promotion letter
/// ('q','r','b','n' as a char code, 0 for none). Returns 1 if the move was
/// legal and applied, 0 otherwise.
#[no_mangle]
pub extern "C" fn bb_apply_move(from: u32, to: u32, promo: u32) -> i32 {
    with_game(|g| {
        let promo_char = char::from_u32(promo);
        let mv = legal_moves(&g.board).into_iter().find(|m| {
            if m.from.index() != from as usize || m.to.index() != to as usize {
                return false;
            }
            match m.kind {
                MoveKind::Promotion(kind) => promo_char == promo_letter(kind),
                _ => promo == 0,
            }
        });
        match mv {
            Some(mv) => {
                g.board = make_move(&g.board, &mv);
                g.history.push(g.board.hash);
                1
            }
            None => 0,
        }
    })
    .unwrap_or(0)
}

/// 1 if the side to move is in check.
#[no_mangle]
pub extern "C" fn bb_in_check() -> i32 {
    with_game(|g| is_in_check(&g.board, g.board.side_to_move) as i32).unwrap_or(-1)
}

/// 1 if the side to move is checkmated.
#[no_mangle]
pub extern "C" fn bb_is_mate() -> i32 {
    with_game(|g| {
        (legal_moves(&g.board).is_empty() && is_in_check(&g.board, g.board.side_to_move)) as i32
    })
    .unwrap_or(-1)
}

/// Best move at the given depth as JSON (string out), "null" if the game is
/// over. The caller applies it via bb_apply_move; the board is not changed.
#[no_mangle]
pub extern "C" fn bb_best_move(depth: u32) -> i32 {
    let json = with_game(|g| {
        g.search
            .search(&g.board, depth.max(1), &g.history)
            .map(|mv| move_json(&mv))
            .unwrap_or_else(|| "null".to_string())
    })
    .unwrap_or_else(|| "null".to_string());
    put_result(json)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The API is one global game, so tests must not interleave.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn serial() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn result_string(len: i32) -> String {
        let r = RESULT.lock().unwrap();
        assert_eq!(r.len(), len as usize);
        String::from_utf8(r.clone()).unwrap()
    }

    fn send_string(s: &str) -> (*mut u8, usize) {
        let ptr = bb_alloc(s.len());
        unsafe { std::ptr::copy_nonoverlapping(s.as_ptr(), ptr, s.len()) };
        (ptr, s.len())
    }

    #[test]
    fn startpos_has_twenty_moves() {
        let _g = serial();
        bb_init();
        let len = bb_legal_moves();
        let json = result_string(len);
        assert_eq!(json.matches("\"from\"").count(), 20);
        assert_eq!(bb_white_to_move(), 1);
        assert_eq!(bb_in_check(), 0);
        assert_eq!(bb_is_mate(), 0);
    }

    #[test]
    fn apply_move_and_fen_roundtrip() {
        let _g = serial();
        bb_init();
        // e2 = 12, e4 = 28
        assert_eq!(bb_apply_move(12, 28, 0), 1);
        assert_eq!(bb_white_to_move(), 0);
        let fen = result_string(bb_fen());
        assert!(fen.starts_with("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b"));
        // Illegal move rejected
        assert_eq!(bb_apply_move(12, 28, 0), 0);
    }

    #[test]
    fn set_fen_and_detect_mate() {
        let _g = serial();
        bb_init();
        // Fool's mate final position: black just mated white.
        let (ptr, len) =
            send_string("rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3");
        assert_eq!(bb_set_fen(ptr, len), 1);
        assert_eq!(bb_in_check(), 1);
        assert_eq!(bb_is_mate(), 1);
        let json = result_string(bb_best_move(3));
        assert_eq!(json, "null");
    }

    #[test]
    fn engine_finds_a_legal_best_move() {
        let _g = serial();
        bb_init();
        let json = result_string(bb_best_move(4));
        assert!(json.starts_with("{\"from\":"), "got: {json}");
        // Apply it back through the API.
        let from: u32 = json["{\"from\":".len()..].split(',').next().unwrap().parse().unwrap();
        let to: u32 = json.split("\"to\":").nth(1).unwrap().trim_end_matches('}').parse().unwrap();
        assert_eq!(bb_apply_move(from, to, 0), 1);
    }

    #[test]
    fn engine_takes_the_hanging_queen() {
        let _g = serial();
        bb_init();
        // 1. e4 e5 2. Qh5?? Nc6 3. Qxe5+?? — now ...Nxe5 wins the queen.
        for uci in ["e2e4", "e7e5", "d1h5", "b8c6", "h5e5"] {
            let mv = with_game(|g| Move::from_uci(uci, &g.board).unwrap()).unwrap();
            assert_eq!(bb_apply_move(mv.from.index() as u32, mv.to.index() as u32, 0), 1);
        }
        let json = result_string(bb_best_move(4));
        // c6 = 42, e5 = 36
        assert_eq!(json, "{\"from\":42,\"to\":36}");
    }
}
