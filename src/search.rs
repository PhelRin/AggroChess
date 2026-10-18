//! AggroChess search: iterative deepening + aspiration windows + PVS with
//! standard modern pruning, shared transposition table and Lazy SMP.
//!
//! Sacrifice-friendly details:
//! * SEE-based pruning is depth-limited, so a sacrifice is only pruned when
//!   there isn't enough depth left to prove it works anyway.
//! * Quiet moves that bring a piece into the enemy king zone get a much more
//!   lenient SEE threshold and are reduced less by LMR.
//! * Checks are extended, and quiescence search looks at quiet checks.
//! * Draws are scored with contempt from the engine's point of view.

use crate::eval::{self, SEE_VALUE};
use cozy_chess::{BitBoard, Board, Color, Move, Piece, Square};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

pub const MAX_PLY: usize = 128;
pub const INF: i32 = 32000;
pub const MATE: i32 = 30000;
pub const MATE_BOUND: i32 = MATE - 1000;

pub static TT_SIZE_MB: AtomicUsize = AtomicUsize::new(128);
pub static NUM_THREADS: AtomicUsize = AtomicUsize::new(1);
/// Draw score penalty (centipawns) from the engine's point of view.
pub static CONTEMPT: AtomicI32 = AtomicI32::new(50);
/// 0..=20. 20 = full strength. Lower levels limit nodes and add evaluation noise.
/// Root bonus (centipawns) for moves that give up material. 0 = off.
pub static SACRIFICE_BONUS: AtomicI32 = AtomicI32::new(220);
/// A sacrifice only gets the bonus if its line scores at least this much.
const SAC_FLOOR: i32 = -260;
pub static SKILL: AtomicI32 = AtomicI32::new(20);

// ---------------------------------------------------------------------------
// Transposition table
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bound {
    None = 0,
    Upper = 1,
    Lower = 2,
    Exact = 3,
}

#[derive(Clone, Copy, Debug)]
pub struct TTData {
    pub mv: Option<Move>,
    pub score: i32,
    pub eval: i32,
    pub depth: i32,
    pub bound: Bound,
}

struct TTEntry {
    key: AtomicU64,
    data: AtomicU64,
}

pub struct TranspositionTable {
    entries: Vec<TTEntry>,
    buckets: usize,
    age: AtomicU8,
    mb: usize,
}

const EVAL_NONE: i32 = -32001;

fn pack_move(mv: Option<Move>) -> u64 {
    match mv {
        None => 0,
        Some(m) => {
            let promo = match m.promotion {
                None => 0,
                Some(Piece::Knight) => 1,
                Some(Piece::Bishop) => 2,
                Some(Piece::Rook) => 3,
                _ => 4,
            };
            (m.from as u64) | ((m.to as u64) << 6) | (promo << 12)
        }
    }
}

fn unpack_move(v: u64) -> Option<Move> {
    let v = v & 0xFFFF;
    if v == 0 {
        return None;
    }
    let from = Square::index((v & 63) as usize);
    let to = Square::index(((v >> 6) & 63) as usize);
    let promotion = match (v >> 12) & 7 {
        1 => Some(Piece::Knight),
        2 => Some(Piece::Bishop),
        3 => Some(Piece::Rook),
        4 => Some(Piece::Queen),
        _ => None,
    };
    Some(Move { from, to, promotion })
}

impl TranspositionTable {
    pub fn new(mb: usize) -> Self {
        let bytes = mb.max(1) * 1024 * 1024;
        let entries = (bytes / std::mem::size_of::<TTEntry>()).max(2);
        let buckets = (entries / 2).max(1);
        let mut v = Vec::with_capacity(buckets * 2);
        for _ in 0..buckets * 2 {
            v.push(TTEntry { key: AtomicU64::new(0), data: AtomicU64::new(0) });
        }
        Self { entries: v, buckets, age: AtomicU8::new(0), mb }
    }

    pub fn mb(&self) -> usize {
        self.mb
    }

    pub fn clear(&self) {
        for e in &self.entries {
            e.key.store(0, Ordering::Relaxed);
            e.data.store(0, Ordering::Relaxed);
        }
        self.age.store(0, Ordering::Relaxed);
    }

    pub fn new_search(&self) {
        let a = self.age.load(Ordering::Relaxed);
        self.age.store((a + 1) & 63, Ordering::Relaxed);
    }

    #[inline]
    fn bucket(&self, hash: u64) -> usize {
        (((hash as u128) * (self.buckets as u128)) >> 64) as usize * 2
    }

    fn decode(data: u64) -> TTData {
        TTData {
            mv: unpack_move(data),
            score: ((data >> 16) & 0xFFFF) as u16 as i16 as i32,
            eval: ((data >> 32) & 0xFFFF) as u16 as i16 as i32,
            depth: ((data >> 48) & 0xFF) as u8 as i8 as i32,
            bound: match (data >> 56) & 3 {
                1 => Bound::Upper,
                2 => Bound::Lower,
                3 => Bound::Exact,
                _ => Bound::None,
            },
        }
    }

    pub fn probe(&self, hash: u64) -> Option<TTData> {
        let b = self.bucket(hash);
        for i in 0..2 {
            let e = &self.entries[b + i];
            let data = e.data.load(Ordering::Relaxed);
            let key = e.key.load(Ordering::Relaxed);
            if data != 0 && key ^ data == hash {
                return Some(Self::decode(data));
            }
        }
        None
    }

    pub fn store(&self, hash: u64, mv: Option<Move>, score: i32, eval: i32, depth: i32, bound: Bound) {
        let b = self.bucket(hash);
        let age = self.age.load(Ordering::Relaxed) as u64;
        let mut target = b;
        let mut worst = i32::MAX;
        for i in 0..2 {
            let e = &self.entries[b + i];
            let data = e.data.load(Ordering::Relaxed);
            let key = e.key.load(Ordering::Relaxed);
            if data == 0 {
                target = b + i;
                break;
            }
            if key ^ data == hash {
                let old = Self::decode(data);
                // keep the deeper entry unless the new one is exact / newer
                if bound != Bound::Exact && depth + 3 < old.depth && (data >> 58) == age {
                    return;
                }
                let mv = mv.or(old.mv);
                self.write(b + i, hash, mv, score, eval, depth, bound, age);
                return;
            }
            let old_age = (data >> 58) & 63;
            let age_diff = ((age + 64 - old_age) & 63) as i32;
            let old_depth = ((data >> 48) & 0xFF) as u8 as i8 as i32;
            let value = old_depth - 8 * age_diff;
            if value < worst {
                worst = value;
                target = b + i;
            }
        }
        self.write(target, hash, mv, score, eval, depth, bound, age);
    }

