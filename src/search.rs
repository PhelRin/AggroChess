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
use crate::nnue::Accumulator;
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
pub static SACRIFICE_BONUS: AtomicI32 = AtomicI32::new(300);
/// A sacrifice only gets the bonus if its line scores at least this much.
pub static SAC_FLOOR: AtomicI32 = AtomicI32::new(-310);
pub static SKILL: AtomicI32 = AtomicI32::new(20);
/// Evaluate with the NNUE (plus the personality layer) instead of the classical evaluation.
pub static USE_NNUE: AtomicBool = AtomicBool::new(true);

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

/// True when `defender`'s king has no flight square: every square next to it is either
/// occupied by its own pieces or attacked by the other side. A trapped king is where
/// quiet "nothing" moves can be mating threats, so those must not be pruned away.
/// Does the quiet move `m` (no capture, promotion, castling or en passant) give check?
/// Worked out from bitboards instead of playing the move: direct checks by the moved piece
/// and discovered checks by our sliders through the square it left.
#[inline]
fn quiet_gives_check(board: &Board, m: Move, piece: Piece, child_occ: u64) -> bool {
    let us = board.side_to_move();
    let k = board.king(!us);
    let kb = k.bitboard().0;
    if piece != Piece::King && piece_attacks(piece, us, m.to, child_occ) & kb != 0 {
        return true;
    }
    // discovered: one of our sliders now sees the king (the moved piece is no longer on
    // `from`; if it is a slider on `to` it was covered above)
    let ours = board.colors(us).0 & !m.from.bitboard().0;
    let diag = (board.pieces(Piece::Bishop) | board.pieces(Piece::Queen)).0 & ours;
    let line = (board.pieces(Piece::Rook) | board.pieces(Piece::Queen)).0 & ours;
    let bocc = BitBoard(child_occ);
    (cozy_chess::get_bishop_moves(k, bocc).0 & diag != 0) || (cozy_chess::get_rook_moves(k, bocc).0 & line != 0)
}

/// Number of squares the king of `defender` could step to (capped at 3).
fn king_flights(board: &Board, defender: Color) -> u32 {
    let k = board.king(defender);
    let own = board.colors(defender).0;
    let enemy = board.colors(!defender).0;
    let occ = board.occupied().0 ^ k.bitboard().0;
    let mut squares = cozy_chess::get_king_moves(k).0 & !own;
    let mut n = 0;
    while squares != 0 {
        let sq = Square::index((squares.trailing_zeros()) as usize);
        squares &= squares - 1;
        if attackers_to(board, sq, occ) & enemy == 0 {
            n += 1;
            if n >= 3 {
                break;
            }
        }
    }
    n
}

fn king_trapped(board: &Board, defender: Color) -> bool {
    let k = board.king(defender);
    let own = board.colors(defender).0;
    let enemy = board.colors(!defender).0;
    let occ = board.occupied().0 ^ k.bitboard().0;
    let mut squares = cozy_chess::get_king_moves(k).0 & !own;
    while squares != 0 {
        let sq = Square::index((squares.trailing_zeros()) as usize);
        squares &= squares - 1;
        if attackers_to(board, sq, occ) & enemy == 0 {
            return false;
        }
    }
    true
}

/// Does the side NOT to move on `board` threaten mate in one (if it were its turn)?
fn mate_threat(board: &Board) -> bool {
    let nb = match board.null_move() {
        Some(b) => b,
        None => return false,
    };
    let mut found = false;
    nb.generate_moves(|mvs| {
        for m in mvs {
            let mut c = nb.clone();
            c.play_unchecked(m);
            if !c.checkers().is_empty() && !c.generate_moves(|_| true) {
                found = true;
                return true;
            }
        }
        false
    });
    found
}

/// After a move (the opponent now to move in `child`), can the opponent win material
/// (a capture worth two pawns or more by static exchange)? Then the move was a sacrifice.
pub fn leaves_material(child: &Board) -> bool {
    let them = child.colors(!child.side_to_move()).0;
    let mut found = false;
    child.generate_moves(|mvs| {
        for m in mvs {
            if them & m.to.bitboard().0 != 0 && see_ge(child, m, 200) {
                found = true;
                return true;
            }
        }
        false
    });
    found
}

/// Root moves for Tal's look: sacrifices of a piece or more (the moved piece, or something left
/// hanging) that check the king or land next to it with at least three attack units aimed at it.
fn tal_candidates(board: &Board, root_moves: &[Move]) -> Vec<Move> {
    tal_candidates_ex(board, root_moves, TAL_LOOK_QUIET.load(Ordering::Relaxed))
}

fn all_moves(b: &Board) -> Vec<Move> {
    let mut v = Vec::new();
    b.generate_moves(|mvs| {
        v.extend(mvs);
        false
    });
    v
}

fn tal_candidates_ex(board: &Board, root_moves: &[Move], quiet_too: bool) -> Vec<Move> {
    let stm = board.side_to_move();
    let ezone = eval::king_zone(board, !stm);
    let mut out: Vec<(i32, Move)> = Vec::new();
    for &m in root_moves {
        let piece = match board.piece_on(m.from) {
            Some(p) => p,
            None => continue,
        };
        if piece == Piece::King || m.promotion.is_some() {
            continue;
        }
        let mut child = board.clone();
        child.play_unchecked(m);
        let gives_check = !child.checkers().is_empty();
        let near = ezone & m.to.bitboard().0 != 0
            || piece_attacks(piece, stm, m.to, child.occupied().0) & ezone != 0;
        // Endgame breakthrough: a piece thrown in front of the enemy pawns (Shirov's
        // ...Bh3!!) to deflect them and run a passed pawn through. Only with little
        // material left, where such a sacrifice is the theme and not a blunder.
        if ENDGAME_LOOK.load(Ordering::Relaxed)
            && piece != Piece::Pawn
            && board.pieces(Piece::Queen).is_empty()
            && eval::non_pawn_material(board, stm) <= 500
            && eval::non_pawn_material(board, !stm) <= 500
            && !see_ge(board, m, -250)
            && {
                // thrown at a pawn that holds back our own pawns (two or more of them on its
                // file or the files next to it): taking the piece lets them through
                let takers = cozy_chess::get_pawn_attacks(m.to, stm) & board.colors(!stm) & board.pieces(Piece::Pawn);
                let ours = board.colors(stm) & board.pieces(Piece::Pawn);
                takers.into_iter().any(|t| {
                    let f = t.file() as i32;
                    ours.into_iter().filter(|p| (p.file() as i32 - f).abs() <= 1).count() >= 2
                })
            }
        {
            out.push((5_000 - see(board, m), m));
            continue;
        }
        if !gives_check && !near {
            continue;
        }
        let big = !see_ge(board, m, -250) || leaves_material_at(&child, 300);
        if !big {
            continue;
        }
        if gives_check || (quiet_too && attack_units(&child, stm, ezone) >= 3) {
            // checks first, then the biggest sacrifices
            out.push((gives_check as i32 * 10_000 - see(board, m), m));
        }
    }
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out.into_iter().take(2).map(|x| x.1).collect()
}

