use std::sync::OnceLock;

use crate::types::{Color, PieceKind};

pub struct ZobristTable {
    // [piece_kind][color][square_index] — 6 kinds × 2 colors × 64 squares = 768 entries
    pieces: [[[u64; 64]; 2]; 6],
    pub black_to_move: u64,
    pub castling: [u64; 4],   // [white_kingside, white_queenside, black_kingside, black_queenside]
    pub en_passant: [u64; 8], // indexed by file (0 = a-file)
}

static TABLE: OnceLock<ZobristTable> = OnceLock::new();

pub fn tables() -> &'static ZobristTable {
    TABLE.get_or_init(ZobristTable::new)
}

impl ZobristTable {
    fn new() -> ZobristTable {
        // Deterministic xorshift64 PRNG with a fixed seed.
        // Same seed → same table → same hashes on every run.
        fn rand(state: &mut u64) -> u64 {
            *state ^= *state << 13;
            *state ^= *state >> 7;
            *state ^= *state << 17;
            *state
        }

        let mut rng = 0x1234_5678_90ab_cdefu64;

        let mut pieces = [[[0u64; 64]; 2]; 6];
        for kind in 0..6usize {
            for color in 0..2usize {
                for sq in 0..64usize {
                    pieces[kind][color][sq] = rand(&mut rng);
                }
            }
        }

        let black_to_move = rand(&mut rng);

        let mut castling = [0u64; 4];
        for v in &mut castling { *v = rand(&mut rng); }

        let mut en_passant = [0u64; 8];
        for v in &mut en_passant { *v = rand(&mut rng); }

        ZobristTable { pieces, black_to_move, castling, en_passant }
    }

    pub fn piece_key(&self, kind: PieceKind, color: Color, square_index: usize) -> u64 {
        self.pieces[kind as usize][color as usize][square_index]
    }
}
