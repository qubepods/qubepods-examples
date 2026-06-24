#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    White,
    Black,
}

impl Color {
    pub fn opposite(self) -> Color {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }

    #[inline]
    pub fn index(self) -> usize {
        match self { Color::White => 0, Color::Black => 1 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PieceKind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

impl PieceKind {
    #[inline]
    pub fn index(self) -> usize {
        match self {
            PieceKind::Pawn => 0, PieceKind::Knight => 1, PieceKind::Bishop => 2,
            PieceKind::Rook => 3, PieceKind::Queen => 4, PieceKind::King => 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub kind: PieceKind,
    pub color: Color,
}

impl Piece {
    pub fn new(kind: PieceKind, color: Color) -> Self {
        Self { kind, color }
    }

    /// FEN character for this piece
    pub fn fen_char(self) -> char {
        let c = match self.kind {
            PieceKind::Pawn => 'p',
            PieceKind::Knight => 'n',
            PieceKind::Bishop => 'b',
            PieceKind::Rook => 'r',
            PieceKind::Queen => 'q',
            PieceKind::King => 'k',
        };
        match self.color {
            Color::White => c.to_ascii_uppercase(),
            Color::Black => c,
        }
    }

    pub fn from_fen_char(c: char) -> Option<Self> {
        let color = if c.is_ascii_uppercase() {
            Color::White
        } else {
            Color::Black
        };
        let kind = match c.to_ascii_lowercase() {
            'p' => PieceKind::Pawn,
            'n' => PieceKind::Knight,
            'b' => PieceKind::Bishop,
            'r' => PieceKind::Rook,
            'q' => PieceKind::Queen,
            'k' => PieceKind::King,
            _ => return None,
        };
        Some(Piece { kind, color })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Square {
    pub file: u8, // 0-7 (a-h)
    pub rank: u8, // 0-7 (1-8)
}

impl Square {
    pub fn new(file: u8, rank: u8) -> Self {
        debug_assert!(file < 8 && rank < 8);
        Self { file, rank }
    }

    pub fn from_algebraic(s: &str) -> Option<Self> {
        let bytes = s.as_bytes();
        if bytes.len() != 2 {
            return None;
        }
        let file = bytes[0].wrapping_sub(b'a');
        let rank = bytes[1].wrapping_sub(b'1');
        if file < 8 && rank < 8 {
            Some(Square::new(file, rank))
        } else {
            None
        }
    }

    pub fn to_algebraic(self) -> String {
        format!("{}{}", (b'a' + self.file) as char, self.rank + 1)
    }

    pub fn index(self) -> usize {
        (self.rank * 8 + self.file) as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CastlingRights {
    pub white_kingside: bool,
    pub white_queenside: bool,
    pub black_kingside: bool,
    pub black_queenside: bool,
}

impl CastlingRights {
    pub fn all() -> Self {
        Self {
            white_kingside: true,
            white_queenside: true,
            black_kingside: true,
            black_queenside: true,
        }
    }

    pub fn none() -> Self {
        Self {
            white_kingside: false,
            white_queenside: false,
            black_kingside: false,
            black_queenside: false,
        }
    }

    /// Encode as 4-bit mask for Zobrist hashing
    pub fn as_u8(self) -> u8 {
        let mut v = 0u8;
        if self.white_kingside { v |= 1; }
        if self.white_queenside { v |= 2; }
        if self.black_kingside { v |= 4; }
        if self.black_queenside { v |= 8; }
        v
    }
}

#[derive(Debug, Clone)]
pub struct Board {
    pub squares: [Option<Piece>; 64],
    pub side_to_move: Color,
    pub castling: CastlingRights,
    pub en_passant: Option<Square>,
    pub halfmove_clock: u32,
    pub fullmove_number: u32,
    /// Zobrist hash of the current position (incrementally updated)
    pub hash: u64,
    /// Bitboards per piece type (Pawn=0, Knight=1, Bishop=2, Rook=3, Queen=4, King=5)
    pub piece_bb: [u64; 6],
    /// Bitboards per color (White=0, Black=1)
    pub color_bb: [u64; 2],
}

impl Board {
    pub fn new() -> Self {
        Self::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap()
    }

    pub fn piece_at(&self, sq: Square) -> Option<Piece> {
        self.squares[sq.index()]
    }

    /// Update a square and maintain bitboards
    pub fn set_piece(&mut self, sq: Square, piece: Option<Piece>) {
        let idx = sq.index();
        let bit = 1u64 << idx;
        // Remove old piece from bitboards
        if let Some(old) = self.squares[idx] {
            self.piece_bb[old.kind.index()] &= !bit;
            self.color_bb[old.color.index()] &= !bit;
        }
        // Add new piece to bitboards
        if let Some(new) = piece {
            self.piece_bb[new.kind.index()] |= bit;
            self.color_bb[new.color.index()] |= bit;
        }
        self.squares[idx] = piece;
    }

    /// All occupied squares
    #[inline]
    pub fn occupied(&self) -> u64 {
        self.color_bb[0] | self.color_bb[1]
    }

    /// Pieces of a given kind and color
    #[inline]
    pub fn pieces(&self, kind: PieceKind, color: Color) -> u64 {
        self.piece_bb[kind.index()] & self.color_bb[color.index()]
    }

    /// Make a "null move" — flip side to move without moving a piece.
    /// Used for null move pruning. Updates hash incrementally.
    pub fn make_null_move(&self) -> Board {
        let mut b = self.clone();
        let mut h = self.hash;
        // Remove old en passant from hash
        if let Some(ep) = self.en_passant {
            h ^= crate::zobrist::ep_key(ep.file as u32);
        }
        b.en_passant = None;
        // Flip side
        h ^= crate::zobrist::side_key();
        b.side_to_move = self.side_to_move.opposite();
        b.hash = h;
        b
    }

    pub fn from_fen(fen: &str) -> Result<Self, String> {
        let parts: Vec<&str> = fen.split_whitespace().collect();
        if parts.len() < 4 {
            return Err("Invalid FEN: too few fields".into());
        }

        let mut squares = [None; 64];

        // Parse piece placement
        let rows: Vec<&str> = parts[0].split('/').collect();
        if rows.len() != 8 {
            return Err("Invalid FEN: expected 8 ranks".into());
        }
        for (rank_idx, row) in rows.iter().enumerate() {
            let rank = 7 - rank_idx as u8;
            let mut file: u8 = 0;
            for c in row.chars() {
                if let Some(digit) = c.to_digit(10) {
                    file += digit as u8;
                } else if let Some(piece) = Piece::from_fen_char(c) {
                    squares[Square::new(file, rank).index()] = Some(piece);
                    file += 1;
                } else {
                    return Err(format!("Invalid FEN piece: {}", c));
                }
            }
        }

        let side_to_move = match parts[1] {
            "w" => Color::White,
            "b" => Color::Black,
            _ => return Err("Invalid FEN: bad side to move".into()),
        };

        let castling = {
            let s = parts[2];
            if s == "-" {
                CastlingRights::none()
            } else {
                CastlingRights {
                    white_kingside: s.contains('K'),
                    white_queenside: s.contains('Q'),
                    black_kingside: s.contains('k'),
                    black_queenside: s.contains('q'),
                }
            }
        };

        let en_passant = if parts[3] == "-" {
            None
        } else {
            Some(
                Square::from_algebraic(parts[3])
                    .ok_or_else(|| "Invalid FEN: bad en passant square".to_string())?,
            )
        };

        let halfmove_clock = parts.get(4).and_then(|s| s.parse().ok()).unwrap_or(0);
        let fullmove_number = parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(1);

        // Build bitboards from squares
        let mut piece_bb = [0u64; 6];
        let mut color_bb = [0u64; 2];
        for (i, sq) in squares.iter().enumerate() {
            if let Some(p) = sq {
                piece_bb[p.kind.index()] |= 1u64 << i;
                color_bb[p.color.index()] |= 1u64 << i;
            }
        }

        let mut board = Board {
            squares,
            side_to_move,
            castling,
            en_passant,
            halfmove_clock,
            fullmove_number,
            hash: 0,
            piece_bb,
            color_bb,
        };
        board.hash = crate::zobrist::hash_position(&board);
        Ok(board)
    }

    pub fn to_fen(&self) -> String {
        let mut fen = String::new();

        // Piece placement
        for rank in (0..8).rev() {
            let mut empty = 0;
            for file in 0..8 {
                match self.squares[Square::new(file, rank).index()] {
                    Some(piece) => {
                        if empty > 0 {
                            fen.push(char::from_digit(empty, 10).unwrap());
                            empty = 0;
                        }
                        fen.push(piece.fen_char());
                    }
                    None => empty += 1,
                }
            }
            if empty > 0 {
                fen.push(char::from_digit(empty, 10).unwrap());
            }
            if rank > 0 {
                fen.push('/');
            }
        }

        // Side to move
        fen.push(' ');
        fen.push(match self.side_to_move {
            Color::White => 'w',
            Color::Black => 'b',
        });

        // Castling
        fen.push(' ');
        let mut castling_str = String::new();
        if self.castling.white_kingside {
            castling_str.push('K');
        }
        if self.castling.white_queenside {
            castling_str.push('Q');
        }
        if self.castling.black_kingside {
            castling_str.push('k');
        }
        if self.castling.black_queenside {
            castling_str.push('q');
        }
        if castling_str.is_empty() {
            fen.push('-');
        } else {
            fen.push_str(&castling_str);
        }

        // En passant
        fen.push(' ');
        match self.en_passant {
            Some(sq) => fen.push_str(&sq.to_algebraic()),
            None => fen.push('-'),
        }

        // Halfmove clock and fullmove number
        fen.push_str(&format!(" {} {}", self.halfmove_clock, self.fullmove_number));

        fen
    }
}

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_starting_position() {
        let board = Board::new();
        assert_eq!(board.side_to_move, Color::White);
        assert_eq!(
            board.piece_at(Square::new(4, 0)),
            Some(Piece::new(PieceKind::King, Color::White))
        );
        assert_eq!(
            board.piece_at(Square::new(4, 7)),
            Some(Piece::new(PieceKind::King, Color::Black))
        );
    }

    #[test]
    fn test_fen_roundtrip() {
        let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
        let board = Board::from_fen(fen).unwrap();
        assert_eq!(board.to_fen(), fen);
    }

    #[test]
    fn test_square_algebraic() {
        let sq = Square::from_algebraic("e4").unwrap();
        assert_eq!(sq.file, 4);
        assert_eq!(sq.rank, 3);
        assert_eq!(sq.to_algebraic(), "e4");
    }
}
