# blunderbus — Chess Engine in Rust

A learning project. The goal is to understand chess engine theory deeply, not to beat Stockfish.

## Project Purpose

Build a working chess engine in Rust, step by step, with full understanding of every decision.
Long-term research question: can LLM-style "next token" prediction (where the output token is a
move and the input is the current board state) serve as a viable training model for a chess engine?
Building blunderbus is about understanding engine theory deeply before tackling that experiment;
the experiment itself could use Stockfish or any other engine for data generation.

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
- `Color` (White/Black) with `#[repr(usize)]`; helpers: `opposite`, `pawn_direction`, `back_rank`
- `PieceKind` (Pawn/Knight/Bishop/Rook/Queen/King) with `#[repr(usize)]`
- `Piece { color, kind }`
- `Square(u8)` — index 0=a1 to 63=h8, layout `rank * 8 + file`
  - `from_file_rank(file, rank)`, `file()`, `rank()`, `index()`, `to_algebraic()`

### `src/board.rs`
- `Board { squares: [Option<Piece>; 64] }` — mailbox representation; now only used internally by `BitboardSet::from_board` during FEN parsing; not stored in `Position`
- `Board::empty()`, `get(sq)`, `set(sq, piece)`
- `Board::from_fen_placement(s)` — parses the piece-placement field of a FEN string
- `Display` impl: text grid with rank labels and `a b c d e f g h`

### `src/bitboard.rs`
Bitboard infrastructure. All hot-path code now reads from here rather than the mailbox.
- File/rank masks: `FILE_A/B/G/H`, `RANK_1` through `RANK_8`
- `file_mask(file: u8) -> Bitboard` — full-file mask for any file index 0–7
- `front_fill(bb, color) -> Bitboard` — fills northward (White) or southward (Black) across all 7 ranks; used for passed-pawn detection
- `ROOK_RAYS: [fn(Bitboard)->Bitboard; 4]` — N/S/E/W shift functions (shared by movegen + position)
- `BISHOP_RAYS: [fn(Bitboard)->Bitboard; 4]` — NE/NW/SE/SW shift functions
- `Bitboard(pub u64)` newtype with `EMPTY`/`FULL`; `from_square`, `contains`, `is_empty`, `popcount`,
  `lsb` (non-destructive LSB read), `pop_lsb` (remove+return LSB), directional shifts
- Operator overloads: `|`, `&`, `^`, `!` and assign variants
- `knight_attacks() -> &'static [Bitboard; 64]` — precomputed via OnceLock (8 knight targets per square)
- `king_attacks() -> &'static [Bitboard; 64]` — precomputed via OnceLock (≤8 king targets per square)
- `BitboardSet { boards: [[Bitboard; 6]; 2] }` indexed by `[Color as usize][PieceKind as usize]`
  - `from_board(&Board)`, `pieces(color, kind)`, `pieces_mut`, `color_occupancy`, `occupancy`, `piece_at`

### `src/position.rs`
Full game state. The central struct passed everywhere.
- `CastlingRights { white_kingside, white_queenside, black_kingside, black_queenside }`
  - `CastlingRights::none()`, `CastlingRights::all()`
- `Position { bbs, side_to_move, castling, en_passant, halfmove_clock, fullmove_number, hash }`
  - `bbs: BitboardSet` updated incrementally in `make_move`; no mailbox board
  - `STARTING_FEN`, `STARTING_PLACEMENT_FEN` constants
  - `starting_position()`, `from_fen(s)`, `to_fen()`
  - `make_move(mv) -> Position` — returns new position; immutable; updates `bbs` incrementally via `pieces_mut`
  - `compute_hash() -> u64` — Zobrist recompute via bitboard pop_lsb iteration
  - `is_in_check(color) -> bool` — finds king via `bbs.pieces().lsb()`
  - `is_square_attacked(sq, by_color) -> bool` — bitboard attack-table lookup + ray walks
- `update_castling_rights(pos, from, to)` — clears rights when king/rook moves/is captured

