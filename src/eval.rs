use cozy_chess::{Board, Color, Piece, Square, BitBoard, Move};

const PAWN_VAL: i32 = 100;
const KNIGHT_VAL: i32 = 320;
const BISHOP_VAL: i32 = 330;
const ROOK_VAL: i32 = 500;
const QUEEN_VAL: i32 = 900;

// Piece-Square Tables (PST) from White's perspective.
// Rank 1 is 0..7 (A1..H1), Rank 8 is 56..63 (A8..H8).

const PAWN_PST: [i32; 64] = [
     0,  0,  0,  0,  0,  0,  0,  0, // Rank 1
     5, 10, 10,-20,-20, 10, 10,  5, // Rank 2
     5, -5,-10,  0,  0,-10, -5,  5, // Rank 3
     0,  0,  0, 20, 20,  0,  0,  0, // Rank 4
     5,  5, 10, 25, 25, 10,  5,  5, // Rank 5
    10, 10, 20, 30, 30, 20, 10, 10, // Rank 6
    50, 50, 50, 50, 50, 50, 50, 50, // Rank 7
     0,  0,  0,  0,  0,  0,  0,  0  // Rank 8
];

const KNIGHT_PST: [i32; 64] = [
    -50,-40,-30,-30,-30,-30,-40,-50, // Rank 1
    -40,-20,  0,  5,  5,  0,-20,-40, // Rank 2
    -30,  5, 10, 15, 15, 10,  5,-30, // Rank 3
    -30,  0, 15, 20, 20, 15,  0,-30, // Rank 4
    -30,  5, 15, 20, 20, 15,  5,-30, // Rank 5
    -30,  0, 10, 15, 15, 10,  0,-30, // Rank 6
    -40,-20,  0,  0,  0,  0,-20,-40, // Rank 7
    -50,-40,-30,-30,-30,-30,-40,-50  // Rank 8
];

const BISHOP_PST: [i32; 64] = [
    -20,-10,-10,-10,-10,-10,-10,-20, // Rank 1
    -10,  5,  0,  0,  0,  0,  5,-10, // Rank 2
    -10, 10, 10, 10, 10, 10, 10,-10, // Rank 3
    -10,  0, 10, 10, 10, 10,  0,-10, // Rank 4
    -10,  5,  5, 10, 10,  5,  5,-10, // Rank 5
    -10,  0,  5, 10, 10,  5,  0,-10, // Rank 6
    -10,  0,  0,  0,  0,  0,  0,-10, // Rank 7
    -20,-10,-10,-10,-10,-10,-10,-20  // Rank 8
];

const ROOK_PST: [i32; 64] = [
      0,  0,  0,  5,  5,  0,  0,  0, // Rank 1
     -5,  0,  0,  0,  0,  0,  0, -5, // Rank 2
     -5,  0,  0,  0,  0,  0,  0, -5, // Rank 3
     -5,  0,  0,  0,  0,  0,  0, -5, // Rank 4
     -5,  0,  0,  0,  0,  0,  0, -5, // Rank 5
     -5,  0,  0,  0,  0,  0,  0, -5, // Rank 6
      5, 10, 10, 10, 10, 10, 10,  5, // Rank 7
      0,  0,  0,  0,  0,  0,  0,  0  // Rank 8
];

const QUEEN_PST: [i32; 64] = [
    -20,-10,-10, -5, -5,-10,-10,-20, // Rank 1
    -10,  0,  5,  0,  0,  5,  0,-10, // Rank 2
    -10,  5,  5,  5,  5,  5,  0,-10, // Rank 3
      0,  0,  5,  5,  5,  5,  0, -5, // Rank 4
     -5,  0,  5,  5,  5,  5,  0, -5, // Rank 5
    -10,  0,  5,  5,  5,  5,  0,-10, // Rank 6
    -10,  0,  0,  0,  0,  0,  0,-10, // Rank 7
    -20,-10,-10, -5, -5,-10,-10,-20  // Rank 8
];

const KING_MIDDLE_PST: [i32; 64] = [
     20, 30, 10,  0,  0, 10, 30, 20, // Rank 1
     20, 20,  0,  0,  0,  0, 20, 20, // Rank 2
    -10,-20,-20,-20,-20,-20,-20,-10, // Rank 3
    -20,-30,-30,-40,-40,-30,-30,-20, // Rank 4
    -30,-40,-40,-50,-50,-40,-40,-30, // Rank 5
    -30,-40,-40,-50,-50,-40,-40,-30, // Rank 6
    -30,-40,-40,-50,-50,-40,-40,-30, // Rank 7
    -30,-40,-40,-50,-50,-40,-40,-30  // Rank 8
];

const KING_END_PST: [i32; 64] = [
    -50,-30,-30,-30,-30,-30,-30,-50, // Rank 1
    -30,-30,  0,  0,  0,  0,-30,-30, // Rank 2
    -30,-10, 20, 30, 30, 20,-10,-30, // Rank 3
    -30,-10, 30, 40, 40, 30,-10,-30, // Rank 4
    -30,-10, 30, 40, 40, 30,-10,-30, // Rank 5
    -30,-10, 20, 30, 30, 20,-10,-30, // Rank 6
    -30,-20,-10,  0,  0,-10,-20,-30, // Rank 7
    -50,-40,-30,-20,-20,-30,-40,-50  // Rank 8
];

// Coordinate attacks scaling bonus
const KING_ATTACK_BONUS: [i32; 32] = [
    0,   0,   10,  50,  100, 180, 280, 400,
    550, 720, 900, 1100, 1300, 1500, 1500, 1500,
    1500, 1500, 1500, 1500, 1500, 1500, 1500, 1500,
    1500, 1500, 1500, 1500, 1500, 1500, 1500, 1500
];

const FILE_A: u64 = 0x0101010101010101;
const FILE_H: u64 = 0x8080808080808080;

const KNIGHT_PROX: [i32; 8] = [0, 90, 65, 40, 20, 0, 0, 0];
const BISHOP_PROX: [i32; 8] = [0, 75, 55, 38, 18, 0, 0, 0];
const ROOK_PROX: [i32; 8] = [0, 65, 50, 35, 15, 0, 0, 0];
const QUEEN_PROX: [i32; 8] = [0, 100, 70, 45, 25, 0, 0, 0];
const CRAMP_BONUS: [i32; 10] = [150, 100, 50, 20, 0, 0, 0, 0, 0, 0];

const PASSED_PAWN_MG_WHITE: [i32; 8] = [5, 5, 5, 5, 20, 40, 80, 5];
const PASSED_PAWN_EG_WHITE: [i32; 8] = [10, 10, 10, 10, 30, 60, 120, 10];
const PASSED_PAWN_MG_BLACK: [i32; 8] = [5, 80, 40, 20, 5, 5, 5, 5];
const PASSED_PAWN_EG_BLACK: [i32; 8] = [10, 120, 60, 30, 10, 10, 10, 10];

const fn make_passed_pawn_mask_white() -> [u64; 64] {
    let mut masks = [0u64; 64];
    let mut sq = 0;
    while sq < 64 {
        let f = sq % 8;
        let r = sq / 8;
        let mut mask = 0u64;
        let mut file_offset = -1;
        while file_offset <= 1 {
            let check_file = f as i32 + file_offset;
            if check_file >= 0 && check_file < 8 {
                let mut check_rank = r + 1;
                while check_rank < 8 {
                    mask |= 1 << (check_rank * 8 + check_file as usize);
                    check_rank += 1;
                }
            }
            file_offset += 1;
        }
        masks[sq] = mask;
        sq += 1;
    }
    masks
}

const fn make_passed_pawn_mask_black() -> [u64; 64] {
    let mut masks = [0u64; 64];
    let mut sq = 0;
    while sq < 64 {
        let f = sq % 8;
        let r = sq / 8;
        let mut mask = 0u64;
        let mut file_offset = -1;
        while file_offset <= 1 {
            let check_file = f as i32 + file_offset;
            if check_file >= 0 && check_file < 8 {
                let mut check_rank = 0;
                while check_rank < r {
                    mask |= 1 << (check_rank * 8 + check_file as usize);
                    check_rank += 1;
                }
            }
            file_offset += 1;
        }
        masks[sq] = mask;
        sq += 1;
    }
    masks
}

static PASSED_PAWN_MASK_WHITE: [u64; 64] = make_passed_pawn_mask_white();
static PASSED_PAWN_MASK_BLACK: [u64; 64] = make_passed_pawn_mask_black();

#[inline(always)]
fn flip_sq(sq: Square) -> Square {
    let idx = sq as usize;
    let flipped_idx = (7 - (idx / 8)) * 8 + (idx % 8);
    Square::ALL[flipped_idx]
}

#[inline(always)]
fn chebyshev_distance(sq1: Square, sq2: Square) -> i32 {
    let f1 = sq1.file() as i32;
    let r1 = sq1.rank() as i32;
    let f2 = sq2.file() as i32;
    let r2 = sq2.rank() as i32;
    (f1 - f2).abs().max((r1 - r2).abs())
}

