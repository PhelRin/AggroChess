use cozy_chess::{Board, Color, Piece, BitBoard, Square};
use std::io::{self, BufRead};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use crate::book::Book;
use crate::search::iterative_deepening;

pub fn uci_loop() {
    let mut board = Board::default();
    let mut book = Book::new("ph-gambitbook.bin");
    let mut history_hashes = vec![board.hash()];
    
    let stop_flag = Arc::new(AtomicBool::new(false));
    let mut search_thread: Option<std::thread::JoinHandle<()>> = None;

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        let cmd = tokens[0];

        match cmd {
            "uci" => {
                println!("id name AggroChess");
                println!("id author Antigravity & Kevin");
                println!("option name BookPath type string default ph-gambitbook.bin");
                println!("option name Hash type spin default 16 min 1 max 1024");
                println!("option name Threads type spin default {} min 1 max 128", crate::search::NUM_THREADS.load(Ordering::Relaxed));
                println!("uciok");
            }
            "isready" => {
                println!("readyok");
            }
            "setoption" => {
                // Parse options
                if tokens.len() >= 5 && tokens[1] == "name" && tokens[3] == "value" {
                    let name = tokens[2];
                    let value = tokens[4];
                    if name == "BookPath" {
                        book = Book::new(value);
                    } else if name == "Hash" {
                        if let Ok(mb) = value.parse::<usize>() {
                            crate::search::TT_SIZE_MB.store(mb, Ordering::Relaxed);
                            println!("info string Option Hash set to {} MB", mb);
                        }
                    } else if name == "Threads" {
                        if let Ok(threads) = value.parse::<usize>() {
                            crate::search::NUM_THREADS.store(threads, Ordering::Relaxed);
                            println!("info string Option Threads set to {}", threads);
                        }
                    }
                }
            }
            "ucinewgame" => {
                board = Board::default();
                history_hashes.clear();
                history_hashes.push(board.hash());
                stop_flag.store(false, Ordering::Relaxed);
            }
            "position" => {
                // Stop search first if running
                stop_flag.store(true, Ordering::Relaxed);
                if let Some(t) = search_thread.take() {
                    let _ = t.join();
                }
                stop_flag.store(false, Ordering::Relaxed);

                history_hashes.clear();

                // Parse board and moves
                let mut moves_index = None;
                if tokens.len() >= 2 {
                    if tokens[1] == "startpos" {
                        board = Board::default();
                        history_hashes.push(board.hash());
                        if tokens.len() >= 3 && tokens[2] == "moves" {
                            moves_index = Some(3);
                        }
                    } else if tokens[1] == "fen" {
                        // Collect FEN parts until "moves" or end of line
                        let mut fen_parts = Vec::new();
                        let mut next_idx = 2;
                        while next_idx < tokens.len() && tokens[next_idx] != "moves" {
                            fen_parts.push(tokens[next_idx]);
                            next_idx += 1;
                        }
                        let fen = fen_parts.join(" ");
                        match fen.parse::<Board>() {
                            Ok(b) => {
                                board = b;
                                history_hashes.push(board.hash());
                            }
                            Err(e) => println!("info string Error parsing FEN: {:?}", e),
                        }
                        if next_idx < tokens.len() && tokens[next_idx] == "moves" {
                            moves_index = Some(next_idx + 1);
                        }
                    }
                }

                // Play moves
                if let Some(start) = moves_index {
                    for &mv_str in &tokens[start..] {
                        match cozy_chess::util::parse_uci_move(&board, mv_str) {
                            Ok(mv) => {
                                if board.is_legal(mv) {
                                    board.play(mv);
                                    history_hashes.push(board.hash());
                                } else {
                                    println!("info string Illegal move in position: {}", mv_str);
                                    break;
                                }
                            }
                            Err(e) => {
                                println!("info string Error parsing move {}: {:?}", mv_str, e);
                                break;
                            }
                        }
                    }
                }
            }
            "go" => {
                // Stop search first if running
                stop_flag.store(true, Ordering::Relaxed);
                if let Some(t) = search_thread.take() {
                    let _ = t.join();
                }
                stop_flag.store(false, Ordering::Relaxed);

                // Check opening book first
                if let Some(book_move_str) = book.get_move(&board) {
                    if let Ok(mv) = cozy_chess::util::parse_uci_move(&board, &book_move_str) {
                        if board.is_legal(mv) {
                            println!("bestmove {}", cozy_chess::util::display_uci_move(&board, mv));
                            continue;
                        }
                    }
                }

                // Parse go parameters
                let mut depth = 100;
                let mut time_limit = None;
                let mut infinite = false;

                let mut wtime = None;
                let mut btime = None;
                let mut winc = None;
                let mut binc = None;
                let mut movetime = None;

                let mut idx = 1;
                while idx < tokens.len() {
                    match tokens[idx] {
                        "infinite" => infinite = true,
                        "depth" => {
                            if idx + 1 < tokens.len() {
                                if let Ok(d) = tokens[idx+1].parse::<i16>() {
                                    depth = d;
                                }
                                idx += 1;
                            }
                        }
                        "movetime" => {
                            if idx + 1 < tokens.len() {
                                if let Ok(t) = tokens[idx+1].parse::<u64>() {
                                    movetime = Some(t);
                                }
                                idx += 1;
                            }
                        }
                        "wtime" => {
                            if idx + 1 < tokens.len() {
                                wtime = tokens[idx+1].parse::<u64>().ok();
                                idx += 1;
                            }
                        }
                        "btime" => {
                            if idx + 1 < tokens.len() {
                                btime = tokens[idx+1].parse::<u64>().ok();
                                idx += 1;
                            }
                        }
                        "winc" => {
                            if idx + 1 < tokens.len() {
                                winc = tokens[idx+1].parse::<u64>().ok();
                                idx += 1;
                            }
                        }
                        "binc" => {
                            if idx + 1 < tokens.len() {
                                binc = tokens[idx+1].parse::<u64>().ok();
                                idx += 1;
                            }
                        }
                        _ => {}
                    }
                    idx += 1;
                }

                // Time calculation
                if let Some(mt) = movetime {
                    time_limit = Some(Duration::from_millis(mt.saturating_sub(20)));
                } else if !infinite {
                    let side = board.side_to_move();
                    let (t, inc) = if side == Color::White {
                        (wtime, winc.unwrap_or(0))
                    } else {
                        (btime, binc.unwrap_or(0))
                    };
                    if let Some(time_remaining) = t {
                        let mut alloc = time_remaining / 30 + inc / 2;
                        if is_attacking_state(&board) {
                            alloc = (alloc * 3) / 2;
                        }
                        let alloc = alloc.min(time_remaining / 2).max(10);
                        time_limit = Some(Duration::from_millis(alloc));
                    }
                }

                // Spawn search in background thread
                let search_board = board.clone();
                let stop_flag_clone = Arc::clone(&stop_flag);
                let history_hashes_clone = history_hashes.clone();
                
                search_thread = Some(std::thread::spawn(move || {
                    let (best_move, _) = iterative_deepening(&search_board, &history_hashes_clone, depth, time_limit, stop_flag_clone);
                    if let Some(mv) = best_move {
                        println!("bestmove {}", cozy_chess::util::display_uci_move(&search_board, mv));
                    } else {
                        // Fallback to any legal move
                        let mut fallback_moves = Vec::new();
                        search_board.generate_moves(|mvs| {
                            for m in mvs {
                                fallback_moves.push(m);
                            }
                            false
                        });
                        
                        // Find first legal move
                        let mut chosen = None;
                        for m in fallback_moves {
                            if search_board.is_legal(m) {
                                chosen = Some(m);
                                break;
                            }
                        }
                        
                        if let Some(m) = chosen {
                            println!("bestmove {}", cozy_chess::util::display_uci_move(&search_board, m));
                        } else {
                            println!("bestmove 0000"); // No legal moves (game is over)
                        }
                    }
                }));
            }
            "stop" => {
                stop_flag.store(true, Ordering::Relaxed);
                if let Some(t) = search_thread.take() {
                    let _ = t.join();
                }
                stop_flag.store(false, Ordering::Relaxed);
            }
            "quit" => {
                stop_flag.store(true, Ordering::Relaxed);
                if let Some(t) = search_thread.take() {
                    let _ = t.join();
                }
                break;
            }
            _ => {
                // Ignore unknown commands
            }
        }
    }
}

