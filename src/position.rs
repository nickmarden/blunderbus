use std::fmt;

use crate::board::Board;
use crate::movegen::{Move, MoveKind};
use crate::types::{Color, Piece, PieceKind, Square};
use crate::zobrist;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CastlingRights {
    pub white_kingside: bool,
    pub white_queenside: bool,
    pub black_kingside: bool,
    pub black_queenside: bool,
}

impl CastlingRights {
    pub fn none() -> CastlingRights {
        CastlingRights {
            white_kingside: false,
            white_queenside: false,
            black_kingside: false,
            black_queenside: false,
        }
    }

    #[allow(dead_code)]
    pub fn all() -> CastlingRights {
        CastlingRights {
            white_kingside: true,
            white_queenside: true,
            black_kingside: true,
            black_queenside: true,
        }
    }
}

impl fmt::Display for CastlingRights {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.white_kingside && !self.white_queenside && !self.black_kingside && !self.black_queenside {
            return write!(f, "-");
        }
        if self.white_kingside  { write!(f, "K")?; }
        if self.white_queenside { write!(f, "Q")?; }
        if self.black_kingside  { write!(f, "k")?; }
        if self.black_queenside { write!(f, "q")?; }
        Ok(())
    }
}

#[derive(Clone)]
pub struct Position {
    pub board: Board,
    pub side_to_move: Color,
    pub castling: CastlingRights,
    pub en_passant: Option<Square>,
    pub halfmove_clock: u32,
    pub fullmove_number: u32,
    pub hash: u64,
}

impl Position {
    // The placement-only prefix of STARTING_FEN, for use in board-level tests.
    #[allow(dead_code)]
    pub const STARTING_PLACEMENT_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";
    pub const STARTING_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    pub fn starting_position() -> Position {
        Position::from_fen(Self::STARTING_FEN)
            .expect("built-in starting position FEN is valid")
    }

    pub fn from_fen(fen: &str) -> Result<Position, String> {
        let mut fields = fen.split_whitespace();

        let placement = fields.next().ok_or("FEN missing piece placement")?;
        let board = Board::from_fen_placement(placement)?;

        let active = fields.next().ok_or("FEN missing active color")?;
        let side_to_move = match active {
            "w" => Color::White,
            "b" => Color::Black,
            _ => return Err(format!("unknown active color '{active}'")),
        };

        let castling_str = fields.next().ok_or("FEN missing castling rights")?;
        let castling = parse_castling(castling_str)?;

        let ep_str = fields.next().ok_or("FEN missing en passant field")?;
        let en_passant = parse_en_passant(ep_str)?;

        let halfmove_clock = fields
            .next()
            .ok_or("FEN missing halfmove clock")?
            .parse::<u32>()
            .map_err(|e| format!("invalid halfmove clock: {e}"))?;

        let fullmove_number = fields
            .next()
            .ok_or("FEN missing fullmove number")?
            .parse::<u32>()
            .map_err(|e| format!("invalid fullmove number: {e}"))?;

        let mut pos = Position {
            board,
            side_to_move,
            castling,
            en_passant,
            halfmove_clock,
            fullmove_number,
            hash: 0,
        };
        pos.hash = pos.compute_hash();
        Ok(pos)
    }

    /// Apply a move and return the resulting position. Does not verify legality.
    pub fn make_move(&self, mv: &Move) -> Position {
        let mut pos = self.clone();
        let piece = self.board.get(mv.from).expect("make_move: no piece on from square");

        pos.board.set(mv.from, None);

        match mv.kind {
            MoveKind::Normal => {
                pos.board.set(mv.to, Some(piece));
            }
            MoveKind::Promotion(kind) => {
                pos.board.set(mv.to, Some(Piece::new(piece.color, kind)));
            }
            MoveKind::EnPassant => {
                pos.board.set(mv.to, Some(piece));
                // The captured pawn is on the same file as the destination but one rank back.
                let cap_rank = mv.to.rank() as i8 - piece.color.pawn_direction();
                pos.board.set(Square::from_file_rank(mv.to.file(), cap_rank as u8), None);
            }
            MoveKind::CastleKingside => {
                pos.board.set(mv.to, Some(piece));
                let rank = piece.color.back_rank();
                let rook = pos.board.get(Square::from_file_rank(7, rank))
                    .expect("kingside rook must exist for castling");
                pos.board.set(Square::from_file_rank(7, rank), None);
                pos.board.set(Square::from_file_rank(5, rank), Some(rook));
            }
            MoveKind::CastleQueenside => {
                pos.board.set(mv.to, Some(piece));
                let rank = piece.color.back_rank();
                let rook = pos.board.get(Square::from_file_rank(0, rank))
                    .expect("queenside rook must exist for castling");
                pos.board.set(Square::from_file_rank(0, rank), None);
                pos.board.set(Square::from_file_rank(3, rank), Some(rook));
            }
        }

        // En passant target: set only on a double pawn push.
        let rank_diff = mv.to.rank() as i8 - mv.from.rank() as i8;
        pos.en_passant = if mv.kind == MoveKind::Normal
            && piece.kind == PieceKind::Pawn
            && rank_diff.abs() == 2
        {
            let ep_rank = (mv.from.rank() + mv.to.rank()) / 2;
            Some(Square::from_file_rank(mv.from.file(), ep_rank))
        } else {
            None
        };

        update_castling_rights(&mut pos, mv.from, mv.to);

        let is_capture = self.board.get(mv.to).is_some() || mv.kind == MoveKind::EnPassant;
        if is_capture || piece.kind == PieceKind::Pawn {
            pos.halfmove_clock = 0;
        } else {
            pos.halfmove_clock += 1;
        }

        if self.side_to_move == Color::Black {
            pos.fullmove_number += 1;
        }

        pos.side_to_move = self.side_to_move.opposite();
        pos.hash = pos.compute_hash();
        pos
    }

