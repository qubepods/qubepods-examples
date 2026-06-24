use crate::board::{Board, Color, PieceKind, Square};
use crate::bitboard;
use crate::moves::{Move, MoveKind};

/// Generate all pseudo-legal moves for the side to move, then filter to legal ones.
pub fn legal_moves(board: &Board) -> Vec<Move> {
    let pseudo = pseudo_legal_moves(board);
    pseudo
        .into_iter()
        .filter(|mv| {
            let new_board = crate::moves::make_move(board, mv);
            !is_in_check(&new_board, board.side_to_move)
        })
        .collect()
}

/// Generate only legal capture moves (for quiescence search).
/// Includes en passant and promotions (which are tactical).
pub fn legal_captures(board: &Board) -> Vec<Move> {
    let pseudo = pseudo_legal_moves(board);
    pseudo
        .into_iter()
        .filter(|mv| {
            let is_capture = board.piece_at(mv.to).is_some()
                || matches!(mv.kind, MoveKind::EnPassant)
                || matches!(mv.kind, MoveKind::Promotion(_));
            if !is_capture {
                return false;
            }
            let new_board = crate::moves::make_move(board, mv);
            !is_in_check(&new_board, board.side_to_move)
        })
        .collect()
}

/// MVV-LVA score for move ordering (higher = search first)
pub fn mvv_lva_score(board: &Board, mv: &Move) -> i32 {
    let victim_value = board.piece_at(mv.to).map_or(0, |p| piece_value(p.kind));
    let attacker_value = board.piece_at(mv.from).map_or(0, |p| piece_value(p.kind));
    // Promotions are high value
    let promo_value = match mv.kind {
        MoveKind::Promotion(k) => piece_value(k),
        MoveKind::EnPassant => 100, // pawn capture
        _ => 0,
    };
    victim_value * 10 - attacker_value + promo_value
}

fn piece_value(kind: PieceKind) -> i32 {
    match kind {
        PieceKind::Pawn => 100,
        PieceKind::Knight => 320,
        PieceKind::Bishop => 330,
        PieceKind::Rook => 500,
        PieceKind::Queen => 900,
        PieceKind::King => 0,
    }
}

/// Is the given color's king in check on this board?
pub fn is_in_check(board: &Board, color: Color) -> bool {
    let king_sq = find_king(board, color);
    match king_sq {
        Some(sq) => is_attacked(board, sq, color.opposite()),
        None => false,
    }
}

fn find_king(board: &Board, color: Color) -> Option<Square> {
    let king_bb = board.pieces(PieceKind::King, color);
    if king_bb == 0 { return None; }
    let idx = king_bb.trailing_zeros();
    Some(Square::new((idx & 7) as u8, (idx >> 3) as u8))
}

/// Is a square attacked by any piece of the given color?
/// Uses pre-maintained bitboards + magic attack tables.
fn is_attacked(board: &Board, target: Square, by_color: Color) -> bool {
    bitboard::init();
    let t = target.index() as u32;
    let occupied = board.occupied();
    let ci = by_color.index();

    // Pawn attacks (reverse lookup)
    let pawn_atk_color = if by_color == Color::White { 1 } else { 0 };
    if bitboard::pawn_attacks(t, pawn_atk_color) & board.piece_bb[PieceKind::Pawn.index()] & board.color_bb[ci] != 0 {
        return true;
    }
    if bitboard::knight_attacks(t) & board.piece_bb[PieceKind::Knight.index()] & board.color_bb[ci] != 0 {
        return true;
    }
    if bitboard::king_attacks(t) & board.piece_bb[PieceKind::King.index()] & board.color_bb[ci] != 0 {
        return true;
    }
    let bq = (board.piece_bb[PieceKind::Bishop.index()] | board.piece_bb[PieceKind::Queen.index()]) & board.color_bb[ci];
    if bitboard::bishop_attacks(t, occupied) & bq != 0 {
        return true;
    }
    let rq = (board.piece_bb[PieceKind::Rook.index()] | board.piece_bb[PieceKind::Queen.index()]) & board.color_bb[ci];
    if bitboard::rook_attacks(t, occupied) & rq != 0 {
        return true;
    }
    false
}

