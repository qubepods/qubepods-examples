/// Bitboard-based attack tables with magic numbers for slider pieces.
///
/// A bitboard is a u64 where bit N represents square N (a1=0, b1=1, ..., h8=63).
/// File = sq % 8, Rank = sq / 8.

pub type Bitboard = u64;

// Precomputed tables — initialized once at startup
static mut KNIGHT_ATTACKS: [Bitboard; 64] = [0; 64];
static mut KING_ATTACKS: [Bitboard; 64] = [0; 64];
static mut PAWN_ATTACKS: [[Bitboard; 64]; 2] = [[0; 64]; 2]; // [color][square]

// Magic bitboard tables for sliders
static mut ROOK_MAGICS: [MagicEntry; 64] = [MagicEntry::EMPTY; 64];
static mut BISHOP_MAGICS: [MagicEntry; 64] = [MagicEntry::EMPTY; 64];
static mut ROOK_TABLE: Vec<Bitboard> = Vec::new();
static mut BISHOP_TABLE: Vec<Bitboard> = Vec::new();

static INIT: std::sync::Once = std::sync::Once::new();

/// Call once at program start to initialize all attack tables.
pub fn init() {
    INIT.call_once(|| unsafe { init_tables() });
}

#[derive(Clone, Copy)]
pub struct MagicEntry {
    pub mask: Bitboard,
    pub magic: u64,
    pub shift: u32,
    pub offset: usize,
}

impl MagicEntry {
    const EMPTY: Self = Self { mask: 0, magic: 0, shift: 0, offset: 0 };
}

// --- Public API ---

#[inline]
pub fn knight_attacks(sq: u32) -> Bitboard {
    unsafe { KNIGHT_ATTACKS[sq as usize] }
}

#[inline]
pub fn king_attacks(sq: u32) -> Bitboard {
    unsafe { KING_ATTACKS[sq as usize] }
}

/// color: 0 = white, 1 = black
#[inline]
pub fn pawn_attacks(sq: u32, color: usize) -> Bitboard {
    unsafe { PAWN_ATTACKS[color][sq as usize] }
}

#[inline]
pub fn rook_attacks(sq: u32, occupied: Bitboard) -> Bitboard {
    unsafe {
        let entry = &ROOK_MAGICS[sq as usize];
        let idx = magic_index(entry, occupied);
        ROOK_TABLE[idx]
    }
}

#[inline]
pub fn bishop_attacks(sq: u32, occupied: Bitboard) -> Bitboard {
    unsafe {
        let entry = &BISHOP_MAGICS[sq as usize];
        let idx = magic_index(entry, occupied);
        BISHOP_TABLE[idx]
    }
}

#[inline]
pub fn queen_attacks(sq: u32, occupied: Bitboard) -> Bitboard {
    rook_attacks(sq, occupied) | bishop_attacks(sq, occupied)
}

// --- Utility functions ---

#[inline]
pub const fn square_bb(sq: u32) -> Bitboard {
    1u64 << sq
}

#[inline]
pub const fn file_of(sq: u32) -> u32 {
    sq & 7
}

#[inline]
pub const fn rank_of(sq: u32) -> u32 {
    sq >> 3
}

pub fn bb_to_squares(mut bb: Bitboard) -> Vec<u32> {
    let mut sqs = Vec::new();
    while bb != 0 {
        sqs.push(bb.trailing_zeros());
        bb &= bb - 1;
    }
    sqs
}

// --- Magic index computation ---

#[inline]
fn magic_index(entry: &MagicEntry, occupied: Bitboard) -> usize {
    let relevant = occupied & entry.mask;
    let hash = relevant.wrapping_mul(entry.magic);
    entry.offset + (hash >> entry.shift) as usize
}

// --- Initialization ---

unsafe fn init_tables() {
    init_knight_attacks();
    init_king_attacks();
    init_pawn_attacks();
    init_rook_magics();
    init_bishop_magics();
}

unsafe fn init_knight_attacks() {
    let jumps: [(i32, i32); 8] = [(1,2),(2,1),(2,-1),(1,-2),(-1,-2),(-2,-1),(-2,1),(-1,2)];
    for sq in 0..64u32 {
        let f = file_of(sq) as i32;
        let r = rank_of(sq) as i32;
        let mut bb = 0u64;
        for (df, dr) in &jumps {
            let nf = f + df;
            let nr = r + dr;
            if (0..8).contains(&nf) && (0..8).contains(&nr) {
                bb |= 1u64 << (nr * 8 + nf);
            }
        }
        KNIGHT_ATTACKS[sq as usize] = bb;
    }
}

