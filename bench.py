#!/usr/bin/env python3
"""
bench.py -- Run blunderbus self-play games and score them with Stockfish.

Requirements:
    pip install chess
    Stockfish binary (brew install stockfish  /  apt install stockfish)

Usage:
    python3 bench.py
    python3 bench.py --games 20 --depth 5 --sf-depth 15
    python3 bench.py --help
"""

import argparse
import io
import math
import statistics
import subprocess
import sys

try:
    import chess
    import chess.engine
    import chess.pgn
except ImportError:
    print("Missing dependency: pip install chess")
    sys.exit(1)


def parse_args():
    p = argparse.ArgumentParser(
        description="Benchmark blunderbus self-play games using Stockfish analysis."
    )
    p.add_argument("--games",      type=int,  default=10,
                   help="Number of self-play games (default: 10)")
    p.add_argument("--depth",      type=int,  default=4,
                   help="Blunderbus search depth (default: 4)")
    p.add_argument("--qdepth",     type=int,  default=6,
                   help="Blunderbus quiescence depth (default: 6)")
    p.add_argument("--strength",   type=int,  default=100,
                   help="Blunderbus strength 0-100 (default: 100)")
    p.add_argument("--candidates", type=int,  default=3,
                   help="Blunderbus top-N candidates (default: 3)")
    p.add_argument("--stockfish",  type=str,  default="stockfish",
                   help="Path to Stockfish binary (default: stockfish)")
    p.add_argument("--sf-depth",   type=int,  default=15,
                   help="Stockfish analysis depth per move (default: 15)")
    return p.parse_args()


def build_release():
    print("Building blunderbus (release)...", flush=True)
    r = subprocess.run(
        ["cargo", "build", "--release"],
        capture_output=True, text=True
    )
    if r.returncode != 0:
        print("Build failed:")
        print(r.stderr)
        sys.exit(1)
    print("  Done.\n")


def run_game(depth, qdepth, strength, candidates):
    """Run one --auto --pgn game. Returns raw PGN string or None on failure."""
    cmd = [
        "./target/release/blunderbus",
        "--auto", "--pgn",
        "--depth",      str(depth),
        "--qdepth",     str(qdepth),
        "--strength",   str(strength),
        "--candidates", str(candidates),
    ]
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=300)
    except subprocess.TimeoutExpired:
        return None

    # The PGN section starts at the first [Event tag.
    idx = result.stdout.find("[Event")
    if idx == -1:
        return None
    return result.stdout[idx:]


def analyze_game(pgn_str, engine, sf_depth):
    """
    Replay a game move by move, asking Stockfish for eval before and after each move.
    Returns (white_cp_losses, black_cp_losses).

    Centipawn loss per move = how much worse the position got for the side that moved.
    Clamped to 0 (we never credit a player for a lucky improvement in Stockfish's view).
    """
    game = chess.pgn.read_game(io.StringIO(pgn_str))
    if game is None:
        return [], []

    board = game.board()
    white_losses = []
    black_losses = []

    limit = chess.engine.Limit(depth=sf_depth)

    for move in game.mainline_moves():
        color = board.turn  # side about to move

        info_before = engine.analyse(board, limit)
        # .pov(color) gives score from the mover's perspective; mate_score caps mates
        score_before = info_before["score"].pov(color).score(mate_score=10000)

        board.push(move)

        info_after = engine.analyse(board, limit)
        # After the push, the same color's perspective (they just moved)
        score_after = info_after["score"].pov(color).score(mate_score=10000)

        loss = max(0, score_before - score_after)

        if color == chess.WHITE:
            white_losses.append(loss)
        else:
            black_losses.append(loss)

    return white_losses, black_losses


def acpl_to_elo(acpl):
    """
    Rough ELO estimate from average centipawn loss.

    Empirical fit anchored to roughly:
      ACPL 10 -> 2200, ACPL 20 -> 1750, ACPL 40 -> 1300, ACPL 80 -> 900
    Treat as a coarse indicator, not a precise rating.
    """
    if acpl <= 0:
        return 3000
    return max(0, int(3500 - 400 * math.sqrt(acpl)))