### `src/movegen.rs`
- `MoveKind`: `Normal | EnPassant | CastleKingside | CastleQueenside | Promotion(PieceKind)`
- `Move { from: Square, to: Square, kind: MoveKind }` plus `Move::normal(from, to)`
- `generate_legal_moves(pos) -> Vec<Move>` — filters pseudo-legal moves via make_move + is_in_check
- `generate_pseudo_legal_moves(pos) -> Vec<Move>` — iterates `bbs` per piece type; no mailbox reads
- `gen_knight_moves`, `gen_king_moves` — bitboard lookup table + pop_lsb
- `gen_slider_moves(pos, from, color, shifts, moves)` — ray walk using shift function array
- `gen_pawn_moves_bb(pos, color, moves)` — bulk bitboard shift generator for all pawn move types
- `QUEEN_RAYS: [fn(Bitboard)->Bitboard; 8]` — local constant combining ROOK_RAYS + BISHOP_RAYS
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
- `pub fn evaluate(pos) -> i32` — material + piece-square table bonuses + king safety + endgame phase blending + mobility
- `material_value(kind) -> i32`: Pawn=100, Knight=320, Bishop=330, Rook=500, Queen=900, King=20000
- `piece_square_bonus(kind, color, sq, phase) -> i32` — indexes into per-piece tables; King blends KING_MG_TABLE and KING_EG_TABLE by phase
- `pub fn game_phase(pos) -> i32` — 0 (opening) to 256 (endgame); counts phase weight of remaining pieces
- `PHASE_WEIGHTS: [i32; 6]` — per-piece phase weights `[0,1,1,2,4,0]`; `TOTAL_PHASE = 24`
- Seven `[i32; 64]` tables: `PAWN_TABLE`, `KNIGHT_TABLE`, `BISHOP_TABLE`, `ROOK_TABLE`,
  `QUEEN_TABLE`, `KING_MG_TABLE`, `KING_EG_TABLE` — written rank 8 (top) to rank 1 (bottom)
- `king_safety_penalty(pos, color) -> i32` — pawn shield (-20 missing / -10 advanced per file)
  + open/semi-open file near king (-25 / -10 per file); only near back rank
