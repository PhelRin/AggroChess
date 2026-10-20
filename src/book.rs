//! AggroChess opening book.
//!
//! A curated repertoire of gambits and sharp lines, written in SAN below. Each line belongs to
//! one side ('W' or 'B'): only that side's moves are book moves, the other side's moves just
//! describe the path. Lines are merged by position, so transpositions are handled. A move can
//! carry its own weight with `{n}` (e.g. `e5{60}`); otherwise it uses the line's weight. When a
//! position is reached through several lines the highest weight for each move is kept.
//!
//! If nothing in the curated book matches, an optional Polyglot book (BookPath) is used.

use cozy_chess::{Board, Color};
use polyglot_book_rs::PolyglotBook;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;
use std::time::SystemTime;

pub struct Book {
    pub book: Option<PolyglotBook>,
}

/// (side, weight, moves in SAN)
pub(crate) static LINES: &[(char, u32, &str)] = &[
    // =====================================================================
    // WHITE
    // =====================================================================
    // first moves
    ('W', 100, "e4"),
    ('W', 35, "d4"),
    ('W', 15, "c4"),
    ('W', 8, "Nf3"),

    // ---- King's Gambit ----
    ('W', 90, "e4{100} e5 f4 exf4 Nf3 g5 h4 g4 Ne5 Nf6 Bc4 d5 exd5 Bd6 d4"),
    ('W', 90, "e4{100} e5 f4 exf4 Nf3 g5 h4 g4 Ne5 d6 Nxg4 Nf6 Nxf6+ Qxf6 Nc3"),
    ('W', 90, "e4{100} e5 f4 exf4 Nf3 d6 Bc4 h6 d4 g5 O-O"),
    ('W', 90, "e4{100} e5 f4 exf4 Nf3 Nf6 e5 Nh5"),
    ('W', 90, "e4{100} e5 f4 exf4 Nf3 d5 exd5 Nf6 Bb5+ c6 dxc6 bxc6 Bc4 Nd5 O-O"),
    ('W', 90, "e4{100} e5 f4 exf4 Nf3 Be7 Bc4 Nf6 e5 Ng4 O-O"),
    ('W', 90, "e4{100} e5 f4 exf4 Nf3 Nc6 d4 g5 h4 g4 Ng5"),
    ('W', 80, "e4{100} e5 f4 Bc5 Nf3 d6 c3 Nf6 d4 exd4 cxd4 Bb4+ Bd2 Bxd2+ Nbxd2"),
    ('W', 80, "e4{100} e5 f4 d5 exd5 exf4 Nf3 Nf6 Bb5+ c6 dxc6 Nxc6 d4"),
    ('W', 80, "e4{100} e5 f4 Nc6 Nf3 exf4 d4 g5 h4 g4 Ng5"),
    ('W', 80, "e4{100} e5 f4 d6 Nf3 exf4 d4 g5 h4 g4 Ng5"),

    // ---- Evans Gambit ----
    ('W', 70, "e4{100} e5 Nf3{60} Nc6 Bc4 Bc5 b4 Bxb4 c3 Ba5 d4 exd4 O-O Nge7 cxd4 d5 exd5 Nxd5 Ba3"),
    ('W', 70, "e4{100} e5 Nf3{60} Nc6 Bc4 Bc5 b4 Bxb4 c3 Be7 d4 Na5 Nxe5 Nxc4 Nxc4 d5 exd5 Qxd5 Ne3"),
    ('W', 70, "e4{100} e5 Nf3{60} Nc6 Bc4 Bc5 b4 Bxb4 c3 Bc5 d4 exd4 O-O d6 cxd4 Bb6 Nc3"),
    ('W', 70, "e4{100} e5 Nf3{60} Nc6 Bc4 Bc5 b4 Bb6 a4 a6 Nc3 Nf6 Nd5"),
    // ---- Two Knights: Fried Liver / vs Traxler ----
    ('W', 80, "e4{100} e5 Nf3{60} Nc6 Bc4 Nf6 Ng5 d5 exd5 Nxd5 Nxf7 Kxf7 Qf3+ Ke6 Nc3 Nb4 a3 Nxc2+ Kd1 Nxa1 Nxd5"),
    ('W', 80, "e4{100} e5 Nf3{60} Nc6 Bc4 Nf6 Ng5 d5 exd5 Na5 Bb5+ c6 dxc6 bxc6 Be2 h6 Nf3 e4 Ne5"),
    ('W', 80, "e4{100} e5 Nf3{60} Nc6 Bc4 Nf6 Ng5 d5 exd5 b5 Bf1 h6 Nxf7 Kxf7 dxc6"),
    ('W', 80, "e4{100} e5 Nf3{60} Nc6 Bc4 Nf6 Ng5 Bc5 Bxf7+ Ke7 Bd5 Rf8 O-O d6 c3"),
    ('W', 80, "e4{100} e5 Nf3{60} Nc6 Bc4 Nf6 Ng5 d5 exd5 Nd4 c3 b5 Bf1 Nxd5 cxd4"),
    ('W', 70, "e4{100} e5 Nf3{60} Nc6 Bc4 Be7 d4 d6 Nc3 Nf6 h3 O-O O-O"),
    ('W', 70, "e4{100} e5 Nf3{60} Nc6 Bc4 d6 c3 Nf6 d4 Bg4 Qb3"),
    // ---- Halloween Gambit: objectively dubious (a knight for a pawn), but Black has to know it ----
    ('W', 30, "e4{100} e5 Nf3{60} Nc6 Nc3{30} Nf6 Nxe5 Nxe5 d4 Ng6 e5 Ng8 Bc4"),
    ('W', 30, "e4{100} e5 Nf3{60} Nc6 Nc3{30} Nf6 Nxe5 Nxe5 d4 Nc6 e5"),
    // ---- Scotch Gambit ----
    ('W', 60, "e4{100} e5 Nf3{60} Nc6 d4{40} exd4 Bc4 Nf6 e5 d5 Bb5 Ne4 Nxd4 Bd7 Bxc6 bxc6 O-O"),
    ('W', 60, "e4{100} e5 Nf3{60} Nc6 d4{40} exd4 Bc4 Bc5 c3 Nf6"),
    ('W', 60, "e4{100} e5 Nf3{60} Nc6 d4{40} exd4 Bc4 Bb4+ c3 dxc3 O-O cxb2 Bxb2"),
    // vs other 2nd moves after 2.Nf3
    ('W', 70, "e4{100} e5 Nf3{60} Nf6 Nxe5 Nc6 Nxc6 dxc6 d3 Bc5 Be2 h5"),
    ('W', 70, "e4{100} e5 Nf3{60} Nf6 Nxe5 d6 Nf3 Nxe4 Nc3 Nxc3 dxc3 Be7 Be3 Nc6 Qd2 O-O O-O-O"),
    ('W', 70, "e4{100} e5 Nf3{60} d6 d4 exd4 Nxd4 Nf6 Nc3 Be7 Bf4 O-O Qd2 c6 O-O-O"),
    ('W', 70, "e4{100} e5 Nf3{60} d6 d4 Nf6 Nc3 Nbd7 Bc4 Be7 O-O O-O Re1"),
    ('W', 70, "e4{100} e5 Nf3{60} f5 Nxe5 Qf6 d4 d6 Nc4 fxe4 Nc3 Qg6 f3"),
    ('W', 70, "e4{100} e5 Nf3{60} Qf6 Bc4 Qg6 O-O Qxe4 Bxf7+"),
    // ---- Danish Gambit ----
    ('W', 50, "e4{100} e5 d4{40} exd4 c3 dxc3 Bc4 cxb2 Bxb2 Bb4+"),
    ('W', 50, "e4{100} e5 d4{40} exd4 c3 dxc3 Bc4 cxb2 Bxb2 d5 Bxd5 Nf6 Nc3"),
    ('W', 50, "e4{100} e5 d4{40} exd4 c3 d5 exd5 Qxd5 cxd4 Nc6 Nf3 Bg4 Be2"),
    ('W', 50, "e4{100} e5 d4{40} exd4 c3 Nf6 e5 Ne4 cxd4"),
    ('W', 50, "e4{100} e5 d4{40} exd4 c3 dxc3 Nxc3 Nc6 Bc4 d6 Nf3 Be6 Bxe6 fxe6 Qb3"),
    ('W', 50, "e4{100} e5 d4{40} d6 Nf3 exd4 Nxd4 Nf6 Nc3 Be7 Bc4 O-O O-O"),
    ('W', 50, "e4{100} e5 d4{40} Nc6 d5 Nce7 Nf3 Ng6 h4"),
    // ---- Vienna Gambit ----
    ('W', 40, "e4{100} e5 Nc3{40} Nf6 f4 d5 fxe5 Nxe4 Nf3 Be7 d4 O-O Bd3 f5 exf6 Bxf6 O-O"),
    ('W', 40, "e4{100} e5 Nc3{40} Nf6 f4 exf4 e5 Ng8 Nf3 d6 d4"),
    ('W', 40, "e4{100} e5 Nc3{40} Nc6 f4 exf4 Nf3 g5 h4 g4 Ng5 h6 Nxf7 Kxf7 d4"),
    ('W', 40, "e4{100} e5 Nc3{40} Bc5 Nf3 d6 Na4"),

    // ---- vs Sicilian: Smith-Morra Gambit ----
    ('W', 100, "e4{100} c5 d4 cxd4 c3 dxc3 Nxc3 Nc6 Nf3 d6 Bc4 e6 O-O Nf6 Qe2 Be7 Rd1 e5 h3"),
    ('W', 100, "e4{100} c5 d4 cxd4 c3 dxc3 Nxc3 e6 Nf3 a6 Bc4 b5 Bb3 Bb7 O-O"),
    ('W', 100, "e4{100} c5 d4 cxd4 c3 dxc3 Nxc3 d6 Bc4 Nf6 e5 dxe5 Bxf7+ Kxf7 Qxd8"),
    ('W', 100, "e4{100} c5 d4 cxd4 c3 dxc3 Nxc3 Nc6 Nf3 e6 Bc4 Qc7 O-O Nf6 Qe2"),
    ('W', 100, "e4{100} c5 d4 cxd4 c3 Nf6 e5 Nd5 Nf3 Nc6 cxd4 d6 Bc4 Nb6 Bb5"),
    ('W', 100, "e4{100} c5 d4 cxd4 c3 d5 exd5 Qxd5 cxd4 Nc6 Nf3 Bg4 Nc3 Qa5 Be2"),
    ('W', 100, "e4{100} c5 d4 cxd4 c3 d3 Bxd3 Nc6 Nf3 d6 O-O"),
    ('W', 100, "e4{100} c5 d4 cxd4 c3 e5 Nf3 Nc6 Bc4"),
    ('W', 100, "e4{100} c5 d4 e6 Nf3 cxd4 Nxd4 Nc6 Nc3 Qc7 Be3"),
    ('W', 100, "e4{100} c5 d4 d6 dxc5 Qa5+ Nc3 Qxc5 Be3 Qa5 Nf3"),

    // ---- vs Scandinavian (incl. accepting the Blackburne-Kloosterboer gambit) ----
    ('W', 100, "e4{100} d5 exd5 Qxd5 Nc3 Qa5 d4 Nf6 Nf3 Bf5 Bc4 e6 Bd2 c6 Qe2"),
    ('W', 100, "e4{100} d5 exd5 Qxd5 Nc3 Qd6 d4 Nf6 Nf3 a6 g3"),
    ('W', 100, "e4{100} d5 exd5 Qxd5 Nc3 Qd8 d4 Nf6 Nf3 Bg4 h3"),
    ('W', 100, "e4{100} d5 exd5 Nf6 d4 Nxd5 c4 Nb6 Nf3 g6 h3"),
    ('W', 100, "e4{100} d5 exd5 c6 dxc6 Nxc6 Nf3 e5 Bb5 Bd6 O-O"),
    ('W', 100, "e4{100} d5 exd5 c6 dxc6 Nf6 cxb7 Bxb7 Nf3 e6 d4 Bd6 Bd3"),
    // ---- vs French: Milner-Barry Gambit / Alekhine-Chatard ----
    ('W', 100, "e4{100} e6 d4 d5 e5 c5 c3 Nc6 Nf3 Qb6 Bd3 cxd4 cxd4 Bd7 O-O Nxd4 Nxd4 Qxd4 Nc3"),
    ('W', 100, "e4{100} e6 d4 d5 e5 c5 c3 Nc6 Nf3 Bd7 Be2 Nge7 O-O"),
    ('W', 100, "e4{100} e6 d4 d5 e5 c5 c3 Qb6 Nf3 Bd7 Be2"),
    ('W', 100, "e4{100} e6 d4 d5 Nc3{40} Nf6 Bg5 Be7 e5 Nfd7 h4 Bxg5 hxg5 Qxg5 Nh3"),
    ('W', 100, "e4{100} e6 d4 d5 Nc3{40} Bb4 e5 c5 a3 Bxc3+ bxc3 Ne7 Qg4"),
    ('W', 100, "e4{100} e6 d4 d5 Nc3{40} dxe4 Nxe4 Nd7 Nf3 Ngf6 Nxf6+ Nxf6 Bg5"),
    ('W', 100, "e4{100} e6 d4 d5 e5 Ne7 Nf3 b6 c3"),
    // ---- vs Caro-Kann ----
    ('W', 100, "e4{100} c6 d4 d5 e5 Bf5 Nc3 e6 g4 Bg6 Nge2 c5 h4 h5 Nf4"),
    ('W', 100, "e4{100} c6 d4 d5 e5 c5 dxc5 Nc6 Nf3 Bg4 Bb5"),
    ('W', 100, "e4{100} c6 d4 d5 e5 Bf5 Nc3 h5 Bd3 Bxd3 Qxd3 e6 Nf3"),
    ('W', 100, "e4{100} c6 d4 d5 Nc3{40} dxe4 Nxe4 Bf5 Ng3 Bg6 h4 h6 Nf3 Nd7 h5"),
    // ---- vs Pirc / Modern / Alekhine / others ----
    ('W', 100, "e4{100} d6 d4 Nf6 Nc3 g6 f4 Bg7 Nf3 O-O e5"),
    ('W', 100, "e4{100} d6 d4 Nf6 Nc3 e5 Nf3 Nbd7 Bc4 Be7 O-O"),
    ('W', 100, "e4{100} g6 d4 Bg7 Nc3 d6 f4 Nf6 Nf3 O-O e5"),
    ('W', 100, "e4{100} g6 d4 Bg7 Nc3 c6 f4 d5 e5"),
    ('W', 100, "e4{100} Nf6 e5 Nd5 d4 d6 c4 Nb6 f4 dxe5 fxe5 Nc6 Be3 Bf5 Nc3 e6 Nf3"),
    ('W', 100, "e4{100} Nf6 e5 Nd5 d4 d6 c4 Nb6 f4 g6 Nc3 Bg7 Be3"),
    ('W', 100, "e4{100} Nc6 d4 d5 Nc3 dxe4 d5 Ne5 Bf4 Ng6 Bg3"),
    ('W', 100, "e4{100} Nc6 d4 e5 d5 Nce7 Nf3"),
    ('W', 100, "e4{100} b6 d4 Bb7 Bd3 e6 Nf3 c5 c3"),
    ('W', 100, "e4{100} a6 d4 b5 Bd3 Bb7 Nf3"),

    // ---- 1.d4: Blackmar-Diemer Gambit, aggressive lines vs Indians, Staunton Gambit ----
    ('W', 100, "d4{35} d5 e4 dxe4 Nc3 Nf6 f3 exf3 Nxf3 Bg4 h3 Bxf3 Qxf3 c6 Be3 e6 Bd3 Nbd7 O-O"),
    ('W', 100, "d4{35} d5 e4 dxe4 Nc3 Nf6 f3 exf3 Nxf3 e6 Bg5 Be7 Bd3 Nbd7 O-O"),
    ('W', 100, "d4{35} d5 e4 dxe4 Nc3 Nf6 f3 exf3 Nxf3 g6 Bc4 Bg7 O-O O-O Qe1"),
    ('W', 100, "d4{35} d5 e4 dxe4 Nc3 Nf6 f3 exf3 Nxf3 Bf5 Ne5 e6 g4 Bg6 h4"),
    ('W', 100, "d4{35} d5 e4 dxe4 Nc3 Nf6 f3 e5"),
    ('W', 100, "d4{35} d5 e4 dxe4 Nc3 Nf6 f3 Bf5 fxe4 Nxe4 Qf3 Nd6 Bf4"),
    ('W', 100, "d4{35} d5 e4 dxe4 Nc3 Nf6 f3 e3 Bxe3 e6 Bd3"),
    ('W', 100, "d4{35} d5 e4 dxe4 Nc3 Bf5 f3 exf3 Nxf3 e6 Bd3"),
    ('W', 100, "d4{35} d5 e4 dxe4 Nc3 e5 Nge2"),
    ('W', 100, "d4{35} d5 e4 e6 Nc3 Nf6 Bg5 Be7 e5 Nfd7 h4"),
    ('W', 100, "d4{35} d5 e4 c6 Nc3 dxe4 Nxe4 Bf5 Ng3 Bg6 h4 h6 Nf3"),
    ('W', 100, "d4{35} Nf6 c4 e6 Nc3 Bb4 f3 d5 a3 Bxc3+ bxc3 c5 cxd5 Nxd5"),
    ('W', 100, "d4{35} Nf6 c4 e6 Nc3 d5 Bg5 Be7 e3 O-O Nf3"),
    ('W', 100, "d4{35} Nf6 c4 e6 Nc3 c5 d5 exd5 cxd5 d6 e4 g6 f4 Bg7 Bb5+"),
    ('W', 100, "d4{35} Nf6 c4 g6 Nc3 Bg7 e4 d6 f3 O-O Be3 e5 d5 Nh5 Qd2"),
    ('W', 100, "d4{35} Nf6 c4 g6 Nc3 d5 cxd5 Nxd5 e4 Nxc3 bxc3 Bg7 Bc4 c5 Ne2"),
    ('W', 100, "d4{35} Nf6 c4 c5 d5 b5 cxb5 a6 bxa6 Bxa6 Nc3 d6 e4 Bxf1 Kxf1 g6 g3"),
    ('W', 100, "d4{35} Nf6 c4 c5 d5 e6 Nc3 exd5 cxd5 d6 e4 g6 f4"),
    ('W', 100, "d4{35} Nf6 c4 e5 dxe5 Ng4 Bf4 Nc6 Nf3 Bb4+ Nbd2 Qe7 e3 Ngxe5 Nxe5 Nxe5 Be2"),
    ('W', 100, "d4{35} Nf6 c4 d6 Nc3 e5 Nf3 Nbd7 e4"),
    ('W', 100, "d4{35} Nf6 c4 b6 Nc3 Bb7 Qc2"),
    ('W', 100, "d4{35} f5 e4 fxe4 Nc3 Nf6 Bg5 e6 Nxe4 Be7 Bxf6 Bxf6 Nf3"),
    ('W', 100, "d4{35} f5 e4 fxe4 Nc3 Nf6 Bg5 g6 f3 exf3 Nxf3"),
    ('W', 100, "d4{35} f5 e4 fxe4 Nc3 Nf6 Bg5 c6 f3"),
    ('W', 100, "d4{35} e6 e4 d5 Nc3 Nf6 Bg5"),
    ('W', 100, "d4{35} d6 e4 Nf6 Nc3 g6 f4 Bg7 Nf3"),
    ('W', 100, "d4{35} g6 e4 Bg7 Nc3 d6 f4"),
    ('W', 100, "d4{35} c5 d5 e5 e4 d6 Nc3 Be7 Bb5+"),
    ('W', 100, "d4{35} c5 d5 Nf6 Nc3 d6 e4 g6 f4"),
    ('W', 100, "d4{35} e5 dxe5 Nc6 Nf3 Qe7 Bf4 Qb4+ Bd2 Qxb2 Nc3"),
    ('W', 100, "d4{35} d5 c4{30} e5 dxe5 d4 Nf3 Nc6 g3 Be6 Nbd2"),
    ('W', 100, "d4{35} d5 c4{30} dxc4 e4 e5 Nf3 exd4 Bxc4"),
    ('W', 100, "d4{35} d5 c4{30} c6 Nc3 Nf6 e4"),
    ('W', 100, "d4{35} d5 c4{30} e6 Nc3 Nf6 Bg5 Be7 e3"),

    // ---- 1.c4: sharp English lines ----
    ('W', 100, "c4{15} e5 Nc3 Nf6 Nf3 Nc6 d4 exd4 Nxd4 Bb4 Bg5 h6 Bh4 Bxc3+ bxc3 Ne5 e3"),
    ('W', 100, "c4{15} e5 Nc3 Nf6 Nf3 Nc6 d4 e4 Ng5 h6 Ngxe4 Nxe4 Nxe4 Qh4 Nc3"),
    ('W', 100, "c4{15} e5 Nc3 Nc6 Nf3 Nf6 d4"),
    ('W', 100, "c4{15} e5 Nc3 Bb4 Nd5 Be7 d4 d6 e4"),
    ('W', 100, "c4{15} e5 Nc3 d6 Nf3 f5 d4 e4 Ng5 Be7 Nh3"),
    ('W', 100, "c4{15} Nf6 Nc3 e6 e4 d5 e5 d4 exf6 dxc3 bxc3 Qxf6 d4 c5 Nf3"),
    ('W', 100, "c4{15} Nf6 Nc3 e6 e4 c5 e5 Ng8 Nf3 Nc6 d4 cxd4 Nxd4"),
    ('W', 100, "c4{15} Nf6 Nc3 e6 e4 Bb4 e5 Ng8 Qg4"),
    ('W', 100, "c4{15} Nf6 Nc3 g6 e4 d6 d4 Bg7 f3 O-O Be3"),
    ('W', 100, "c4{15} Nf6 Nc3 c5 Nf3 Nc6 d4 cxd4 Nxd4 e6 g3"),
    ('W', 100, "c4{15} Nf6 Nc3 e5 Nf3 Nc6 d4"),
    ('W', 100, "c4{15} Nf6 Nc3 d5 cxd5 Nxd5 e4 Nxc3 bxc3 e5 Bc4"),
    ('W', 100, "c4{15} c5 Nc3 Nc6 g3 g6 Bg2 Bg7 e4 e6 Nge2 Nge7 d3 O-O O-O d6 Be3"),
    ('W', 100, "c4{15} c5 Nc3 Nf6 Nf3 d5 cxd5 Nxd5 e4 Nb4 Bc4"),
    ('W', 100, "c4{15} e6 Nc3 d5 d4 Nf6 Bg5 Be7 e3 O-O Nf3"),
    ('W', 100, "c4{15} c6 e4 d5 exd5 cxd5 cxd5 Nf6 Bb5+ Nbd7 Nc3 g6 d4"),
    ('W', 100, "c4{15} c6 e4 e5 Nf3 d6 d4 Nd7 Nc3"),
    ('W', 100, "c4{15} g6 e4 Bg7 d4 d6 Nc3 Nf6 f3"),
    ('W', 100, "c4{15} f5 Nc3 Nf6 g3 e6 Bg2 d5 d3"),
    ('W', 100, "c4{15} d5 cxd5 Qxd5 Nc3 Qa5 d4 Nf6 Nf3 c6 e4"),
    ('W', 100, "c4{15} d5 cxd5 Nf6 e4 Nxe4 Qa4+"),
    ('W', 100, "c4{15} b6 d4 e6 e4 Bb7 Bd3"),

    // ---- 1.Nf3: Tennison Gambit and friends ----
    ('W', 100, "Nf3{8} d5 e4 dxe4 Ng5 Nf6 Nc3 Bf5 Qe2"),
    ('W', 100, "Nf3{8} d5 e4 dxe4 Ng5 e5 d3 exd3 Bxd3 Nf6 O-O"),
    ('W', 100, "Nf3{8} d5 e4 d4 Bc4 c5 O-O Nc6 c3"),
    ('W', 100, "Nf3{8} Nf6 c4 g6 Nc3 Bg7 e4 d6 d4 O-O Be2"),
    ('W', 100, "Nf3{8} Nf6 c4 e6 Nc3 d5 d4 Be7 Bg5"),
    ('W', 100, "Nf3{8} c5 e4 d6 d4 cxd4 Nxd4 Nf6 Nc3 a6 Bg5"),
    ('W', 100, "Nf3{8} c5 e4 Nc6 d4 cxd4 Nxd4 Nf6 Nc3 e5 Ndb5"),
    ('W', 100, "Nf3{8} e6 e4 d5 Nc3 Nf6 e5"),
    ('W', 100, "Nf3{8} f5 e4 fxe4 Ng5 Nf6 d3 exd3 Bxd3"),
    ('W', 100, "Nf3{8} d6 e4 Nf6 Nc3 g6 d4"),
    ('W', 100, "Nf3{8} g6 e4 Bg7 d4"),

    // =====================================================================
    // BLACK
    // =====================================================================
    // ---- vs 1.e4: the three-pawn Scandinavian gambit (Blackburne-Kloosterboer) ----
    ('B', 100, "e4 d5 exd5 c6 dxc6 Nf6 cxb7 Bxb7 Nf3 Nbd7 Be2 e5 d3 Bd6 O-O O-O"),
    ('B', 100, "e4 d5 exd5 c6 dxc6 Nf6 cxb7 Bxb7 d4 e6 Nf3 Bd6 Bd3 Nbd7 O-O O-O"),
    ('B', 100, "e4 d5 exd5 c6 dxc6 Nf6 cxb7 Bxb7 Nc3 e5 d3 Bb4 Bd2 O-O"),
    ('B', 100, "e4 d5 exd5 c6 dxc6 Nf6 cxb7 Bxb7 Bb5+ Nbd7 Nf3 e6 O-O Bd6"),
    ('B', 100, "e4 d5 exd5 c6 dxc6 Nf6 cxb7 Bxb7 c4 e5 d3 Bc5 Nc3 O-O"),
    ('B', 100, "e4 d5 exd5 c6 dxc6 Nf6 Nc3 Nxc6 Nf3 e5 Bb5 Bd6"),
    ('B', 100, "e4 d5 exd5 c6 dxc6 Nf6 Nf3 Nxc6 d4 Bg4 Be2 e6"),
    ('B', 100, "e4 d5 exd5 c6 dxc6 Nf6 d3 Nxc6 Nf3 e5 Be2 Bc5"),
    ('B', 100, "e4 d5 exd5 c6 dxc6 Nf6 Be2 Nxc6 Nf3 e5 d3 Bc5"),
    ('B', 100, "e4 d5 exd5 c6 dxc6 Nf6 c3 Nxc6 Nf3 e5 d3 Bc5"),
    ('B', 100, "e4 d5 exd5 c6 c4 cxd5 cxd5 Nf6 Nc3 Nxd5 Nf3 Nc6 d4"),
    ('B', 100, "e4 d5 exd5 c6 Nc3 cxd5 d4 Nc6 Nf3 Bg4 Be2 e6"),
    ('B', 100, "e4 d5 exd5 c6 d4 cxd5 c4 Nf6 Nc3 Nc6 Nf3 Bg4"),
    ('B', 100, "e4 d5 exd5 c6 Nf3 cxd5 d4 Nc6 Bb5 Bg4"),
    ('B', 100, "e4 d5 exd5 c6 Bc4 Nf6 Nc3 b5 Bb3 b4 Na4"),
    ('B', 100, "e4 d5 e5 Bf5 d4 e6 c3 c5 Nf3 Nc6 Be2 Qb6"),
    ('B', 100, "e4 d5 Nc3 d4 Nce2 e5 Ng3 Be6 c3 c5"),
    ('B', 100, "e4 d5 Nc3 dxe4 Nxe4 Nd7 Nf3 Ngf6 Nxf6+ Nxf6"),
    ('B', 100, "e4 d5 d3 e5 Nf3 Nc6 Be2 Bd6"),
    ('B', 100, "e4 d5 Nf3 dxe4 Ng5 Nf6 Nc3 Bf5"),
    // ---- vs 1.e4: 1...e5 gambits (Stafford, Traxler, Schliemann, fishing pole, Albin-style) ----
    ('B', 60, "e4 e5 Nf3 Nf6{70} Nxe5 Nc6 Nxc6 dxc6 d3 Bc5 Be2 h5 c3 Ng4 d4 Qh4 g3 Qf6"),
    ('B', 60, "e4 e5 Nf3 Nf6{70} Nxe5 Nc6 Nxc6 dxc6 d3 Bc5 Be2 h5 Nc3 Ng4 O-O Qd4"),
    ('B', 60, "e4 e5 Nf3 Nf6{70} Nxe5 Nc6 Nxc6 dxc6 e5 Ne4 d3 Bc5 dxe4 Bxf2+ Ke2 Bg4+"),
    ('B', 60, "e4 e5 Nf3 Nf6{70} Nxe5 Nc6 Nxc6 dxc6 Nc3 Bc5 Be2 h5 d3 Ng4"),
    ('B', 60, "e4 e5 Nf3 Nf6{70} Nxe5 Nc6 Nxc6 dxc6 d4 Nxe4"),
    ('B', 60, "e4 e5 Nf3 Nf6{70} Nxe5 Nc6 Nf3 Nxe4 Qe2 Qe7 Qxe4 Qxe4+"),
    ('B', 60, "e4 e5 Nf3 Nf6{70} Nxe5 Nc6 d4 Nxe5 dxe5 Nxe4 Bd3 Nc5"),
    ('B', 60, "e4 e5 Nf3 Nf6{70} Nxe5 Nc6 Nc4 Nxe4 Nc3 Nxc3 dxc3"),
    ('B', 60, "e4 e5 Nf3 Nf6{70} Nc3 Nc6 Bb5 Nd4 Nxd4 exd4 e5 dxc3 exf6 Qxf6"),
    ('B', 60, "e4 e5 Nf3 Nf6{70} Nc3 Nc6 d4 exd4 Nxd4 Bb4 Nxc6 bxc6 Bd3 d5"),
    ('B', 60, "e4 e5 Nf3 Nf6{70} Nc3 Nc6 Bc4 Nxe4 Nxe4 d5"),
    ('B', 60, "e4 e5 Nf3 Nf6{70} d4 Nxe4 Bd3 d5 Nxe5 Nd7 Nxd7 Bxd7 O-O Bd6"),
    ('B', 60, "e4 e5 Nf3 Nf6{70} Bc4 Nxe4 Nc3 Nxc3 dxc3 f6 O-O Qe7"),
    ('B', 60, "e4 e5 Nf3 Nf6{70} d3 Nc6 Be2 d5 Nbd2 Bc5"),
    ('B', 60, "e4 e5 Nf3 Nc6{50} Bc4 Nf6 Ng5 Bc5 Nxf7 Bxf2+ Kxf2 Nxe4+"),
    ('B', 60, "e4 e5 Nf3 Nc6{50} Bc4 Nf6 Ng5 Bc5 Bxf7+ Ke7 Bd5 Rf8 O-O d6"),
    ('B', 60, "e4 e5 Nf3 Nc6{50} Bc4 Nf6 d3 Bc5 c3 d6 O-O O-O"),
    ('B', 60, "e4 e5 Nf3 Nc6{50} Bc4 Nf6 d4 exd4 e5 d5 Bb5 Ne4 Nxd4 Bc5"),
    ('B', 60, "e4 e5 Nf3 Nc6{50} Bc4 Nf6 Nc3 Nxe4 Nxe4 d5 Bd3 dxe4 Bxe4 Bd6"),
    ('B', 60, "e4 e5 Nf3 Nc6{50} Bc4 Nf6 O-O Bc5 d3 d6 c3 O-O"),
    ('B', 60, "e4 e5 Nf3 Nc6{50} Bb5 f5 Nc3 fxe4 Nxe4 d5 Nxe5 dxe4 Nxc6 Qg5"),
    ('B', 60, "e4 e5 Nf3 Nc6{50} Bb5 f5 d3 fxe4 dxe4 Nf6 O-O Bc5"),
    ('B', 60, "e4 e5 Nf3 Nc6{50} Bb5 f5 d4 fxe4 Nxe5 Nxe5 dxe5 c6"),
    ('B', 60, "e4 e5 Nf3 Nc6{50} Bb5 f5 exf5 e4 Qe2 Qe7 Bxc6 dxc6"),
    ('B', 60, "e4 e5 Nf3 Nc6{50} Bb5 f5 Bxc6 dxc6"),
    ('B', 60, "e4 e5 Nf3 Nc6{50} d4 exd4 Nxd4 Nf6 Nxc6 bxc6 e5 Qe7 Qe2 Nd5 c4 Ba6"),
    ('B', 60, "e4 e5 Nf3 Nc6{50} d4 exd4 Bc4 Bc5 c3 Nf6"),
    ('B', 60, "e4 e5 Nf3 Nc6{50} d4 exd4 c3 d5 exd5 Qxd5 cxd4 Bg4"),
    ('B', 60, "e4 e5 Nf3 Nc6{50} Nc3 Nf6 Bb5 Nd4"),
    ('B', 60, "e4 e5 Nf3 Nc6{50} c3 d5 Qa4 f6"),
    ('B', 60, "e4 e5 f4 exf4 Nf3 g5 h4 g4 Ne5 Nf6 Bc4 d5 exd5 Bd6"),
    ('B', 60, "e4 e5 f4 exf4 Nf3 g5 Bc4 g4 O-O gxf3 Qxf3 Qf6"),
        ('B', 60, "e4 e5 f4 exf4 Nf3 g5 d4 g4 Ne5 Qh4+ g3 fxg3"),
    ('B', 60, "e4 e5 f4 exf4 Bc4 d5 Bxd5 Nf6 Nc3 Bb4"),
    ('B', 60, "e4 e5 f4 exf4 Nc3 Qh4+ Ke2 d5 Nxd5 Bg4+"),
    ('B', 60, "e4 e5 d4 exd4 c3 d5 exd5 Qxd5 cxd4 Nc6 Nf3 Bg4 Be2 Bb4+"),
    ('B', 60, "e4 e5 d4 exd4 Qxd4 Nc6 Qe3 Nf6 Nc3 Bb4 Bd2 O-O O-O-O Re8"),
    ('B', 60, "e4 e5 d4 exd4 Nf3 Nc6 Bc4 Bc5 c3 Nf6"),
    ('B', 60, "e4 e5 Nc3 Nf6 f4 d5 fxe5 Nxe4 Nf3 Be7 d4 O-O"),
    ('B', 60, "e4 e5 Nc3 Nf6 Bc4 Nxe4 Qh5 Nd6 Bb3 Be7 Nf3 Nc6"),
    ('B', 60, "e4 e5 Nc3 Nf6 Nf3 Nc6 Bb5 Nd4"),
    ('B', 60, "e4 e5 Nc3 Nf6 g3 d5 exd5 Nxd5 Bg2 Nxc3 bxc3 Bd6"),
    ('B', 60, "e4 e5 Bc4 Nf6 d3 c6 Nf3 d5 Bb3 Bd6"),
    ('B', 60, "e4 e5 Bc4 Nf6 d4 exd4 Nf3 Nc6 e5 d5 Bb5 Ne4 Nxd4 Bc5"),
    ('B', 60, "e4 e5 Bc4 Nf6 d4 exd4 Nf3 Nc6 O-O Nxe4 Re1 d5 Bxd5 Qxd5 Nc3 Qa5 Nxe4 Be6"),
    ('B', 60, "e4 e5 Qh5 Nc6 Bc4 g6 Qf3 Nf6"),
    // ---- vs 1.e4: Sicilian Dragon ----
    // ---- Latvian Gambit: 2...f5!? (dubious, full of traps) ----
    ('B', 25, "e4 e5 Nf3 f5{25} Nxe5 Qf6 d4 d6 Nc4 fxe4 Nc3 Qg6"),
    ('B', 25, "e4 e5 Nf3 f5{25} Nxe5 Qf6 Nc4 fxe4 Nc3 Qf7"),
    ('B', 25, "e4 e5 Nf3 f5{25} Bc4 fxe4 Nxe5 Qg5 d4 Qxg2"),
    ('B', 25, "e4 e5 Nf3 f5{25} exf5 e4 Ne5 Nf6"),
    ('B', 25, "e4 e5 Nf3 f5{25} d4 fxe4 Nxe5 Nf6"),
    ('B', 25, "e4 e5 Nf3 f5{25} Nc3 Nf6"),
    ('B', 40, "e4 c5 Nf3 d6 d4 cxd4 Nxd4 Nf6 Nc3 g6 Be3 Bg7 f3 O-O Qd2 Nc6 Bc4 Bd7 O-O-O Rc8"),
    ('B', 40, "e4 c5 Nf3 d6 d4 cxd4 Nxd4 Nf6 Nc3 g6 Be2 Bg7 O-O O-O Be3 Nc6"),
    ('B', 40, "e4 c5 Nf3 d6 d4 cxd4 Nxd4 Nf6 Nc3 g6 f4 Nbd7"),
    ('B', 40, "e4 c5 Nf3 d6 Bb5+ Bd7 Bxd7+ Qxd7 O-O Nc6 c3 Nf6"),
    ('B', 40, "e4 c5 c3 d5 exd5 Qxd5 d4 Nf6 Nf3 Bg4"),
    ('B', 40, "e4 c5 Nc3 Nc6 g3 g6 Bg2 Bg7 d3 d6"),
    ('B', 40, "e4 c5 d4 cxd4 c3 Nf6 e5 Nd5 cxd4 d6 Nf3 Nc6"),
    ('B', 40, "e4 c5 f4 d5 exd5 Nf6 Bb5+ Bd7 Bxd7+ Qxd7"),

    // ---- vs 1.d4: Albin Counter-Gambit, Benko, Budapest ----
    // ---- Englund Gambit: 1.d4 e5?! (dubious; 7.Qd2?? Bxc3 8.Qxc3 Qc1# is the famous trap) ----
    ('B', 25, "d4 e5{25} dxe5 Nc6 Nf3 Qe7 Bf4 Qb4+ Bd2 Qxb2 Bc3 Bb4 Qd2 Bxc3 Qxc3 Qc1#"),
    ('B', 25, "d4 e5{25} dxe5 Nc6 Nf3 Qe7 Qd5 f6 exf6 Nxf6 Qb3 d5"),
    ('B', 25, "d4 e5{25} dxe5 Nc6 Nf3 Qe7"),
    ('B', 25, "d4 e5{25} dxe5 Nc6 Nf3 Qe7 Bg5 f6 exf6 Nxf6"),
    ('B', 25, "d4 e5{25} dxe5 Nc6 Bf4 Qe7 Nf3 Qb4+ Bd2 Qxb2 Bc3 Bb4"),
    ('B', 25, "d4 e5{25} e4 exd4 Qxd4 Nc6"),
    ('B', 25, "d4 e5{25} d5 Bc5"),
    ('B', 25, "d4 e5{25} Nf3 e4 Ne5 d6 Nc4 d5 Ne3 f5"),
    ('B', 70, "d4 d5 c4 e5 dxe5 d4 Nf3 Nc6 g3 Be6 Nbd2 Qd7 Bg2 O-O-O"),
    ('B', 70, "d4 d5 c4 e5 dxe5 d4 Nf3 Nc6 a3 Be6 Nbd2 Qd7"),
    ('B', 70, "d4 d5 c4 e5 dxe5 d4 e3 Bb4+ Bd2 dxe3 Bxb4 exf2+ Ke2 fxg1=N+"),
    ('B', 70, "d4 d5 c4 e5 dxe5 d4 e4 Nc6 f4 g5"),
    ('B', 70, "d4 d5 c4 e5 e3 exd4 exd4 Nf6 Nc3 Be7 Nf3 O-O"),
    ('B', 70, "d4 d5 c4 e5 Nc3 exd4 Qxd4 Nc6"),
    ('B', 70, "d4 d5 c4 e5 cxd5 Qxd5 e3 exd4 Nc3 Qd8 exd4 Nf6"),
    ('B', 70, "d4 d5 c4 e5 Nf3 e4 Ne5 Nf6 Nc3"),
    ('B', 70, "d4 d5 Nf3 Nf6 c4 c6 Nc3 dxc4 a4 Bf5"),
    ('B', 70, "d4 d5 Nf3 Nf6 Bf4 c5 e3 Nc6 c3 Bg4 Nbd2 e6"),
    ('B', 70, "d4 d5 Bf4 c5 e3 Nc6 c3 Bf5 Nd2 e6 Ngf3 Nf6"),
    ('B', 70, "d4 d5 e4 dxe4 Nc3 Nf6 f3 exf3 Nxf3 Bg4 h3 Bxf3 Qxf3 c6"),
    ('B', 70, "d4 d5 Bg5 h6 Bh4 c6 e3 Qb6 b3 Bf5"),
    ('B', 70, "d4 d5 Nc3 Nf6 Bg5 Nbd7 Nf3 h6 Bh4 e6"),
    ('B', 70, "d4 d5 e3 Nf6 Bd3 c5 c3 Nc6 f4 Bg4"),
    ('B', 60, "d4 Nf6{60} c4 c5 d5 b5 cxb5 a6 bxa6 Bxa6 Nc3 d6 e4 Bxf1 Kxf1 g6 g3 Bg7 Kg2 O-O Nf3 Nbd7"),
    ('B', 60, "d4 Nf6{60} c4 c5 d5 b5 cxb5 a6 b6 Qxb6 Nc3 d6 e4 g6"),
    ('B', 60, "d4 Nf6{60} c4 c5 d5 b5 cxb5 a6 e3 g6 Nc3 Bg7"),
    ('B', 60, "d4 Nf6{60} c4 c5 d5 b5 Nf3 g6 cxb5 a6"),
    ('B', 60, "d4 Nf6{60} c4 c5 e3 g6 Nc3 Bg7 Nf3 O-O"),
    ('B', 60, "d4 Nf6{60} c4 c5 Nf3 cxd4 Nxd4 e5 Nb5 d5"),
    ('B', 60, "d4 Nf6{60} c4 c5 dxc5 e6 Nc3 Bxc5 Nf3 O-O"),
    ('B', 50, "d4 Nf6{60} c4 e5{40} dxe5 Ng4 Bf4 Nc6 Nf3 Bb4+ Nbd2 Qe7 e3 Ngxe5 Nxe5 Nxe5 Be2 O-O O-O d6"),
    ('B', 50, "d4 Nf6{60} c4 e5{40} dxe5 Ng4 e4 Nxe5 f4 Nec6 Be3 Bb4+ Nc3 Qh4+"),
    ('B', 50, "d4 Nf6{60} c4 e5{40} dxe5 Ng4 Nf3 Bc5 e3 Nc6 Be2 O-O O-O Re8"),
    ('B', 50, "d4 Nf6{60} c4 e5{40} dxe5 Ne4 Nf3 Nc6 a3 d6"),
    ('B', 50, "d4 Nf6{60} Nf3 g6 c4 Bg7 Nc3 O-O e4 d6 Be2 e5 O-O Nc6 d5 Ne7"),
    ('B', 50, "d4 Nf6{60} Nf3 g6 g3 Bg7 Bg2 O-O O-O d6 c4 Nc6"),
    ('B', 50, "d4 Nf6{60} Nf3 g6 Bf4 Bg7 e3 d6 h3 O-O"),
    ('B', 50, "d4 Nf6{60} Bg5 Ne4 Bf4 c5 f3 Qa5+ c3 Nf6"),
    ('B', 50, "d4 Nf6{60} Bf4 c5 e3 Qb6 Nc3 Qxb2"),
    ('B', 50, "d4 Nf6{60} Nc3 d5 Bg5 Nbd7 f3 c6"),
    ('B', 50, "d4 Nf6{60} c4 c5 d5 b5 a4 bxc4 Nc3 g6"),

    // ---- vs 1.c4 ----
    ('B', 80, "c4 e5 Nc3 Nf6 g3 d5 cxd5 Nxd5 Bg2 Nb6 Nf3 Nc6 O-O Be7 d3 O-O"),
    ('B', 80, "c4 e5 Nc3 Nf6 Nf3 Nc6 g3 d5 cxd5 Nxd5 Bg2 Nb6 O-O Be7"),
    ('B', 80, "c4 e5 Nc3 Nf6 Nf3 Nc6 e3 d5 cxd5 Nxd5 Bb5 Nxc3"),
    ('B', 80, "c4 e5 Nc3 Nf6 Nf3 Nc6 d4 e4 Ng5 h6 Ngxe4 Nxe4 Nxe4 Qh4"),
    ('B', 80, "c4 e5 Nc3 Nf6 Nf3 Nc6 a3 d5 cxd5 Nxd5 Qc2"),
    ('B', 80, "c4 e5 Nc3 Nf6 e4 Bc5 Nf3 d6 d4 exd4 Nxd4 O-O"),
    ('B', 80, "c4 e5 Nc3 Nf6 d3 d5 cxd5 Nxd5 Nf3 Nc6"),
    ('B', 80, "c4 e5 Nc3 Nf6 e3 d5 cxd5 Nxd5 Nf3 Nc6"),
    ('B', 80, "c4 e5 Nc3 Nc6 g3 f5 Bg2 Nf6 e3 g6"),
    ('B', 80, "c4 e5 g3 Nf6 Bg2 c6 d4 e4 Nc3 d5"),
    ('B', 80, "c4 e5 g3 Nf6 Bg2 d5 cxd5 Nxd5 Nc3 Nb6"),
    ('B', 80, "c4 e5 Nf3 e4 Nd4 Nc6 Nxc6 dxc6 Nc3 Nf6"),
    ('B', 80, "c4 e5 d3 Nf6 Nc3 d5 cxd5 Nxd5"),
    ('B', 80, "c4 e5 e3 Nf6 Nc3 d5 cxd5 Nxd5"),
    ('B', 80, "c4 e5 b3 Nf6 Bb2 Nc6 e3 d5"),
    ('B', 80, "c4 e5 d4 exd4 Qxd4 Nc6 Qd1 Nf6"),

    // ---- vs 1.Nf3 ----
    ('B', 80, "Nf3 d5 g3 Bg4 Bg2 Nd7 O-O c6 d3 e5 h3 Bh5"),
    ('B', 80, "Nf3 d5 c4 d4 e3 Nc6 exd4 Nxd4 Nxd4 Qxd4"),
    ('B', 80, "Nf3 d5 c4 d4 g3 c5 Bg2 Nc6 O-O e5"),
    ('B', 80, "Nf3 d5 c4 d4 b4 f6 e3 e5"),
    ('B', 80, "Nf3 d5 d4 Nf6 c4 c6 Nc3 dxc4"),
    ('B', 80, "Nf3 d5 d4 Nf6 Bf4 c5 e3 Nc6"),
    ('B', 80, "Nf3 d5 e3 Nf6 b3 Bg4 Bb2 e6"),
    ('B', 80, "Nf3 d5 b3 Bg4 Bb2 Nd7 e3 e5"),
    ('B', 80, "Nf3 d5 e4 dxe4 Ng5 Nf6 Nc3 Bf5"),
    ('B', 80, "Nf3 d5 d3 Nf6 g3 e5"),

    // ---- vs other first moves ----
    ('B', 100, "f4 e5 fxe5 d6 exd6 Bxd6 Nf3 g5 g3 g4 Nh4 Ne7 d4 Ng6"),
    ('B', 100, "f4 e5 fxe5 d6 exd6 Bxd6 Nf3 g5 d4 g4 Ne5 Bxe5 dxe5"),
    ('B', 100, "f4 e5 fxe5 d6 exd6 Bxd6 Nf3 g5 e4 g4 e5 gxf3 exd6 Qxd6"),
    ('B', 100, "f4 e5 fxe5 d6 Nf3 dxe5 e4 Bc5 Bc4 Nc6"),
    ('B', 100, "f4 e5 e4 exf4 Nf3 g5 h4 g4 Ne5 Nf6"),
    ('B', 100, "f4 e5 Nf3 exf4 d4 g5 h4 g4 Ng5"),
    ('B', 100, "f4 e5 d4 exd4 Nf3 c5"),
    ('B', 100, "f4 e5 e3 exf4 exf4 Qh4+ g3 Qe7+"),
    ('B', 100, "b3 e5 Bb2 Nc6 e3 d5 Bb5 Bd6 Nf3 Qe7"),
    ('B', 100, "b3 e5 Bb2 Nc6 c4 Nf6 e3 d5 cxd5 Nxd5"),
    ('B', 100, "g3 d5 Bg2 e5 d3 Nf6 Nf3 Nc6 O-O Be7"),
    ('B', 100, "e3 e5 d4 exd4 exd4 d5 Nf3 Nf6"),
    ('B', 100, "e3 e5 c4 Nf6 Nc3 d5"),
    ('B', 100, "Nc3 d5 e4 d4 Nce2 e5 Ng3 Be6"),
    ('B', 100, "Nc3 d5 d4 Nf6 Bg5 Nbd7"),
    ('B', 100, "d3 e5 Nf3 Nc6 e4 Nf6"),
    ('B', 100, "b4 e5 Bb2 Bxb4 Bxe5 Nf6 c4 O-O Nf3 d5"),
    ('B', 100, "b4 e5 a3 d5 Bb2 Bd6"),
    ('B', 100, "a3 e5 c4 Nf6 Nc3 d5"),
    ('B', 100, "h3 e5 e4 Nf6 Nc3 Bb4"),
    ('B', 100, "c3 e5 d4 exd4 cxd4 d5"),
    ('B', 100, "Nh3 d5 g3 e5 f4 Bxh3"),
    ('B', 100, "a4 e5 e4 Nf6 Nc3 Bc5"),
    ('B', 100, "g4 d5 Bg2 Bxg4 c4 c6"),
];

