#!/usr/bin/env python3
"""
Diagnose a single position: sweep blunderbus across depths and show what it
picks vs Stockfish's best at each depth, with the centipawn cost of each choice.

The depth where blunderbus first agrees with Stockfish tells you whether the
mistake is a search issue (horizon effect) or an evaluation issue (persists
even at high depth).

Usage:
    python diagnose.py --fen "FEN" --played e2e4
    python diagnose.py --fen "FEN" --played e2e4 --max-depth 14 --movetime 500
"""

import argparse
import subprocess
import sys
import time
from pathlib import Path

try:
    import chess
except ImportError:
    print("Missing dependency: pip install chess", file=sys.stderr)
    sys.exit(1)

BLUNDERBUS_BIN = str(Path(__file__).parent / "target" / "release" / "blunderbus")
DEFAULT_STOCKFISH = "/opt/homebrew/bin/stockfish"


class UCIEngine:
    def __init__(self, cmd, movetime_ms=300):
        self.movetime_ms = movetime_ms
        self._p = subprocess.Popen(
            cmd if isinstance(cmd, list) else [cmd],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL, text=True, bufsize=1,
        )
        self._send("uci")
        self._wait_for("uciok")
        self._send("isready")
        self._wait_for("readyok")

    def _send(self, cmd):
        self._p.stdin.write(cmd + "\n")
        self._p.stdin.flush()

    def _wait_for(self, keyword, timeout=10.0):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            line = self._p.stdout.readline().rstrip()
            if keyword in line:
                return
        raise TimeoutError(f"Engine timed out waiting for {keyword!r}")

    def _read_bestmove(self, timeout=60.0):
        deadline = time.monotonic() + timeout
        best, cp = None, None
        while time.monotonic() < deadline:
            line = self._p.stdout.readline().rstrip()
            if "score cp" in line:
                parts = line.split()
                try:
                    cp = int(parts[parts.index("cp") + 1])
                except (ValueError, IndexError):
                    pass
            elif "score mate" in line:
                parts = line.split()
                try:
                    m = int(parts[parts.index("mate") + 1])
                    cp = 10000 if m > 0 else -10000
                except (ValueError, IndexError):
                    pass
            if line.startswith("bestmove"):
                parts = line.split()
                best = parts[1] if len(parts) >= 2 and parts[1] != "(none)" else None
                break
        return best, cp

    def best_move_at_depth(self, fen, depth):
        """Ask blunderbus for its best move at exactly this depth (fresh TT each call)."""
        self._send("ucinewgame")
        self._send(f"position fen {fen}")
        self._send(f"go depth {depth}")
        best, _ = self._read_bestmove()
        return best

    def best_move_timed(self, fen):
        """Ask Stockfish for its best move with movetime budget."""
        self._send(f"position fen {fen}")
        self._send(f"go movetime {self.movetime_ms}")
        best, cp = self._read_bestmove()
        return best, cp

    def eval_after_move(self, fen, move_uci):
        """Return SF's cp eval of playing move_uci, from the moving side's perspective."""
        board = chess.Board(fen)
        board.push(chess.Move.from_uci(move_uci))
        self._send(f"position fen {board.fen()}")
        self._send(f"go movetime {self.movetime_ms}")
        _, cp_opp = self._read_bestmove()
        if cp_opp is None:
            return None
        return -cp_opp  # convert from opponent's POV to moving side's POV

    def quit(self):
        try:
            self._send("quit")
            self._p.wait(timeout=3)
        except Exception:
            self._p.kill()


def fmt_cp(cp):
    if cp is None:
        return "?"
    if abs(cp) >= 9000:
        sign = "+" if cp > 0 else "-"
        return f"{sign}M"
    return f"{cp/100:+.2f}"