unsafe fn init_king_attacks() {
    for sq in 0..64u32 {
        let f = file_of(sq) as i32;
        let r = rank_of(sq) as i32;
        let mut bb = 0u64;
        for df in -1..=1i32 {
            for dr in -1..=1i32 {
                if df == 0 && dr == 0 { continue; }
                let nf = f + df;
                let nr = r + dr;
                if (0..8).contains(&nf) && (0..8).contains(&nr) {
                    bb |= 1u64 << (nr * 8 + nf);
                }
            }
        }
        KING_ATTACKS[sq as usize] = bb;
    }
}

unsafe fn init_pawn_attacks() {
    for sq in 0..64u32 {
        let f = file_of(sq) as i32;
        let r = rank_of(sq) as i32;
        // White pawns attack up-left and up-right
        let mut w = 0u64;
        if r < 7 {
            if f > 0 { w |= 1u64 << ((r + 1) * 8 + f - 1); }
            if f < 7 { w |= 1u64 << ((r + 1) * 8 + f + 1); }
        }
        PAWN_ATTACKS[0][sq as usize] = w;
        // Black pawns attack down-left and down-right
        let mut b = 0u64;
        if r > 0 {
            if f > 0 { b |= 1u64 << ((r - 1) * 8 + f - 1); }
            if f < 7 { b |= 1u64 << ((r - 1) * 8 + f + 1); }
        }
        PAWN_ATTACKS[1][sq as usize] = b;
    }
}

// --- Rook magic bitboards ---

/// Rook relevant occupancy mask (excludes edges the rook is on)
fn rook_mask(sq: u32) -> Bitboard {
    let f = file_of(sq) as i32;
    let r = rank_of(sq) as i32;
    let mut mask = 0u64;
    // Rank (exclude file 0 and 7 edges)
    for nf in (f + 1)..7 { mask |= 1u64 << (r * 8 + nf); }
    for nf in 1..f { mask |= 1u64 << (r * 8 + nf); }
    // File (exclude rank 0 and 7 edges)
    for nr in (r + 1)..7 { mask |= 1u64 << (nr * 8 + f); }
    for nr in 1..r { mask |= 1u64 << (nr * 8 + f); }
    mask
}

/// Rook attacks for a given occupancy (not just relevant bits)
fn rook_attacks_slow(sq: u32, occupied: Bitboard) -> Bitboard {
    let f = file_of(sq) as i32;
    let r = rank_of(sq) as i32;
    let mut attacks = 0u64;
    for (df, dr) in &[(1i32,0),(- 1,0),(0,1),(0,-1)] {
        let mut nf = f + df;
        let mut nr = r + dr;
        while (0..8).contains(&nf) && (0..8).contains(&nr) {
            let bit = 1u64 << (nr * 8 + nf);
            attacks |= bit;
            if occupied & bit != 0 { break; }
            nf += df;
            nr += dr;
        }
    }
    attacks
}

/// Bishop relevant occupancy mask
fn bishop_mask(sq: u32) -> Bitboard {
    let f = file_of(sq) as i32;
    let r = rank_of(sq) as i32;
    let mut mask = 0u64;
    for (df, dr) in &[(1i32,1),(1,-1),(-1,1),(-1,-1)] {
        let mut nf = f + df;
        let mut nr = r + dr;
        while (1..7).contains(&nf) && (1..7).contains(&nr) {
            mask |= 1u64 << (nr * 8 + nf);
            nf += df;
            nr += dr;
        }
    }
    mask
}

/// Bishop attacks for a given occupancy
fn bishop_attacks_slow(sq: u32, occupied: Bitboard) -> Bitboard {
    let f = file_of(sq) as i32;
    let r = rank_of(sq) as i32;
    let mut attacks = 0u64;
    for (df, dr) in &[(1i32,1),(1,-1),(-1,1),(-1,-1)] {
        let mut nf = f + df;
        let mut nr = r + dr;
        while (0..8).contains(&nf) && (0..8).contains(&nr) {
            let bit = 1u64 << (nr * 8 + nf);
            attacks |= bit;
            if occupied & bit != 0 { break; }
            nf += df;
            nr += dr;
        }
    }
    attacks
}

/// Enumerate all subsets of a mask (Carry-Rippler trick)
fn enumerate_subsets(mask: Bitboard) -> Vec<Bitboard> {
    let mut subsets = Vec::new();
    let mut subset = 0u64;
    loop {
        subsets.push(subset);
        if subset == mask { break; }
        subset = subset.wrapping_sub(mask) & mask;
    }
    subsets
}

