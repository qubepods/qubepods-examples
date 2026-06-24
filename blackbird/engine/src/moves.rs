use crate::board::{Board, Color, Piece, PieceKind, Square};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveKind {
    Normal,
    DoublePawnPush,
    EnPassant,
    CastleKingside,
    CastleQueenside,
    Promotion(PieceKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Move {
    pub from: Square,
    pub to: Square,
    pub kind: MoveKind,
}

impl Move {
    pub fn new(from: Square, to: Square) -> Self {
        Self {
            from,
            to,
            kind: MoveKind::Normal,
        }
    }

    pub fn with_kind(from: Square, to: Square, kind: MoveKind) -> Self {
        Self { from, to, kind }
    }

    /// UCI notation (e.g. "e2e4", "e7e8q")
    pub fn to_uci(&self) -> String {
        let mut s = format!("{}{}", self.from.to_algebraic(), self.to.to_algebraic());
        if let MoveKind::Promotion(kind) = self.kind {
            s.push(match kind {
                PieceKind::Queen => 'q',
                PieceKind::Rook => 'r',
                PieceKind::Bishop => 'b',
                PieceKind::Knight => 'n',
                _ => unreachable!(),
            });
        }
        s
    }

    pub fn from_uci(s: &str, board: &Board) -> Option<Self> {
        if s.len() < 4 || s.len() > 5 {
            return None;
        }
        let from = Square::from_algebraic(&s[0..2])?;
        let to = Square::from_algebraic(&s[2..4])?;

        let piece = board.piece_at(from)?;

        // Determine move kind
        let kind = if s.len() == 5 {
            let promo = match s.as_bytes()[4] {
                b'q' => PieceKind::Queen,
                b'r' => PieceKind::Rook,
                b'b' => PieceKind::Bishop,
                b'n' => PieceKind::Knight,
                _ => return None,
            };
            MoveKind::Promotion(promo)
        } else if piece.kind == PieceKind::King {
            if from.file == 4 && to.file == 6 {
                MoveKind::CastleKingside
            } else if from.file == 4 && to.file == 2 {
                MoveKind::CastleQueenside
            } else {
                MoveKind::Normal
            }
        } else if piece.kind == PieceKind::Pawn {
            let rank_diff = (to.rank as i8 - from.rank as i8).unsigned_abs();
            if rank_diff == 2 {
                MoveKind::DoublePawnPush
            } else if from.file != to.file && board.piece_at(to).is_none() {
                MoveKind::EnPassant
            } else {
                MoveKind::Normal
            }
        } else {
            MoveKind::Normal
        };

        Some(Move { from, to, kind })
    }
}

/// Apply a move to a board, returning the new board state.
/// Incrementally updates the Zobrist hash.
pub fn make_move(board: &Board, mv: &Move) -> Board {
    use crate::zobrist;

    let mut new_board = board.clone();
    let piece = board.piece_at(mv.from).expect("No piece at from square");
    let mut h = board.hash;
    let from_idx = mv.from.index() as u32;
    let to_idx = mv.to.index() as u32;

    // Remove old castling hash
    let old_castling = board.castling.as_u8();
    h ^= zobrist::castling_key(old_castling);

    // Remove old en passant hash
    if let Some(ep) = board.en_passant {
        h ^= zobrist::ep_key(ep.file as u32);
    }

    // Remove piece from source
    h ^= zobrist::piece_key(piece.color, piece.kind, from_idx);
    new_board.set_piece(mv.from, None);

    // Remove captured piece at destination (if any, and not en passant)
    if mv.kind != MoveKind::EnPassant {
        if let Some(captured) = board.piece_at(mv.to) {
            h ^= zobrist::piece_key(captured.color, captured.kind, to_idx);
        }
    }

    match mv.kind {
        MoveKind::Normal | MoveKind::DoublePawnPush => {
            new_board.set_piece(mv.to, Some(piece));
            h ^= zobrist::piece_key(piece.color, piece.kind, to_idx);
        }
        MoveKind::EnPassant => {
            new_board.set_piece(mv.to, Some(piece));
            h ^= zobrist::piece_key(piece.color, piece.kind, to_idx);
            // Remove captured pawn
            let captured_rank = match piece.color {
                Color::White => mv.to.rank - 1,
                Color::Black => mv.to.rank + 1,
            };
            let cap_sq = Square::new(mv.to.file, captured_rank);
            let cap_idx = cap_sq.index() as u32;
            h ^= zobrist::piece_key(piece.color.opposite(), PieceKind::Pawn, cap_idx);
            new_board.set_piece(cap_sq, None);
        }
        MoveKind::CastleKingside => {
            new_board.set_piece(mv.to, Some(piece));
            h ^= zobrist::piece_key(piece.color, piece.kind, to_idx);
            let rank = mv.from.rank;
            let rook = Piece::new(PieceKind::Rook, piece.color);
            let rook_from = Square::new(7, rank).index() as u32;
            let rook_to = Square::new(5, rank).index() as u32;
            h ^= zobrist::piece_key(piece.color, PieceKind::Rook, rook_from);
            h ^= zobrist::piece_key(piece.color, PieceKind::Rook, rook_to);
            new_board.set_piece(Square::new(7, rank), None);
            new_board.set_piece(Square::new(5, rank), Some(rook));
        }
        MoveKind::CastleQueenside => {
            new_board.set_piece(mv.to, Some(piece));
            h ^= zobrist::piece_key(piece.color, piece.kind, to_idx);
            let rank = mv.from.rank;
            let rook = Piece::new(PieceKind::Rook, piece.color);
            let rook_from = Square::new(0, rank).index() as u32;
            let rook_to = Square::new(3, rank).index() as u32;
            h ^= zobrist::piece_key(piece.color, PieceKind::Rook, rook_from);
            h ^= zobrist::piece_key(piece.color, PieceKind::Rook, rook_to);
            new_board.set_piece(Square::new(0, rank), None);
            new_board.set_piece(Square::new(3, rank), Some(rook));
        }
        MoveKind::Promotion(promo_kind) => {
            let promo_piece = Piece::new(promo_kind, piece.color);
            new_board.set_piece(mv.to, Some(promo_piece));
            h ^= zobrist::piece_key(piece.color, promo_kind, to_idx);
        }
    }

    // Update en passant square
    new_board.en_passant = if mv.kind == MoveKind::DoublePawnPush {
        let ep_rank = match piece.color {
            Color::White => mv.from.rank + 1,
            Color::Black => mv.from.rank - 1,
        };
        let ep_sq = Square::new(mv.from.file, ep_rank);
        h ^= zobrist::ep_key(ep_sq.file as u32);
        Some(ep_sq)
    } else {
        None
    };

    // Update castling rights
    if piece.kind == PieceKind::King {
        match piece.color {
            Color::White => {
                new_board.castling.white_kingside = false;
                new_board.castling.white_queenside = false;
            }
            Color::Black => {
                new_board.castling.black_kingside = false;
                new_board.castling.black_queenside = false;
            }
        }
    }
    if mv.from == Square::new(0, 0) || mv.to == Square::new(0, 0) {
        new_board.castling.white_queenside = false;
    }
    if mv.from == Square::new(7, 0) || mv.to == Square::new(7, 0) {
        new_board.castling.white_kingside = false;
    }
    if mv.from == Square::new(0, 7) || mv.to == Square::new(0, 7) {
        new_board.castling.black_queenside = false;
    }
    if mv.from == Square::new(7, 7) || mv.to == Square::new(7, 7) {
        new_board.castling.black_kingside = false;
    }

    // Hash new castling rights
    h ^= zobrist::castling_key(new_board.castling.as_u8());

    // Flip side to move
    h ^= zobrist::side_key();

    // Update clocks
    if piece.kind == PieceKind::Pawn || board.piece_at(mv.to).is_some() {
        new_board.halfmove_clock = 0;
    } else {
        new_board.halfmove_clock += 1;
    }

    if board.side_to_move == Color::Black {
        new_board.fullmove_number += 1;
    }

    new_board.side_to_move = board.side_to_move.opposite();
    new_board.hash = h;
    new_board
}
