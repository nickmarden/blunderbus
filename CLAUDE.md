# blunderbus — Chess Engine in Rust

A learning project. The goal is to understand chess engine theory deeply, not to beat Stockfish.

## Project Purpose

Build a working chess engine in Rust, step by step, with full understanding of every decision.
Long-term research question: can LLM-style "next token" prediction (where a token is a move, and
the input sequence is board state) serve as a viable training model for a chess engine? Getting
there requires a working, well-understood engine first.

## Nick's Background

- Experienced programmer in general; professional C++ dev in the STL era (4/10 today)
- Knows Go at a 2/10 level
- Rust: complete beginner (0/10) — explain Rust concepts from scratch, with C++ analogies where helpful
- Comfortable with data structures, algorithms, and systems thinking

---

## Source Code Map

Every file, every public type, every key function. **Update this section whenever anything changes.**

### `src/main.rs`
Entry point. Calls `cli::run(options::CliOptions::from_args())`. Module declarations only.

### `src/types.rs`
Primitive types shared across all modules.
- `Color` (White/Black) with helpers: `opposite`, `pawn_start_rank`, `pawn_promotion_rank`,
  `pawn_direction`, `back_rank`
- `PieceKind` (Pawn/Knight/Bishop/Rook/Queen/King)
- `Piece { color, kind }`
- `Square(u8)` — index 0=a1 to 63=h8, layout `rank * 8 + file`
  - `from_file_rank(file, rank)`, `file()`, `rank()`, `index()`, `to_algebraic()`

### `src/board.rs`
- `Board { squares: [Option<Piece>; 64] }` — mailbox representation
- `Board::empty()`, `get(sq)`, `set(sq, piece)`
- `Board::from_fen_placement(s)` — parses the piece-placement field of a FEN string
- `Display` impl: text grid with rank labels and `a b c d e f g h`

### `src/position.rs`
Full game state. The central struct passed everywhere.
- `CastlingRights { white_kingside, white_queenside, black_kingside, black_queenside }`
  - `CastlingRights::none()`, `CastlingRights::all()`
- `Position { board, side_to_move, castling, en_passant, halfmove_clock, fullmove_number, hash }`
  - `STARTING_FEN`, `STARTING_PLACEMENT_FEN` constants
  - `starting_position()`, `from_fen(s)`, `to_fen()`
  - `make_move(mv) -> Position` — returns new position; immutable (no mutation)
  - `compute_hash() -> u64` — full Zobrist recompute (called at end of `make_move`)
  - `is_in_check(color) -> bool`
  - `is_square_attacked(sq, by_color) -> bool` — attack-ray reversal
- `update_castling_rights(pos, from, to)` — clears rights when king/rook moves or is captured on
  its starting square

### `src/movegen.rs`
- `MoveKind`: `Normal | EnPassant | CastleKingside | CastleQueenside | Promotion(PieceKind)`
- `Move { from: Square, to: Square, kind: MoveKind }` plus `Move::normal(from, to)`
- `generate_legal_moves(pos) -> Vec<Move>` — filters pseudo-legal moves via make_move + is_in_check
- `generate_pseudo_legal_moves(pos) -> Vec<Move>` — all moves following piece rules; may leave king in check
- Per-piece generators: `gen_knight_moves`, `gen_king_moves`, `gen_ray_moves` (rook/bishop/queen),
  `gen_pawn_moves` (single/double push, diagonal capture, en passant, promotion)
- `push_promotions(from, to, moves)` — appends Q/R/B/N promotion variants
- `perft(pos, depth) -> u64` — node count for move generator verification
- `perft_divide(pos, depth)` — perft broken down by first move (for debugging)

### `src/zobrist.rs`
- `ZobristTable { pieces[kind][color][sq], black_to_move, castling[4], en_passant[8] }`
- Deterministic xorshift64 PRNG with fixed seed — same hash table every run
- `zobrist::tables() -> &'static ZobristTable` — singleton via `OnceLock`
- `z.piece_key(kind, color, sq_index) -> u64`

### `src/eval.rs`
Static evaluation from White's perspective (positive = White ahead, negative = Black ahead).
- `pub fn evaluate(pos) -> i32` — material + piece-square table bonuses
- `material_value(kind) -> i32`: Pawn=100, Knight=320, Bishop=330, Rook=500, Queen=900, King=20000
- `piece_square_bonus(kind, color, sq) -> i32` — indexes into per-piece tables from the piece's
  own perspective (White tables flip the rank index for Black)
- Six `[i32; 64]` tables: `PAWN_TABLE`, `KNIGHT_TABLE`, `BISHOP_TABLE`, `ROOK_TABLE`,
  `QUEEN_TABLE`, `KING_TABLE` — written rank 8 (top) to rank 1 (bottom)

### `src/search.rs`
- `SearchResult { best_move: Option<Move>, score: i32, depth: u32, nodes: u64 }`
  - `score` is from the side-to-move's perspective at the root of the search
- `pub fn search(pos, max_depth, game_history, qdepth) -> SearchResult`
  - Iterative deepening from 1 to max_depth; accumulates node counts across depths
  - `game_history`: hashes of positions before the current one; used for repetition detection
