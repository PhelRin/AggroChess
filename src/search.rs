use cozy_chess::{Board, Piece, Move, Square, Color, BitBoard};
use std::sync::atomic::{AtomicBool, Ordering, AtomicUsize, AtomicU64, AtomicI32};
use std::sync::Arc;
use std::time::{Instant, Duration};

pub static TT_SIZE_MB: AtomicUsize = AtomicUsize::new(16);
pub static NUM_THREADS: AtomicUsize = AtomicUsize::new(1);
pub static CONTEMPT: AtomicI32 = AtomicI32::new(20);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TTFlag {
    Exact,
    LowerBound, // beta cutoff
    UpperBound, // alpha cutoff
}

#[derive(Clone, Copy, Debug)]
pub struct TTEntryView {
    pub hash: u64,
    pub best_move: Option<Move>,
    pub score: i32,
    pub depth: i16,
    pub flag: TTFlag,
}

pub struct TTEntry {
    pub hash_depth: AtomicU64,
    pub data_depth: AtomicU64,
    pub hash_always: AtomicU64,
    pub data_always: AtomicU64,
}

pub struct TranspositionTable {
    table: Vec<TTEntry>,
    mask: u64,
}

fn pack_move(mv: Option<Move>) -> u16 {
    match mv {
        None => 0,
        Some(m) => {
            let from = m.from as u16;
            let to = m.to as u16;
            let promo = match m.promotion {
                None => 0,
                Some(Piece::Knight) => 1,
                Some(Piece::Bishop) => 2,
                Some(Piece::Rook) => 3,
                Some(Piece::Queen) => 4,
                _ => 0,
            };
            (1 << 15) | (from << 9) | (to << 3) | promo
        }
    }
}

fn unpack_move(val: u16) -> Option<Move> {
    if (val & (1 << 15)) == 0 {
        None
    } else {
        let from = Square::ALL[((val >> 9) & 0x3F) as usize];
        let to = Square::ALL[((val >> 3) & 0x3F) as usize];
        let promo = match val & 0x7 {
            1 => Some(Piece::Knight),
            2 => Some(Piece::Bishop),
            3 => Some(Piece::Rook),
            4 => Some(Piece::Queen),
            _ => None,
        };
        Some(Move { from, to, promotion: promo })
    }
}

fn pack_data2(best_move: Option<Move>, score: i32, depth: i16, flag: TTFlag) -> u64 {
    let mv_val = pack_move(best_move) as u64; // 16 bits
    let score_val = score.clamp(-32000, 32000) as i16 as u16 as u64; // 16 bits
    let depth_val = depth.clamp(-128, 127) as i8 as u8 as u64; // 8 bits
    let flag_val = match flag {
        TTFlag::Exact => 0u64,
        TTFlag::LowerBound => 1u64,
        TTFlag::UpperBound => 2u64,
    }; // 2 bits
    mv_val | (score_val << 16) | (depth_val << 32) | (flag_val << 40)
}

fn unpack_data2(val: u64) -> (Option<Move>, i32, i16, TTFlag) {
    let best_move = unpack_move((val & 0xFFFF) as u16);
    let score = (val >> 16) as u16 as i16 as i32;
    let depth = (val >> 32) as u8 as i8 as i16;
    let flag = match (val >> 40) & 3 {
        0 => TTFlag::Exact,
        1 => TTFlag::LowerBound,
        2 => TTFlag::UpperBound,
        _ => TTFlag::Exact,
    };
    (best_move, score, depth, flag)
}

impl TranspositionTable {
    pub fn new(mb: usize) -> Self {
        let entry_size = std::mem::size_of::<TTEntry>(); // 32 bytes
        let mut num_entries = (mb * 1024 * 1024) / entry_size;
        if num_entries == 0 {
            num_entries = 1;
        }
        let power = num_entries.next_power_of_two();
        let size = if power > num_entries && power > 1 {
            power >> 1
        } else {
            power
        };
        let mut table = Vec::with_capacity(size);
        for _ in 0..size {
            table.push(TTEntry {
                hash_depth: AtomicU64::new(0),
                data_depth: AtomicU64::new(0),
                hash_always: AtomicU64::new(0),
                data_always: AtomicU64::new(0),
            });
        }
        Self {
            table,
            mask: (size - 1) as u64,
        }
    }

    pub fn lookup(&self, hash: u64) -> Option<TTEntryView> {
        let idx = (hash & self.mask) as usize;
        let entry = &self.table[idx];
        
        let hash_d = entry.hash_depth.load(Ordering::Acquire);
        if hash_d == hash {
            let data = entry.data_depth.load(Ordering::Relaxed);
            let (best_move, score, depth, flag) = unpack_data2(data);
            return Some(TTEntryView {
                hash,
                best_move,
                score,
                depth,
                flag,
            });
        }
        
        let hash_a = entry.hash_always.load(Ordering::Acquire);
        if hash_a == hash {
            let data = entry.data_always.load(Ordering::Relaxed);
            let (best_move, score, depth, flag) = unpack_data2(data);
            return Some(TTEntryView {
                hash,
                best_move,
                score,
                depth,
                flag,
            });
        }
        None
    }

    pub fn store(&self, hash: u64, best_move: Option<Move>, score: i32, depth: i16, flag: TTFlag) {
        let idx = (hash & self.mask) as usize;
        let entry = &self.table[idx];
        
        let old_hash_d = entry.hash_depth.load(Ordering::Relaxed);
        let old_data_d = entry.data_depth.load(Ordering::Relaxed);
        let (_, _, old_depth_d, _) = unpack_data2(old_data_d);
        
        let replace_d = old_hash_d == 0 || depth >= old_depth_d;
        if replace_d {
            let data = pack_data2(best_move, score, depth, flag);
            entry.data_depth.store(data, Ordering::Relaxed);
            entry.hash_depth.store(hash, Ordering::Release);
        } else {
            let data = pack_data2(best_move, score, depth, flag);
            entry.data_always.store(data, Ordering::Relaxed);
            entry.hash_always.store(hash, Ordering::Release);
        }
    }
}

pub struct Searcher {
    pub stop: Arc<AtomicBool>,
    pub start_time: Instant,
    pub time_limit: Option<Duration>,
    pub nodes: Arc<AtomicU64>,
    pub local_nodes: u64,
    pub tt: Arc<TranspositionTable>,
    pub history: [[i32; 64]; 64],
    pub countermove_table: [[Option<Move>; 64]; 64],
    pub capture_history: [[i32; 64]; 64],
    pub threat_history: [[i32; 64]; 64],
    pub counter_history: Box<[i32]>,
    pub followup_history: Box<[i32]>,
    pub killers: [[Option<Move>; 2]; 128],
    pub moves_played: [Option<Move>; 128],
    pub excluded_move: Option<Move>,
    pub seldepth: i16,
    pub root_best_move: Option<Move>,
    pub path_hashes: [u64; 1024],
    pub path_len: usize,
    pub extensions_at_ply: [i16; 128],
}

impl Drop for Searcher {
    fn drop(&mut self) {
        let remainder = self.local_nodes & 1023;
        if remainder > 0 {
            self.nodes.fetch_add(remainder, Ordering::Relaxed);
        }
    }
}


