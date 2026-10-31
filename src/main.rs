/// Print a UCI line (and copy it to the debug log, if enabled).
macro_rules! out {
    ($($arg:tt)*) => { $crate::uci::out(&format!($($arg)*)) };
}

pub mod book;
pub mod datagen;
pub mod eval;
pub mod matesearch;
pub mod nnue;
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
    if args.len() > 1 && args[1] == "datagen" {
        datagen::run(&args[2..]);
        return;
    }
    if args.len() > 1 && args[1] == "evalcmp" {
        // print "classical nnue" (neutral, side to move's view) for every k-th record of a data file
        let data = std::fs::read(&args[2]).unwrap();
        let step: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(100);
        eval::set_engine_color(None);
        for rec in data.chunks_exact(datagen::RECORD).step_by(step) {
            if let Some(b) = datagen::unpack(rec) {
                println!("{} {}", eval::evaluate(&b), nnue::evaluate_fresh(&b));
            }
        }
        return;
    }
    if args.len() > 1 && args[1] == "matefind" {
        // matefind "<fen>" "<moves>" <max moves> <seconds>
        let mut b: cozy_chess::Board = args[2].parse().unwrap();
        for mv in args[3].split_whitespace() {
            let m = cozy_chess::util::parse_uci_move(&b, mv).unwrap();
            b.play(m);
        }
        let n: u8 = args[4].parse().unwrap();
        let secs: f64 = args[5].parse().unwrap();
        let stop = std::sync::atomic::AtomicBool::new(false);
        let t = std::time::Instant::now();
        let td: u8 = std::env::var("TD").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
        if std::env::var("PNS").is_ok() {
            let r = matesearch::prove_mate(&b, n, std::time::Duration::from_secs_f64(secs), &stop, td);
            match r {
                Some((m, nodes)) => println!("proved mate with {} ({} nodes, {:.1}s)", cozy_chess::util::display_uci_move(&b, m), nodes, t.elapsed().as_secs_f64()),
                None => println!("no proof ({:.1}s)", t.elapsed().as_secs_f64()),
            }
            return;
        }
        let r = matesearch::find_mate(&b, n, std::time::Duration::from_secs_f64(secs), &stop, td);
        match r {
            Some((m, k, nodes)) => println!("mate in {} with {} ({} nodes, {:.1}s)", k, cozy_chess::util::display_uci_move(&b, m), nodes, t.elapsed().as_secs_f64()),
            None => println!("no mate found ({:.1}s)", t.elapsed().as_secs_f64()),
        }
        return;
    }
    if args.len() > 1 && args[1] == "evalfen" {
        // one FEN per stdin line -> "classical nnue" from the side to move's view
        use std::io::BufRead;
        eval::set_engine_color(None);
        for line in std::io::stdin().lock().lines() {
            let line = line.unwrap();
            if let Ok(b) = line.trim().parse::<cozy_chess::Board>() {
                println!("{} {}  {}", eval::evaluate(&b), nnue::evaluate_fresh(&b), line.trim());
            }
        }
        return;
    }
    if args.len() > 1 && args[1] == "evalbench" {
        use std::hint::black_box;
        let b: cozy_chess::Board = "r1bq1rk1/pp2bppp/2n1pn2/3p4/2PP4/2N1PN2/PP1B1PPP/R2QKB1R w KQ - 0 8".parse().unwrap();
        eval::set_engine_color(Some(cozy_chess::Color::White));
        let n = 2_000_000;
        let t = std::time::Instant::now();
        for _ in 0..n { black_box(eval::evaluate(black_box(&b))); }
        println!("classical+personality {:.0} ns", t.elapsed().as_nanos() as f64 / n as f64);
        let t = std::time::Instant::now();
        for _ in 0..n { black_box(eval::personality(black_box(&b))); }
        println!("personality only      {:.0} ns", t.elapsed().as_nanos() as f64 / n as f64);
        let mut acc = nnue::Accumulator::default();
        acc.refresh(&b);
        let t = std::time::Instant::now();
        for _ in 0..n { black_box(black_box(&acc).evaluate(black_box(&b))); }
        println!("nnue output           {:.0} ns", t.elapsed().as_nanos() as f64 / n as f64);
        let t = std::time::Instant::now();
        for _ in 0..n { let mut a = nnue::Accumulator::default(); a.refresh(black_box(&b)); black_box(&a); }
        println!("nnue refresh          {:.0} ns", t.elapsed().as_nanos() as f64 / n as f64);
        let m = cozy_chess::util::parse_uci_move(&b, "c4d5").unwrap();
        let t = std::time::Instant::now();
        for _ in 0..n { let mut a = black_box(&acc).clone(); a.apply(black_box(&b), m); black_box(&a); }
        println!("nnue capture update   {:.0} ns", t.elapsed().as_nanos() as f64 / n as f64);
        return;
    }
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
    fn prefers_sacrificial_mate() {
        // Re8# mates at once, but Qd8+! Bxd8 Re8# is prettier
        let b: Board = "6k1/5ppp/5b2/8/8/3Q4/5PPP/4R1K1 w - - 0 1".parse().unwrap();
        let limits = search::Limits { depth: None, soft: Some(std::time::Duration::from_millis(500)), hard: Some(std::time::Duration::from_millis(1000)), nodes: None };
        let (m, score) = search::search_position(&b, &[], limits, std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)), false);
        assert!(score >= search::MATE_BOUND);
        assert_eq!(format!("{}", cozy_chess::util::display_uci_move(&b, m.unwrap())), "d3d8");
    }

    #[test]
    fn swindle_stays_in_time_and_legal() {
        // White is clearly worse here; with swindles on it may pick a trickier move, but it must
        // still be legal and respect the hard time limit
        let b: Board = "5r1R/ppp2kp1/3p4/4npr1/4P3/2P4Q/PPq3PP/4R1K1 w - - 3 23".parse().unwrap();
        let limits = search::Limits { depth: None, soft: Some(std::time::Duration::from_millis(400)), hard: Some(std::time::Duration::from_millis(1200)), nodes: None };
        let t = std::time::Instant::now();
        let (m, score) = search::search_position(&b, &[], limits, std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)), false);
        assert!(t.elapsed() < std::time::Duration::from_millis(1500), "took {:?}", t.elapsed());
        assert!(score < 0, "expected a worse position, got {}", score);
        assert!(b.is_legal(m.unwrap()));
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
    fn no_cheap_pawn_sacrifice_in_the_albin() {
        // 1.d4 d5 2.c4 e5 3.dxe5 d4 4.Nf3 Nc6 5.a3 Be6 6.Qb3: 6...d3? only "attacks" e2 and loses
        // about three pawns; a single pawn must not earn the sacrifice bonus (v4.0 played it)
        let (mv, _) = best("r2qkbnr/ppp2ppp/2n1b3/4P3/2Pp4/PQ3N2/1P2PPPP/RNB1KB1R b KQkq - 2 6", 30, 2000);
        assert_ne!(mv, "d4d3");
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
    #[test]
    fn penrose_fortress_is_a_draw() {
        // Penrose's Puzzle: Black has a queen, two rooks and three bishops, but everything except
        // the dark-squared bishops is walled in and White lives on the light squares
        let b: Board = "8/p7/kpP5/qrp1b3/rpP2b2/pP4b1/P3K3/8 w - - 0 1".parse().unwrap();
        assert!(eval::is_locked_fortress(&b));
        // ...but an ordinary middlegame with a material edge is not a fortress
        let n: Board = "rnbqkb1r/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".parse().unwrap();
        assert!(!eval::is_locked_fortress(&n));
        // and a blocked position where the extra pieces can still move is not one either
        let c: Board = "r1bqk2r/pp3ppp/2n1p3/2ppP3/3P4/P1P2N2/2P2PPP/R1BQKB1R w KQkq - 0 9".parse().unwrap();
        assert!(!eval::is_locked_fortress(&c));
    }

    #[test]
    fn king_clamp_rewards_h6_and_a6() {
        use std::sync::atomic::Ordering;
        eval::set_engine_color(Some(cozy_chess::Color::White));
        let with = |fen: &str, c: i32| {
            eval::KING_CLAMP.store(c, Ordering::Relaxed);
            let b: Board = fen.parse().unwrap();
            eval::personality(&b)
        };
        // h6 against the castled king on g8 (black to move: the score is from black's view)
        let h6 = "r4rk1/ppp2p1p/2n3pP/8/8/8/PPP2PP1/R3R1K1 b - - 0 1";
        assert!(with(h6, 45) < with(h6, 0));
        // a6 against a king on b8
        let a6 = "1k1r3r/p1p2ppp/P1n5/8/8/8/1PP2PPP/3RR1K1 b - - 0 1";
        assert!(with(a6, 45) < with(a6, 0));
        eval::KING_CLAMP.store(0, Ordering::Relaxed);
        eval::set_engine_color(None);
    }

    #[test]
    fn sealed_wall_is_a_draw() {
        // the end of Friedel's study: Black has two rooks and a bishop more, but the rammed pawn
        // chain lets nothing through
        let b: Board = "3k4/1r2p3/r2pPp2/b1pP1Pp1/1pP3Pp/pP2K2P/P7/8 b - - 0 6".parse().unwrap();
        assert!(eval::is_sealed_fortress(&b));
        // with a gap in the wall (no h-pawns) the rooks get in
        let o: Board = "3k4/1r2p3/r2pPp2/b1pP1Pp1/1pP3P1/pP2K3/P7/8 b - - 0 6".parse().unwrap();
        assert!(!eval::is_sealed_fortress(&o));
        // and the start of the study is no fortress yet
        let s: Board = "3B4/1r2p3/r2p1p2/bkp1P1p1/1p1P1PPp/p1P1K2P/PPB5/8 w - - 0 1".parse().unwrap();
        assert!(!eval::is_sealed_fortress(&s));
    }

    #[test]
    fn plaskett_tal_move_is_found_in_trouble() {
        // Plaskett's Puzzle after 1.Nf6+ Kg7 2.Nh5+ Kg6: only 3.Bc2+!! wins (Tal solved it).
        // White looks lost here, which switches Tal mode on.
        if std::env::var("AGGRO_SLOW_TESTS").is_err() {
            return;
        }
        let mut b: Board = "8/3P3k/n2K3p/2p3n1/1b4N1/2p1p1P1/8/3B4 w - - 0 1".parse().unwrap();
        for m in ["g4f6", "h7g7", "f6h5", "g7g6"] {
            let mv = cozy_chess::util::parse_uci_move(&b, m).unwrap();
            b.play(mv);
        }
        let limits = search::Limits { depth: None, soft: Some(std::time::Duration::from_secs(30)), hard: Some(std::time::Duration::from_secs(30)), nodes: None };
        let (m, score) = search::search_position(&b, &[], limits, std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)), false);
        assert_eq!(format!("{}", cozy_chess::util::display_uci_move(&b, m.unwrap())), "d1c2", "score {}", score);
    }

}

#[cfg(test)]
mod nnue_tests {
    use cozy_chess::Board;

    /// Incremental accumulator updates must give exactly the same result as a full refresh,
    /// through captures, promotions, castling and en passant.
    #[test]
    fn incremental_matches_refresh() {
        let mut seed = 0x2545F4914F6CDD1Du64;
        for _game in 0..200 {
            let mut b = Board::default();
            let mut acc = crate::nnue::Accumulator::default();
            acc.refresh(&b);
            for _ in 0..200 {
                let mut moves = Vec::new();
                b.generate_moves(|m| {
                    moves.extend(m);
                    false
                });
                if moves.is_empty() {
                    break;
                }
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let m = moves[(seed % moves.len() as u64) as usize];
                let mut next = acc.clone();
                next.apply(&b, m);
                b.play_unchecked(m);
                let mut fresh = crate::nnue::Accumulator::default();
                fresh.refresh(&b);
                assert!(next.v == fresh.v, "mismatch after {} in {}", m, b);
                acc = next;
            }
        }
    }
}