// Pre-found magic numbers for each square (rooks)
// These are known-good magics — avoids runtime search
const ROOK_MAGIC_NUMBERS: [u64; 64] = [
    0x0080001020400080, 0x0040001000200040, 0x0080081000200080, 0x0080040800100080,
    0x0080020400080080, 0x0080010200040080, 0x0080008001000200, 0x0080002040800100,
    0x0000800020400080, 0x0000400020005000, 0x0000801000200080, 0x0000800800100080,
    0x0000800400080080, 0x0000800200040080, 0x0000800100020080, 0x0000800040800100,
    0x0000208000400080, 0x0000404000201000, 0x0000808010002000, 0x0000808008001000,
    0x0000808004000800, 0x0000808002000400, 0x0000010100020004, 0x0000020000408104,
    0x0000208080004000, 0x0000200040005000, 0x0000100080200080, 0x0000080080100080,
    0x0000040080080080, 0x0000020080040080, 0x0000010080800200, 0x0000800080004100,
    0x0000204000800080, 0x0000200040401000, 0x0000100080802000, 0x0000080080801000,
    0x0000040080800800, 0x0000020080800400, 0x0000020001010004, 0x0000800040800100,
    0x0000204000808000, 0x0000200040008080, 0x0000100020008080, 0x0000080010008080,
    0x0000040008008080, 0x0000020004008080, 0x0000010002008080, 0x0000004081020004,
    0x0000204000800080, 0x0000200040008080, 0x0000100020008080, 0x0000080010008080,
    0x0000040008008080, 0x0000020004008080, 0x0000800100020080, 0x0000800041000080,
    0x00FFFCDDFCED714A, 0x007FFCDDFCED714A, 0x003FFFCDFFD88096, 0x0000040810002101,
    0x0001000204080011, 0x0001000204000801, 0x0001000082000401, 0x0001FFFAABFAD1A2,
];

const BISHOP_MAGIC_NUMBERS: [u64; 64] = [
    0x0002020202020200, 0x0002020202020000, 0x0004010202000000, 0x0004040080000000,
    0x0001104000000000, 0x0000821040000000, 0x0000410410400000, 0x0000104104104000,
    0x0000040404040400, 0x0000020202020200, 0x0000040102020000, 0x0000040400800000,
    0x0000011040000000, 0x0000008210400000, 0x0000004104104000, 0x0000002082082000,
    0x0004000808080800, 0x0002000404040400, 0x0001000202020200, 0x0000800802004000,
    0x0000800400A00000, 0x0000200100884000, 0x0000400082082000, 0x0000200041041000,
    0x0002080010101000, 0x0001040008080800, 0x0000208004010400, 0x0000404004010200,
    0x0000840000802000, 0x0000404002011000, 0x0000808001041000, 0x0000404000820800,
    0x0001041000202000, 0x0000820800101000, 0x0000104400080800, 0x0000020080080080,
    0x0000404040040100, 0x0000808100020100, 0x0001010100020800, 0x0000808080010400,
    0x0000820820004000, 0x0000410410002000, 0x0000082088001000, 0x0000002011000800,
    0x0000080100400400, 0x0001010101000200, 0x0002020202000400, 0x0001010101000200,
    0x0000410410400000, 0x0000208208200000, 0x0000002084100000, 0x0000000020880000,
    0x0000001002020000, 0x0000040408020000, 0x0004040404040000, 0x0002020202020000,
    0x0000104104104000, 0x0000002082082000, 0x0000000020841000, 0x0000000000208800,
    0x0000000010020200, 0x0000000404080200, 0x0000040404040400, 0x0002020202020200,
];

// Rook shift amounts (64 - number of relevant bits in mask)
const ROOK_SHIFTS: [u32; 64] = [
    52, 53, 53, 53, 53, 53, 53, 52,
    53, 54, 54, 54, 54, 54, 54, 53,
    53, 54, 54, 54, 54, 54, 54, 53,
    53, 54, 54, 54, 54, 54, 54, 53,
    53, 54, 54, 54, 54, 54, 54, 53,
    53, 54, 54, 54, 54, 54, 54, 53,
    53, 54, 54, 54, 54, 54, 54, 53,
    52, 53, 53, 53, 53, 53, 53, 52,
];

const BISHOP_SHIFTS: [u32; 64] = [
    58, 59, 59, 59, 59, 59, 59, 58,
    59, 59, 59, 59, 59, 59, 59, 59,
    59, 59, 57, 57, 57, 57, 59, 59,
    59, 59, 57, 55, 55, 57, 59, 59,
    59, 59, 57, 55, 55, 57, 59, 59,
    59, 59, 57, 57, 57, 57, 59, 59,
    59, 59, 59, 59, 59, 59, 59, 59,
    58, 59, 59, 59, 59, 59, 59, 58,
];

