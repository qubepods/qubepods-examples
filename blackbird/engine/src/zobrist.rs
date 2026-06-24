/// Zobrist hashing for chess positions.
///
/// A unique 64-bit hash for each position, incrementally updated on each move.
/// Used for transposition table lookups and repetition detection.

use crate::board::{Color, PieceKind};

/// Random numbers for Zobrist hashing — initialized deterministically from a seed.
static mut PIECE_KEYS: [[[u64; 64]; 6]; 2] = [[[0; 64]; 6]; 2]; // [color][piece][square]
static mut CASTLING_KEYS: [u64; 16] = [0; 16]; // 4 bits = 16 combinations
static mut EP_KEYS: [u64; 8] = [0; 8]; // en passant file (0-7)
static mut SIDE_KEY: u64 = 0; // XOR when black to move

static INIT: std::sync::Once = std::sync::Once::new();

pub fn init() {
    INIT.call_once(|| unsafe { init_keys() });
}

/// Simple PRNG (xorshift64) for deterministic key generation
fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

unsafe fn init_keys() {
    let mut rng: u64 = 0x3243F6A8885A308D; // seed from pi digits
    
    for color in 0..2 {
        for piece in 0..6 {
            for sq in 0..64 {
                PIECE_KEYS[color][piece][sq] = xorshift64(&mut rng);
            }
        }
    }
    for i in 0..16 {
        CASTLING_KEYS[i] = xorshift64(&mut rng);
    }
    for i in 0..8 {
        EP_KEYS[i] = xorshift64(&mut rng);
    }
    SIDE_KEY = xorshift64(&mut rng);
}

// --- Public API ---

#[inline]
fn color_idx(c: Color) -> usize {
    match c { Color::White => 0, Color::Black => 1 }
}

#[inline]
fn piece_idx(k: PieceKind) -> usize {
    match k {
        PieceKind::Pawn => 0,
        PieceKind::Knight => 1,
        PieceKind::Bishop => 2,
        PieceKind::Rook => 3,
        PieceKind::Queen => 4,
        PieceKind::King => 5,
    }
}

/// Get the Zobrist key for a piece on a square
#[inline]
pub fn piece_key(color: Color, kind: PieceKind, sq: u32) -> u64 {
    unsafe { PIECE_KEYS[color_idx(color)][piece_idx(kind)][sq as usize] }
}

/// Get the Zobrist key for castling rights (encoded as 4 bits)
#[inline]
pub fn castling_key(rights: u8) -> u64 {
    unsafe { CASTLING_KEYS[rights as usize & 0xF] }
}

/// Get the Zobrist key for en passant file
#[inline]
pub fn ep_key(file: u32) -> u64 {
    unsafe { EP_KEYS[file as usize] }
}

/// Get the Zobrist key for side to move (XOR when black)
#[inline]
pub fn side_key() -> u64 {
    unsafe { SIDE_KEY }
}

/// Compute full Zobrist hash from a board position
pub fn hash_position(board: &crate::board::Board) -> u64 {
    init();
    let mut h = 0u64;
    
    // Pieces
    for sq in 0..64u32 {
        let s = crate::board::Square::new((sq & 7) as u8, (sq >> 3) as u8);
        if let Some(p) = board.piece_at(s) {
            h ^= piece_key(p.color, p.kind, sq);
        }
    }
    
    // Castling
    let mut rights = 0u8;
    if board.castling.white_kingside { rights |= 1; }
    if board.castling.white_queenside { rights |= 2; }
    if board.castling.black_kingside { rights |= 4; }
    if board.castling.black_queenside { rights |= 8; }
    h ^= castling_key(rights);
    
    // En passant
    if let Some(ep) = board.en_passant {
        h ^= ep_key(ep.file as u32);
    }
    
    // Side to move
    if board.side_to_move == Color::Black {
        h ^= side_key();
    }
    
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Board;

    #[test]
    fn test_starting_hash() {
        init();
        let b1 = Board::new();
        let b2 = Board::new();
        let h1 = hash_position(&b1);
        let h2 = hash_position(&b2);
        assert_eq!(h1, h2, "Same position should have same hash");
        assert_ne!(h1, 0, "Hash should not be zero");
    }

    #[test]
    fn test_different_positions() {
        init();
        let b1 = Board::new();
        let b2 = Board::from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1").unwrap();
        let h1 = hash_position(&b1);
        let h2 = hash_position(&b2);
        assert_ne!(h1, h2, "Different positions should have different hashes");
    }
}