type BookMap = HashMap<u64, Vec<(String, u32)>>;
static BOOK: OnceLock<BookMap> = OnceLock::new();
/// Every move that appears anywhere in the book, for either side (used to label "book" moves
/// in game reviews, including the opponent's theory moves).
static KNOWN: OnceLock<std::collections::HashSet<(u64, String)>> = OnceLock::new();

fn strip(s: &str) -> &str {
    s.trim_end_matches(['+', '#', '!', '?'])
}

/// Finds the legal move whose SAN matches `san` (check marks ignored).
pub(crate) fn parse_san(board: &Board, san: &str) -> Option<cozy_chess::Move> {
    let want = strip(san);
    let mut found = None;
    board.generate_moves(|mvs| {
        for m in mvs {
            let s = format!("{}", cozy_chess::util::display_san_move(board, m));
            if strip(&s) == want {
                found = Some(m);
                return true;
            }
        }
        false
    });
    found
}

/// Builds the position -> moves map. Returns the map and any lines that failed to parse.
pub(crate) fn build_book() -> (BookMap, Vec<String>) {
    let mut map: BookMap = HashMap::new();
    let mut errors = Vec::new();
    for &(side, line_weight, text) in LINES {
        let mine = if side == 'W' { Color::White } else { Color::Black };
        let mut board = Board::default();
        for tok in text.split_whitespace() {
            let (san, weight) = match tok.find('{') {
                Some(i) => (&tok[..i], tok[i + 1..tok.len() - 1].parse().unwrap_or(line_weight)),
                None => (tok, line_weight),
            };
            let Some(mv) = parse_san(&board, san) else {
                errors.push(format!("{} | illegal or unknown move '{}'", text, san));
                break;
            };
            if board.side_to_move() == mine && weight > 0 {
                let uci = format!("{}", cozy_chess::util::display_uci_move(&board, mv));
                let entry = map.entry(board.hash()).or_default();
                match entry.iter_mut().find(|(u, _)| *u == uci) {
                    Some(e) => e.1 = e.1.max(weight),
                    None => entry.push((uci, weight)),
                }
            }
            board.play(mv);
        }
    }
    (map, errors)
}