    pub fn compute_hash(&self) -> u64 {
        let z = zobrist::tables();
        let mut hash = 0u64;

        for index in 0..64u8 {
            if let Some(piece) = self.board.get(Square::new(index)) {
                hash ^= z.piece_key(piece.kind, piece.color, index as usize);
            }
        }

        if self.side_to_move == Color::Black {
            hash ^= z.black_to_move;
        }

        if self.castling.white_kingside  { hash ^= z.castling[0]; }
        if self.castling.white_queenside { hash ^= z.castling[1]; }
        if self.castling.black_kingside  { hash ^= z.castling[2]; }
        if self.castling.black_queenside { hash ^= z.castling[3]; }

        if let Some(ep) = self.en_passant {
            hash ^= z.en_passant[ep.file() as usize];
        }

        hash
    }

    /// Returns true if the given color's king is in check.
    pub fn is_in_check(&self, color: Color) -> bool {
        let king_sq = (0..64u8)
            .map(Square::new)
            .find(|&sq| self.board.get(sq) == Some(Piece::new(color, PieceKind::King)));
        match king_sq {
            Some(sq) => self.is_square_attacked(sq, color.opposite()),
            None => false,
        }
    }

    /// Returns true if `sq` is attacked by any piece of color `by`.
    /// Uses attack-ray reversal: looks outward from the square in each attack pattern.
    pub fn is_square_attacked(&self, sq: Square, by: Color) -> bool {
        // Knight attacks
        for (df, dr) in KNIGHT_OFFSETS {
            if let Some(from) = offset_sq(sq, df, dr) {
                if self.board.get(from) == Some(Piece::new(by, PieceKind::Knight)) {
                    return true;
                }
            }
        }

        // Rook / Queen attacks along orthogonal rays
        for (df, dr) in ROOK_DIRS {
            if ray_contains_attacker(&self.board, sq, df, dr, by, |k| {
                k == PieceKind::Rook || k == PieceKind::Queen
            }) {
                return true;
            }
        }

        // Bishop / Queen attacks along diagonal rays
        for (df, dr) in BISHOP_DIRS {
            if ray_contains_attacker(&self.board, sq, df, dr, by, |k| {
                k == PieceKind::Bishop || k == PieceKind::Queen
            }) {
                return true;
            }
        }

        // Pawn attacks: look one rank in the direction enemy pawns come FROM.
        // A white pawn on (f, r) attacks (f±1, r+1), so to be attacked by a white pawn
        // we look at (sq.file±1, sq.rank - 1) — one rank below sq.
        let pawn_rank_offset = -by.pawn_direction();
        for df in [-1i8, 1i8] {
            if let Some(from) = offset_sq(sq, df, pawn_rank_offset) {
                if self.board.get(from) == Some(Piece::new(by, PieceKind::Pawn)) {
                    return true;
                }
            }
        }

        // King attacks
        for (df, dr) in KING_OFFSETS {
            if let Some(from) = offset_sq(sq, df, dr) {
                if self.board.get(from) == Some(Piece::new(by, PieceKind::King)) {
                    return true;
                }
            }
        }

        false
    }

