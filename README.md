# AggroChess ⚔️

**AggroChess** is an aggressive, sacrificial chess engine written in Rust, in the spirit of Mikhail Tal. It is built to be *fun to play against*: it castles into attacks, throws pawns at your king, keeps pieces on the board, avoids draws, and will happily give up material to rip your king open. The goal is not to beat Stockfish. The goal is to play the kind of chess people enjoy losing to (and occasionally beating).

Current version: **4.6.1**. What changed in each version is on the [Releases](https://github.com/PhelRin/AggroChess/releases) page.

* **Play it in your browser:** https://playaggro.netlify.app/ (game review, clocks, premoves, play from any position, share links and highlight clips).
* **Watch it solve puzzles:** https://playaggro.netlify.app/puzzles.html
* **Use it in a chess GUI:** download `aggro_chess.exe` (Windows) or `aggro_chess-linux-x86_64` (Linux) from the latest release. It is a standard UCI engine, so it works in Coliseum, Arena, Cute Chess, BanksiaGUI, lichess-bot and others. The release also has a WebAssembly build for web projects.

## What it does

### A sacrifice in nearly every game

AggroChess keeps track of whether it has sacrificed yet in the current game. Until it has, its appetite for a sacrifice grows from move 20, and faster once the game is decided either way. After that, extra sacrifices have to be close to sound on their own.

What counts as a sacrifice: the mover is at least 1.5 pawns down after the opponent's reply. A piece, the exchange and two pawns count; a single pawn does not. Material won back by force (a fork, a discovered attack, a mating net) still counts. Taking straight back on the same square is a trade and does not.

Against its own previous version it sacrificed in 198 of 200 games; the two without one were over in 21 and 27 moves.

### It plays the famous sacrifices

Given 30 seconds on one core, it finds all twelve positions of `tests/famous.epd`, among them Morphy's 10.Nxb5 and 13.Rxd7 from the Opera Game, Fischer's 11...Na4 and 17...Be6 against Byrne, the Evergreen Game's 20.Rxe7+, Kasparov's 24.Rxd4 against Topalov, the Greek gift, the Evans Gambit and the Göring Gambit.

### It solves studies that engines get wrong

30 seconds, 1 thread, each position run 8 times:

| Puzzle | Solved |
|---|---|
| Plaskett's Puzzle, 1.Nf6+!! (the one only Tal solved) | 8 / 8 |
| Plaskett's Puzzle, the Tal move 3.Bc2+!! | 7 / 8 |
| Topalov-Shirov 1998, 47...Bh3!! | 8 / 8 |
| Asauskas-Malisauskas 2004, Qf6+!! | 8 / 8 |
| Penrose's Puzzle (holds the fortress and knows it is a draw) | 8 / 8 |
| The Weird Check, 1.Rd6+!! | 8 / 8 |
| Friedel's study, 1.Ba4+!! | 8 / 8 |
| The Nc6 study, 1.Nc6!! | 8 / 8 |
| Amann's study, 1.Qd8!! | 8 / 8 |
| The Rxh1 study, 1.Rxh1!! | 7 / 8 |
| Salai's study, 1.f4!! | 8 / 8 |
| Smyslov's study, 1.f6!! | 0 / 8 |

The positions are in `tests/`. For comparison, Stockfish 17.1 was given the first seven at the same setting (30 seconds, one core) when they were added: it missed both Plaskett positions, the Weird Check and Friedel's study, found Qf6+ in one of two runs, and held Penrose's fortress while scoring it as lost. That is one setting; with more threads or more time its results can differ.

How it finds them, when it has at least 4 seconds for a move:

* **Tal's look:** sacrifices get a search of their own, apart from the main search, and so do quiet moves whose main line goes on with a sacrifice.
* **Rescue look:** in a lost endgame, every move that gives material back gets its own search, so a saving sacrifice or a stalemate is not pruned away.
* **A mate prover** that hunts for a forced mate when it is down material near your king.
* **Fortress knowledge:** locked pawn walls, sealed positions, stalemate boxes, bishops walled in by pawns.
* **A second network** for endings with a buried bishop (see below).

Fast games are not affected by any of that.

## How it plays

* **Sacrifices, and plans them:** a sacrifice aimed at your king gets a bonus of about 3 pawns when it brings real attackers, and it may play one when the line looks up to about 3 pawns worse. At a long think, a move that simply leaves a rook or the queen to be taken gets half of that. The bonus also applies to its next move, so it steers towards positions where a sacrifice will be there. A second sacrifice a move or two after the first (a Tal chain) keeps a real bonus as long as the first one is working.
* **Gives material back:** when you sacrificed first and it is ahead without clearly winning, returning some of it counts as a sacrifice too.
* **Attacks:** a king-danger model that grows quadratically with the attackers, safe checks and holes around your king. Aggression (220%) multiplies the danger it creates against *your* king, not how it judges its own. A pawn on h6 or a6 against your castled king (the king clamp) is valued for the mate threats it brings later.
* **Forcing moves first:** when moves are close, it prefers checks, captures and moves into your king zone.
* **Avoids draws:** contempt 50, so it treats a draw as a small loss and plays on.
* **Finishes beautifully:** with a forced mate, it prefers a mate that starts with a sacrifice (a queen sacrifice above all) if that still mates within two moves of the fastest one. Instead of a plain Re8#, you get Qd8+! Bxd8 Re8#.
* **Swindles when worse:** it looks at its best move plus up to five that are nearly as good, checks every reply you could make to each, and plays the one where most of your natural replies would throw your advantage away.
* **Keeps pieces on:** every trade costs it, queen trades a lot, and more again when it is behind.

### Opening book

About 300 hand-picked lines covering both colours, all built in (no file needed):

* **White:** King's Gambit, Evans Gambit, Danish Gambit, Göring Gambit, Fried Liver, Smith-Morra, Vienna Gambit, Blackmar-Diemer, Milner-Barry (against the French), Alekhine-Chatard, Staunton Gambit (against the Dutch), the Tennison Gambit and the Halloween Gambit, plus sharp English lines after 1.c4.
* **Black:** the three-pawn Scandinavian (1.e4 d5 2.exd5 c6 3.dxc6 Nf6 4.cxb7 Bxb7), Stafford, Traxler, Schliemann, Latvian Gambit, Albin Counter-Gambit (with the Lasker trap), Englund Gambit, Benko, Budapest and From's Gambit, plus answers to 1.c4, 1.Nf3, 1.f4, 1.b3, 1.g3, 1.b4 and other offbeat first moves.

Every line is checked for legality by a test, and every book move was screened with a neutral analysis to remove real blunders. The gambits stay in.

### Strength

AggroChess is deliberately weaker than it could be: its speculative sacrifices cost it games a calmer version of itself would win. It is still far stronger than most human players. When it plays a sacrifice, the score it reports includes the sacrifice bonus, so it can look a bit more optimistic than it is for that move.

## Under the hood

* **Evaluation (`src/nnue.rs`, `src/eval.rs`):** an NNUE with 768 inputs (piece and square), a 256-neuron hidden layer for each side, SCReLU activation and 8 output buckets by the number of pieces on the board, updated incrementally as the search makes moves. It was trained **only on AggroChess's own self-play games**: no Stockfish or Leela data, no outside nets. The personality (king danger against *your* king, trade aversion, pieces aimed at the king) is added on top of it. A classical evaluation takes over against a bare king and in pawnless endgames with five pieces or fewer.
* **A second network (`aggro-ocb.nnue`):** opposite-coloured bishop endings in which one bishop is buried behind its own pawns are judged by a network of their own (the main network trained further). The positions it learned from were labelled by long AggroChess searches, not by game results; `trainer/themes/` has the recipe. The main network is used everywhere else.
* **Search (`src/search.rs`):** iterative deepening, aspiration windows, principal variation search, a lock-free shared hash table and Lazy SMP. Null-move pruning, reverse futility pruning, razoring, late move reductions and pruning, singular extensions. Static-exchange pruning is lenient for moves into the enemy king zone, so sacrifices are searched deeply enough to be proven. On top of that, at long thinks: Tal's look, the rescue look and the mate prover (`src/matesearch.rs`).
* **Training (`src/datagen.rs`, `trainer/`):** `aggro_chess datagen` plays self-play games with the personality off and writes quiet positions with the search score and the game result; `aggro_chess label` searches chosen positions far deeper and writes them with that score. The trainer is plain Rust with no outside libraries.

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
3. Find the binary at `target/release/aggro_chess.exe` (Windows) or `target/release/aggro_chess` (Linux/macOS). The opening book and both networks are built in; `ph-gambitbook.bin` next to it is optional.

`cargo test --release` runs the test suite. `aggro_chess bench` prints a node count that is the same for every correct build of a version (170599 for v4.6.1), and the speed of your machine.

### Retraining the network

1. Generate self-play data (here 2 threads, 1 million positions, 3000 nodes per move):
   ```bash
   aggro_chess datagen data/run1 2 1000000 3000
   ```
2. Train (30 passes over the data, learning rate 0.001, 30% weight on game results):
   ```bash
   cd trainer && cargo build --release
   ./target/release/aggro_trainer train ../aggro.nnue 30 0.001 0.3 ../data/run1_0.bin ../data/run1_1.bin
   ```
   Put an existing `.nnue` file among the inputs to carry on from it instead of starting fresh.
3. Rebuild the engine. If the new network's scores come out on a different scale, adjust `OUT_SCALE` in `src/nnue.rs` (`aggro_chess evalcmp <data file>` prints classical and network evaluations side by side).

---
© 2026 SHIFT · [Play online](https://playaggro.netlify.app/) · [Discord](https://discord.com/invite/uwrQMSsjRM)