fn evaluate_pawn_shield(_board: &Board, color: Color, king: Square, pawns: u64, enemy_power: u32) -> i32 {
    if enemy_power == 0 {
        return 0;
    }
    let king_file = king.file() as i32;
    let king_rank = king.rank() as i32;
    
    let mut penalty = 0;
    let start_file = (king_file - 1).max(0);
    let end_file = (king_file + 1).min(7);
    
    for f in start_file..=end_file {
        let mut has_pawn = false;
        if color == Color::White {
            for r in (king_rank + 1)..8 {
                let sq_idx = r * 8 + f;
                if (pawns & (1u64 << sq_idx)) != 0 {
                    has_pawn = true;
                    let dist = r - king_rank;
                    if dist == 1 {
                        // perfect shield
                    } else if dist == 2 {
                        penalty += 40;
                    } else {
                        penalty += 80;
                    }
                    break;
                }
            }
        } else {
            for r in (0..king_rank).rev() {
                let sq_idx = r * 8 + f;
                if (pawns & (1u64 << sq_idx)) != 0 {
                    has_pawn = true;
                    let dist = king_rank - r;
                    if dist == 1 {
                        // perfect
                    } else if dist == 2 {
                        penalty += 40;
                    } else {
                        penalty += 80;
                    }
                    break;
                }
            }
        }
        if !has_pawn {
            penalty += 150;
        }
    }
    
    penalty * enemy_power as i32 / 8
}

#[derive(Clone, Copy, Debug)]
pub struct EvalState {
    pub w_material: i32,
    pub b_material: i32,
    pub w_pst_mg: i32,
    pub w_pst_eg: i32,
    pub b_pst_mg: i32,
    pub b_pst_eg: i32,
    pub phase: i32,
}

fn get_piece_val_eval(piece: Piece) -> i32 {
    match piece {
        Piece::Pawn => PAWN_VAL,
        Piece::Knight => KNIGHT_VAL,
        Piece::Bishop => BISHOP_VAL,
        Piece::Rook => ROOK_VAL,
        Piece::Queen => QUEEN_VAL,
        Piece::King => 0,
    }
}

fn get_phase_val(piece: Piece) -> i32 {
    match piece {
        Piece::Pawn => 0,
        Piece::Knight => 1,
        Piece::Bishop => 1,
        Piece::Rook => 2,
        Piece::Queen => 4,
        Piece::King => 0,
    }
}

fn get_pst_vals(color: Color, piece: Piece, sq_idx: usize) -> (i32, i32) {
    let flipped_sq = if color == Color::Black {
        flip_sq(Square::ALL[sq_idx]) as usize
    } else {
        sq_idx
    };
    match piece {
        Piece::Pawn => (PAWN_PST[flipped_sq], PAWN_PST[flipped_sq]),
        Piece::Knight => (KNIGHT_PST[flipped_sq], KNIGHT_PST[flipped_sq]),
        Piece::Bishop => (BISHOP_PST[flipped_sq], BISHOP_PST[flipped_sq]),
        Piece::Rook => (ROOK_PST[flipped_sq], ROOK_PST[flipped_sq]),
        Piece::Queen => (QUEEN_PST[flipped_sq], QUEEN_PST[flipped_sq]),
        Piece::King => (KING_MIDDLE_PST[flipped_sq], KING_END_PST[flipped_sq]),
    }
}

impl EvalState {
    pub fn new(board: &Board) -> Self {
        let mut w_material = 0;
        let mut b_material = 0;
        let mut w_pst_mg = 0;
        let mut w_pst_eg = 0;
        let mut b_pst_mg = 0;
        let mut b_pst_eg = 0;
        let mut phase = 0;

        // White pieces
        let mut bb = board.colored_pieces(Color::White, Piece::Pawn);
        while bb.0 != 0 {
            let sq = bb.0.trailing_zeros() as usize;
            bb.0 &= bb.0 - 1;
            w_material += PAWN_VAL;
            w_pst_mg += PAWN_PST[sq];
            w_pst_eg += PAWN_PST[sq];
        }
        let mut bb = board.colored_pieces(Color::White, Piece::Knight);
        while bb.0 != 0 {
            let sq = bb.0.trailing_zeros() as usize;
            bb.0 &= bb.0 - 1;
            phase += 1;
            w_material += KNIGHT_VAL;
            w_pst_mg += KNIGHT_PST[sq];
            w_pst_eg += KNIGHT_PST[sq];
        }
        let mut bb = board.colored_pieces(Color::White, Piece::Bishop);
        while bb.0 != 0 {
            let sq = bb.0.trailing_zeros() as usize;
            bb.0 &= bb.0 - 1;
            phase += 1;
            w_material += BISHOP_VAL;
            w_pst_mg += BISHOP_PST[sq];
            w_pst_eg += BISHOP_PST[sq];
        }
        let mut bb = board.colored_pieces(Color::White, Piece::Rook);
        while bb.0 != 0 {
            let sq = bb.0.trailing_zeros() as usize;
            bb.0 &= bb.0 - 1;
            phase += 2;
            w_material += ROOK_VAL;
            w_pst_mg += ROOK_PST[sq];
            w_pst_eg += ROOK_PST[sq];
        }
        let mut bb = board.colored_pieces(Color::White, Piece::Queen);
        while bb.0 != 0 {
            let sq = bb.0.trailing_zeros() as usize;
            bb.0 &= bb.0 - 1;
            phase += 4;
            w_material += QUEEN_VAL;
            w_pst_mg += QUEEN_PST[sq];
            w_pst_eg += QUEEN_PST[sq];
        }
        let mut bb = board.colored_pieces(Color::White, Piece::King);
        while bb.0 != 0 {
            let sq = bb.0.trailing_zeros() as usize;
            bb.0 &= bb.0 - 1;
            w_pst_mg += KING_MIDDLE_PST[sq];
            w_pst_eg += KING_END_PST[sq];
        }

        // Black pieces
        let mut bb = board.colored_pieces(Color::Black, Piece::Pawn);
        while bb.0 != 0 {
            let sq = bb.0.trailing_zeros() as usize;
            let flipped_sq = flip_sq(Square::ALL[sq]) as usize;
            bb.0 &= bb.0 - 1;
            b_material += PAWN_VAL;
            b_pst_mg += PAWN_PST[flipped_sq];
            b_pst_eg += PAWN_PST[flipped_sq];
        }
        let mut bb = board.colored_pieces(Color::Black, Piece::Knight);
        while bb.0 != 0 {
            let sq = bb.0.trailing_zeros() as usize;
            let flipped_sq = flip_sq(Square::ALL[sq]) as usize;
            bb.0 &= bb.0 - 1;
            phase += 1;
            b_material += KNIGHT_VAL;
            b_pst_mg += KNIGHT_PST[flipped_sq];
            b_pst_eg += KNIGHT_PST[flipped_sq];
        }
        let mut bb = board.colored_pieces(Color::Black, Piece::Bishop);
        while bb.0 != 0 {
            let sq = bb.0.trailing_zeros() as usize;
            let flipped_sq = flip_sq(Square::ALL[sq]) as usize;
            bb.0 &= bb.0 - 1;
            phase += 1;
            b_material += BISHOP_VAL;
            b_pst_mg += BISHOP_PST[flipped_sq];
            b_pst_eg += BISHOP_PST[flipped_sq];
        }
        let mut bb = board.colored_pieces(Color::Black, Piece::Rook);
        while bb.0 != 0 {
            let sq = bb.0.trailing_zeros() as usize;
            let flipped_sq = flip_sq(Square::ALL[sq]) as usize;
            bb.0 &= bb.0 - 1;
            phase += 2;
            b_material += ROOK_VAL;
            b_pst_mg += ROOK_PST[flipped_sq];
            b_pst_eg += ROOK_PST[flipped_sq];
        }
        let mut bb = board.colored_pieces(Color::Black, Piece::Queen);
        while bb.0 != 0 {
            let sq = bb.0.trailing_zeros() as usize;
            let flipped_sq = flip_sq(Square::ALL[sq]) as usize;
            bb.0 &= bb.0 - 1;
            phase += 4;
            b_material += QUEEN_VAL;
            b_pst_mg += QUEEN_PST[flipped_sq];
            b_pst_eg += QUEEN_PST[flipped_sq];
        }
        let mut bb = board.colored_pieces(Color::Black, Piece::King);
        while bb.0 != 0 {
            let sq = bb.0.trailing_zeros() as usize;
            let flipped_sq = flip_sq(Square::ALL[sq]) as usize;
            bb.0 &= bb.0 - 1;
            b_pst_mg += KING_MIDDLE_PST[flipped_sq];
            b_pst_eg += KING_END_PST[flipped_sq];
        }

        Self {
            w_material,
            b_material,
            w_pst_mg,
            w_pst_eg,
            b_pst_mg,
            b_pst_eg,
            phase,
        }
    }

    fn add_piece(&mut self, color: Color, piece: Piece, sq: Square) {
        let sq_idx = sq as usize;
        let (pst_mg, pst_eg) = get_pst_vals(color, piece, sq_idx);
        let val = get_piece_val_eval(piece);
        let phase_inc = get_phase_val(piece);
        
        if color == Color::White {
            self.w_material += val;
            self.w_pst_mg += pst_mg;
            self.w_pst_eg += pst_eg;
        } else {
            self.b_material += val;
            self.b_pst_mg += pst_mg;
            self.b_pst_eg += pst_eg;
        }
        self.phase += phase_inc;
    }

    fn remove_piece(&mut self, color: Color, piece: Piece, sq: Square) {
        let sq_idx = sq as usize;
        let (pst_mg, pst_eg) = get_pst_vals(color, piece, sq_idx);
        let val = get_piece_val_eval(piece);
        let phase_inc = get_phase_val(piece);
        
        if color == Color::White {
            self.w_material -= val;
            self.w_pst_mg -= pst_mg;
            self.w_pst_eg -= pst_eg;
        } else {
            self.b_material -= val;
            self.b_pst_mg -= pst_mg;
            self.b_pst_eg -= pst_eg;
        }
        self.phase -= phase_inc;
    }