fn book_map() -> &'static BookMap {
    BOOK.get_or_init(|| build_book().0)
}

fn known_moves() -> &'static std::collections::HashSet<(u64, String)> {
    KNOWN.get_or_init(|| {
        let mut set = std::collections::HashSet::new();
        for &(_, _, text) in LINES {
            let mut board = Board::default();
            for tok in text.split_whitespace() {
                let san = tok.split('{').next().unwrap_or(tok);
                let Some(mv) = parse_san(&board, san) else { break };
                set.insert((board.hash(), format!("{}", cozy_chess::util::display_uci_move(&board, mv))));
                board.play(mv);
            }
        }
        set
    })
}

/// Number of positions in the curated book.
pub fn curated_positions() -> usize {
    book_map().len()
}

fn seed() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(88172645463325252)
}

impl Book {
    pub fn new(path: &str) -> Self {
        if Path::new(path).exists() {
            match PolyglotBook::load(path) {
                Ok(book) => {
                    out!("info string Loaded opening book from {}", path);
                    Self { book: Some(book) }
                }
                Err(e) => {
                    out!("info string Failed to load opening book: {:?}", e);
                    Self { book: None }
                }
            }
        } else {
            Self { book: None }
        }
    }

    /// Is `uci` a known theory move in this position (for either side)?
    pub fn is_curated_move(&self, board: &Board, uci: &str) -> bool {
        known_moves().contains(&(board.hash(), uci.to_string()))
    }