impl Searcher {
    pub fn new(stop: Arc<AtomicBool>, time_limit: Option<Duration>) -> Self {
        let size_mb = TT_SIZE_MB.load(Ordering::Relaxed);
        Self {
            stop,
            start_time: Instant::now(),
            time_limit,
            nodes: Arc::new(AtomicU64::new(0)),
            local_nodes: 0,
            tt: Arc::new(TranspositionTable::new(size_mb)),
            history: [[0; 64]; 64],
            countermove_table: [[None; 64]; 64],
            capture_history: [[0; 64]; 64],
            threat_history: [[0; 64]; 64],
            counter_history: vec![0; 262144].into_boxed_slice(),
            followup_history: vec![0; 262144].into_boxed_slice(),
            killers: [[None; 2]; 128],
            moves_played: [None; 128],
            excluded_move: None,
            seldepth: 0,
            root_best_move: None,
            path_hashes: [0; 1024],
            path_len: 0,
            extensions_at_ply: [0; 128],
        }
    }

    pub fn new_with_shared_tt(stop: Arc<AtomicBool>, time_limit: Option<Duration>, tt: Arc<TranspositionTable>, nodes: Arc<AtomicU64>) -> Self {
        Self {
            stop,
            start_time: Instant::now(),
            time_limit,
            nodes,
            local_nodes: 0,
            tt,
            history: [[0; 64]; 64],
            countermove_table: [[None; 64]; 64],
            capture_history: [[0; 64]; 64],
            threat_history: [[0; 64]; 64],
            counter_history: vec![0; 262144].into_boxed_slice(),
            followup_history: vec![0; 262144].into_boxed_slice(),
            killers: [[None; 2]; 128],
            moves_played: [None; 128],
            excluded_move: None,
            seldepth: 0,
            root_best_move: None,
            path_hashes: [0; 1024],
            path_len: 0,
            extensions_at_ply: [0; 128],
        }
    }

    pub fn is_repetition(&self, hash: u64) -> bool {
        if self.path_len < 2 {
            return false;
        }
        for i in (0..self.path_len - 1).rev() {
            if self.path_hashes[i] == hash {
                return true;
            }
        }
        false
    }