/// Like `leaves_material`, with the gain threshold given.
fn leaves_material_at(child: &Board, gain: i32) -> bool {
    let them = child.colors(!child.side_to_move()).0;
    let mut found = false;
    child.generate_moves(|mvs| {
        for m in mvs {
            if them & m.to.bitboard().0 != 0 && see_ge(child, m, gain) {
                found = true;
                return true;
            }
        }
        false
    });
    found
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

/// The sacrifice settings for one search.
#[derive(Clone, Copy, Debug)]
pub struct SacParams {
    pub bonus: i32,
    pub floor: i32,
    pub units: i32,
    /// any material sacrifice gets (half) the bonus, not only ones aimed at the king
    pub any: bool,
    /// bonus for a sacrifice that follows our own sacrifice within the search (a chain)
    pub chain: i32,
}

impl SacParams {
    pub fn base() -> Self {
        SacParams {
            bonus: SACRIFICE_BONUS.load(Ordering::Relaxed),
            floor: SAC_FLOOR.load(Ordering::Relaxed),
            units: SAC_FULL_UNITS.load(Ordering::Relaxed),
            any: false,
            chain: chain_bonus(),
        }
    }
}

/// Sacrifices AggroChess has already made in the current game, by colour (worked out from the
/// game's moves by the UCI layer, with the same rule the match statistics use).
pub static GAME_SACS: [AtomicI32; 2] = [AtomicI32::new(0), AtomicI32::new(0)];
/// Score of the engine's previous search in this game (its own point of view).
pub static LAST_SCORE: AtomicI32 = AtomicI32::new(0);
/// The same without the sacrifice bonus: what the engine objectively thought of its last move.
pub static LAST_OBJ: AtomicI32 = AtomicI32::new(0);
/// Every game should have a sacrifice: until AggroChess has made one, its appetite grows with
/// every move, and faster when the game is already decided.
pub static SAC_PRESSURE: AtomicBool = AtomicBool::new(true);
/// Pressure tuning: the move it starts at, and growth per move.
pub static PRESSURE_START: AtomicI32 = AtomicI32::new(20);
pub static PRESSURE_STEP: AtomicI32 = AtomicI32::new(15);
/// Sacrifice bonus once AggroChess has already sacrificed in this game (-1 = unchanged); +56 Elo at long time controls over keeping the full bonus.
pub static AFTER_SAC_BONUS: AtomicI32 = AtomicI32::new(60);
/// Chained sacrifices (Tal): a sacrifice within CHAIN_MOVES of our own moves after another
/// one keeps (at least) this bonus. -1 = the normal sacrifice bonus.
pub static CHAIN_BONUS: AtomicI32 = AtomicI32::new(150);
pub static CHAIN_MOVES: AtomicI32 = AtomicI32::new(2);
/// ...but only while the sacrifice is working: our previous search scored at least this.
pub static CHAIN_MIN: AtomicI32 = AtomicI32::new(50);
/// Own moves since each side's latest sacrifice in the game (set by the UCI layer).
pub static LAST_SAC_AGO: [AtomicI32; 2] = [AtomicI32::new(1000), AtomicI32::new(1000)];

fn chain_bonus() -> i32 {
    let c = CHAIN_BONUS.load(Ordering::Relaxed);
    if c < 0 { SACRIFICE_BONUS.load(Ordering::Relaxed) } else { c }
}

/// Sacrifice settings for a search from `board` (AggroChess to move).
pub fn sac_params_for(board: &Board) -> SacParams {
    let mut p = SacParams::base();
    if p.bonus <= 0 || !SAC_PRESSURE.load(Ordering::Relaxed) {
        return p;
    }
    if GAME_SACS[board.side_to_move() as usize].load(Ordering::Relaxed) > 0 {
        // The game already has its sacrifice: from now on a sacrifice has to be (nearly)
        // sound on its own merits, so the appetite drops to the after-sacrifice level.
        let after = AFTER_SAC_BONUS.load(Ordering::Relaxed);
        let ago = LAST_SAC_AGO[board.side_to_move() as usize].load(Ordering::Relaxed);
        if ago <= CHAIN_MOVES.load(Ordering::Relaxed) && LAST_OBJ.load(Ordering::Relaxed) >= CHAIN_MIN.load(Ordering::Relaxed) {
            // we sacrificed a move or two ago and the attack is working (our last search
            // already liked the position): the next sacrifice continues the chain
            p.bonus = p.bonus.max(p.chain);
        } else if after >= 0 {
            p.bonus = after.min(p.bonus);
        }
        return p;
    }
    let moves = board.fullmove_number() as i32 - PRESSURE_START.load(Ordering::Relaxed);
    let last = LAST_SCORE.load(Ordering::Relaxed).abs();
    // a decided game ends soon (and a sacrifice costs little there): hurry
    let decided = if last >= 600 { 30 } else if last >= 300 { 12 } else { 0 };
    let t = (moves.max(0) + decided).min(30);
    if t == 0 {
        return p;
    }
    let step = PRESSURE_STEP.load(Ordering::Relaxed);
    p.bonus += step * t;
    p.floor -= step * t * 2 / 3;
    // when already well behind, every line scores below the usual floor: measure from where we are
    let own = LAST_SCORE.load(Ordering::Relaxed);
    if own < 0 {
        p.floor = p.floor.min(own - 250);
    }
    if t >= 6 {
        p.units = p.units.min(2);
    }
    if t >= 12 {
        p.units = 1;
        p.any = true;
    }
    p
}

/// Attack units (knight/bishop/rook 1, queen 2) aimed at the king for the full sacrifice bonus.
pub static SAC_FULL_UNITS: AtomicI32 = AtomicI32::new(3);

/// Search sacrifice candidates at the root one ply deeper.
pub static SAC_VERIFY: AtomicBool = AtomicBool::new(false);

/// Mating nets: when the enemy king has no flight square, quiet moves are not pruned (they may
/// be mating threats), and a boxed-in king does not try the null move.
/// UCI `go searchmoves`: when not empty, only these root moves are searched.
pub static ROOT_FILTER: std::sync::Mutex<Vec<Move>> = std::sync::Mutex::new(Vec::new());

/// Root sacrifices that fail low in the null-window search are searched again as PV moves.
pub static SAC_PV: AtomicBool = AtomicBool::new(true);

/// Root moves that leave material hanging (not only the moved piece) count as sacrifices.
pub static TAL_SACS: AtomicBool = AtomicBool::new(false);

/// Recognise locked fortresses (Penrose's Puzzle) as draws.
pub static FORTRESS: AtomicBool = AtomicBool::new(true);
/// Also recognise sealed pawn walls (every pawn rammed, nothing can cross: Friedel's study).
pub static SEALED: AtomicBool = AtomicBool::new(true);
/// No null move for a side with a single piece (plus pawns): queen against rook and similar
/// endings are decided by zugzwang (Amann's study), which the null move cannot see.
pub static ZUGZWANG_GUARD: AtomicBool = AtomicBool::new(false);
/// Recognise stalemate fortresses (boxed-in king, frozen pawns, one piece left).
pub static STALEMATE_BOX: AtomicBool = AtomicBool::new(false);
/// Rescue look: in a lost endgame (quick score at or below -RESCUE_BELOW), RESCUE_PCT of the
/// time goes to searching up to RESCUE_MAX material-returning moves on their own.
pub static RESCUE: AtomicBool = AtomicBool::new(false);
pub static RESCUE_BELOW: AtomicI32 = AtomicI32::new(250);
pub static RESCUE_PCT: AtomicI32 = AtomicI32::new(50);
pub static RESCUE_MAX: AtomicI32 = AtomicI32::new(3);

fn lone_piece(board: &Board, c: Color) -> bool {
    (board.colors(c) & !board.pieces(Piece::Pawn) & !board.pieces(Piece::King)).len() == 1
}
/// Root moves re-searched at full depth with an open window when the best so far is a draw.
pub static ROOT_DRAW_ESCAPE: AtomicI32 = AtomicI32::new(0);
/// ...and root moves are not reduced while the best so far is this bad (centipawns).
pub static ROOT_FULL_BELOW: AtomicI32 = AtomicI32::new(100_000);
/// Endgame bishops that cannot move at all are worth this much less (centipawns, 0 = off).
pub static DEAD_BISHOP: AtomicI32 = AtomicI32::new(200);
/// Minor-piece endgames (at most one minor per side, no rooks or queens): the net saw few of
/// them and misjudges passed pawns against a lone bishop, so the classical evaluation (which
/// knows about passers, their king support and the square rule) is mixed in.
/// 0 = off, otherwise the classical share in percent.
pub static EG_BLEND: AtomicI32 = AtomicI32::new(0);
/// 0 = only minor-against-pawns endings, 1 = every ending with at most one minor per side.
pub static EG_BLEND_WIDE: AtomicBool = AtomicBool::new(false);
/// Experimental: fewer reductions in minor-piece endings.
pub static EG_LMR: AtomicI32 = AtomicI32::new(0);
/// Draw escape: on the principal variation, when the best move so far only draws, a move
/// that fails low against that draw is searched again with an open lower bound. Against a
/// draw score the null-window search lets the other side "prove" a draw by repetition in
/// lines where the winning plan is just beyond the reduced depth (Shirov's ...Bh3!!).
/// Scale the net's verdict in opposite-coloured bishop endings (see `eval::ocb_scale`).
pub static OCB_SCALE: AtomicBool = AtomicBool::new(true);
/// Tal's look also takes piece sacrifices that break through pawns in the endgame.
pub static ENDGAME_LOOK: AtomicBool = AtomicBool::new(true);
/// Tal's look also for non-checking sacrifices with a big attack behind them (off: they
/// are common in attacking positions and almost never better, so the look cost time).
pub static TAL_LOOK_QUIET: AtomicBool = AtomicBool::new(false);
/// Tal's look only when there is time for it (ms of hard limit for the move). The
/// sacrifices it is for need depth 17+ to be seen at all; in a fast game the look would
/// only take time from the main search.
pub static TAL_LOOK_MIN_MS: AtomicI32 = AtomicI32::new(4000);
/// Hash size (MB) of Tal's look's own search.
pub static LOOK_TT_MB: AtomicI32 = AtomicI32::new(16);
/// Mate probe: when the engine is a lot of material down (a sacrifice or worse), its own
/// positions in the search are checked by the threat-space mate prover. Material lost for a
/// mating attack only pays off if the mate is there, and the alpha-beta search finds long
/// mates of quiet threats (Plaskett's Puzzle) far too late. Value: node budget per probe
/// (0 = off).
pub static MATE_PROBE: AtomicI32 = AtomicI32::new(2000);
pub static MATE_PROBE_DEFICIT: AtomicI32 = AtomicI32::new(900);
pub static MATE_PROBE_DEPTH: AtomicI32 = AtomicI32::new(6);
/// Tal's re-search of failed sacrifices also on our moves along the principal variation, up
/// to this ply (a combination often starts with checks before the real sacrifice).
pub static SAC_PV_PLY: AtomicI32 = AtomicI32::new(0);
const MATE_PROBE_VISITS: u32 = 12;
/// The mate prover's share of the search effort, in percent of the search's nodes.
pub static MATE_PROBE_SHARE: AtomicI32 = AtomicI32::new(15);
pub static MATE_PROBES: AtomicU64 = AtomicU64::new(0);
/// Tal mode (mate-threat extensions) inside Tal's look.
pub static LOOK_TAL: AtomicBool = AtomicBool::new(false);
pub static DRAW_ESCAPE: AtomicI32 = AtomicI32::new(2);

fn minor_ending(board: &Board) -> bool {
    if !(board.pieces(Piece::Rook) | board.pieces(Piece::Queen)).is_empty() {
        return false;
    }
    let minors = board.pieces(Piece::Knight) | board.pieces(Piece::Bishop);
    let w = (minors & board.colors(Color::White)).len();
    let b = (minors & board.colors(Color::Black)).len();
    if w > 1 || b > 1 || w + b == 0 {
        return false;
    }
    EG_BLEND_WIDE.load(Ordering::Relaxed) || w + b == 1
}

pub static MATE_NET: AtomicBool = AtomicBool::new(true);

/// Tal mode: when AggroChess is clearly worse (its last completed iteration scored below
/// TAL_MODE_BELOW), it spends effort on Tal's way out: quiet moves in a mating net are not
/// pruned, a boxed-in king facing mate is searched deeper, and a sacrificial check or king
/// attack at the root that failed a quick test gets a full second look. These cost too much
/// search speed to use all the time; when already in trouble, a hidden resource is worth more.
pub static TAL_MODE: AtomicBool = AtomicBool::new(false);
pub static TAL_MODE_BELOW: AtomicI32 = AtomicI32::new(-150);

/// The cheap mating-net guards (no futility/late-move pruning of quiet moves, no razoring,
/// when the enemy king has no flight square) also outside Tal mode.
/// Captures in the enemy king zone that lose material are reduced less (late-move reductions).
pub static ZONE_SAC_LMR: AtomicBool = AtomicBool::new(false);

pub static ZONE_SAC_ORDER: AtomicBool = AtomicBool::new(true);

/// Tal's look (see search_position) and the share of the thinking time it gets.
pub static TAL_LOOK: AtomicBool = AtomicBool::new(true);
pub static TAL_LOOK_PCT: AtomicI32 = AtomicI32::new(30);
pub static TAL_LOOK_PCT_PROMISING: AtomicI32 = AtomicI32::new(45);

pub static NET_ALWAYS: AtomicBool = AtomicBool::new(true);

/// Deep-think mode: the v4.3 additions (Tal's look and the mating-net, sacrifice, fortress
/// and endgame search changes) are for long thinks, where the positions they are about can
/// actually be searched out. With little time per move the search is exactly 4.1's, which
/// keeps fast-game strength where it was.
pub static DEEP: AtomicBool = AtomicBool::new(false);
/// Thinking time per move (ms of the hard limit) from which deep mode is on.
pub static DEEP_MIN_MS: AtomicI32 = AtomicI32::new(4000);

#[inline]
fn deep() -> bool {
    DEEP.load(Ordering::Relaxed)
}

#[inline]
fn pv_node_excluded(excluded: Option<Move>) -> bool {
    excluded.is_some()
}

#[inline]
fn stm_is_root(board: &Board, root: Color) -> bool {
    board.side_to_move() == root
}

#[inline]
fn net_always() -> bool {
    NET_ALWAYS.load(Ordering::Relaxed) && MATE_NET.load(Ordering::Relaxed) && deep()
}

#[inline]
fn tal_mode() -> bool {
    TAL_MODE.load(Ordering::Relaxed)
}
/// Root sacrifice candidates are searched to full depth (no late-move reduction).
pub static SAC_FULL_DEPTH: AtomicBool = AtomicBool::new(true);

/// Whether the sacrifice bonus is scaled by the attack the sacrifice brings.
pub static SAC_NEEDS_ATTACK: AtomicBool = AtomicBool::new(true);

/// Attack units `us` has aimed at the enemy king zone `ezone` on `board`: knights, bishops and
/// rooks count 1, queens 2.
fn attack_units(board: &Board, us: Color, ezone: u64) -> u32 {
    let occ = board.occupied().0;
    let mut units = 0;
    for p in [Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen] {
        for sq in board.colored_pieces(us, p) {
            if piece_attacks(p, us, sq, occ) & ezone != 0 {
                units += if p == Piece::Queen { 2 } else { 1 };
            }
        }
    }
    units
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
    acc: Vec<Accumulator>,
    acc_key: Vec<u64>,
    nnue: bool,
    sac: SacParams,
    /// the mate prover for desperate positions (see MATE_PROBE), made on first use
    mate: Option<Box<crate::matesearch::MateSearch<'static>>>,
    mate_probed: std::collections::HashMap<u64, u32>,
    /// the root move being searched is a sacrifice (a sacrifice two plies later is a chain)
    root_sac: bool,
    /// sacrifice bonus included in the root's best score (so the objective score is known)
    root_best_sb: i32,
    /// root moves to search (empty = all), for Tal's look
    root_only: Vec<Move>,
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
            acc: vec![Accumulator::default(); MAX_PLY + 8],
            acc_key: vec![0; MAX_PLY + 8],
            nnue: USE_NNUE.load(Ordering::Relaxed),
            sac: SacParams::base(),
            root_only: Vec::new(),
            mate: None,
            mate_probed: std::collections::HashMap::new(),
            root_sac: false,
            root_best_sb: 0,
        }
    }

    /// Make sure the accumulator for `ply` matches `board` (refresh it from scratch if not).
    #[inline]
    fn ensure_acc(&mut self, board: &Board, ply: usize) {
        if self.nnue && self.acc_key[ply] != board.hash() {
            self.acc[ply].refresh(board);
            self.acc_key[ply] = board.hash();
        }
    }

    /// Build the child's accumulator incrementally from the parent's.
    #[inline]
    fn push_acc(&mut self, board: &Board, m: Move, child: &Board, ply: usize) {
        if self.nnue && self.acc_key[ply] == board.hash() {
            let (lo, hi) = self.acc.split_at_mut(ply + 1);
            hi[0].apply_from(&lo[ply], board, m);
            self.acc_key[ply + 1] = child.hash();
        }
    }

    /// Static evaluation from the side to move's point of view.
    #[inline]
    fn static_eval(&mut self, board: &Board, ply: usize) -> i32 {
        if FORTRESS.load(Ordering::Relaxed)
            && deep()
            && (eval::is_locked_fortress(board) || (SEALED.load(Ordering::Relaxed) && eval::is_sealed_fortress(board)))
        {
            return 0;
        }
        if self.nnue {
            if eval::is_insufficient_material(board) {
                return 0;
            }
            // Against a bare king the net has no idea how to drive it into a corner (it hardly
            // ever saw such positions in training); the classical evaluation's mop-up does.
            // The same goes for other sparse endgames without pawns (5 pieces or fewer, kings
            // included, like queen against rook): the net saw few of them, the classical
            // evaluation handles them well. Pawn endgames stay with the net.
            let w = board.colors(Color::White);
            let b = board.colors(Color::Black);
            if w.len() == 1
                || b.len() == 1
                || (board.occupied().len() <= 5 && board.pieces(Piece::Pawn).is_empty())
            {
                return eval::evaluate(board);
            }
            self.ensure_acc(board, ply);
            let mut nn = self.acc[ply].evaluate(board);
            if OCB_SCALE.load(Ordering::Relaxed) && deep() {
                if let Some(f) = eval::ocb_scale(board) {
                    nn = nn * f / 64;
                }
            }
            if STALEMATE_BOX.load(Ordering::Relaxed) && deep() {
                if let Some(f) = eval::stalemate_box_scale(board) {
                    nn = nn * f / 64;
                }
            }
            let dead = DEAD_BISHOP.load(Ordering::Relaxed);
            if dead > 0 && deep() {
                nn += eval::dead_bishop_adj(board, dead);
            }
            let mut v = nn + eval::personality(board) + eval::noise(board);
            let mix = EG_BLEND.load(Ordering::Relaxed);
            if mix > 0 && minor_ending(board) {
                v = (v * (100 - mix) + eval::evaluate(board) * mix) / 100;
            }
            // a static evaluation must never look like a mate score
            v.clamp(-(MATE_BOUND - 2000), MATE_BOUND - 2000)
        } else {
            eval::evaluate(board)
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
                } else if ezone & m.to.bitboard().0 != 0 && ZONE_SAC_ORDER.load(Ordering::Relaxed) && deep() {
                    // a capture next to the king that gives material (Rxh7!, Bxh7+) is tried
                    // with the good moves: it is a sacrifice, not a bad trade
                    2_950_000 + base + ch / 16
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
        // mate threat extension: a boxed-in king facing mate in one gets one more ply, so long
        // chains of quiet mating threats (Plaskett's Puzzle) are followed to the end

        self.nodes += 1;
        self.check_time();
        if ply > self.seldepth {
            self.seldepth = ply;
        }
        self.ensure_acc(board, ply);

        if !root {
            if self.stop.load(Ordering::Relaxed) {
                return 0;
            }
            if board.halfmove_clock() >= 100 || self.is_repetition(board) || eval::is_insufficient_material(board) {
                return self.draw_score(board);
            }
            if ply >= MAX_PLY - 2 {
                return if in_check { 0 } else { self.static_eval(board, ply) };
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
                _ => self.static_eval(board, ply),
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
        // Mate threat: a boxed-in king facing mate in one is not pruned on its material lead
        // (reverse futility, null move) and gets one more ply, so long chains of quiet mating
        // threats (Plaskett's Puzzle) are followed to the end. Only checked where it matters:
        // on the principal variation, and where the side to move looks good enough to prune.
        let threatened = !in_check
            && ply > 0
            && (pv_node || eval_est >= beta)
            && tal_mode()
            && king_trapped(board, board.side_to_move())
            && mate_threat(board);
        if threatened && (ply as i32) < 2 * self.root_depth {
            depth += 1;
        }
        // mate probe (deep mode): far behind in material, look for a forced mate
        if deep()
            && !root
            && !pv_node_excluded(excluded)
            && ply <= 12
            && depth >= MATE_PROBE_DEPTH.load(Ordering::Relaxed)
            && stm_is_root(board, self.root_color)
            && king_flights(board, !board.side_to_move()) <= 1
            && eval::material_balance(board, board.side_to_move()) <= -MATE_PROBE_DEFICIT.load(Ordering::Relaxed)
        {
            let budget = MATE_PROBE.load(Ordering::Relaxed);
            // each visit probes again with a bigger budget (the prover's table keeps what it
            // learned), so positions the search keeps coming back to get looked at harder
            let visits = {
                let v = self.mate_probed.entry(hash).or_insert(0);
                *v += 1;
                *v
            };
            let budget = budget * visits as i32;
            // the prover never gets more than a share of the search's own effort
            let spent = self.mate.as_ref().map_or(0, |m| m.nodes());
            let allowed = self.nodes * MATE_PROBE_SHARE.load(Ordering::Relaxed) as u64 / 100 + 20_000;
            if budget > 0 && visits <= MATE_PROBE_VISITS && spent < allowed {
                MATE_PROBES.fetch_add(1, Ordering::Relaxed);
                let ms = self.mate.get_or_insert_with(|| Box::new(crate::matesearch::probe_searcher()));
                if let Some((_, n)) = ms.probe_mate(board, 14, budget as u64) {
                    let score = MATE - ply as i32 - (2 * n as i32 - 1);
                    self.tt.store(hash, None, score_to_tt(score, ply), static_eval, MAX_PLY as i32, Bound::Exact);
                    return score;
                }
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

        if !pv_node && !in_check && excluded.is_none() && !threatened {
            // reverse futility pruning
            if depth <= 8 && eval_est.abs() < MATE_BOUND && eval_est - (80 * depth - 60 * improving as i32) >= beta {
                return eval_est;
            }
            // razoring
            if depth <= 3 && has_pieces && eval_est + 200 + 150 * depth <= alpha
                && !(net_always() && king_trapped(board, !stm))
            {
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
                && !(tal_mode() && king_trapped(board, stm))
                && !(deep() && ZUGZWANG_GUARD.load(Ordering::Relaxed) && lone_piece(board, stm))
            {
                if let Some(nb) = board.null_move() {
                    let r = 3 + depth / 3 + ((eval_est - beta) / 200).min(3);
                    self.stack[ply].mv = None;
                    self.stack[ply].cont = NO_CONT;
                    self.hashes.push(hash);
                    if self.nnue && self.acc_key[ply] == hash {
                        let (lo, hi) = self.acc.split_at_mut(ply + 1);
                        hi[0].v = lo[ply].v;
                        self.acc_key[ply + 1] = nb.hash();
                    }
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
            self.sac.bonus
        } else if ply == 2 && stm == self.root_color {
            if self.root_sac { self.sac.bonus.max(self.sac.chain) } else { self.sac.bonus }
        } else if ply == 4 && stm == self.root_color {
            self.sac.bonus / 2
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
        let tal = tal_mode();
        let mate_net = net_always() || tal;
        let mut sac_researches = 0;
        let mut draw_researches = 0;
        let draw_here = self.draw_score(board);
        // only in endgames, where "no progress" is the danger (in the middlegame a draw on
        // the board is a perpetual, and the other moves are searched well enough)
        let draw_escape = if pv_node
            && depth >= 10
            && board.pieces(Piece::Queen).is_empty()
            && eval::non_pawn_material(board, Color::White) <= 500
            && eval::non_pawn_material(board, Color::Black) <= 500
        {
            if deep() { DRAW_ESCAPE.load(Ordering::Relaxed) } else { 0 }
        } else if root && depth >= 10 && deep() {
            // at the root a draw is never taken without a real look at the alternatives:
            // a quiet winning try (Amann's Qd8!) is otherwise only seen through reductions
            ROOT_DRAW_ESCAPE.load(Ordering::Relaxed)
        } else {
            0
        };
        let mut enemy_net: Option<bool> = None;

        let root_filter: Vec<Move> = if !root {
            Vec::new()
        } else if !self.root_only.is_empty() {
            self.root_only.clone()
        } else {
            ROOT_FILTER.lock().unwrap().clone()
        };
        for i in 0..list.len {
            let m = list.pick(i);
            if Some(m) == excluded {
                continue;
            }
            if !root_filter.is_empty() && !root_filter.contains(&m) {
                continue;
            }
            let capture = is_capture(board, m);
            let quiet = !capture && m.promotion.is_none();
            if quiet && skip_quiets {
                continue;
            }
            let piece = board.piece_on(m.from).unwrap();

            // Quiet moves (most of which get pruned) are judged without playing them: the
            // board copy is only made for moves that are actually searched.
            let simple_quiet = quiet && !(piece == Piece::King && board.colors(stm).has(m.to));
            let mut child_slot: Option<Board> = None;
            let (gives_check, child_occ) = if simple_quiet {
                let child_occ = (occ & !m.from.bitboard().0) | m.to.bitboard().0;
                (quiet_gives_check(board, m, piece, child_occ), child_occ)
            } else {
                let mut c = board.clone();
                c.play_unchecked(m);
                let gc = !c.checkers().is_empty();
                let o = c.occupied().0;
                child_slot = Some(c);
                (gc, o)
            };
            let attacks_zone = quiet
                && piece != Piece::King
                && piece_attacks(piece, stm, m.to, child_occ) & ezone != 0
                && piece_attacks(piece, stm, m.from, occ) & ezone == 0;

            move_count += 1;
            let hist = if quiet { self.quiet_score(board, m, ply) } else { 0 };

            // ---- pruning at shallow depth ----
            if !root && best_score > -MATE_BOUND && has_pieces {
                let lmr_d = (depth - 1 - self.lmr[depth.min(63) as usize][move_count.min(63)]).max(0);
                if quiet && !gives_check {
                    let lmp = move_count as i32 > lmp_limit && !attacks_zone;
                    let futile = !in_check && lmr_d <= 7 && eval_est + 100 + 90 * lmr_d + hist / 128 <= alpha;
                    // in a mating net, quiet moves may be mating threats: keep them
                    if (lmp || futile) && !(mate_net && *enemy_net.get_or_insert_with(|| king_trapped(board, them))) {
                        if lmp {
                            skip_quiets = true;
                        }
                        continue;
                    }
                    let threshold = if attacks_zone { -60 * depth * depth } else { -25 * lmr_d * lmr_d };
                    if !see_ge(board, m, threshold) && !(tal && *enemy_net.get_or_insert_with(|| king_trapped(board, them))) {
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

            let child = match child_slot {
                Some(c) => c,
                None => {
                    let mut c = board.clone();
                    c.play_unchecked(m);
                    debug_assert_eq!(gives_check, !c.checkers().is_empty(), "quiet_gives_check {:?}", m);
                    c
                }
            };

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
            self.push_acc(board, m, &child, ply);
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
                && !see_ge(board, m, -150) // about two pawns or more: a single pawn is not a sacrifice
            {
                if SAC_NEEDS_ATTACK.load(Ordering::Relaxed) {
                    // Scale the bonus by how much attack the sacrifice brings: a sacrifice with
                    // several pieces aimed at the king is a real attack, a lone piece is a spite sac.
                    let units = attack_units(&child, stm, ezone) as i32;
                    let full = self.sac.units;
                    if units >= full {
                        sac_bonus
                    } else if units == full - 1 {
                        sac_bonus * 3 / 5
                    } else {
                        sac_bonus / 4
                    }
                } else {
                    sac_bonus
                }
            } else if sac_bonus > 0
                && piece == Piece::Rook
                && matches!(board.piece_on(m.to), Some(Piece::Knight) | Some(Piece::Bishop))
                && !see_ge(board, m, -90)
            {
                // exchange sacrifice (Rxf6!, Rxc3!): a rook for a minor piece to wreck the
                // pawns or open lines, even when it isn't aimed straight at the king
                sac_bonus
            } else if root && sac_bonus > 0 && TAL_SACS.load(Ordering::Relaxed) && leaves_material(&child) {
                // Tal's kind of sacrifice: the moved piece is safe, but something else is left
                // hanging (a piece, the exchange, two pawns) for the attack
                let units = attack_units(&child, stm, ezone) as i32;
                let full = self.sac.units;
                if units >= full {
                    sac_bonus * 3 / 4
                } else if units == full - 1 {
                    sac_bonus * 2 / 5
                } else {
                    sac_bonus / 6
                }
            } else if sac_bonus > 0 && self.sac.any && piece != Piece::King && !see_ge(board, m, -150) {
                // under sacrifice pressure, any real material sacrifice counts (at a discount)
                sac_bonus / 2
            } else if root && sac_bonus > 0 && (gives_check || attacks_zone || capture) {
                12 // forcing tie-break: among near-equal moves prefer the forcing one
            } else {
                0
            };
            // verify sacrifice candidates at the root one ply deeper before trusting them
            let new_depth = if root && sb > 12 && SAC_VERIFY.load(Ordering::Relaxed) { new_depth + 1 } else { new_depth };
            if root {
                self.root_sac = sb > 12;
            }
            let (wa, wb) = (alpha - sb, beta - sb); // search window for this move
            let mut score;
            let full_sac = root && (sb > 12 || (tal && gives_check)) && SAC_FULL_DEPTH.load(Ordering::Relaxed) && deep();
            // at the root, when the best so far is a draw, every alternative gets its full depth:
            // a quiet winning try is invisible through reductions (Amann's Qd8!)
            // the same when the best so far is clearly bad: hidden resources (a sacrifice into a
            // stalemate fortress) are what a worse position needs
            let no_draw_lmr = root
                && deep()
                && depth >= 10
                && ROOT_DRAW_ESCAPE.load(Ordering::Relaxed) > 0
                && (best_score == draw_here || (best_score > -INF && best_score <= -ROOT_FULL_BELOW.load(Ordering::Relaxed)));
            if depth >= 3 && move_count > 1 + root as usize && (quiet || !see_ge(board, m, 0)) && !full_sac && !no_draw_lmr {
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
                    if enemy_net == Some(true) {
                        r -= 1;
                    }
                } else if capture && ezone & m.to.bitboard().0 != 0 && ZONE_SAC_LMR.load(Ordering::Relaxed) {
                    // a capture next to the king that gives material back (Rxh7!) is a
                    // sacrifice to rip the king open, not a bad trade: barely reduce it
                    r -= 2;
                }
                let egl = EG_LMR.load(Ordering::Relaxed);
                if egl > 0 && minor_ending(board) {
                    r -= egl;
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
            } else if (root || (pv_node && (ply as i32) <= SAC_PV_PLY.load(Ordering::Relaxed) && stm == self.root_color))
                && move_count > 1
                && depth >= 12
                && sac_researches < if no_draw_lmr { 3 } else { 1 }
                && tal
                && SAC_PV.load(Ordering::Relaxed)
                && (gives_check || sb > 12 || (no_draw_lmr && !see_ge(board, m, -150)))
                && score > wa - 1000
                && leaves_material(&child)
            {
                sac_researches += 1;
                // Tal's rule: a sacrifice at the root gets a real look. A null-window search
                // prunes the quiet, long-term follow-ups a sacrifice relies on, so a sacrificing
                // move that failed low is searched again as a principal variation.
                // with an open lower bound: a narrow window here decides what gets pruned below
                score = -self.search(&child, new_depth, -wb, INF, ply + 1, false);
            } else if pv_node
                && move_count > 1
                && depth >= 10
                && draw_researches < draw_escape
                && stm == self.root_color
                && best_score == draw_here
                && score > best_score - if root { 1000 } else { 300 }
                && !self.stop.load(Ordering::Relaxed)
            {
                draw_researches += 1;
                score = -self.search(&child, new_depth, -wb, INF, ply + 1, false);
            }
            let mut sb_added = 0;
            if sb != 0 && score >= wb && score < self.sac.floor && deep() && !self.stop.load(Ordering::Relaxed) {
                // The shifted window failed high, but below the floor the bonus does not apply:
                // the score is only a bound, so it must not be used as the move's value. Search
                // the move again with the true window.
                score = -self.search(&child, new_depth, -beta, -alpha, ply + 1, false);
            } else if sb != 0 && score >= self.sac.floor && score.abs() < MATE_BOUND {
                score += sb;
                sb_added = sb;
            }
            self.hashes.pop();

            if self.stop.load(Ordering::Relaxed) {
                return 0;
            }

            if score > best_score {
                best_score = score;
                if score > alpha {
                    best_move = Some(m);
                    if root {
                        self.root_best_sb = sb_added;
                    }
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
            return if in_check { 0 } else { self.static_eval(board, ply) };
        }
        self.ensure_acc(board, ply);
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
                _ => self.static_eval(board, ply),
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
            self.push_acc(board, m, &child, ply);
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
// Tal's forcing lines
// ---------------------------------------------------------------------------

/// Priming: in deep mode, when the position looks bad, our checks are followed a few plies
/// down their main lines, and positions there with a big checking sacrifice on (Plaskett's
/// 3.Bc2+!! after 1.Nf6+ Kg7 2.Nh5+ Kg6) are searched first as positions of their own. The
/// results go into the shared hash table, so the main search then knows what those lines
/// are worth. Inside the main search such a position is only ever a side line, searched with
/// a window set elsewhere and too little depth for a sacrifice whose point is 25 plies away.
pub static PRIME: AtomicBool = AtomicBool::new(true);
pub static LOOK_PRIME: AtomicBool = AtomicBool::new(true);
pub static LOOK_PRIME_PCT: AtomicI32 = AtomicI32::new(50);
pub static PRIME_PCT: AtomicI32 = AtomicI32::new(35);

fn sub_search(tt: &Arc<TranspositionTable>, board: &Board, hist: &[u64], sac: SacParams, budget: Duration, root_only: Vec<Move>, max_depth: i32) -> Option<(i32, i32, Vec<Move>)> {
    if budget < Duration::from_millis(10) {
        return None;
    }
    let stop = Arc::new(AtomicBool::new(false));
    let mut t = Searcher::new(Arc::clone(tt), Arc::clone(&stop), Arc::new(AtomicU64::new(0)), true);
    t.sac = sac;
    t.hashes = hist.to_vec();
    t.root_color = board.side_to_move();
    t.start = Instant::now();
    t.hard_limit = Some(budget);
    t.root_only = root_only;
    let mut sc = 0;
    let mut last = None;
    for depth in 1..=max_depth {
        t.root_depth = depth;
        TAL_MODE.store(
            MATE_NET.load(Ordering::Relaxed) && depth > 8 && sc < TAL_MODE_BELOW.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        sc = aspiration(&mut t, board, depth, sc);
        if stop.load(Ordering::Relaxed) {
            break;
        }
        last = Some((sc, depth, t.root_pv()));
        if sc.abs() >= MATE_BOUND {
            break;
        }
    }
    TAL_MODE.store(false, Ordering::Relaxed);
    last
}

fn prime_forcing_lines(board: &Board, hist: &[u64], tt: &Arc<TranspositionTable>, sac: SacParams, budget: Duration, print_info: bool) -> Option<i32> {
    let start = Instant::now();
    let left = |start: Instant| budget.saturating_sub(start.elapsed());
    // 1. are we in trouble?
    let quick = match sub_search(tt, board, hist, sac, budget / 8, Vec::new(), 12) {
        Some(q) => q,
        None => return None,
    };
    if quick.0 >= TAL_MODE_BELOW.load(Ordering::Relaxed) || quick.0.abs() >= MATE_BOUND {
        return Some(quick.0);
    }
    // 2. our checks, the most forcing first
    let mut checks: Vec<(usize, Move)> = Vec::new();
    board.generate_moves(|mvs| {
        for m in mvs {
            let mut c = board.clone();
            c.play_unchecked(m);
            if !c.checkers().is_empty() {
                let mut n = 0;
                c.generate_moves(|r| {
                    n += r.len();
                    false
                });
                checks.push((n, m));
            }
        }
        false
    });
    checks.sort_by_key(|x| x.0);
    // 3. follow each check's main line; collect our positions with a checking sacrifice
    let mut targets: Vec<(Board, Vec<u64>)> = Vec::new();
    for &(_, c) in checks.iter().take(3) {
        let line = match sub_search(tt, board, hist, sac, budget / 16, vec![c], 12) {
            Some(l) => l.2,
            None => continue,
        };
        let mut b = board.clone();
        let mut h = hist.to_vec();
        for (i, &m) in line.iter().enumerate().take(4) {
            h.push(b.hash());
            b.play_unchecked(m);
            if i % 2 == 1 && !tal_candidates(&b, &{
                let mut v = Vec::new();
                b.generate_moves(|mvs| {
                    v.extend(mvs);
                    false
                });
                v
            })
            .is_empty()
                && !targets.iter().any(|t| t.0.hash() == b.hash())
            {
                targets.push((b.clone(), h.clone()));
            }
        }
    }
    targets.truncate(2);
    // 4. search them on their own, into the shared table
    let n = targets.len() as u32;
    for (i, (tb, th)) in targets.iter().enumerate() {
        let each = left(start) / (n - i as u32);
        let r = sub_search(tt, tb, th, sac_params_for(tb), each, Vec::new(), MAX_PLY as i32 - 8);
        if print_info {
            if let Some((sc, d, pv)) = r {
                out!("info string forcing line: {} scores {} at depth {}", pv_string(tb, &pv), sc, d);
            }
        }
    }
    Some(quick.0)
}

/// Rescue candidates (lost endgames): root moves that give material back, a piece thrown in or
/// an exchange sacrificed, the way studies save hopeless positions (a stalemate box, a
/// fortress). The smallest sacrifices first.
fn rescue_candidates(board: &Board, root_moves: &[Move]) -> Vec<Move> {
    let mut out: Vec<(i32, Move)> = Vec::new();
    for &m in root_moves {
        match board.piece_on(m.from) {
            Some(Piece::King) | Some(Piece::Pawn) | None => continue,
            _ => {}
        }
        if m.promotion.is_some() || see_ge(board, m, -150) {
            continue;
        }
        out.push((see(board, m), m));
    }
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out.into_iter().take(RESCUE_MAX.load(Ordering::Relaxed).max(1) as usize).map(|x| x.1).collect()
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
    LAST_SWINDLE.store(false, Ordering::Relaxed);
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

    // deep mode: a long think (or open-ended analysis)
    let deep_mode = match limits.hard {
        Some(h) => h >= Duration::from_millis(DEEP_MIN_MS.load(Ordering::Relaxed).max(0) as u64),
        None => limits.depth.is_none() && limits.nodes.is_none(),
    };
    DEEP.store(deep_mode, Ordering::Relaxed);
    TAL_MODE.store(false, Ordering::Relaxed);

    let max_depth = limits.depth.unwrap_or(MAX_PLY as i32 - 8).clamp(1, MAX_PLY as i32 - 8);
    let nodes = Arc::new(AtomicU64::new(0));
    let sac = sac_params_for(board);

    // Tal's look: big sacrifices against the king (a piece or more, with a real attack behind
    // them) are searched again on their own at the end, with part of the time. Inside the full
    // search they are only ever tested against a window set by the other moves, and that
    // window decides what gets pruned below them, so a sacrifice that needs depth is missed.
    let tal_cands = if TAL_LOOK.load(Ordering::Relaxed)
        && deep_mode
        && sac.bonus > 0
        && skill >= 20
        && limits.hard.map_or(false, |h| h >= Duration::from_millis(TAL_LOOK_MIN_MS.load(Ordering::Relaxed) as u64))
        && limits.nodes.is_none()
        && limits.depth.is_none()
        && ROOT_FILTER.lock().unwrap().is_empty()
    {
        tal_candidates(board, &root_moves)
    } else {
        Vec::new()
    };
    let full_hard = limits.hard;
    let look_pct = TAL_LOOK_PCT.load(Ordering::Relaxed).clamp(1, 50);
    if !tal_cands.is_empty() {
        let keep = (100 - look_pct) as u32;
        limits.hard = limits.hard.map(|h| h * keep / 100);
        limits.soft = limits.soft.map(|h| h * keep / 100);
    }

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
            s.sac = sac;
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
    s.sac = sac;
    if print_info && (sac.bonus != SACRIFICE_BONUS.load(Ordering::Relaxed) || sac.any) {
        out!("info string sacrifice appetite {} (no sacrifice yet this game)", sac.bonus);
    }
    s.hashes = hist;
    s.root_color = board.side_to_move();
    s.start = start;
    s.hard_limit = limits.hard;
    s.node_limit = limits.nodes;

    // ---- Tal's forcing lines (deep mode, in trouble) ----
    let mut quick_score: Option<i32> = None;
    if deep_mode && PRIME.load(Ordering::Relaxed) && skill >= 20 && ROOT_FILTER.lock().unwrap().is_empty() {
        if let Some(h) = full_hard {
            quick_score = prime_forcing_lines(board, &s.hashes, &tt, sac, h * PRIME_PCT.load(Ordering::Relaxed).clamp(0, 60) as u32 / 100, print_info);
        }
    }
    // ---- rescue look: lost endgames keep time back for giving material back ----
    let rescue_cands: Vec<Move> = match quick_score {
        Some(q) if RESCUE.load(Ordering::Relaxed)
            && q <= -RESCUE_BELOW.load(Ordering::Relaxed)
            && q > -MATE_BOUND
            && board.pieces(Piece::Queen).is_empty()
            && full_hard.is_some() =>
        {
            rescue_candidates(board, &root_moves)
        }
        _ => Vec::new(),
    };
    if print_info && std::env::var("AGGRO_LOOKDBG").is_ok() {
        out!("info string quick {:?} rescue candidates {}", quick_score, rescue_cands.len());
    }
    if !rescue_cands.is_empty() {
        let keep = (100 - RESCUE_PCT.load(Ordering::Relaxed).clamp(10, 80)) as u32;
        s.hard_limit = s.hard_limit.map(|h| h * keep / 100);
        limits.soft = limits.soft.map(|h| h * keep / 100);
    }

    let mut best_move = Some(ROOT_FILTER.lock().unwrap().first().copied().unwrap_or(root_moves[0]));
    let mut best_score = 0;
    let mut prev_best: Option<Move> = None;
    let mut stability = 0;
    let mut prev_score = 0;
    let mut done_depth = 0;

    TAL_MODE.store(false, Ordering::Relaxed);
    for depth in 1..=max_depth {
        s.root_depth = depth;
        s.seldepth = 0;
        TAL_MODE.store(
            MATE_NET.load(Ordering::Relaxed) && deep() && depth > 8 && best_score < TAL_MODE_BELOW.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
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
        done_depth = depth;

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

    // ---- rescue look ----
    if !rescue_cands.is_empty() && best_score.abs() < MATE_BOUND {
        let left = full_hard.unwrap().saturating_sub(start.elapsed());
        let cands: Vec<Move> = rescue_cands.iter().copied().filter(|&m| Some(m) != best_move).collect();
        let n = cands.len() as u32;
        for (i, &c) in cands.iter().enumerate() {
            let each = full_hard.unwrap().saturating_sub(start.elapsed()) / (n - i as u32);
            let _ = left;
            let rtt = Arc::new(TranspositionTable::new(LOOK_TT_MB.load(Ordering::Relaxed).max(1) as usize));
            if let Some((sc, d, pv)) = sub_search(&rtt, board, &s.hashes, sac, each, vec![c], MAX_PLY as i32 - 8) {
                if print_info {
                    out!("info string rescue look: {} scores {} at depth {}", pv_string(board, &pv), sc, d);
                }
                if sc > best_score + 30 && sc.abs() < MATE_BOUND || sc >= MATE_BOUND {
                    best_move = Some(c);
                    best_score = sc;
                    if print_info {
                        out!("info depth {} score {} pv {}", d, format_score(sc), pv_string(board, &pv));
                    }
                }
            }
        }
    }

    // ---- Tal's look ----
    if !tal_cands.is_empty() && best_score.abs() < MATE_BOUND {
        let used = start.elapsed();
        let pct = look_pct as u32;
        let mut budget = used * pct / (100 - pct);
        if let Some(h) = full_hard {
            budget = budget.min(h.saturating_sub(used));
        }
        let cands: Vec<Move> = tal_cands.iter().copied().filter(|&m| Some(m) != best_move).collect();
        if !cands.is_empty() && budget >= Duration::from_millis(20) {
            // the look is a plain search of the sacrifice: Tal mode's extra depth on threats
            // only slows a single-move search down
            TAL_MODE.store(LOOK_TAL.load(Ordering::Relaxed), Ordering::Relaxed);
            let look_dbg = std::env::var("AGGRO_LOOKDBG").is_ok();
            let look_start = Instant::now();
            // A quick look at each candidate first: one that is hopeless even then (far below
            // the best move) is dropped, so the real look goes to the others.
            let cands: Vec<Move> = if cands.len() > 1 {
                let quick = budget / 12;
                cands
                    .iter()
                    .copied()
                    .filter(|&c| {
                        let qtt = Arc::new(TranspositionTable::new(8));
                        match sub_search(&qtt, board, &s.hashes, sac, quick, vec![c], 10) {
                            Some((sc, d, _)) => {
                                if look_dbg {
                                    out!("info string look quick {} d{} {}", cozy_chess::util::display_uci_move(board, c), d, sc);
                                }
                                sc >= best_score - 900 || sc.abs() >= MATE_BOUND
                            }
                            None => true,
                        }
                    })
                    .collect()
            } else {
                cands
            };
            let budget = budget.saturating_sub(look_start.elapsed());
            let look_start = Instant::now();
            for (ci, &cand) in cands.iter().enumerate() {
                // what is left of the look's time, shared by the candidates still to come
                // (a hopeless candidate given up early leaves its time to the next one)
                let each = budget.saturating_sub(look_start.elapsed()) / (cands.len() - ci) as u32;
                let look_stop = Arc::new(AtomicBool::new(false));
                let look_tt = Arc::new(TranspositionTable::new(LOOK_TT_MB.load(Ordering::Relaxed) as usize));
                let cand_start = Instant::now();
                // The sacrifice's own follow-up sacrifice (Qf6+ Bxf6 gxf6+ Kg8, and now
                // Rxh7!): a short search finds the main line, and our next position on it with
                // a sacrifice of its own is searched first, into the look's table.
                if LOOK_PRIME.load(Ordering::Relaxed) && each >= Duration::from_millis(500) {
                    if let Some((_, _, line)) = sub_search(&look_tt, board, &s.hashes, sac, each / 10, vec![cand], 12) {
                        // the deepest such position on the line (two or four plies on)
                        let mut b = board.clone();
                        let mut h = s.hashes.clone();
                        let mut target = None;
                        for (i, &m) in line.iter().enumerate().take(4) {
                            h.push(b.hash());
                            b.play_unchecked(m);
                            if i % 2 == 1 && !tal_candidates_ex(&b, &all_moves(&b), true).is_empty() {
                                target = Some((b.clone(), h.clone()));
                            }
                        }
                        if let Some((b, h)) = target {
                            let r = sub_search(&look_tt, &b, &h, sac_params_for(&b), each * LOOK_PRIME_PCT.load(Ordering::Relaxed).clamp(0, 90) as u32 / 100, Vec::new(), MAX_PLY as i32 - 8);
                            if look_dbg {
                                if let Some((sc, d, pv)) = r {
                                    out!("info string look prime {} d{} {}", pv_string(&b, &pv), d, sc);
                                }
                            }
                        }
                    }
                }
                let each = each.saturating_sub(cand_start.elapsed());
                let mut t = Searcher::new(Arc::clone(&look_tt), Arc::clone(&look_stop), Arc::new(AtomicU64::new(0)), true);
                t.sac = sac;
                t.hashes = s.hashes.clone();
                t.root_color = board.side_to_move();
                t.start = Instant::now();
                t.hard_limit = Some(each);
                t.root_only = vec![cand];
                let mut sc = 0;
                let mut last = None;
                for depth in 1..=max_depth {
                    t.root_depth = depth;
                    sc = aspiration(&mut t, board, depth, sc);
                    if look_stop.load(Ordering::Relaxed) {
                        break;
                    }
                    last = Some((sc, depth, t.root_pv()));
                    // hopeless: far below the best move even after a real search
                    if depth >= 12 && sc < best_score - 900 && sc.abs() < MATE_BOUND && ci + 1 < cands.len() {
                        break;
                    }
                    if look_dbg {
                        out!("info string look {} d{} {} t{} pv {}", cozy_chess::util::display_uci_move(board, cand), depth, sc, start.elapsed().as_millis(), pv_string(board, &t.root_pv()));
                    }
                }
                if let Some((sc, d, pv)) = last {
                    if print_info {
                        out!("info string Tal's look: {} scores {} at depth {}", cozy_chess::util::display_uci_move(board, cand), sc, d);
                    }
                    if sc > best_score + 30 && d >= 10 && sc.abs() < MATE_BOUND || (sc >= MATE_BOUND && sc > best_score) {
                        best_move = Some(cand);
                        best_score = sc;
                        if print_info {
                            out!("info depth {} score {} pv {}", d, format_score(sc), pv_string(board, &pv));
                        }
                    }
                }
            }
        }
    }

    if std::env::var("AGGRO_LOOKDBG").is_ok() && print_info {
        out!("info string mate probes {}", MATE_PROBES.swap(0, Ordering::Relaxed));
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
    // ---- swindle when worse ----
    // When AggroChess is clearly worse, it doesn't just play the objectively "least bad" move:
    // among moves that are nearly as good, it picks the one that sets the most traps, i.e. where
    // the most of the opponent's natural replies (captures and checks count extra) would throw
    // their advantage away.
    if SWINDLE.load(Ordering::Relaxed)
        && s.node_limit.is_none()
        && best_score <= -SWINDLE_MIN
        && best_score > -SWINDLE_MAX
        && root_moves.len() >= 2
        && done_depth >= 5
    {
        if let (Some(bm), Some(soft), Some(hard)) = (best_move, limits.soft, limits.hard) {
            let left = hard.saturating_sub(start.elapsed());
            let budget = (soft / 2).min(left * 3 / 5).min(Duration::from_millis(2000));
            if budget >= Duration::from_millis(60) {
                if let Some((m, rate, best_rate)) = swindle_pick(&mut s, board, &root_moves, bm, best_score, done_depth, budget, start) {
                    if print_info {
                        out!(
                            "info string swindle {} (traps {:.0}%) instead of {} ({:.0}%)",
                            cozy_chess::util::display_uci_move(board, m),
                            rate * 100.0,
                            cozy_chess::util::display_uci_move(board, bm),
                            best_rate * 100.0
                        );
                    }
                    best_move = Some(m);
                    LAST_SWINDLE.store(true, Ordering::Relaxed);
                }
            }
        }
    }
    LAST_SCORE.store(best_score, Ordering::Relaxed);
    LAST_OBJ.store(if best_score.abs() < MATE_BOUND { best_score - s.root_best_sb } else { best_score }, Ordering::Relaxed);
    (best_move, best_score)
}

/// Look for a trickier move than `bm` when worse. Returns (move, its trap rate, bm's trap rate).
#[allow(clippy::too_many_arguments)]
fn swindle_pick(
    s: &mut Searcher,
    board: &Board,
    root_moves: &[Move],
    bm: Move,
    best_score: i32,
    done_depth: i32,
    budget: Duration,
    start: Instant,
) -> Option<(Move, f64, f64)> {
    let behind = -best_score;
    // how much worse than the best move a candidate may be (more slack when the game is lost anyway)
    let margin = (30 + behind / 8).min(150);
    let t_cand = best_score - margin;
    // a reply "falls for it" if it hands back at least half the deficit (and at least 1.5 pawns)
    let gain = (behind / 2).max(150);
    let stm = board.side_to_move();
    let deadline = start.elapsed() + budget;
    s.stop = Arc::new(AtomicBool::new(false));

    // 1. candidates: the best move plus up to 5 others that are nearly as good (forcing moves first)
    let mut others: Vec<Move> = root_moves.iter().copied().filter(|&m| m != bm).collect();
    others.sort_by_key(|&m| {
        let mut c = board.clone();
        c.play_unchecked(m);
        let check = !c.checkers().is_empty();
        -(check as i32 * 2 + is_capture(board, m) as i32)
    });
    let dc = (done_depth - 3).clamp(2, 10);
    s.hard_limit = Some(start.elapsed() + budget * 2 / 5);
    let mut cands = vec![bm];
    for &m in &others {
        if cands.len() >= 6 {
            break;
        }
        let piece = board.piece_on(m.from).unwrap();
        let mut child = board.clone();
        child.play_unchecked(m);
        s.hashes.push(board.hash());
        s.stack[0].mv = Some(m);
        s.stack[0].cont = pc12(stm, piece) * 64 + m.to as usize;
        s.root_depth = dc + 1;
        let opp = s.search(&child, dc, -t_cand, -t_cand + 1, 1, false);
        s.hashes.pop();
        if s.stop.load(Ordering::Relaxed) {
            break;
        }
        if -opp >= t_cand {
            cands.push(m);
        }
    }
    if cands.len() < 2 {
        return None;
    }

    // 2. trap rate of each candidate
    s.stop.store(false, Ordering::Relaxed);
    s.hard_limit = Some(deadline);
    let dr = (done_depth / 2).clamp(2, 6);
    let mut rates: Vec<(Move, f64)> = Vec::new();
    'cand: for &m in &cands {
        let piece = board.piece_on(m.from).unwrap();
        let mut child = board.clone();
        child.play_unchecked(m);
        let mut replies = Vec::new();
        child.generate_moves(|mvs| {
            replies.extend(mvs);
            false
        });
        if replies.is_empty() {
            continue;
        }
        // the score after the opponent's best reply, at the same depth the replies are checked;
        // a reply falls for the trap if it does much worse for them than that
        s.hashes.push(board.hash());
        s.stack[0].mv = Some(m);
        s.stack[0].cont = pc12(stm, piece) * 64 + m.to as usize;
        s.root_depth = dr + 2;
        let base = -s.search(&child, dr + 1, -INF, INF, 1, false);
        s.hashes.pop();
        if s.stop.load(Ordering::Relaxed) {
            break 'cand;
        }
        let t_trap = base + gain;
        let (mut fall, mut total) = (0.0f64, 0.0f64);
        for &r in &replies {
            let mut gc = child.clone();
            gc.play_unchecked(r);
            let natural = if !gc.checkers().is_empty() {
                2.0
            } else if is_capture(&child, r) {
                3.0
            } else {
                1.0
            };
            total += natural;
            s.hashes.push(board.hash());
            s.hashes.push(child.hash());
            s.stack[0].mv = Some(m);
            s.stack[0].cont = pc12(stm, piece) * 64 + m.to as usize;
            let rp = child.piece_on(r.from).unwrap();
            s.stack[1].mv = Some(r);
            s.stack[1].cont = pc12(!stm, rp) * 64 + r.to as usize;
            s.root_depth = dr + 2;
            let v = s.search(&gc, dr, t_trap - 1, t_trap, 2, false);
            s.hashes.pop();
            s.hashes.pop();
            if s.stop.load(Ordering::Relaxed) {
                break 'cand;
            }
            if v >= t_trap {
                fall += natural;
            }
        }
        rates.push((m, fall / total));
    }
    let best_rate = rates.iter().find(|(m, _)| *m == bm).map(|x| x.1)?;
    let (m, rate) = rates.iter().copied().filter(|(m, _)| *m != bm).fold(None, |acc: Option<(Move, f64)>, x| match acc {
        Some(a) if a.1 >= x.1 => Some(a),
        _ => Some(x),
    })?;
    if rate >= 0.2 && rate >= best_rate + 0.15 {
        Some((m, rate, best_rate))
    } else {
        None
    }
}

/// Play for traps when clearly worse (between these scores, in centipawns). Analysis turns it off.
pub static SWINDLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
/// Whether the last search picked a swindle move.
pub static LAST_SWINDLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
const SWINDLE_MIN: i32 = 150;
const SWINDLE_MAX: i32 = 2500;

/// Prefer sacrificial mates over the fastest one ("finish beautifully"). Analysis turns it off.
pub static BEAUTIFUL_MATES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// A self-contained node-limited search for data generation: its own TT, one thread, no
/// personality tricks at the root beyond what the global settings say. Iterates until
/// `soft_nodes` have been searched (finishing the iteration in progress, hard-capped at
/// `hard_nodes`). Returns (best move, score from the side to move's view).
pub struct NodeSearcher {
    s: Searcher,
}

impl NodeSearcher {
    pub fn new(tt_mb: usize) -> Self {
        let tt = Arc::new(TranspositionTable::new(tt_mb));
        let s = Searcher::new(tt, Arc::new(AtomicBool::new(false)), Arc::new(AtomicU64::new(0)), true);
        Self { s }
    }

    pub fn new_game(&mut self) {
        self.s.tt.clear();
        self.s.history = Box::new([[[0; 64]; 64]; 2]);
        self.s.cont_hist.iter_mut().for_each(|v| *v = 0);
        self.s.capt_hist = Box::new([[[0; 6]; 64]; 12]);
        self.s.counter = Box::new([[None; 64]; 12]);
    }

    pub fn search(&mut self, board: &Board, history: &[u64], soft_nodes: u64, hard_nodes: u64) -> (Option<Move>, i32) {
        let s = &mut self.s;
        s.tt.new_search();
        s.stop.store(false, Ordering::Relaxed);
        s.shared_nodes.store(0, Ordering::Relaxed);
        s.nodes = 0;
        s.hashes.clear();
        s.hashes.extend_from_slice(history);
        if s.hashes.last() == Some(&board.hash()) {
            s.hashes.pop();
        }
        s.root_color = board.side_to_move();
        s.start = Instant::now();
        s.hard_limit = None;
        s.node_limit = Some(hard_nodes);
        s.killers = [[None; 2]; MAX_PLY + 2];
        let mut best_move = None;
        let mut best_score = 0;
        for depth in 1..=(MAX_PLY as i32 - 8) {
            s.root_depth = depth;
            s.seldepth = 0;
            let score = aspiration(s, board, depth, best_score);
            if s.stop.load(Ordering::Relaxed) && depth > 1 {
                break;
            }
            if let Some(&m) = s.root_pv().first() {
                best_move = Some(m);
            }
            best_score = score;
            if s.nodes >= soft_nodes || best_score.abs() >= MATE_BOUND {
                break;
            }
        }
        if best_move.is_none() {
            board.generate_moves(|mvs| {
                best_move = mvs.into_iter().next();
                true
            });
        }
        (best_move, best_score)
    }
}

fn aspiration(s: &mut Searcher, board: &Board, depth: i32, prev: i32) -> i32 {
    if let Ok(v) = std::env::var("AGGRO_DBGWIN") {
        let mut it = v.split(',').map(|x| x.parse::<i32>().unwrap());
        let lo = it.next().unwrap();
        let hi = it.next().unwrap_or(lo + 1);
        return s.search(board, depth, lo, hi, 0, false);
    }
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
