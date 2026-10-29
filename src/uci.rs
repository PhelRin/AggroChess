use crate::book::Book;
use crate::search::{self, Limits};
use cozy_chess::{Board, Color};
use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const SEARCH_STACK: usize = 64 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Opt-in debug log: if a file named `aggro_debug.log` exists in the engine's
// working directory, every line received (>>) and sent (<<) is appended to it.
// ---------------------------------------------------------------------------
static LOG: std::sync::Mutex<Option<std::fs::File>> = std::sync::Mutex::new(None);
static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

fn init_log() {
    START.get_or_init(std::time::Instant::now);
    if std::path::Path::new("aggro_debug.log").exists() {
        if let Ok(f) = std::fs::OpenOptions::new().append(true).open("aggro_debug.log") {
            if let Ok(mut l) = LOG.lock() {
                *l = Some(f);
            }
            log_line("==", "engine started");
        }
    }
}

fn log_line(dir: &str, s: &str) {
    if let Ok(mut l) = LOG.lock() {
        if let Some(f) = l.as_mut() {
            let ms = START.get().map(|t| t.elapsed().as_millis()).unwrap_or(0);
            let _ = writeln!(f, "{:>9} {} {}", ms, dir, s);
        }
    }
}

/// Write one line to the GUI (stdout) and to the debug log.
pub fn out(s: &str) {
    let stdout = io::stdout();
    let mut h = stdout.lock();
    let _ = writeln!(h, "{}", s);
    let _ = h.flush();
    drop(h);
    log_line("<<", s);
}

