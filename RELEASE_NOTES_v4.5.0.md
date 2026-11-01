# AggroChess v4.5.0 ⚔️ "Tal's Eyes, sharper"

AggroChess now solves nine "impossible" positions in 30 seconds on one core, one more than v4.3.1: the Nc6 study, which Stockfish 17.1 misses. It scores 54% against v4.3.1 at 20 s + 0.2 s, and it still sacrifices in practically every game.

## What's new

**A sacrifice in every game, even when it is winning without one**
- When AggroChess is already clearly winning and hasn't sacrificed yet, any sacrifice that keeps it well ahead gets the full bonus. Far ahead, the bonus grows with the lead, so a big advantage no longer makes every give-away look too expensive.
- With a forced mate in hand and no sacrifice yet, it prefers a mate that starts with a real sacrifice, even if that mate is a few moves longer.

**Rescue look for lost endgames** (4 seconds or more per move, or infinite analysis)
- When the position is lost, the moves that give material back (a piece thrown in, an exchange sacrifice) each get a separate search of their own, so the saving sacrifice or fortress is not pruned away.
- Stalemate boxes are recognised as drawn.

**Infinite analysis and several threads** (from 4.3.1, which this release includes)
- `go infinite` first runs a full 30-second timed search (forcing lines, Tal's look and all), then keeps searching. The sacrifice it finds stays on show while nothing better turns up.
- Helper threads start after the forcing lines are primed, so they no longer starve or overwrite them. Plaskett's Puzzle with 2 threads: 4 of 4 (it was 0 of 4).

**Deep combinations stay visible:** at nodes where the search deliberately does not prune, it also no longer trusts hash-table bounds that came from pruned searches.

Fast games are unaffected by the long-think features. The network and the options (`Threads`, `BookPath`) are unchanged.

## The "impossible" puzzles

30 seconds per position, 1 thread:

| Puzzle | AggroChess 4.5 | AggroChess 4.3.1 | Stockfish 17.1 |
|---|---|---|---|
| Plaskett's Puzzle, 1.Nf6+!! | ✅ | ✅ | ❌ |
| Plaskett's Puzzle, the Tal move 3.Bc2+!! | ✅ | ✅ | ❌ |
| Topalov-Shirov 1998, 47...Bh3!! | ✅ | ✅ | ✅ |
| Asauskas-Malisauskas 2004, Qf6+!! | ✅ | ✅ | ✅/❌ (found it in one of two runs) |
| Penrose's Puzzle (fortress) | ✅ | ✅ | ❌ |
| The Weird Check, 1.Rd6+!! | ✅ | ✅ | ❌ |
| Friedel's study, 1.Ba4+!! | ✅ | ✅ | ❌ |
| Study #2, 1.Nc6!! | ✅ | ❌ Nd7 | ❌ Bd2 |
| Queen sacrifice, mate in 17 (1.c8=Q Rxc8 2.Qg6!!) | ✅ | ✅ | ❌ Ne7 |

Still unsolved: Amann's study (1.Qd8!), Salai's study (1.f4!!), Smyslov's study (1.f6!) and study #3 (1.Rh1!!).

## Test results

All games played to the end, 50 balanced openings from both sides, vs v4.3.1:

| | W / D / L | Elo | Games with a sacrifice |
|---|---|---|---|
| 20 s + 0.2 s, release build (100 games) | **52 / 4 / 44 (54.0%)** | **about +28** | 99% |
| 20 s + 0.2 s, release candidate (100 games) | 48 / 5 / 47 (50.5%) | about +3 | 99% |
| 10 s + 0.1 s, release candidate (100 games) | 53 / 5 / 42 (55.5%) | about +38 | 100% |

The long-think features switch on from 1.3 s per move in the 20 s games. Each 100-game result is about ±65 Elo, so 4.5 is at least as strong as 4.3.1, not clearly stronger. The one release-build game without a counted sacrifice was a win where AggroChess gave a rook for a pawn and won the bishop straight back.

## Download
`aggro_chess.exe` (Windows x64). Works in any UCI GUI: Coliseum, Arena, Cute Chess, BanksiaGUI, lichess-bot. The network is built in; no extra files needed.