    pub fn search(&mut self, board: &Board, eval_state: &crate::eval::EvalState, mut depth: i16, mut alpha: i32, mut beta: i32, ply: usize) -> i32 {
        self.local_nodes += 1;
        self.seldepth = self.seldepth.max(ply as i16);

        if self.local_nodes & 1023 == 0 {
            self.nodes.fetch_add(1024, Ordering::Relaxed);
            if let Some(limit) = self.time_limit {
                if self.start_time.elapsed() >= limit {
                    self.stop.store(true, Ordering::Relaxed);
                }
            }
        }

        if self.stop.load(Ordering::Relaxed) {
            return 0;
        }

        // Mate distance pruning
        let mated_score = -30000 + ply as i32;
        let mating_score = 30000 - ply as i32;
        if mated_score >= beta {
            return beta;
        }
        if mating_score <= alpha {
            return alpha;
        }
        alpha = alpha.max(mated_score);
        beta = beta.min(mating_score);

        let hash = board.hash();
        if ply > 0 && (board.halfmove_clock() >= 100 || self.is_repetition(hash)) {
            let contempt_val = CONTEMPT.load(Ordering::Relaxed);
            let mut dynamic_contempt = contempt_val;
            let piece_count = board.occupied().0.count_ones() as i32;
            if piece_count > 26 {
                dynamic_contempt += 25;
            } else if piece_count > 16 {
                dynamic_contempt += 10;
            } else {
                dynamic_contempt -= 15;
            }
            let deficit = if board.side_to_move() == Color::White {
                (eval_state.b_material - eval_state.w_material).max(0)
            } else {
                (eval_state.w_material - eval_state.b_material).max(0)
            };
            if deficit > 0 {
                dynamic_contempt += (deficit / 4).min(80);
            }
            dynamic_contempt = dynamic_contempt.max(0);
            let draw_score = if ply % 2 == 0 { -dynamic_contempt } else { dynamic_contempt };
            return draw_score;
        }

        if depth <= 0 {
            return self.quiescence(board, eval_state, alpha, beta, ply);
        }

        let mut tt_move = None;
        let mut tt_entry = None;
        let mut tt_score = 0;

        if let Some(entry) = self.tt.lookup(hash) {
            tt_move = entry.best_move;
            tt_entry = Some(entry);
            let score = if entry.score > 29000 {
                entry.score - ply as i32
            } else if entry.score < -29000 {
                entry.score + ply as i32
            } else {
                entry.score
            };
            tt_score = score;

            let is_mate = score.abs() > 29000;
            if is_mate || entry.depth >= depth {
                match entry.flag {
                    TTFlag::Exact => return score,
                    TTFlag::LowerBound => {
                        if score >= beta {
                            return score;
                        }
                    }
                    TTFlag::UpperBound => {
                        if score <= alpha {
                            return score;
                        }
                    }
                }
            }
        }

        let mut singular_extended = false;
        let mut double_singular = false;
        if depth >= 7 && ply > 0 && tt_move.is_some() && tt_entry.is_some() && self.excluded_move.is_none() {
            let entry = tt_entry.unwrap();
            if entry.depth >= depth - 3 && entry.flag != TTFlag::UpperBound {
                let tt_move_val = tt_move.unwrap();
                let side = board.side_to_move();
                let enemy_king = board.king(!side);
                let enemy_king_ring = cozy_chess::get_king_moves(enemy_king) | BitBoard(1 << (enemy_king as u64));
                let moved_piece = board.piece_on(tt_move_val.from).unwrap_or(Piece::Pawn);
                let occupied = board.occupied();
                let piece_attacks = match moved_piece {
                    Piece::Knight => cozy_chess::get_knight_moves(tt_move_val.to),
                    Piece::Bishop => cozy_chess::get_bishop_moves(tt_move_val.to, occupied),
                    Piece::Rook => cozy_chess::get_rook_moves(tt_move_val.to, occupied),
                    Piece::Queen => cozy_chess::get_bishop_moves(tt_move_val.to, occupied) | cozy_chess::get_rook_moves(tt_move_val.to, occupied),
                    Piece::Pawn => {
                        let file = tt_move_val.to.file() as i32;
                        let rank = tt_move_val.to.rank() as i32;
                        let mut p_attacks = 0u64;
                        let dir = if side == Color::White { 1 } else { -1 };
                        let target_rank = rank + dir;
                        if target_rank >= 0 && target_rank < 8 {
                            if file > 0 {
                                p_attacks |= 1 << (target_rank * 8 + (file - 1));
                            }
                            if file < 7 {
                                p_attacks |= 1 << (target_rank * 8 + (file + 1));
                            }
                        }
                        BitBoard(p_attacks)
                    }
                    _ => BitBoard(0),
                };
                let attacks_king = (piece_attacks.0 & enemy_king_ring.0) != 0;
                let is_capture = board.piece_on(tt_move_val.to).is_some() || 
                    (moved_piece == Piece::Pawn && tt_move_val.from.file() != tt_move_val.to.file());

                let is_attacking_move = attacks_king || is_capture;
                let margin = if is_attacking_move { depth as i32 } else { 2 * depth as i32 };
                let singular_beta = tt_score - margin;

                self.excluded_move = Some(tt_move_val);
                let s_score = self.search(board, eval_state, depth - 4, singular_beta - 1, singular_beta, ply);
                self.excluded_move = None;
                if s_score < singular_beta {
                    singular_extended = true;
                    if s_score < singular_beta - 300 {
                        double_singular = true;
                    }
                }
            }
        }

        if ply == 0 {
            self.extensions_at_ply[0] = 0;
        }
        let in_check = !board.checkers().is_empty();
        if in_check {
            let current_exts = self.extensions_at_ply[ply.min(127)];
            if current_exts < 2 {
                depth += 1;
                self.extensions_at_ply[ply.min(127)] = current_exts + 1;
            }
        }

        let mut attacking_king = false;
        if !in_check && ply > 0 {
            let side = board.side_to_move();
            let enemy_king = board.king(match side {
                Color::White => Color::Black,
                Color::Black => Color::White,
            });
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
            if attacking_pieces >= 2 {
                attacking_king = true;
            }
        }

        // Reverse Futility Pruning (RFP) / Static Null Move Pruning
        if depth <= 3 && !in_check && ply > 0 && !attacking_king {
            let mut static_eval = crate::eval::evaluate_incremental(board, eval_state);
            if crate::eval::has_material_imbalance(board) {
                let sign = if ply % 2 == 0 { 1 } else { -1 };
                static_eval += sign * 40;
            }
            let margin = depth as i32 * 120;
            if static_eval - margin >= beta {
                return static_eval - margin;
            }
        }

        let enemy_king = board.king(match board.side_to_move() {
            Color::White => Color::Black,
            Color::Black => Color::White,
        });
        let mut dist_2_mask = BitBoard(1 << (enemy_king as u64));
        dist_2_mask |= cozy_chess::get_king_moves(enemy_king);
        let mut temp = dist_2_mask;
        while temp.0 != 0 {
            let sq = Square::ALL[temp.0.trailing_zeros() as usize];
            temp.0 &= temp.0 - 1;
            dist_2_mask |= cozy_chess::get_king_moves(sq);
        }
        let proximity_nmp_bypass = (board.colors(board.side_to_move()).0 & dist_2_mask.0) != 0;

        let mut opponent_has_mate_threat = false;
        // Null Move Pruning
        // Null Move Pruning
        if depth >= 3 && !in_check && ply > 0 && !attacking_king && !proximity_nmp_bypass {
            let major_pieces = board.colors(board.side_to_move()).0 & 
                !(board.pieces(Piece::Pawn).0 | board.pieces(Piece::King).0);
            if major_pieces != 0 {
                if let Some(null_board) = board.null_move() {
                    let pushed = if self.path_len < 1024 {
                        self.path_hashes[self.path_len] = hash;
                        self.path_len += 1;
                        true
                    } else {
                        false
                    };
                    
                    let nmp_reduction = 2 + (depth / 4);
                    let null_score = -self.search(&null_board, eval_state, depth - 1 - nmp_reduction, -beta, -beta + 1, ply + 1);
                    
                    if pushed {
                        self.path_len -= 1;
                    }
                    
                    if null_score >= beta {
                        if depth >= 6 {
                            let verify_depth = depth - 1 - nmp_reduction;
                            let verify_score = self.search(board, eval_state, verify_depth, beta - 1, beta, ply);
                            if verify_score >= beta {
                                return beta;
                            }
                        } else {
                            return beta;
                        }
                    } else if null_score < -29000 + (ply as i32) {
                        opponent_has_mate_threat = true;
                    }
                }
            }
        }

        let dummy = Move {
            from: Square::A1,
            to: Square::A1,
            promotion: None,
        };

        let mut moves = [dummy; 256];
        let mut moves_len = 0;
        board.generate_moves(|mvs| {
            for m in mvs {
                if moves_len < 256 {
                    moves[moves_len] = m;
                    moves_len += 1;
                }
            }
            false
        });

        if moves_len == 0 {
            if in_check {
                return -30000 + ply as i32;
            } else {
                return 0;
            }
        }

        let mut scores = [0i32; 256];
        for i in 0..moves_len {
            scores[i] = score_move(moves[i], board, tt_move, ply, self);
        }

        // Insertion sort
        for i in 1..moves_len {
            let key_mv = moves[i];
            let key_score = scores[i];
            let mut j = i;
            while j > 0 && scores[j - 1] < key_score {
                moves[j] = moves[j - 1];
                scores[j] = scores[j - 1];
                j -= 1;
            }
            moves[j] = key_mv;
            scores[j] = key_score;
        }

        let is_king_hunt = {
            let enemy_color = match board.side_to_move() {
                Color::White => Color::Black,
                Color::Black => Color::White,
            };
            let enemy_king = board.king(enemy_color);
            let is_dragged = if enemy_color == Color::Black {
                enemy_king.rank() as usize <= 3
            } else {
                enemy_king.rank() as usize >= 4
            };
            let pawns = board.colored_pieces(enemy_color, Piece::Pawn);
            let shield_files: &[usize] = if enemy_king.file() as usize >= 5 {
                &[5, 6, 7]
            } else if enemy_king.file() as usize <= 2 {
                &[0, 1, 2]
            } else {
                &[3, 4]
            };
            let mut missing_count = 0;
            for &file in shield_files {
                let mut has_pawn = false;
                if enemy_color == Color::Black {
                    for r in 4..=6 {
                        let sq = Square::new(cozy_chess::File::ALL[file], cozy_chess::Rank::ALL[r]);
                        if pawns.has(sq) {
                            has_pawn = true;
                            break;
                        }
                    }
                } else {
                    for r in 1..=3 {
                        let sq = Square::new(cozy_chess::File::ALL[file], cozy_chess::Rank::ALL[r]);
                        if pawns.has(sq) {
                            has_pawn = true;
                            break;
                        }
                    }
                }
                if !has_pawn {
                    missing_count += 1;
                }
            }
            is_dragged || missing_count == shield_files.len()
        };

        let futility_pruning = if depth <= 2 && !in_check {
            let mut static_eval = crate::eval::evaluate_incremental(board, eval_state);
            if crate::eval::has_material_imbalance(board) {
                let sign = if ply % 2 == 0 { 1 } else { -1 };
                static_eval += sign * 40;
            }
            static_eval + (depth as i32) * 150 < alpha
        } else {
            false
        };

        let mut best_move = None;
        let mut best_score = -30000;
        let mut flag = TTFlag::UpperBound;
        let mut moves_searched = 0;
        let mut quiet_moves_searched = 0;
        let mut opponent_scores = Vec::new();

        for i in 0..moves_len {
            let mv = moves[i];
            if Some(mv) == self.excluded_move {
                continue;
            }
            if !board.is_legal(mv) {
                continue;
            }

            let is_capture = board.piece_on(mv.to).is_some() || 
                (board.piece_on(mv.from) == Some(Piece::Pawn) && mv.from.file() != mv.to.file());
            let is_promo = mv.promotion.is_some();

            let mut next_board = board.clone();
            next_board.play(mv);
            let next_eval_state = eval_state.update(board, mv);

            // Do not reduce moves that attack the enemy king safety ring
            let enemy_king = board.king(match board.side_to_move() {
                Color::White => Color::Black,
                Color::Black => Color::White,
            });
            let enemy_king_ring = cozy_chess::get_king_moves(enemy_king) | BitBoard(1 << (enemy_king as u64));
            let moved_piece = board.piece_on(mv.from).unwrap_or(Piece::Pawn);
            let piece_attacks = match moved_piece {
                Piece::Knight => cozy_chess::get_knight_moves(mv.to),
                Piece::Bishop => cozy_chess::get_bishop_moves(mv.to, next_board.occupied()),
                Piece::Rook => cozy_chess::get_rook_moves(mv.to, next_board.occupied()),
                Piece::Queen => cozy_chess::get_bishop_moves(mv.to, next_board.occupied()) | cozy_chess::get_rook_moves(mv.to, next_board.occupied()),
                Piece::Pawn => {
                    let file = mv.to.file() as i32;
                    let rank = mv.to.rank() as i32;
                    let mut p_attacks = 0u64;
                    let dir = if board.side_to_move() == Color::White { 1 } else { -1 };
                    let target_rank = rank + dir;
                    if target_rank >= 0 && target_rank < 8 {
                        if file > 0 {
                            p_attacks |= 1 << (target_rank * 8 + (file - 1));
                        }
                        if file < 7 {
                            p_attacks |= 1 << (target_rank * 8 + (file + 1));
                        }
                    }
                    BitBoard(p_attacks)
                }
                _ => BitBoard(0),
            };
            let attacks_king = (piece_attacks.0 & enemy_king_ring.0) != 0;

            let is_killer = ply < 128 && (Some(mv) == self.killers[ply][0] || Some(mv) == self.killers[ply][1]);
            let gives_check = !next_board.checkers().is_empty();

            // Futility Pruning
            if futility_pruning && !is_capture && !is_promo && !gives_check && !attacks_king && !is_killer {
                continue;
            }

            // Depth-Based Late Move Pruning (LMP)
            if moves_searched > 0 && depth < 4 && !in_check && !is_capture && !is_promo && !gives_check && !attacks_king && !is_killer {
                if quiet_moves_searched >= 2 + (depth as i32) * (depth as i32) {
                    continue;
                }
            }

            // SEE-Based Quiet Move Pruning (Quiet SEE)
            if moves_searched > 0 && depth >= 1 && !is_capture && !is_promo {
                let threshold = if attacks_king { -80 } else { 0 };
                if see(board, mv) < threshold {
                    continue;
                }
            }

            moves_searched += 1;
            if !is_capture && !is_promo {
                quiet_moves_searched += 1;
            }

            let pushed = if self.path_len < 1024 {
                self.path_hashes[self.path_len] = hash;
                self.path_len += 1;
                true
            } else {
                false
            };

            self.moves_played[ply] = Some(mv);

            // ProbCut Pruning
            if depth >= 5 && !in_check && beta.abs() < 29000 && (is_capture || is_promo) && see(board, mv) >= 0 {
                let pc_beta = (beta + 200).min(29000);
                let pc_score = -self.search(&next_board, &next_eval_state, depth - 4, -pc_beta, -pc_beta + 1, ply + 1);
                if pc_score >= pc_beta {
                    if pushed {
                        self.path_len -= 1;
                    }
                    self.moves_played[ply] = None;
                    return pc_beta;
                }
            }
            let mut score;
            let is_killer = ply < 128 && (Some(mv) == self.killers[ply][0] || Some(mv) == self.killers[ply][1]);

            let mut next_depth = depth - 1;
            let gives_check = !next_board.checkers().is_empty();
            let side = board.side_to_move();
            let enters_king_ring = (1 << (mv.to as u64)) & enemy_king_ring.0 != 0;
            let enemy_queens = board.pieces(Piece::Queen) & board.colors(!side);
            let enemy_rooks = board.pieces(Piece::Rook) & board.colors(!side);
            let enemy_major_pieces = enemy_queens | enemy_rooks;
            let targets_major_piece = (piece_attacks.0 & enemy_major_pieces.0) != 0;

            let is_attacking_extension = ply % 2 == 0 && (gives_check || enters_king_ring || targets_major_piece) && depth >= 2 && ply < 24;
            let is_king_hunt_extension = ply % 2 == 0 && is_king_hunt && 
                (gives_check || enters_king_ring || attacks_king || chebyshev_distance(mv.to, enemy_king) <= 2) && 
                depth >= 2 && ply < 24;
            let is_quiet_attack_extension = ply % 2 == 0 && !is_capture && !gives_check && !is_promo &&
                attacks_king && depth >= 3 && ply < 20;

            let mut is_recapture = false;
            if ply > 0 {
                if let Some(prev_mv) = self.moves_played[ply - 1] {
                    if is_capture && mv.to == prev_mv.to {
                        is_recapture = true;
                    }
                }
            }
            let is_recapture_extension = is_recapture && depth >= 2 && ply < 24;

            let is_ext = is_attacking_extension || is_king_hunt_extension || is_quiet_attack_extension || is_recapture_extension;
            let current_exts = self.extensions_at_ply[ply.min(127)];
            if is_ext && current_exts < 2 {
                next_depth = depth;
                self.extensions_at_ply[(ply + 1).min(127)] = current_exts + 1;
            } else {
                self.extensions_at_ply[(ply + 1).min(127)] = current_exts;
                if Some(mv) == tt_move && singular_extended && ply % 2 == 0 {
                    if double_singular && current_exts < 1 {
                        next_depth = depth + 1;
                        self.extensions_at_ply[(ply + 1).min(127)] = current_exts + 2;
                    } else if current_exts < 2 {
                        next_depth = depth;
                        self.extensions_at_ply[(ply + 1).min(127)] = current_exts + 1;
                    }
                }
            }

            if moves_searched == 1 {
                score = -self.search(&next_board, &next_eval_state, next_depth, -beta, -alpha, ply + 1);
            } else {
                let mut reduction = 0;
                if depth >= 3 && moves_searched >= 4 && !in_check && next_board.checkers().is_empty() && !is_capture && !is_promo && !attacks_king && !enters_king_ring && !targets_major_piece && !is_killer && !opponent_has_mate_threat {
                    reduction = 1;
                    if depth > 4 {
                        reduction += (depth / 4) as i16;
                    }
                    if moves_searched > 8 {
                        reduction += 1;
                    }
                    
                    let mut history_score = self.history[mv.from as usize][mv.to as usize];
                    let prev_mv = if ply > 0 { self.moves_played[ply - 1] } else { None };
                    if let Some(pmv) = prev_mv {
                        let p_to = pmv.to as usize;
                        let idx = (p_to << 12) | ((mv.from as usize) << 6) | (mv.to as usize);
                        history_score += self.counter_history[idx] / 8;
                    }
                    let followup_mv = if ply >= 2 { self.moves_played[ply - 2] } else { None };
                    if let Some(fmv) = followup_mv {
                        let f_to = fmv.to as usize;
                        let idx = (f_to << 12) | ((mv.from as usize) << 6) | (mv.to as usize);
                        history_score += self.followup_history[idx] / 8;
                    }
                    
                    if history_score > 9000 {
                        reduction -= 1;
                    } else if history_score < -4500 {
                        reduction += 1;
                        if history_score < -9000 {
                            reduction += 1;
                        }
                    }
                    
                    reduction = reduction.min(depth - 2).max(0);
                }

                score = -self.search(&next_board, &next_eval_state, next_depth - reduction, -alpha - 1, -alpha, ply + 1);

                if score > alpha && reduction > 0 {
                    score = -self.search(&next_board, &next_eval_state, next_depth, -alpha - 1, -alpha, ply + 1);
                }

                if score > alpha && score < beta {
                    score = -self.search(&next_board, &next_eval_state, next_depth, -beta, -alpha, ply + 1);
                }
            }

            self.moves_played[ply] = None;

            if pushed {
                self.path_len -= 1;
            }

            if ply == 1 && score.abs() <= 29000 {
                opponent_scores.push(score);
            }

            if score > best_score {
                best_score = score;
                best_move = Some(mv);
                if ply == 0 {
                    self.root_best_move = Some(mv);
                }
            }

            if score > alpha {
                alpha = score;
                flag = TTFlag::Exact;
            }

            if score >= beta {
                let from_idx = mv.from as usize;
                let to_idx = mv.to as usize;
                let is_capture_or_promo = is_capture || is_promo;
                if is_capture_or_promo {
                    self.capture_history[from_idx][to_idx] += (depth * depth) as i32;
                } else if ply < 128 {
                    self.killers[ply][1] = self.killers[ply][0];
                    self.killers[ply][0] = Some(mv);

                    let bonus = if attacks_king { (depth * depth) as i32 * 2 } else { (depth * depth) as i32 };

                    // Relative History Update (using Stockfish-like gravity)
                    let hist_val = self.history[from_idx][to_idx];
                    self.history[from_idx][to_idx] = hist_val + bonus - (hist_val * bonus / 16384);

                    // Penalize all quiet moves searched before this one that failed to cause a cutoff
                    for j in 0..i {
                        let prev_mv_val = moves[j];
                        if prev_mv_val == mv || Some(prev_mv_val) == self.excluded_move || !board.is_legal(prev_mv_val) {
                            continue;
                        }
                        let prev_is_capture = board.piece_on(prev_mv_val.to).is_some() || 
                            (board.piece_on(prev_mv_val.from) == Some(Piece::Pawn) && prev_mv_val.from.file() != prev_mv_val.to.file());
                        let prev_is_promo = prev_mv_val.promotion.is_some();
                        if !prev_is_capture && !prev_is_promo {
                            let p_from = prev_mv_val.from as usize;
                            let p_to = prev_mv_val.to as usize;
                            let p_val = self.history[p_from][p_to];
                            self.history[p_from][p_to] = p_val - bonus - (p_val * bonus / 16384);
                        }
                    }

                    // Threat History Update
                    let attacks_enemy_piece = {
                        let next_occupied = next_board.occupied();
                        let attacks = match moved_piece {
                            Piece::Knight => cozy_chess::get_knight_moves(mv.to),
                            Piece::Bishop => cozy_chess::get_bishop_moves(mv.to, next_occupied),
                            Piece::Rook => cozy_chess::get_rook_moves(mv.to, next_occupied),
                            Piece::Queen => cozy_chess::get_bishop_moves(mv.to, next_occupied) | cozy_chess::get_rook_moves(mv.to, next_occupied),
                            Piece::Pawn => {
                                let file = mv.to.file() as i32;
                                let rank = mv.to.rank() as i32;
                                let mut p_attacks = 0u64;
                                let dir = if board.side_to_move() == Color::White { 1 } else { -1 };
                                let target_rank = rank + dir;
                                if target_rank >= 0 && target_rank < 8 {
                                    if file > 0 { p_attacks |= 1 << (target_rank * 8 + (file - 1)); }
                                    if file < 7 { p_attacks |= 1 << (target_rank * 8 + (file + 1)); }
                                }
                                BitBoard(p_attacks)
                            }
                            _ => BitBoard(0),
                        };
                        let enemy_pieces = board.colors(!board.side_to_move());
                        (attacks.0 & enemy_pieces.0) != 0
                    };
                    if attacks_enemy_piece {
                        let t_val = self.threat_history[from_idx][to_idx];
                        self.threat_history[from_idx][to_idx] = t_val + bonus - (t_val * bonus / 16384);
                    }

                    // Countermove History Update
                    let prev_mv = if ply > 0 { self.moves_played[ply - 1] } else { None };
                    if let Some(pmv) = prev_mv {
                        let p_to = pmv.to as usize;
                        let idx = (p_to << 12) | (from_idx << 6) | to_idx;
                        let c_val = self.counter_history[idx];
                        self.counter_history[idx] = c_val + bonus - (c_val * bonus / 16384);
                        
                        for j in 0..i {
                            let prev_mv_val = moves[j];
                            if prev_mv_val == mv || Some(prev_mv_val) == self.excluded_move || !board.is_legal(prev_mv_val) {
                                continue;
                            }
                            let prev_is_capture = board.piece_on(prev_mv_val.to).is_some() || 
                                (board.piece_on(prev_mv_val.from) == Some(Piece::Pawn) && prev_mv_val.from.file() != prev_mv_val.to.file());
                            let prev_is_promo = prev_mv_val.promotion.is_some();
                            if !prev_is_capture && !prev_is_promo {
                                let p_from = prev_mv_val.from as usize;
                                let p_to_curr = prev_mv_val.to as usize;
                                let idx_prev = (p_to << 12) | (p_from << 6) | p_to_curr;
                                let val = self.counter_history[idx_prev];
                                self.counter_history[idx_prev] = val - bonus - (val * bonus / 16384);
                            }
                        }
                    }

                    // Follow-up History Update
                    let followup_mv = if ply >= 2 { self.moves_played[ply - 2] } else { None };
                    if let Some(fmv) = followup_mv {
                        let f_to = fmv.to as usize;
                        let idx = (f_to << 12) | (from_idx << 6) | to_idx;
                        let f_val = self.followup_history[idx];
                        self.followup_history[idx] = f_val + bonus - (f_val * bonus / 16384);
                        
                        for j in 0..i {
                            let prev_mv_val = moves[j];
                            if prev_mv_val == mv || Some(prev_mv_val) == self.excluded_move || !board.is_legal(prev_mv_val) {
                                continue;
                            }
                            let prev_is_capture = board.piece_on(prev_mv_val.to).is_some() || 
                                (board.piece_on(prev_mv_val.from) == Some(Piece::Pawn) && prev_mv_val.from.file() != prev_mv_val.to.file());
                            let prev_is_promo = prev_mv_val.promotion.is_some();
                            if !prev_is_capture && !prev_is_promo {
                                let p_from = prev_mv_val.from as usize;
                                let p_to_curr = prev_mv_val.to as usize;
                                let idx_prev = (f_to << 12) | (p_from << 6) | p_to_curr;
                                let val = self.followup_history[idx_prev];
                                self.followup_history[idx_prev] = val - bonus - (val * bonus / 16384);
                            }
                        }
                    }

                    // Update countermove table
                    if ply > 0 {
                        if let Some(prev_mv) = self.moves_played[ply - 1] {
                            self.countermove_table[prev_mv.from as usize][prev_mv.to as usize] = Some(mv);
                        }
                    }
                }

                let mut adjusted_score = score;
                if singular_extended && ply % 2 == 1 && score.abs() <= 29000 {
                    adjusted_score -= 25;
                }
                let score_to_store = if adjusted_score > 29000 {
                    adjusted_score + ply as i32
                } else if adjusted_score < -29000 {
                    adjusted_score - ply as i32
                } else {
                    adjusted_score
                };
                self.tt.store(hash, Some(mv), score_to_store, depth, TTFlag::LowerBound);
                return adjusted_score;
            }
        }

        if moves_searched == 0 {
            if in_check {
                return -30000 + ply as i32;
            } else {
                return 0;
            }
        }

        let mut adjusted_best_score = best_score;
        if ply == 1 && !opponent_scores.is_empty() {
            opponent_scores.sort_unstable_by(|a, b| b.cmp(a));
            let len = opponent_scores.len();
            adjusted_best_score = if len == 1 {
                opponent_scores[0]
            } else if len == 2 {
                ((opponent_scores[0] as f64 * 0.8) + (opponent_scores[1] as f64 * 0.2)).round() as i32
            } else {
                ((opponent_scores[0] as f64 * 0.7) + (opponent_scores[1] as f64 * 0.2) + (opponent_scores[2] as f64 * 0.1)).round() as i32
            };
        }
        if singular_extended && ply % 2 == 1 && adjusted_best_score.abs() <= 29000 {
            adjusted_best_score -= 25;
        }
        let score_to_store = if adjusted_best_score > 29000 {
            adjusted_best_score + ply as i32
        } else if adjusted_best_score < -29000 {
            adjusted_best_score - ply as i32
        } else {
            adjusted_best_score
        };
        self.tt.store(hash, best_move, score_to_store, depth, flag);

        adjusted_best_score
    }