pub fn uci_loop() {
    let mut board = Board::default();
    let mut book_path = String::from("ph-gambitbook.bin");
    let mut book: Option<Book> = None; // loaded lazily so `uci` answers instantly
    let own_book = true; // curated gambits always; Polyglot book used if BookPath exists
    let move_overhead: u64 = 50; // ms reserved per move for GUI / network lag
    let mut history_hashes = vec![board.hash()];
    let mut prev_plies: usize = 0;

    let stop_flag = Arc::new(AtomicBool::new(false));
    let mut search_thread: Option<std::thread::JoinHandle<()>> = None;

    init_log();
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        log_line(">>", line.trim_end());
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }

        match tokens[0] {
            "uci" => {
                out!("id name AggroChess 4.1");
                out!("id author Rinarto Gibson");
                // Only a few knobs are exposed on purpose. Everything else (hash size,
                // contempt, time safety margin) is fixed at tuned defaults.
                out!(
                    "option name Threads type spin default {} min 1 max 128",
                    search::NUM_THREADS.load(Ordering::Relaxed)
                );
                out!("option name BookPath type string default ph-gambitbook.bin");
                out!("option name Swindle type check default true");
                out!("uciok");
            }
            "isready" => {
                out!("readyok");
            }
            "setoption" => {
                // setoption name <name...> value <value...>
                let name_pos = tokens.iter().position(|&t| t == "name");
                let value_pos = tokens.iter().position(|&t| t == "value");
                if let Some(np) = name_pos {
                    let name_end = value_pos.unwrap_or(tokens.len());
                    let name = tokens[np + 1..name_end].join(" ").to_lowercase();
                    let value = value_pos.map(|vp| tokens[vp + 1..].join(" ")).unwrap_or_default();
                    match name.as_str() {
                        "threads" => {
                            if let Ok(t) = value.parse::<usize>() {
                                search::NUM_THREADS.store(t.clamp(1, 128), Ordering::Relaxed);
                            }
                        }
                        "bookpath" => {
                            book_path = value.clone();
                            book = None;
                        }
                        "swindle" => {
                            search::SWINDLE.store(value.trim().eq_ignore_ascii_case("true"), Ordering::Relaxed);
                        }
                        // hidden tuning knobs (not advertised in "uci"; used for testing)
                        "x_nnuescale" => {
                            if let Ok(v) = value.trim().parse::<i32>() {
                                crate::nnue::OUT_SCALE.store(v, Ordering::Relaxed);
                            }
                        }
                        "x_sacbonus" => {
                            if let Ok(v) = value.trim().parse::<i32>() {
                                search::SACRIFICE_BONUS.store(v, Ordering::Relaxed);
                            }
                        }
                        "x_sacfloor" => {
                            if let Ok(v) = value.trim().parse::<i32>() {
                                search::SAC_FLOOR.store(v, Ordering::Relaxed);
                            }
                        }
                        "x_aggression" => {
                            if let Ok(v) = value.trim().parse::<i32>() {
                                crate::eval::AGGRESSION.store(v, Ordering::Relaxed);
                            }
                        }
                        "x_contempt" => {
                            if let Ok(v) = value.trim().parse::<i32>() {
                                search::CONTEMPT.store(v, Ordering::Relaxed);
                            }
                        }
                        "x_sacattack" => {
                            search::SAC_NEEDS_ATTACK.store(value.trim().eq_ignore_ascii_case("true"), Ordering::Relaxed);
                        }
                        "x_sacverify" => {
                            search::SAC_VERIFY.store(value.trim().eq_ignore_ascii_case("true"), Ordering::Relaxed);
                        }
                        "x_sacunits" => {
                            if let Ok(v) = value.trim().parse::<i32>() {
                                search::SAC_FULL_UNITS.store(v, Ordering::Relaxed);
                            }
                        }
                        "x_pressure" => {
                            search::SAC_PRESSURE.store(value.trim().eq_ignore_ascii_case("true"), Ordering::Relaxed);
                        }
                        "x_pstart" => {
                            if let Ok(v) = value.trim().parse::<i32>() {
                                search::PRESSURE_START.store(v, Ordering::Relaxed);
                            }
                        }
                        "x_pstep" => {
                            if let Ok(v) = value.trim().parse::<i32>() {
                                search::PRESSURE_STEP.store(v, Ordering::Relaxed);
                            }
                        }
                        "x_nnue" => {
                            search::USE_NNUE.store(value.trim().eq_ignore_ascii_case("true"), Ordering::Relaxed);
                        }
                        _ => {} // all other settings are fixed
                    }
                }
            }
            "ucinewgame" => {
                stop_and_join(&stop_flag, &mut search_thread);
                search::LAST_SCORE.store(0, Ordering::Relaxed);
                prev_plies = 0;
                board = Board::default();
                history_hashes = vec![board.hash()];
                search::clear_global_tt();
            }
            "position" => {
                stop_and_join(&stop_flag, &mut search_thread);
                history_hashes.clear();
                let mut moves_index = None;
                if tokens.len() >= 2 && tokens[1] == "startpos" {
                    board = Board::default();
                    if tokens.len() >= 3 && tokens[2] == "moves" {
                        moves_index = Some(3);
                    }
                } else if tokens.len() >= 2 && tokens[1] == "fen" {
                    let mut i = 2;
                    let mut parts = Vec::new();
                    while i < tokens.len() && tokens[i] != "moves" {
                        parts.push(tokens[i]);
                        i += 1;
                    }
                    match Board::from_fen(&parts.join(" "), false) {
                        Ok(b) => board = b,
                        Err(e) => out!("info string Error parsing FEN: {:?}", e),
                    }
                    if i < tokens.len() {
                        moves_index = Some(i + 1);
                    }
                }
                history_hashes.push(board.hash());
                // material (White's view) before each move, and who moved, for sacrifice tracking
                let mut mat_hist = vec![sac_material(&board)];
                let mut movers = Vec::new();
                if let Some(start) = moves_index {
                    for &mv_str in &tokens[start.min(tokens.len())..] {
                        match cozy_chess::util::parse_uci_move(&board, mv_str) {
                            Ok(mv) if board.is_legal(mv) => {
                                movers.push(board.side_to_move());
                                board.play_unchecked(mv);
                                history_hashes.push(board.hash());
                                mat_hist.push(sac_material(&board));
                            }
                            _ => {
                                out!("info string Illegal move in position: {}", mv_str);
                                break;
                            }
                        }
                    }
                }
                let sacs = count_sacrifices(&mat_hist, &movers);
                search::GAME_SACS[0].store(sacs[0], Ordering::Relaxed);
                search::GAME_SACS[1].store(sacs[1], Ordering::Relaxed);
                // a shorter game than last time means a new game: forget the old score
                if movers.len() < prev_plies || movers.len() < 2 {
                    search::LAST_SCORE.store(0, Ordering::Relaxed);
                }
                prev_plies = movers.len();
            }
            "go" => {
                stop_and_join(&stop_flag, &mut search_thread);

                let mut limits = Limits::default();
                let mut wtime = None;
                let mut btime = None;
                let mut winc = 0u64;
                let mut binc = 0u64;
                let mut movestogo: Option<u64> = None;
                let mut movetime = None;
                let mut infinite = false;
                let mut i = 1;
                while i < tokens.len() {
                    let next = tokens.get(i + 1).copied().unwrap_or("");
                    match tokens[i] {
                        "infinite" => infinite = true,
                        "depth" => {
                            limits.depth = next.parse().ok();
                            i += 1;
                        }
                        "nodes" => {
                            limits.nodes = next.parse().ok();
                            i += 1;
                        }
                        "movetime" => {
                            movetime = next.parse::<u64>().ok();
                            i += 1;
                        }
                        "wtime" => {
                            wtime = next.parse::<i64>().ok().map(|v| v.max(0) as u64);
                            i += 1;
                        }
                        "btime" => {
                            btime = next.parse::<i64>().ok().map(|v| v.max(0) as u64);
                            i += 1;
                        }
                        "winc" => {
                            winc = next.parse().unwrap_or(0);
                            i += 1;
                        }
                        "binc" => {
                            binc = next.parse().unwrap_or(0);
                            i += 1;
                        }
                        "movestogo" => {
                            movestogo = next.parse().ok();
                            i += 1;
                        }
                        _ => {}
                    }
                    i += 1;
                }

                // Opening book (only for normal timed games / plain "go")
                if own_book && limits.depth.is_none() && limits.nodes.is_none() && !infinite {
                    if book.is_none() {
                        book = Some(Book::new(&book_path));
                    }
                    if let Some(bm) = book.as_ref().and_then(|b| b.get_move(&board)) {
                        if let Ok(mv) = cozy_chess::util::parse_uci_move(&board, &bm) {
                            if board.is_legal(mv) {
                                out!("info string book move");
                                out!("bestmove {}", cozy_chess::util::display_uci_move(&board, mv));
                                let _ = io::stdout().flush();
                                continue;
                            }
                        }
                    }
                }

                if let Some(mt) = movetime {
                    let t = Duration::from_millis(mt.saturating_sub(move_overhead).max(1));
                    limits.soft = Some(t);
                    limits.hard = Some(t);
                } else if !infinite {
                    let (time, inc) = if board.side_to_move() == Color::White { (wtime, winc) } else { (btime, binc) };
                    if let Some(time) = time {
                        let avail = time.saturating_sub(move_overhead).max(1);
                        let mtg = movestogo.unwrap_or(30).clamp(1, 50);
                        let mut soft = avail / mtg + inc * 3 / 4;
                        soft = soft.min(avail / 4);
                        let hard = (soft * 4).min(avail / 2).max(soft);
                        limits.soft = Some(Duration::from_millis(soft.max(1)));
                        limits.hard = Some(Duration::from_millis(hard.max(1)));
                    }
                }

                let search_board = board.clone();
                let stop_clone = Arc::clone(&stop_flag);
                let hist = history_hashes.clone();
                stop_flag.store(false, Ordering::Relaxed);
                search_thread = Some(
                    std::thread::Builder::new()
                        .stack_size(SEARCH_STACK)
                        .spawn(move || {
                            let (best, _) = search::search_position(&search_board, &hist, limits, stop_clone, true);
                            match best {
                                Some(mv) => out!("bestmove {}", cozy_chess::util::display_uci_move(&search_board, mv)),
                                None => out!("bestmove 0000"),
                            }
                            let _ = io::stdout().flush();
                        })
                        .expect("failed to spawn search thread"),
                );
            }
            "stop" => {
                stop_and_join(&stop_flag, &mut search_thread);
            }
            "quit" => {
                stop_and_join(&stop_flag, &mut search_thread);
                break;
            }
            "d" => {
                out!("{}", board);
                out!("eval (side to move): {} (nnue {}, personality {}, classical {})", crate::nnue::evaluate_fresh(&board) + crate::eval::personality(&board), crate::nnue::evaluate_fresh(&board), crate::eval::personality(&board), crate::eval::evaluate(&board));
            }
            _ => {}
        }
        let _ = io::stdout().flush();
    }
}