    /// Serialize this position to a FEN string.
    pub fn to_fen(&self) -> String {
        let mut fen = String::new();

        // Piece placement: rank 8 down to rank 1, a-file to h-file.
        for rank in (0..8u8).rev() {
            let mut empty = 0u8;
            for file in 0..8u8 {
                let sq = Square::from_file_rank(file, rank);
                match self.board.get(sq) {
                    None => empty += 1,
                    Some(piece) => {
                        if empty > 0 {
                            fen.push((b'0' + empty) as char);
                            empty = 0;
                        }
                        let ch = match piece.kind {
                            PieceKind::Pawn   => 'p',
                            PieceKind::Knight => 'n',
                            PieceKind::Bishop => 'b',
                            PieceKind::Rook   => 'r',
                            PieceKind::Queen  => 'q',
                            PieceKind::King   => 'k',
                        };
                        fen.push(if piece.color == Color::White { ch.to_ascii_uppercase() } else { ch });
                    }
                }
            }
            if empty > 0 { fen.push((b'0' + empty) as char); }
            if rank > 0  { fen.push('/'); }
        }

        fen.push(' ');
        fen.push(if self.side_to_move == Color::White { 'w' } else { 'b' });
        fen.push(' ');
        fen.push_str(&self.castling.to_string());
        fen.push(' ');
        match self.en_passant {
            Some(sq) => fen.push_str(&sq.to_algebraic()),
            None     => fen.push('-'),
        }
        fen.push(' ');
        fen.push_str(&self.halfmove_clock.to_string());
        fen.push(' ');
        fen.push_str(&self.fullmove_number.to_string());

        fen
    }
}

// --- Geometry constants (duplicated from movegen to avoid coupling) ---

const KNIGHT_OFFSETS: [(i8, i8); 8] = [
    (-2, -1), (-2, 1), (-1, -2), (-1, 2),
    ( 1, -2), ( 1, 2), ( 2, -1), ( 2, 1),
];

const KING_OFFSETS: [(i8, i8); 8] = [
    (-1, -1), (-1, 0), (-1, 1),
    ( 0, -1),          ( 0, 1),
    ( 1, -1), ( 1, 0), ( 1, 1),
];

const ROOK_DIRS:   [(i8, i8); 4] = [(0, 1), (0, -1), (1, 0), (-1, 0)];
const BISHOP_DIRS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

// --- Helper functions ---

fn offset_sq(sq: Square, df: i8, dr: i8) -> Option<Square> {
    let f = sq.file() as i8 + df;
    let r = sq.rank() as i8 + dr;
    if (0..8).contains(&f) && (0..8).contains(&r) {
        Some(Square::from_file_rank(f as u8, r as u8))
    } else {
        None
    }
}

/// Walk a ray from `from` in direction (df, dr); return true if the first piece
/// encountered belongs to `by` and satisfies `is_match`.
///
/// `impl Fn(PieceKind) -> bool` is Rust's way of accepting a closure as a parameter.
/// The compiler monomorphizes this (like a C++ template), so there's no runtime overhead.
fn ray_contains_attacker(
    board: &Board,
    from: Square,
    df: i8, dr: i8,
    by: Color,
    is_match: impl Fn(PieceKind) -> bool,
) -> bool {
    let (mut f, mut r) = (from.file() as i8 + df, from.rank() as i8 + dr);
    while (0..8).contains(&f) && (0..8).contains(&r) {
        let sq = Square::from_file_rank(f as u8, r as u8);
        if let Some(piece) = board.get(sq) {
            return piece.color == by && is_match(piece.kind);
        }
        f += df;
        r += dr;
    }
    false
}

fn update_castling_rights(pos: &mut Position, from: Square, to: Square) {
    // King moves from starting square
    if from == Square::from_file_rank(4, 0) {
        pos.castling.white_kingside = false;
        pos.castling.white_queenside = false;
    }
    if from == Square::from_file_rank(4, 7) {
        pos.castling.black_kingside = false;
        pos.castling.black_queenside = false;
    }
    // Rook moves from, or any piece captures on, rook starting squares
    if from == Square::from_file_rank(7, 0) || to == Square::from_file_rank(7, 0) {
        pos.castling.white_kingside = false;
    }
    if from == Square::from_file_rank(0, 0) || to == Square::from_file_rank(0, 0) {
        pos.castling.white_queenside = false;
    }
    if from == Square::from_file_rank(7, 7) || to == Square::from_file_rank(7, 7) {
        pos.castling.black_kingside = false;
    }
    if from == Square::from_file_rank(0, 7) || to == Square::from_file_rank(0, 7) {
        pos.castling.black_queenside = false;
    }
}