fn pseudo_legal_moves(board: &Board) -> Vec<Move> {
    bitboard::init();
    let mut moves = Vec::with_capacity(64);
    let color = board.side_to_move;
    let ci = color.index();
    let friendly = board.color_bb[ci];
    let occupied = board.occupied();

    // Knights — bitboard generation
    let mut knights = board.piece_bb[PieceKind::Knight.index()] & friendly;
    while knights != 0 {
        let sq_idx = knights.trailing_zeros() as u32;
        let from = Square::new((sq_idx & 7) as u8, (sq_idx >> 3) as u8);
        let mut targets = bitboard::knight_attacks(sq_idx) & !friendly;
        while targets != 0 {
            let to_idx = targets.trailing_zeros() as u32;
            let to = Square::new((to_idx & 7) as u8, (to_idx >> 3) as u8);
            moves.push(Move::new(from, to));
            targets &= targets - 1;
        }
        knights &= knights - 1;
    }

    // Bishops — bitboard generation
    let mut bishops = board.piece_bb[PieceKind::Bishop.index()] & friendly;
    while bishops != 0 {
        let sq_idx = bishops.trailing_zeros() as u32;
        let from = Square::new((sq_idx & 7) as u8, (sq_idx >> 3) as u8);
        let mut targets = bitboard::bishop_attacks(sq_idx, occupied) & !friendly;
        while targets != 0 {
            let to_idx = targets.trailing_zeros() as u32;
            let to = Square::new((to_idx & 7) as u8, (to_idx >> 3) as u8);
            moves.push(Move::new(from, to));
            targets &= targets - 1;
        }
        bishops &= bishops - 1;
    }

    // Rooks — bitboard generation
    let mut rooks = board.piece_bb[PieceKind::Rook.index()] & friendly;
    while rooks != 0 {
        let sq_idx = rooks.trailing_zeros() as u32;
        let from = Square::new((sq_idx & 7) as u8, (sq_idx >> 3) as u8);
        let mut targets = bitboard::rook_attacks(sq_idx, occupied) & !friendly;
        while targets != 0 {
            let to_idx = targets.trailing_zeros() as u32;
            let to = Square::new((to_idx & 7) as u8, (to_idx >> 3) as u8);
            moves.push(Move::new(from, to));
            targets &= targets - 1;
        }
        rooks &= rooks - 1;
    }

    // Queens — bitboard generation
    let mut queens = board.piece_bb[PieceKind::Queen.index()] & friendly;
    while queens != 0 {
        let sq_idx = queens.trailing_zeros() as u32;
        let from = Square::new((sq_idx & 7) as u8, (sq_idx >> 3) as u8);
        let mut targets = bitboard::queen_attacks(sq_idx, occupied) & !friendly;
        while targets != 0 {
            let to_idx = targets.trailing_zeros() as u32;
            let to = Square::new((to_idx & 7) as u8, (to_idx >> 3) as u8);
            moves.push(Move::new(from, to));
            targets &= targets - 1;
        }
        queens &= queens - 1;
    }

    // Pawns — keep existing generator (complex: promotions, en passant, double push)
    let mut pawns = board.piece_bb[PieceKind::Pawn.index()] & friendly;
    while pawns != 0 {
        let sq_idx = pawns.trailing_zeros();
        let from = Square::new((sq_idx & 7) as u8, (sq_idx >> 3) as u8);
        gen_pawn_moves(board, from, color, &mut moves);
        pawns &= pawns - 1;
    }

    // King — keep existing generator (complex: castling)
    let king = board.piece_bb[PieceKind::King.index()] & friendly;
    if king != 0 {
        let sq_idx = king.trailing_zeros();
        let from = Square::new((sq_idx & 7) as u8, (sq_idx >> 3) as u8);
        gen_king_moves(board, from, color, &mut moves);
    }

    moves
}

fn in_bounds(f: i8, r: i8) -> bool {
    (0..8).contains(&f) && (0..8).contains(&r)
}

fn gen_pawn_moves(board: &Board, from: Square, color: Color, moves: &mut Vec<Move>) {
    let dir: i8 = if color == Color::White { 1 } else { -1 };
    let start_rank = if color == Color::White { 1 } else { 6 };
    let promo_rank = if color == Color::White { 7 } else { 0 };
    let f = from.file as i8;
    let r = from.rank as i8;

    // Single push
    let nr = r + dir;
    if in_bounds(f, nr) {
        let to = Square::new(f as u8, nr as u8);
        if board.piece_at(to).is_none() {
            if nr as u8 == promo_rank {
                for kind in [PieceKind::Queen, PieceKind::Rook, PieceKind::Bishop, PieceKind::Knight] {
                    moves.push(Move::with_kind(from, to, MoveKind::Promotion(kind)));
                }
            } else {
                moves.push(Move::new(from, to));

                // Double push
                if from.rank == start_rank {
                    let nr2 = r + dir * 2;
                    let to2 = Square::new(f as u8, nr2 as u8);
                    if board.piece_at(to2).is_none() {
                        moves.push(Move::with_kind(from, to2, MoveKind::DoublePawnPush));
                    }
                }
            }
        }
    }

    // Captures
    for df in [-1i8, 1] {
        let nf = f + df;
        if in_bounds(nf, nr) {
            let to = Square::new(nf as u8, nr as u8);
            let is_capture = board.piece_at(to).map_or(false, |p| p.color != color);
            let is_ep = board.en_passant == Some(to);
            if is_capture || is_ep {
                if nr as u8 == promo_rank {
                    for kind in [PieceKind::Queen, PieceKind::Rook, PieceKind::Bishop, PieceKind::Knight] {
                        moves.push(Move::with_kind(from, to, MoveKind::Promotion(kind)));
                    }
                } else if is_ep {
                    moves.push(Move::with_kind(from, to, MoveKind::EnPassant));
                } else {
                    moves.push(Move::new(from, to));
                }
            }
        }
    }
}

