use crate::movegen::Move;

/// Number of TT slots. Must be a power of 2 so we can index with `hash & (SIZE-1)`.
/// 2^20 = 1,048,576 entries. Each entry is ~24 bytes → ~24 MB on the heap.
pub const TT_SIZE: usize = 1 << 20;

/// How the stored score should be interpreted relative to alpha/beta.
///
/// Alpha-beta doesn't always produce exact scores:
/// - Exact:  the search completed fully; score is the true minimax value at `depth`.
/// - Lower:  a beta cutoff occurred; true score is >= this (we stopped searching).
/// - Upper:  all moves failed low; true score is <= this.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Bound { Exact, Lower, Upper }

#[derive(Clone, Copy)]
pub struct TtEntry {
    /// Full hash stored alongside score so we can detect collisions on lookup.
    pub hash:  u64,
    pub score: i32,
    pub depth: u8,
    pub bound: Bound,
    /// Best move found at this node; used for move ordering even when depth is too shallow
    /// to trust the score.
    pub mv:    Option<Move>,
}

/// Fixed-size, heap-allocated transposition table using an always-replace policy.
/// Indexed by `hash % TT_SIZE`; collisions simply overwrite the existing entry.
pub struct TranspositionTable {
    entries: Box<[TtEntry]>,
}

impl TranspositionTable {
    pub fn new() -> Self {
        let empty = TtEntry { hash: 0, score: 0, depth: 0, bound: Bound::Exact, mv: None };
        TranspositionTable { entries: vec![empty; TT_SIZE].into_boxed_slice() }
    }

    /// Look up `hash`. Returns `Some(entry)` only when the hash matches (no collision).
    pub fn probe(&self, hash: u64) -> Option<TtEntry> {
        let e = self.entries[hash as usize & (TT_SIZE - 1)];
        if e.hash == hash { Some(e) } else { None }
    }

    /// Store a result. Always overwrites whatever was in the slot.
    pub fn store(&mut self, hash: u64, score: i32, depth: u8, bound: Bound, mv: Option<Move>) {
        self.entries[hash as usize & (TT_SIZE - 1)] = TtEntry { hash, score, depth, bound, mv };
    }

    /// Clear all entries (call on `ucinewgame`).
    pub fn clear(&mut self) {
        let empty = TtEntry { hash: 0, score: 0, depth: 0, bound: Bound::Exact, mv: None };
        self.entries.fill(empty);
    }
}
