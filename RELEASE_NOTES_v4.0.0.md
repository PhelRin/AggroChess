# AggroChess v4.0.0 ⚔️ "Neural Tal"

v4.0 gives AggroChess a neural network, trained only on its own games, and makes it sacrifice in practically every game. It is clearly stronger than v3.5 and just as willing to throw pieces at your king.

## What's new

**A neural network (NNUE), trained only on AggroChess's own games**
- Positions are now evaluated by a small, fast neural network instead of hand-written rules.
- It was trained purely on AggroChess's own self-play: no Stockfish or Leela data, no outside nets. AggroChess played itself with its personality switched off, so every position was labelled with its own honest judgement and the game's result. Three rounds, about 3.1 million positions.
- The network only replaces the *objective* part of the evaluation. The personality is still on top: the extra value on attacking your king, the sacrifice bonus, contempt, trade aversion, swindles, beautiful mates and the gambit book.
- On its own the network is worth about +90 Elo (the same engine with the old evaluation scores 37% against it).

**A sacrifice in (almost) every game**
- AggroChess now keeps track of whether it has sacrificed yet in the current game (2+ pawns down two moves later, still 1.5+ down two moves after that; gambits from its book count).
- If it has, it plays at full strength. If it hasn't, its appetite grows every move from move 22, and much faster once the game is decided either way, until it finds one.
- It looks for real attacks first, and only later accepts any material sacrifice.

**Smarter sacrifices**
- The full sacrifice bonus (now about 3 pawns) goes to sacrifices with real attackers behind them, at least a couple of pieces aimed at your king afterwards, not lone spite sacrifices. Exchange sacrifices still always get it.

**Endgames**
- Against a bare king and in small pawnless endgames (such as rook against bare king, or queen against rook), AggroChess switches back to the classical evaluation, whose mop-up drives the king into a corner.

## Test results

**SPRT in Coliseum** (H1: at least +10 Elo, alpha = beta = 0.05; 10 s + 0.1 s, Coliseum's balanced openings):

| | v4.0 | v3.5 |
|---|---|---|
| Result | **H1 accepted after 572 games: v4.0 is stronger** | |
| Score | **291 W / 91 D / 190 L, +62 Elo** (95%: +36 to +89), LOS 100% | |
| Games with a sacrifice | **561 / 572 (98%)** | 546 / 572 (95%) |
| Sacrifices per game | 2.03 | 2.30 |

**Our own match**, 200 games (10 s + 0.1 s, 50 balanced openings played from both sides):

| | v4.0 | v3.5 |
|---|---|---|
| Score | **62.5% (112 W / 26 D / 62 L), about +89 Elo** | 37.5% |
| Games with a sacrifice | **200 / 200** | 191 / 200 |
| Sacrifices per game | 2.09 | 2.59 |

v3.5's count includes a lot of material it simply loses to the stronger engine; v4.0's sacrifices are chosen. About two thirds of v4.0's games have two or more. The few Coliseum games without one were decided and adjudicated very early (Coliseum stops a game once one side is 7 pawns down for 4 moves).

## Unchanged
The same trade aversion, swindle mode (`Swindle` UCI option), "finish beautifully" sacrificial mates and the built-in gambit book (King's Gambit, Evans, Danish, Halloween, Latvian, Englund, Stafford and the rest). The UCI options are the same three: `Threads`, `Swindle`, `BookPath`.

## Also in this release
- `aggro_chess datagen` (self-play data generation) and the network trainer (`trainer/`, plain Rust) are included, so the network can be retrained from scratch. See the README.
- The website (https://playaggro.netlify.app/) runs the same v4.0 engine, sacrifice tracking included.

## Download
`aggro_chess.exe` (Windows x64). Works in any UCI GUI: Coliseum, Arena, Cute Chess, BanksiaGUI, lichess-bot. The network is built in; no extra files needed.

SHA-256: `25938a31aa9bcaa3f4ffd0cd1801f70d16d464d90e61c04d16a0429f6cf9ba75`
