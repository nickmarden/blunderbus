#!/usr/bin/env python3
"""
Analyze a chess game from PGN on stdin using Stockfish.

Usage:
    python analyze_pgn.py < game.pgn
    curl "https://lichess.org/game/export/GAMEID" | python analyze_pgn.py
    python analyze_pgn.py --movetime 500 < game.pgn

Requires: pip install chess  (or: .venv/bin/pip install chess)
Stockfish: brew install stockfish
"""

import argparse
import subprocess
import sys
import time

import chess
import chess.pgn

STOCKFISH = "/opt/homebrew/bin/stockfish"


def send(p, cmd):
    p.stdin.write(cmd + "\n")
    p.stdin.flush()


def analyze_position(p, board, movetime_ms):
    send(p, f"position fen {board.fen()}")
    send(p, f"go movetime {movetime_ms}")
    best, cp, mate = "", None, None
    deadline = time.monotonic() + movetime_ms / 1000 + 5
    while time.monotonic() < deadline:
        line = p.stdout.readline().rstrip()
        if "score cp" in line:
            parts = line.split()
            try:
                cp = int(parts[parts.index("cp") + 1])
            except (ValueError, IndexError):
                pass
        if "score mate" in line:
            parts = line.split()
            try:
                mate = int(parts[parts.index("mate") + 1])
            except (ValueError, IndexError):
                pass
        if line.startswith("bestmove"):
            parts = line.split()
            best = parts[1] if len(parts) >= 2 else ""
            break
    return best, cp, mate


def format_eval(cp, mate, side):
    """Format eval from White's perspective. side=1 for White to move, -1 for Black."""
    if mate is not None:
        return f"M{mate * side:+d}"
    if cp is not None:
        return f"{cp * side / 100:+.2f}"
    return "?"


def main():
    parser = argparse.ArgumentParser(description="Analyze a PGN game with Stockfish")
    parser.add_argument("--movetime", type=int, default=300,
                        help="Milliseconds per position (default 300)")
    parser.add_argument("--stockfish", default=STOCKFISH,
                        help=f"Path to Stockfish binary (default {STOCKFISH})")
    parser.add_argument("--threads", type=int, default=4,
                        help="Stockfish thread count (default 4)")
    args = parser.parse_args()

    pgn = chess.pgn.read_game(sys.stdin)
    if pgn is None:
        print("No PGN found on stdin", file=sys.stderr)
        sys.exit(1)

    white = pgn.headers.get("White", "?")
    black = pgn.headers.get("Black", "?")
    event = pgn.headers.get("Event", "?")
    result = pgn.headers.get("Result", "*")
    print(f"# {event}: {white} (W) vs {black} (B)  [{result}]")
    print(f"# {args.movetime}ms/position, Stockfish {args.threads} threads")
    print()
    print(f"{'Ply':<5} {'SF eval':>9}  {'Best':>7}  {'Played':>7}  Note")
    print("─" * 60)

    sf = subprocess.Popen(
        [args.stockfish],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL, text=True, bufsize=1,
    )
    send(sf, "uci")
    while True:
        if "uciok" in sf.stdout.readline():
            break
    send(sf, f"setoption name Threads value {args.threads}")
    send(sf, "isready")
    while True:
        if "readyok" in sf.stdout.readline():
            break

    board = pgn.board()
    for node in pgn.mainline():
        move = node.move
        best, cp, mate = analyze_position(sf, board, args.movetime)

        side = 1 if board.turn == chess.WHITE else -1
        ev = format_eval(cp, mate, side)
        label = f"{board.fullmove_number}{'w' if board.turn == chess.WHITE else 'b'}"
        played = move.uci()
        note = f"  <- SF: {best}" if best and best != played and best != "0000" else ""

        print(f"{label:<5} {ev:>9}  {best:>7}  {played:>7}{note}")
        board.push(move)

    send(sf, "quit")
    sf.wait()


if __name__ == "__main__":
    main()
