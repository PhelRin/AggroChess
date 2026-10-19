# AggroChess v3.3.0 ⚔️

**AggroChess** is an aggressive, sacrificial chess engine written in Rust, in the spirit of Mikhail Tal. It is built to be *fun to play against*: it castles into attacks, throws pawns at your king, keeps pieces on the board, avoids draws, and will happily give up material to rip your king open. The goal is not to beat Stockfish. The goal is to play the kind of chess people enjoy losing to (and occasionally beating).

**Play it in your browser:** https://playaggro.netlify.app/ (the same engine, with game review, clocks, premoves, play-from-any-position, share links and highlight clips).

**Use it in a chess GUI:** download `aggro_chess.exe` from the latest release. It's a standard UCI engine, so it works in Arena, Cute Chess, BanksiaGUI, lichess-bot and others.

## v3.3 vs v3.2

v3.2 taught AggroChess to sacrifice in almost every game. v3.3, "The Swindler", teaches it what to do when those sacrifices go wrong: it doesn't go quietly.

| | v3.2 | v3.3 |
|---|---|---|
| When it's clearly losing | Plays the objectively "least bad" move | **Plays the move that sets the most traps**, if it's nearly as good |
| Turning that off | – | **New UCI option `Swindle`** (default on) |
| Resigning (website) | Never resigns | **Resigns** after seeing itself lost for 3 moves in a row (plays on if you're low on time) |
| Falling for a trap (website) | – | **"Swindled!"** message |
| Personality (Aggression / Sacrifice / Contempt) | 220 / 220 / 50 | 220 / 220 / 50 (same) |
| Opening book | ~200 built-in gambit lines | Same |
| "Finish beautifully" sacrificial mates | Yes | Yes (same) |
| Score vs the previous build | 22.5% vs v3.0 (more sacrifices, on purpose) | 45.5% vs v3.3 without swindles (−31 Elo, not a clear difference) |

### How swindling works

When AggroChess is between about 1.5 and 25 pawns down, it looks at its best move plus up to five others that are nearly as good (within half a pawn to 1.5 pawns of it, with more slack the more lost it is). For each one, it checks every reply you could make. Then it plays the move where the largest share of your natural replies would throw away at least half of your advantage. Captures count 3× and checks 2×, because those are the moves people reach for first.

* It only switches when the trap rate is clearly higher than the best move's (at least 15 points higher, and at least 20%).
* It never spends more than half the planned time for the move on this.

**Test:** 100 games, v3.3 with swindles against itself without them (10 s + 0.1 s per game, from 50 balanced openings):

| | Swindle on | Swindle off |
|---|---|---|
| Score | 45.5% | 54.5% |
| Elo | −31 (error bars −95 to +30) | |

That's not a clear difference, which was the goal. Engines don't fall for traps, so against them a swindle is just a slightly worse move. Against people, that's the whole point. For engine-vs-engine tournaments, set `Swindle` to false.

## How it plays

* **Sacrifices, and plans them:** a sacrifice aimed at your king gets a bonus of about 2.2 pawns, and it may play one when the line looks up to about 2.6 pawns worse. The bonus also applies to its next move (and at half strength to the move after), so it steers towards positions where a sacrifice will be available.
* **Attacks:** a Stockfish-style king-danger model that grows quadratically with the attackers, safe checks and holes around your king. Aggression (220%) multiplies the danger it creates against *your* king, not how it judges its own. Pieces get credit for aiming at your king early, and rooks and queens for open files next to it.
* **Hates trades:** every trade costs it a little, and swapping queens costs it a lot.
* **Forcing moves first:** when moves are close, it prefers checks, captures and moves into your king zone.
* **Avoids draws:** contempt 50, so it treats a draw as a small loss and plays on.
* **Finishes beautifully:** with a forced mate, it prefers a mate that starts with a sacrifice (a queen sac above all) if that still mates within two moves of the fastest one. Instead of a plain Re8#, you get Qd8+! Bxd8 Re8#.
* **Swindles when worse** (new in v3.3, see above).

### Opening book

About 200 hand-picked lines covering both colours, all built in (no file needed):

* **White:** King's Gambit, Evans Gambit, Danish Gambit, Scotch Gambit, Fried Liver, Smith-Morra, Vienna Gambit, Blackmar-Diemer, Milner-Barry (vs the French), Alekhine-Chatard, Staunton Gambit (vs the Dutch) and the Tennison Gambit, plus sharp English lines after 1.c4.
* **Black:** the three-pawn Scandinavian (1.e4 d5 2.exd5 c6 3.dxc6 Nf6 4.cxb7 Bxb7), Stafford, Traxler, Schliemann, Albin Counter-Gambit (with the Lasker trap), Benko, Budapest and From's Gambit, plus answers to 1.c4, 1.Nf3, 1.f4, 1.b3, 1.g3, 1.b4 and other offbeat first moves.

Every line is checked for legality by a test, and every book move was screened with a neutral analysis to remove real blunders. The gambits stay in.

### Strength

AggroChess is deliberately weaker than it could be: its speculative sacrifices cost it about 215 Elo against its calmer v3.0 self. It is still far stronger than most human players. When it plays a sacrifice, the score it reports includes the sacrifice bonus, so it can look a bit more optimistic than it is for that move.

## Under the hood

* **Evaluation (`src/eval.rs`):** PeSTO tapered piece-square tables, cached pawn structure (passed, doubled, isolated, connected), mobility, outposts, rooks on open files, bishop pair, threats and hanging pieces, passed-pawn races, endgame scaling (opposite bishops, insufficient material) and mop-up for mating a bare king, plus the king-danger model and personality layer above.
* **Search (`src/search.rs`):** iterative deepening, aspiration windows, principal variation search, a lock-free shared hash table and Lazy SMP multi-threading. Null-move pruning, reverse futility pruning, razoring, late move reductions and pruning, futility pruning and singular extensions. Static-exchange pruning is depth-limited and lenient for moves into the enemy king zone, so sacrifices get searched deeply enough to be proven. Check extensions, quiet checks in quiescence search, and killer, counter-move, history and continuation-history move ordering. Time management with soft and hard limits that adapts to how stable the best move is.

## UCI options

Only three settings are exposed. Everything else is fixed: Aggression 220, Sacrifice Appetite 220, Contempt 50, 128 MB hash, 50 ms move overhead.

| Option | Default | What it does |
|---|---|---|
| `Threads` | CPU cores (max 8) | How many CPU threads the engine uses. |
| `Swindle` | true | When clearly worse, play the move that sets the most traps instead of the objectively best one (if it is nearly as good). Turn off for engine-vs-engine matches. |
| `BookPath` | ph-gambitbook.bin | Optional Polyglot `.bin` opening book, used only after the built-in gambit lines run out. If the file doesn't exist, the engine thinks for itself from there. |

To change the personality, edit `AGGRESSION` in `src/eval.rs` or `SACRIFICE_BONUS` in `src/search.rs` and rebuild. Aggression 130 with Sacrifice 0 is the strongest setting found in testing.

## Building from source

1. Install [Rust](https://www.rust-lang.org/).
2. Clone the repository and build:
   ```bash
   cargo build --release
   ```
3. Find the binary at `target/release/aggro_chess.exe` (Windows) or `target/release/aggro_chess` (Linux/macOS). The opening book is built in; `ph-gambitbook.bin` next to it is optional.

`cargo test --release` runs the test suite (mates, the Greek Gift sacrifice, sacrificial mates, swindle timing, every opening-book line, SEE, eval symmetry, …). `aggro_chess bench` prints a node count and speed.

Notes for earlier versions are on the [Releases](https://github.com/PhelRin/AggroChess/releases) page.

---
© 2026 SHIFT · [Play online](https://playaggro.netlify.app/) · [Discord](https://discord.com/invite/uwrQMSsjRM)