- `fn negamax_root(pos, depth, nodes, history, qdepth) -> (i32, Option<Move>)`
- `fn negamax(pos, depth, alpha, beta, ply, nodes, history, qdepth) -> i32`
  - Repetition: score 0 if pos.hash appears >= 2 times in history
  - At depth==0: calls `quiescence`; otherwise: move order, alpha-beta, beta cutoff, 50-move rule
- `fn quiescence(pos, alpha, beta, nodes, qdepth) -> i32`
  - Stand-pat score as lower bound; explores captures/en passant/promotions only
  - Returns alpha (stand-pat) immediately when qdepth == 0
  - Recurses with qdepth - 1 to cap search depth
- `pub fn quiescence_eval(pos, qdepth) -> i32`
  - Standalone quiescence from a position; returns White-perspective score
  - Used for `--eval` display during human turn when no search has been run
- `fn order_moves(pos, moves)` — captures before quiet moves (basic improvement to alpha-beta efficiency)
- `fn eval_from_stm(pos) -> i32` — wraps `evaluate()`, flips sign for Black to move

### `src/pgn.rs`
PGN and SAN output for game export (paste into Lichess analysis, etc.).
- `pub fn move_to_san(pos, mv) -> String` — Standard Algebraic Notation
  - Castling: `O-O` / `O-O-O` (handled first, before piece lookup)
  - Pawns: `e4`, `exd5`, `e8=Q` (file prefix on capture, `=X` on promotion)
  - Pieces: `Nf3`, `Rxe1`, `Qxf7` plus disambiguation and check/checkmate suffixes
  - Disambiguation: scans all legal moves for same piece type to same destination;
    uses source file if unique, rank if unique, both if needed (3+ like pieces)
  - Check/mate suffix: calls `make_move` then `is_in_check` + `generate_legal_moves`
- `pub fn format_pgn(white, black, moves, result) -> String`
  - 7-tag headers + move list (`1. e4 e5 2. Nf3 ...`) wrapped at `$COLUMNS` width (default 80)
  - `result`: `"1-0"` / `"0-1"` / `"1/2-1/2"` / `"*"` (abandoned)
- `fn disambiguate(pos, mv, kind, san)`, `fn check_suffix(pos)`, `fn piece_letter(kind)`,
  `fn push_token(pgn, line, token, width)`

### `src/options.rs`
CLI argument parsing from `std::env::args()`.
- `CliOptions { show_eval, show_hint, depth, qdepth, pretty, auto, human_color, show_fen, show_pgn, no_clear_screen }`
- `CliOptions::from_args() -> CliOptions`

| Flag | Default | Effect |
|------|---------|--------|
| `--depth N` / `-d N` | 4 | Negamax search depth (plies) |
| `--qdepth N` | 6 | Quiescence depth cap (0 = disabled) |
| `--eval` / `-e` | off | Show quiescence-stable eval after each move |
| `--hint` / `-h` | off | Show suggested best move before human's prompt |
| `--pretty` / `-p` | off | Unicode pieces + ANSI colored squares |
| `--auto` / `-a` | off | Engine plays both sides |
| `--black` | off | Human plays Black |
| `--random-color` | off | Randomly assign human color |
| `--fen` / `-f` | off | Print FEN string after every move |
| `--pgn` | off | Print PGN transcript when the game ends |
| `--no-clear-screen` | off | Suppress terminal clear in pretty mode |

### `src/cli.rs`
Interactive game loop and rendering.
- `pub fn run(opts: CliOptions)` — labeled `'game: loop { ... }` that evaluates to the PGN result
  string (`"1-0"`, `"0-1"`, `"1/2-1/2"`, or `"*"`) from every exit path
- `san_moves: Vec<String>` accumulates SAN moves throughout the game for PGN output
- `game_history: Vec<u64>` accumulates position hashes for repetition detection in search
- CLS timing: human turn clears screen at top of loop; engine/auto turn clears after search,
  before announcing the move (so the announcement and new board appear together)
- Eval display: pulled from search result where available; `quiescence_eval` only as fallback for
  human live play with neither `--hint` nor `--eval` active (unreachable in practice)
- `render_position(pos, pretty)` — plain ASCII or colored Unicode board + game state line
- `parse_move(input, legal) -> Result<Move, String>` — coordinate notation (`e2e4`, `e7e8q`)
- `player_names(opts) -> (&str, &str)` — `(white, black)` for PGN headers
- `move_label(mv) -> String` — coordinate notation for display

---

## Feature Status

### Implemented

