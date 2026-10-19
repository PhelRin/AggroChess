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
                out!("id name AggroChess 3.3");
                out!("id author Antigravity & Kevin");
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
                        _ => {} // all other settings are fixed
                    }
                }
            }
            "ucinewgame" => {
                stop_and_join(&stop_flag, &mut search_thread);
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
                if let Some(start) = moves_index {
                    for &mv_str in &tokens[start.min(tokens.len())..] {
                        match cozy_chess::util::parse_uci_move(&board, mv_str) {
                            Ok(mv) if board.is_legal(mv) => {
                                board.play_unchecked(mv);
                                history_hashes.push(board.hash());
                            }
                            _ => {
                                out!("info string Illegal move in position: {}", mv_str);
                                break;
                            }
                        }
                    }
                }
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
                out!("eval (side to move): {}", crate::eval::evaluate(&board));
            }
            _ => {}
        }
        let _ = io::stdout().flush();
    }
}

fn stop_and_join(stop: &Arc<AtomicBool>, handle: &mut Option<std::thread::JoinHandle<()>>) {
    if let Some(h) = handle.take() {
        stop.store(true, Ordering::Relaxed);
        let _ = h.join();
    }
    stop.store(false, Ordering::Relaxed);
}