/// Material from White's point of view (pawn 100, knight 320, bishop 330, rook 500, queen 900).
fn sac_material(board: &Board) -> i32 {
    const VAL: [i32; 6] = [100, 320, 330, 500, 900, 0];
    let mut s = 0;
    for p in cozy_chess::Piece::ALL {
        s += VAL[p as usize]
            * (board.colored_pieces(cozy_chess::Color::White, p).len() as i32
                - board.colored_pieces(cozy_chess::Color::Black, p).len() as i32);
    }
    s
}

/// Sacrifices made by each colour in a game: the mover is at least 2 pawns down two plies
/// later and still at least 1.5 pawns down two plies after that (the same rule as the match
/// statistics). `mat_hist[k]` is the material before move `k`, `movers[k]` who made it.
pub fn count_sacrifices(mat_hist: &[i32], movers: &[cozy_chess::Color]) -> [i32; 2] {
    let mut sacs = [0; 2];
    // each side on its own, so one side's sacrifice never hides the other's
    for side in [cozy_chess::Color::White, cozy_chess::Color::Black] {
        let sign = if side == cozy_chess::Color::White { 1 } else { -1 };
        let mut k = 0;
        while k < movers.len() {
            if movers[k] != side {
                k += 1;
                continue;
            }
            let before = mat_hist[k] * sign;
            if k + 2 < mat_hist.len() {
                let after2 = mat_hist[k + 2] * sign;
                let after4 = if k + 4 < mat_hist.len() { mat_hist[k + 4] * sign } else { after2 };
                if after2 <= before - 200 && after4 <= before - 150 {
                    sacs[side as usize] += 1;
                    k += 4;
                    continue;
                }
            }
            k += 1;
        }
    }
    sacs
}

fn stop_and_join(stop: &Arc<AtomicBool>, handle: &mut Option<std::thread::JoinHandle<()>>) {
    if let Some(h) = handle.take() {
        stop.store(true, Ordering::Relaxed);
        let _ = h.join();
    }
    stop.store(false, Ordering::Relaxed);
}
