//! AggroChess evaluation.
//!
//! The evaluation is built in two layers:
//!
//! 1. A *sound* classical core: PeSTO piece-square tables, pawn structure,
//!    mobility, threats, passed pawns and a Stockfish-style quadratic
//!    king-danger model. This is what lets the engine tell a real attack from
//!    a fake one.
//! 2. A *personality* layer (the "Tal" part): the danger the engine creates
//!    against the opponent's king is multiplied by `AGGRESSION` (default 130%),
//!    the engine prefers keeping pieces (and queens) on the board, and draws
//!    are scored as losses by `CONTEMPT` in the search.
//!
//! Because king danger grows quadratically with the number of attackers, safe
//! checks and weak squares around the king, a sacrifice that rips the king
//! open with several pieces ready to join *does* evaluate well, while a "spite
//! sac" with nothing behind it does not. That is the difference between a
//! beautiful sacrifice and a blunder.

use cozy_chess::{BitBoard, Board, Color, Piece, Square};
use std::sync::atomic::{AtomicI32, AtomicU8, Ordering};

/// Percentage applied to the king danger *the engine* inflicts on its opponent.
/// 100 = objective, 180 = default ("Tal mode", tuned for playing humans).
pub static AGGRESSION: AtomicI32 = AtomicI32::new(220);
/// 0 = white, 1 = black, 2 = no engine side (fully symmetric evaluation).
pub static ENGINE_COLOR: AtomicU8 = AtomicU8::new(2);

/// Evaluation noise amplitude in centipawns (used by the Skill Level option).
pub static NOISE: AtomicI32 = AtomicI32::new(0);
pub static NOISE_SEED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn set_engine_color(color: Option<Color>) {
    let v = match color {
        Some(Color::White) => 0,
        Some(Color::Black) => 1,
        None => 2,
    };
    ENGINE_COLOR.store(v, Ordering::Relaxed);
}

