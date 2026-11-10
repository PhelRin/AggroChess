# AggroChess v4.5.1

A bug-fix release for analysis. Games and puzzle solving are unchanged.

## Fixed
- **"stop" is answered at once.** During its first seconds of analysis, AggroChess looks for forcing lines and sacrifices with searches of its own. Those didn't listen to "stop", so it could take up to about 10 seconds to answer. Meanwhile some GUIs moved on to the next position and drew the old position's line on the new one: an arrow starting on an empty square, reported in Coliseum's Analysis Board. It now answers within a few milliseconds.
- **Infinite analysis really keeps going.** `go infinite` was meant to run a full 30-second search and then carry on, but the end of that 30 seconds counted as a "stop" and the analysis ended. It now keeps searching until the GUI says stop, and the sacrifice found in the first 30 seconds stays on show until something better turns up.
- **No early "bestmove" in infinite analysis.** When AggroChess found a forced mate it answered straight away, which GUIs read as the end of the analysis. It now waits for "stop", as the UCI protocol asks.

## Tests
- **Puzzles:** 9 of 13 "impossible" puzzles at 30 s on 1 thread, the same as 4.5.0. Plaskett's Puzzle is still found in infinite analysis with 1 and 2 threads.
- **Analysis:** "stop" now gets a bestmove within 1–4 ms, where 4.5.0 took 9 s on Plaskett. Over 16 back-to-back position changes there were no stale or illegal moves.
- **Games:** 50 W / 7 D / 43 L against 4.5.0 at 10 s + 0.1 s (100 games, about +24 Elo, which is within noise), with a sacrifice in all 100 games.

## Download
`aggro_chess.exe` (Windows x64). Works in any UCI GUI: Coliseum, Arena, Cute Chess, BanksiaGUI, lichess-bot.

SHA-256: `51fdbd5b3999ef0d912140d40751e0ce3dedbcbf5f1152f6e2e4eb5d8fadf1c2`