    pub fn quiescence(&mut self, board: &Board, eval_state: &crate::eval::EvalState, mut alpha: i32, beta: i32, ply: usize) -> i32 {
        self.local_nodes += 1;

        if self.local_nodes & 1023 == 0 {
            self.nodes.fetch_add(1024, Ordering::Relaxed);
            if let Some(limit) = self.time_limit {
                if self.start_time.elapsed() >= limit {
                    self.stop.store(true, Ordering::Relaxed);
                }
            }
        }

        if self.stop.load(Ordering::Relaxed) {
            return 0;
        }

        let in_check = !board.checkers().is_empty();
        
        if !in_check {
            let mut stand_pat = crate::eval::evaluate_incremental(board, eval_state);
            if crate::eval::has_material_imbalance(board) {
                let sign = if ply % 2 == 0 { 1 } else { -1 };
                stand_pat += sign * 40;
            }
            if stand_pat >= beta {
                return beta;
            }
            if stand_pat > alpha {
                alpha = stand_pat;
            }
        }

        let dummy = Move {
            from: Square::A1,
            to: Square::A1,
            promotion: None,
        };

        let mut moves = [dummy; 256];
        let mut moves_len = 0;
        board.generate_moves(|mvs| {
            for m in mvs {
                if moves_len < 256 {
                    let is_ok = if in_check {
                        true
                    } else {
                        board.piece_on(m.to).is_some() || m.promotion.is_some() || 
                        (board.piece_on(m.from) == Some(Piece::Pawn) && m.from.file() != m.to.file())
                    };
                    if is_ok {
                        moves[moves_len] = m;
                        moves_len += 1;
                    }
                }
            }
            false
        });

        let mut scores = [0i32; 256];
        for i in 0..moves_len {
            scores[i] = score_move(moves[i], board, None, ply, self);
        }

        // Sort using insertion sort
        for i in 1..moves_len {
            let key_mv = moves[i];
            let key_score = scores[i];
            let mut j = i;
            while j > 0 && scores[j - 1] < key_score {
                moves[j] = moves[j - 1];
                scores[j] = scores[j - 1];
                j -= 1;
            }
            moves[j] = key_mv;
            scores[j] = key_score;
        }

        let mut has_legal_moves = false;
        for i in 0..moves_len {
            let mv = moves[i];
            if !board.is_legal(mv) {
                continue;
            }
            if !in_check {
                let is_capture = board.piece_on(mv.to).is_some() || 
                    (board.piece_on(mv.from) == Some(Piece::Pawn) && mv.from.file() != mv.to.file());
                if is_capture {
                    let them = !board.side_to_move();
                    let enemy_king = board.king(them);
                    let is_king_vicinity = chebyshev_distance(mv.to, enemy_king) <= 2;
                    if !is_king_vicinity && see(board, mv) < 0 {
                        continue;
                    }
                }
            }
            has_legal_moves = true;

            let mut next_board = board.clone();
            next_board.play(mv);
            let next_eval_state = eval_state.update(board, mv);

            let score = -self.quiescence(&next_board, &next_eval_state, -beta, -alpha, ply + 1);

            if score >= beta {
                return beta;
            }
            if score > alpha {
                alpha = score;
            }
        }

        if in_check && !has_legal_moves {
            return -30000 + ply as i32;
        }

        alpha
    }
}