fn engine_color() -> Option<Color> {
    match ENGINE_COLOR.load(Ordering::Relaxed) {
        0 => Some(Color::White),
        1 => Some(Color::Black),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Material and piece-square tables (PeSTO, by Ronald Friederich)
// Tables are laid out a8..h8, a7..h7, ..., a1..h1 (rank 8 first).
// ---------------------------------------------------------------------------

pub const MG_VALUE: [i32; 6] = [82, 337, 365, 477, 1025, 0];
pub const EG_VALUE: [i32; 6] = [94, 281, 297, 512, 936, 0];
/// Simple values used by SEE, move ordering and material bookkeeping.
pub const SEE_VALUE: [i32; 6] = [100, 320, 330, 500, 950, 20000];
const PHASE_INC: [i32; 6] = [0, 1, 1, 2, 4, 0];

#[rustfmt::skip]
const MG_PST: [[i32; 64]; 6] = [
    // pawn
    [  0,   0,   0,   0,   0,   0,  0,   0,
      98, 134,  61,  95,  68, 126, 34, -11,
      -6,   7,  26,  31,  65,  56, 25, -20,
     -14,  13,   6,  21,  23,  12, 17, -23,
     -27,  -2,  -5,  12,  17,   6, 10, -25,
     -26,  -4,  -4, -10,   3,   3, 33, -12,
     -35,  -1, -20, -23, -15,  24, 38, -22,
       0,   0,   0,   0,   0,   0,  0,   0],
    // knight
    [-167, -89, -34, -49,  61, -97, -15, -107,
      -73, -41,  72,  36,  23,  62,   7,  -17,
      -47,  60,  37,  65,  84, 129,  73,   44,
       -9,  17,  19,  53,  37,  69,  18,   22,
      -13,   4,  16,  13,  28,  19,  21,   -8,
      -23,  -9,  12,  10,  19,  17,  25,  -16,
      -29, -53, -12,  -3,  -1,  18, -14,  -19,
     -105, -21, -58, -33, -17, -28, -19,  -23],
    // bishop
    [-29,   4, -82, -37, -25, -42,   7,  -8,
     -26,  16, -18, -13,  30,  59,  18, -47,
     -16,  37,  43,  40,  35,  50,  37,  -2,
      -4,   5,  19,  50,  37,  37,   7,  -2,
      -6,  13,  13,  26,  34,  12,  10,   4,
       0,  15,  15,  15,  14,  27,  18,  10,
       4,  15,  16,   0,   7,  21,  33,   1,
     -33,  -3, -14, -21, -13, -12, -39, -21],
    // rook
    [ 32,  42,  32,  51, 63,  9,  31,  43,
      27,  32,  58,  62, 80, 67,  26,  44,
      -5,  19,  26,  36, 17, 45,  61,  16,
     -24, -11,   7,  26, 24, 35,  -8, -20,
     -36, -26, -12,  -1,  9, -7,   6, -23,
     -45, -25, -16, -17,  3,  0,  -5, -33,
     -44, -16, -20,  -9, -1, 11,  -6, -71,
     -19, -13,   1,  17, 16,  7, -37, -26],
    // queen
    [-28,   0,  29,  12,  59,  44,  43,  45,
     -24, -39,  -5,   1, -16,  57,  28,  54,
     -13, -17,   7,   8,  29,  56,  47,  57,
     -27, -27, -16, -16,  -1,  17,  -2,   1,
      -9, -26,  -9, -10,  -2,  -4,   3,  -3,
     -14,   2, -11,  -2,  -5,   2,  14,   5,
     -35,  -8,  11,   2,   8,  15,  -3,   1,
      -1, -18,  -9,  10, -15, -25, -31, -50],
    // king
    [-65,  23,  16, -15, -56, -34,   2,  13,
      29,  -1, -20,  -7,  -8,  -4, -38, -29,
      -9,  24,   2, -16, -20,   6,  22, -22,
     -17, -20, -12, -27, -30, -25, -14, -36,
     -49,  -1, -27, -39, -46, -44, -33, -51,
     -14, -14, -22, -46, -44, -30, -15, -27,
       1,   7,  -8, -64, -43, -16,   9,   8,
     -15,  36,  12, -54,   8, -28,  24,  14],
];

#[rustfmt::skip]
const EG_PST: [[i32; 64]; 6] = [
    // pawn
    [  0,   0,   0,   0,   0,   0,   0,   0,
     178, 173, 158, 134, 147, 132, 165, 187,
      94, 100,  85,  67,  56,  53,  82,  84,
      32,  24,  13,   5,  -2,   4,  17,  17,
      13,   9,  -3,  -7,  -7,  -8,   3,  -1,
       4,   7,  -6,   1,   0,  -5,  -1,  -8,
      13,   8,   8,  10,  13,   0,   2,  -7,
       0,   0,   0,   0,   0,   0,   0,   0],
    // knight
    [-58, -38, -13, -28, -31, -27, -63, -99,
     -25,  -8, -25,  -2,  -9, -25, -24, -52,
     -24, -20,  10,   9,  -1,  -9, -19, -41,
     -17,   3,  22,  22,  22,  11,   8, -18,
     -18,  -6,  16,  25,  16,  17,   4, -18,
     -23,  -3,  -1,  15,  10,  -3, -20, -22,
     -42, -20, -10,  -5,  -2, -20, -23, -44,
     -29, -51, -23, -15, -22, -18, -50, -64],
    // bishop
    [-14, -21, -11,  -8, -7,  -9, -17, -24,
      -8,  -4,   7, -12, -3, -13,  -4, -14,
       2,  -8,   0,  -1, -2,   6,   0,   4,
      -3,   9,  12,   9, 14,  10,   3,   2,
      -6,   3,  13,  19,  7,  10,  -3,  -9,
     -12,  -3,   8,  10, 13,   3,  -7, -15,
     -14, -18,  -7,  -1,  4,  -9, -15, -27,
     -23,  -9, -23,  -5, -9, -16,  -5, -17],
    // rook
    [13, 10, 18, 15, 12,  12,   8,   5,
     11, 13, 13, 11, -3,   3,   8,   3,
      7,  7,  7,  5,  4,  -3,  -5,  -3,
      4,  3, 13,  1,  2,   1,  -1,   2,
      3,  5,  8,  4, -5,  -6,  -8, -11,
     -4,  0, -5, -1, -7, -12,  -8, -16,
     -6, -6,  0,  2, -9,  -9, -11,  -3,
     -9,  2,  3, -1, -5, -13,   4, -20],
    // queen
    [ -9,  22,  22,  27,  27,  19,  10,  20,
     -17,  20,  32,  41,  58,  25,  30,   0,
     -20,   6,   9,  49,  47,  35,  19,   9,
       3,  22,  24,  45,  57,  40,  57,  36,
     -18,  28,  19,  47,  31,  34,  39,  23,
     -16, -27,  15,   6,   9,  17,  10,   5,
     -22, -23, -30, -16, -16, -23, -36, -32,
     -33, -28, -22, -43,  -5, -32, -20, -41],
    // king
    [-74, -35, -18, -18, -11,  15,   4, -17,
     -12,  17,  14,  17,  17,  38,  23,  11,
      10,  17,  23,  15,  20,  45,  44,  13,
      -8,  22,  24,  27,  26,  33,  26,   3,
     -18,  -4,  21,  24,  27,  23,   9, -11,
     -19,  -3,  11,  21,  23,  16,   7,  -9,
     -27, -11,   4,  13,  14,   4,  -5, -17,
     -53, -34, -21, -11, -28, -14, -24, -43],
];

#[inline(always)]
fn pst_index(color: Color, sq: usize) -> usize {
    match color {
        Color::White => sq ^ 56,
        Color::Black => sq,
    }
}

// ---------------------------------------------------------------------------
// Bitboard helpers
// ---------------------------------------------------------------------------

const FILE_A: u64 = 0x0101_0101_0101_0101;
const FILE_H: u64 = FILE_A << 7;
const NOT_A: u64 = !FILE_A;
const NOT_H: u64 = !FILE_H;

#[inline(always)]
pub fn pawn_attacks(pawns: u64, color: Color) -> u64 {
    match color {
        Color::White => ((pawns & NOT_A) << 7) | ((pawns & NOT_H) << 9),
        Color::Black => ((pawns & NOT_A) >> 9) | ((pawns & NOT_H) >> 7),
    }
}

#[inline(always)]
fn pawn_double_attacks(pawns: u64, color: Color) -> u64 {
    match color {
        Color::White => ((pawns & NOT_A) << 7) & ((pawns & NOT_H) << 9),
        Color::Black => ((pawns & NOT_A) >> 9) & ((pawns & NOT_H) >> 7),
    }
}

#[inline(always)]
fn fill_north(mut b: u64) -> u64 {
    b |= b << 8;
    b |= b << 16;
    b |= b << 32;
    b
}

#[inline(always)]
fn fill_south(mut b: u64) -> u64 {
    b |= b >> 8;
    b |= b >> 16;
    b |= b >> 32;
    b
}

/// Squares strictly in front of the given squares, from `color`'s point of view.
#[inline(always)]
fn front_span(b: u64, color: Color) -> u64 {
    match color {
        Color::White => fill_north(b << 8),
        Color::Black => fill_south(b >> 8),
    }
}

#[inline(always)]
fn forward(b: u64, color: Color) -> u64 {
    match color {
        Color::White => b << 8,
        Color::Black => b >> 8,
    }
}

#[inline(always)]
fn adjacent_files(b: u64) -> u64 {
    ((b & NOT_A) >> 1) | ((b & NOT_H) << 1)
}

#[inline(always)]
fn sq(i: u32) -> Square {
    Square::index(i as usize)
}

#[inline(always)]
fn rel_rank(color: Color, sq_idx: u32) -> i32 {
    let r = (sq_idx / 8) as i32;
    match color {
        Color::White => r,
        Color::Black => 7 - r,
    }
}

#[inline(always)]
fn dist(a: u32, b: u32) -> i32 {
    let (fa, ra) = ((a % 8) as i32, (a / 8) as i32);
    let (fb, rb) = ((b % 8) as i32, (b / 8) as i32);
    (fa - fb).abs().max((ra - rb).abs())
}

#[inline(always)]
fn idx(c: Color) -> usize {
    c as usize
}

// ---------------------------------------------------------------------------
// Pawn structure (cached)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Default)]
struct PawnEntry {
    wp: u64,
    bp: u64,
    valid: bool,
    mg: [i32; 2],
    eg: [i32; 2],
    passed: [u64; 2],
}

const PAWN_CACHE_SIZE: usize = 1 << 14;

thread_local! {
    static PAWN_CACHE: std::cell::RefCell<Vec<PawnEntry>> =
        std::cell::RefCell::new(vec![PawnEntry::default(); PAWN_CACHE_SIZE]);
}

fn pawn_structure(wp: u64, bp: u64) -> PawnEntry {
    let key = (wp.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ bp.wrapping_mul(0xC2B2_AE3D_27D4_EB4F))
        .rotate_left(29);
    let slot = (key as usize) & (PAWN_CACHE_SIZE - 1);
    let hit = PAWN_CACHE.with(|c| {
        let e = c.borrow()[slot];
        if e.valid && e.wp == wp && e.bp == bp {
            Some(e)
        } else {
            None
        }
    });
    if let Some(e) = hit {
        return e;
    }

    let mut e = PawnEntry { wp, bp, valid: true, ..Default::default() };
    for color in [Color::White, Color::Black] {
        let (own, enemy) = if color == Color::White { (wp, bp) } else { (bp, wp) };
        let c = idx(color);
        let own_att = pawn_attacks(own, color);
        let mut it = own;
        while it != 0 {
            let s = it.trailing_zeros();
            it &= it - 1;
            let b = 1u64 << s;
            let file_bb = FILE_A << (s % 8);
            let front = front_span(b, color);
            let front_adj = front_span(adjacent_files(b), color);

            if own & front != 0 {
                // doubled (the rear pawn is penalised)
                e.mg[c] -= 10;
                e.eg[c] -= 22;
            } else if enemy & (front | front_adj) == 0 {
                e.passed[c] |= b;
            }
            if own & adjacent_files(file_bb) == 0 {
                // isolated
                e.mg[c] -= 7;
                e.eg[c] -= 14;
            }
            if own_att & b != 0 {
                let r = rel_rank(color, s);
                e.mg[c] += 6 + r;
                e.eg[c] += 5 + r;
            } else if own & adjacent_files(b) != 0 {
                // phalanx
                let r = rel_rank(color, s);
                e.mg[c] += 3 + r;
                e.eg[c] += 2 + r;
            }
        }
    }
    PAWN_CACHE.with(|c| c.borrow_mut()[slot] = e);
    e
}

const PASSED_MG: [i32; 8] = [0, 2, 4, 8, 15, 30, 50, 0];
const PASSED_EG: [i32; 8] = [0, 4, 8, 14, 25, 45, 75, 0];

// ---------------------------------------------------------------------------
// King safety tables
// ---------------------------------------------------------------------------

/// Weight of each piece type attacking the king zone (Stockfish values).
const KING_ATTACK_WEIGHT: [i32; 6] = [0, 81, 52, 44, 10, 0];
/// Shelter penalty by relative rank of our closest pawn on a file (0 = no pawn).
const SHELTER_PEN: [i32; 8] = [36, 0, 8, 22, 30, 32, 34, 36];
/// Storm penalty by relative rank of the enemy pawn (from our side), unblocked.
const STORM_UNBLOCKED: [i32; 8] = [0, 0, 45, 30, 15, 5, 0, 0];
const STORM_BLOCKED: [i32; 8] = [0, 0, 20, 10, 5, 0, 0, 0];

// ---------------------------------------------------------------------------
// Main evaluation
// ---------------------------------------------------------------------------

/// Non-pawn material (SEE units) for a color.
pub fn non_pawn_material(board: &Board, color: Color) -> i32 {
    let c = board.colors(color);
    (board.pieces(Piece::Knight) & c).len() as i32 * SEE_VALUE[1]
        + (board.pieces(Piece::Bishop) & c).len() as i32 * SEE_VALUE[2]
        + (board.pieces(Piece::Rook) & c).len() as i32 * SEE_VALUE[3]
        + (board.pieces(Piece::Queen) & c).len() as i32 * SEE_VALUE[4]
}

/// Material balance in pawns-ish units from `color`'s point of view.
pub fn material_balance(board: &Board, color: Color) -> i32 {
    let mut s = 0;
    for p in [Piece::Pawn, Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen] {
        let v = SEE_VALUE[p as usize];
        s += v * (board.colored_pieces(color, p).len() as i32
            - board.colored_pieces(!color, p).len() as i32);
    }
    s
}

/// True when neither side can possibly mate (or it is a trivially dead draw).
pub fn is_insufficient_material(board: &Board) -> bool {
    if !(board.pieces(Piece::Pawn) | board.pieces(Piece::Rook) | board.pieces(Piece::Queen))
        .is_empty()
    {
        return false;
    }
    let minors = board.pieces(Piece::Knight) | board.pieces(Piece::Bishop);
    let w = (minors & board.colors(Color::White)).len();
    let b = (minors & board.colors(Color::Black)).len();
    w <= 1 && b <= 1
}

struct Ctx {
    occ: u64,
    pcs: [[u64; 6]; 2],
    colors: [u64; 2],
    ksq: [u32; 2],
    att: [[u64; 6]; 2],
    att_all: [u64; 2],
    att2: [u64; 2],
    zone: [u64; 2],
    // king attack bookkeeping, indexed by *attacker* colour
    ka_count: [i32; 2],
    ka_weight: [i32; 2],
    ka_adj: [i32; 2],
    mg: [i32; 2],
    eg: [i32; 2],
}

/// Evaluate from the side to move's point of view (centipawns): classical evaluation plus
/// the personality layer (and Skill Level noise).
pub fn evaluate(board: &Board) -> i32 {
    let (hce, pers) = compute(board, true);
    hce + pers + noise(board)
}

/// Whether the personality layer is active (an engine side is set and aggression > 100).
#[inline]
pub fn personality_active() -> bool {
    engine_color().is_some() && AGGRESSION.load(Ordering::Relaxed) > 100
}

/// Just the personality layer, from the side to move's point of view: the extra king danger
/// the engine inflicts beyond the objective amount, trade aversion, pieces aimed at the king.
/// Added on top of the NNUE evaluation.
pub fn personality(board: &Board) -> i32 {
    if !personality_active() {
        return 0;
    }
    compute(board, false).1
}

/// Skill Level noise (zero at full strength).
#[inline]
pub fn noise(board: &Board) -> i32 {
    let noise = NOISE.load(Ordering::Relaxed);
    if noise > 0 {
        // deterministic per position within one search, so the hash table stays consistent
        let mut h = board.hash() ^ NOISE_SEED.load(Ordering::Relaxed);
        h ^= h >> 33;
        h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
        h ^= h >> 33;
        (h % (2 * noise as u64 + 1)) as i32 - noise
    } else {
        0
    }
}

/// (classical evaluation, personality layer), both from the side to move's point of view.
/// With `full` false only the personality layer is computed (the first value is then junk).
fn compute(board: &Board, full: bool) -> (i32, i32) {
    if is_insufficient_material(board) {
        return (0, 0);
    }
    // personality terms, kept apart from the objective ones
    let mut pmg = [0i32; 2];
    let mut peg = [0i32; 2];

    let mut x = Ctx {
        occ: board.occupied().0,
        pcs: [[0; 6]; 2],
        colors: [board.colors(Color::White).0, board.colors(Color::Black).0],
        ksq: [board.king(Color::White) as u32, board.king(Color::Black) as u32],
        att: [[0; 6]; 2],
        att_all: [0; 2],
        att2: [0; 2],
        zone: [0; 2],
        ka_count: [0; 2],
        ka_weight: [0; 2],
        ka_adj: [0; 2],
        mg: [0; 2],
        eg: [0; 2],
    };
    for c in [Color::White, Color::Black] {
        for p in Piece::ALL {
            x.pcs[idx(c)][p as usize] = board.colored_pieces(c, p).0;
        }
    }

    let mut phase = 0;

    // --- pawns & kings: attack maps, material, PST -----------------------
    for color in [Color::White, Color::Black] {
        let c = idx(color);
        let pawns = x.pcs[c][0];
        let pa = pawn_attacks(pawns, color);
        x.att[c][0] = pa;
        x.att_all[c] = pa;
        x.att2[c] = pawn_double_attacks(pawns, color);

        let k = x.ksq[c];
        let kmoves = cozy_chess::get_king_moves(sq(k)).0;
        x.att[c][5] = kmoves;
        x.att2[c] |= x.att_all[c] & kmoves;
        x.att_all[c] |= kmoves;
        let ring = kmoves | (1u64 << k);
        x.zone[c] = ring | forward(ring, color);

        let mut it = pawns;
        while it != 0 {
            let s = it.trailing_zeros() as usize;
            it &= it - 1;
            let i = pst_index(color, s);
            x.mg[c] += MG_VALUE[0] + MG_PST[0][i];
            x.eg[c] += EG_VALUE[0] + EG_PST[0][i];
        }
        let i = pst_index(color, k as usize);
        x.mg[c] += MG_PST[5][i];
        x.eg[c] += EG_PST[5][i];
    }

    // pawns attacking the enemy king zone count as attackers (weight 0)
    for color in [Color::White, Color::Black] {
        let c = idx(color);
        let them = 1 - c;
        x.ka_count[c] = (x.zone[them] & x.att[c][0]).count_ones() as i32;
    }

    // --- pieces -----------------------------------------------------------
    let queens_all = x.pcs[0][4] | x.pcs[1][4];
    for color in [Color::White, Color::Black] {
        let c = idx(color);
        let them = 1 - c;
        let mob_area = !(x.pcs[c][0] | (1u64 << x.ksq[c]) | x.att[them][0]);
        let enemy_pawn_span = {
            // squares enemy pawns can ever attack (they move towards us)
            let ep = x.pcs[them][0];
            let filled = if color == Color::White { fill_south(ep) } else { fill_north(ep) };
            pawn_attacks(filled, !color)
        };
        let own_pawns = x.pcs[c][0];
        let enemy_pawns = x.pcs[them][0];

        for p in [Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen] {
            let pi = p as usize;
            let mut it = x.pcs[c][pi];
            while it != 0 {
                let s = it.trailing_zeros();
                it &= it - 1;
                let square = sq(s);
                let a = match p {
                    Piece::Knight => cozy_chess::get_knight_moves(square).0,
                    Piece::Bishop => cozy_chess::get_bishop_moves(square, BitBoard(x.occ ^ queens_all)).0,
                    Piece::Rook => cozy_chess::get_rook_moves(
                        square,
                        BitBoard(x.occ ^ queens_all ^ x.pcs[c][3]),
                    )
                    .0,
                    _ => {
                        cozy_chess::get_bishop_moves(square, BitBoard(x.occ)).0
                            | cozy_chess::get_rook_moves(square, BitBoard(x.occ)).0
                    }
                };
                x.att2[c] |= x.att_all[c] & a;
                x.att_all[c] |= a;
                x.att[c][pi] |= a;

                phase += PHASE_INC[pi];
                if full {
                    // material + PST
                    let i = pst_index(color, s as usize);
                    x.mg[c] += MG_VALUE[pi] + MG_PST[pi][i];
                    x.eg[c] += EG_VALUE[pi] + EG_PST[pi][i];

                    // mobility
                    let mob = (a & mob_area).count_ones() as i32;
                    let (mm, me) = match p {
                        Piece::Knight => ((mob - 4) * 4, (mob - 4) * 4),
                        Piece::Bishop => ((mob - 6) * 5, (mob - 6) * 5),
                        Piece::Rook => ((mob - 7) * 2, (mob - 7) * 4),
                        _ => (mob - 13, (mob - 13) * 2),
                    };
                    x.mg[c] += mm;
                    x.eg[c] += me;
                }

                // king attack
                if a & x.zone[them] != 0 {
                    x.ka_count[c] += 1;
                    x.ka_weight[c] += KING_ATTACK_WEIGHT[pi];
                    x.ka_adj[c] += (a & x.att[them][5]).count_ones() as i32;
                }

                let b = 1u64 << s;
                if !full {
                    continue;
                }
                match p {
                    Piece::Knight | Piece::Bishop => {
                        let rr = rel_rank(color, s);
                        if (3..=5).contains(&rr) && x.att[c][0] & b != 0 && enemy_pawn_span & b == 0 {
                            if p == Piece::Knight {
                                x.mg[c] += 25;
                                x.eg[c] += 12;
                            } else {
                                x.mg[c] += 12;
                                x.eg[c] += 5;
                            }
                        }
                    }
                    Piece::Rook => {
                        let file_bb = FILE_A << (s % 8);
                        if own_pawns & file_bb == 0 {
                            if enemy_pawns & file_bb == 0 {
                                x.mg[c] += 25;
                                x.eg[c] += 8;
                            } else {
                                x.mg[c] += 12;
                                x.eg[c] += 5;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // bishop pair
        if x.pcs[c][2].count_ones() >= 2 {
            x.mg[c] += 30;
            x.eg[c] += 55;
        }
    }

    // --- pawn structure ---------------------------------------------------
    let pe = if full { pawn_structure(x.pcs[0][0], x.pcs[1][0]) } else { PawnEntry::default() };
    for color in [Color::White, Color::Black] {
        if !full {
            break;
        }
        let c = idx(color);
        let them = 1 - c;
        x.mg[c] += pe.mg[c];
        x.eg[c] += pe.eg[c];

        let mut it = pe.passed[c];
        while it != 0 {
            let s = it.trailing_zeros();
            it &= it - 1;
            let r = rel_rank(color, s) as usize;
            let mut bm = PASSED_MG[r];
            let mut be = PASSED_EG[r];
            if r >= 3 {
                let stop = if color == Color::White { s + 8 } else { s - 8 };
                let w = r as i32 - 2;
                be += (dist(x.ksq[them], stop) * 5 - dist(x.ksq[c], stop) * 2) * w;
                let path = front_span(1u64 << s, color);
                if path & x.occ == 0 {
                    be += w * 6;
                    if path & x.att_all[them] == 0 {
                        be += w * 6;
                    }
                } else if (1u64 << stop) & x.occ != 0 {
                    bm /= 2;
                    be /= 2;
                }
            }
            x.mg[c] += bm;
            x.eg[c] += be;
        }
    }

    // --- threats ----------------------------------------------------------
    for color in [Color::White, Color::Black] {
        if !full {
            break;
        }
        let c = idx(color);
        let them = 1 - c;
        let their_nonpawn = x.colors[them] & !x.pcs[them][0] & !x.pcs[them][5];
        let n = (their_nonpawn & x.att[c][0]).count_ones() as i32;
        x.mg[c] += n * 60;
        x.eg[c] += n * 35;

        let minor_att = x.att[c][1] | x.att[c][2];
        let n = ((x.pcs[them][3] | x.pcs[them][4]) & minor_att).count_ones() as i32;
        x.mg[c] += n * 35;
        x.eg[c] += n * 45;
        let n = (x.pcs[them][4] & x.att[c][3]).count_ones() as i32;
        x.mg[c] += n * 35;
        x.eg[c] += n * 30;

        let hanging = x.colors[them] & !x.pcs[them][5] & x.att_all[c] & !x.att_all[them];
        let hp = (hanging & x.pcs[them][0]).count_ones() as i32;
        let hn = (hanging & !x.pcs[them][0]).count_ones() as i32;
        x.mg[c] += hn * 30 + hp * 6;
        x.eg[c] += hn * 18 + hp * 12;
    }

    // --- king safety --------------------------------------------------------
    let engine = engine_color();
    let aggression = AGGRESSION.load(Ordering::Relaxed).clamp(0, 400);
    for color in [Color::White, Color::Black] {
        if !full && Some(!color) != engine {
            continue; // only the engine's attack matters for the personality
        }
        let (mg_pen, eg_pen) = king_danger(&x, color);
        if Some(!color) == engine {
            // The engine is the attacker here: value its attack more.
            pmg[idx(color)] -= mg_pen * aggression / 100 - mg_pen;
        }
        x.mg[idx(color)] -= mg_pen;
        x.eg[idx(color)] -= eg_pen;
    }

    // --- personality extras -------------------------------------------------
    if let Some(e) = engine.filter(|_| aggression > 100) {
        let ec = idx(e);
        let k = aggression - 100; // 80 at the default setting
        // Prefer complex positions: every trade costs (zero at the start). Against a human,
        // pieces on the board mean chances, so this is strong, and stronger still when the
        // engine is behind in material: a losing swindler wants as many pieces on as possible,
        // all the way into the endgame.
        let mat = |c: usize| {
            x.pcs[c][0].count_ones() as i32
                + 3 * (x.pcs[c][1] | x.pcs[c][2]).count_ones() as i32
                + 5 * x.pcs[c][3].count_ones() as i32
                + 9 * x.pcs[c][4].count_ones() as i32
        };
        let behind = mat(ec) < mat(1 - ec) - 1;
        // Only *matched* losses count as trades: a bishop for a bishop is a trade, a bishop for
        // nothing is a sacrifice and costs nothing here (the sacrifice has its own risk already).
        let pieces = |c: usize| (x.pcs[c][1] | x.pcs[c][2] | x.pcs[c][3] | x.pcs[c][4]).count_ones() as i32;
        let lost = |c: usize| (7 - pieces(c)).max(0);
        let gone = -2 * lost(0).min(lost(1)); // two pieces leave the board per trade
        if behind {
            pmg[ec] += gone * 16 * k / 50;
            peg[ec] += gone * 6 * k / 50;
        } else {
            pmg[ec] += gone * 10 * k / 50;
        }
        if x.pcs[0][4] == 0 && x.pcs[1][4] == 0 {
            // queens traded off: much less fun (a queen *sacrifice* is not a trade)
            pmg[ec] -= if behind { 90 } else { 60 } * k / 50;
        }
        // Reward simply *pointing* pieces at the enemy king, even before the attack is
        // strong enough to register as king danger: it pulls pieces towards the king.
        let oc = 1 - ec;
        let zone_hits = (x.zone[oc] & x.att_all[ec]).count_ones() as i32;
        pmg[ec] += (x.ka_count[ec] * 9 + zone_hits * 4) * k / 80;
        // Rooks and queens on open or half-open files next to the enemy king.
        let kf = (x.ksq[oc] % 8) as i32;
        for f in (kf - 1).max(0)..=(kf + 1).min(7) {
            let file_bb = FILE_A << f;
            if x.pcs[ec][0] & file_bb == 0 {
                let heavy = ((x.pcs[ec][3] | x.pcs[ec][4]) & file_bb).count_ones() as i32;
                pmg[ec] += heavy * 18 * k / 80;
            }
        }
    }

    // --- tempo ----------------------------------------------------------
    let stm = idx(board.side_to_move());
    x.mg[stm] += 15;
    x.eg[stm] += 5;

    // --- combine ----------------------------------------------------------
    let phase = phase.min(24);
    let mg = x.mg[0] - x.mg[1];
    let mut eg = x.eg[0] - x.eg[1];

    // mop-up: help drive a bare king into the corner
    for strong in [Color::White, Color::Black] {
        let s = idx(strong);
        let w = 1 - s;
        let weak_bare = x.colors[w] == (1u64 << x.ksq[w]);
        if weak_bare && non_pawn_material(board, strong) >= SEE_VALUE[3] {
            let wk = x.ksq[w];
            let (f, r) = ((wk % 8) as i32, (wk / 8) as i32);
            let center = (3 - f).max(f - 4) + (3 - r).max(r - 4);
            let kd = (f - (x.ksq[s] % 8) as i32).abs() + (r - (x.ksq[s] / 8) as i32).abs();
            let bonus = 200 + center * 12 + (14 - kd) * 5;
            if strong == Color::White {
                eg += bonus;
            } else {
                eg -= bonus;
            }
        }
    }

    let sf = if full { scale_factor(board, &x, eg) } else { 64 };
    let score = (mg * phase + eg * sf / 64 * (24 - phase)) / 24;
    let pers = ((pmg[0] - pmg[1]) * phase + (peg[0] - peg[1]) * (24 - phase)) / 24;

    if board.side_to_move() == Color::White {
        (score, pers)
    } else {
        (-score, -pers)
    }
}

/// Returns (mg penalty, eg penalty) for the king of `us`.
fn king_danger(x: &Ctx, us: Color) -> (i32, i32) {
    let u = idx(us);
    let t = 1 - u;
    let them = !us;
    let k = x.ksq[u];
    let kf = (k % 8) as i32;
    let kr = rel_rank(us, k);

    // Pawn shelter and storm on the three files around the king.
    let center_f = kf.clamp(1, 6);
    let mut shelter = 0;
    for f in (center_f - 1)..=(center_f + 1) {
        let file_bb = FILE_A << f;
        let mut ours_r = 0;
        let mut theirs_r = 0;
        let mut it = x.pcs[u][0] & file_bb;
        while it != 0 {
            let s = it.trailing_zeros();
            it &= it - 1;
            let r = rel_rank(us, s);
            if r >= kr && (ours_r == 0 || r < ours_r) {
                ours_r = r;
            }
        }
        let mut it = x.pcs[t][0] & file_bb;
        while it != 0 {
            let s = it.trailing_zeros();
            it &= it - 1;
            let r = rel_rank(us, s);
            if r >= kr && (theirs_r == 0 || r < theirs_r) {
                theirs_r = r;
            }
        }
        let mut pen = SHELTER_PEN[ours_r as usize];
        if ours_r == 0 && theirs_r == 0 {
            pen += 12; // fully open file towards the king
        }
        if f == kf {
            pen += pen / 2;
        }
        let storm = if theirs_r == 0 {
            0
        } else if ours_r != 0 && theirs_r == ours_r + 1 {
            STORM_BLOCKED[theirs_r as usize]
        } else {
            STORM_UNBLOCKED[theirs_r as usize]
        };
        shelter += pen + storm;
    }
    let their_queen = x.pcs[t][4] != 0;
    if !their_queen {
        shelter /= 2;
    }

    let mut mg_pen = shelter / 2;
    let mut eg_pen = 0;

    let attackers = x.ka_count[t];
    if attackers > 1 - their_queen as i32 {
        let weak = x.att_all[t] & !x.att2[u] & (!x.att_all[u] | x.att[u][5] | x.att[u][4]);
        let safe = !x.colors[t] & (!x.att_all[u] | (weak & x.att2[t]));
        let occ_noq = BitBoard(x.occ ^ x.pcs[u][4]);
        let ks = sq(k);
        let rook_chk = cozy_chess::get_rook_moves(ks, occ_noq).0;
        let bishop_chk = cozy_chess::get_bishop_moves(ks, occ_noq).0;
        let queen_checks = (rook_chk | bishop_chk) & x.att[t][4] & safe & !x.att[u][4];
        let rook_checks = rook_chk & x.att[t][3] & safe;
        let bishop_checks = bishop_chk & x.att[t][2] & safe & !queen_checks;
        let knight_checks = cozy_chess::get_knight_moves(ks).0 & x.att[t][1] & safe;

        let mut danger = attackers * x.ka_weight[t]
            + 69 * x.ka_adj[t]
            + 185 * (x.zone[u] & weak).count_ones() as i32
            + if rook_checks != 0 { 1000 } else { 0 }
            + if queen_checks != 0 { 780 } else { 0 }
            + if bishop_checks != 0 { 630 } else { 0 }
            + if knight_checks != 0 { 780 } else { 0 }
            + shelter * 3 / 2
            + 37;
        if !their_queen {
            danger -= 873;
        }
        if x.att[u][1] & (x.att[u][5] | (1u64 << k)) != 0 {
            danger -= 100; // a knight defending the king is a great defender
        }
        let _ = them;
        if danger > 100 {
            mg_pen += (danger * danger / 5400).min(1600);
            eg_pen += danger / 16;
        }
    }
    (mg_pen, eg_pen)
}

/// Endgame scale factor in 1/64ths, applied to the eg score.
fn scale_factor(board: &Board, x: &Ctx, eg: i32) -> i32 {
    let strong = if eg > 0 { Color::White } else { Color::Black };
    let s = idx(strong);
    let w = 1 - s;
    let npm_s = non_pawn_material(board, strong);
    let npm_w = non_pawn_material(board, !strong);

    if x.pcs[s][0] == 0 {
        // no pawns for the stronger side
        if npm_s - npm_w <= 350 {
            return if npm_s <= SEE_VALUE[2] { 0 } else { 12 };
        }
        // two knights cannot force mate
        if x.pcs[s][1].count_ones() == 2 && npm_s == 2 * SEE_VALUE[1] && x.pcs[w][0] == 0 {
            return 0;
        }
    }
    // opposite coloured bishops, bishops + pawns only
    let only_bishops = |c: usize| {
        x.pcs[c][1] == 0 && x.pcs[c][3] == 0 && x.pcs[c][4] == 0 && x.pcs[c][2].count_ones() == 1
    };
    if only_bishops(0) && only_bishops(1) {
        const LIGHT: u64 = 0x55AA_55AA_55AA_55AA;
        let wl = x.pcs[0][2] & LIGHT != 0;
        let bl = x.pcs[1][2] & LIGHT != 0;
        if wl != bl {
            return 32;
        }
    }
    64
}

/// How many of `attacker`'s pieces bear on the enemy king zone. Used by the
/// search to recognise "attacking" moves and positions.
pub fn king_zone(board: &Board, defender: Color) -> u64 {
    let k = board.king(defender);
    let ring = cozy_chess::get_king_moves(k).0 | k.bitboard().0;
    ring | forward(ring, defender)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The lean personality-only path must give exactly the same personality score as the
    /// full evaluation.
    #[test]
    fn lean_personality_matches_full() {
        let mut seed = 0x9E3779B97F4A7C15u64;
        for side in [Color::White, Color::Black] {
            set_engine_color(Some(side));
            for _game in 0..100 {
                let mut b = Board::default();
                for _ in 0..120 {
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
                    b.play_unchecked(moves[(seed % moves.len() as u64) as usize]);
                    assert_eq!(compute(&b, true).1, compute(&b, false).1, "{}", b);
                }
            }
        }
        set_engine_color(None);
    }
}
