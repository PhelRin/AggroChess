# AggroChess v3.0.0 ⚔️

**AggroChess** is an aggressive, sacrificial chess engine written in Rust, in the spirit of Mikhail Tal. It is built to be *fun to play against*: it castles into attacks, throws pawns at your king, keeps pieces on the board, avoids draws, and will happily give up material to rip your king open. The goal is not to beat Stockfish. The goal is to play the kind of chess people enjoy losing to (and occasionally beating).

AggroChess is UCI-compatible, so it works in standard chess GUIs (Arena, Cute Chess, BanksiaGUI, lichess-bot, …).

Play against it easily here: https://playaggro.netlify.app/

## What changed in v3.0.0

v2 was aggressive but it was bad at it. Its evaluation was ~40 hand-written "Tal" bonuses stacked on top of each other, and they double-counted. King-attack bonuses alone could add up to 15–20 pawns, so the engine kept "seeing" crushing attacks that weren't there and sacrificing into nothing. The search also had a bug that threw away every quiet move that hung material, at every depth. That made real quiet sacrifices (Nf5!, Bh6!, Rh3) impossible to find.

v3 rebuilds the evaluation and search on proven foundations and moves the personality into one deliberate, tunable layer.

### Evaluation (`src/eval.rs`)
* **Sound core:** PeSTO tapered piece-square tables, pawn structure (passed/doubled/isolated/connected, cached), mobility, outposts, rooks on open files, bishop pair, threats (pawn attacks on pieces, hanging pieces), passed-pawn races, endgame scaling (opposite bishops, insufficient material) and mop-up for mating with a bare king.
* **Real king safety:** a Stockfish-style king-danger model. It counts the attackers and their weight, attacks next to the king, weak squares in the king zone, *safe* checks available to each piece type, the pawn shelter and pawn storms. Danger grows *quadratically*. One piece near the king means little. Three pieces with safe checks against a broken shelter means a lot. This is what lets the engine tell a real attack from a fake one.
* **Personality layer (the "Tal" part):**
  * Aggression (fixed at 180%) multiplies the danger *the engine* creates against your king. It doesn't touch how it judges its own king.
  * Sacrifice appetite (fixed at 150): when choosing its move, a sacrifice aimed at your king (a check, a piece landing next to your king, or a piece brought to bear on it) is played if the engine rates it within 1.5 pawns of its best move, as long as the line isn't clearly bad for it. Every sacrifice is still searched fully, so it never plays one it sees losing outright.
  * It prefers keeping pieces and queens on the board (each trade costs it a little).
  * Contempt (fixed at 25) makes it treat a draw as a small loss, so it plays on.

Because the attack has to be *real* to score well, sacrifices now come from the evaluation doing its job. A piece sac pays off when it opens the king and three or four pieces are ready to pile in.

### Search (`src/search.rs`)
* Iterative deepening, aspiration windows, principal variation search, a lock-free shared hash table and Lazy SMP multi-threading.
* Null-move pruning, reverse futility pruning, razoring, late move reductions and pruning, futility pruning and singular extensions, all with standard modern safeguards.
* **Sacrifice-friendly pruning:** static-exchange pruning is depth-limited. Quiet moves that bring a piece into the enemy king zone get a very lenient threshold and smaller reductions, so sacrifices get searched deeply enough to be proven.
* Check extensions, quiet checks in quiescence search, killer/counter-move/history/continuation-history move ordering.
* Proper stalemate and repetition handling, mate-distance pruning, contempt-aware draw scores.
* Time management with soft and hard limits that adapts to how stable the best move is.

## Tuned for playing humans ("Tal mode")

AggroChess is designed to be played against by people, so its personality is fixed at the "Tal mode" settings: Aggression 180% and Sacrifice Appetite 150. In 40-game test matches against a full-strength copy of itself, Tal mode made about 2.4× as many sacrifices (a sacrifice in 35 of 40 games, roughly 2 per game). The cost was about 320 Elo against that full-strength engine, but it is still far stronger than most human players. When it plays a sacrifice, the score it reports includes the sacrifice bonus, so it can look a bit more optimistic than it is for that move.

## How it compares to v2

Measured with the Tal mode build, books off:
* **40-game match vs v2** (2 s + 0.02 s per game): v3 won 39, lost 1, drew 0.
* **Tactics test** (11 classic positions, 2 s each): v3 found 10/11, v2 found 6/11. That includes the Greek Gift (Bxh7+).

v3 hasn't been formally rated against other engines yet.

## UCI options

Only two settings are exposed. Everything else is fixed: Aggression 180, Sacrifice Appetite 150, Contempt 25, 128 MB hash, 50 ms move overhead.

| Option | Default | What it does |
|---|---|---|
| `Threads` | CPU cores (max 8) | How many CPU threads the engine uses. |
| `BookPath` | ph-gambitbook.bin | Polyglot `.bin` opening book. If the file doesn't exist, the engine just uses its built-in gambit lines and then thinks for itself. |

To change the personality, edit `AGGRESSION` in `src/eval.rs` or `SACRIFICE_BONUS` in `src/search.rs` and rebuild. Aggression 130 with Sacrifice 0 is the strongest setting found in testing.

## Building from source

1. Install [Rust](https://www.rust-lang.org/).
2. Clone the repository and build:
   ```bash
   cargo build --release
   ```
3. Find the binary at `target/release/aggro_chess.exe` (Windows) or `target/release/aggro_chess` (Linux/macOS). Keep `ph-gambitbook.bin` next to it if you want the gambit opening book.

`cargo test --release` runs the test suite (mates, the Greek Gift sacrifice, SEE, eval symmetry, …). `aggro_chess bench` prints a node count and speed.
