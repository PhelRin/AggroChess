# AggroChess v4.6.1 ⚔️ "What counts"

A small release about counting honestly: what counts as a sacrifice, and how often each "impossible" study is really solved.

## What counts as a sacrifice

The count used to ask for two pawns of material that stayed lost for two more moves. That left out the exchange sacrifice (a rook for a minor piece is 1.7 pawns) and every sacrifice that wins its material back by force, and it let some plain trades in. The rule is now:

- A move is a sacrifice when, after the opponent's reply, the mover is at least **1.5 pawns** down on the material he had before it. A piece, the exchange and two pawns count; a single pawn does not.
- One case is taken out: the reply was a capture and the mover's next move **takes back on the same square**, ending within 1.5 pawns again. That is a trade.
- Material won back any other way (a fork, a discovered attack, a mating net), however soon, leaves it a sacrifice.

AggroChess uses this count itself: until it has sacrificed in a game its appetite for one keeps growing. Two more things push the same way:

- **Giving material back counts.** When the opponent sacrificed first and AggroChess is ahead in material but not clearly winning, any sound give-back gets the full sacrifice bonus. It used to sit on the material until the game was over.
- **A won game owes its sacrifice from +4 on** (it was +5, and the full hurry only came at +6).

## How often each study is solved

30 seconds, 1 thread, each position run 8 times one after another (5 runs of v4.6.0 and 3 of v4.6.1, which search these positions the same way):

| Puzzle | Solved |
|---|---|
| Plaskett's Puzzle, 1.Nf6+!! | 8 / 8 |
| Plaskett's Puzzle, the Tal move 3.Bc2+!! | 7 / 8 |
| Topalov-Shirov 1998, 47...Bh3!! | 8 / 8 |
| Asauskas-Malisauskas 2004, Qf6+!! | 8 / 8 |
| Penrose's Puzzle (fortress) | 8 / 8 |
| The Weird Check, 1.Rd6+!! | 8 / 8 |
| Friedel's study, 1.Ba4+!! | 8 / 8 |
| Study #2, 1.Nc6!! | 8 / 8 |
| Amann's study, 1.Qd8!! | 8 / 8 |
| Study #3, 1.Rxh1!! | 7 / 8 |
| Salai's study, 1.f4!! | 8 / 8 |
| Smyslov's study, 1.f6!! | 0 / 8 |

The twelve classics (`tests/famous.epd`) were solved in every one of 4 runs. Two studies are not yet repeatable: the Tal move and 1.Rxh1 each missed once. Both are found by a look that finishes about a second before the time is up.

`aggro_chess bench` prints a node count that is the same for every correct build of a version, on any machine: **170599 nodes** for v4.6.0 and v4.6.1. A quick way to check a binary.

## Test results

Against v4.5.1 at 10 s + 0.1 s, 100 balanced openings played from both sides, every game played to the end, counted with the new rule:

| | W / D / L | Games with a sacrifice |
|---|---|---|
| v4.6.1 (200 games) | 89 / 19 / 92 (49.3%) | 198 / 200 |
| v4.6.0, the 400 games of its release, counted again | 187 / 28 / 185 (50.3%) | 397 / 400 |

So the count is where it was, at about 99%, and the strength is unchanged. The games without a sacrifice are now all of one kind: games that are over quickly. In the v4.6.1 run they were a loss in 21 moves and a win in 27. Getting those would mean sacrificing by force in the first twenty moves, which costs more than it gives.

## On the website

A puzzles page: pick a study and watch the engine think about it in the browser. The browser build is slower than the desktop engine, so each study has its own think time (30 to 90 seconds); with those it finds 11 of the 12.

## Downloads
- `aggro_chess.exe`: Windows x64. SHA-256: `ab4da104968848b38e52fa475810e02b276835aa7706eee7f8408dc2c0dc2653`
- `aggro_chess-linux-x86_64`: Linux x64 (a static build that needs nothing installed). Run `chmod +x aggro_chess-linux-x86_64` once. SHA-256: `508be152053d08129458bb98a1cbb0303cde227201d44995f2401bcc16320493`
- `AggroChess-4.6.1-wasm.zip`: the WebAssembly build for web projects, with a README and an example worker. SHA-256: `9c5fa3582a654dad466c493aff08abbb1fa946d8ab7e1c7a893d15c399949b30`

The options are still only `Threads` and `BookPath`.
