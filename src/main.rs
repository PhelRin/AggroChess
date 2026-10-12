/// Print a UCI line (and copy it to the debug log, if enabled).
macro_rules! out {
    ($($arg:tt)*) => { $crate::uci::out(&format!($($arg)*)) };
}

pub mod book;
pub mod eval;
pub mod search;
pub mod uci;

fn main() {
    let default_threads = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(1)
        .min(8);
    search::NUM_THREADS.store(default_threads, std::sync::atomic::Ordering::Relaxed);

    // Optional: `aggro_chess bench` prints a fixed-depth node count / speed figure.
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "bench" {
        let h = std::thread::Builder::new().stack_size(64 * 1024 * 1024).spawn(bench).unwrap();
        let _ = h.join();
        return;
    }
    uci::uci_loop();
}

fn bench() {
    use cozy_chess::Board;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    let fens = [
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "r1bqkb1r/pppp1ppp/2n2n2/4p2Q/2B1P3/8/PPPP1PPP/RNB1K1NR w KQkq - 4 4",
        "r1bq1rk1/pp2bppp/2n1pn2/3p4/2PP4/2N1PN2/PP1B1PPP/R2QKB1R w KQ - 0 8",
        "r2q1rk1/pb1nbppp/1p2pn2/2pp4/2PP4/1PN1PN2/PB2BPPP/R2Q1RK1 w - - 0 10",
        "r1b2rk1/2q1bppp/p2ppn2/1p6/3BPP2/2NB4/PPPQ2PP/2KR3R w - - 0 13",
        "8/8/3k4/3p4/3P4/3K4/8/8 w - - 0 1",
    ];
    let depth: i32 = std::env::args().nth(2).and_then(|d| d.parse().ok()).unwrap_or(11);
    search::NUM_THREADS.store(1, Ordering::Relaxed);
    let start = std::time::Instant::now();
    let mut total = 0u64;
    for fen in fens {
        let board = Board::from_fen(fen, false).unwrap();
        search::clear_global_tt();
        let stop = Arc::new(AtomicBool::new(false));
        let tt = search::get_global_tt(search::TT_SIZE_MB.load(Ordering::Relaxed));
        let nodes = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let mut s = search::Searcher::new(tt, stop, Arc::clone(&nodes), true);
        eval::set_engine_color(Some(board.side_to_move()));
        for d in 1..=depth {
            s.search(&board, d, -search::INF, search::INF, 0, false);
        }
        total += s.nodes;
    }
    let ms = start.elapsed().as_millis().max(1);
    println!("{} nodes {} nps", total, total as u128 * 1000 / ms);
}

#[cfg(test)]
mod tests {
    use super::*;
    use cozy_chess::Board;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    fn board(fen: &str) -> Board {
        Board::from_fen(fen, false).unwrap()
    }

