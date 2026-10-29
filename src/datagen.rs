//! Self-play data generation for training the NNUE.
//!
//! `aggro_chess datagen <out-prefix> <threads> <positions> [soft-nodes] [seed]`
//!
//! Each thread plays games against itself from a random opening (8 or 9 random plies),
//! searching every move to a fixed node budget with the personality switched off (no
//! sacrifice bonus, no contempt, objective king danger), so the labels are AggroChess's own
//! honest judgement of the position. Quiet positions are written with the search score and
//! the final game result.
//!
//! Record format (32 bytes, little endian):
//!   occupancy u64 | 16 bytes of 4-bit pieces (colour << 3 | piece) in occupancy order |
//!   score i16 (White's view, centipawns) | result u8 (0 = Black won, 1 = draw, 2 = White won) |
//!   side to move u8 (0 = White) | 4 bytes unused

use crate::eval;
use crate::search::{self, NodeSearcher, MATE_BOUND};
use cozy_chess::{Board, Color, GameStatus, Move, Piece};
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub const RECORD: usize = 32;

pub fn pack(board: &Board, score_white: i16, result: u8) -> [u8; RECORD] {
    let mut out = [0u8; RECORD];
    let occ = board.occupied().0;
    out[0..8].copy_from_slice(&occ.to_le_bytes());
    let mut it = occ;
    let mut i = 0;
    while it != 0 {
        let s = it.trailing_zeros() as usize;
        it &= it - 1;
        let sq = cozy_chess::Square::index(s);
        let p = board.piece_on(sq).unwrap() as u8;
        let c = board.color_on(sq).unwrap() as u8;
        let nib = (c << 3) | p;
        out[8 + i / 2] |= if i % 2 == 0 { nib } else { nib << 4 };
        i += 1;
    }
    out[24..26].copy_from_slice(&score_white.to_le_bytes());
    out[26] = result;
    out[27] = board.side_to_move() as u8;
    out
}

/// Rebuild a board from a record (castling rights and en passant are not stored).
pub fn unpack(rec: &[u8]) -> Option<Board> {
    let occ = u64::from_le_bytes(rec[0..8].try_into().ok()?);
    let mut grid = [None; 64];
    let mut it = occ;
    let mut i = 0;
    while it != 0 {
        let s = it.trailing_zeros() as usize;
        it &= it - 1;
        let nib = (rec[8 + i / 2] >> (4 * (i % 2))) & 15;
        grid[s] = Some(((nib >> 3) as usize, (nib & 7) as usize));
        i += 1;
    }
    let mut fen = String::new();
    for r in (0..8).rev() {
        let mut empty = 0;
        for f in 0..8 {
            match grid[r * 8 + f] {
                None => empty += 1,
                Some((c, p)) => {
                    if empty > 0 {
                        fen.push_str(&empty.to_string());
                        empty = 0;
                    }
                    let ch = b"pnbrqk"[p] as char;
                    fen.push(if c == 0 { ch.to_ascii_uppercase() } else { ch });
                }
            }
        }
        if empty > 0 {
            fen.push_str(&empty.to_string());
        }
        if r > 0 {
            fen.push('/');
        }
    }
    fen.push_str(if rec[27] == 0 { " w - - 0 1" } else { " b - - 0 1" });
    Board::from_fen(&fen, false).ok()
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn legal_moves(board: &Board) -> Vec<Move> {
    let mut v = Vec::new();
    board.generate_moves(|mvs| {
        v.extend(mvs);
        false
    });
    v
}

pub fn run(args: &[String]) {
    let prefix = args.first().cloned().unwrap_or_else(|| "data".into());
    let threads: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let target: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1_000_000);
    let soft: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(5000);
    let seed: u64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or_else(|| {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64
    });

    // AGGRO_HCE=1 labels with the classical evaluation (generation 0); otherwise the built-in net
    if std::env::var("AGGRO_HCE").is_ok() {
        search::USE_NNUE.store(false, Ordering::Relaxed);
    }
    // neutral personality: objective labels
    eval::AGGRESSION.store(100, Ordering::Relaxed);
    eval::set_engine_color(None);
    search::CONTEMPT.store(0, Ordering::Relaxed);
    search::SACRIFICE_BONUS.store(0, Ordering::Relaxed);
    search::SWINDLE.store(false, Ordering::Relaxed);
    search::BEAUTIFUL_MATES.store(false, Ordering::Relaxed);

    let written = Arc::new(AtomicU64::new(0));
    let games = Arc::new(AtomicU64::new(0));
    let start = std::time::Instant::now();
    let mut handles = Vec::new();
    for t in 0..threads {
        let written = Arc::clone(&written);
        let games = Arc::clone(&games);
        let path = format!("{}_{}.bin", prefix, t);
        let tseed = seed ^ (0x9E37_79B9_7F4A_7C15u64.wrapping_mul(t as u64 + 1)) | 1;
        handles.push(
            std::thread::Builder::new()
                .stack_size(64 * 1024 * 1024)
                .spawn(move || worker(&path, tseed, soft, target, written, games))
                .unwrap(),
        );
    }
    // progress
    loop {
        std::thread::sleep(std::time::Duration::from_secs(30));
        let w = written.load(Ordering::Relaxed);
        let g = games.load(Ordering::Relaxed);
        let secs = start.elapsed().as_secs_f64();
        eprintln!("{} positions, {} games, {:.0} pos/s", w, g, w as f64 / secs);
        if w >= target {
            break;
        }
    }
    for h in handles {
        let _ = h.join();
    }
}

