# AggroChess v4.3.0 ⚔️

**AggroChess** is an aggressive, sacrificial chess engine written in Rust, in the spirit of Mikhail Tal. It is built to be *fun to play against*: it castles into attacks, throws pawns at your king, keeps pieces on the board, avoids draws, and will happily give up material to rip your king open. The goal is not to beat Stockfish. The goal is to play the kind of chess people enjoy losing to (and occasionally beating).

**Play it in your browser:** https://playaggro.netlify.app/ (game review, clocks, premoves, play-from-any-position, share links and highlight clips).

**Use it in a chess GUI:** download `aggro_chess.exe` from the latest release. It's a standard UCI engine, so it works in Coliseum, Arena, Cute Chess, BanksiaGUI, lichess-bot and others.

## What's new in v4.3: Tal's eyes

**It finds sacrifices other engines can't see.** Given 30 seconds on one core, AggroChess solves seven famous "impossible" positions. At the same setting, Stockfish 17.1 misses four of them outright and misjudges a fifth:

| Puzzle | AggroChess 4.3 | Stockfish 17.1 |
|---|---|---|
| Plaskett's Puzzle, 1.Nf6+!! (the one only Tal solved) | ✅ | ❌ |
| Plaskett's Puzzle, the Tal move 3.Bc2+!! | ✅ | ❌ |
| Topalov-Shirov 1998, 47...Bh3!! | ✅ | ✅ |
| Asauskas-Malisauskas 2004, Qf6+!! | ✅ | found it in one of two runs |
| Penrose's Puzzle (holds the fortress and knows it's a draw) | ✅ | holds, but thinks it's lost |
| The Weird Check, 1.Rd6+!! | ✅ | ❌ |
| Friedel's study, 1.Ba4+!! | ✅ | ❌ |