    #[allow(clippy::too_many_arguments)]
    fn write(&self, i: usize, hash: u64, mv: Option<Move>, score: i32, eval: i32, depth: i32, bound: Bound, age: u64) {
        let data = pack_move(mv)
            | (((score.clamp(-32000, 32000) as i16) as u16 as u64) << 16)
            | (((eval.clamp(-32001, 32000) as i16) as u16 as u64) << 32)
            | (((depth.clamp(-1, 127) as i8) as u8 as u64) << 48)
            | ((bound as u64) << 56)
            | (age << 58);
        // data is never zero because the bound or depth bits are set for real entries;
        // force a bit so an empty-move upper bound at depth 0 is still distinguishable.
        let data = if data == 0 { 1 << 63 } else { data };
        let e = &self.entries[i];
        e.key.store(hash ^ data, Ordering::Relaxed);
        e.data.store(data, Ordering::Relaxed);
    }

    pub fn hashfull(&self) -> usize {
        let n = self.entries.len().min(2000);
        let age = self.age.load(Ordering::Relaxed) as u64;
        let mut c = 0;
        for e in &self.entries[..n] {
            let d = e.data.load(Ordering::Relaxed);
            if d != 0 && (d >> 58) == age {
                c += 1;
            }
        }
        c * 1000 / n
    }
}

static GLOBAL_TT: RwLock<Option<Arc<TranspositionTable>>> = RwLock::new(None);

pub fn get_global_tt(mb: usize) -> Arc<TranspositionTable> {
    if let Ok(lock) = GLOBAL_TT.read() {
        if let Some(tt) = lock.as_ref() {
            if tt.mb() == mb {
                return Arc::clone(tt);
            }
        }
    }
    let mut lock = GLOBAL_TT.write().unwrap();
    if let Some(tt) = lock.as_ref() {
        if tt.mb() == mb {
            return Arc::clone(tt);
        }
    }
    let tt = Arc::new(TranspositionTable::new(mb));
    *lock = Some(Arc::clone(&tt));
    tt
}

pub fn clear_global_tt() {
    if let Ok(lock) = GLOBAL_TT.read() {
        if let Some(tt) = lock.as_ref() {
            tt.clear();
        }
    }
}

#[inline]
fn score_to_tt(score: i32, ply: usize) -> i32 {
    if score >= MATE_BOUND {
        score + ply as i32
    } else if score <= -MATE_BOUND {
        score - ply as i32
    } else {
        score
    }
}

#[inline]
fn score_from_tt(score: i32, ply: usize) -> i32 {
    if score >= MATE_BOUND {
        score - ply as i32
    } else if score <= -MATE_BOUND {
        score + ply as i32
    } else {
        score
    }
}

// ---------------------------------------------------------------------------
// Move helpers
// ---------------------------------------------------------------------------

#[inline]
pub fn is_capture(board: &Board, mv: Move) -> bool {
    let them = board.colors(!board.side_to_move());
    if them.has(mv.to) {
        return true;
    }
    // en passant
    board.piece_on(mv.from) == Some(Piece::Pawn) && mv.from.file() != mv.to.file()
}

#[inline]
fn captured_piece(board: &Board, mv: Move) -> Piece {
    if board.colors(!board.side_to_move()).has(mv.to) {
        board.piece_on(mv.to).unwrap_or(Piece::Pawn)
    } else {
        Piece::Pawn // en passant
    }
}

#[inline]
fn is_castle(board: &Board, mv: Move) -> bool {
    board.colors(board.side_to_move()).has(mv.to)
}

fn attackers_to(board: &Board, sq: Square, occ: u64) -> u64 {
    let b = sq.bitboard().0;
    let pawns = board.pieces(Piece::Pawn).0;
    let w = board.colors(Color::White).0;
    let bl = board.colors(Color::Black).0;
    let bq = (board.pieces(Piece::Bishop) | board.pieces(Piece::Queen)).0;
    let rq = (board.pieces(Piece::Rook) | board.pieces(Piece::Queen)).0;
    (eval::pawn_attacks(b, Color::Black) & pawns & w)
        | (eval::pawn_attacks(b, Color::White) & pawns & bl)
        | (cozy_chess::get_knight_moves(sq).0 & board.pieces(Piece::Knight).0)
        | (cozy_chess::get_king_moves(sq).0 & board.pieces(Piece::King).0)
        | (cozy_chess::get_bishop_moves(sq, BitBoard(occ)).0 & bq)
        | (cozy_chess::get_rook_moves(sq, BitBoard(occ)).0 & rq)
}

/// Static exchange evaluation: is the material outcome of `mv` >= `threshold`?
pub fn see_ge(board: &Board, mv: Move, threshold: i32) -> bool {
    if is_castle(board, mv) {
        return 0 >= threshold;
    }
    let from = mv.from;
    let to = mv.to;
    let moved = match board.piece_on(from) {
        Some(p) => p,
        None => return false,
    };
    let mut gain = if is_capture(board, mv) { SEE_VALUE[captured_piece(board, mv) as usize] } else { 0 };
    let mut moved_val = SEE_VALUE[moved as usize];
    if let Some(p) = mv.promotion {
        gain += SEE_VALUE[p as usize] - SEE_VALUE[0];
        moved_val = SEE_VALUE[p as usize];
    }
    let mut swap = gain - threshold;
    if swap < 0 {
        return false;
    }
    swap = moved_val - swap;
    if swap <= 0 {
        return true;
    }

    let mut occ = board.occupied().0 ^ from.bitboard().0 ^ to.bitboard().0;
    if moved == Piece::Pawn && from.file() != to.file() && board.piece_on(to).is_none() {
        // en passant: remove the captured pawn
        let cap = Square::new(to.file(), from.rank());
        occ ^= cap.bitboard().0;
    }
    let bq = (board.pieces(Piece::Bishop) | board.pieces(Piece::Queen)).0;
    let rq = (board.pieces(Piece::Rook) | board.pieces(Piece::Queen)).0;
    let mut attackers = attackers_to(board, to, occ);
    let mut stm = board.side_to_move();
    let mut res = 1;

    loop {
        stm = !stm;
        attackers &= occ;
        let my = attackers & board.colors(stm).0;
        if my == 0 {
            break;
        }
        res ^= 1;
        let mut found = None;
        for p in [Piece::Pawn, Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen, Piece::King] {
            let bb = my & board.pieces(p).0;
            if bb != 0 {
                found = Some((p, bb & bb.wrapping_neg()));
                break;
            }
        }
        let (p, lsb) = found.unwrap();
        if p == Piece::King {
            return if attackers & board.colors(!stm).0 != 0 { res ^ 1 == 1 } else { res == 1 };
        }
        swap = SEE_VALUE[p as usize] - swap;
        if swap < res {
            break;
        }
        occ ^= lsb;
        if matches!(p, Piece::Pawn | Piece::Bishop | Piece::Queen) {
            attackers |= cozy_chess::get_bishop_moves(to, BitBoard(occ)).0 & bq;
        }
        if matches!(p, Piece::Rook | Piece::Queen) {
            attackers |= cozy_chess::get_rook_moves(to, BitBoard(occ)).0 & rq;
        }
    }
    res == 1
}

