use std::fmt;

use crate::types::{Color, Piece, PieceKind, Square};


#[derive(Clone)]
pub struct Board {
    squares: [Option<Piece>; 64],
}

impl Board {
    pub fn empty() -> Board {
        Board {
            squares: [None; 64],
        }
    }

    pub fn get(&self, sq: Square) -> Option<Piece> {
        self.squares[sq.index() as usize]
    }

    pub fn set(&mut self, sq: Square, piece: Option<Piece>) {
        self.squares[sq.index() as usize] = piece;
    }

    /// Parse the piece-placement field of a FEN string (everything before the first space).
    /// FEN lists ranks from 8 down to 1; digits mean consecutive empty squares.
    pub fn from_fen_placement(placement: &str) -> Result<Board, String> {
        let mut board = Board::empty();
        let mut rank: i8 = 7; // FEN starts at rank 8 (internal index 7)
        let mut file: u8 = 0;

        for ch in placement.chars() {
            match ch {
                '/' => {
                    if file != 8 {
                        return Err(format!("rank {} had {file} files, expected 8", rank + 1));
                    }
                    rank -= 1;
                    file = 0;
                    if rank < 0 {
                        return Err("too many rank separators in FEN".to_string());
                    }
                }
                '1'..='8' => {
                    file += ch as u8 - b'0';
                    if file > 8 {
                        return Err(format!("file overflow on rank {}", rank + 1));
                    }
                }
                _ => {
                    let piece = fen_char_to_piece(ch)
                        .ok_or_else(|| format!("unknown FEN character '{ch}'"))?;
                    board.set(Square::from_file_rank(file, rank as u8), Some(piece));
                    file += 1;
                    if file > 8 {
                        return Err(format!("file overflow on rank {}", rank + 1));
                    }
                }
            }
        }

        Ok(board)
    }
}

fn fen_char_to_piece(ch: char) -> Option<Piece> {
    let (color, kind) = match ch {
        'P' => (Color::White, PieceKind::Pawn),
        'N' => (Color::White, PieceKind::Knight),
        'B' => (Color::White, PieceKind::Bishop),
        'R' => (Color::White, PieceKind::Rook),
        'Q' => (Color::White, PieceKind::Queen),
        'K' => (Color::White, PieceKind::King),
        'p' => (Color::Black, PieceKind::Pawn),
        'n' => (Color::Black, PieceKind::Knight),
        'b' => (Color::Black, PieceKind::Bishop),
        'r' => (Color::Black, PieceKind::Rook),
        'q' => (Color::Black, PieceKind::Queen),
        'k' => (Color::Black, PieceKind::King),
        _ => return None,
    };
    Some(Piece::new(color, kind))
}

fn piece_to_char(piece: Piece) -> char {
    let ch = match piece.kind {
        PieceKind::Pawn   => 'p',
        PieceKind::Knight => 'n',
        PieceKind::Bishop => 'b',
        PieceKind::Rook   => 'r',
        PieceKind::Queen  => 'q',
        PieceKind::King   => 'k',
    };
    if piece.color == Color::White {
        ch.to_ascii_uppercase()
    } else {
        ch
    }
}

impl fmt::Display for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for rank in (0..8u8).rev() {
            write!(f, "{}  ", rank + 1)?;
            for file in 0..8u8 {
                let sq = Square::from_file_rank(file, rank);
                let ch = match self.get(sq) {
                    Some(piece) => piece_to_char(piece),
                    None => '.',
                };
                if file < 7 {
                    write!(f, "{ch} ")?;
                } else {
                    write!(f, "{ch}")?;
                }
            }
            writeln!(f)?;
        }
        write!(f, "   a b c d e f g h")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starting_position_white_king_on_e1() {
        let board = Board::from_fen_placement(crate::position::Position::STARTING_PLACEMENT_FEN).unwrap();
        let e1 = Square::from_file_rank(4, 0);
        assert_eq!(board.get(e1), Some(Piece::new(Color::White, PieceKind::King)));
    }

    #[test]
    fn starting_position_black_king_on_e8() {
        let board = Board::from_fen_placement(crate::position::Position::STARTING_PLACEMENT_FEN).unwrap();
        let e8 = Square::from_file_rank(4, 7);
        assert_eq!(board.get(e8), Some(Piece::new(Color::Black, PieceKind::King)));
    }

    #[test]
    fn starting_position_center_is_empty() {
        let board = Board::from_fen_placement(crate::position::Position::STARTING_PLACEMENT_FEN).unwrap();
        for rank in 2..6 {
            for file in 0..8 {
                let sq = Square::from_file_rank(file, rank);
                assert_eq!(board.get(sq), None, "expected empty square at {}", sq.to_algebraic());
            }
        }
    }

    #[test]
    fn display_contains_rank_labels() {
        let board = Board::from_fen_placement(crate::position::Position::STARTING_PLACEMENT_FEN).unwrap();
        let rendered = format!("{board}");
        for n in 1..=8 {
            assert!(rendered.contains(&n.to_string()));
        }
        assert!(rendered.contains("a b c d e f g h"));
    }

    #[test]
    fn display_starting_position_first_rank() {
        let board = Board::from_fen_placement(crate::position::Position::STARTING_PLACEMENT_FEN).unwrap();
        let rendered = format!("{board}");
        // Rank 1 should contain white pieces: R N B Q K B N R
        assert!(rendered.contains("R N B Q K B N R"));
        // Rank 8 should contain black pieces: r n b q k b n r
        assert!(rendered.contains("r n b q k b n r"));
    }

    #[test]
    fn fen_parse_error_on_bad_char() {
        assert!(Board::from_fen_placement("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNX").is_err());
    }
}
