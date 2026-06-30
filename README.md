# AggroChess v2.0.0 ⚔️

**AggroChess** is a tactical, highly aggressive chess engine written in Rust, inspired by the legendary playstyle of World Champion Mikhail Tal. Unlike engines that grind out microscopic positional advantages, AggroChess prioritizes active piece play, direct king attacks, and speculative sacrifices to generate sharp, complex, and fun tactical struggles.

**Version 2.0.0** introduces a massive playing strength boost (estimated **+200+ Elo** overall) through advanced search optimizations, sophisticated move ordering, and refined attacking heuristics. It is much stronger and tactically precise while remaining fiercely aggressive.

AggroChess is UCI-compatible, allowing it to be easily integrated into standard chess GUIs (like Arena, Cute Chess, Lichess, or ChessBase).

---

### Installation & Setup Instructions

Since this release only includes the precompiled standalone executable, follow these steps to play against it:

1.  **Download the Binary**: Download the `aggro_chess.exe` file attached to this release.
2.  **Load in a Chess GUI**:
    *   **Arena Chess GUI**: Go to `Engines` -> `New Engine` -> Select `UCI` -> Browse and select the downloaded `aggro_chess.exe`.
    *   **Cute Chess / Lichess / ChessBase**: Add the engine as a standard UCI engine pointing to the path of `aggro_chess.exe`.
3.  **Opening Book Setup (Optional)**:
    *   By default, the engine searches for `ph-gambitbook.bin` in the folder it is executed from to supplement non-gambit openings.
    *   To play **strictly the built-in curated gambits**, configure the engine option `BookPath` to a non-existent file (e.g. `none.bin`).
4.  **TT Hash Customization**: Adjust the `Hash` option in your GUI to change search memory size in Megabytes (defaults to 16MB).