When it has time to think (4 seconds or more per move, or infinite analysis), it now:
* gives checking sacrifices a separate search of their own (*Tal's look*),
* follows its most forcing lines first when it's in trouble,
* hunts for a forced mate when it's down material near your king,
* takes a second look before settling for a draw in an endgame,
* knows opposite-coloured bishop endings, breakthrough sacrifices (Shirov's ...Bh3!!), bishops walled in by pawns, and two kinds of fortress (Penrose's locked pawns and a sealed pawn wall).

Fast games are not affected by any of that.

**One sacrifice every game, more only when they work.** Until it has sacrificed, its appetite grows from move 20. After that, extra sacrifices have to be close to sound on their own. A sacrifice played a move or two after another one (a Tal chain) keeps a real bonus, as long as its own search says the first one is working.

**The king clamp.** A pawn on h6 or a6 (h3/a3 as Black) against your castled king takes squares from it for good, and every later attack comes with mate threats. AggroChess now values that, and the march of the pawn towards it.

**About 9% faster** (AVX2 for the network when the CPU has it), and only two UCI options are left: `Threads` and `BookPath`.

Against v4.1, every game played to the end, 50 balanced openings from both sides:

| Time control | Result | Games with a sacrifice |
|---|---|---|
| 10 s + 0.1 s (100 games) | **59 W / 12 D / 29 L (65.0%), about +108 Elo** | 100 / 100 |
| 20 s + 0.2 s, long-game features from 1.3 s per move (100 games) | **51 W / 10 D / 39 L (56.0%), about +42 Elo** | 100 / 100 |
| 60 s + 0.6 s (83 games) | **43 W / 5 D / 35 L (54.8%), about +34 Elo** | 83 / 83 |

## What's new in v4.1: no cheap sacrifices

In v4.0 a single pawn pushed next to your king counted as a "sacrifice aimed at the king" and got the full sacrifice bonus, even when it just lost a pawn (1.d4 d5 2.c4 e5 3.dxe5 d4 4.Nf3 Nc6 5.a3 Be6 6.Qb3 d3?, as a player on our Discord found out). A one-pawn sacrifice doesn't even count as a sacrifice, so the bonus now needs a sacrifice of about two pawns or more: a piece, an exchange or two pawns. AggroChess still sacrifices in every game, just not cheaply.

200 games each against v3.5, 10 s + 0.1 s per game, 50 balanced openings from both sides, **every game played to the end** (no adjudication):

| | v4.1 | v4.0 |
|---|---|---|
| **Score vs v3.5** | **123 wins, 37 draws, 40 losses (70.8%), about +153 Elo** (95%: +109 to +203) | 104 W / 32 D / 64 L (60.0%), about +70 Elo (+26 to +117) |
| **Games with a sacrifice** | **200 / 200** | 200 / 200 |
| **Sacrifices per game** | 2.52 | 2.75 |
| **Games with two or more** | 167 / 200 | 163 / 200 |

v3.5 sacrificed in 193 of its 200 games against v4.1 (3.37 per game, many of them material it simply lost).

## What's new in v4.0: a neural network, and a sacrifice in every game

### NNUE, trained only on AggroChess's own games

v4.0 evaluates positions with an **NNUE** (a small, fast neural network) instead of hand-written rules. It was trained **only on AggroChess's own self-play games**: no Stockfish or Leela data, no outside nets. AggroChess played itself with its personality switched off, so every position is labelled with its own honest judgement and the game result, and the network learned from that. It took three rounds and about 3.1 million positions: the first round of games was played and labelled with the classical evaluation, the next two by the network from the round before.

The network only replaces the *objective* part of the evaluation. Everything that makes AggroChess AggroChess is still on top of it: the extra value on attacking your king, the sacrifice bonus, contempt, trade aversion, swindles, beautiful mates and the gambit book.

On its own, the network is worth about **+90 Elo** (same engine, same personality, network against the old evaluation: 63% over 100 games).

### A sacrifice in (almost) every game

AggroChess now keeps track of whether it has sacrificed yet in the current game (2+ pawns down two moves later, still 1.5+ down two moves after that; gambits from its book count too). Once it has, it plays at full strength. If it hasn't, its appetite for a sacrifice grows every move from move 22 (move 20 since v4.3), and much faster once the game is decided either way, until it finds one. It looks for real attacks first and only later accepts any material sacrifice.

Sacrifices are also smarter: the full sacrifice bonus now goes to sacrifices with real attackers behind them (at least a few pieces aimed at your king afterwards), not lone spite sacrifices, and since v4.1 only to sacrifices of about two pawns or more.

### v4.0 vs v3.5 (as released)

**SPRT in Coliseum** (H0: no gain, H1: at least +10 Elo, alpha = beta = 0.05; 10 s + 0.1 s per game, Coliseum's balanced openings): **H1 accepted after 572 games: v4.0 is stronger.** 291 wins, 91 draws, 190 losses, **+62 Elo** (95%: +36 to +89), LOS 100%. v4.0 sacrificed in 561 of the 572 games (98%), 2.03 times per game; the few games without one were decided and adjudicated very early.

Our own match, 200 games, 10 s + 0.1 s per game, 50 balanced openings played from both sides:

| | v4.0 | v3.5 |
|---|---|---|
| **Score** | **112 wins, 26 draws, 62 losses (62.5%), about +89 Elo** (error bars about +43 to +137) | 37.5% |
| **Games with a sacrifice** | **200 / 200** | 191 / 200 |
| **Sacrifices per game** | 2.09 | 2.59 |

v3.5's sacrifices include a lot of material it simply loses to the stronger engine; v4.0's are chosen. About 60% of v4.0's sacrifices come in the first 15 moves, half of them give up a knight or bishop, and around 4 in 10 are aimed straight at the king.

## How it plays

* **Sacrifices, and plans them:** a sacrifice of about two pawns or more aimed at your king gets a bonus of about 3 pawns when it brings real attackers (any exchange sacrifice, a rook for a knight or bishop, gets it too), and it may play one when the line looks up to about 3.1 pawns worse. The bonus also applies to its next move (and at half strength to the move after), so it steers towards positions where a sacrifice will be available. Until it has sacrificed in a game, that appetite keeps growing (see above).
* **Attacks:** a Stockfish-style king-danger model that grows quadratically with the attackers, safe checks and holes around your king. Aggression (220%) multiplies the danger it creates against *your* king, not how it judges its own. Pieces get credit for aiming at your king early, and rooks and queens for open files next to it.
* **Forcing moves first:** when moves are close, it prefers checks, captures and moves into your king zone.
* **Avoids draws:** contempt 50, so it treats a draw as a small loss and plays on.
* **Finishes beautifully:** with a forced mate, it prefers a mate that starts with a sacrifice (a queen sac above all) if that still mates within two moves of the fastest one. Instead of a plain Re8#, you get Qd8+! Bxd8 Re8#.
* **Swindles when worse:** when it's between about 1.5 and 25 pawns down, it looks at its best move plus up to five that are nearly as good, checks every reply you could make to each, and plays the one where most of your natural replies (captures and checks count extra) would throw your advantage away. Since v4.3 it is always on.
* **Keeps pieces on:** every trade costs it, queen trades a lot, and more again when it's behind. Only real trades count: a sacrifice is not a trade.

### Opening book

About 300 hand-picked lines covering both colours, all built in (no file needed):

* **White:** King's Gambit, Evans Gambit, Danish Gambit, Scotch Gambit, Fried Liver, Smith-Morra, Vienna Gambit, Blackmar-Diemer, Milner-Barry (vs the French), Alekhine-Chatard, Staunton Gambit (vs the Dutch), the Tennison Gambit and the Halloween Gambit, plus sharp English lines after 1.c4.
* **Black:** the three-pawn Scandinavian (1.e4 d5 2.exd5 c6 3.dxc6 Nf6 4.cxb7 Bxb7), Stafford, Traxler, Schliemann, Latvian Gambit, Albin Counter-Gambit (with the Lasker trap), Englund Gambit, Benko, Budapest and From's Gambit, plus answers to 1.c4, 1.Nf3, 1.f4, 1.b3, 1.g3, 1.b4 and other offbeat first moves.

Every line is checked for legality by a test, and every book move was screened with a neutral analysis to remove real blunders. The gambits stay in. Lines that let the queens come off early were changed to keep them on.

### Strength

AggroChess is deliberately weaker than it could be: its speculative sacrifices make it weaker than a calmer version of itself would be. v4.1 is clearly stronger than v3.5 all the same, and far stronger than most human players. When it plays a sacrifice, the score it reports includes the sacrifice bonus, so it can look a bit more optimistic than it is for that move.

## Under the hood

* **Evaluation (`src/nnue.rs`, `src/eval.rs`):** an NNUE with 768 inputs (piece and square), a 256-neuron hidden layer for each side, SCReLU activation and 8 output buckets by the number of pieces on the board, updated incrementally as the search makes moves. The personality layer from the classical evaluation (king danger it creates against *your* king at 220%, trade aversion, pieces aimed at the king, heavy pieces on open files near the king) is added on top. The classical evaluation (PeSTO tables, pawn structure, mobility, threats, the king-danger model) is still in the code: it labelled the first round of training data, and it takes over against a bare king and in pawnless endgames with five pieces or fewer (such as rook against bare king, or queen against rook), where its mop-up knows how to drive the king into a corner and the network, which saw few of those positions, does not.
* **Training (`src/datagen.rs`, `trainer/`):** `aggro_chess datagen` plays self-play games from random openings with the personality off and writes quiet positions with the search score and the game result; the trainer (plain Rust, no outside libraries) learns from a blend of the two and writes `aggro.nnue`, which is built into the engine.
* **Search (`src/search.rs`):** iterative deepening, aspiration windows, principal variation search, a lock-free shared hash table and Lazy SMP multi-threading. Null-move pruning, reverse futility pruning, razoring, late move reductions and pruning, futility pruning and singular extensions. Static-exchange pruning is depth-limited and lenient for moves into the enemy king zone, so sacrifices get searched deeply enough to be proven. Check extensions, quiet checks in quiescence search, and killer, counter-move, history and continuation-history move ordering. Time management with soft and hard limits that adapts to how stable the best move is.

## UCI options

Only two settings are exposed. Everything else is fixed: Aggression 220, Sacrifice Appetite 300 (growing until it has sacrificed, then 60), Contempt 50, swindles on, 128 MB hash, 50 ms move overhead.

| Option | Default | What it does |
|---|---|---|
| `Threads` | CPU cores (max 8) | How many CPU threads the engine uses. |
| `BookPath` | ph-gambitbook.bin | Optional Polyglot `.bin` opening book, used only after the built-in gambit lines run out. If the file doesn't exist, the engine thinks for itself from there. |

To change the personality, edit `AGGRESSION` or `KING_CLAMP` in `src/eval.rs`, or `SACRIFICE_BONUS`, `AFTER_SAC_BONUS`, `CHAIN_BONUS` and the sacrifice pressure settings (`PRESSURE_START`, `PRESSURE_STEP`) in `src/search.rs`, and rebuild.

## Building from source

1. Install [Rust](https://www.rust-lang.org/).
2. Clone the repository and build:
   ```bash
   cargo build --release
   ```
3. Find the binary at `target/release/aggro_chess.exe` (Windows) or `target/release/aggro_chess` (Linux/macOS). The opening book and the network (`aggro.nnue`) are built in; `ph-gambitbook.bin` next to it is optional.

`cargo test --release` runs the test suite (mates, the Greek Gift sacrifice, sacrificial mates, swindle timing, every opening-book line, SEE, eval symmetry, incremental network updates matching a full refresh through every kind of move, …). `aggro_chess bench` prints a node count and speed.

### Retraining the network

1. Generate self-play data (here 2 threads, 1 million positions, 3000 nodes per move; add `AGGRO_HCE=1` to label with the classical evaluation instead of the built-in network):
   ```bash
   aggro_chess datagen data/run1 2 1000000 3000
   ```
2. Train (30 passes over the data, learning rate 0.001, 30% weight on game results):
   ```bash
   cd trainer && cargo build --release
   ./target/release/aggro_trainer train ../aggro.nnue 30 0.001 0.3 ../data/run1_0.bin ../data/run1_1.bin
   ```
3. Rebuild the engine. If the new network's scores come out on a different scale, adjust `OUT_SCALE` in `src/nnue.rs` (`aggro_chess evalcmp <data file>` prints classical and network evaluations side by side).

Notes for earlier versions are on the [Releases](https://github.com/PhelRin/AggroChess/releases) page.

---
© 2026 SHIFT · [Play online](https://playaggro.netlify.app/) · [Discord](https://discord.com/invite/uwrQMSsjRM)