fn worker(path: &str, seed: u64, soft: u64, target: u64, written: Arc<AtomicU64>, games: Arc<AtomicU64>) {
    let mut rng = Rng(seed);
    let mut file = std::io::BufWriter::new(
        std::fs::OpenOptions::new().create(true).append(true).open(path).expect("open output"),
    );
    let mut ns = NodeSearcher::new(16);
    let mut buf: Vec<(Board, i16)> = Vec::with_capacity(512);

    while written.load(Ordering::Relaxed) < target {
        // ---- random opening ----
        let mut board = Board::default();
        let plies = 8 + rng.below(2);
        let mut ok = true;
        for _ in 0..plies {
            let moves = legal_moves(&board);
            if moves.is_empty() {
                ok = false;
                break;
            }
            board.play_unchecked(moves[rng.below(moves.len())]);
        }
        if !ok || board.status() != GameStatus::Ongoing {
            continue;
        }
        ns.new_game();
        // throw away openings that are already lopsided
        let (_, s) = ns.search(&board, &[], 2000, 20000);
        if s.abs() > 300 {
            continue;
        }

        let mut hashes: Vec<u64> = vec![board.hash()];
        buf.clear();
        let mut win_run = 0i32; // consecutive plies with a decisive score (sign = White's view)
        let mut draw_run = 0i32;
        let result: u8;
        let mut ply = 0;
        loop {
            match board.status() {
                GameStatus::Won => {
                    // side to move is mated
                    result = if board.side_to_move() == Color::White { 0 } else { 2 };
                    break;
                }
                GameStatus::Drawn => {
                    result = 1;
                    break;
                }
                GameStatus::Ongoing => {}
            }
            if board.halfmove_clock() >= 100 || eval::is_insufficient_material(&board) {
                result = 1;
                break;
            }
            let h = board.hash();
            if hashes.iter().filter(|&&x| x == h).count() >= 3 {
                result = 1;
                break;
            }
            if ply >= 400 {
                result = 1;
                break;
            }

            let (mv, score) = ns.search(&board, &hashes, soft, soft * 8);
            let mv = match mv {
                Some(m) => m,
                None => {
                    result = 1;
                    break;
                }
            };
            let white = if board.side_to_move() == Color::White { score } else { -score };

            // adjudication
            if white.abs() >= 1500 {
                win_run = if win_run.signum() == white.signum() { win_run + white.signum() } else { white.signum() };
            } else {
                win_run = 0;
            }
            if win_run.abs() >= 6 {
                result = if win_run > 0 { 2 } else { 0 };
                break;
            }
            if ply >= 60 && white.abs() <= 10 {
                draw_run += 1;
            } else {
                draw_run = 0;
            }
            if draw_run >= 12 {
                result = 1;
                break;
            }

            // keep quiet positions only
            let in_check = !board.checkers().is_empty();
            let noisy = search::is_capture(&board, mv) || mv.promotion.is_some();
            if !in_check && !noisy && score.abs() < MATE_BOUND {
                buf.push((board.clone(), white.clamp(-4000, 4000) as i16));
            }

            board.play_unchecked(mv);
            hashes.push(board.hash());
            ply += 1;
        }
        let _ = Piece::Pawn;
        for (b, s) in &buf {
            file.write_all(&pack(b, *s, result)).unwrap();
        }
        file.flush().unwrap();
        written.fetch_add(buf.len() as u64, Ordering::Relaxed);
        games.fetch_add(1, Ordering::Relaxed);
    }
}