def print_side_stats(label, game_acpls, all_losses):
    """
    game_acpls: one ACPL value per game (used for median/mean — resistant to blowouts).
    all_losses:  every individual move loss (used for blunder/mistake/inaccuracy counts).
    """
    if not game_acpls:
        print(f"  {label:6s}  (no data)")
        return
    median_acpl  = statistics.median(game_acpls)
    mean_acpl    = statistics.mean(game_acpls)
    blunders     = sum(1 for l in all_losses if l >= 300)
    mistakes     = sum(1 for l in all_losses if 100 <= l < 300)
    inaccuracies = sum(1 for l in all_losses if  50 <= l < 100)
    elo_est      = acpl_to_elo(median_acpl)
    print(
        f"  {label:6s}  ACPL median={median_acpl:5.0f}  mean={mean_acpl:6.1f}  ELO~{elo_est:4d}  "
        f"blunders={blunders:3d}  mistakes={mistakes:3d}  inaccuracies={inaccuracies:3d}  "
        f"({len(game_acpls)} games / {len(all_losses)} moves)"
    )


def main():
    args = parse_args()
    build_release()

    print(f"Opening Stockfish (analysis depth {args.sf_depth})...")
    try:
        engine = chess.engine.SimpleEngine.popen_uci(args.stockfish)
    except FileNotFoundError:
        print(f"Stockfish not found at '{args.stockfish}'.")
        print("Install it with:  brew install stockfish  (macOS)")
        print("                  sudo apt install stockfish  (Debian/Ubuntu)")
        sys.exit(1)
    print("  Done.\n")

    print(
        f"Running {args.games} game(s): "
        f"depth={args.depth}  qdepth={args.qdepth}  "
        f"strength={args.strength}  candidates={args.candidates}\n"
    )

    # Per-game ACPLs: one value per game per side, used for median.
    white_game_acpls = []
    black_game_acpls = []
    # All individual move losses: used for blunder/mistake/inaccuracy counts.
    all_white_losses = []
    all_black_losses = []
    result_counts = {"1-0": 0, "0-1": 0, "1/2-1/2": 0, "other": 0}

    for i in range(args.games):
        print(f"Game {i+1}/{args.games} ...", end="  ", flush=True)

        pgn = run_game(args.depth, args.qdepth, args.strength, args.candidates)
        if pgn is None:
            print("FAILED (no PGN output)")
            continue

        game_obj = chess.pgn.read_game(io.StringIO(pgn))
        result_str = game_obj.headers.get("Result", "*") if game_obj else "*"
        key = result_str if result_str in result_counts else "other"
        result_counts[key] += 1

        print(f"result={result_str}  analyzing...", end="  ", flush=True)

        white_losses, black_losses = analyze_game(pgn, engine, args.sf_depth)
        all_white_losses.extend(white_losses)
        all_black_losses.extend(black_losses)

        w_acpl = statistics.mean(white_losses) if white_losses else 0.0
        b_acpl = statistics.mean(black_losses) if black_losses else 0.0
        if white_losses:
            white_game_acpls.append(w_acpl)
        if black_losses:
            black_game_acpls.append(b_acpl)
        print(f"W-ACPL={w_acpl:.0f}  B-ACPL={b_acpl:.0f}")

    engine.quit()

    total = sum(result_counts.values())
    print(f"\n{'='*70}")
    print(f"Summary: {total} game(s) completed")
    print(
        f"  White wins: {result_counts['1-0']}  "
        f"Black wins: {result_counts['0-1']}  "
        f"Draws: {result_counts['1/2-1/2']}"
    )
    print()
    print("Stockfish analysis (aggregate):")
    # In --auto mode both sides are the same engine, so pool them for the
    # headline number; the per-side breakdown shows the first-move asymmetry.
    engine_game_acpls = white_game_acpls + black_game_acpls
    engine_all_losses = all_white_losses + all_black_losses
    print_side_stats(f"Engine ({total} games, both sides)",
                     engine_game_acpls, engine_all_losses)
    print()
    print("  Per side (first-move asymmetry):")
    print_side_stats("White", white_game_acpls, all_white_losses)
    print_side_stats("Black", black_game_acpls, all_black_losses)
    print()
    print("ELO estimate is a rough approximation from ACPL; use for relative")
    print("comparisons across depths/settings, not as an absolute rating.")


if __name__ == "__main__":
    main()