    pub fn update(&self, board: &Board, mv: Move) -> Self {
        let mut next = *self;
        let us = board.side_to_move();
        let them = !us;
        let piece = board.piece_on(mv.from).unwrap();
        
        // Remove piece from starting square
        next.remove_piece(us, piece, mv.from);
        
        // Capture logic
        if piece == Piece::Pawn && board.en_passant() == Some(mv.to.file()) {
            let cap_sq = Square::new(mv.to.file(), mv.from.rank());
            next.remove_piece(them, Piece::Pawn, cap_sq);
        } else if piece == Piece::King && board.color_on(mv.to) == Some(us) {
            // Castling
            let rook_sq = mv.to;
            let king_side = rook_sq.file() as usize > mv.from.file() as usize;
            let rank = mv.from.rank();
            
            let (new_king_sq, new_rook_sq) = if king_side {
                (Square::new(cozy_chess::File::G, rank), Square::new(cozy_chess::File::F, rank))
            } else {
                (Square::new(cozy_chess::File::C, rank), Square::new(cozy_chess::File::D, rank))
            };
            
            next.remove_piece(us, Piece::Rook, rook_sq);
            next.add_piece(us, Piece::King, new_king_sq);
            next.add_piece(us, Piece::Rook, new_rook_sq);
            return next;
        } else if let Some(captured_piece) = board.piece_on(mv.to) {
            next.remove_piece(them, captured_piece, mv.to);
        }
        
        // Place piece on target square (or handle promotion)
        if let Some(promo) = mv.promotion {
            next.add_piece(us, promo, mv.to);
        } else {
            next.add_piece(us, piece, mv.to);
        }
        
        next
    }
}

pub fn evaluate(board: &Board) -> i32 {
    let state = EvalState::new(board);
    evaluate_incremental(board, &state)
}