fn gen_king_moves(board: &Board, from: Square, color: Color, moves: &mut Vec<Move>) {
    let f = from.file as i8;
    let r = from.rank as i8;
    for df in -1..=1i8 {
        for dr in -1..=1i8 {
            if df == 0 && dr == 0 { continue; }
            let nf = f + df;
            let nr = r + dr;
            if in_bounds(nf, nr) {
                let to = Square::new(nf as u8, nr as u8);
                if board.piece_at(to).map_or(true, |p| p.color != color) {
                    moves.push(Move::new(from, to));
                }
            }
        }
    }

    // Castling
    let opp = color.opposite();
    if color == Color::White && from == Square::new(4, 0) {
        if board.castling.white_kingside
            && board.piece_at(Square::new(5, 0)).is_none()
            && board.piece_at(Square::new(6, 0)).is_none()
            && !is_attacked(board, Square::new(4, 0), opp)
            && !is_attacked(board, Square::new(5, 0), opp)
            && !is_attacked(board, Square::new(6, 0), opp)
        {
            moves.push(Move::with_kind(from, Square::new(6, 0), MoveKind::CastleKingside));
        }
        if board.castling.white_queenside
            && board.piece_at(Square::new(3, 0)).is_none()
            && board.piece_at(Square::new(2, 0)).is_none()
            && board.piece_at(Square::new(1, 0)).is_none()
            && !is_attacked(board, Square::new(4, 0), opp)
            && !is_attacked(board, Square::new(3, 0), opp)
            && !is_attacked(board, Square::new(2, 0), opp)
        {
            moves.push(Move::with_kind(from, Square::new(2, 0), MoveKind::CastleQueenside));
        }
    }
    if color == Color::Black && from == Square::new(4, 7) {
        if board.castling.black_kingside
            && board.piece_at(Square::new(5, 7)).is_none()
            && board.piece_at(Square::new(6, 7)).is_none()
            && !is_attacked(board, Square::new(4, 7), opp)
            && !is_attacked(board, Square::new(5, 7), opp)
            && !is_attacked(board, Square::new(6, 7), opp)
        {
            moves.push(Move::with_kind(from, Square::new(6, 7), MoveKind::CastleKingside));
        }
        if board.castling.black_queenside
            && board.piece_at(Square::new(3, 7)).is_none()
            && board.piece_at(Square::new(2, 7)).is_none()
            && board.piece_at(Square::new(1, 7)).is_none()
            && !is_attacked(board, Square::new(4, 7), opp)
            && !is_attacked(board, Square::new(3, 7), opp)
            && !is_attacked(board, Square::new(2, 7), opp)
        {
            moves.push(Move::with_kind(from, Square::new(2, 7), MoveKind::CastleQueenside));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_starting_moves() {
        let board = Board::new();
        let moves = legal_moves(&board);
        assert_eq!(moves.len(), 20); // 16 pawn + 4 knight
    }

    #[test]
    fn test_starting_moves_detail() {
        let board = Board::new();
        let mut moves: Vec<String> = legal_moves(&board)
            .iter()
            .map(|m| m.to_uci())
            .collect();
        moves.sort();

        let expected = vec![
            "a2a3", "a2a4", "b1a3", "b1c3",
            "b2b3", "b2b4", "c2c3", "c2c4",
            "d2d3", "d2d4", "e2e3", "e2e4",
            "f2f3", "f2f4", "g1f3", "g1h3",
            "g2g3", "g2g4", "h2h3", "h2h4",
        ];

        assert_eq!(moves, expected, "Starting position legal moves mismatch.\nGot: {:?}", moves);
    }

    #[test]
    fn test_check_detection() {
        // Fool's mate position
        let board = Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();
        assert!(!is_in_check(&board, Color::White));
    }
}