    /// Run a search on a big-stack thread (search recursion is deep).
    fn best(fen: &str, depth: i16, ms: u64) -> (String, i32) {
        let fen = fen.to_string();
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                let b = board(&fen);
                search::NUM_THREADS.store(1, Ordering::Relaxed);
                let stop = Arc::new(AtomicBool::new(false));
                let (mv, score) =
                    search::iterative_deepening(&b, &[b.hash()], depth, Some(Duration::from_millis(ms)), stop);
                let s = mv.map(|m| cozy_chess::util::display_uci_move(&b, m).to_string()).unwrap_or_default();
                (s, score)
            })
            .unwrap()
            .join()
            .unwrap()
    }

    fn eval_neutral(fen: &str) -> i32 {
        eval::set_engine_color(None);
        eval::evaluate(&board(fen))
    }

    #[test]
    fn book_move_is_legal() {
        let book = book::Book::new("ph-gambitbook.bin");
        let b = Board::default();
        let mv = book.get_move(&b).expect("book should have a move for startpos");
        let m = cozy_chess::util::parse_uci_move(&b, &mv).unwrap();
        assert!(b.is_legal(m));
    }

    #[test]
    fn eval_startpos_is_balanced() {
        let s = eval_neutral("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        assert!(s.abs() < 40, "startpos eval {}", s);
    }

    #[test]
    fn eval_is_color_symmetric() {
        // Mirrored positions must evaluate identically when no engine side is set.
        let pairs = [
            (
                "r1bqkb1r/pppp1ppp/2n2n2/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4",
                "rnbqk2r/pppp1ppp/5n2/2b1p3/4P3/2N2N2/PPPP1PPP/R1BQKB1R b KQkq - 4 4",
            ),
            (
                "r4rk1/ppp2ppp/2n5/3q4/3P4/2PB1N2/P4PPP/R2Q1RK1 w - - 0 14",
                "r2q1rk1/p4ppp/2pb1n2/3p4/3Q4/2N5/PPP2PPP/R4RK1 b - - 0 14",
            ),
        ];
        for (a, b) in pairs {
            assert_eq!(eval_neutral(a), eval_neutral(b), "{} vs {}", a, b);
        }
    }

    #[test]
    fn eval_material_sanity() {
        let up_knight = eval_neutral("rnbqkb1r/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        assert!(up_knight > 200, "being a knight up should be clearly winning: {}", up_knight);
    }

    #[test]
    fn insufficient_material_is_draw() {
        assert_eq!(eval_neutral("8/8/4k3/8/8/2B5/4K3/8 w - - 0 1"), 0);
        assert_eq!(eval_neutral("8/8/4k3/8/8/2N5/4K3/8 b - - 0 1"), 0);
    }

    #[test]
    fn exposed_king_is_penalised() {
        // Same material; black king stripped of its pawn shield with white pieces aimed at it.
        let safe = eval_neutral("r1bq1rk1/pppp1ppp/2n2n2/2b1p3/2B1P3/2NP1N2/PPP2PPP/R1BQ1RK1 w - - 0 7");
        let open = eval_neutral("r1bq1rk1/pppp1p2/2n2n2/2b1p3/2B1P3/2NP1N2/PPP2PPP/R1BQ1RK1 w - - 0 7");
        assert!(open > safe + 30, "missing g/h pawns should hurt black: safe {} open {}", safe, open);
    }

    #[test]
    fn aggression_raises_attack_value() {
        let fen = "r1bq1rk1/pppp1p2/2n2n1Q/2b1p3/2B1P3/2NP1N2/PPP2PPP/R4RK1 w - - 0 9";
        eval::set_engine_color(None);
        let neutral = eval::evaluate(&board(fen));
        eval::set_engine_color(Some(cozy_chess::Color::White));
        let aggro = eval::evaluate(&board(fen));
        eval::set_engine_color(None);
        assert!(aggro > neutral, "engine side should value its attack more: {} vs {}", aggro, neutral);
    }

    #[test]
    fn finds_mate_in_one() {
        let (mv, score) = best("r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5Q2/PPPP1PPP/RNB1K1NR w KQkq - 4 4", 6, 2000);
        assert_eq!(mv, "f3f7");
        assert!(score > search::MATE_BOUND);
    }

    #[test]
    fn finds_back_rank_mate() {
        let (mv, _) = best("6k1/5ppp/8/8/8/8/5PPP/3R2K1 w - - 0 1", 6, 2000);
        assert_eq!(mv, "d1d8");
    }

    #[test]
    fn checkmated_side_has_no_move() {
        let (mv, score) = best("rnb1kbnr/pppp1ppp/4p3/8/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3", 4, 500);
        assert!(mv.is_empty());
        assert!(score < -search::MATE_BOUND);
    }

    #[test]
    fn greek_gift_sacrifice() {
        // Classic Bxh7+ setup: Bxh7+ Kxh7 Ng5+ with the queen joining via h5.
        let (mv, score) = best("r1bq1rk1/pppn1ppp/4p3/3pP3/1b1P4/2NB1N2/PPP2PPP/R1BQK2R w KQ - 0 8", 30, 8000);
        assert_eq!(mv, "d3h7", "engine should find the Greek gift, score {}", score);
    }

    #[test]
    fn does_not_hang_knight() {
        // 1. e4 c6 2. Nf3 d5 3. d4 dxe4: the f3 knight is attacked and must move.
        let (mv, _) = best("rnbqkbnr/pp2pppp/2p5/8/3Pp3/5N2/PPP2PPP/RNBQKB1R w KQkq - 0 4", 30, 2000);
        assert!(["f3e5", "f3g5", "f3d2", "f3g1", "f3h4"].contains(&mv.as_str()), "played {}", mv);
    }

    #[test]
    fn see_values() {
        let b = board("4k3/4b3/2n5/8/8/8/8/4R1K1 w - - 0 1");
        let m = cozy_chess::util::parse_uci_move(&b, "e1e7").unwrap();
        assert!(!search::see_ge(&b, m, 0), "Rxe7 loses the exchange");
        assert!(search::see_ge(&b, m, -200));

        let b = board("4k3/8/2p5/1B6/8/8/8/6K1 w - - 0 1");
        let m = cozy_chess::util::parse_uci_move(&b, "b5c6").unwrap();
        assert_eq!(search::see(&b, m), 100);

        let b = board("4k3/8/8/3PpP2/8/8/8/4K3 w - e6 0 2");
        let m = cozy_chess::util::parse_uci_move(&b, "d5e6").unwrap();
        assert_eq!(search::see(&b, m), 100);
    }

    #[test]
    fn castling_round_trip() {
        let mut b = Board::default();
        for mv in ["e2e4", "e7e5", "g1f3", "b8c6", "f1c4", "f8c5"] {
            let m = cozy_chess::util::parse_uci_move(&b, mv).unwrap();
            b.play(m);
        }
        let m = cozy_chess::util::parse_uci_move(&b, "e1g1").unwrap();
        assert!(b.is_legal(m));
        assert_eq!(cozy_chess::util::display_uci_move(&b, m).to_string(), "e1g1");
    }

    #[test]
    fn repetition_is_handled() {
        let mut b = Board::default();
        let mut hist = vec![b.hash()];
        for mv in ["g1f3", "g8f6", "f3g1", "f6g8"] {
            let m = cozy_chess::util::parse_uci_move(&b, mv).unwrap();
            b.play(m);
            hist.push(b.hash());
        }
        let stop = Arc::new(AtomicBool::new(false));
        let (mv, _) = std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || search::iterative_deepening(&b, &hist, 4, Some(Duration::from_millis(500)), stop))
            .unwrap()
            .join()
            .unwrap();
        assert!(mv.is_some());
    }

    #[test]
    fn tt_round_trip() {
        use cozy_chess::{Move, Square};
        let tt = search::TranspositionTable::new(1);
        let mv = Some(Move { from: Square::E2, to: Square::E4, promotion: None });
        tt.store(12345, mv, 100, 37, 5, search::Bound::Exact);
        let e = tt.probe(12345).unwrap();
        assert_eq!(e.mv, mv);
        assert_eq!(e.score, 100);
        assert_eq!(e.eval, 37);
        assert_eq!(e.depth, 5);
        assert_eq!(e.bound, search::Bound::Exact);
        assert!(tt.probe(54321).is_none());
    }

    #[test]
    fn multithreaded_search_returns_move() {
        let r = std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(|| {
                search::NUM_THREADS.store(3, Ordering::Relaxed);
                let b = Board::default();
                let stop = Arc::new(AtomicBool::new(false));
                let r = search::iterative_deepening(&b, &[], 6, Some(Duration::from_millis(1000)), stop);
                search::NUM_THREADS.store(1, Ordering::Relaxed);
                r
            })
            .unwrap()
            .join()
            .unwrap();
        assert!(r.0.is_some());
    }
}