pub fn evaluate_incremental(board: &Board, state: &EvalState) -> i32 {
    let white_material = state.w_material;
    let black_material = state.b_material;
    let mut white_positional_mg = state.w_pst_mg;
    let mut white_positional_eg = state.w_pst_eg;
    let mut black_positional_mg = state.b_pst_mg;
    let mut black_positional_eg = state.b_pst_eg;

    let (is_closed, _) = locked_center_status(board);
    let knight_outpost_mg = if is_closed { 60 } else { 40 };
    let knight_outpost_eg = if is_closed { 40 } else { 20 };
    let bishop_outpost_mg = if is_closed { 50 } else { 30 };
    let bishop_outpost_eg = if is_closed { 35 } else { 15 };

    let white_pieces = board.colors(Color::White);
    let black_pieces = board.colors(Color::Black);
    let occupied = white_pieces | black_pieces;

    let w_pawns_val = board.colored_pieces(Color::White, Piece::Pawn).0;
    let b_pawns_val = board.colored_pieces(Color::Black, Piece::Pawn).0;

    let phase = state.phase;

    let w_enemy_power = board.colored_pieces(Color::Black, Piece::Knight).0.count_ones()
        + board.colored_pieces(Color::Black, Piece::Bishop).0.count_ones()
        + board.colored_pieces(Color::Black, Piece::Rook).0.count_ones()
        + board.colored_pieces(Color::Black, Piece::Queen).0.count_ones();
    let b_enemy_power = board.colored_pieces(Color::White, Piece::Knight).0.count_ones()
        + board.colored_pieces(Color::White, Piece::Bishop).0.count_ones()
        + board.colored_pieces(Color::White, Piece::Rook).0.count_ones()
        + board.colored_pieces(Color::White, Piece::Queen).0.count_ones();

    // King Attack & Piece Mobility/Activity Evaluation
    let w_king = board.king(Color::White);
    let b_king = board.king(Color::Black);

    let w_king_ring = cozy_chess::get_king_moves(w_king) | BitBoard(1 << (w_king as u64));
    let b_king_ring = cozy_chess::get_king_moves(b_king) | BitBoard(1 << (b_king as u64));

    // Evaluate White attacks on Black King & White Piece Mobility
    let mut w_attack_weight = 0;
    let mut w_attacking_pieces = 0;
    let mut w_close_attackers = 0;
    let mut w_attack_squares = 0u64;
    let mut w_all_attacks = 0u64;

    // Evaluate Black attacks on White King & Black Piece Mobility
    let mut b_attack_weight = 0;
    let mut b_attacking_pieces = 0;
    let mut b_close_attackers = 0;
    let mut b_attack_squares = 0u64;
    let mut b_all_attacks = 0u64;

    // White attacking pieces & mobility
    // Knight
    let mut bb = board.colored_pieces(Color::White, Piece::Knight);
    while bb.0 != 0 {
        let sq = bb.0.trailing_zeros();
        let square = Square::ALL[sq as usize];
        bb.0 &= bb.0 - 1;
        let attacks = cozy_chess::get_knight_moves(square);
        w_all_attacks |= attacks.0;
        let mobility_bonus = (attacks.0.count_ones() as i32) * 5;
        white_positional_mg += mobility_bonus;
        white_positional_eg += mobility_bonus;

        let dist = chebyshev_distance(square, b_king) as usize;
        white_positional_mg += KNIGHT_PROX[dist];
        
        // Opponent King Gravitational Pull
        white_positional_mg += (8 - dist as i32) * 8;

        // Opposite Wing Development Penalty
        let enemy_king_file = b_king.file() as i32;
        let sq_file = square.file() as i32;
        let opposite_wing = ((enemy_king_file >= 5 && sq_file <= 2) || (enemy_king_file <= 2 && sq_file >= 5)) as i32;
        white_positional_mg -= opposite_wing * ((phase > 18) as i32) * 60;

        // Knight Assault Network (2 jumps away from king)
        let is_assault_net = (attacks.0 & cozy_chess::get_knight_moves(b_king).0 != 0) as i32;
        white_positional_mg += is_assault_net * 60;

        // King ring penetration
        white_positional_mg += (((1u64 << sq) & b_king_ring.0 != 0) as i32) * 80;

        let hits = attacks.0 & b_king_ring.0;
        let is_attacking = (hits != 0) as i32;
        w_attacking_pieces += is_attacking;
        if dist <= 3 && is_attacking != 0 {
            w_close_attackers += 1;
        }
        w_attack_weight += is_attacking * 3;
        w_attack_squares |= hits;
    }
    // Bishop
    let mut bb = board.colored_pieces(Color::White, Piece::Bishop);
    while bb.0 != 0 {
        let sq = bb.0.trailing_zeros();
        let square = Square::ALL[sq as usize];
        bb.0 &= bb.0 - 1;
        let attacks = cozy_chess::get_bishop_moves(square, occupied);
        w_all_attacks |= attacks.0;
        let mobility_bonus = (attacks.0.count_ones() as i32) * 5;
        white_positional_mg += mobility_bonus;
        white_positional_eg += mobility_bonus;

        let dist = chebyshev_distance(square, b_king) as usize;
        white_positional_mg += BISHOP_PROX[dist];

        // Opponent King Gravitational Pull
        white_positional_mg += (8 - dist as i32) * 8;

        // Opposite Wing Development Penalty
        let enemy_king_file = b_king.file() as i32;
        let sq_file = square.file() as i32;
        let opposite_wing = ((enemy_king_file >= 5 && sq_file <= 2) || (enemy_king_file <= 2 && sq_file >= 5)) as i32;
        white_positional_mg -= opposite_wing * ((phase > 18) as i32) * 60;

        // Ray-Attack threats to Black King
        let f_diff = (square.file() as i32 - b_king.file() as i32).abs();
        let r_diff = (square.rank() as i32 - b_king.rank() as i32).abs();
        white_positional_mg += ((f_diff == r_diff) as i32) * 80;

        // King ring penetration
        white_positional_mg += (((1u64 << sq) & b_king_ring.0 != 0) as i32) * 60;

        let hits = attacks.0 & b_king_ring.0;
        let is_attacking = (hits != 0) as i32;
        w_attacking_pieces += is_attacking;
        if dist <= 3 && is_attacking != 0 {
            w_close_attackers += 1;
        }
        w_attack_weight += is_attacking * 3;
        w_attack_squares |= hits;
    }
    // Rook
    let mut bb = board.colored_pieces(Color::White, Piece::Rook);
    while bb.0 != 0 {
        let sq = bb.0.trailing_zeros();
        let square = Square::ALL[sq as usize];
        bb.0 &= bb.0 - 1;
        let attacks = cozy_chess::get_rook_moves(square, occupied);
        w_all_attacks |= attacks.0;
        let mobility_bonus = (attacks.0.count_ones() as i32) * 4;
        white_positional_mg += mobility_bonus;
        white_positional_eg += mobility_bonus;

        let dist = chebyshev_distance(square, b_king) as usize;
        white_positional_mg += ROOK_PROX[dist];

        // Opponent King Gravitational Pull
        white_positional_mg += (8 - dist as i32) * 8;

        // Ray-Attack threats to Black King (same file or adjacent files)
        white_positional_mg += ((square.file() == b_king.file()) as i32) * 90;
        let adj_file = ((square.file() as i32 - b_king.file() as i32).abs() == 1) as i32;
        white_positional_mg += adj_file * 50;

        // King ring penetration
        white_positional_mg += (((1u64 << sq) & b_king_ring.0 != 0) as i32) * 90;

        // Open/Semi-Open File Rook
        let f = sq & 7;
        let file_mask = 0x0101010101010101u64 << f;
        let w_pawn_exists = (w_pawns_val & file_mask) != 0;
        let b_pawn_exists = (b_pawns_val & file_mask) != 0;
        let rook_bonus = (!w_pawn_exists as i32) * (15 + (!b_pawn_exists as i32) * 15);
        white_positional_mg += rook_bonus;
        white_positional_eg += rook_bonus;

        let hits = attacks.0 & b_king_ring.0;
        let is_attacking = (hits != 0) as i32;
        w_attacking_pieces += is_attacking;
        if dist <= 3 && is_attacking != 0 {
            w_close_attackers += 1;
        }
        w_attack_weight += is_attacking * 4;
        w_attack_squares |= hits;
    }
    // Queen
    let mut bb = board.colored_pieces(Color::White, Piece::Queen);
    while bb.0 != 0 {
        let sq = bb.0.trailing_zeros();
        let square = Square::ALL[sq as usize];
        bb.0 &= bb.0 - 1;
        let attacks = cozy_chess::get_bishop_moves(square, occupied) | cozy_chess::get_rook_moves(square, occupied);
        w_all_attacks |= attacks.0;
        let mobility_bonus = (attacks.0.count_ones() as i32) * 2;
        white_positional_mg += mobility_bonus;
        white_positional_eg += mobility_bonus;

        let dist = chebyshev_distance(square, b_king) as usize;
        white_positional_mg += QUEEN_PROX[dist];

        // Opponent King Gravitational Pull
        white_positional_mg += (8 - dist as i32) * 8;

        // Ray-Attack threats to Black King (diagonal or orthogonal files/diagonals)
        white_positional_mg += ((square.file() == b_king.file()) as i32) * 90;
        let adj_file = ((square.file() as i32 - b_king.file() as i32).abs() == 1) as i32;
        white_positional_mg += adj_file * 50;
        let f_diff = (square.file() as i32 - b_king.file() as i32).abs();
        let r_diff = (square.rank() as i32 - b_king.rank() as i32).abs();
        white_positional_mg += ((f_diff == r_diff) as i32) * 80;

        // King ring penetration
        white_positional_mg += (((1u64 << sq) & b_king_ring.0 != 0) as i32) * 120;

        let hits = attacks.0 & b_king_ring.0;
        let is_attacking = (hits != 0) as i32;
        w_attacking_pieces += is_attacking;
        if dist <= 3 && is_attacking != 0 {
            w_close_attackers += 1;
        }
        w_attack_weight += is_attacking * 7;
        w_attack_squares |= hits;
    }

    // Black attacking pieces & mobility
    // Knight
    let mut bb = board.colored_pieces(Color::Black, Piece::Knight);
    while bb.0 != 0 {
        let sq = bb.0.trailing_zeros();
        let square = Square::ALL[sq as usize];
        bb.0 &= bb.0 - 1;
        let attacks = cozy_chess::get_knight_moves(square);
        b_all_attacks |= attacks.0;
        let mobility_bonus = (attacks.0.count_ones() as i32) * 5;
        black_positional_mg += mobility_bonus;
        black_positional_eg += mobility_bonus;

        let dist = chebyshev_distance(square, w_king) as usize;
        black_positional_mg += KNIGHT_PROX[dist];

        // Opponent King Gravitational Pull
        black_positional_mg += (8 - dist as i32) * 8;

        // Opposite Wing Development Penalty
        let enemy_king_file = w_king.file() as i32;
        let sq_file = square.file() as i32;
        let opposite_wing = ((enemy_king_file >= 5 && sq_file <= 2) || (enemy_king_file <= 2 && sq_file >= 5)) as i32;
        black_positional_mg -= opposite_wing * ((phase > 18) as i32) * 60;

        // Knight Assault Network (2 jumps away from king)
        let is_assault_net = (attacks.0 & cozy_chess::get_knight_moves(w_king).0 != 0) as i32;
        black_positional_mg += is_assault_net * 60;

        // King ring penetration
        black_positional_mg += (((1u64 << sq) & w_king_ring.0 != 0) as i32) * 80;

        let hits = attacks.0 & w_king_ring.0;
        let is_attacking = (hits != 0) as i32;
        b_attacking_pieces += is_attacking;
        if dist <= 3 && is_attacking != 0 {
            b_close_attackers += 1;
        }
        b_attack_weight += is_attacking * 3;
        b_attack_squares |= hits;
    }
    // Bishop
    let mut bb = board.colored_pieces(Color::Black, Piece::Bishop);
    while bb.0 != 0 {
        let sq = bb.0.trailing_zeros();
        let square = Square::ALL[sq as usize];
        bb.0 &= bb.0 - 1;
        let attacks = cozy_chess::get_bishop_moves(square, occupied);
        b_all_attacks |= attacks.0;
        let mobility_bonus = (attacks.0.count_ones() as i32) * 5;
        black_positional_mg += mobility_bonus;
        black_positional_eg += mobility_bonus;

        let dist = chebyshev_distance(square, w_king) as usize;
        black_positional_mg += BISHOP_PROX[dist];

        // Opponent King Gravitational Pull
        black_positional_mg += (8 - dist as i32) * 8;

        // Opposite Wing Development Penalty
        let enemy_king_file = w_king.file() as i32;
        let sq_file = square.file() as i32;
        let opposite_wing = ((enemy_king_file >= 5 && sq_file <= 2) || (enemy_king_file <= 2 && sq_file >= 5)) as i32;
        black_positional_mg -= opposite_wing * ((phase > 18) as i32) * 60;

        // Ray-Attack threats to White King
        let f_diff = (square.file() as i32 - w_king.file() as i32).abs();
        let r_diff = (square.rank() as i32 - w_king.rank() as i32).abs();
        black_positional_mg += ((f_diff == r_diff) as i32) * 80;

        // King ring penetration
        black_positional_mg += (((1u64 << sq) & w_king_ring.0 != 0) as i32) * 60;

        let hits = attacks.0 & w_king_ring.0;
        let is_attacking = (hits != 0) as i32;
        b_attacking_pieces += is_attacking;
        if dist <= 3 && is_attacking != 0 {
            b_close_attackers += 1;
        }
        b_attack_weight += is_attacking * 3;
        b_attack_squares |= hits;
    }
    // Rook
    let mut bb = board.colored_pieces(Color::Black, Piece::Rook);
    while bb.0 != 0 {
        let sq = bb.0.trailing_zeros();
        let square = Square::ALL[sq as usize];
        bb.0 &= bb.0 - 1;
        let attacks = cozy_chess::get_rook_moves(square, occupied);
        b_all_attacks |= attacks.0;
        let mobility_bonus = (attacks.0.count_ones() as i32) * 4;
        black_positional_mg += mobility_bonus;
        black_positional_eg += mobility_bonus;

        let dist = chebyshev_distance(square, w_king) as usize;
        black_positional_mg += ROOK_PROX[dist];

        // Opponent King Gravitational Pull
        black_positional_mg += (8 - dist as i32) * 8;

        // Ray-Attack threats to White King (same file or adjacent files)
        black_positional_mg += ((square.file() == w_king.file()) as i32) * 90;
        let adj_file = ((square.file() as i32 - w_king.file() as i32).abs() == 1) as i32;
        black_positional_mg += adj_file * 50;

        // King ring penetration
        black_positional_mg += (((1u64 << sq) & w_king_ring.0 != 0) as i32) * 90;

        // Open/Semi-Open File Rook
        let f = sq & 7;
        let file_mask = 0x0101010101010101u64 << f;
        let w_pawn_exists = (w_pawns_val & file_mask) != 0;
        let b_pawn_exists = (b_pawns_val & file_mask) != 0;
        let rook_bonus = (!b_pawn_exists as i32) * (15 + (!w_pawn_exists as i32) * 15);
        black_positional_mg += rook_bonus;
        black_positional_eg += rook_bonus;

        let hits = attacks.0 & w_king_ring.0;
        let is_attacking = (hits != 0) as i32;
        b_attacking_pieces += is_attacking;
        if dist <= 3 && is_attacking != 0 {
            b_close_attackers += 1;
        }
        b_attack_weight += is_attacking * 4;
        b_attack_squares |= hits;
    }
    // Queen
    let mut bb = board.colored_pieces(Color::Black, Piece::Queen);
    while bb.0 != 0 {
        let sq = bb.0.trailing_zeros();
        let square = Square::ALL[sq as usize];
        bb.0 &= bb.0 - 1;
        let attacks = cozy_chess::get_bishop_moves(square, occupied) | cozy_chess::get_rook_moves(square, occupied);
        b_all_attacks |= attacks.0;
        let mobility_bonus = (attacks.0.count_ones() as i32) * 2;
        black_positional_mg += mobility_bonus;
        black_positional_eg += mobility_bonus;

        let dist = chebyshev_distance(square, w_king) as usize;
        black_positional_mg += QUEEN_PROX[dist];

        // Opponent King Gravitational Pull
        black_positional_mg += (8 - dist as i32) * 8;

        // Ray-Attack threats to White King (diagonal or orthogonal files/diagonals)
        black_positional_mg += ((square.file() == w_king.file()) as i32) * 90;
        let adj_file = ((square.file() as i32 - w_king.file() as i32).abs() == 1) as i32;
        black_positional_mg += adj_file * 50;
        let f_diff = (square.file() as i32 - w_king.file() as i32).abs();
        let r_diff = (square.rank() as i32 - w_king.rank() as i32).abs();
        black_positional_mg += ((f_diff == r_diff) as i32) * 80;

        // King ring penetration
        black_positional_mg += (((1u64 << sq) & w_king_ring.0 != 0) as i32) * 120;

        let hits = attacks.0 & w_king_ring.0;
        let is_attacking = (hits != 0) as i32;
        b_attacking_pieces += is_attacking;
        if dist <= 3 && is_attacking != 0 {
            b_close_attackers += 1;
        }
        b_attack_weight += is_attacking * 7;
        b_attack_squares |= hits;
    }

    // Bishop Pair
    let w_bishop_pair = (board.colored_pieces(Color::White, Piece::Bishop).0.count_ones() >= 2) as i32 * 40;
    white_positional_mg += w_bishop_pair;
    white_positional_eg += w_bishop_pair;

    // Bishop Pair
    let b_bishop_pair = (board.colored_pieces(Color::Black, Piece::Bishop).0.count_ones() >= 2) as i32 * 40;
    black_positional_mg += b_bishop_pair;
    black_positional_eg += b_bishop_pair;

    // Fast Bitwise Pawn Attacks Calculation
    let w_pawn_attacks = ((w_pawns_val & !FILE_A) << 7) | ((w_pawns_val & !FILE_H) << 9);
    let b_pawn_attacks = ((b_pawns_val & !FILE_A) >> 9) | ((b_pawns_val & !FILE_H) >> 7);

    // Chaos Factor Tension Bonus
    let w_total_attacks = w_all_attacks | w_pawn_attacks;
    let b_total_attacks = b_all_attacks | b_pawn_attacks;
    let w_pieces_under_attack = white_pieces.0 & b_total_attacks;
    let b_pieces_under_attack = black_pieces.0 & w_total_attacks;
    let tension_count = (w_pieces_under_attack.count_ones() + b_pieces_under_attack.count_ones()) as i32;
    if tension_count >= 3 {
        let chaos_bonus = (tension_count - 2) * 15;
        white_positional_mg += chaos_bonus;
        black_positional_mg += chaos_bonus;
    }

    // Merge White Pawn attacks on Black King Ring
    let w_pawn_king_attacks = w_pawn_attacks & b_king_ring.0;
    let w_pawn_king_atk_count = w_pawn_king_attacks.count_ones() as i32;
    w_attacking_pieces += w_pawn_king_atk_count;
    w_attack_weight += w_pawn_king_atk_count * 2;
    w_attack_squares |= w_pawn_king_attacks;

    // Merge Black Pawn attacks on White King Ring
    let b_pawn_king_attacks = b_pawn_attacks & w_king_ring.0;
    let b_pawn_king_atk_count = b_pawn_king_attacks.count_ones() as i32;
    b_attacking_pieces += b_pawn_king_atk_count;
    b_attack_weight += b_pawn_king_atk_count * 2;
    b_attack_squares |= b_pawn_king_attacks;

    let w_squares_count = w_attack_squares.count_ones() as i32;
    white_positional_mg += w_squares_count * 30; // Control space around black king

    let b_squares_count = b_attack_squares.count_ones() as i32;
    black_positional_mg += b_squares_count * 30; // Control space around white king

    // Attack Coordination Heuristics:
    // Award +25 cp for each square in the opponent's king ring that is attacked by two or more coordinating friendly pieces (including pawns).
    let w_knights = board.colored_pieces(Color::White, Piece::Knight);
    let w_bishops_queens = board.colored_pieces(Color::White, Piece::Bishop) | board.colored_pieces(Color::White, Piece::Queen);
    let w_rooks_queens = board.colored_pieces(Color::White, Piece::Rook) | board.colored_pieces(Color::White, Piece::Queen);
    let w_pawns = board.colored_pieces(Color::White, Piece::Pawn);

    let mut w_cohesive_squares = 0;
    let mut ring_bb = b_king_ring.0;
    while ring_bb != 0 {
        let sq = ring_bb.trailing_zeros() as usize;
        ring_bb &= ring_bb - 1;
        let square = Square::ALL[sq];
        
        let mut attackers = 0;
        
        // Knight attackers
        attackers += (cozy_chess::get_knight_moves(square) & w_knights).0.count_ones() as i32;
        
        // Bishop/Queen diagonal attackers
        attackers += (cozy_chess::get_bishop_moves(square, occupied) & w_bishops_queens).0.count_ones() as i32;
        
        // Rook/Queen orthogonal attackers
        attackers += (cozy_chess::get_rook_moves(square, occupied) & w_rooks_queens).0.count_ones() as i32;
        
        // Pawn attackers
        let sq_mod_8 = sq & 7;
        let right_pawn = if sq_mod_8 != 7 && sq >= 7 { (w_pawns.0 & (1 << (sq - 7))) != 0 } else { false };
        let left_pawn = if sq_mod_8 != 0 && sq >= 9 { (w_pawns.0 & (1 << (sq - 9))) != 0 } else { false };
        attackers += right_pawn as i32 + left_pawn as i32;
        
        if attackers >= 2 {
            w_cohesive_squares += 1;
            white_positional_mg += 25;
        }
    }

    let b_knights = board.colored_pieces(Color::Black, Piece::Knight);
    let b_bishops_queens = board.colored_pieces(Color::Black, Piece::Bishop) | board.colored_pieces(Color::Black, Piece::Queen);
    let b_rooks_queens = board.colored_pieces(Color::Black, Piece::Rook) | board.colored_pieces(Color::Black, Piece::Queen);
    let b_pawns = board.colored_pieces(Color::Black, Piece::Pawn);

    let mut b_cohesive_squares = 0;
    let mut ring_bb = w_king_ring.0;
    while ring_bb != 0 {
        let sq = ring_bb.trailing_zeros() as usize;
        ring_bb &= ring_bb - 1;
        let square = Square::ALL[sq];
        
        let mut attackers = 0;
        
        // Knight attackers
        attackers += (cozy_chess::get_knight_moves(square) & b_knights).0.count_ones() as i32;
        
        // Bishop/Queen diagonal attackers
        attackers += (cozy_chess::get_bishop_moves(square, occupied) & b_bishops_queens).0.count_ones() as i32;
        
        // Rook/Queen orthogonal attackers
        attackers += (cozy_chess::get_rook_moves(square, occupied) & b_rooks_queens).0.count_ones() as i32;
        
        // Pawn attackers
        let sq_mod_8 = sq & 7;
        let right_pawn = if sq_mod_8 != 0 && sq + 7 < 64 { (b_pawns.0 & (1 << (sq + 7))) != 0 } else { false };
        let left_pawn = if sq_mod_8 != 7 && sq + 9 < 64 { (b_pawns.0 & (1 << (sq + 9))) != 0 } else { false };
        attackers += right_pawn as i32 + left_pawn as i32;
        
        if attackers >= 2 {
            b_cohesive_squares += 1;
            black_positional_mg += 25;
        }
    }

    // Option 5: Piece Participation Escalation
    if w_attacking_pieces >= 4 {
        white_positional_mg += (w_attacking_pieces - 3) * (w_attacking_pieces - 3) * 60;
    }
    if b_attacking_pieces >= 4 {
        black_positional_mg += (b_attacking_pieces - 3) * (b_attacking_pieces - 3) * 60;
    }

    // Virtual escape squares for Black King (restricted by White attacks)
    let black_king_escapes = b_king_ring.0 & !board.colors(Color::Black).0 & !w_attack_squares;
    let black_escape_count = black_king_escapes.count_ones() as usize;
    white_positional_mg += CRAMP_BONUS[black_escape_count.min(9)];

    // Virtual escape squares for White King (restricted by Black attacks)
    let white_king_escapes = w_king_ring.0 & !board.colors(Color::White).0 & !b_attack_squares;
    let white_escape_count = white_king_escapes.count_ones() as usize;
    black_positional_mg += CRAMP_BONUS[white_escape_count.min(9)];

    // King Escape-Route Blockades
    let b_king_moves = cozy_chess::get_king_moves(b_king).0;
    let b_escape_routes = b_king_moves & !occupied.0;
    let w_attacks = w_all_attacks | w_pawn_attacks;
    let w_blocked_escapes = b_escape_routes & w_attacks;
    white_positional_mg += (w_blocked_escapes.count_ones() as i32) * 25;

    let w_king_moves = cozy_chess::get_king_moves(w_king).0;
    let w_escape_routes = w_king_moves & !occupied.0;
    let b_attacks = b_all_attacks | b_pawn_attacks;
    let b_blocked_escapes = w_escape_routes & b_attacks;
    black_positional_mg += (b_blocked_escapes.count_ones() as i32) * 25;


    // Dynamic Pawn Shields Check
    if w_attacking_pieces < 2 {
        white_positional_mg -= evaluate_pawn_shield(board, Color::White, w_king, w_pawns_val, w_enemy_power);
    }
    if b_attacking_pieces < 2 {
        black_positional_mg -= evaluate_pawn_shield(board, Color::Black, b_king, b_pawns_val, b_enemy_power);
    }

    // Open Center File Penalties for Center King & Castling Bonuses
    // Complete Disregard for Self-Safety (No open file penalties, castling is penalized in middlegame)
    // White King
    let w_king_val = w_king as usize;
    let w_castled = w_king_val == Square::G1 as usize || w_king_val == Square::C1 as usize || 
                    w_king_val == Square::H1 as usize || w_king_val == Square::B1 as usize || 
                    w_king_val == Square::A1 as usize;
    if phase > 12 {
        white_positional_mg += (w_castled as i32) * 50;
    } else {
        white_positional_eg += (w_castled as i32) * 25;
    }

    // Black King
    let b_king_val = b_king as usize;
    let b_castled = b_king_val == Square::G8 as usize || b_king_val == Square::C8 as usize || 
                    b_king_val == Square::H8 as usize || b_king_val == Square::B8 as usize || 
                    b_king_val == Square::A8 as usize;
    if phase > 12 {
        black_positional_mg += (b_castled as i32) * 50;
    } else {
        black_positional_eg += (b_castled as i32) * 25;
    }

    // Pawn storms (White pawn storm on Black King)
    let b_king_file = b_king.file() as i32;
    let mut bb = w_pawns_val;
    while bb != 0 {
        let sq = bb.trailing_zeros() as usize;
        let square = Square::ALL[sq];
        bb &= bb - 1; // Clear lowest set bit

        let f = square.file() as i32;
        let r = square.rank() as i32;
        let is_adjacent = (f - b_king_file).abs() <= 1;
        let term = (r - 1).max(0);
        
        let is_open_file_storm = if is_adjacent {
            let mut blocked = false;
            for r_check in (r + 1)..8 {
                let sq_check = r_check * 8 + f;
                if (b_pawns_val & (1u64 << sq_check)) != 0 {
                    blocked = true;
                    break;
                }
            }
            !blocked
        } else {
            false
        };
        let storm_bonus = (is_adjacent as i32) * term * 20;
        let open_file_bonus = if is_open_file_storm { term * 10 } else { 0 };
        white_positional_mg += storm_bonus + open_file_bonus;
    }

    // Black pawn storm on White King
    let w_king_file = w_king.file() as i32;
    let mut bb = b_pawns_val;
    while bb != 0 {
        let sq = bb.trailing_zeros() as usize;
        let square = Square::ALL[sq];
        bb &= bb - 1; // Clear lowest set bit

        let f = square.file() as i32;
        let r = square.rank() as i32;
        let is_adjacent = (f - w_king_file).abs() <= 1;
        let term = (6 - r).max(0);
        
        let is_open_file_storm = if is_adjacent {
            let mut blocked = false;
            for r_check in 0..r {
                let sq_check = r_check * 8 + f;
                if (w_pawns_val & (1u64 << sq_check)) != 0 {
                    blocked = true;
                    break;
                }
            }
            !blocked
        } else {
            false
        };
        let storm_bonus = (is_adjacent as i32) * term * 20;
        let open_file_bonus = if is_open_file_storm { term * 10 } else { 0 };
        black_positional_mg += storm_bonus + open_file_bonus;
    }

    // Passed Pawns Progressive Bonuses
    // White Pawns:
    let mut bb = board.colored_pieces(Color::White, Piece::Pawn);
    while bb.0 != 0 {
        let sq = bb.0.trailing_zeros() as usize;
        bb.0 &= bb.0 - 1;
        let r = sq >> 3;
        
        let is_passed = (b_pawns_val & PASSED_PAWN_MASK_WHITE[sq]) == 0;
        white_positional_mg += (is_passed as i32) * PASSED_PAWN_MG_WHITE[r];
        white_positional_eg += (is_passed as i32) * PASSED_PAWN_EG_WHITE[r];

        // Line-Opening Pawn Sacrifices
        let is_sac_break = ((b_pawn_attacks & (1 << sq)) != 0) &&
            (((sq % 8) as i32 - b_king.file() as i32).abs() <= 1);
        white_positional_mg += (is_sac_break as i32) * 30;
    }

    // Black Pawns:
    let mut bb = board.colored_pieces(Color::Black, Piece::Pawn);
    while bb.0 != 0 {
        let sq = bb.0.trailing_zeros() as usize;
        bb.0 &= bb.0 - 1;
        let r = sq >> 3;
        
        let is_passed = (w_pawns_val & PASSED_PAWN_MASK_BLACK[sq]) == 0;
        black_positional_mg += (is_passed as i32) * PASSED_PAWN_MG_BLACK[r];
        black_positional_eg += (is_passed as i32) * PASSED_PAWN_EG_BLACK[r];

        // Line-Opening Pawn Sacrifices
        let is_sac_break = ((w_pawn_attacks & (1 << sq)) != 0) &&
            (((sq % 8) as i32 - w_king.file() as i32).abs() <= 1);
        black_positional_mg += (is_sac_break as i32) * 30;
    }

    // Option 8: Passive Piece Penalties
    if w_attacking_pieces >= 2 {
        let w_back_ranks = 0x000000000000FFFFu64; // rank 1 and 2
        let w_minors_majors = board.colored_pieces(Color::White, Piece::Knight).0 |
                              board.colored_pieces(Color::White, Piece::Bishop).0 |
                              board.colored_pieces(Color::White, Piece::Rook).0 |
                              board.colored_pieces(Color::White, Piece::Queen).0;
        let w_passive = (w_minors_majors & w_back_ranks).count_ones() as i32;
        white_positional_mg -= w_passive * 20 * phase as i32 / 24;
    }
    if b_attacking_pieces >= 2 {
        let b_back_ranks = 0xFFFF000000000000u64; // rank 7 and 8
        let b_minors_majors = board.colored_pieces(Color::Black, Piece::Knight).0 |
                              board.colored_pieces(Color::Black, Piece::Bishop).0 |
                              board.colored_pieces(Color::Black, Piece::Rook).0 |
                              board.colored_pieces(Color::Black, Piece::Queen).0;
        let b_passive = (b_minors_majors & b_back_ranks).count_ones() as i32;
        black_positional_mg -= b_passive * 20 * phase as i32 / 24;
    }

    // Attacking Initiative Bonus (Active Sacrifice Bonus)
    if white_material < black_material && w_attacking_pieces >= 2 {
        let deficit = black_material - white_material;
        if deficit >= 100 {
            let mut sac_bonus = 120 + w_attack_weight * 3;
            if deficit >= 800 {
                sac_bonus += 300 + w_attack_weight * 5;
            }
            white_positional_mg += sac_bonus * phase as i32 / 24;
        }
    }

    if black_material < white_material && b_attacking_pieces >= 2 {
        let deficit = white_material - black_material;
        if deficit >= 100 {
            let mut sac_bonus = 120 + b_attack_weight * 3;
            if deficit >= 800 {
                sac_bonus += 300 + b_attack_weight * 5;
            }
            black_positional_mg += sac_bonus * phase as i32 / 24;
        }
    }

    // Positional difference
    let pos_diff = white_positional_mg - black_positional_mg;

    // Dynamic Positional Scaling of King Attack Bonus
    let mut w_attack_bonus = if w_attacking_pieces >= 2 {
        KING_ATTACK_BONUS[w_attack_weight.min(31) as usize]
    } else {
        0
    };
    if pos_diff > 0 && w_attack_bonus > 0 {
        let multiplier = 100 + (pos_diff / 20).min(50); // up to +50%
        w_attack_bonus = w_attack_bonus * multiplier / 100;
    }
    white_positional_mg += w_attack_bonus;

    let mut b_attack_bonus = if b_attacking_pieces >= 2 {
        KING_ATTACK_BONUS[b_attack_weight.min(31) as usize]
    } else {
        0
    };
    if pos_diff < 0 && b_attack_bonus > 0 {
        let multiplier = 100 + ((-pos_diff) / 20).min(50); // up to +50%
        b_attack_bonus = b_attack_bonus * multiplier / 100;
    }
    black_positional_mg += b_attack_bonus;

    // Phase and Dynamic Material Discount (Speculative Sacrifices)
    let phase = phase.min(24);

    let mut final_white_material = white_material;
    let mut final_black_material = black_material;

    // Calculate pawn shield damage for sacrifice scaling
    let b_king = board.king(Color::Black);
    let b_shield_files: &[usize] = if b_king.file() as usize >= 5 {
        &[5, 6, 7]
    } else if b_king.file() as usize <= 2 {
        &[0, 1, 2]
    } else {
        &[3, 4]
    };
    let mut b_shield_damage = 0;
    let b_pawns = board.colored_pieces(Color::Black, Piece::Pawn);
    for &file in b_shield_files {
        let base_sq = Square::new(cozy_chess::File::ALL[file], cozy_chess::Rank::ALL[6]);
        if b_pawns.has(base_sq) {
            // Pawn is on its base starting square
        } else {
            let mut pushed = false;
            for r in 4..=5 {
                let check_sq = Square::new(cozy_chess::File::ALL[file], cozy_chess::Rank::ALL[r]);
                if b_pawns.has(check_sq) {
                    pushed = true;
                    break;
                }
            }
            if pushed {
                b_shield_damage += 5; // Pushed pawn shield: +5% discount
            } else {
                b_shield_damage += 10; // Missing pawn shield: +10% discount
            }
        }
    }

    let w_king = board.king(Color::White);
    let w_shield_files: &[usize] = if w_king.file() as usize >= 5 {
        &[5, 6, 7]
    } else if w_king.file() as usize <= 2 {
        &[0, 1, 2]
    } else {
        &[3, 4]
    };
    let mut w_shield_damage = 0;
    let w_pawns = board.colored_pieces(Color::White, Piece::Pawn);
    for &file in w_shield_files {
        let base_sq = Square::new(cozy_chess::File::ALL[file], cozy_chess::Rank::ALL[1]);
        if w_pawns.has(base_sq) {
            // Pawn is on base
        } else {
            let mut pushed = false;
            for r in 2..=3 {
                let check_sq = Square::new(cozy_chess::File::ALL[file], cozy_chess::Rank::ALL[r]);
                if w_pawns.has(check_sq) {
                    pushed = true;
                    break;
                }
            }
            if pushed {
                w_shield_damage += 5; // Pushed pawn: +5%
            } else {
                w_shield_damage += 10; // Missing pawn: +10%
            }
        }
    }

    if white_material < black_material {
        let deficit = black_material - white_material;
        if w_attacking_pieces >= 2 && w_close_attackers >= 2 && w_cohesive_squares >= 1 {
            let b_dist_2_mask = {
                let mut mask = cozy_chess::get_king_moves(b_king) | BitBoard(1 << (b_king as u64));
                let mut temp = mask;
                while temp.0 != 0 {
                    let sq = Square::ALL[temp.0.trailing_zeros() as usize];
                    temp.0 &= temp.0 - 1;
                    mask |= cozy_chess::get_king_moves(sq);
                }
                mask
            };
            let b_defenders_near = (board.colors(Color::Black).0 & b_dist_2_mask.0).count_ones() as i32;

            let mut discount_pct = (w_attacking_pieces * 6 + w_attack_weight * 3).min(50);
            if pos_diff > 0 {
                discount_pct += (pos_diff / 50).min(10); // up to +10% more discount
            }
            if w_attacking_pieces >= 3 {
                discount_pct += 15;
            }
            discount_pct += b_shield_damage; // Add shield damage to discount
            
            // Option 6: Opponent Defenses Counter-Scaling
            if b_defenders_near >= 4 {
                discount_pct = (discount_pct - (b_defenders_near - 3) * 8).max(0);
            }

            let cap = if is_closed { 85 } else { 90 };
            discount_pct = discount_pct.min(cap); // Cap at 85% (closed) or 90% (open)
            let discounted_deficit = deficit * discount_pct / 100;
            let discounted_deficit = discounted_deficit * phase as i32 / 24;
            final_white_material += discounted_deficit;
        }
    }

    if black_material < white_material {
        let deficit = white_material - black_material;
        if b_attacking_pieces >= 2 && b_close_attackers >= 2 && b_cohesive_squares >= 1 {
            let w_dist_2_mask = {
                let mut mask = cozy_chess::get_king_moves(w_king) | BitBoard(1 << (w_king as u64));
                let mut temp = mask;
                while temp.0 != 0 {
                    let sq = Square::ALL[temp.0.trailing_zeros() as usize];
                    temp.0 &= temp.0 - 1;
                    mask |= cozy_chess::get_king_moves(sq);
                }
                mask
            };
            let w_defenders_near = (board.colors(Color::White).0 & w_dist_2_mask.0).count_ones() as i32;

            let mut discount_pct = (b_attacking_pieces * 6 + b_attack_weight * 3).min(50);
            if pos_diff < 0 {
                discount_pct += ((-pos_diff) / 50).min(10); // up to +10% more discount
            }
            if b_attacking_pieces >= 3 {
                discount_pct += 15;
            }
            discount_pct += w_shield_damage; // Add shield damage to discount

            // Option 6: Opponent Defenses Counter-Scaling
            if w_defenders_near >= 4 {
                discount_pct = (discount_pct - (w_defenders_near - 3) * 8).max(0);
            }

            let cap = if is_closed { 85 } else { 90 };
            discount_pct = discount_pct.min(cap); // Cap at 85% (closed) or 90% (open)
            let discounted_deficit = deficit * discount_pct / 100;
            let discounted_deficit = discounted_deficit * phase as i32 / 24;
            final_black_material += discounted_deficit;
        }
    }

    // Knight & Bishop Outposts
    let w_pawns = board.colored_pieces(Color::White, Piece::Pawn);
    let b_pawns = board.colored_pieces(Color::Black, Piece::Pawn);

    // White Outposts
    // White Knights
    let mut bb = board.colored_pieces(Color::White, Piece::Knight);
    while bb.0 != 0 {
        let sq_idx = bb.0.trailing_zeros() as usize;
        let sq = Square::ALL[sq_idx];
        bb.0 &= bb.0 - 1;
        let rank = sq.rank() as usize;
        if rank >= 3 && rank <= 5 {
            let file = sq.file() as usize;
            let mut defended = false;
            if file > 0 {
                let pawn_left = Square::new(cozy_chess::File::ALL[file - 1], cozy_chess::Rank::ALL[rank - 1]);
                if w_pawns.has(pawn_left) {
                    defended = true;
                }
            }
            if file < 7 {
                let pawn_right = Square::new(cozy_chess::File::ALL[file + 1], cozy_chess::Rank::ALL[rank - 1]);
                if w_pawns.has(pawn_right) {
                    defended = true;
                }
            }
            if defended {
                let mut weak = true;
                if file > 0 {
                    for r_check in rank..8 {
                        let check_sq = Square::new(cozy_chess::File::ALL[file - 1], cozy_chess::Rank::ALL[r_check]);
                        if b_pawns.has(check_sq) {
                            weak = false;
                            break;
                        }
                    }
                }
                if file < 7 {
                    for r_check in rank..8 {
                        let check_sq = Square::new(cozy_chess::File::ALL[file + 1], cozy_chess::Rank::ALL[r_check]);
                        if b_pawns.has(check_sq) {
                            weak = false;
                            break;
                        }
                    }
                }
                if weak {
                    white_positional_mg += knight_outpost_mg;
                    white_positional_eg += knight_outpost_eg;
                }
            }
        }
    }

    // White Bishops
    let mut bb = board.colored_pieces(Color::White, Piece::Bishop);
    while bb.0 != 0 {
        let sq_idx = bb.0.trailing_zeros() as usize;
        let sq = Square::ALL[sq_idx];
        bb.0 &= bb.0 - 1;
        let rank = sq.rank() as usize;
        if rank >= 3 && rank <= 5 {
            let file = sq.file() as usize;
            let mut defended = false;
            if file > 0 {
                let pawn_left = Square::new(cozy_chess::File::ALL[file - 1], cozy_chess::Rank::ALL[rank - 1]);
                if w_pawns.has(pawn_left) {
                    defended = true;
                }
            }
            if file < 7 {
                let pawn_right = Square::new(cozy_chess::File::ALL[file + 1], cozy_chess::Rank::ALL[rank - 1]);
                if w_pawns.has(pawn_right) {
                    defended = true;
                }
            }
            if defended {
                let mut weak = true;
                if file > 0 {
                    for r_check in rank..8 {
                        let check_sq = Square::new(cozy_chess::File::ALL[file - 1], cozy_chess::Rank::ALL[r_check]);
                        if b_pawns.has(check_sq) {
                            weak = false;
                            break;
                        }
                    }
                }
                if file < 7 {
                    for r_check in rank..8 {
                        let check_sq = Square::new(cozy_chess::File::ALL[file + 1], cozy_chess::Rank::ALL[r_check]);
                        if b_pawns.has(check_sq) {
                            weak = false;
                            break;
                        }
                    }
                }
                if weak {
                    white_positional_mg += bishop_outpost_mg;
                    white_positional_eg += bishop_outpost_eg;
                }
            }
        }
    }

    // Black Outposts
    // Black Knights
    let mut bb = board.colored_pieces(Color::Black, Piece::Knight);
    while bb.0 != 0 {
        let sq_idx = bb.0.trailing_zeros() as usize;
        let sq = Square::ALL[sq_idx];
        bb.0 &= bb.0 - 1;
        let rank = sq.rank() as usize;
        if rank >= 2 && rank <= 4 {
            let file = sq.file() as usize;
            let mut defended = false;
            if file > 0 {
                let pawn_left = Square::new(cozy_chess::File::ALL[file - 1], cozy_chess::Rank::ALL[rank + 1]);
                if b_pawns.has(pawn_left) {
                    defended = true;
                }
            }
            if file < 7 {
                let pawn_right = Square::new(cozy_chess::File::ALL[file + 1], cozy_chess::Rank::ALL[rank + 1]);
                if b_pawns.has(pawn_right) {
                    defended = true;
                }
            }
            if defended {
                let mut weak = true;
                if file > 0 {
                    for r_check in 0..=rank {
                        let check_sq = Square::new(cozy_chess::File::ALL[file - 1], cozy_chess::Rank::ALL[r_check]);
                        if w_pawns.has(check_sq) {
                            weak = false;
                            break;
                        }
                    }
                }
                if file < 7 {
                    for r_check in 0..=rank {
                        let check_sq = Square::new(cozy_chess::File::ALL[file + 1], cozy_chess::Rank::ALL[r_check]);
                        if w_pawns.has(check_sq) {
                            weak = false;
                            break;
                        }
                    }
                }
                if weak {
                    black_positional_mg += knight_outpost_mg;
                    black_positional_eg += knight_outpost_eg;
                }
            }
        }
    }

    // Black Bishops
    let mut bb = board.colored_pieces(Color::Black, Piece::Bishop);
    while bb.0 != 0 {
        let sq_idx = bb.0.trailing_zeros() as usize;
        let sq = Square::ALL[sq_idx];
        bb.0 &= bb.0 - 1;
        let rank = sq.rank() as usize;
        if rank >= 2 && rank <= 4 {
            let file = sq.file() as usize;
            let mut defended = false;
            if file > 0 {
                let pawn_left = Square::new(cozy_chess::File::ALL[file - 1], cozy_chess::Rank::ALL[rank + 1]);
                if b_pawns.has(pawn_left) {
                    defended = true;
                }
            }
            if file < 7 {
                let pawn_right = Square::new(cozy_chess::File::ALL[file + 1], cozy_chess::Rank::ALL[rank + 1]);
                if b_pawns.has(pawn_right) {
                    defended = true;
                }
            }
            if defended {
                let mut weak = true;
                if file > 0 {
                    for r_check in 0..=rank {
                        let check_sq = Square::new(cozy_chess::File::ALL[file - 1], cozy_chess::Rank::ALL[r_check]);
                        if w_pawns.has(check_sq) {
                            weak = false;
                            break;
                        }
                    }
                }
                if file < 7 {
                    for r_check in 0..=rank {
                        let check_sq = Square::new(cozy_chess::File::ALL[file + 1], cozy_chess::Rank::ALL[r_check]);
                        if w_pawns.has(check_sq) {
                            weak = false;
                            break;
                        }
                    }
                }
                if weak {
                    black_positional_mg += bishop_outpost_mg;
                    black_positional_eg += bishop_outpost_eg;
                }
            }
        }
    }

    // Pawn Storm Incentives
    if b_king.file() as usize >= 5 {
        for f in 5..=7 {
            for r in 3..=5 {
                let p_sq = Square::new(cozy_chess::File::ALL[f], cozy_chess::Rank::ALL[r]);
                if w_pawns.has(p_sq) {
                    let bonus = match r {
                        3 => 20,
                        4 => 50,
                        5 => 100,
                        _ => 0,
                    };
                    white_positional_mg += bonus;
                }
            }
        }
    } else if b_king.file() as usize <= 2 {
        for f in 0..=2 {
            for r in 3..=5 {
                let p_sq = Square::new(cozy_chess::File::ALL[f], cozy_chess::Rank::ALL[r]);
                if w_pawns.has(p_sq) {
                    let bonus = match r {
                        3 => 20,
                        4 => 50,
                        5 => 100,
                        _ => 0,
                    };
                    white_positional_mg += bonus;
                }
            }
        }
    }

    if w_king.file() as usize >= 5 {
        for f in 5..=7 {
            for r in 2..=4 {
                let p_sq = Square::new(cozy_chess::File::ALL[f], cozy_chess::Rank::ALL[r]);
                if b_pawns.has(p_sq) {
                    let bonus = match r {
                        4 => 20,
                        3 => 50,
                        2 => 100,
                        _ => 0,
                    };
                    black_positional_mg += bonus;
                }
            }
        }
    } else if w_king.file() as usize <= 2 {
        for f in 0..=2 {
            for r in 2..=4 {
                let p_sq = Square::new(cozy_chess::File::ALL[f], cozy_chess::Rank::ALL[r]);
                if b_pawns.has(p_sq) {
                    let bonus = match r {
                        4 => 20,
                        3 => 50,
                        2 => 100,
                        _ => 0,
                    };
                    black_positional_mg += bonus;
                }
            }
        }
    }

    // King Shield Breakers
    let b_king_ring = cozy_chess::get_king_moves(b_king) | BitBoard(1 << (b_king as u64));
    let w_knights = board.colored_pieces(Color::White, Piece::Knight);
    let w_bishops = board.colored_pieces(Color::White, Piece::Bishop);
    let w_rooks = board.colored_pieces(Color::White, Piece::Rook);
    let w_attackers_in_ring = (w_knights | w_bishops | w_rooks) & b_king_ring;
    let shield_breaker_bonus = if is_closed { 160 } else { 120 };
    if w_attackers_in_ring.0 != 0 {
        let count = w_attackers_in_ring.0.count_ones() as i32;
        white_positional_mg += count * shield_breaker_bonus;
    }

    let w_king_ring = cozy_chess::get_king_moves(w_king) | BitBoard(1 << (w_king as u64));
    let b_knights = board.colored_pieces(Color::Black, Piece::Knight);
    let b_bishops = board.colored_pieces(Color::Black, Piece::Bishop);
    let b_rooks = board.colored_pieces(Color::Black, Piece::Rook);
    let b_attackers_in_ring = (b_knights | b_bishops | b_rooks) & w_king_ring;
    if b_attackers_in_ring.0 != 0 {
        let count = b_attackers_in_ring.0.count_ones() as i32;
        black_positional_mg += count * shield_breaker_bonus;
    }

    // Dynamic Asymmetric PSTs (Swarming the King's Wing)
    const KINGSIDE_MASK: u64 = 0xF0F0F0F0F0F0F0F0;
    const QUEENSIDE_MASK: u64 = 0x0F0F0F0F0F0F0F0F;

    let b_king_file = b_king.file() as usize;
    let target_mask_w = if b_king_file >= 4 { KINGSIDE_MASK } else { QUEENSIDE_MASK };
    let w_queens = board.colored_pieces(Color::White, Piece::Queen);
    let w_swarming = (w_knights | w_bishops | w_queens).0 & target_mask_w;
    let swarming_bonus = if is_closed { 40 } else { 25 };
    if w_swarming != 0 {
        let count = w_swarming.count_ones() as i32;
        white_positional_mg += count * swarming_bonus;
    }

    let w_king_file = w_king.file() as usize;
    let target_mask_b = if w_king_file >= 4 { KINGSIDE_MASK } else { QUEENSIDE_MASK };
    let b_queens = board.colored_pieces(Color::Black, Piece::Queen);
    let b_swarming = (b_knights | b_bishops | b_queens).0 & target_mask_b;
    if b_swarming != 0 {
        let count = b_swarming.count_ones() as i32;
        black_positional_mg += count * swarming_bonus;
    }

    // Final Middlegame and Endgame Scores
    let mg_white = final_white_material + white_positional_mg;
    let eg_white = final_white_material + white_positional_eg;
    let mg_black = final_black_material + black_positional_mg;
    let eg_black = final_black_material + black_positional_eg;

    // Tapered evaluation interpolation
    let mg_score = mg_white - mg_black;
    let eg_score = eg_white - eg_black;

    let score = (mg_score * phase + eg_score * (24 - phase)) / 24;

    let mut final_score = score;
    if is_closed {
        if board.side_to_move() == Color::White {
            final_score += 20;
        } else {
            final_score -= 20;
        }
    }

    let mut side_score = if board.side_to_move() == Color::White {
        final_score
    } else {
        -final_score
    };

    // Forcing Moves & "Panic" Bonuses
    // If the player to move is in check, count their legal moves.
    // If they have exactly 1 legal reply (forced evasion), penalize them by -80 cp.
    if !board.checkers().is_empty() {
        let mut legal_moves = 0;
        board.generate_moves(|mvs| {
            for m in mvs {
                if board.is_legal(m) {
                    legal_moves += 1;
                }
            }
            false
        });
        if legal_moves == 1 {
            side_score -= 80;
        }
    }

    side_score
}

