#!/usr/bin/env python3
"""
bench_vs_sf.py -- Play blunderbus head-to-head against Stockfish at a limited ELO.

Alternates colors each game so neither engine has a permanent first-move advantage.
Use the win/loss/draw score to bracket blunderbus's real ELO against known SF levels.

Requirements:
    pip install chess
    Stockfish binary (brew install stockfish)

Usage:
    python3 bench_vs_sf.py
    python3 bench_vs_sf.py --games 20 --depth 4 --sf-elo 1500
    python3 bench_vs_sf.py --help
"""

import argparse
import datetime
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
        description="Play blunderbus vs Stockfish at a target ELO."
    )
    p.add_argument("--games",       type=int,  default=20,
                   help="Number of games (default: 20)")
    p.add_argument("--depth",       type=int,  default=4,
                   help="Blunderbus search depth (default: 4)")
    p.add_argument("--qdepth",      type=int,  default=6,
                   help="Blunderbus quiescence depth (default: 6)")
    p.add_argument("--strength",    type=int,  default=100,
                   help="Blunderbus strength 0-100 (default: 100)")
    p.add_argument("--candidates",  type=int,  default=3,
                   help="Blunderbus top-N candidates (default: 3)")
    p.add_argument("--stockfish",   type=str,  default="stockfish",
                   help="Path to Stockfish binary (default: stockfish)")
    p.add_argument("--sf-elo",      type=int,  default=1500,
                   help="Stockfish target ELO, 1320-3190 (default: 1500)")
    p.add_argument("--sf-movetime", type=float, default=None,
                   help="Stockfish seconds per move (default: scales with --sf-elo)")
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


def play_game(bb, sf, bb_depth, sf_movetime, bb_plays_white, sf_elo, round_num, total_games):
    """
    Play one game. Returns (score, pgn_str).
    score: 1.0=blunderbus win, 0.5=draw, 0.0=loss.
    Caps at 300 half-moves to prevent infinite games.
    """
    today = datetime.date.today().strftime("%Y.%m.%d")
    sf_name = f"stockfish-{sf_elo}"
    game = chess.pgn.Game()
    game.headers["Event"] = "Blunderbus Benchmark"
    game.headers["Site"]  = "Nick's Laptop"
    game.headers["Date"]  = today
    game.headers["Round"] = f"{round_num}/{total_games}"
    game.headers["White"] = "blunderbus" if bb_plays_white else sf_name
    game.headers["Black"] = sf_name if bb_plays_white else "blunderbus"

    board = game.board()
    node = game
    half_moves = 0

    while not board.is_game_over(claim_draw=True) and half_moves < 300:
        is_bb_turn = (board.turn == chess.WHITE) == bb_plays_white
        engine = bb if is_bb_turn else sf
        limit = (chess.engine.Limit(depth=bb_depth) if is_bb_turn
                 else chess.engine.Limit(time=sf_movetime))

        result = engine.play(board, limit)
        if result.move is None:
            break
        node = node.add_variation(result.move)
        board.push(result.move)
        half_moves += 1

    outcome = board.outcome(claim_draw=True)
    if outcome is None:
        score = 0.5
        result_str = "1/2-1/2"
    elif outcome.winner is None:
        score = 0.5
        result_str = "1/2-1/2"
    elif (outcome.winner == chess.WHITE) == bb_plays_white:
        score = 1.0
        result_str = "1-0" if bb_plays_white else "0-1"
    else:
        score = 0.0
        result_str = "0-1" if bb_plays_white else "1-0"

    game.headers["Result"] = result_str
    return score, str(game)


def main():
    args = parse_args()
    if args.sf_movetime is None:
        # Scale with ELO: higher skill needs more search time for calibration to hold.
        # Formula: max(0.5, (elo - 1000) / 500) gives ~1s at 1500, ~2s at 2000, ~3s at 2500.
        args.sf_movetime = max(0.5, (args.sf_elo - 1000) / 500.0)
    build_release()

    bb_cmd = [
        "./target/release/blunderbus", "--uci",
        "--qdepth",     str(args.qdepth),
        "--strength",   str(args.strength),
        "--candidates", str(args.candidates),
    ]

    print(f"Opening Stockfish (target ELO {args.sf_elo})...", flush=True)
    try:
        sf = chess.engine.SimpleEngine.popen_uci(args.stockfish)
    except FileNotFoundError:
        print(f"Stockfish not found at '{args.stockfish}'.")
        print("Install: brew install stockfish  /  sudo apt install stockfish")
        sys.exit(1)
    sf.configure({"UCI_LimitStrength": True, "UCI_Elo": args.sf_elo})
    print("  Done.")

    print("Opening blunderbus...", flush=True)
    try:
        bb = chess.engine.SimpleEngine.popen_uci(bb_cmd)
    except Exception as e:
        print(f"Failed to start blunderbus: {e}")
        sf.quit()
        sys.exit(1)
    print("  Done.\n")

    print(
        f"Playing {args.games} game(s): "
        f"blunderbus depth={args.depth}  vs  Stockfish ELO={args.sf_elo}  "
        f"({args.sf_movetime:.1f}s/move)\n"
    )

    wins = losses = draws = 0
    w_wins = w_losses = w_draws = 0
    b_wins = b_losses = b_draws = 0

    for i in range(args.games):
        bb_plays_white = (i % 2 == 0)
        color_str = "White" if bb_plays_white else "Black"
        print(f"Game {i+1}/{args.games} (blunderbus={color_str}) ...", end="  ", flush=True)

        score, pgn = play_game(bb, sf, args.depth, args.sf_movetime, bb_plays_white, args.sf_elo, i + 1, args.games)

        if score == 1.0:
            result_str = "WIN"
            wins += 1
            if bb_plays_white: w_wins += 1
            else:              b_wins += 1
        elif score == 0.5:
            result_str = "draw"
            draws += 1
            if bb_plays_white: w_draws += 1
            else:              b_draws += 1
        else:
            result_str = "loss"
            losses += 1
            if bb_plays_white: w_losses += 1
            else:              b_losses += 1

        print(result_str)
        print(pgn)
        print()

    bb.quit()
    sf.quit()

    total = wins + losses + draws
    score_pct = (wins + 0.5 * draws) / total * 100 if total else 0
    bb_as_white = (args.games + 1) // 2
    bb_as_black = args.games // 2

    print(f"\n{'='*60}")
    print(f"blunderbus depth={args.depth}  vs  Stockfish ELO {args.sf_elo}")
    print(f"Score: {wins}W / {losses}L / {draws}D  =  {score_pct:.1f}%")
    print()
    print(f"  As White ({bb_as_white} games): {w_wins}W / {w_losses}L / {w_draws}D")
    print(f"  As Black ({bb_as_black} games): {b_wins}W / {b_losses}L / {b_draws}D")
    print()
    if score_pct >= 60:
        print(f"  Likely stronger than SF ELO {args.sf_elo} — try --sf-elo {args.sf_elo + 200}")
    elif score_pct <= 40:
        print(f"  Likely weaker than SF ELO {args.sf_elo} — try --sf-elo {args.sf_elo - 200}")
    else:
        print(f"  Roughly matched with SF ELO {args.sf_elo}")


if __name__ == "__main__":
    main()