#[inline(always)]
fn chebyshev_distance(sq1: Square, sq2: Square) -> i32 {
    let f1 = sq1.file() as i32;
    let r1 = sq1.rank() as i32;
    let f2 = sq2.file() as i32;
    let r2 = sq2.rank() as i32;
    (f1 - f2).abs().max((r1 - r2).abs())
}

pub fn score_move(mv: Move, board: &Board, tt_move: Option<Move>, ply: usize, searcher: &Searcher) -> i32 {
    if Some(mv) == tt_move {
        return 100000;
    }

    let from_piece = board.piece_on(mv.from);
    let to_piece = board.piece_on(mv.to);

    if let Some(promo) = mv.promotion {
        return 40000 + get_piece_val(promo);
    }

    if let Some(victim) = to_piece {
        let victim_val = get_piece_val(victim);
        let aggressor_val = get_piece_val(from_piece.unwrap_or(Piece::Pawn));
        let base = 50000 + 10 * victim_val - aggressor_val;
        let from_idx = mv.from as usize;
        let to_idx = mv.to as usize;
        return base + searcher.capture_history[from_idx][to_idx] / 100;
    }

    if from_piece == Some(Piece::Pawn) && mv.from.file() != mv.to.file() && to_piece.is_none() {
        let from_idx = mv.from as usize;
        let to_idx = mv.to as usize;
        return 50000 + 10 * 1 - 1 + searcher.capture_history[from_idx][to_idx] / 100;
    }

    if ply < 128 {
        if Some(mv) == searcher.killers[ply][0] {
            return 30000;
        }
        if Some(mv) == searcher.killers[ply][1] {
            return 29000;
        }

        let prev_mv = if ply > 0 { searcher.moves_played[ply - 1] } else { None };
        if let Some(pmv) = prev_mv {
            if Some(mv) == searcher.countermove_table[pmv.from as usize][pmv.to as usize] {
                return 28500;
            }
        }
    }

    // King-attacking move ordering: prioritize quiet moves attacking the enemy king ring
    let side = board.side_to_move();
    let enemy_king = board.king(match side {
        Color::White => Color::Black,
        Color::Black => Color::White,
    });
    let enemy_king_ring = cozy_chess::get_king_moves(enemy_king) | BitBoard(1 << (enemy_king as u64));
    let moved_piece = from_piece.unwrap_or(Piece::Pawn);
    let occupied = board.occupied();
    let piece_attacks = match moved_piece {
        Piece::Knight => cozy_chess::get_knight_moves(mv.to),
        Piece::Bishop => cozy_chess::get_bishop_moves(mv.to, occupied),
        Piece::Rook => cozy_chess::get_rook_moves(mv.to, occupied),
        Piece::Queen => cozy_chess::get_bishop_moves(mv.to, occupied) | cozy_chess::get_rook_moves(mv.to, occupied),
        Piece::Pawn => {
            let file = mv.to.file() as i32;
            let rank = mv.to.rank() as i32;
            let mut p_attacks = 0u64;
            let dir = if side == Color::White { 1 } else { -1 };
            let target_rank = rank + dir;
            if target_rank >= 0 && target_rank < 8 {
                if file > 0 {
                    p_attacks |= 1 << (target_rank * 8 + (file - 1));
                }
                if file < 7 {
                    p_attacks |= 1 << (target_rank * 8 + (file + 1));
                }
            }
            BitBoard(p_attacks)
        }
        _ => BitBoard(0),
    };
    let from_idx = mv.from as usize;
    let to_idx = mv.to as usize;

    let mut score = searcher.history[from_idx][to_idx] + searcher.threat_history[from_idx][to_idx];

    let prev_mv = if ply > 0 { searcher.moves_played[ply - 1] } else { None };
    if let Some(pmv) = prev_mv {
        let p_to = pmv.to as usize;
        let idx = (p_to << 12) | (from_idx << 6) | to_idx;
        score += searcher.counter_history[idx] / 16;
    }
    let followup_mv = if ply >= 2 { searcher.moves_played[ply - 2] } else { None };
    if let Some(fmv) = followup_mv {
        let f_to = fmv.to as usize;
        let idx = (f_to << 12) | (from_idx << 6) | to_idx;
        score += searcher.followup_history[idx] / 16;
    }

    let attacks_king_directly = (piece_attacks.0 & (1 << enemy_king as u64)) != 0;
    if attacks_king_directly {
        return 27000 + score / 10;
    }
    if (piece_attacks.0 & enemy_king_ring.0) != 0 {
        return 20000 + score / 10;
    }

    // Closer to enemy king ordering: quiet moves moving closer to the enemy king
    if moved_piece != Piece::King {
        let dist_before = chebyshev_distance(mv.from, enemy_king);
        let dist_after = chebyshev_distance(mv.to, enemy_king);
        if dist_after < dist_before {
            return 15000 + score / 10;
        }
    }

    score
}

