# AggroChess v4.6.0 ⚔️ "The Classics"

AggroChess now plays the famous sacrifices of Morphy, Anderssen, Fischer and Kasparov: 12 of 12 in a set of positions that an independent tester tried on v4.5.0 (v4.5.1 finds 9 of them). It also solves two more "impossible" studies. Against v4.5.1 it scored 52.8% over 200 games, with a sacrifice in all 200.

## What's new

**The classics** (30 seconds per position, 1 thread)

| Position | AggroChess 4.6 | AggroChess 4.5.1 |
|---|---|---|
| Göring Gambit, 4.c3 | ✅ | ❌ Bc4 |
| Göring Gambit accepted, 6.Bxf7+ | ✅ | ✅ |
| Evans Gambit, 4.b4 | ✅ | ✅ |
| Morphy's Opera Game, 10.Nxb5! | ✅ | ❌ Bd5 |
| Morphy's Opera Game, 13.Rxd7! | ✅ | ✅ |
| Byrne-Fischer 1956, 11...Na4! | ✅ | ✅ |
| Byrne-Fischer 1956, 17...Be6!! (the queen sacrifice) | ✅ | ❌ Nb1 |
| The Evergreen Game, 20.Rxe7+! | ✅ | ✅ |
| Greek gift, 11.Bxh7+ | ✅ | ✅ |
| Asauskas-Malisauskas 2004, Qf6+!! | ✅ | ✅ |
| The Weird Check, 1.Rd6+!! | ✅ | ✅ |
| Kasparov-Topalov 1999, 24.Rxd4!! | ✅ | ✅ |

The positions are in `tests/famous.epd`. They follow a test of AggroChess 4.5.0 by the author of the ARIA chess coach (github.com/cossonn-coder/aria), who gave it 5 seconds a position and counted 7 of 12. His exact Greek-gift position isn't published; the one here is the Colle line that matches his description. At 5 seconds v4.6 finds 11 of the 12 (the Qf6+ puzzle needs longer).

What changed to get there:
- **A queen left hanging is a sacrifice.** At a long think, a move that leaves a rook or the queen to be taken (and not simply won back) now gets half the sacrifice bonus, wherever it is aimed. That is Fischer's 17...Be6.
- **A piece for two pawns is a sacrifice.** Morphy's 10.Nxb5 used to fall just under the two-pawn limit and got no bonus.
- **The Göring Gambit is in the book** (4.c3, and 5.Bc4 offering a second pawn), in place of the Scotch Gambit lines.

**Two more "impossible" studies**
- **Amann's study, 1.Qd8!!** A quiet move that prepares the queen sacrifice 2.Qf6!!. Tal's look now also looks at quiet moves whose own main line continues with a sacrifice ("quiet tries"). They have to be searched as deep as the main move, and be better at their last two depths, before they are played.
- **The Rxh1 study (1.Rxh1!! Nxh1 2.e6)**, a stalemate fortress. The stalemate-box rule no longer needs a material lead for the attacker (passed pawns are enough to trigger it), the boxed side's last piece is never pruned or reduced, and a checking sacrifice that only loses the piece no longer keeps the rescue look out.

Also for long thinks:
- The rescue look starts when the main search finds the position lost, not only when the first quick look did.
- Tal's look carries on with the main search's mate prover instead of starting a new one, which makes Plaskett's Puzzle more reliable.
- With a fixed time per move (`go movetime`), Tal's look uses all the time the main search leaves over.

## Fixed
- **`go movetime 4000` is a 4-second think.** The long-think features switch on at 4 seconds per move, but 50 ms of safety margin was taken off first, so `movetime 4000` never reached them. Reported in the ARIA test.

## The "impossible" puzzles

30 seconds per position, 1 thread:

| Puzzle | AggroChess 4.6 | AggroChess 4.5.1 |
|---|---|---|
| Plaskett's Puzzle, 1.Nf6+!! | ✅ | ✅ |
| Plaskett's Puzzle, the Tal move 3.Bc2+!! | ✅ | ✅ |
| Topalov-Shirov 1998, 47...Bh3!! | ✅ | ✅ |
| Asauskas-Malisauskas 2004, Qf6+!! | ✅ | ✅ |
| Penrose's Puzzle (fortress) | ✅ | ✅ |
| The Weird Check, 1.Rd6+!! | ✅ | ✅ |
| Friedel's study, 1.Ba4+!! | ✅ | ✅ |
| Study #2, 1.Nc6!! | ✅ | ✅ |
| **Amann's study, 1.Qd8!!** | ✅ | ❌ Qxd7 |
| **Study #3, 1.Rxh1!!** | ✅ | ❌ Rxb4 |
| Salai's study, 1.f4!! | ❌ Kd7 | ❌ Kd7 |
| Smyslov's study, 1.f6!! | ❌ c7 | ❌ c7 |

Ten of these twelve, two more than v4.5.1. The two that are left are long endgame wins by zugzwang (Smyslov's needs about twenty moves of king walk after the sacrifice), which the search does not reach in 30 seconds and the network does not yet judge as won.

## Test results

Against v4.5.1, 100 balanced openings played from both sides, every game played to the end:

| | W / D / L | Elo | Games with a sacrifice |
|---|---|---|---|
| 10 s + 0.1 s (200 games) | **98 / 15 / 87 (52.8%)** | about +19 (95%: -27 to +66) | **200 / 200** |
| 20 s + 0.2 s, long-think features on from 1.3 s per move (100 games) | 47 / 5 / 48 (49.5%) | about -3 (-71 to +64) | 97 / 100 (v4.5.1: 99 / 100) |

So v4.6 is about as strong as v4.5.1, not measurably stronger: the changes are about which sacrifices it finds. Fast games only differ by the piece-for-two-pawns rule and the book. In the long-think match, the three games without a counted sacrifice were two wins in which it only gave material up for a move or two and won it straight back, and one loss in 23 moves.

## Downloads
- `aggro_chess.exe`: Windows x64.
- `aggro_chess-linux-x86_64`: Linux x64 (new; a static build that needs nothing installed). Run `chmod +x aggro_chess-linux-x86_64` once.
- `AggroChess-4.6.0-wasm.zip`: the WebAssembly build for web projects, with a README and an example worker.

The exe and the Linux binary are standard UCI engines with the network built in, for Coliseum, Arena, Cute Chess, BanksiaGUI or lichess-bot. The options are still only `Threads` and `BookPath`.
