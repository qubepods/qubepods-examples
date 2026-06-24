/// Transposition Table — hash table for storing search results.
///
/// Lock-free design using atomic operations, safe for Lazy SMP.
/// Each entry stores: hash verification, depth, score, best move, node type.

use std::sync::atomic::{AtomicU64, Ordering};

/// Node type from alpha-beta search
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum NodeType {
    /// Exact score (PV node)
    Exact = 0,
    /// Lower bound (failed high / cut node)
    LowerBound = 1,
    /// Upper bound (failed low / all node)
    UpperBound = 2,
}

/// A transposition table entry, packed into 16 bytes.
///
/// Layout:
///   data word (u64):  [hash_verify:16][depth:8][node_type:2][score:16][best_move:22]
///   key word (u64):   hash XOR data (for lock-free verification)
#[derive(Clone, Copy)]
pub struct TTEntry {
    pub hash_key: u64,   // upper bits of position hash for verification
    pub depth: i8,       // search depth
    pub score: i16,      // evaluation score
    pub best_move: u32,  // packed move (from:6, to:6, promo:4 = 16 bits)
    pub node_type: NodeType,
    pub age: u8,         // generation counter for replacement
}

impl TTEntry {
    pub const EMPTY: Self = Self {
        hash_key: 0, depth: 0, score: 0, best_move: 0,
        node_type: NodeType::Exact, age: 0,
    };
    
    /// Pack a move into 16 bits: from(6) | to(6) | promo(4)
    pub fn pack_move(from: u32, to: u32, promo: u32) -> u32 {
        (from & 0x3F) | ((to & 0x3F) << 6) | ((promo & 0xF) << 12)
    }
    
    pub fn unpack_from(mv: u32) -> u32 { mv & 0x3F }
    pub fn unpack_to(mv: u32) -> u32 { (mv >> 6) & 0x3F }
    pub fn unpack_promo(mv: u32) -> u32 { (mv >> 12) & 0xF }
    
    /// Pack entry into two u64s for atomic storage
    fn pack(&self) -> (u64, u64) {
        let data: u64 = (self.hash_key & 0xFFFF_0000_0000_0000)
            | ((self.depth as u8 as u64) << 40)
            | ((self.node_type as u64) << 38)
            | (((self.score as u16) as u64) << 22)
            | (self.best_move as u64 & 0x003F_FFFF)
            | ((self.age as u64) << 16);
        let key = self.hash_key ^ data;
        (key, data)
    }
    
    /// Unpack entry from two u64s
    fn unpack(key: u64, data: u64) -> Self {
        let hash_key = key ^ data;
        let depth = ((data >> 40) & 0xFF) as i8;
        let node_type = match (data >> 38) & 3 {
            0 => NodeType::Exact,
            1 => NodeType::LowerBound,
            _ => NodeType::UpperBound,
        };
        let score = ((data >> 22) & 0xFFFF) as i16;
        let best_move = (data & 0x003F_FFFF) as u32;
        let age = ((data >> 16) & 0x3F) as u8;
        Self { hash_key, depth, score, best_move, node_type, age }
    }
}

/// Atomic entry pair for lock-free access
struct AtomicTTEntry {
    key: AtomicU64,
    data: AtomicU64,
}

impl AtomicTTEntry {
    fn new() -> Self {
        Self {
            key: AtomicU64::new(0),
            data: AtomicU64::new(0),
        }
    }
    
    fn store(&self, entry: &TTEntry) {
        let (key, data) = entry.pack();
        self.key.store(key, Ordering::Relaxed);
        self.data.store(data, Ordering::Relaxed);
    }
    
    fn load(&self, hash: u64) -> Option<TTEntry> {
        let key = self.key.load(Ordering::Relaxed);
        let data = self.data.load(Ordering::Relaxed);
        let entry = TTEntry::unpack(key, data);
        // Verify hash matches (XOR scheme catches torn reads)
        if entry.hash_key == hash {
            Some(entry)
        } else {
            None
        }
    }
}

/// The transposition table
pub struct TranspositionTable {
    entries: Vec<AtomicTTEntry>,
    mask: usize,
    generation: u8,
}

