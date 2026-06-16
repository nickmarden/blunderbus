use std::fmt;

use crate::bitboard::{king_attacks, knight_attacks, Bitboard, BitboardSet, BISHOP_RAYS, ROOK_RAYS};
use crate::board::Board;
use crate::movegen::{Move, MoveKind};
use crate::types::{Color, PieceKind, Square};
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
    pub bbs: BitboardSet,
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

        let bbs = BitboardSet::from_board(&Board::from_fen_placement(placement)?);
        let mut pos = Position {
            bbs,
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
        let piece = self.bbs.piece_at(mv.from).expect("make_move: no piece on from square");
        let from_bb = Bitboard::from_square(mv.from);
        let to_bb   = Bitboard::from_square(mv.to);

        // Remove the moving piece from its source square.
        *pos.bbs.pieces_mut(piece.color, piece.kind) &= !from_bb;

        // Remove any captured piece from the destination.
        if let Some(captured) = self.bbs.piece_at(mv.to) {
            *pos.bbs.pieces_mut(captured.color, captured.kind) &= !to_bb;
        }

        match mv.kind {
            MoveKind::Normal => {
                *pos.bbs.pieces_mut(piece.color, piece.kind) |= to_bb;
            }
            MoveKind::Promotion(kind) => {
                *pos.bbs.pieces_mut(piece.color, kind) |= to_bb;
            }
            MoveKind::EnPassant => {
                *pos.bbs.pieces_mut(piece.color, piece.kind) |= to_bb;
                // The captured pawn is on the same file as the destination but one rank back.
                let cap_rank = mv.to.rank() as i8 - piece.color.pawn_direction();
                let cap_sq = Square::from_file_rank(mv.to.file(), cap_rank as u8);
                *pos.bbs.pieces_mut(piece.color.opposite(), PieceKind::Pawn) &= !Bitboard::from_square(cap_sq);
            }
            MoveKind::CastleKingside => {
                *pos.bbs.pieces_mut(piece.color, piece.kind) |= to_bb;
                let rank = piece.color.back_rank();
                *pos.bbs.pieces_mut(piece.color, PieceKind::Rook) &= !Bitboard::from_square(Square::from_file_rank(7, rank));
                *pos.bbs.pieces_mut(piece.color, PieceKind::Rook) |=  Bitboard::from_square(Square::from_file_rank(5, rank));
            }
            MoveKind::CastleQueenside => {
                *pos.bbs.pieces_mut(piece.color, piece.kind) |= to_bb;
                let rank = piece.color.back_rank();
                *pos.bbs.pieces_mut(piece.color, PieceKind::Rook) &= !Bitboard::from_square(Square::from_file_rank(0, rank));
                *pos.bbs.pieces_mut(piece.color, PieceKind::Rook) |=  Bitboard::from_square(Square::from_file_rank(3, rank));
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

        let is_capture = self.bbs.occupancy().contains(mv.to) || mv.kind == MoveKind::EnPassant;
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

        for color in [Color::White, Color::Black] {
            for kind in [PieceKind::Pawn, PieceKind::Knight, PieceKind::Bishop,
                         PieceKind::Rook, PieceKind::Queen, PieceKind::King] {
                let mut bb = self.bbs.pieces(color, kind);
                while !bb.is_empty() {
                    let sq = bb.pop_lsb();
                    hash ^= z.piece_key(kind, color, sq.index() as usize);
                }
            }
        }

        if self.side_to_move == Color::Black { hash ^= z.black_to_move; }
        if self.castling.white_kingside  { hash ^= z.castling[0]; }
        if self.castling.white_queenside { hash ^= z.castling[1]; }
        if self.castling.black_kingside  { hash ^= z.castling[2]; }
        if self.castling.black_queenside { hash ^= z.castling[3]; }
        if let Some(ep) = self.en_passant { hash ^= z.en_passant[ep.file() as usize]; }

        hash
    }

    /// Returns true if the given color's king is in check.
    pub fn is_in_check(&self, color: Color) -> bool {
        let king_bb = self.bbs.pieces(color, PieceKind::King);
        if king_bb.is_empty() { return false; }
        self.is_square_attacked(king_bb.lsb(), color.opposite())
    }

    /// Returns true if `sq` is attacked by any piece of color `by`.
    /// Uses attack-ray reversal via bitboard lookups and shift walks.
    pub fn is_square_attacked(&self, sq: Square, by: Color) -> bool {
        let bbs = &self.bbs;
        let sq_bb = Bitboard::from_square(sq);
        let any_occ = bbs.occupancy();

        // Pawn attacks: reverse-direction check.
        // A white pawn attacks diagonally northward; to check whether sq is attacked by
        // a white pawn, look at the two squares diagonally below sq.
        let pawn_sources = match by {
            Color::White => sq_bb.south_east() | sq_bb.south_west(),
            Color::Black => sq_bb.north_east() | sq_bb.north_west(),
        };
        if !(pawn_sources & bbs.pieces(by, PieceKind::Pawn)).is_empty() { return true; }

        // Knight attacks
        if !(knight_attacks()[sq.index() as usize] & bbs.pieces(by, PieceKind::Knight)).is_empty() {
            return true;
        }

        // King attacks
        if !(king_attacks()[sq.index() as usize] & bbs.pieces(by, PieceKind::King)).is_empty() {
            return true;
        }

        // Rook/Queen (orthogonal rays)
        let rq = bbs.pieces(by, PieceKind::Rook) | bbs.pieces(by, PieceKind::Queen);
        for &ray in &ROOK_RAYS {
            let mut cur = ray(sq_bb);
            while !cur.is_empty() {
                if !(cur & rq).is_empty() { return true; }
                if !(cur & any_occ).is_empty() { break; }
                cur = ray(cur);
            }
        }

        // Bishop/Queen (diagonal rays)
        let bq = bbs.pieces(by, PieceKind::Bishop) | bbs.pieces(by, PieceKind::Queen);
        for &ray in &BISHOP_RAYS {
            let mut cur = ray(sq_bb);
            while !cur.is_empty() {
                if !(cur & bq).is_empty() { return true; }
                if !(cur & any_occ).is_empty() { break; }
                cur = ray(cur);
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
                match self.bbs.piece_at(sq) {
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
        for rank in (0..8u8).rev() {
            write!(f, "{}  ", rank + 1)?;
            for file in 0..8u8 {
                let sq = Square::from_file_rank(file, rank);
                let ch = match self.bbs.piece_at(sq) {
                    None => '.',
                    Some(p) => {
                        let c = match p.kind {
                            PieceKind::Pawn   => 'p',
                            PieceKind::Knight => 'n',
                            PieceKind::Bishop => 'b',
                            PieceKind::Rook   => 'r',
                            PieceKind::Queen  => 'q',
                            PieceKind::King   => 'k',
                        };
                        if p.color == Color::White { c.to_ascii_uppercase() } else { c }
                    }
                };
                if file < 7 { write!(f, "{ch} ")?; } else { write!(f, "{ch}")?; }
            }
            writeln!(f)?;
        }
        write!(f, "   a b c d e f g h")?;
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
