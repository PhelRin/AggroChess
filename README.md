# AggroChess v3.2.0 ⚔️

**AggroChess** is an aggressive, sacrificial chess engine written in Rust, in the spirit of Mikhail Tal. It is built to be *fun to play against*: it castles into attacks, throws pawns at your king, keeps pieces on the board, avoids draws, and will happily give up material to rip your king open. The goal is not to beat Stockfish. The goal is to play the kind of chess people enjoy losing to (and occasionally beating).

AggroChess is UCI-compatible, so it works in standard chess GUIs (Arena, Cute Chess, BanksiaGUI, lichess-bot, …).

Play against it easily here: https://playaggro.netlify.app/ (the site runs the same engine in your browser, with game review, clocks, premoves and game sharing).

## What changed in v3.2.0

v3.2 is the "extreme Tal" release: it sacrifices in almost every game, and it is happy to be wrong about it.

* **Sacrifices it plans ahead:** the sacrifice bonus no longer applies only to the move it is about to play. It also applies in full to its *next* move and at half strength to the move after, so it steers towards positions where a sacrifice will be available instead of only spotting the ones on the board.
* **More speculative:** sacrifices are allowed when the line looks up to about 2.6 pawns worse (was 1.2), with a bigger bonus (220, was 150) and more aggression (220%, was 180%).
* **Pulls its pieces towards your king:** pieces get credit for aiming at your king before an attack is strong enough to count as danger, and rooks and queens get a bonus on open files next to your king.
* **Hates trades:** every trade costs it about twice as much as before, and swapping queens costs it a lot more.
* **Forcing moves first:** when moves are close, it prefers checks, captures and moves that hit the king zone. Its own checks are pruned less in the search.
* **Avoids draws harder:** contempt is 50 (was 25).
* **Finishes beautifully:** when it has a forced mate, it prefers a mate that starts with a sacrifice (a queen sac above all) if that still mates within two moves of the fastest one. So instead of a plain Re8#, you get Qd8+! Bxd8 Re8#.
* **New opening book:** about 200 hand-picked lines covering both colours, all built in (no file needed).
  * White: King's Gambit, Evans Gambit, Danish Gambit, Scotch Gambit, Fried Liver, Smith-Morra, Vienna Gambit, Blackmar-Diemer, Milner-Barry (vs the French), Alekhine-Chatard, Staunton Gambit (vs the Dutch) and the Tennison Gambit, plus sharp English lines after 1.c4.
  * Black: the three-pawn Scandinavian (1.e4 d5 2.exd5 c6 3.dxc6 Nf6 4.cxb7 Bxb7), Stafford, Traxler, Schliemann, Albin Counter-Gambit (with the Lasker trap), Benko, Budapest and From's Gambit, plus answers to 1.c4, 1.Nf3, 1.f4, 1.b3, 1.g3, 1.b4 and other offbeat first moves.
  * Every line is checked for legality by a test, and every book move was screened with a neutral analysis to remove real blunders. Gambits stay in.

**Test results** (100 games against v3.0, 5 s + 0.05 s per game, from 50 balanced openings):

| | v3.2 | v3.0 |
|---|---|---|
| Sacrifices per game | **2.00** | 1.26 |
| Games with a sacrifice | **91 / 100** | 71 / 100 |
| Score | 22.5% (about −215 Elo) | 77.5% |

So v3.2 is clearly weaker than v3.0, on purpose: most of its extra sacrifices are speculative, which makes it far more fun (and more beatable) for humans. A milder in-between build (v3.1 settings: Aggression 180, Sacrifice 150, next-move bonus 60%) scored 49.5% against v3.0 with about the same number of sacrifices, if you prefer strength.

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

AggroChess is designed to be played against by people, so its personality is fixed. In v3.2 that is Aggression 220%, Sacrifice Appetite 220 and Contempt 50 (v3.0's "Tal mode" was 180 / 150 / 25; the numbers below are from v3.0). In 40-game test matches against a full-strength copy of itself, Tal mode made about 2.4× as many sacrifices (a sacrifice in 35 of 40 games, roughly 2 per game). The cost was about 320 Elo against that full-strength engine, but it is still far stronger than most human players. When it plays a sacrifice, the score it reports includes the sacrifice bonus, so it can look a bit more optimistic than it is for that move.

## How it compares to v2

Measured with the Tal mode build, books off:
* **40-game match vs v2** (2 s + 0.02 s per game): v3 won 39, lost 1, drew 0.
* **Tactics test** (11 classic positions, 2 s each): v3 found 10/11, v2 found 6/11. That includes the Greek Gift (Bxh7+).

v3 hasn't been formally rated against other engines yet.

## UCI options

Only two settings are exposed. Everything else is fixed: Aggression 220, Sacrifice Appetite 220, Contempt 50, 128 MB hash, 50 ms move overhead.

| Option | Default | What it does |
|---|---|---|
| `Threads` | CPU cores (max 8) | How many CPU threads the engine uses. |
| `BookPath` | ph-gambitbook.bin | Optional Polyglot `.bin` opening book, used only after the built-in gambit repertoire runs out. If the file doesn't exist, the engine thinks for itself from there. |

To change the personality, edit `AGGRESSION` in `src/eval.rs` or `SACRIFICE_BONUS` in `src/search.rs` and rebuild. Aggression 130 with Sacrifice 0 is the strongest setting found in testing.

## Building from source

1. Install [Rust](https://www.rust-lang.org/).
2. Clone the repository and build:
   ```bash
   cargo build --release
   ```
3. Find the binary at `target/release/aggro_chess.exe` (Windows) or `target/release/aggro_chess` (Linux/macOS). The gambit opening repertoire is built in; `ph-gambitbook.bin` next to it is optional and only used after the built-in lines run out.

`cargo test --release` runs the test suite (mates, the Greek Gift sacrifice, sacrificial mates, every opening-book line, SEE, eval symmetry, …). `aggro_chess bench` prints a node count and speed.