// Safety: AtomicTTEntry uses atomics, safe to share across threads
unsafe impl Send for TranspositionTable {}
unsafe impl Sync for TranspositionTable {}

impl TranspositionTable {
    /// Create a new table with the given size in MB
    pub fn new(size_mb: usize) -> Self {
        let entry_size = 16; // two u64s
        let num_entries = (size_mb * 1024 * 1024) / entry_size;
        // Round down to power of 2 for mask-based indexing
        let num_entries = num_entries.next_power_of_two() >> 1;
        let num_entries = num_entries.max(1024);
        
        let mut entries = Vec::with_capacity(num_entries);
        for _ in 0..num_entries {
            entries.push(AtomicTTEntry::new());
        }
        
        Self {
            entries,
            mask: num_entries - 1,
            generation: 0,
        }
    }
    
    /// Probe the table for a position
    pub fn probe(&self, hash: u64) -> Option<TTEntry> {
        let idx = (hash as usize) & self.mask;
        self.entries[idx].load(hash)
    }
    
    /// Store an entry in the table
    pub fn store(&self, hash: u64, depth: i8, score: i16, best_move: u32, node_type: NodeType) {
        let idx = (hash as usize) & self.mask;
        
        // Replacement: always replace if deeper or same depth or different position
        if let Some(existing) = self.entries[idx].load(hash) {
            // Keep existing entry if it's deeper and from current generation
            if existing.depth > depth && existing.age == self.generation {
                return;
            }
        }
        
        let entry = TTEntry {
            hash_key: hash,
            depth,
            score,
            best_move,
            node_type,
            age: self.generation,
        };
        self.entries[idx].store(&entry);
    }
    
    /// Increment generation (call at start of each new search)
    pub fn new_search(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }
    
    /// Clear the entire table
    pub fn clear(&self) {
        for entry in &self.entries {
            entry.key.store(0, Ordering::Relaxed);
            entry.data.store(0, Ordering::Relaxed);
        }
    }
    
    /// Occupancy percentage (for UCI hashfull)
    pub fn hashfull(&self) -> u32 {
        let sample = self.entries.len().min(1000);
        let mut used = 0u32;
        for i in 0..sample {
            if self.entries[i].key.load(Ordering::Relaxed) != 0 {
                used += 1;
            }
        }
        (used * 1000) / sample as u32
    }
    
    pub fn len(&self) -> usize { self.entries.len() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_probe() {
        let tt = TranspositionTable::new(1); // 1 MB
        let hash = 0xDEADBEEF12345678u64;
        
        tt.store(hash, 5, 100, TTEntry::pack_move(12, 28, 0), NodeType::Exact);
        
        let entry = tt.probe(hash).expect("Should find stored entry");
        assert_eq!(entry.depth, 5);
        assert_eq!(entry.score, 100);
        assert_eq!(TTEntry::unpack_from(entry.best_move), 12);
        assert_eq!(TTEntry::unpack_to(entry.best_move), 28);
        assert_eq!(entry.node_type, NodeType::Exact);
    }
    
    #[test]
    fn test_miss() {
        let tt = TranspositionTable::new(1);
        assert!(tt.probe(0x123456789ABCDEF0).is_none());
    }
    
    #[test]
    fn test_replacement() {
        let tt = TranspositionTable::new(1);
        let hash = 0xAAAABBBBCCCCDDDD;
        
        tt.store(hash, 3, 50, 0, NodeType::LowerBound);
        tt.store(hash, 6, 75, 0, NodeType::Exact);
        
        let entry = tt.probe(hash).unwrap();
        // Deeper entry should replace
        assert_eq!(entry.depth, 6);
        assert_eq!(entry.score, 75);
    }
    
    #[test]
    fn test_pack_move() {
        let mv = TTEntry::pack_move(4, 28, 5); // e1 to e4, promo=queen
        assert_eq!(TTEntry::unpack_from(mv), 4);
        assert_eq!(TTEntry::unpack_to(mv), 28);
        assert_eq!(TTEntry::unpack_promo(mv), 5);
    }
}
