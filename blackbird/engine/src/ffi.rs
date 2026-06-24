//! C FFI for the chess engine — callable from Swift via bridging header.

use crate::board::Board;
use crate::movegen::legal_moves;
use crate::moves::make_move;
use crate::search::SearchState;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::Mutex;

static ENGINE: Mutex<Option<Board>> = Mutex::new(None);
static SEARCH: Mutex<Option<SearchState>> = Mutex::new(None);

fn with_board<T>(f: impl FnOnce(&Board) -> T) -> Option<T> {
    let lock = ENGINE.lock().ok()?;
    lock.as_ref().map(f)
}

fn with_board_mut<T>(f: impl FnOnce(&mut Board) -> T) -> Option<T> {
    let mut lock = ENGINE.lock().ok()?;
    lock.as_mut().map(f)
}

/// Initialize engine with starting position
#[no_mangle]
pub extern "C" fn engine_new() {
    crate::zobrist::init();
    if let Ok(mut lock) = ENGINE.lock() {
        *lock = Some(Board::new());
    }
    if let Ok(mut lock) = SEARCH.lock() {
        if lock.is_none() {
            let threads = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1);
            *lock = Some(SearchState::new(16, threads));
        }
    }
}

/// Set position from FEN string. Returns 1 on success, 0 on failure.
#[no_mangle]
pub extern "C" fn engine_set_fen(fen: *const c_char) -> i32 {
    let c_str = unsafe { CStr::from_ptr(fen) };
    let fen_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return 0,
    };
    match Board::from_fen(fen_str) {
        Ok(board) => {
            if let Ok(mut lock) = ENGINE.lock() {
                *lock = Some(board);
                1
            } else {
                0
            }
        }
        Err(_) => 0,
    }
}

/// Get current FEN. Caller must free the returned string with engine_free_string.
#[no_mangle]
pub extern "C" fn engine_get_fen() -> *mut c_char {
    let fen = with_board(|b| b.to_fen()).unwrap_or_default();
    CString::new(fen).unwrap_or_default().into_raw()
}

/// Get side to move: 0 = white, 1 = black, -1 = error
#[no_mangle]
pub extern "C" fn engine_side_to_move() -> i32 {
    with_board(|b| match b.side_to_move {
        crate::board::Color::White => 0,
        crate::board::Color::Black => 1,
    })
    .unwrap_or(-1)
}

/// Get legal moves as a string: "e2e3,e2e4,g1f3,..." 
/// Caller must free with engine_free_string.
#[no_mangle]
pub extern "C" fn engine_legal_moves() -> *mut c_char {
    let moves_str = with_board(|b| {
        let moves = legal_moves(b);
        moves
            .iter()
            .map(|m| m.to_uci())
            .collect::<Vec<_>>()
            .join(",")
    })
    .unwrap_or_default();
    CString::new(moves_str).unwrap_or_default().into_raw()
}

/// Get legal moves from a specific square: "e3,e4" (just destination squares)
/// Caller must free with engine_free_string.
#[no_mangle]
pub extern "C" fn engine_moves_from(square: *const c_char) -> *mut c_char {
    let c_str = unsafe { CStr::from_ptr(square) };
    let sq_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return CString::new("").unwrap().into_raw(),
    };
    let from_sq = match crate::board::Square::from_algebraic(sq_str) {
        Some(s) => s,
        None => return CString::new("").unwrap().into_raw(),
    };
    let moves_str = with_board(|b| {
        let moves = legal_moves(b);
        moves
            .iter()
            .filter(|m| m.from == from_sq)
            .map(|m| m.to_uci())
            .collect::<Vec<_>>()
            .join(",")
    })
    .unwrap_or_default();
    CString::new(moves_str).unwrap_or_default().into_raw()
}

/// Make a move in UCI notation (e.g. "e2e4"). Returns 1 on success, 0 on failure.
#[no_mangle]
pub extern "C" fn engine_make_move(uci: *const c_char) -> i32 {
    let c_str = unsafe { CStr::from_ptr(uci) };
    let uci_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return 0,
    };
    if let Ok(mut lock) = ENGINE.lock() {
        if let Some(board) = lock.as_ref() {
            if let Some(mv) = crate::moves::Move::from_uci(uci_str, board) {
                let new_board = make_move(board, &mv);
                *lock = Some(new_board);
                return 1;
            }
        }
    }
    0
}

