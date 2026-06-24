/// Piece-Square Tables — positional bonuses per piece per square.
/// Values from white's perspective, a1=index 0, h8=index 63.
/// For black, we mirror vertically (rank 0 ↔ rank 7).

/// Pawns: encourage center control and advancement
const PAWN: [i32; 64] = [
     0,  0,  0,  0,  0,  0,  0,  0,
    50, 50, 50, 50, 50, 50, 50, 50,
    10, 10, 20, 30, 30, 20, 10, 10,
     5,  5, 10, 25, 25, 10,  5,  5,
     0,  0,  0, 20, 20,  0,  0,  0,
     5, -5,-10,  0,  0,-10, -5,  5,
     5, 10, 10,-20,-20, 10, 10,  5,
     0,  0,  0,  0,  0,  0,  0,  0,
];

/// Knights: love the center, hate the rim
const KNIGHT: [i32; 64] = [
   -50,-40,-30,-30,-30,-30,-40,-50,
   -40,-20,  0,  0,  0,  0,-20,-40,
   -30,  0, 10, 15, 15, 10,  0,-30,
   -30,  5, 15, 20, 20, 15,  5,-30,
   -30,  0, 15, 20, 20, 15,  0,-30,
   -30,  5, 10, 15, 15, 10,  5,-30,
   -40,-20,  0,  5,  5,  0,-20,-40,
   -50,-40,-30,-30,-30,-30,-40,-50,
];

/// Bishops: prefer center diagonals, avoid corners
const BISHOP: [i32; 64] = [
   -20,-10,-10,-10,-10,-10,-10,-20,
   -10,  0,  0,  0,  0,  0,  0,-10,
   -10,  0, 10, 10, 10, 10,  0,-10,
   -10,  5,  5, 10, 10,  5,  5,-10,
   -10,  0, 10, 10, 10, 10,  0,-10,
   -10, 10, 10, 10, 10, 10, 10,-10,
   -10,  5,  0,  0,  0,  0,  5,-10,
   -20,-10,-10,-10,-10,-10,-10,-20,
];

/// Rooks: 7th rank is great, open files
const ROOK: [i32; 64] = [
     0,  0,  0,  0,  0,  0,  0,  0,
     5, 10, 10, 10, 10, 10, 10,  5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
     0,  0,  0,  5,  5,  0,  0,  0,
];

/// Queen: slight center preference, avoid early development to edges
const QUEEN: [i32; 64] = [
   -20,-10,-10, -5, -5,-10,-10,-20,
   -10,  0,  0,  0,  0,  0,  0,-10,
   -10,  0,  5,  5,  5,  5,  0,-10,
    -5,  0,  5,  5,  5,  5,  0, -5,
     0,  0,  5,  5,  5,  5,  0, -5,
   -10,  5,  5,  5,  5,  5,  0,-10,
   -10,  0,  5,  0,  0,  0,  0,-10,
   -20,-10,-10, -5, -5,-10,-10,-20,
];

/// King middlegame: stay castled, avoid center
const KING_MG: [i32; 64] = [
   -30,-40,-40,-50,-50,-40,-40,-30,
   -30,-40,-40,-50,-50,-40,-40,-30,
   -30,-40,-40,-50,-50,-40,-40,-30,
   -30,-40,-40,-50,-50,-40,-40,-30,
   -20,-30,-30,-40,-40,-30,-30,-20,
   -10,-20,-20,-20,-20,-20,-20,-10,
    20, 20,  0,  0,  0,  0, 20, 20,
    20, 30, 10,  0,  0, 10, 30, 20,
];

/// King endgame: centralize!
const KING_EG: [i32; 64] = [
   -50,-40,-30,-20,-20,-30,-40,-50,
   -30,-20,-10,  0,  0,-10,-20,-30,
   -30,-10, 20, 30, 30, 20,-10,-30,
   -30,-10, 30, 40, 40, 30,-10,-30,
   -30,-10, 30, 40, 40, 30,-10,-30,
   -30,-10, 20, 30, 30, 20,-10,-30,
   -30,-30,  0,  0,  0,  0,-30,-30,
   -50,-30,-30,-30,-30,-30,-30,-50,
];

use crate::board::{Board, Color, PieceKind, Square};

/// Mirror a square index vertically for black (rank 0 ↔ rank 7)
#[inline]
fn mirror(sq: usize) -> usize {
    sq ^ 56 // flips rank: rank 0 ↔ rank 7, rank 1 ↔ rank 6, etc.
}

/// Determine if we're in the endgame (rough heuristic: no queens, or queen + minor piece only)
fn is_endgame(board: &Board) -> bool {
    let mut white_material = 0i32;
    let mut black_material = 0i32;
    for sq in 0..64u8 {
        let s = Square::new(sq & 7, sq >> 3);
        if let Some(p) = board.piece_at(s) {
            let val = match p.kind {
                PieceKind::Queen => 900,
                PieceKind::Rook => 500,
                PieceKind::Bishop => 330,
                PieceKind::Knight => 320,
                _ => 0,
            };
            match p.color {
                Color::White => white_material += val,
                Color::Black => black_material += val,
            }
        }
    }
    // Endgame if both sides have ≤ 1300 (roughly rook + minor or less)
    white_material <= 1300 && black_material <= 1300
}

/// Full evaluation with material + piece-square tables
pub fn evaluate(board: &Board) -> i32 {
    let endgame = is_endgame(board);
    let mut score = 0i32;

    for sq in 0..64usize {
        let s = Square::new((sq & 7) as u8, (sq >> 3) as u8);
        if let Some(p) = board.piece_at(s) {
            // Material value
            let material = match p.kind {
                PieceKind::Pawn => 100,
                PieceKind::Knight => 320,
                PieceKind::Bishop => 330,
                PieceKind::Rook => 500,
                PieceKind::Queen => 900,
                PieceKind::King => 0,
            };

            // PST bonus
            let pst_idx = if p.color == Color::White { sq } else { mirror(sq) };
            let positional = match p.kind {
                PieceKind::Pawn => PAWN[pst_idx],
                PieceKind::Knight => KNIGHT[pst_idx],
                PieceKind::Bishop => BISHOP[pst_idx],
                PieceKind::Rook => ROOK[pst_idx],
                PieceKind::Queen => QUEEN[pst_idx],
                PieceKind::King => if endgame { KING_EG[pst_idx] } else { KING_MG[pst_idx] },
            };

            let value = material + positional;
            match p.color {
                Color::White => score += value,
                Color::Black => score -= value,
            }
        }
    }

    score
}