unsafe fn init_rook_magics() {
    let mut total_size = 0usize;
    // First pass: compute total table size
    for sq in 0..64u32 {
        let bits = 64 - ROOK_SHIFTS[sq as usize];
        total_size += 1 << bits;
    }
    ROOK_TABLE = vec![0u64; total_size];
    
    let mut offset = 0usize;
    for sq in 0..64u32 {
        let mask = rook_mask(sq);
        let magic = ROOK_MAGIC_NUMBERS[sq as usize];
        let shift = ROOK_SHIFTS[sq as usize];
        let bits = 64 - shift;
        let size = 1usize << bits;
        
        ROOK_MAGICS[sq as usize] = MagicEntry { mask, magic, shift, offset };
        
        // Fill the table for all occupancy subsets
        for subset in enumerate_subsets(mask) {
            let attacks = rook_attacks_slow(sq, subset);
            let idx = ((subset.wrapping_mul(magic)) >> shift) as usize;
            ROOK_TABLE[offset + idx] = attacks;
        }
        
        offset += size;
    }
}

unsafe fn init_bishop_magics() {
    let mut total_size = 0usize;
    for sq in 0..64u32 {
        let bits = 64 - BISHOP_SHIFTS[sq as usize];
        total_size += 1 << bits;
    }
    BISHOP_TABLE = vec![0u64; total_size];
    
    let mut offset = 0usize;
    for sq in 0..64u32 {
        let mask = bishop_mask(sq);
        let magic = BISHOP_MAGIC_NUMBERS[sq as usize];
        let shift = BISHOP_SHIFTS[sq as usize];
        let bits = 64 - shift;
        let size = 1usize << bits;
        
        BISHOP_MAGICS[sq as usize] = MagicEntry { mask, magic, shift, offset };
        
        for subset in enumerate_subsets(mask) {
            let attacks = bishop_attacks_slow(sq, subset);
            let idx = ((subset.wrapping_mul(magic)) >> shift) as usize;
            BISHOP_TABLE[offset + idx] = attacks;
        }
        
        offset += size;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        init();
        // Knight on e4 (square 28) should attack 8 squares
        let n = knight_attacks(28).count_ones();
        assert_eq!(n, 8, "Knight on e4 should attack 8 squares, got {}", n);
        
        // Knight on a1 (square 0) should attack 2 squares
        let n = knight_attacks(0).count_ones();
        assert_eq!(n, 2, "Knight on a1 should attack 2 squares, got {}", n);
        
        // King on e4 should attack 8 squares
        let k = king_attacks(28).count_ones();
        assert_eq!(k, 8);
        
        // King on a1 should attack 3 squares
        let k = king_attacks(0).count_ones();
        assert_eq!(k, 3);
    }
    
    #[test]
    fn test_rook_attacks() {
        init();
        // Rook on e4 (28), empty board
        let attacks = rook_attacks(28, 0);
        assert_eq!(attacks.count_ones(), 14, "Rook on e4 empty board should attack 14 squares");
        
        // Rook on a1 (0), empty board
        let attacks = rook_attacks(0, 0);
        assert_eq!(attacks.count_ones(), 14);
        
        // Rook on e4 with blocker on e6 (44)
        let occupied = square_bb(44); // e6
        let attacks = rook_attacks(28, occupied);
        // Should attack e5, e6 (blocked), and full rank + e3,e2,e1
        assert!(attacks & square_bb(36) != 0, "Should attack e5");
        assert!(attacks & square_bb(44) != 0, "Should attack e6 (capture)");
        assert!(attacks & square_bb(52) == 0, "Should NOT attack e7 (blocked)");
    }
    
    #[test]
    fn test_bishop_attacks() {
        init();
        // Bishop on e4 (28), empty board
        let attacks = bishop_attacks(28, 0);
        assert_eq!(attacks.count_ones(), 13, "Bishop on e4 empty board should attack 13 squares");
    }
    
    #[test]
    fn test_queen_attacks() {
        init();
        // Queen on e4 empty board = rook + bishop
        let q = queen_attacks(28, 0);
        assert_eq!(q.count_ones(), 27, "Queen on e4 empty board should attack 27 squares");
    }
    
    #[test]
    fn test_pawn_attacks() {
        init();
        // White pawn on e4 (28) attacks d5 and f5
        let w = pawn_attacks(28, 0);
        assert!(w & square_bb(35) != 0, "White pawn e4 should attack d5");
        assert!(w & square_bb(37) != 0, "White pawn e4 should attack f5");
        assert_eq!(w.count_ones(), 2);
        
        // Black pawn on e5 (36) attacks d4 and f4
        let b = pawn_attacks(36, 1);
        assert!(b & square_bb(27) != 0, "Black pawn e5 should attack d4");
        assert!(b & square_bb(29) != 0, "Black pawn e5 should attack f4");
    }
}