- `passed_pawn_bonus(pos, color) -> i32` — bitboard front-fill detection; rank-scaled bonus table `[0,0,10,20,35,55,80,0]`
- `PASSED_PAWN_BONUS: [i32; 8]` — rank-indexed bonus (rank 0-based from pawn's own perspective)
- `pawn_structure_penalty(pos, color) -> i32` — doubled pawns (-20 per extra) + isolated pawns (-15 each)
- `DOUBLED_PAWN_PENALTY: i32`, `ISOLATED_PAWN_PENALTY: i32`
- `rook_bonus(pos, color) -> i32` — open file (+20), semi-open file (+10), 7th rank (+25); bonuses stack
- `mobility_bonus(pos, color) -> i32` — per-square bonus: Knight=4cp, Bishop=3cp, Rook=2cp, Queen=1cp; uses precomputed knight table + ray walks
- `KNIGHT_MOBILITY_BONUS`, `BISHOP_MOBILITY_BONUS`, `ROOK_MOBILITY_BONUS`, `QUEEN_MOBILITY_BONUS: i32`
- `ROOK_OPEN_FILE_BONUS`, `ROOK_SEMI_OPEN_FILE_BONUS`, `ROOK_SEVENTH_RANK_BONUS: i32`

### `src/search.rs`
- `SearchResult { best_move: Option<Move>, score: i32, depth: u32, nodes: u64, candidates: Vec<(Move, i32)> }`
  - `score` is from the side-to-move's perspective at the root of the search
  - `candidates`: top-N (move, score) pairs sorted best-first; used by `select_move` for strength control
- `pub fn search(pos, max_depth, game_history, qdepth, n, deadline) -> SearchResult`
  - `n`: number of top candidates to collect; `deadline: Option<Instant>` for time-based cutoff
  - Iterative deepening from 1 to max_depth; breaks early if deadline passes
  - `game_history`: hashes of positions before the current one; used for repetition detection
- `fn negamax_root(pos, depth, nodes, history, qdepth) -> (i32, Option<Move>, Vec<(Move,i32)>)`
  - Collects all root-move scores (alpha-beta at root has no cutoffs from -INF window)
- `fn negamax(pos, depth, alpha, beta, ply, nodes, history, qdepth, tt, killers, last_was_null) -> i32`
  - Repetition: score 0 if pos.hash appears >= 2 times in history
  - At depth==0: calls `quiescence`; otherwise: null move pruning, move order, alpha-beta, beta cutoff, 50-move rule
- `fn make_null_move(pos) -> Position` — flip side_to_move, clear en_passant, recompute hash; used by null move pruning
- `fn quiescence(pos, alpha, beta, nodes, qdepth) -> i32`
  - Stand-pat score as lower bound; explores captures/en passant/promotions only
  - Returns alpha (stand-pat) immediately when qdepth == 0
- `pub fn quiescence_eval(pos, qdepth) -> i32` — standalone quiescence, White-perspective score
- `fn order_moves(pos, moves, tt_move, killers)` — TT move first; captures by MVV-LVA; promotions; killer moves; quiet moves last
- `KillerTable = [[Option<Move>; 2]; MAX_PLY]` — two quiet beta-cutoff moves per ply, reset each `search()` call
- `fn eval_from_stm(pos) -> i32` — wraps `evaluate()`, flips sign for Black to move

### `src/pgn.rs`
PGN and SAN output for game export (paste into Lichess analysis, etc.).
- `pub fn move_to_san(pos, mv) -> String` — Standard Algebraic Notation
  - Castling: `O-O` / `O-O-O`; pawns: `e4`, `exd5`, `e8=Q`; pieces: `Nf3`, `Rxe1`
  - Disambiguation + check/checkmate suffixes
- `pub fn format_pgn(white, black, moves, result) -> String`
  - 7-tag headers + move list wrapped at `$COLUMNS` width (default 80)
- `fn disambiguate`, `fn check_suffix`, `fn piece_letter`, `fn push_token`

### `src/options.rs`
CLI argument parsing from `std::env::args()`.
- `CliOptions { show_eval, show_hint, depth, qdepth, pretty, auto, human_color, show_fen, show_pgn, no_clear_screen, candidates, strength, uci }`
- `CliOptions::from_args() -> CliOptions`

| Flag | Default | Effect |
|------|---------|--------|
| `--depth N` / `-d N` | 4 | Negamax search depth (plies) |
| `--qdepth N` | 6 | Quiescence depth cap (0 = disabled) |
| `--candidates N` / `-n N` | 3 | Top-N moves to consider for strength randomization |
| `--strength N` / `-s N` | 100 | 0-100; % chance to pick best move vs random from top-N |
| `--eval` / `-e` | off | Show quiescence-stable eval after each move |
| `--hint` / `-h` | off | Show suggested best move before human's prompt |
| `--pretty` / `-p` | off | Unicode pieces + ANSI colored squares |
| `--auto` / `-a` | off | Engine plays both sides |
| `--black` | off | Human plays Black |
| `--random-color` | off | Randomly assign human color |
| `--fen` / `-f` | off | Print FEN string after every move |
| `--pgn` | off | Print PGN transcript when the game ends |
| `--no-clear-screen` | off | Suppress terminal clear in pretty mode |
| `--uci` | off | Run in UCI protocol mode (stdin/stdout, for GUIs and Lichess bot) |

### `src/cli.rs`
Interactive game loop and rendering.
- `pub fn run(opts: CliOptions)` — game loop; returns PGN result string
- `san_moves: Vec<String>` accumulates SAN moves; `game_history: Vec<u64>` for repetition detection
- `select_move(candidates, best, strength, rng) -> Move` — picks best move or random from top-N based on strength
- `lcg_next(state: &mut u64) -> u64` — Knuth multiplicative LCG for strength randomization
- `render_position(pos, pretty)` — plain ASCII or colored Unicode board
- `pub(crate) fn parse_move(input, legal) -> Result<Move, String>` — coordinate notation
- `pub(crate) fn move_label(mv) -> String` — coordinate notation for display

### `src/uci.rs`
UCI (Universal Chess Interface) protocol loop for GUI integration and Lichess bot API.
- `pub fn run(opts: &CliOptions)` — reads stdin line by line; writes responses to stdout
- Commands handled: `uci`, `isready`, `ucinewgame`, `position` (startpos/fen + moves), `go`, `stop`, `quit`
- `parse_position(tokens) -> Option<(Position, Vec<u64>)>` — parses position + move history
- `GoParams { max_depth: u32, deadline: Option<Instant> }`
- `parse_go(tokens, side, default_depth) -> GoParams`
  - `depth N`: fixed depth; `movetime N`: Instant deadline after N ms
  - `wtime/btime/winc/binc`: time control; budget = `remaining/30 + inc/2` (min 50 ms)

### `lichess_accuracy.py`
Fetches blunderbus's Lichess games and analyzes each move against Stockfish for engine tuning.
- Fetches PGN via `/api/games/user/{username}` (filters: `--since`, `--until`, `--max`)
- Analyzes only blunderbus's moves (detects color from PGN headers)
- Per-move cp_loss = `max(0, eval_before + eval_after_opp)` — Stockfish evals before and after each move
- Phase detection matches `eval.rs` phase weights; cap at 1000cp for ACPL averaging
- Output: per-game ACPL table; ACPL by phase (opening/middlegame/endgame); ACPL by piece type; worst-moves table with FENs
- Run: `.venv/bin/python lichess_accuracy.py [--max N] [--since YYYY-MM-DD] [--movetime N] [--top N]`

### `lichess_bot.py`
Python bot driver that connects blunderbus to the Lichess Bot API.
- Reads `LICHESS_TOKEN` from `.env` or environment; binary path: `target/release/blunderbus`
- Streams `/api/stream/event` for challenges and game-start events
- Accepts standard chess challenges (declines variants), up to `--max-games` concurrent games
- Per game: streams `/api/bot/game/stream/{gameId}`, drives a blunderbus UCI subprocess
- Time control: passes `wtime/btime/winc/binc` through to `go`; falls back to `go depth N`
- `LichessAPI` — thin `requests.Session` wrapper with NDJSON streaming
- `UCI` — subprocess wrapper with thread-safe `best_move()` call
- `GameHandler` — per-game state machine; one thread per active game
- Run: `.venv/bin/python lichess_bot.py [--depth N] [--max-games N]`
- Dependencies: `requirements.txt` (`requests`); venv in `.venv/`

---

## Feature Status

### Implemented

- [x] Board: 8x8 mailbox (`[Option<Piece>; 64]`) + parallel `BitboardSet` (12 bitboards)
- [x] FEN: full 6-field parse and serialize
- [x] Move generation: all piece types, promotions, castling, en passant
- [x] Legal move filtering (make_move + is_in_check)
- [x] Attack detection: bitboard lookup tables + ray walks (knights, kings, sliders, pawns)
- [x] Castling legality: king cannot castle out of, through, or into check
- [x] Game state: castling rights, en passant, halfmove clock, fullmove number
- [x] Zobrist hashing (deterministic, xorshift64)
- [x] Threefold repetition detection (in search and game loop)
- [x] 50-move rule (in search and game loop)
- [x] Static evaluation: material + piece-square tables + king safety + passed pawns (bitboard iteration)
- [x] Transposition table (Zobrist hash → score/move/bound; always-replace, 1M entries ~24 MB)
- [x] Search: negamax with alpha-beta pruning
- [x] Iterative deepening (1 to max_depth) with optional time deadline
- [x] Quiescence search with configurable depth cap (`--qdepth`, default 6)
- [x] Basic move ordering: captures before quiet moves, ordered by MVV-LVA within captures
- [x] Top-N candidate collection at root (`--candidates`, default 3)
- [x] Strength control (`--strength 0-100`): probabilistic best-vs-random-from-top-N selection
- [x] Perft testing: verified correct at depth 1-3 (depth 4-5 exist, `#[ignore]` for speed)
- [x] Human CLI: coordinate notation input, quit, hint, eval display
- [x] Pretty rendering: Unicode pieces, ANSI colors, clear-screen between moves
- [x] Auto mode: engine plays both sides
- [x] FEN display after moves (`--fen`)
- [x] Standard Algebraic Notation (SAN) conversion with disambiguation + check/mate suffixes
- [x] PGN output at game end (`--pgn`), line-wrapped at `$COLUMNS`
- [x] UCI protocol (`--uci`): position, go (depth/movetime/wtime+btime+inc), ucinewgame, quit

### Known Bugs

None currently known.

### TODO

Search improvements (in order):
- [x] MVV-LVA capture ordering (plans/mvv-lva.md)
- [x] Killer move heuristic (plans/killer-moves.md)
- [x] Null move pruning (plans/null-move-pruning.md)
- [x] Late move reductions / LMR (plans/late-move-reductions.md)

Evaluation improvements (in order):
- [x] Endgame phase detection + tapered king piece-square tables (plans/endgame-phase-eval.md)
- [x] Mobility bonus for knights, bishops, rooks, queens (plans/mobility-eval.md)

Long-term:
- [x] Remove mailbox `Board` from `Position` (make_move now updates bbs incrementally)
- [x] Lichess bot deployment via Lichess Bot API + `--uci` mode (`lichess_bot.py`)
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
cargo build --release                # release binary (needed for lichess_bot.py)
cargo test 2>&1 | tail -10           # quick test summary
cargo test 2>&1                      # full test output
cargo run -- --pretty --depth 4 --eval --hint   # play a game
cargo run -- --pretty --auto --depth 4          # engine vs engine
cargo run -- --pgn --depth 4                    # game + PGN at end

# Lichess bot
python3 -m venv .venv && .venv/bin/pip install -r requirements.txt  # first-time setup
.venv/bin/python lichess_bot.py --depth 4       # connect to Lichess
```