pub fn is_attacking_state(board: &Board) -> bool {
    let side = board.side_to_move();
    let enemy_king = board.king(!side);
    let enemy_king_ring = cozy_chess::get_king_moves(enemy_king) | BitBoard(1 << (enemy_king as u64));
    let occupied = board.occupied();
    
    let mut attacking_pieces = 0;
    for &piece in &[Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen] {
        let mut bb = board.colored_pieces(side, piece);
        while bb.0 != 0 {
            let sq = bb.0.trailing_zeros();
            let square = Square::ALL[sq as usize];
            bb.0 &= bb.0 - 1;
            let attacks = match piece {
                Piece::Knight => cozy_chess::get_knight_moves(square),
                Piece::Bishop => cozy_chess::get_bishop_moves(square, occupied),
                Piece::Rook => cozy_chess::get_rook_moves(square, occupied),
                Piece::Queen => cozy_chess::get_bishop_moves(square, occupied) | cozy_chess::get_rook_moves(square, occupied),
                _ => BitBoard(0),
            };
            if (attacks.0 & enemy_king_ring.0) != 0 {
                attacking_pieces += 1;
            }
        }
    }
    
    let pawns = board.colored_pieces(side, Piece::Pawn);
    let mut pawn_bb = pawns.0;
    while pawn_bb != 0 {
        let sq = pawn_bb.trailing_zeros();
        pawn_bb &= pawn_bb - 1;
        let file = sq % 8;
        let rank = sq / 8;
        let mut attacks = 0u64;
        if side == Color::White {
            if rank < 7 {
                if file > 0 { attacks |= 1 << ((rank + 1) * 8 + (file - 1)); }
                if file < 7 { attacks |= 1 << ((rank + 1) * 8 + (file + 1)); }
            }
        } else {
            if rank > 0 {
                if file > 0 { attacks |= 1 << ((rank - 1) * 8 + (file - 1)); }
                if file < 7 { attacks |= 1 << ((rank - 1) * 8 + (file + 1)); }
            }
        }
        if (attacks & enemy_king_ring.0) != 0 {
            attacking_pieces += 1;
        }
    }
    
    let state = crate::eval::EvalState::new(board);
    let (our_mat, enemy_mat) = if side == Color::White {
        (state.w_material, state.b_material)
    } else {
        (state.b_material, state.w_material)
    };
    let has_material_deficit = our_mat < enemy_mat;

    attacking_pieces >= 2 || has_material_deficit
}