    pub fn get_move(&self, board: &Board) -> Option<String> {
        let mut prng = SimplePrng::new();

        // 1. curated gambit repertoire
        if let Some(cands) = book_map().get(&board.hash()) {
            let legal: Vec<&(String, u32)> = cands
                .iter()
                .filter(|(u, _)| {
                    cozy_chess::util::parse_uci_move(board, u).is_ok_and(|m| board.is_legal(m))
                })
                .collect();
            let total: u32 = legal.iter().map(|c| c.1).sum();
            if total > 0 {
                let r = prng.next_range(0, total as u64) as u32;
                let mut sum = 0;
                for (u, w) in &legal {
                    sum += w;
                    if r < sum {
                        return Some(u.clone());
                    }
                }
            }
        }

        // 2. optional Polyglot book
        let book = self.book.as_ref()?;
        let fen = board.to_string();
        let entries = book.get_all_moves_from_fen(&fen);
        if entries.is_empty() {
            return None;
        }
        let total_weight: u32 = entries.iter().map(|e| e.weight as u32).sum();
        if total_weight == 0 {
            let idx = prng.next_range(0, entries.len() as u64) as usize;
            return Some(entries[idx].move_string.clone());
        }
        let r = prng.next_range(0, total_weight as u64) as u32;
        let mut sum = 0;
        for entry in &entries {
            sum += entry.weight as u32;
            if r < sum {
                return Some(entry.move_string.clone());
            }
        }
        Some(entries[0].move_string.clone())
    }
}

