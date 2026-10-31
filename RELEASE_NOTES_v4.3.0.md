# AggroChess v4.3.0 ⚔️ "Tal's Eyes"

AggroChess now finds sacrifices that engines can't see. Given time to think, it solves seven famous "impossible" positions, including Plaskett's Puzzle (the one only Tal solved). At the same 30 seconds on one core, Stockfish 17.1 misses four of the seven outright and misjudges a fifth. It is also stronger than v4.1 at both fast and long time controls, and still sacrifices in every game.

## What's new

**One sacrifice every game, more only when they work**
- Every game still gets a sacrifice: until AggroChess has sacrificed, its appetite grows from move 20 (it was move 22).
- Once it has sacrificed, the extra appetite drops right down. Later sacrifices have to be close to sound on their own. In v4.1 it averaged about three forced-feeling sacrifices a game, and that was costing it games at longer time controls.

**Chained sacrifices**
- A sacrifice played one or two moves after another one keeps a real bonus, as long as AggroChess's own search (without the bonus) says the first sacrifice is working.
- Inside the search, a sacrifice followed by another sacrifice two plies later gets the full bonus for the second one too. So when a Tal-style double sacrifice is there, it goes for it.

**The king clamp (AlphaZero's h-pawn)**
- A pawn of AggroChess's on the 6th rank against your castled king (h6 or a6, or h3/a3 when it plays Black) takes squares from your king for good.
- Nothing happens straight away, so a plain search can't see what it's worth. But every later attack comes with mate threats you'll have to keep parrying. AggroChess now values the clamp, and the march of the pawn towards it.

**Thinking like Tal when it has time** (4 seconds or more per move, or infinite analysis; fast games are unaffected)
- **Tal's look:** checking sacrifices at the root get a separate search of their own. Inside the normal search they are only tested against a window set by the other moves, so a deep sacrifice gets pruned away.
- **Forcing lines first:** when it's in trouble, it follows its most forcing checks a few moves ahead and searches the critical positions first.
- **Mate hunt:** when it is down a lot of material near your king, a dedicated mate search (checks, promotions and mate threats) looks for a forced mate.
- **No easy draws in endgames:** a move that only draws gets a second look at the alternatives.
- **Endgame knowledge the network lacked:**
  - opposite-coloured bishop endings are drawish;
  - a piece thrown in front of the pawns to make a passed pawn (Shirov's ...Bh3!!);
  - a bishop walled in by pawns is nearly worthless;
  - two kinds of fortress: Penrose's locked pawns, and a sealed pawn wall that nothing can cross (Friedel's study).

**About 9% faster:** AVX2 for the network when the CPU has it, a fused network update, and fewer board copies in the search.

**Simpler options:** only `Threads` and `BookPath` are left. The swindle mode is always on.

## The "impossible" puzzles

30 seconds per position, 1 thread:

| Puzzle | AggroChess 4.3 | Stockfish 17.1 |
|---|---|---|
| Plaskett's Puzzle, 1.Nf6+!! (the one only Tal solved) | ✅ | ❌ Ne3 |
| Plaskett's Puzzle, the Tal move 3.Bc2+!! | ✅ | ❌ Nf4 |
| Topalov-Shirov 1998, 47...Bh3!! | ✅ | ✅ |
| Asauskas-Malisauskas 2004, Qf6+!! | ✅ | ✅/❌ (found it in one of two runs) |
| Penrose's Puzzle (fortress: holds it and knows it's a draw) | ✅ | ❌ holds, but thinks it's lost (−7) |
| The Weird Check, 1.Rd6+!! | ✅ | ❌ Ne7+ |
| Friedel's study, 1.Ba4+!! | ✅ | ❌ exf6 |

## Test results

All games played to the end, 50 balanced openings from both sides:

| | vs v4.1 |
|---|---|
| 60 s + 0.6 s per game (83 games) | **43 W / 5 D / 35 L (54.8%), about +34 Elo** |
| 20 s + 0.2 s per game (long-game features from 1.3 s per move, 100 games) | **51 W / 10 D / 39 L (56.0%), about +42 Elo** |
| 10 s + 0.1 s per game (100 games) | **59 W / 12 D / 29 L (65.0%), about +108 Elo** |
| Games with a sacrifice | **100%** in each match |
| Tal sacrifice suite (100 of Tal's sacrifices, 30 s each) | 59 (v4.1: 60) |

On the Tal suite Stockfish 17.1 plays Tal's exact move 66 times out of 100, AggroChess 59. But in 6 of the positions AggroChess "misses", its own move is just as good as Tal's (Stockfish agrees), and it finds 7 of Tal's sacrifices that Stockfish doesn't play, including both knight sacrifices against Larsen (1965) and Botvinnik-Tal 1960, 21...Nf4.

## Download
`aggro_chess.exe` (Windows x64). Works in any UCI GUI: Coliseum, Arena, Cute Chess, BanksiaGUI, lichess-bot. The network is built in; no extra files needed.

SHA-256: `665d907703497e8d0dd15ca6a3e5d9d69d706384342485bbdd9479f496892eca`
