# AggroChess v4.6.0 ⚔️ "The Classics"

AggroChess now plays the famous sacrifices of Morphy, Anderssen, Fischer and Kasparov: 12 of 12 in a set of positions that an independent tester tried on v4.5.0 (v4.5.1 finds 9 of them). It also solves three more "impossible" studies, one of them with a small second network. Against v4.5.1 it scored 50.3% over 400 games, with a sacrifice in 398 of them.

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

**Three more "impossible" studies**
- **Amann's study, 1.Qd8!!** A quiet move that prepares the queen sacrifice 2.Qf6!!. Tal's look now also looks at quiet moves whose own main line continues with a sacrifice ("quiet tries"). They have to be searched as deep as the main move, and be better at their last two depths, before they are played.
- **The Rxh1 study (1.Rxh1!! Nxh1 2.e6)**, a stalemate fortress. The stalemate-box rule no longer needs a material lead for the attacker (passed pawns are enough to trigger it), the boxed side's last piece is never pruned or reduced, and a checking sacrifice that only loses the piece no longer keeps the rescue look out.

- **Salai's study, 1.f4!!** A pawn is given away so that the other f-pawn keeps one spare move. Twenty moves later that tempo decides a pawn ending in which Black's bishop is buried behind its own pawns. The main network counts the buried bishop as a bishop and sees no difference between the win and the draw, and quick self-play cannot teach it: games played fast from these positions end the wrong way. So AggroChess now has a **second network for exactly these endings** (opposite-coloured bishops, one of them unable to move). It is the main network trained further on about 14,000 positions around the study, each labelled by a long AggroChess search with null-move verification on. The main network is not changed, so every other position is judged exactly as before. How the positions and labels were made is in `trainer/themes/`; no other engine and no outside data is involved.

Also for long thinks:
- The rescue look starts when the main search finds the position lost, not only when the first quick look did.
- Tal's look carries on with the main search's mate prover instead of starting a new one, which makes Plaskett's Puzzle more reliable.
- With a fixed time per move (`go movetime`), Tal's look uses all the time the main search leaves over.
- The rescue look gets 85% of the thinking time in a lost endgame (it was 70%). With 70% the Nc6 study was only found about one time in four.

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
| **Salai's study, 1.f4!!** | ✅ | ❌ Kd7 |
| Smyslov's study, 1.f6!! | ❌ c7 | ❌ c7 |

Eleven of these twelve, three more than v4.5.1. The one that is left, Smyslov's 1.f6!!, ties a rook up for twenty moves while both sides count pawn moves. AggroChess wins the position once the bind stands on the board, but from the start it does not yet see through Black's best defence (2...c3) in 30 seconds.

## Test results

Against v4.5.1 at 10 s + 0.1 s, 100 balanced openings played from both sides, every game played to the end:

| | W / D / L | Elo | Games with a sacrifice |
|---|---|---|---|
| The build released here (200 games) | 89 / 13 / 98 (47.8%) | about -16 (95%: -63 to +31) | 198 / 200 (v4.5.1: 199 / 200) |
| The build before the second network (200 games) | 98 / 15 / 87 (52.8%) | about +19 (95%: -27 to +66) | 200 / 200 |
| **Both runs (400 games)** | **187 / 28 / 185 (50.3%)** | about +2 | **398 / 400** |

The two builds search ordinary positions identically (same node counts at a fixed depth); they only differ in buried-bishop endings and in how a long think splits its time in a lost endgame. So the two runs are two samples of the same strength: v4.6 is as strong as v4.5.1, not stronger. The changes are about which sacrifices it finds.

The two games without a counted sacrifice: a 122-move draw in which it gave up the exchange (a rook for a minor piece is under the two-pawn limit of the count), and a win by mate in 30 moves.

An earlier build of v4.6 also played 100 games at 20 s + 0.2 s with the long-think features on from 1.3 s per move: 47 / 5 / 48 (49.5%), with a sacrifice in 97 of 100 games (v4.5.1: 99 of 100). The three games without a counted sacrifice were two wins in which it only gave material up for a move or two and won it straight back, and one loss in 23 moves.

## Downloads
- `aggro_chess.exe`: Windows x64. SHA-256: `e39e1c9089a2f8f7dcacecd82c41f21f67ca47a599c245a33dc4a145e09ecadc`
- `aggro_chess-linux-x86_64`: Linux x64 (new; a static build that needs nothing installed). Run `chmod +x aggro_chess-linux-x86_64` once. SHA-256: `dc277411a3700a2db9b27e89cd999d72fb68157c734b2146522857c01cb6a4b6`
- `AggroChess-4.6.0-wasm.zip`: the WebAssembly build for web projects, with a README and an example worker. SHA-256: `abcb2a406ba32867d124efc81958d17e36b68c00eceb33223f461e80d922cc77`

The exe and the Linux binary are standard UCI engines with both networks built in, for Coliseum, Arena, Cute Chess, BanksiaGUI or lichess-bot. The options are still only `Threads` and `BookPath`.