/// Old-style SEE value (kept for tests / external callers).
pub fn see(board: &Board, mv: Move) -> i32 {
    // binary search on the threshold
    let (mut lo, mut hi): (i32, i32) = (-2000, 2000);
    while lo < hi {
        let mid = (lo + hi + 1).div_euclid(2);
        if see_ge(board, mv, mid) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

/// Squares attacked by a piece of type `p` standing on `s`.
#[inline]
fn piece_attacks(p: Piece, color: Color, s: Square, occ: u64) -> u64 {
    match p {
        Piece::Pawn => eval::pawn_attacks(s.bitboard().0, color),
        Piece::Knight => cozy_chess::get_knight_moves(s).0,
        Piece::Bishop => cozy_chess::get_bishop_moves(s, BitBoard(occ)).0,
        Piece::Rook => cozy_chess::get_rook_moves(s, BitBoard(occ)).0,
        Piece::Queen => {
            cozy_chess::get_bishop_moves(s, BitBoard(occ)).0 | cozy_chess::get_rook_moves(s, BitBoard(occ)).0
        }
        Piece::King => cozy_chess::get_king_moves(s).0,
    }
}

struct MoveList {
    moves: [Move; 256],
    scores: [i32; 256],
    len: usize,
}

impl MoveList {
    fn new() -> Self {
        Self {
            moves: [Move { from: Square::A1, to: Square::A1, promotion: None }; 256],
            scores: [0; 256],
            len: 0,
        }
    }
    #[inline]
    fn push(&mut self, m: Move) {
        if self.len < 256 {
            self.moves[self.len] = m;
            self.len += 1;
        }
    }
    /// Selection sort step: bring the best remaining move to index `i`.
    #[inline]
    fn pick(&mut self, i: usize) -> Move {
        let mut best = i;
        for j in (i + 1)..self.len {
            if self.scores[j] > self.scores[best] {
                best = j;
            }
        }
        self.moves.swap(i, best);
        self.scores.swap(i, best);
        self.moves[i]
    }
}

// ---------------------------------------------------------------------------
// Searcher
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct StackEntry {
    eval: i32,
    mv: Option<Move>,
    /// piece*64+to index of the move made at this ply (for continuation history), or usize::MAX
    cont: usize,
    excluded: Option<Move>,
}

const NO_CONT: usize = usize::MAX;
const HIST_MAX: i32 = 16384;

pub struct Searcher {
    pub tt: Arc<TranspositionTable>,
    pub stop: Arc<AtomicBool>,
    pub shared_nodes: Arc<AtomicU64>,
    pub nodes: u64,
    main: bool,
    start: Instant,
    hard_limit: Option<Duration>,
    node_limit: Option<u64>,

    history: Box<[[[i32; 64]; 64]; 2]>,
    cont_hist: Vec<i16>,
    capt_hist: Box<[[[i32; 6]; 64]; 12]>,
    killers: [[Option<Move>; 2]; MAX_PLY + 2],
    counter: Box<[[Option<Move>; 64]; 12]>,
    stack: [StackEntry; MAX_PLY + 4],
    pv: Box<[[Option<Move>; MAX_PLY + 1]; MAX_PLY + 1]>,
    pv_len: [usize; MAX_PLY + 1],
    pub hashes: Vec<u64>,
    root_color: Color,
    root_depth: i32,
    pub seldepth: usize,
    lmr: Box<[[i32; 64]; 64]>,
}

#[inline]
fn pc12(color: Color, p: Piece) -> usize {
    color as usize * 6 + p as usize
}

impl Searcher {
    pub fn new(tt: Arc<TranspositionTable>, stop: Arc<AtomicBool>, shared_nodes: Arc<AtomicU64>, main: bool) -> Self {
        let mut lmr = Box::new([[0i32; 64]; 64]);
        for d in 1..64 {
            for m in 1..64 {
                lmr[d][m] = (0.75 + (d as f64).ln() * (m as f64).ln() / 2.25) as i32;
            }
        }
        Self {
            tt,
            stop,
            shared_nodes,
            nodes: 0,
            main,
            start: Instant::now(),
            hard_limit: None,
            node_limit: None,
            history: Box::new([[[0; 64]; 64]; 2]),
            cont_hist: vec![0; 768 * 768],
            capt_hist: Box::new([[[0; 6]; 64]; 12]),
            killers: [[None; 2]; MAX_PLY + 2],
            counter: Box::new([[None; 64]; 12]),
            stack: [StackEntry { eval: EVAL_NONE, mv: None, cont: NO_CONT, excluded: None }; MAX_PLY + 4],
            pv: Box::new([[None; MAX_PLY + 1]; MAX_PLY + 1]),
            pv_len: [0; MAX_PLY + 1],
            hashes: Vec::with_capacity(1024),
            root_color: Color::White,
            root_depth: 0,
            seldepth: 0,
            lmr,
        }
    }

    #[inline]
    fn check_time(&mut self) {
        if self.nodes & 1023 == 0 {
            self.shared_nodes.fetch_add(1024, Ordering::Relaxed);
            if self.main {
                if let Some(h) = self.hard_limit {
                    if self.start.elapsed() >= h {
                        self.stop.store(true, Ordering::Relaxed);
                    }
                }
                if let Some(n) = self.node_limit {
                    if self.shared_nodes.load(Ordering::Relaxed) >= n {
                        self.stop.store(true, Ordering::Relaxed);
                    }
                }
            }
        }
    }

    #[inline]
    fn draw_score(&self, board: &Board) -> i32 {
        let c = CONTEMPT.load(Ordering::Relaxed);
        if board.side_to_move() == self.root_color {
            -c
        } else {
            c
        }
    }

    fn is_repetition(&self, board: &Board) -> bool {
        let h = board.hash();
        let n = self.hashes.len();
        let limit = (board.halfmove_clock() as usize).min(n);
        let mut i = 2;
        while i <= limit {
            if self.hashes[n - i] == h {
                return true;
            }
            i += 2;
        }
        false
    }

    #[inline]
    fn cont_index(prev: usize, cur: usize) -> usize {
        prev * 768 + cur
    }

    fn quiet_score(&self, board: &Board, m: Move, ply: usize) -> i32 {
        let stm = board.side_to_move();
        let p = board.piece_on(m.from).unwrap();
        let cur = pc12(stm, p) * 64 + m.to as usize;
        let mut s = self.history[stm as usize][m.from as usize][m.to as usize];
        if ply >= 1 && self.stack[ply - 1].cont != NO_CONT {
            s += self.cont_hist[Self::cont_index(self.stack[ply - 1].cont, cur)] as i32;
        }
        if ply >= 2 && self.stack[ply - 2].cont != NO_CONT {
            s += self.cont_hist[Self::cont_index(self.stack[ply - 2].cont, cur)] as i32;
        }
        s
    }

    fn score_moves(&self, board: &Board, list: &mut MoveList, tt_move: Option<Move>, ply: usize) {
        let stm = board.side_to_move();
        let them = !stm;
        let ek = board.king(them);
        let occ = board.occupied().0;
        let knight_chk = cozy_chess::get_knight_moves(ek).0;
        let bishop_chk = cozy_chess::get_bishop_moves(ek, BitBoard(occ)).0;
        let rook_chk = cozy_chess::get_rook_moves(ek, BitBoard(occ)).0;
        let pawn_chk = eval::pawn_attacks(ek.bitboard().0, them);
        let ezone = eval::king_zone(board, them);
        let counter = if ply >= 1 {
            match (self.stack[ply - 1].mv, self.stack[ply - 1].cont) {
                (Some(_), c) if c != NO_CONT => self.counter[c / 64][c % 64],
                _ => None,
            }
        } else {
            None
        };

        for i in 0..list.len {
            let m = list.moves[i];
            let s = if Some(m) == tt_move {
                20_000_000
            } else if let Some(promo) = m.promotion {
                if promo == Piece::Queen {
                    5_000_000
                } else {
                    -5_000_000
                }
            } else if is_capture(board, m) {
                let victim = captured_piece(board, m);
                let attacker = board.piece_on(m.from).unwrap();
                let base = SEE_VALUE[victim as usize] * 10 - SEE_VALUE[attacker as usize] / 10;
                let ch = self.capt_hist[pc12(stm, attacker)][m.to as usize][victim as usize];
                if see_ge(board, m, -50) {
                    4_000_000 + base + ch / 16
                } else {
                    -4_000_000 + base + ch / 16
                }
            } else if Some(m) == self.killers[ply][0] {
                3_000_000
            } else if Some(m) == self.killers[ply][1] {
                2_900_000
            } else if Some(m) == counter {
                2_800_000
            } else {
                let p = board.piece_on(m.from).unwrap();
                let mut s = self.quiet_score(board, m, ply);
                let to = m.to.bitboard().0;
                let checks = match p {
                    Piece::Pawn => pawn_chk,
                    Piece::Knight => knight_chk,
                    Piece::Bishop => bishop_chk,
                    Piece::Rook => rook_chk,
                    Piece::Queen => bishop_chk | rook_chk,
                    Piece::King => 0,
                };
                if to & checks != 0 {
                    s += 6000;
                }
                if p != Piece::King && p != Piece::Pawn && piece_attacks(p, stm, m.to, occ) & ezone != 0 {
                    s += 2500;
                }
                s
            };
            list.scores[i] = s;
        }
    }

    fn update_quiet_stats(&mut self, board: &Board, best: Move, quiets: &[Move], depth: i32, ply: usize) {
        let stm = board.side_to_move();
        let bonus = (16 * depth * depth + 32 * depth).min(1600);
        let apply = |v: &mut i32, b: i32| {
            *v += b - *v * b.abs() / HIST_MAX;
        };
        let prev1 = if ply >= 1 { self.stack[ply - 1].cont } else { NO_CONT };
        let prev2 = if ply >= 2 { self.stack[ply - 2].cont } else { NO_CONT };
        for &m in quiets {
            let b = if m == best { bonus } else { -bonus };
            apply(&mut self.history[stm as usize][m.from as usize][m.to as usize], b);
            let p = board.piece_on(m.from).unwrap();
            let cur = pc12(stm, p) * 64 + m.to as usize;
            for prev in [prev1, prev2] {
                if prev != NO_CONT {
                    let i = Self::cont_index(prev, cur);
                    let mut v = self.cont_hist[i] as i32;
                    apply(&mut v, b);
                    self.cont_hist[i] = v.clamp(-HIST_MAX, HIST_MAX) as i16;
                }
            }
        }
        if self.killers[ply][0] != Some(best) {
            self.killers[ply][1] = self.killers[ply][0];
            self.killers[ply][0] = Some(best);
        }
        if prev1 != NO_CONT {
            self.counter[prev1 / 64][prev1 % 64] = Some(best);
        }
    }

    fn update_capture_stats(&mut self, board: &Board, best: Option<Move>, captures: &[Move], depth: i32) {
        let stm = board.side_to_move();
        let bonus = (16 * depth * depth + 32 * depth).min(1600);
        for &m in captures {
            let b = if Some(m) == best { bonus } else { -bonus };
            let attacker = board.piece_on(m.from).unwrap();
            let victim = captured_piece(board, m);
            let v = &mut self.capt_hist[pc12(stm, attacker)][m.to as usize][victim as usize];
            *v += b - *v * b.abs() / HIST_MAX;
        }
    }

    #[inline]
    fn update_pv(&mut self, ply: usize, m: Move) {
        self.pv[ply][ply] = Some(m);
        let next_len = self.pv_len[ply + 1].max(ply + 1);
        for i in (ply + 1)..next_len.min(MAX_PLY) {
            self.pv[ply][i] = self.pv[ply + 1][i];
        }
        self.pv_len[ply] = next_len.min(MAX_PLY);
    }

    pub fn search(&mut self, board: &Board, mut depth: i32, mut alpha: i32, mut beta: i32, ply: usize, cut_node: bool) -> i32 {
        let pv_node = beta - alpha > 1;
        let root = ply == 0;
        self.pv_len[ply] = ply;

        let in_check = !board.checkers().is_empty();
        if in_check && ply < MAX_PLY - 8 {
            depth += 1;
        }
        if depth <= 0 {
            return self.qsearch(board, alpha, beta, ply, 0);
        }

        self.nodes += 1;
        self.check_time();
        if ply > self.seldepth {
            self.seldepth = ply;
        }

        if !root {
            if self.stop.load(Ordering::Relaxed) {
                return 0;
            }
            if board.halfmove_clock() >= 100 || self.is_repetition(board) || eval::is_insufficient_material(board) {
                return self.draw_score(board);
            }
            if ply >= MAX_PLY - 2 {
                return if in_check { 0 } else { eval::evaluate(board) };
            }
            // mate distance pruning
            alpha = alpha.max(-MATE + ply as i32);
            beta = beta.min(MATE - ply as i32 - 1);
            if alpha >= beta {
                return alpha;
            }
        }

        let excluded = self.stack[ply].excluded;
        let hash = board.hash();
        let tt = if excluded.is_none() { self.tt.probe(hash) } else { None };
        let tt_move = tt.and_then(|e| e.mv).filter(|&m| board.is_legal(m));
        let tt_score = tt.map(|e| score_from_tt(e.score, ply)).unwrap_or(-INF);

        if let Some(e) = tt {
            if !pv_node && e.depth >= depth && board.halfmove_clock() < 90 {
                let cut = match e.bound {
                    Bound::Exact => true,
                    Bound::Lower => tt_score >= beta,
                    Bound::Upper => tt_score <= alpha,
                    Bound::None => false,
                };
                if cut {
                    return tt_score;
                }
            }
        }

        // static evaluation
        let static_eval;
        let mut eval_est;
        if in_check {
            static_eval = EVAL_NONE;
            eval_est = -INF;
        } else {
            static_eval = match tt {
                Some(e) if e.eval != EVAL_NONE => e.eval,
                _ => eval::evaluate(board),
            };
            eval_est = static_eval;
            if let Some(e) = tt {
                let use_tt = match e.bound {
                    Bound::Exact => true,
                    Bound::Lower => tt_score > eval_est,
                    Bound::Upper => tt_score < eval_est,
                    Bound::None => false,
                };
                if use_tt && tt_score.abs() < MATE_BOUND {
                    eval_est = tt_score;
                }
            } else if excluded.is_none() {
                self.tt.store(hash, None, 0, static_eval, -1, Bound::None);
            }
        }
        self.stack[ply].eval = static_eval;
        let improving = !in_check
            && ply >= 2
            && self.stack[ply - 2].eval != EVAL_NONE
            && static_eval > self.stack[ply - 2].eval;

        self.killers[ply + 1] = [None; 2];
        self.stack[ply + 1].excluded = None;

        let stm = board.side_to_move();
        let has_pieces = eval::non_pawn_material(board, stm) > 0;

        if !pv_node && !in_check && excluded.is_none() {
            // reverse futility pruning
            if depth <= 8 && eval_est.abs() < MATE_BOUND && eval_est - (80 * depth - 60 * improving as i32) >= beta {
                return eval_est;
            }
            // razoring
            if depth <= 3 && has_pieces && eval_est + 200 + 150 * depth <= alpha {
                let q = self.qsearch(board, alpha, alpha + 1, ply, 0);
                if q <= alpha {
                    return q;
                }
            }
            // null move pruning
            if depth >= 3
                && eval_est >= beta
                && static_eval >= beta - 20 * depth + 100
                && has_pieces
                && ply >= 1
                && self.stack[ply - 1].mv.is_some()
                && beta.abs() < MATE_BOUND
            {
                if let Some(nb) = board.null_move() {
                    let r = 3 + depth / 3 + ((eval_est - beta) / 200).min(3);
                    self.stack[ply].mv = None;
                    self.stack[ply].cont = NO_CONT;
                    self.hashes.push(hash);
                    let s = -self.search(&nb, depth - r, -beta, -beta + 1, ply + 1, !cut_node);
                    self.hashes.pop();
                    if self.stop.load(Ordering::Relaxed) {
                        return 0;
                    }
                    if s >= beta {
                        return if s >= MATE_BOUND { beta } else { s };
                    }
                }
            }
        }

        // internal iterative reduction
        if depth >= 4 && tt_move.is_none() && excluded.is_none() && (pv_node || cut_node) {
            depth -= 1;
        }

        let mut list = MoveList::new();
        board.generate_moves(|mvs| {
            for m in mvs {
                list.push(m);
            }
            false
        });
        if list.len == 0 {
            return if in_check { -MATE + ply as i32 } else { self.draw_score(board) };
        }
        self.score_moves(board, &mut list, tt_move, ply);

        let them = !stm;
        let ezone = eval::king_zone(board, them);
        let occ = board.occupied().0;
        let lmp_limit = (3 + depth * depth) / if improving { 1 } else { 2 };
        // Sacrifice appetite: full at the root, 60% on the engine's next move, so it also
        // steers towards positions where a sacrifice will be available.
        let sac_bonus = if root {
            SACRIFICE_BONUS.load(Ordering::Relaxed)
        } else if ply == 2 && stm == self.root_color {
            SACRIFICE_BONUS.load(Ordering::Relaxed)
        } else if ply == 4 && stm == self.root_color {
            SACRIFICE_BONUS.load(Ordering::Relaxed) / 2
        } else {
            0
        };

        let mut best_score = -INF;
        let mut best_move: Option<Move> = None;
        let mut move_count = 0usize;
        let mut quiets: [Move; 64] = [Move { from: Square::A1, to: Square::A1, promotion: None }; 64];
        let mut nquiets = 0usize;
        let mut captures: [Move; 32] = [Move { from: Square::A1, to: Square::A1, promotion: None }; 32];
        let mut ncaptures = 0usize;
        let mut skip_quiets = false;
        let orig_alpha = alpha;

        for i in 0..list.len {
            let m = list.pick(i);
            if Some(m) == excluded {
                continue;
            }
            let capture = is_capture(board, m);
            let quiet = !capture && m.promotion.is_none();
            if quiet && skip_quiets {
                continue;
            }
            let piece = board.piece_on(m.from).unwrap();

            let mut child = board.clone();
            child.play_unchecked(m);
            let gives_check = !child.checkers().is_empty();
            let attacks_zone = quiet
                && piece != Piece::King
                && piece_attacks(piece, stm, m.to, child.occupied().0) & ezone != 0
                && piece_attacks(piece, stm, m.from, occ) & ezone == 0;

            move_count += 1;
            let hist = if quiet { self.quiet_score(board, m, ply) } else { 0 };

            // ---- pruning at shallow depth ----
            if !root && best_score > -MATE_BOUND && has_pieces {
                let lmr_d = (depth - 1 - self.lmr[depth.min(63) as usize][move_count.min(63)]).max(0);
                if quiet && !gives_check {
                    if move_count as i32 > lmp_limit && !attacks_zone {
                        skip_quiets = true;
                        continue;
                    }
                    if !in_check && lmr_d <= 7 && eval_est + 100 + 90 * lmr_d + hist / 128 <= alpha {
                        continue;
                    }
                    let threshold = if attacks_zone { -60 * depth * depth } else { -25 * lmr_d * lmr_d };
                    if !see_ge(board, m, threshold) {
                        continue;
                    }
                } else if depth <= 8 {
                    let threshold = if gives_check {
                        if stm == self.root_color { -200 * depth } else { -120 * depth }
                    } else {
                        -90 * depth
                    };
                    if !see_ge(board, m, threshold) {
                        continue;
                    }
                }
            }

            // ---- extensions ----
            let mut ext = 0;
            if !root
                && depth >= 8
                && Some(m) == tt_move
                && excluded.is_none()
                && (ply as i32) < 2 * self.root_depth
            {
                if let Some(e) = tt {
                    if e.depth >= depth - 3 && matches!(e.bound, Bound::Lower | Bound::Exact) && tt_score.abs() < MATE_BOUND {
                        let s_beta = tt_score - 2 * depth;
                        self.stack[ply].excluded = Some(m);
                        let s = self.search(board, (depth - 1) / 2, s_beta - 1, s_beta, ply, cut_node);
                        self.stack[ply].excluded = None;
                        if self.stop.load(Ordering::Relaxed) {
                            return 0;
                        }
                        if s < s_beta {
                            ext = 1;
                        } else if s_beta >= beta {
                            return s_beta;
                        } else if tt_score >= beta {
                            ext = -1;
                        }
                    }
                }
            }

            // sacrifice extension: a material-losing move aimed at the enemy king is
            // verified one ply deeper instead of being dismissed (checks are already extended)
            if ext == 0
                && !gives_check
                && depth >= 4
                && (ply as i32) < self.root_depth
                && ((quiet && attacks_zone) || (capture && ezone & m.to.bitboard().0 != 0))
                && !see_ge(board, m, -100)
            {
                ext = 1;
            }
            let new_depth = depth - 1 + ext;
            self.stack[ply].mv = Some(m);
            self.stack[ply].cont = pc12(stm, piece) * 64 + m.to as usize;
            self.hashes.push(hash);

            // Sacrifice appetite (root only): a move aimed at the enemy king (a check,
            // landing in the king zone, or attacking it) that gives up material is judged
            // `sb` centipawns better than it is. The search window is shifted for that
            // move so alpha-beta stays exact, and the bonus is only granted if the line
            // is not clearly bad for us.
            let sb = if sac_bonus > 0
                && (gives_check
                    || ezone & m.to.bitboard().0 != 0
                    || (piece != Piece::King
                        && piece_attacks(piece, stm, m.to, child.occupied().0) & ezone != 0))
                && !see_ge(board, m, -90)
            {
                sac_bonus
            } else if root && sac_bonus > 0 && (gives_check || attacks_zone || capture) {
                12 // forcing tie-break: among near-equal moves prefer the forcing one
            } else {
                0
            };
            let (wa, wb) = (alpha - sb, beta - sb); // search window for this move
            let mut score;
            if depth >= 3 && move_count > 1 + root as usize && (quiet || !see_ge(board, m, 0)) {
                let mut r = self.lmr[depth.min(63) as usize][move_count.min(63)];
                if !improving {
                    r += 1;
                }
                if cut_node {
                    r += 1;
                }
                if pv_node {
                    r -= 1;
                }
                if gives_check {
                    r -= 1;
                }
                if quiet {
                    if Some(m) == self.killers[ply][0] || Some(m) == self.killers[ply][1] {
                        r -= 1;
                    }
                    r -= hist / 8000;
                    if attacks_zone {
                        r -= 1;
                    }
                }
                let d = (new_depth - r).clamp(1, new_depth.max(1));
                score = -self.search(&child, d, -wa - 1, -wa, ply + 1, true);
                if score > wa && d < new_depth {
                    score = -self.search(&child, new_depth, -wa - 1, -wa, ply + 1, !cut_node);
                }
            } else if !pv_node || move_count > 1 {
                score = -self.search(&child, new_depth, -wa - 1, -wa, ply + 1, !cut_node);
            } else {
                score = -INF;
            }
            if pv_node && (move_count == 1 || score > wa) {
                score = -self.search(&child, new_depth, -wb, -wa, ply + 1, false);
            }
            self.hashes.pop();
            if sb != 0 {
                if score >= SAC_FLOOR && score.abs() < MATE_BOUND {
                    score += sb;
                }
            }

            if self.stop.load(Ordering::Relaxed) {
                return 0;
            }

            if score > best_score {
                best_score = score;
                if score > alpha {
                    best_move = Some(m);
                    if pv_node {
                        self.update_pv(ply, m);
                    }
                    if score >= beta {
                        break;
                    }
                    alpha = score;
                }
            }

            // moves that did not cause a cutoff get a history malus later
            if quiet && nquiets < 63 {
                quiets[nquiets] = m;
                nquiets += 1;
            } else if capture && ncaptures < 31 {
                captures[ncaptures] = m;
                ncaptures += 1;
            }
        }

        if move_count == 0 {
            // only the excluded move was legal (singular search)
            return if excluded.is_some() { alpha } else if in_check { -MATE + ply as i32 } else { self.draw_score(board) };
        }

        if let Some(bm) = best_move {
            if best_score >= beta {
                if !is_capture(board, bm) && bm.promotion.is_none() {
                    if nquiets < 64 {
                        quiets[nquiets] = bm;
                        nquiets += 1;
                    }
                    self.update_quiet_stats(board, bm, &quiets[..nquiets], depth, ply);
                    self.update_capture_stats(board, None, &captures[..ncaptures], depth);
                } else {
                    if ncaptures < 32 && is_capture(board, bm) {
                        captures[ncaptures] = bm;
                        ncaptures += 1;
                    }
                    self.update_capture_stats(board, Some(bm), &captures[..ncaptures], depth);
                }
            }
        }

        if excluded.is_none() {
            let bound = if best_score >= beta {
                Bound::Lower
            } else if pv_node && best_score > orig_alpha {
                Bound::Exact
            } else {
                Bound::Upper
            };
            self.tt.store(hash, best_move, score_to_tt(best_score, ply), static_eval, depth, bound);
        }
        best_score
    }

    pub fn qsearch(&mut self, board: &Board, mut alpha: i32, beta: i32, ply: usize, qply: usize) -> i32 {
        if ply <= MAX_PLY {
            self.pv_len[ply] = ply;
        }
        self.nodes += 1;
        self.check_time();
        if self.stop.load(Ordering::Relaxed) {
            return 0;
        }
        if ply > self.seldepth {
            self.seldepth = ply;
        }
        let in_check = !board.checkers().is_empty();
        if ply >= MAX_PLY - 2 {
            return if in_check { 0 } else { eval::evaluate(board) };
        }
        if eval::is_insufficient_material(board) {
            return self.draw_score(board);
        }

        let hash = board.hash();
        let tt = self.tt.probe(hash);
        if let Some(e) = tt {
            let s = score_from_tt(e.score, ply);
            let cut = match e.bound {
                Bound::Exact => true,
                Bound::Lower => s >= beta,
                Bound::Upper => s <= alpha,
                Bound::None => false,
            };
            if cut && e.bound != Bound::None {
                return s;
            }
        }

        let stm = board.side_to_move();
        // Stalemate is invisible to a captures-only search; check it when it is plausible.
        if !in_check && eval::non_pawn_material(board, stm) == 0 {
            let mut any = false;
            board.generate_moves(|mvs| {
                any = !mvs.is_empty();
                any
            });
            if !any {
                return self.draw_score(board);
            }
        }
        let mut best_score;
        let static_eval;
        if in_check {
            best_score = -MATE + ply as i32;
            static_eval = EVAL_NONE;
        } else {
            static_eval = match tt {
                Some(e) if e.eval != EVAL_NONE => e.eval,
                _ => eval::evaluate(board),
            };
            best_score = static_eval;
            if let Some(e) = tt {
                let s = score_from_tt(e.score, ply);
                let better = match e.bound {
                    Bound::Exact => true,
                    Bound::Lower => s > best_score,
                    Bound::Upper => s < best_score,
                    Bound::None => false,
                };
                if better && s.abs() < MATE_BOUND {
                    best_score = s;
                }
            }
            if best_score >= beta {
                return best_score;
            }
            if best_score > alpha {
                alpha = best_score;
            }
        }

        // generate moves
        let mut list = MoveList::new();
        let them = !stm;
        let enemy = board.colors(them).0;
        let promo_rank = match stm {
            Color::White => 0xFF00_0000_0000_0000u64,
            Color::Black => 0xFFu64,
        };
        let ep = board.en_passant().map(|f| {
            let r = if stm == Color::White { cozy_chess::Rank::Sixth } else { cozy_chess::Rank::Third };
            Square::new(f, r).bitboard().0
        }).unwrap_or(0);
        let want_checks = !in_check && qply == 0;
        let (knight_chk, bishop_chk, rook_chk, pawn_chk) = if want_checks {
            let ek = board.king(them);
            let occ = BitBoard(board.occupied().0);
            (
                cozy_chess::get_knight_moves(ek).0,
                cozy_chess::get_bishop_moves(ek, occ).0,
                cozy_chess::get_rook_moves(ek, occ).0,
                eval::pawn_attacks(ek.bitboard().0, them),
            )
        } else {
            (0, 0, 0, 0)
        };
        board.generate_moves(|mut mvs| {
            if !in_check {
                let mut mask = enemy;
                if mvs.piece == Piece::Pawn {
                    mask |= promo_rank | ep;
                }
                if want_checks {
                    mask |= match mvs.piece {
                        Piece::Pawn => pawn_chk,
                        Piece::Knight => knight_chk,
                        Piece::Bishop => bishop_chk,
                        Piece::Rook => rook_chk,
                        Piece::Queen => bishop_chk | rook_chk,
                        Piece::King => 0,
                    };
                }
                mvs.to &= BitBoard(mask);
            }
            for m in mvs {
                list.push(m);
            }
            false
        });

        // order: captures by MVV-LVA, then checks
        for i in 0..list.len {
            let m = list.moves[i];
            list.scores[i] = if is_capture(board, m) {
                let v = captured_piece(board, m);
                let a = board.piece_on(m.from).unwrap();
                100_000 + SEE_VALUE[v as usize] * 10 - SEE_VALUE[a as usize] / 10
            } else if m.promotion == Some(Piece::Queen) {
                90_000
            } else if in_check {
                self.quiet_score(board, m, ply)
            } else {
                0
            };
        }

        let mut best_move = None;
        let mut legal = 0;
        for i in 0..list.len {
            let m = list.pick(i);
            legal += 1;
            if !in_check {
                let capture = is_capture(board, m);
                if let Some(p) = m.promotion {
                    if p != Piece::Queen {
                        continue;
                    }
                }
                if capture {
                    // delta pruning
                    let gain = SEE_VALUE[captured_piece(board, m) as usize];
                    if m.promotion.is_none() && static_eval + gain + 200 <= alpha {
                        continue;
                    }
                }
                if !see_ge(board, m, 0) {
                    continue;
                }
            }
            let mut child = board.clone();
            child.play_unchecked(m);
            if !in_check && !is_capture(board, m) && m.promotion.is_none() && child.checkers().is_empty() {
                continue; // not actually a check (should not happen often)
            }
            self.stack[ply].mv = Some(m);
            self.stack[ply].cont = pc12(stm, board.piece_on(m.from).unwrap()) * 64 + m.to as usize;
            let score = -self.qsearch(&child, -beta, -alpha, ply + 1, qply + 1);
            if self.stop.load(Ordering::Relaxed) {
                return 0;
            }
            if score > best_score {
                best_score = score;
                if score > alpha {
                    best_move = Some(m);
                    if score >= beta {
                        break;
                    }
                    alpha = score;
                }
            }
        }
        if in_check && legal == 0 {
            return -MATE + ply as i32;
        }

        let bound = if best_score >= beta { Bound::Lower } else { Bound::Upper };
        self.tt.store(hash, best_move, score_to_tt(best_score, ply), static_eval, 0, bound);
        best_score
    }

    fn root_pv(&self) -> Vec<Move> {
        (0..self.pv_len[0]).filter_map(|i| self.pv[0][i]).collect()
    }
}

// ---------------------------------------------------------------------------
// Iterative deepening driver
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default)]
pub struct Limits {
    pub depth: Option<i32>,
    /// Target time: don't start a new iteration after this (scaled by stability).
    pub soft: Option<Duration>,
    /// Absolute maximum time for this move.
    pub hard: Option<Duration>,
    pub nodes: Option<u64>,
}