def main():
    parser = argparse.ArgumentParser(
        description="Sweep blunderbus across depths on a single position"
    )
    parser.add_argument("--fen", required=True, help="FEN of the position to diagnose")
    parser.add_argument("--played", default=None,
                        help="Move blunderbus played on Lichess (UCI notation, e.g. e2e4)")
    parser.add_argument("--max-depth", type=int, default=12,
                        help="Maximum search depth to test (default 12)")
    parser.add_argument("--movetime", type=int, default=300,
                        help="Stockfish ms per evaluation (default 300)")
    parser.add_argument("--threads", type=int, default=4,
                        help="Stockfish threads (default 4)")
    parser.add_argument("--stockfish", default=DEFAULT_STOCKFISH,
                        help=f"Stockfish binary (default {DEFAULT_STOCKFISH})")
    parser.add_argument("--blunderbus", default=BLUNDERBUS_BIN,
                        help=f"Blunderbus binary (default {BLUNDERBUS_BIN})")
    args = parser.parse_args()

    # Validate FEN
    try:
        board = chess.Board(args.fen)
    except ValueError as e:
        print(f"Invalid FEN: {e}", file=sys.stderr)
        sys.exit(1)

    # Validate played move if given
    if args.played:
        try:
            played_move = chess.Move.from_uci(args.played)
            if played_move not in board.legal_moves:
                print(f"Played move {args.played!r} is not legal in this position", file=sys.stderr)
                sys.exit(1)
        except ValueError as e:
            print(f"Invalid move {args.played!r}: {e}", file=sys.stderr)
            sys.exit(1)

    if not Path(args.blunderbus).exists():
        print(f"Blunderbus binary not found at {args.blunderbus!r} — run: cargo build --release")
        sys.exit(1)

    print(f"Position: {args.fen}")
    print(f"Side to move: {'White' if board.turn == chess.WHITE else 'Black'}")
    if args.played:
        print(f"Played on Lichess: {args.played}")
    print()

    # Start engines
    print(f"Starting Stockfish ({args.movetime}ms/eval, {args.threads} threads)...", flush=True)
    try:
        sf = UCIEngine(args.stockfish, movetime_ms=args.movetime)
        sf._send(f"setoption name Threads value {args.threads}")
    except FileNotFoundError:
        print(f"Stockfish not found at {args.stockfish!r}. Install: brew install stockfish")
        sys.exit(1)

    print("Starting blunderbus...", flush=True)
    try:
        bb = UCIEngine([args.blunderbus, "--uci"])
    except FileNotFoundError:
        print(f"Blunderbus not found at {args.blunderbus!r}")
        sys.exit(1)

    # Stockfish ground truth
    print("Running Stockfish analysis...", flush=True)
    sf_best, sf_best_cp = sf.best_move_timed(args.fen)
    print(f"Stockfish best: {sf_best}  eval {fmt_cp(sf_best_cp)}")

    # Eval of the played move (if given and different from SF best)
    eval_cache = {}
    if sf_best:
        eval_cache[sf_best] = sf_best_cp
    if args.played and args.played != sf_best:
        played_eval = sf.eval_after_move(args.fen, args.played)
        eval_cache[args.played] = played_eval
        cost = (sf_best_cp - played_eval) if (sf_best_cp is not None and played_eval is not None) else None
        print(f"Played move:    {args.played}  eval {fmt_cp(played_eval)}"
              + (f"  (cost: {cost:+}cp vs SF best)" if cost is not None else ""))
    print()

    # Depth sweep
    print(f"{'Depth':>6}  {'BB picks':>8}  {'SF eval':>8}  {'Cost vs SF':>12}  Notes")
    print("-" * 58)

    first_correct_depth = None
    for depth in range(1, args.max_depth + 1):
        print(f"  depth {depth:>2} ...", end="\r", flush=True)
        bb_move = bb.best_move_at_depth(args.fen, depth)
        if bb_move is None:
            print(f"  depth {depth:>2}  (no move returned)")
            continue

        # Eval this move (cached to avoid redundant SF calls)
        if bb_move not in eval_cache:
            eval_cache[bb_move] = sf.eval_after_move(args.fen, bb_move)
        bb_eval = eval_cache[bb_move]

        cost = None
        if sf_best_cp is not None and bb_eval is not None:
            cost = sf_best_cp - bb_eval

        notes = []
        if bb_move == sf_best:
            notes.append("SF best")
            if first_correct_depth is None:
                first_correct_depth = depth
        if args.played and bb_move == args.played:
            notes.append("= Lichess played")

        cost_str = f"{cost:+}cp" if cost is not None else "?"
        print(f"  depth {depth:>2}  {bb_move:>8}  {fmt_cp(bb_eval):>8}  {cost_str:>12}  {', '.join(notes)}")

    bb.quit()
    sf.quit()

    print()
    if first_correct_depth is not None:
        if first_correct_depth <= 4:
            print(f"Diagnosis: corrects at depth {first_correct_depth} — likely a search/horizon issue.")
        elif first_correct_depth <= 8:
            print(f"Diagnosis: corrects at depth {first_correct_depth} — deep horizon effect; "
                  "consider evaluation improvements to make the mistake visible at shallower depth.")
        else:
            print(f"Diagnosis: corrects at depth {first_correct_depth} — likely an evaluation issue; "
                  "check piece-square tables, mobility, or king safety in eval.rs.")
    else:
        print(f"Diagnosis: blunderbus never picks SF's best move up to depth {args.max_depth} — "
              "strong evaluation issue.")


if __name__ == "__main__":
    main()