fn get_piece_val(piece: Piece) -> i32 {
    match piece {
        Piece::Pawn => 1,
        Piece::Knight => 2,
        Piece::Bishop => 3,
        Piece::Rook => 4,
        Piece::Queen => 5,
        Piece::King => 6,
    }
}

pub fn iterative_deepening(board: &Board, history_hashes: &[u64], max_depth: i16, time_limit: Option<Duration>, stop: Arc<AtomicBool>) -> (Option<Move>, i32) {
    let size_mb = TT_SIZE_MB.load(Ordering::Relaxed);
    let tt = Arc::new(TranspositionTable::new(size_mb));
    let root_eval_state = crate::eval::EvalState::new(board);
    
    // Determine number of threads
    let num_threads = NUM_THREADS.load(Ordering::Relaxed).max(1);
        
    let mut handles = Vec::new();
    let nodes_count = Arc::new(AtomicU64::new(0));

    // Spawn helper threads (threads 1..num_threads)
    for thread_id in 1..num_threads {
        let board_clone = board.clone();
        let history_hashes_clone = history_hashes.to_vec();
        let stop_clone = Arc::clone(&stop);
        let tt_clone = Arc::clone(&tt);
        let nodes_clone = Arc::clone(&nodes_count);
        
        handles.push(std::thread::spawn(move || {
            let mut searcher = Searcher::new_with_shared_tt(stop_clone, None, tt_clone, nodes_clone);
            searcher.path_len = history_hashes_clone.len().min(1024);
            for i in 0..searcher.path_len {
                searcher.path_hashes[i] = history_hashes_clone[i];
            }
            
            // Search with a slight depth offset to diversify the search tree in Lazy SMP
            for depth in 1..=(max_depth + thread_id as i16) {
                if searcher.stop.load(Ordering::Relaxed) {
                    break;
                }
                let _ = searcher.search(&board_clone, &root_eval_state, depth, -30000, 30000, 0);
            }
        }));
    }
    
    // Main thread search
    let main_stop = Arc::clone(&stop);
    let main_tt = Arc::clone(&tt);
    let main_nodes = Arc::clone(&nodes_count);
    let mut main_searcher = Searcher::new_with_shared_tt(main_stop, time_limit, main_tt, main_nodes);
    main_searcher.path_len = history_hashes.len().min(1024);
    for i in 0..main_searcher.path_len {
        main_searcher.path_hashes[i] = history_hashes[i];
    }
    
    let original_time_limit = time_limit;
    let mut best_move = None;
    let mut best_score = -30000;
    let mut previous_best_move = None;
    let mut best_move_stable_iterations = 0;

    for depth in 1..=max_depth {
        if stop.load(Ordering::Relaxed) {
            break;
        }

        let mut alpha = -30000;
        let mut beta = 30000;
        if depth >= 5 {
            let margin = 40 + depth as i32 * 5;
            alpha = best_score - margin;
            beta = best_score + margin;
        }

        let mut score;
        let mut fail_count = 1;
        loop {
            score = main_searcher.search(board, &root_eval_state, depth, alpha, beta, 0);

            if stop.load(Ordering::Relaxed) {
                break;
            }

            if score <= alpha && alpha > -30000 {
                let step = fail_count * fail_count * 80;
                alpha = (alpha - step).max(-30000);
                fail_count += 1;
            } else if score >= beta && beta < 30000 {
                let step = fail_count * fail_count * 80;
                beta = (beta + step).min(30000);
                fail_count += 1;
            } else {
                break;
            }
        }

        if !stop.load(Ordering::Relaxed) {
            best_score = score;
            let current_best_move = main_searcher.root_best_move;
            if current_best_move.is_some() {
                best_move = current_best_move;
            }

            // Dynamic Time Management
            if depth > 1 {
                if current_best_move == previous_best_move {
                    best_move_stable_iterations += 1;
                } else {
                    best_move_stable_iterations = 0;
                    if let Some(orig_limit) = original_time_limit {
                        if let Some(current_limit) = main_searcher.time_limit {
                            let new_limit = current_limit.mul_f64(1.5).min(orig_limit * 4);
                            main_searcher.time_limit = Some(new_limit);
                        }
                    }
                }
            }
            previous_best_move = current_best_move;

            let elapsed = main_searcher.start_time.elapsed().as_millis().max(1);
            let total_nodes = nodes_count.load(Ordering::Relaxed) + (main_searcher.local_nodes & 1023);
            let nps = (total_nodes as f64 / (elapsed as f64 / 1000.0)) as u64;

            let score_str = if best_score > 29000 {
                format!("mate {}", (30000 - best_score + 1) / 2)
            } else if best_score < -29000 {
                format!("mate -{}", (30000 + best_score + 1) / 2)
            } else {
                format!("cp {}", best_score)
            };

            let pv_str = get_pv(&main_searcher, board);

            println!(
                "info depth {} seldepth {} score {} nodes {} nps {} time {} pv {}",
                depth, main_searcher.seldepth, score_str, total_nodes, nps, elapsed, pv_str
            );

            // Early exit if best move is highly stable (e.g. 4+ iterations, depth >= 8) and we've spent >= 30% of time limit
            if let Some(limit) = main_searcher.time_limit {
                let elapsed_dur = main_searcher.start_time.elapsed();
                if best_move_stable_iterations >= 4 && depth >= 8 && elapsed_dur >= limit / 3 {
                    stop.store(true, Ordering::Relaxed);
                    break;
                }
            }
        }
    }
    
    // Stop all helper threads
    stop.store(true, Ordering::Relaxed);
    
    // Join handles
    for handle in handles {
        let _ = handle.join();
    }

    (best_move, best_score)
}