fn parse_castling(s: &str) -> Result<CastlingRights, String> {
    if s == "-" {
        return Ok(CastlingRights::none());
    }
    let mut rights = CastlingRights::none();
    for ch in s.chars() {
        match ch {
            'K' => rights.white_kingside = true,
            'Q' => rights.white_queenside = true,
            'k' => rights.black_kingside = true,
            'q' => rights.black_queenside = true,
            _ => return Err(format!("unknown castling character '{ch}'")),
        }
    }
    Ok(rights)
}

fn parse_en_passant(s: &str) -> Result<Option<Square>, String> {
    if s == "-" {
        return Ok(None);
    }
    let mut chars = s.chars();
    let file_ch = chars.next().ok_or("en passant square too short")?;
    let rank_ch = chars.next().ok_or("en passant square too short")?;

    let file = file_ch as u8;
    let rank = rank_ch as u8;

    if !(b'a'..=b'h').contains(&file) {
        return Err(format!("invalid en passant file '{file_ch}'"));
    }
    if !(b'1'..=b'8').contains(&rank) {
        return Err(format!("invalid en passant rank '{rank_ch}'"));
    }

    Ok(Some(Square::from_file_rank(file - b'a', rank - b'1')))
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.board)?;
        let side = match self.side_to_move {
            Color::White => "White",
            Color::Black => "Black",
        };
        let ep = match self.en_passant {
            Some(sq) => sq.to_algebraic(),
            None => "-".to_string(),
        };
        write!(f, "\n{side} to move | Castling: {} | En passant: {ep} | Halfmove: {} | Move: {}",
            self.castling, self.halfmove_clock, self.fullmove_number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starting_position_parses() {
        let pos = Position::starting_position();
        assert_eq!(pos.side_to_move, Color::White);
        assert_eq!(pos.castling, CastlingRights::all());
        assert_eq!(pos.en_passant, None);
        assert_eq!(pos.halfmove_clock, 0);
        assert_eq!(pos.fullmove_number, 1);
    }

    #[test]
    fn fen_with_en_passant() {
        let pos = Position::from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1")
            .unwrap();
        assert_eq!(pos.side_to_move, Color::Black);
        assert_eq!(pos.en_passant, Some(Square::from_file_rank(4, 2)));
    }

    #[test]
    fn fen_with_no_castling() {
        let pos = Position::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1")
            .unwrap();
        assert_eq!(pos.castling, CastlingRights::none());
    }

    #[test]
    fn display_shows_side_to_move() {
        let pos = Position::starting_position();
        let rendered = format!("{pos}");
        assert!(rendered.contains("White to move"));
    }

    #[test]
    fn fen_error_on_bad_active_color() {
        assert!(Position::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR x KQkq - 0 1").is_err());
    }

    #[test]
    fn make_move_e2e4_sets_en_passant() {
        let pos = Position::starting_position();
        let e2 = Square::from_file_rank(4, 1);
        let e4 = Square::from_file_rank(4, 3);
        let mv = Move::normal(e2, e4);
        let after = pos.make_move(&mv);
        assert_eq!(after.en_passant, Some(Square::from_file_rank(4, 2)));
        assert_eq!(after.side_to_move, Color::Black);
        assert_eq!(after.halfmove_clock, 0); // pawn move resets clock
    }

    #[test]
    fn make_move_clears_en_passant_after_non_pawn_move() {
        // Position after 1.e4 has en passant on e3
        let pos = Position::from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1")
            .unwrap();
        let g8 = Square::from_file_rank(6, 7);
        let f6 = Square::from_file_rank(5, 5);
        let after = pos.make_move(&Move::normal(g8, f6));
        assert_eq!(after.en_passant, None);
    }

    #[test]
    fn make_move_king_move_loses_castling_rights() {
        let pos = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w KQ - 0 1").unwrap();
        let after = pos.make_move(&Move::normal(
            Square::from_file_rank(4, 0),
            Square::from_file_rank(4, 1),
        ));
        assert!(!after.castling.white_kingside);
        assert!(!after.castling.white_queenside);
    }

    #[test]
    fn starting_position_not_in_check() {
        let pos = Position::starting_position();
        assert!(!pos.is_in_check(Color::White));
        assert!(!pos.is_in_check(Color::Black));
    }

    #[test]
    fn scholar_mate_is_checkmate_position() {
        // Queen on f7 gives check — known checkmate position
        let pos = Position::from_fen("r1bqkb1r/pppp1Qpp/2n2n2/4p3/2B1P3/8/PPPP1PPP/RNB1K1NR b KQkq - 0 4")
            .unwrap();
        assert!(pos.is_in_check(Color::Black));
    }
}
