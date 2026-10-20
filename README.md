# AggroChess v3.4.0 ⚔️

**AggroChess** is an aggressive, sacrificial chess engine written in Rust, in the spirit of Mikhail Tal. It is built to be *fun to play against*: it castles into attacks, throws pawns at your king, keeps pieces on the board, avoids draws, and will happily give up material to rip your king open. The goal is not to beat Stockfish. The goal is to play the kind of chess people enjoy losing to (and occasionally beating).

**Play it in your browser:** https://playaggro.netlify.app/ (the same engine, with game review, clocks, premoves, play-from-any-position, share links and highlight clips).

**Use it in a chess GUI:** download `aggro_chess.exe` from the latest release. It's a standard UCI engine, so it works in Arena, Cute Chess, BanksiaGUI, lichess-bot and others.

## v3.4 vs v3.3

v3.4 keeps pieces on the board. v3.3 traded too easily, and one of its opening lines even handed the opponent a queenless game on move 7. A Tal-style engine wants the opposite: the more pieces on the board, the more chances a human has to go wrong.

| | v3.3 | v3.4 |
|---|---|---|
| **Trades** | Each trade cost it about 0.12 pawns | **About 0.24 pawns** per trade (0.38 when it's behind) |
| **Queen trades** | Cost about 0.7 pawns | **About 1.4 pawns** (2.2 when it's behind) |
| **When behind in material** | Traded like in any other position | **Avoids trades all the way into the endgame**, to keep swindling chances |
| **Opening book** | A few lines led to early queen trades (e.g. the Urusov: 6...Qe7+ 7.Qe2 and the queens come off on move 7) | Those lines now keep the queens on |
| **New gambits** | – | **Halloween**, **Latvian** and **Englund** gambits |
| **Thinking on your time (website)** | No | **Yes** (pondering) |
| **Sacrifice sound (website)** | Played a sound, which gave the trap away | **Silent** |
| Personality, swindle mode, "finish beautifully" mates | Aggression 220, Sacrifice 220, Contempt 50 | Same |
| **Head-to-head (50 games)** | 36% | **64%, about +100 Elo** (error bars +13 to +201) |
| Sacrifices per game in that match | 2.14 | 1.64 |

### New gambits

Objectively dubious, but they set real problems for anyone who doesn't know them. Each comes up in roughly one game in five or six where it's possible.

* **Halloween Gambit** (White): 1.e4 e5 2.Nf3 Nc6 3.Nc3 Nf6 4.Nxe5!?, a whole knight for a pawn and a big centre.
* **Latvian Gambit** (Black): 1.e4 e5 2.Nf3 f5!?, the wildest reply to 2.Nf3.
* **Englund Gambit** (Black): 1.d4 e5!?, including the famous trap 7.Qd2?? Bxc3 8.Qxc3 Qc1#.

### The match

50 games, v3.4 against v3.3, 10 s + 0.1 s per game, 25 balanced openings played from both sides: **28 wins, 8 draws, 14 losses (64%)**. Keeping pieces on made it both more dangerous and stronger. It sacrificed a bit less often in this match.

### Pondering (website)

While you think, AggroChess guesses your reply from its own analysis and keeps searching the position after it. If you play the move it expected, it starts its turn already deep in the search; if not, much of the work still helps. It runs in short slices so your move is never kept waiting, and stops after 45 seconds or when the tab is hidden.

## How it plays

* **Sacrifices, and plans them:** a sacrifice aimed at your king gets a bonus of about 2.2 pawns, and it may play one when the line looks up to about 2.6 pawns worse. The bonus also applies to its next move (and at half strength to the move after), so it steers towards positions where a sacrifice will be available.
* **Attacks:** a Stockfish-style king-danger model that grows quadratically with the attackers, safe checks and holes around your king. Aggression (220%) multiplies the danger it creates against *your* king, not how it judges its own. Pieces get credit for aiming at your king early, and rooks and queens for open files next to it.
* **Forcing moves first:** when moves are close, it prefers checks, captures and moves into your king zone.
* **Avoids draws:** contempt 50, so it treats a draw as a small loss and plays on.
* **Finishes beautifully:** with a forced mate, it prefers a mate that starts with a sacrifice (a queen sac above all) if that still mates within two moves of the fastest one. Instead of a plain Re8#, you get Qd8+! Bxd8 Re8#.
* **Swindles when worse:** when it's between about 1.5 and 25 pawns down, it looks at its best move plus up to five that are nearly as good, checks every reply you could make to each, and plays the one where most of your natural replies (captures and checks count extra) would throw your advantage away. Turn it off with the `Swindle` UCI option for engine-vs-engine matches.
* **Keeps pieces on:** every trade costs it, queen trades a lot, and more again when it's behind.

### Opening book

About 300 hand-picked lines covering both colours, all built in (no file needed):

* **White:** King's Gambit, Evans Gambit, Danish Gambit, Scotch Gambit, Fried Liver, Smith-Morra, Vienna Gambit, Blackmar-Diemer, Milner-Barry (vs the French), Alekhine-Chatard, Staunton Gambit (vs the Dutch), the Tennison Gambit and the Halloween Gambit, plus sharp English lines after 1.c4.
* **Black:** the three-pawn Scandinavian (1.e4 d5 2.exd5 c6 3.dxc6 Nf6 4.cxb7 Bxb7), Stafford, Traxler, Schliemann, Latvian Gambit, Albin Counter-Gambit (with the Lasker trap), Englund Gambit, Benko, Budapest and From's Gambit, plus answers to 1.c4, 1.Nf3, 1.f4, 1.b3, 1.g3, 1.b4 and other offbeat first moves.

Every line is checked for legality by a test, and every book move was screened with a neutral analysis to remove real blunders. The gambits stay in. Lines that let the queens come off early were changed to keep them on.

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