struct SimplePrng {
    state: u64,
}

impl SimplePrng {
    fn new() -> Self {
        let s = seed();
        Self { state: if s == 0 { 88172645463325252 } else { s } }
    }

    fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn next_range(&mut self, min: u64, max: u64) -> u64 {
        if min >= max {
            return min;
        }
        min + (self.next() % (max - min))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_book_line_is_legal() {
        let (map, errors) = build_book();
        for e in &errors {
            eprintln!("{}", e);
        }
        assert!(errors.is_empty(), "{} bad book lines", errors.len());
        assert!(map.len() > 300, "book only has {} positions", map.len());
    }
}

#[cfg(test)]
mod dump {
    use super::*;
    /// `cargo test --release dump_book -- --ignored` writes every book move to /tmp/book_dump.txt
    #[test]
    #[ignore]
    fn dump_book() {
        let mut out = String::new();
        for &(side, _, text) in LINES {
            let mine = if side == 'W' { Color::White } else { Color::Black };
            let mut board = Board::default();
            let mut path = Vec::new();
            for tok in text.split_whitespace() {
                let san = tok.split('{').next().unwrap();
                let mv = parse_san(&board, san).unwrap();
                let uci = format!("{}", cozy_chess::util::display_uci_move(&board, mv));
                if board.side_to_move() == mine {
                    out.push_str(&format!("{}\t{}\t{}\t{}\n", side, path.join(" "), uci, san));
                }
                path.push(uci);
                board.play(mv);
            }
        }
        std::fs::write("/tmp/book_dump.txt", out).unwrap();
        eprintln!("positions: {}", build_book().0.len());
    }
}

#[cfg(test)]
mod first_moves {
    use super::*;
    #[test]
    fn first_move_weights() {
        let (map, _) = build_book();
        let mut v = map.get(&Board::default().hash()).unwrap().clone();
        v.sort();
        eprintln!("{:?}", v);
        let w = |m: &str| v.iter().find(|x| x.0 == m).unwrap().1;
        assert_eq!((w("e2e4"), w("d2d4"), w("c2c4"), w("g1f3")), (100, 35, 15, 8));
    }
}

#[cfg(test)]
mod queen_trades {
    use super::*;
    use crate::{eval, search};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    fn queens(b: &Board) -> (bool, bool) {
        (
            !b.colored_pieces(Color::White, cozy_chess::Piece::Queen).is_empty(),
            !b.colored_pieces(Color::Black, cozy_chess::Piece::Queen).is_empty(),
        )
    }

