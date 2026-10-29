# AggroChess v4.1.0 ⚔️ "No Cheap Sacrifices"

A small change with a big effect: AggroChess no longer throws away single pawns and calls it a sacrifice. It is about +80 Elo stronger than v4.0 and still sacrifices in every game.

## What's new

**Real sacrifices only**
- In v4.0 a single pawn pushed next to your king counted as a "sacrifice aimed at the king" and got the full ~3-pawn sacrifice bonus, even when it simply lost a pawn. A player on our Discord found one in the Albin Countergambit: 1.d4 d5 2.c4 e5 3.dxe5 d4 4.Nf3 Nc6 5.a3 Be6 6.Qb3 d3?, which loses about three pawns.
- The sacrifice bonus now needs a sacrifice of about two pawns or more: a piece, an exchange or two pawns. A one-pawn "sacrifice" never counted as a sacrifice anyway, so it was pure cost.
- A new test makes sure that Albin position never gets 6...d3 again.

Everything else is unchanged from v4.0: the NNUE trained only on AggroChess's own games, the sacrifice tracking that keeps its appetite growing until it has sacrificed, the attack-scaled bonus, contempt, trade aversion, swindles, beautiful mates and the gambit book.

## Test results

200 games each against v3.5, 10 s + 0.1 s per game, 50 balanced openings from both sides, **every game played to the end** (no adjudication; 82% ended in checkmate):

| | v4.1 | v4.0 |
|---|---|---|
| Score vs v3.5 | **123 W / 37 D / 40 L (70.8%)** | 104 W / 32 D / 64 L (60.0%) |
| Elo vs v3.5 | **+153** (95%: +109 to +203) | +70 (95%: +26 to +117) |
| Games with a sacrifice | **200 / 200** | 200 / 200 |
| Sacrifices per game | 2.52 | 2.75 |
| Games with two or more | 167 / 200 | 163 / 200 |

v4.1 makes slightly fewer sacrifices per game because the missing ones were exactly those one-pawn giveaways.

## Also updated
- The website (https://playaggro.netlify.app/) runs v4.1, and now also highlights **your** sacrifices: marked ⚔ in the move list, a "Your sacrifice!" note, who sacrificed how often on the result card, a "Your sacrifices" stat, and they can be the moment a highlight clip shows.

## Download
`aggro_chess.exe` (Windows x64). Works in any UCI GUI: Coliseum, Arena, Cute Chess, BanksiaGUI, lichess-bot. The network is built in; no extra files needed.

SHA-256: `99971284266dba086d6c836b305fd54e4fc39a9d392f2308ec4287b26cb0c387`