pub fn has_material_imbalance(board: &Board) -> bool {
    let w_q = board.colored_pieces(Color::White, Piece::Queen).0.count_ones();
    let b_q = board.colored_pieces(Color::Black, Piece::Queen).0.count_ones();
    let w_r = board.colored_pieces(Color::White, Piece::Rook).0.count_ones();
    let b_r = board.colored_pieces(Color::Black, Piece::Rook).0.count_ones();
    let w_minors = board.colored_pieces(Color::White, Piece::Bishop).0.count_ones() 
                 + board.colored_pieces(Color::White, Piece::Knight).0.count_ones();
    let b_minors = board.colored_pieces(Color::Black, Piece::Bishop).0.count_ones() 
                 + board.colored_pieces(Color::Black, Piece::Knight).0.count_ones();

    // 1. Queen vs 3+ Minor Pieces
    if (w_q == 1 && b_q == 0 && b_minors >= 3) || (b_q == 1 && w_q == 0 && w_minors >= 3) {
        return true;
    }
    // 2. Queen vs Rook + Minor Piece(s)
    if (w_q == 1 && b_q == 0 && b_r >= 1 && b_minors >= 1) || (b_q == 1 && w_q == 0 && w_r >= 1 && w_minors >= 1) {
        return true;
    }
    // 3. Two Rooks vs Queen
    if (w_r >= 2 && w_q == 0 && b_q == 1 && b_r <= 1) || (b_r >= 2 && b_q == 0 && w_q == 1 && w_r <= 1) {
        return true;
    }
    // 4. Rook vs 2 Minor Pieces
    if (w_r >= 1 && w_minors == 0 && b_r == 0 && b_minors >= 2) || (b_r >= 1 && b_minors == 0 && w_r == 0 && w_minors >= 2) {
        return true;
    }

    false
}

pub fn locked_center_status(board: &Board) -> (bool, i32) {
    let w_pawns_val = board.colored_pieces(Color::White, Piece::Pawn).0;
    let b_pawns_val = board.colored_pieces(Color::Black, Piece::Pawn).0;

    let c_locked = ((w_pawns_val >> 26) & (b_pawns_val >> 34)) & 1;
    let d_locked = ((w_pawns_val >> 27) & (b_pawns_val >> 35)) & 1;
    let e_locked = ((w_pawns_val >> 28) & (b_pawns_val >> 36)) & 1;
    let locked_count = (c_locked + d_locked + e_locked) as i32 * 2;

    (locked_count > 0, locked_count)
}