fn get_pv(searcher: &Searcher, board: &Board) -> String {
    let mut pv = Vec::new();
    let mut temp_board = board.clone();
    let mut visited = Vec::new();

    for _ in 0..16 {
        let hash = temp_board.hash();
        if visited.contains(&hash) {
            break;
        }
        visited.push(hash);

        if let Some(entry) = searcher.tt.lookup(hash) {
            if let Some(mv) = entry.best_move {
                pv.push(mv);
                temp_board.play(mv);
            } else {
                break;
            }
        } else {
            break;
        }
    }

    pv.iter().map(|m| m.to_string()).collect::<Vec<String>>().join(" ")
}

pub fn get_piece_value_see(piece: Piece) -> i32 {
    match piece {
        Piece::Pawn => 100,
        Piece::Knight => 320,
        Piece::Bishop => 330,
        Piece::Rook => 500,
        Piece::Queen => 900,
        Piece::King => 20000,
    }
}

fn get_pawn_attacks_to(sq: Square, color: Color) -> BitBoard {
    let file = sq.file() as i32;
    let rank = sq.rank() as i32;
    let mut p_attacks = 0u64;
    let dir = if color == Color::White { -1 } else { 1 };
    let source_rank = rank + dir;
    if source_rank >= 0 && source_rank < 8 {
        if file > 0 {
            p_attacks |= 1 << (source_rank * 8 + (file - 1));
        }
        if file < 7 {
            p_attacks |= 1 << (source_rank * 8 + (file + 1));
        }
    }
    BitBoard(p_attacks)
}