fn format_score(score: i32) -> String {
    if score >= MATE_BOUND {
        format!("mate {}", (MATE - score + 1) / 2)
    } else if score <= -MATE_BOUND {
        format!("mate -{}", (MATE + score) / 2)
    } else {
        format!("cp {}", score)
    }
}

fn pv_string(board: &Board, pv: &[Move]) -> String {
    let mut b = board.clone();
    let mut out = Vec::new();
    for &m in pv {
        if !b.is_legal(m) {
            break;
        }
        out.push(format!("{}", cozy_chess::util::display_uci_move(&b, m)));
        b.play_unchecked(m);
    }
    out.join(" ")
}

/// Backwards-compatible entry point (used by tests).
pub fn iterative_deepening(
    board: &Board,
    history_hashes: &[u64],
    max_depth: i16,
    time_limit: Option<Duration>,
    stop: Arc<AtomicBool>,
) -> (Option<Move>, i32) {
    let limits = Limits { depth: Some(max_depth as i32), soft: time_limit, hard: time_limit, nodes: None };
    search_position(board, history_hashes, limits, stop, false)
}

pub fn search_position(
    board: &Board,
    history_hashes: &[u64],
    limits: Limits,
    stop: Arc<AtomicBool>,
    print_info: bool,
) -> (Option<Move>, i32) {
    let start = Instant::now();
    let tt = get_global_tt(TT_SIZE_MB.load(Ordering::Relaxed));
    tt.new_search();
    let mut limits = limits;
    let skill = SKILL.load(Ordering::Relaxed).clamp(0, 20);
    if skill < 20 {
        let cap = (200.0 * 1.45f64.powi(skill)) as u64;
        limits.nodes = Some(limits.nodes.map_or(cap, |n| n.min(cap)));
        eval::NOISE.store((20 - skill) * 7, Ordering::Relaxed);
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1);
        eval::NOISE_SEED.store(seed, Ordering::Relaxed);
    } else {
        eval::NOISE.store(0, Ordering::Relaxed);
    }
    eval::set_engine_color(Some(board.side_to_move()));

    // history before the root (exclude the root position itself)
    let mut hist: Vec<u64> = history_hashes.to_vec();
    if hist.last() == Some(&board.hash()) {
        hist.pop();
    }

    let mut root_moves = Vec::new();
    board.generate_moves(|mvs| {
        root_moves.extend(mvs);
        false
    });
    if root_moves.is_empty() {
        let score = if board.checkers().is_empty() { 0 } else { -MATE };
        return (None, score);
    }

    let max_depth = limits.depth.unwrap_or(MAX_PLY as i32 - 8).clamp(1, MAX_PLY as i32 - 8);
    let nodes = Arc::new(AtomicU64::new(0));
    let num_threads = NUM_THREADS.load(Ordering::Relaxed).max(1);

    let mut handles = Vec::new();
    for t in 1..num_threads {
        let b = board.clone();
        let h = hist.clone();
        let tt = Arc::clone(&tt);
        let stop = Arc::clone(&stop);
        let nodes = Arc::clone(&nodes);
        let builder = std::thread::Builder::new().stack_size(64 * 1024 * 1024);
        handles.push(builder.spawn(move || {
            let mut s = Searcher::new(tt, stop, nodes, false);
            s.hashes = h;
            s.root_color = b.side_to_move();
            let mut d = 1 + (t % 2) as i32;
            let mut score = 0;
            while d <= max_depth && !s.stop.load(Ordering::Relaxed) {
                s.root_depth = d;
                score = aspiration(&mut s, &b, d, score);
                d += 1;
            }
        }).expect("failed to spawn helper thread"));
    }

    let mut s = Searcher::new(Arc::clone(&tt), Arc::clone(&stop), Arc::clone(&nodes), true);
    s.hashes = hist;
    s.root_color = board.side_to_move();
    s.start = start;
    s.hard_limit = limits.hard;
    s.node_limit = limits.nodes;

    let mut best_move = Some(root_moves[0]);
    let mut best_score = 0;
    let mut prev_best: Option<Move> = None;
    let mut stability = 0;
    let mut prev_score = 0;

    for depth in 1..=max_depth {
        s.root_depth = depth;
        s.seldepth = 0;
        let score = aspiration(&mut s, board, depth, best_score);
        if stop.load(Ordering::Relaxed) && depth > 1 {
            // partial iteration: keep the move from the last completed one
            break;
        }
        let pv = s.root_pv();
        if let Some(&m) = pv.first() {
            best_move = Some(m);
        }
        best_score = score;

        let elapsed = start.elapsed();
        if print_info {
            let total_nodes = nodes.load(Ordering::Relaxed) + (s.nodes & 1023);
            let ms = elapsed.as_millis().max(1);
            out!(
                "info depth {} seldepth {} score {} nodes {} nps {} hashfull {} time {} pv {}",
                depth,
                s.seldepth,
                format_score(score),
                total_nodes,
                (total_nodes as u128 * 1000 / ms) as u64,
                tt.hashfull(),
                ms,
                pv_string(board, &pv)
            );
        }

        if best_score.abs() >= MATE_BOUND && depth >= 8 && limits.soft.is_some() {
            // a forced mate was found deep enough; no need to keep thinking
            let mate_dist = MATE - best_score.abs();
            if mate_dist < depth {
                break;
            }
        }

        // time management
        if best_move == prev_best {
            stability += 1;
        } else {
            stability = 0;
        }
        prev_best = best_move;
        if let Some(soft) = limits.soft {
            let mut factor = match stability {
                0 => 1.6,
                1 => 1.2,
                2 => 1.0,
                3 => 0.85,
                _ => 0.7,
            };
            if depth > 4 && best_score < prev_score - 30 {
                factor *= 1.3;
            }
            if root_moves.len() == 1 {
                factor = 0.1;
            }
            if elapsed.as_secs_f64() > soft.as_secs_f64() * factor {
                break;
            }
        }
        prev_score = best_score;
    }

    stop.store(true, Ordering::Relaxed);
    for h in handles {
        let _ = h.join();
    }

    // ---- finish beautifully ----
    // With a forced mate in hand, prefer a mate that starts with a sacrifice (a queen sac
    // above all), as long as it still mates within two moves of the fastest mate.
    if best_score >= MATE_BOUND && s.node_limit.is_none() && BEAUTIFUL_MATES.load(Ordering::Relaxed) {
        if let Some(bm) = best_move {
            let mate_plies = MATE - best_score;
            if mate_plies <= 11 && see_ge(board, bm, -90) {
                let budget = limits
                    .hard
                    .map(|h| (h / 10).min(Duration::from_millis(300)))
                    .unwrap_or(Duration::from_millis(300));
                if budget >= Duration::from_millis(20) {
                    s.stop = Arc::new(AtomicBool::new(false));
                    s.hard_limit = Some(start.elapsed() + budget);
                    let target = MATE - (mate_plies + 4);
                    let d = (mate_plies + 3).min(16);
                    s.root_depth = d;
                    let stm = board.side_to_move();
                    let mut best_sac: Option<(i32, Move)> = None;
                    for &m in &root_moves {
                        if m == bm || see_ge(board, m, -90) {
                            continue;
                        }
                        let piece = board.piece_on(m.from).unwrap();
                        let mut child = board.clone();
                        child.play_unchecked(m);
                        s.hashes.push(board.hash());
                        s.stack[0].mv = Some(m);
                        s.stack[0].cont = pc12(stm, piece) * 64 + m.to as usize;
                        let sc = -s.search(&child, d, -target, -target + 1, 1, false);
                        s.hashes.pop();
                        if s.stop.load(Ordering::Relaxed) {
                            break;
                        }
                        if sc >= target {
                            let beauty = -see(board, m) + if piece == Piece::Queen { 500 } else { 0 };
                            if best_sac.map_or(true, |(b, _)| beauty > b) {
                                best_sac = Some((beauty, m));
                            }
                        }
                    }
                    if let Some((_, m)) = best_sac {
                        if print_info {
                            out!(
                                "info string sacrificial mate {} instead of {}",
                                cozy_chess::util::display_uci_move(board, m),
                                cozy_chess::util::display_uci_move(board, bm)
                            );
                        }
                        best_move = Some(m);
                    }
                }
            }
        }
    }
    (best_move, best_score)
}

/// Prefer sacrificial mates over the fastest one ("finish beautifully"). Analysis turns it off.
pub static BEAUTIFUL_MATES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

fn aspiration(s: &mut Searcher, board: &Board, depth: i32, prev: i32) -> i32 {
    let mut delta = 20;
    let (mut alpha, mut beta) = if depth >= 5 && prev.abs() < MATE_BOUND {
        (prev - delta, prev + delta)
    } else {
        (-INF, INF)
    };
    loop {
        let score = s.search(board, depth, alpha, beta, 0, false);
        if s.stop.load(Ordering::Relaxed) {
            return score;
        }
        if score <= alpha {
            beta = (alpha + beta) / 2;
            alpha = (score - delta).max(-INF);
        } else if score >= beta {
            beta = (score + delta).min(INF);
        } else {
            return score;
        }
        delta += delta;
        if delta > 1000 {
            alpha = -INF;
            beta = INF;
        }
    }
}
