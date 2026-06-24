//! qubepods:blackbird/blackbird — the engine as a real WIT component.
//!
//! Replaces the placeholder the first deploy shipped (a raw core module
//! copied to the component path): this exports the set-position / best-move
//! world the manifest declares, built for wasm32-wasip2.

use std::sync::Mutex;

use chess_engine::board::Board;
use chess_engine::movegen::legal_moves;
use chess_engine::moves::{make_move, Move};
use chess_engine::search::SearchState;

wit_bindgen::generate!({
    path: "wit",
    world: "blackbird",
});

const TT_MB: usize = 16;

struct Game {
    board: Board,
    search: SearchState,
    history: Vec<u64>,
}

static GAME: Mutex<Option<Game>> = Mutex::new(None);

fn with_game<T>(f: impl FnOnce(&mut Game) -> T) -> T {
    let mut lock = GAME.lock().unwrap();
    let game = lock.get_or_insert_with(|| {
        chess_engine::zobrist::init();
        let board = Board::new();
        Game {
            history: vec![board.hash],
            board,
            search: SearchState::new(TT_MB, 1),
        }
    });
    f(game)
}

struct Component;

impl Guest for Component {
    fn set_position(fen: String) -> bool {
        match Board::from_fen(&fen) {
            Ok(board) => with_game(|g| {
                g.history = vec![board.hash];
                g.board = board;
                true
            }),
            Err(_) => false,
        }
    }

    fn fen() -> String {
        with_game(|g| g.board.to_fen())
    }

    fn legal_moves() -> String {
        with_game(|g| {
            legal_moves(&g.board)
                .iter()
                .map(|m| m.to_uci())
                .collect::<Vec<_>>()
                .join(",")
        })
    }

    fn apply_move(uci: String) -> bool {
        with_game(|g| {
            let Some(mv) = Move::from_uci(&uci, &g.board) else {
                return false;
            };
            if !legal_moves(&g.board).contains(&mv) {
                return false;
            }
            g.board = make_move(&g.board, &mv);
            g.history.push(g.board.hash);
            true
        })
    }

    fn best_move(depth: u32) -> String {
        with_game(|g| {
            g.search
                .search(&g.board, depth.clamp(1, 64), &g.history)
                .map(|m| m.to_uci())
                .unwrap_or_default()
        })
    }
}

export!(Component);