/// Get best move at given depth. Returns UCI string (e.g. "e2e4").
/// Uses all available CPU cores via Lazy SMP.
/// Caller must free with engine_free_string.
#[no_mangle]
pub extern "C" fn engine_best_move(depth: u32) -> *mut c_char {
    let mv_str = with_board(|b| {
        if let Ok(lock) = SEARCH.lock() {
            if let Some(search) = lock.as_ref() {
                return search.search(b, depth, &[]).map(|m| m.to_uci()).unwrap_or_default();
            }
        }
        crate::best_move(b, depth).map(|m| m.to_uci()).unwrap_or_default()
    })
    .unwrap_or_default();
    CString::new(mv_str).unwrap_or_default().into_raw()
}

/// Get piece at square. Returns piece string (e.g. "wK", "bP") or empty.
/// Caller must free with engine_free_string.
#[no_mangle]
pub extern "C" fn engine_piece_at(square: *const c_char) -> *mut c_char {
    let c_str = unsafe { CStr::from_ptr(square) };
    let sq_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return CString::new("").unwrap().into_raw(),
    };
    let sq = match crate::board::Square::from_algebraic(sq_str) {
        Some(s) => s,
        None => return CString::new("").unwrap().into_raw(),
    };
    let piece_str = with_board(|b| {
        b.piece_at(sq)
            .map(|p| {
                let c = match p.color {
                    crate::board::Color::White => 'w',
                    crate::board::Color::Black => 'b',
                };
                let k = match p.kind {
                    crate::board::PieceKind::King => 'K',
                    crate::board::PieceKind::Queen => 'Q',
                    crate::board::PieceKind::Rook => 'R',
                    crate::board::PieceKind::Bishop => 'B',
                    crate::board::PieceKind::Knight => 'N',
                    crate::board::PieceKind::Pawn => 'P',
                };
                format!("{}{}", c, k)
            })
            .unwrap_or_default()
    })
    .unwrap_or_default();
    CString::new(piece_str).unwrap_or_default().into_raw()
}

/// Get all pieces as "sq:piece,sq:piece,..." e.g. "e1:wK,d8:bQ,..."
/// Caller must free with engine_free_string.
#[no_mangle]
pub extern "C" fn engine_get_pieces() -> *mut c_char {
    let pieces_str = with_board(|b| {
        let mut parts = Vec::new();
        for rank in 0..8u8 {
            for file in 0..8u8 {
                let sq = crate::board::Square::new(file, rank);
                if let Some(p) = b.piece_at(sq) {
                    let c = match p.color {
                        crate::board::Color::White => 'w',
                        crate::board::Color::Black => 'b',
                    };
                    let k = match p.kind {
                        crate::board::PieceKind::King => 'K',
                        crate::board::PieceKind::Queen => 'Q',
                        crate::board::PieceKind::Rook => 'R',
                        crate::board::PieceKind::Bishop => 'B',
                        crate::board::PieceKind::Knight => 'N',
                        crate::board::PieceKind::Pawn => 'P',
                    };
                    parts.push(format!("{}:{}{}", sq.to_algebraic(), c, k));
                }
            }
        }
        parts.join(",")
    })
    .unwrap_or_default();
    CString::new(pieces_str).unwrap_or_default().into_raw()
}

/// Check if the current side is in check. Returns 1 if in check, 0 if not, -1 on error.
#[no_mangle]
pub extern "C" fn engine_is_check() -> i32 {
    with_board(|b| {
        if crate::movegen::is_in_check(b, b.side_to_move) { 1 } else { 0 }
    })
    .unwrap_or(-1)
}

/// Get best move with time limit in milliseconds.
/// Searches as deep as possible within the time budget.
/// Caller must free with engine_free_string.
#[no_mangle]
pub extern "C" fn engine_best_move_timed(ms: u64) -> *mut c_char {
    let mv_str = with_board(|b| {
        if let Ok(lock) = SEARCH.lock() {
            if let Some(search) = lock.as_ref() {
                return search.search_timed(b, ms, &[]).map(|m| m.to_uci()).unwrap_or_default();
            }
        }
        crate::best_move(b, 6).map(|m| m.to_uci()).unwrap_or_default()
    })
    .unwrap_or_default();
    CString::new(mv_str).unwrap_or_default().into_raw()
}

/// Get number of search threads being used
#[no_mangle]
pub extern "C" fn engine_thread_count() -> i32 {
    if let Ok(lock) = SEARCH.lock() {
        if let Some(search) = lock.as_ref() {
            return search.threads as i32;
        }
    }
    1
}

/// Free a string returned by the engine
#[no_mangle]
pub extern "C" fn engine_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe { drop(CString::from_raw(s)); }
    }
}
