//! AggroChess NNUE: (768 -> 256) x 2 -> SCReLU -> 1 with 8 output buckets by piece count.
//!
//! The network is trained only on AggroChess's own self-play games (see `datagen.rs`), so it
//! is AggroChess's judgement, learned. It gives the *objective* part of the evaluation; the
//! personality (extra king danger against the opponent, trade aversion and so on) is still
//! added on top in `eval.rs`, and the sacrifice bonus and contempt still live in the search.
//!
//! Accumulators are updated incrementally as the search makes moves (`Accumulator::apply`),
//! and refreshed from scratch whenever the search enters a position it has no accumulator for.

use cozy_chess::{Board, Color, Move, Piece, Square};
use std::sync::OnceLock;

pub const H: usize = 256;
const INPUTS: usize = 768;
const BUCKETS: usize = 8;
const SCALE: i32 = 400;
const QA: i32 = 255;
const QB: i32 = 64;

static NET_BYTES: &[u8] = include_bytes!("../aggro.nnue");

/// Output scale in percent. The net's evaluations come out larger than the classical
/// evaluation's (it learned from game results too), so they are scaled down to the classical
/// scale that the search margins and the personality layer were tuned for.
pub static OUT_SCALE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(67);

pub struct Net {
    ft_w: Vec<i16>,
    ft_b: Vec<i16>,
    out_w: Vec<i16>,
    out_b: Vec<i32>,
}

static NET: OnceLock<Net> = OnceLock::new();

fn parse(v: &[u8]) -> Net {
    assert_eq!(&v[0..4], b"AGNN", "bad network file");
    let h = u32::from_le_bytes(v[4..8].try_into().unwrap()) as usize;
    assert_eq!(h, H, "network has a different hidden size");
    let mut off = 8;
    let mut rd = |n: usize| -> Vec<i16> {
        let r = (0..n).map(|i| i16::from_le_bytes([v[off + 2 * i], v[off + 2 * i + 1]])).collect();
        off += 2 * n;
        r
    };
    let ft_w = rd(INPUTS * H);
    let ft_b = rd(H);
    let out_w = rd(BUCKETS * 2 * H);
    let out_b = (0..BUCKETS).map(|i| i32::from_le_bytes(v[off + 4 * i..off + 4 * i + 4].try_into().unwrap())).collect();
    Net { ft_w, ft_b, out_w, out_b }
}

#[inline]
pub fn net() -> &'static Net {
    NET.get_or_init(|| parse(NET_BYTES))
}

#[inline(always)]
fn feature(persp: usize, color: usize, piece: usize, sq: usize) -> usize {
    let s = if persp == 0 { sq } else { sq ^ 56 };
    (if color == persp { 0 } else { 384 }) + piece * 64 + s
}

#[derive(Clone)]
#[repr(C, align(64))]
pub struct Accumulator {
    /// [white perspective, black perspective]
    pub v: [[i16; H]; 2],
}

impl Default for Accumulator {
    fn default() -> Self {
        Accumulator { v: [[0; H]; 2] }
    }
}

#[inline(always)]
fn add(acc: &mut [i16; H], w: &[i16]) {
    for (a, b) in acc.iter_mut().zip(w) {
        *a = a.wrapping_add(*b);
    }
}

#[inline(always)]
fn sub(acc: &mut [i16; H], w: &[i16]) {
    for (a, b) in acc.iter_mut().zip(w) {
        *a = a.wrapping_sub(*b);
    }
}

impl Accumulator {
    pub fn refresh(&mut self, board: &Board) {
        let n = net();
        for persp in 0..2 {
            self.v[persp].copy_from_slice(&n.ft_b);
        }
        for color in [Color::White, Color::Black] {
            for piece in Piece::ALL {
                for sq in board.colored_pieces(color, piece) {
                    for persp in 0..2 {
                        let f = feature(persp, color as usize, piece as usize, sq as usize);
                        add(&mut self.v[persp], &n.ft_w[f * H..f * H + H]);
                    }
                }
            }
        }
    }

    #[inline(always)]
    fn toggle(&mut self, on: bool, color: Color, piece: Piece, sq: Square) {
        let n = net();
        for persp in 0..2 {
            let f = feature(persp, color as usize, piece as usize, sq as usize);
            let w = &n.ft_w[f * H..f * H + H];
            if on {
                add(&mut self.v[persp], w);
            } else {
                sub(&mut self.v[persp], w);
            }
        }
    }

    /// Update `self` (a copy of the parent's accumulator) for `mv` played on `board` (the
    /// position *before* the move).
    pub fn apply(&mut self, board: &Board, mv: Move) {
        let us = board.side_to_move();
        let piece = match board.piece_on(mv.from) {
            Some(p) => p,
            None => return,
        };
        // castling: cozy-chess encodes it as the king capturing its own rook
        if piece == Piece::King && board.colors(us).has(mv.to) {
            let rank = mv.from.rank();
            let short = mv.to.file() > mv.from.file();
            let (kf, rf) = if short {
                (cozy_chess::File::G, cozy_chess::File::F)
            } else {
                (cozy_chess::File::C, cozy_chess::File::D)
            };
            self.toggle(false, us, Piece::King, mv.from);
            self.toggle(false, us, Piece::Rook, mv.to);
            self.toggle(true, us, Piece::King, Square::new(kf, rank));
            self.toggle(true, us, Piece::Rook, Square::new(rf, rank));
            return;
        }
        self.toggle(false, us, piece, mv.from);
        if let Some(cap) = board.piece_on(mv.to) {
            self.toggle(false, !us, cap, mv.to);
        } else if piece == Piece::Pawn && mv.from.file() != mv.to.file() {
            // en passant
            self.toggle(false, !us, Piece::Pawn, Square::new(mv.to.file(), mv.from.rank()));
        }
        self.toggle(true, us, mv.promotion.unwrap_or(piece), mv.to);
    }

    /// Evaluation from the side to move's point of view, centipawns.
    pub fn evaluate(&self, board: &Board) -> i32 {
        let n = net();
        let stm = board.side_to_move() as usize;
        let count = board.occupied().len() as usize;
        let bucket = ((count.max(2) - 2) / 4).min(BUCKETS - 1);
        let ow = &n.out_w[bucket * 2 * H..(bucket + 1) * 2 * H];
        let sum = screlu_dot(&self.v[stm], &ow[..H]) + screlu_dot(&self.v[1 - stm], &ow[H..]);
        let y = sum / QA as i64 + n.out_b[bucket] as i64;
        let scale = OUT_SCALE.load(std::sync::atomic::Ordering::Relaxed) as i64;
        (y * SCALE as i64 * scale / (100 * (QA * QB) as i64)) as i32
    }
}

#[inline(always)]
fn screlu_dot(acc: &[i16; H], w: &[i16]) -> i64 {
    // (v * w) fits in i16 because |w| <= 127 and v <= 255; multiply by v again in i32
    let mut total: i64 = 0;
    for (ca, cw) in acc.chunks_exact(32).zip(w.chunks_exact(32)) {
        let mut s: i32 = 0;
        for (a, b) in ca.iter().zip(cw) {
            let v = (*a).clamp(0, QA as i16);
            let vw = v.wrapping_mul(*b);
            s += vw as i32 * v as i32;
        }
        total += s as i64;
    }
    total
}

/// One-off evaluation (refreshes an accumulator from scratch).
pub fn evaluate_fresh(board: &Board) -> i32 {
    let mut a = Accumulator::default();
    a.refresh(board);
    a.evaluate(board)
}