    /// Plays each book line to its end, then 6 more plies (book side with its personality,
    /// the other side neutral) and reports lines where the queens come off early.
    #[test]
    #[ignore]
    fn scan_queen_trades() {
        let mut out = String::new();
        for &(side, _, text) in LINES {
            let mine = if side == 'W' { Color::White } else { Color::Black };
            let mut board = Board::default();
            let mut hist = vec![board.hash()];
            let mut sans = Vec::new();
            for tok in text.split_whitespace() {
                let san = tok.split('{').next().unwrap();
                let mv = parse_san(&board, san).unwrap();
                sans.push(san.to_string());
                board.play(mv);
                hist.push(board.hash());
            }
            let in_line = queens(&board) == (false, false);
            let mut tail = Vec::new();
            let mut gone_at = None;
            if !in_line {
                for p in 0..6 {
                    if board.status() != cozy_chess::GameStatus::Ongoing {
                        break;
                    }
                    let ours = board.side_to_move() == mine;
                    let (a, sb, c) = if ours { (220, 220, 50) } else { (100, 0, 0) };
                    eval::AGGRESSION.store(a, Ordering::Relaxed);
                    search::SACRIFICE_BONUS.store(sb, Ordering::Relaxed);
                    search::CONTEMPT.store(c, Ordering::Relaxed);
                    let limits = search::Limits { depth: Some(12), soft: Some(std::time::Duration::from_millis(250)), hard: Some(std::time::Duration::from_millis(400)), nodes: None };
                    let (m, _) = search::search_position(&board, &hist, limits, Arc::new(AtomicBool::new(false)), false);
                    let Some(m) = m else { break };
                    tail.push(format!("{}", cozy_chess::util::display_san_move(&board, m)));
                    board.play(m);
                    hist.push(board.hash());
                    if queens(&board) == (false, false) && gone_at.is_none() {
                        gone_at = Some(p + 1);
                    }
                }
            }
            if in_line || gone_at.is_some() {
                out.push_str(&format!(
                    "{} | {} || {} {}\n",
                    side,
                    text,
                    tail.join(" "),
                    if in_line { "(queens off inside the line)".to_string() } else { format!("(queens off {} plies after)", gone_at.unwrap()) }
                ));
            }
        }
        eval::AGGRESSION.store(220, Ordering::Relaxed);
        search::SACRIFICE_BONUS.store(220, Ordering::Relaxed);
        search::CONTEMPT.store(50, Ordering::Relaxed);
        std::fs::write("/tmp/queen_trades.txt", &out).unwrap();
        eprintln!("{}", out);
    }
}
