# AggroChess v2.0.0 ⚔️

**AggroChess** is a tactical, highly aggressive chess engine written in Rust, inspired by the legendary playstyle of World Champion Mikhail Tal. Unlike engines that grind out microscopic positional advantages, AggroChess prioritizes active piece play, direct king attacks, and speculative sacrifices to generate sharp, complex, and fun tactical struggles.

**Version 2.0.0** introduces a massive playing strength boost (estimated **+200+ Elo** overall) through advanced search optimizations, sophisticated move ordering, and refined attacking heuristics. It is much stronger and tactically precise while remaining fiercely aggressive.

AggroChess is UCI-compatible, allowing it to be easily integrated into standard chess GUIs (like Arena, Cute Chess, Lichess, or ChessBase).

---

## What's New in v2.0.0 🚀

### 1. Refined Mikhail Tal Evaluation Heuristics 🎭
*   **Attack Cohesion Requirements**: The speculative sacrifice discount is now disabled unless the attacking pieces target the *same* squares in the enemy king safety ring, ensuring sacrifices are coordinated rather than random.
*   **Piece Participation Escalation**: Awards massive exponential bonuses when multiple pieces participate in the assault (e.g. +60 cp for 4 attackers, +240 cp for 5, and +540 cp for 6), forcing the engine to attack with its entire army.
*   **Opponent Defenses Counter-Scaling**: Measures the defensive wall around the enemy king and scales down the sacrifice discount if the king is heavily guarded, preventing suicidal opening blunders (e.g. premature knight sacrifices).
*   **Passive Piece Penalties**: Applies a middlegame penalty of `-20 cp` per minor/major piece sitting passively on the 1st or 2nd rank during active attacks, forcing full-army mobilization.
*   **Attacking Piece Proximity Check**: Restricts speculative sacrifices to situations where at least two active attackers are within Chebyshev distance 3 of the enemy king.
*   **Dynamic Pawn Shields & Storms**: Evaluates pawn shield damage dynamically and rewards open-file pawn storms.

### 2. High-Performance Search Optimization ⚡
*   **Attacking Move Extensions**: Automatically extends search depth by 1 ply when a move delivers a check, enters the opponent's king safety ring, or targets a major piece (Queen/Rook).
*   **Singular Extensions at Depth 7**: Lowered the singular extensions threshold to depth 7 to resolve forcing tactical sequences and critical defensive replies earlier in the tree.
*   **Quiet SEE Pruning**: Prunes quiet moves failing Static Exchange Evaluation (SEE) at shallow depths (`depth >= 1`), but relaxes the threshold to `-80` for moves attacking the enemy king to preserve speculative play.
*   **Tactical NMP Verification**: Detects when passing the turn creates an immediate mating threat, disabling Late Move Reductions (LMR) defensively to verify defensive responses.
*   **ProbCut Pruning**: Cuts off search branches early when a capture/promotion easily meets the beta threshold in a shallow verification search.

### 3. Advanced Move Ordering & Pruning Refinements 🗂️
*   **Countermove & Follow-up History**: Tracks quiet move quality based on the opponent's previous move (countermove) and our own move two plies ago (follow-up) using flat 1D boxed slices for maximum L1/L2 cache efficiency and zero stack overflow risk.
*   **History-Based LMR**: Scales Late Move Reductions dynamically using the compound history, countermove, and follow-up history scores, focusing depth on historically successful lines.
*   **Threat & Relative History**: normalizes and bounds history scores in `[-16384, 16384]` using Stockfish-style gravity.

### 4. Dynamic Time & Draw Management ⏱️
*   **Dynamic Search Time Management**: Scales the time limit by $1.5\times$ (up to $4\times$) when the root best move is unstable, and exits search early once at least 30% of the limit is spent if the move remains stable for $\ge 4$ iterations at `depth >= 8`.
*   **Dynamic Contempt Scaling**: Contempt scales dynamically based on the remaining piece count—rising to `+45 cp` in crowded middlegames to actively avoid draws, and dropping in dry endgames to play objectively.
*   **Aspiration Windows**: Scale dynamically based on depth and use exponential widening steps on search failures.

---

## Configurable UCI Options
*   `BookPath` (string, default: `ph-gambitbook.bin`): Path to a Polyglot opening book file. Set this to any non-existent path (e.g., `none.bin`) to disable Polyglot lookup and use only the built-in curated gambits.
*   `Hash` (spin, default: `16`, min: `1`, max: `1024`): Resizes the search transposition table in Megabytes.

---

## Building from Source
1. Install [Rust](https://www.rust-lang.org/).
2. Clone the repository and build:
   ```bash
   cargo build --release
   ```
3. Find the binary at `target/release/aggro_chess.exe`.