- [x] Board: 8x8 mailbox (`[Option<Piece>; 64]`)
- [x] FEN: full 6-field parse and serialize
- [x] Move generation: all piece types, promotions, castling, en passant
- [x] Legal move filtering (make_move + is_in_check)
- [x] Attack detection: reverse-ray and reverse-offset lookup
- [x] Game state: castling rights, en passant, halfmove clock, fullmove number
- [x] Zobrist hashing (deterministic, xorshift64)
- [x] Threefold repetition detection (in search and game loop)
- [x] 50-move rule (in search and game loop)
- [x] Static evaluation: material + piece-square tables
- [x] Search: negamax with alpha-beta pruning
- [x] Iterative deepening (1 to max_depth)
- [x] Quiescence search with configurable depth cap (`--qdepth`, default 6)
- [x] Basic move ordering: captures before quiet moves
- [x] Perft testing: verified correct at depth 1-3 (depth 4-5 exist, `#[ignore]` for speed)
- [x] Human CLI: coordinate notation input, quit, hint, eval display
- [x] Pretty rendering: Unicode pieces, ANSI colors, clear-screen between moves
- [x] Auto mode: engine plays both sides
- [x] FEN display after moves (`--fen`)
- [x] Standard Algebraic Notation (SAN) conversion with disambiguation + check/mate suffixes
- [x] PGN output at game end (`--pgn`), line-wrapped at `$COLUMNS`

### Known Bugs

- **Castling through check not detected**: pseudo-legal move generator allows castling even when the
  king's path passes through an attacked square (f1/g1 for kingside, d1/c1 for queenside). Legal
  move filtering only catches the king ending in check. Fix: check transit squares in `gen_king_moves`.

### TODO

- [ ] Fix castling through check (add `is_square_attacked` check for the king's transit squares)
- [ ] Transposition table (hash map Zobrist hash -> score/move; major search speedup)
- [ ] Better move ordering: killer moves, history heuristic
- [ ] UCI protocol (standard interface for Arena, Lichess bot, chess GUIs)
- [ ] Evaluation improvements: king safety, passed pawns, open files, rook on seventh
- [ ] Endgame detection and adjusted king evaluation (active king in endgame)
- [ ] Bitboard representation (major rewrite; discuss architecture before starting)
- [ ] LLM experiment: board state as token sequence, move prediction as next-token generation

---

## How to Work on This Project

### 0. Architecture First

Before writing any code in a new area, lay out the architecture and get agreement.
Explain what the component does, why it's designed that way, and what the alternatives are.
Don't start implementing until Nick says to proceed.

### 1. One Step at a Time, with Tests

Build incrementally. Each piece should be testable in isolation before moving to the next.
Write tests as we go — not as an afterthought. Explain what each test is verifying and why
that matters for correctness.

### 2. Explain Everything

For every non-trivial concept, explain:

- **The chess theory**: why does this matter for a chess engine?
- **The data structure or algorithm**: what is it, how does it work, what are the tradeoffs?
- **The Rust specifics**: why does it look this way in Rust? What ownership/borrowing concepts
  are at play? How does this compare to how you'd do it in C++ or Go?

Don't assume Nick knows Rust idioms. Explain `impl`, traits, lifetimes, `Option`, `Result`,
iterators, pattern matching, etc. when they first appear and when they're used in non-obvious ways.
Use C++ analogies where they help (e.g., traits ≈ abstract base classes / concepts).

Pace explanations to comprehension — check in, adjust depth, don't rush past confusion.

### 3. Interactivity Is Critical

Two interaction modes matter:

- **Human (CLI)**: Nick should be able to play moves, inspect board state, and observe engine
  thinking at any point during development. Build this early and keep it working.
- **UCI protocol**: the standard chess engine protocol for talking to GUIs (Arena, Lichess, etc.).
  Design with UCI in mind from the start, even if full implementation comes later.

---

## Code Style

- Prefer clarity over cleverness, especially early on
- Comments should explain *why*, not *what* — but during this learning phase, add explanatory
  comments generously since the code itself is a teaching artifact
- No premature optimization; note where optimization opportunities exist but don't implement
  them until the simpler version is working and understood

---

## Session Hygiene (Read This at the Start of Every Session)

These rules prevent context bloat and keep sessions productive.

### Before touching code

1. Read this CLAUDE.md fully — it is the project map. Do not re-read source files to rediscover
   what is implemented. If the map description is insufficient for a specific task, read only that
   one file.
2. Run `cargo test 2>&1 | tail -10` to confirm the baseline before making changes.

### While coding

3. Navigate via the Source Code Map above. Only open a file when you need to edit it.
4. Do not read a file back after editing — the Edit tool confirms success or reports the exact error.
5. Grep for a specific symbol rather than reading whole files:
   `grep -n "fn foo" src/*.rs`
6. Make targeted edits rather than full-file rewrites whenever possible.

### When finishing a session

7. Run `cargo test 2>&1` to confirm all tests pass.
8. Update this CLAUDE.md:
   - Check off any newly implemented items in the Feature Status section
   - Update the Source Code Map for new or changed types/functions
   - Add newly discovered bugs to Known Bugs
   - Add newly planned work to TODO
9. Commit with a clear message describing what changed and why.

### Quick reference commands

```bash
cargo build                          # compile
cargo test 2>&1 | tail -10           # quick test summary
cargo test 2>&1                      # full test output
cargo run -- --pretty --depth 4 --eval --hint   # play a game
cargo run -- --pretty --auto --depth 4          # engine vs engine
cargo run -- --pgn --depth 4                    # game + PGN at end
```