fn get_attackers(board: &Board, sq: Square, occupied: BitBoard, removed: u64) -> BitBoard {
    let knights = board.pieces(Piece::Knight).0 & !removed;
    let kings = board.pieces(Piece::King).0 & !removed;
    let pawns = board.pieces(Piece::Pawn).0 & !removed;
    let bishops = (board.pieces(Piece::Bishop).0 | board.pieces(Piece::Queen).0) & !removed;
    let rooks = (board.pieces(Piece::Rook).0 | board.pieces(Piece::Queen).0) & !removed;

    let knight_attacks = cozy_chess::get_knight_moves(sq).0 & knights;
    let king_attacks = cozy_chess::get_king_moves(sq).0 & kings;
    let pawn_attacks = (get_pawn_attacks_to(sq, Color::White).0 & board.colors(Color::White).0 & pawns)
        | (get_pawn_attacks_to(sq, Color::Black).0 & board.colors(Color::Black).0 & pawns);
    let bishop_attacks = cozy_chess::get_bishop_moves(sq, occupied).0 & bishops;
    let rook_attacks = cozy_chess::get_rook_moves(sq, occupied).0 & rooks;

    BitBoard(knight_attacks | king_attacks | pawn_attacks | bishop_attacks | rook_attacks)
}

pub fn see(board: &Board, mv: Move) -> i32 {
    let to_sq = mv.to;
    let from_sq = mv.from;

    let from_piece = board.piece_on(from_sq);
    let target_piece_opt = board.piece_on(to_sq);
    let is_en_passant = from_piece == Some(Piece::Pawn) && from_sq.file() != to_sq.file() && target_piece_opt.is_none();

    let target_val = if is_en_passant {
        100
    } else if let Some(pc) = target_piece_opt {
        get_piece_value_see(pc)
    } else {
        0
    };
    let mut attacker_piece = from_piece.unwrap_or(Piece::Pawn);

    let mut gain = [0i32; 32];
    let mut d = 0;

    gain[d] = target_val;

    let mut removed = 1u64 << from_sq as u64;
    if is_en_passant {
        let cap_idx = (from_sq.rank() as usize) * 8 + (to_sq.file() as usize);
        let cap_sq = Square::ALL[cap_idx];
        removed |= 1u64 << cap_sq as u64;
    }
    let mut occupied = BitBoard((board.occupied().0 & !removed) | (1u64 << to_sq as u64));

    let mut attackers = get_attackers(board, to_sq, occupied, removed);
    let mut us = board.side_to_move();

    loop {
        d += 1;
        if d >= 31 {
            break;
        }
        us = !us;

        let side_attackers = attackers & board.colors(us);
        if side_attackers.0 == 0 {
            break;
        }

        let mut best_attacker_sq = None;
        let mut best_attacker_val = 999999;
        let mut best_attacker_piece = Piece::Pawn;

        let mut temp_bb = side_attackers.0;
        while temp_bb != 0 {
            let lsb = temp_bb.trailing_zeros();
            temp_bb &= temp_bb - 1;
            let sq = Square::ALL[lsb as usize];
            if let Some(pc) = board.piece_on(sq) {
                let val = get_piece_value_see(pc);
                if val < best_attacker_val {
                    best_attacker_val = val;
                    best_attacker_sq = Some(sq);
                    best_attacker_piece = pc;
                }
            }
        }

        let atk_sq = match best_attacker_sq {
            Some(sq) => sq,
            None => break,
        };

        gain[d] = get_piece_value_see(attacker_piece);

        removed |= 1u64 << atk_sq as u64;
        occupied = BitBoard((board.occupied().0 & !removed) | (1u64 << to_sq as u64));
        attackers = get_attackers(board, to_sq, occupied, removed);
        attacker_piece = best_attacker_piece;
    }

    while d > 0 {
        d -= 1;
        gain[d] = gain[d] - gain[d + 1];
        if d > 0 {
            gain[d] = gain[d].max(0);
        }
    }

    gain[0]
}
