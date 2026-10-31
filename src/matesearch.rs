//! Threat-space mate search ("Tal's eye for a mating net").
//!
//! A separate, narrow search that only looks for forced mates. The attacker may only play
//! forcing moves: checks, promotions, and moves that threaten mate in one; when in check,
//! any evasion. The defender may play anything. Because every attacking move either checks
//! or threatens mate, most defending moves are refuted at once by the mate in one, so this
//! search reaches mates far deeper than the normal alpha-beta search, which has to weigh
//! every quiet move and prunes the quiet mating threats it doesn't understand. That is how a
//! human sees Plaskett's Puzzle: as a chain of checks and mate threats, not as material.
//!
//! It only ever claims a mate it has proven move by move, so it cannot make the engine
//! play a losing move: a found mate is a real mate.

use cozy_chess::{Board, Move, Piece};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

const TT_BITS: u32 = 20;

#[derive(Clone, Copy, Default)]
struct Entry {
    key: u64,
    /// attacker to move: proven mate within this many attacker moves (0 = unknown)
    proven: u8,
    /// attacker to move: proven there is no forcing mate within this many moves
    disproven: u8,
    /// attacker to move: the move that mates (from, to, promotion packed), for ordering
    best: u16,
}

pub struct MateSearch<'a> {
    tt: Vec<Entry>,
    nodes: u64,
    deadline: Instant,
    stop: &'a AtomicBool,
    aborted: bool,
    node_limit: u64,
    /// win mode: material lead (cp) that counts as a won position, besides mate (0 = mate only)
    pub goal: i32,
    pub threat_depth: u8,
    /// threats found by a lighter search (no threats within threats): faster, finds less
    pub lite_threats: bool,
}

fn pack(m: Move) -> u16 {
    (m.from as u16) | ((m.to as u16) << 6) | ((m.promotion.map(|p| p as u16 + 1).unwrap_or(0)) << 12)
}

fn legal_moves(b: &Board) -> Vec<Move> {
    let mut v = Vec::with_capacity(48);
    b.generate_moves(|mvs| {
        v.extend(mvs);
        false
    });
    v
}

fn has_legal_move(b: &Board) -> bool {
    let mut any = false;
    b.generate_moves(|_| {
        any = true;
        true
    });
    any
}

fn is_mate(b: &Board) -> bool {
    !b.checkers().is_empty() && !has_legal_move(b)
}

fn attacks(p: Piece, c: cozy_chess::Color, sq: cozy_chess::Square, occ: cozy_chess::BitBoard) -> cozy_chess::BitBoard {
    use cozy_chess::*;
    match p {
        Piece::Pawn => get_pawn_attacks(sq, c),
        Piece::Knight => get_knight_moves(sq),
        Piece::Bishop => get_bishop_moves(sq, occ),
        Piece::Rook => get_rook_moves(sq, occ),
        Piece::Queen => get_bishop_moves(sq, occ) | get_rook_moves(sq, occ),
        Piece::King => get_king_moves(sq),
    }
}

/// Could the piece that just moved to `to` give check with its next move? (It attacks a
/// square from which it would check the enemy king.) Quiet moves that cannot are almost
/// never mate threats, and testing every quiet move for a threat is the costly part.
fn could_check_next(c: &Board, to: cozy_chess::Square) -> bool {
    let p = match c.piece_on(to) {
        Some(p) => p,
        None => return false,
    };
    let me = !c.side_to_move();
    let k = c.king(c.side_to_move());
    let occ = c.occupied();
    match p {
        Piece::King => {
            let (df, dr) = ((k.file() as i32 - to.file() as i32).abs(), (k.rank() as i32 - to.rank() as i32).abs());
            df.max(dr) <= 2
        }
        Piece::Pawn => {
            let (df, dr) = ((k.file() as i32 - to.file() as i32).abs(), (k.rank() as i32 - to.rank() as i32).abs());
            df <= 2 && dr <= 3
        }
        _ => {
            let reach = attacks(p, me, to, occ) & !c.colors(me);
            // squares from which this piece would attack the king (symmetric for pieces)
            let checks = attacks(p, me, k, occ);
            !(reach & checks).is_empty()
        }
    }
}

/// Does the side to move have a mate in one?
fn mate_in_one(b: &Board) -> Option<Move> {
    let mut found = None;
    b.generate_moves(|mvs| {
        for m in mvs {
            let mut c = b.clone();
            c.play_unchecked(m);
            if !c.checkers().is_empty() && !has_legal_move(&c) {
                found = Some(m);
                return true;
            }
        }
        false
    });
    found
}

impl<'a> MateSearch<'a> {
    pub fn new(deadline: Instant, stop: &'a AtomicBool) -> Self {
        MateSearch { tt: vec![Entry::default(); 1 << TT_BITS], nodes: 0, deadline, stop, aborted: false, node_limit: u64::MAX, goal: 0, threat_depth: 2, lite_threats: false }
    }

    fn idx(&self, key: u64) -> usize {
        (key >> (64 - TT_BITS)) as usize
    }

    fn probe(&self, key: u64) -> Option<Entry> {
        let e = self.tt[self.idx(key)];
        if e.key == key { Some(e) } else { None }
    }

    fn store(&mut self, key: u64, proven: u8, disproven: u8, best: u16) {
        let i = self.idx(key);
        let e = &mut self.tt[i];
        if e.key != key {
            *e = Entry { key, proven, disproven, best };
        } else {
            if proven > 0 && (e.proven == 0 || proven < e.proven) {
                e.proven = proven;
                e.best = best;
            }
            if disproven > e.disproven {
                e.disproven = disproven;
            }
        }
    }

    fn tick(&mut self) -> bool {
        self.nodes += 1;
        if self.nodes >= self.node_limit {
            self.aborted = true;
        }
        if self.nodes & 1023 == 0 && (Instant::now() >= self.deadline || self.stop.load(Ordering::Relaxed)) {
            self.aborted = true;
        }
        self.aborted
    }

    /// The attacker's forcing moves, most forcing first (checks, then promotions, then mate
    /// threats). In check, every evasion.
    fn attacker_moves(&mut self, b: &Board, n: u8, hint: u16) -> Vec<Move> {
        let all = legal_moves(b);
        if !b.checkers().is_empty() {
            return all;
        }
        let mut scored: Vec<(i32, Move)> = Vec::new();
        for m in all {
            let mut c = b.clone();
            c.play_unchecked(m);
            let score = if !c.checkers().is_empty() {
                // fewer replies first
                let mut replies = 0;
                c.generate_moves(|mvs| {
                    replies += mvs.len();
                    false
                });
                3000 - replies as i32 * 10
            } else if n < 2 {
                continue; // only a check can mate in one
            } else if m.promotion.is_some() {
                2000 + if m.promotion == Some(Piece::Queen) { 1 } else { 0 }
            } else if self.goal > 0 && b.piece_on(m.to).is_some() {
                // in win mode, captures are forcing too
                800
            } else if !could_check_next(&c, m.to) {
                continue;
            } else if let Some(nb) = c.null_move() {
                // a quiet move must threaten mate: in one, or (with moves to spare) in two
                if mate_in_one(&nb).is_some() {
                    1000
                } else if n >= 3 && self.threat_depth >= 2 && {
                    // a threat of mate in 2..t, found by a lighter search whose quiet moves
                    // must threaten mate in one (no threats within threats)
                    let t = self.threat_depth.min(n - 1);
                    let saved = self.threat_depth;
                    if self.lite_threats {
                        self.threat_depth = 1;
                    }
                    let mut hit = false;
                    for k in 2..=t {
                        if self.attack(&nb, k) {
                            hit = true;
                            break;
                        }
                    }
                    self.threat_depth = saved;
                    hit
                } {
                    500
                } else {
                    continue;
                }
            } else {
                continue;
            };
            let score = if pack(m) == hint { 100_000 } else { score };
            scored.push((score, m));
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.into_iter().map(|x| x.1).collect()
    }

    /// Attacker to move: a mate within `n` moves by checks alone (every attacking move a
    /// check, so the defender only ever has evasions). Cheap; used to spot mate threats.
    fn checks_mate(&mut self, b: &Board, n: u8) -> bool {
        if n == 0 || self.tick() {
            return false;
        }
        let key = b.hash() ^ 0x5bd1_e995_c3a5_c85c;
        if let Some(e) = self.probe(key) {
            if e.proven > 0 && e.proven <= n {
                return true;
            }
            if e.disproven >= n {
                return false;
            }
        }
        let mut checks = Vec::new();
        b.generate_moves(|mvs| {
            for m in mvs {
                let mut c = b.clone();
                c.play_unchecked(m);
                if !c.checkers().is_empty() {
                    checks.push(c);
                }
            }
            false
        });
        let mut found = false;
        for c in checks {
            let mut replies = Vec::new();
            c.generate_moves(|mvs| {
                for m in mvs {
                    let mut d = c.clone();
                    d.play_unchecked(m);
                    replies.push(d);
                }
                false
            });
            if replies.is_empty() {
                found = true;
                break;
            }
            if n > 1 {
                let mut all = true;
                for d in replies {
                    if !self.checks_mate(&d, n - 1) {
                        all = false;
                        break;
                    }
                }
                if all {
                    found = true;
                    break;
                }
            }
            if self.aborted {
                return false;
            }
        }
        if !self.aborted {
            if found { self.store(key, n, 0, 0) } else { self.store(key, 0, n, 0) }
        }
        found
    }

    /// Attacker to move: is there a forced mate within `n` attacker moves?
    fn attack(&mut self, b: &Board, n: u8) -> bool {
        if n == 0 || self.tick() {
            return false;
        }
        let key = b.hash();
        let mut hint = 0;
        if let Some(e) = self.probe(key) {
            if e.proven > 0 && e.proven <= n {
                return true;
            }
            if e.disproven >= n {
                return false;
            }
            hint = e.best;
        }
        for m in self.attacker_moves(b, n, hint) {
            let mut c = b.clone();
            c.play_unchecked(m);
            let mates = if !has_legal_move(&c) {
                !c.checkers().is_empty()
            } else {
                n > 1 && self.defend(&c, n - 1)
            };
            if self.aborted {
                return false;
            }
            if mates {
                self.store(key, n, 0, pack(m));
                return true;
            }
        }
        if !self.aborted {
            self.store(key, 0, n, 0);
        }
        false
    }

    /// Defender to move (and not mated): does every move lose to a mate within `n`?
    fn defend(&mut self, b: &Board, n: u8) -> bool {
        if self.tick() {
            return false;
        }
        let moves = legal_moves(b);
        // try the defences that refuted before first: captures and checks tend to
        let mut ordered: Vec<(i32, Move)> = moves
            .into_iter()
            .map(|m| {
                let cap = b.piece_on(m.to).is_some() as i32;
                (cap, m)
            })
            .collect();
        ordered.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, m) in ordered {
            let mut c = b.clone();
            c.play_unchecked(m);
            if !self.attack(&c, n) {
                return false;
            }
        }
        true
    }

    /// Iterative deepening: the first move of the shortest forcing mate found within
    /// `max_n` moves, with its length, or None.
    pub fn find(&mut self, b: &Board, max_n: u8) -> Option<(Move, u8)> {
        for n in 1..=max_n {
            if self.attack(b, n) {
                let e = self.probe(b.hash())?;
                let mv = legal_moves(b).into_iter().find(|&m| pack(m) == e.best)?;
                return Some((mv, n));
            }
            if self.aborted {
                break;
            }
        }
        None
    }

    pub fn nodes(&self) -> u64 {
        self.nodes
    }

    /// A budgeted probe (for use inside the main search): the table is kept between
    /// probes, so proofs found once help every later probe.
    pub fn probe_mate(&mut self, b: &Board, max_n: u8, budget: u64) -> Option<(Move, u8)> {
        self.aborted = false;
        self.node_limit = self.nodes + budget;
        let r = self.find(b, max_n);
        self.node_limit = u64::MAX;
        self.aborted = false;
        r
    }
}

static NEVER: AtomicBool = AtomicBool::new(false);

/// A mate searcher for the main search's own use: no clock, only node budgets.
pub fn probe_searcher() -> MateSearch<'static> {
    let mut m = MateSearch::new(Instant::now() + std::time::Duration::from_secs(1 << 30), &NEVER);
    m.threat_depth = 3;
    m
}

// ---------------------------------------------------------------------------------------
// Depth-first proof-number search over the same forcing tree. Iterative deepening wastes
// almost all its time disproving the attacker's wrong tries again and again; proof numbers
// steer the search to the defences that are hardest to beat and the attacking moves that
// are closest to a proof, so long mates (Plaskett's Puzzle: mate in 15) come out quickly.

const PN_INF: u32 = 1 << 28;
const DTT_BITS: u32 = 22;
const MAX_PLY: usize = 100;

#[derive(Clone, Copy, Default)]
struct DEntry {
    key: u64,
    hash: u64,
    pn: u32,
    dn: u32,
    or: bool,
}

pub struct Dfpn<'a> {
    ms: MateSearch<'a>,
    dtt: Vec<DEntry>,
    path: Vec<u64>,
    attacker: cozy_chess::Color,
    /// the attacker's forcing moves per position (finding mate threats is the costly part)
    move_cache: Vec<(u64, Vec<Move>)>,
    pub mids: u64,
    pub gens: u64,
}
const MC_BITS: u32 = 18;

impl<'a> Dfpn<'a> {
    pub fn new(deadline: Instant, stop: &'a AtomicBool, threat_depth: u8, attacker: cozy_chess::Color) -> Self {
        let mut ms = MateSearch::new(deadline, stop);
        ms.threat_depth = threat_depth;
        Dfpn { ms, dtt: vec![DEntry::default(); 1 << DTT_BITS], path: Vec::new(), attacker, move_cache: vec![(0, Vec::new()); 1 << MC_BITS], mids: 0, gens: 0 }
    }

    fn didx(key: u64) -> usize {
        (key >> (64 - DTT_BITS)) as usize
    }

    fn dkey(b: &Board, left: u8) -> u64 {
        b.hash() ^ (left as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
    }

    fn lookup(&self, b: &Board, or: bool, left: u8) -> (u32, u32) {
        if or && left == 0 {
            return (PN_INF, 0);
        }
        let key = Self::dkey(b, left);
        if self.path.contains(&b.hash()) {
            // going round in circles proves nothing
            return (PN_INF, 0);
        }
        let i = Self::didx(key);
        let e = if self.dtt[i].key == key { self.dtt[i] } else { self.dtt[i ^ 1] };
        if e.key == key {
            (e.pn, e.dn)
        } else if or {
            if self.goal_met(b) { (0, PN_INF) } else { (1, 1) }
        } else {
            // a defender with few moves is easier to beat
            let mut n = 0u32;
            b.generate_moves(|mvs| {
                n += mvs.len() as u32;
                false
            });
            (n.max(1), 1)
        }
    }

    fn save(&mut self, b: &Board, left: u8, pn: u32, dn: u32) {
        let key = Self::dkey(b, left);
        let i = Self::didx(key);
        // two slots per bucket; a proof is kept over other entries (the main search uses them)
        let j = if self.dtt[i].key == key {
            i
        } else if self.dtt[i ^ 1].key == key {
            i ^ 1
        } else if self.dtt[i].pn == 0 && self.dtt[i].key != 0 {
            i ^ 1
        } else {
            i
        };
        let or = b.side_to_move() == self.attacker;
        self.dtt[j] = DEntry { key, hash: b.hash(), pn, dn, or };
    }

    /// Every position proven so far: (hash, true if the side to move there mates, false if
    /// the side to move gets mated).
    pub fn proofs(&self) -> Vec<(u64, bool)> {
        self.dtt.iter().filter(|e| e.key != 0 && e.pn == 0).map(|e| (e.hash, e.or)).collect()
    }

    fn children(&mut self, b: &Board, or: bool) -> Vec<(Move, Board)> {
        let moves = if or {
            let key = b.hash();
            let i = (key >> (64 - MC_BITS)) as usize;
            if self.move_cache[i].0 == key && key != 0 {
                self.move_cache[i].1.clone()
            } else {
                self.gens += 1;
                let v = self.ms.attacker_moves(b, 99, 0);
                if !self.ms.aborted {
                    self.move_cache[i] = (key, v.clone());
                }
                v
            }
        } else {
            legal_moves(b)
        };
        moves
            .into_iter()
            .map(|m| {
                let mut c = b.clone();
                c.play_unchecked(m);
                (m, c)
            })
            .collect()
    }

    /// Win mode: the attacker (to move, not in check) is far enough ahead in material, even
    /// counting the defender's best capture if it were the defender's turn.
    fn goal_met(&self, b: &Board) -> bool {
        let goal = self.ms.goal;
        if goal <= 0 || !b.checkers().is_empty() {
            return false;
        }
        let mat = crate::eval::material_balance(b, b.side_to_move());
        if mat < goal {
            return false;
        }
        let nb = match b.null_move() {
            Some(nb) => nb,
            None => return false,
        };
        let mut threat = 0;
        nb.generate_moves(|mvs| {
            for m in mvs {
                if nb.piece_on(m.to).is_some() {
                    threat = threat.max(crate::search::see(&nb, m));
                }
            }
            false
        });
        mat - threat >= goal
    }

    /// Terminal value of a node, if it is one.
    fn terminal(b: &Board, or: bool) -> Option<(u32, u32)> {
        if has_legal_move(b) {
            return None;
        }
        let mated = !b.checkers().is_empty();
        // the defender mated: proven; anything else (attacker mated, stalemate): disproven
        Some(if !or && mated { (0, PN_INF) } else { (PN_INF, 0) })
    }

    /// `left`: attacking moves left (the mate must come within that many)
    fn mid(&mut self, b: &Board, th_pn: u32, th_dn: u32, or: bool, left: u8) -> (u32, u32) {
        self.mids += 1;
        if self.ms.tick() {
            return (1, 1);
        }
        if let Some(t) = Self::terminal(b, or) {
            self.save(b, left, t.0, t.1);
            return t;
        }
        if or && self.goal_met(b) {
            self.save(b, left, 0, PN_INF);
            return (0, PN_INF);
        }
        if self.path.len() >= MAX_PLY || (or && left == 0) {
            return (PN_INF, 0);
        }
        // children of an attacking node have one attacking move fewer left
        let cleft = if or { left - 1 } else { left };
        let kids = self.children(b, or);
        if self.ms.aborted {
            return (1, 1);
        }
        if kids.is_empty() {
            // the attacker has no forcing move left
            self.save(b, left, PN_INF, 0);
            return (PN_INF, 0);
        }
        self.path.push(b.hash());
        let result;
        loop {
            // gather the children's numbers
            let mut best = 0usize;
            let (mut pn, mut dn);
            let mut second = PN_INF;
            if or {
                pn = PN_INF;
                dn = 0;
                for (i, (_, c)) in kids.iter().enumerate() {
                    let (cp, cd) = self.lookup(c, false, cleft);
                    dn = (dn + cd).min(PN_INF);
                    if cp < pn {
                        second = pn;
                        pn = cp;
                        best = i;
                    } else if cp < second {
                        second = cp;
                    }
                }
            } else {
                pn = 0;
                dn = PN_INF;
                for (i, (_, c)) in kids.iter().enumerate() {
                    let (cp, cd) = self.lookup(c, true, cleft);
                    pn = (pn + cp).min(PN_INF);
                    if cd < dn {
                        second = dn;
                        dn = cd;
                        best = i;
                    } else if cd < second {
                        second = cd;
                    }
                }
            }
            if pn >= th_pn || dn >= th_dn || self.ms.aborted {
                self.save(b, left, pn, dn);
                result = (pn, dn);
                break;
            }
            let c = kids[best].1.clone();
            let (cp, cd) = self.lookup(&c, !or, cleft);
            if or {
                let tp = th_pn.min(second.saturating_add(1));
                let td = (th_dn - dn).saturating_add(cd).min(PN_INF);
                self.mid(&c, tp, td, false, cleft);
            } else {
                let td = th_dn.min(second.saturating_add(1));
                let tp = (th_pn - pn).saturating_add(cp).min(PN_INF);
                self.mid(&c, tp, td, true, cleft);
            }
        }
        self.path.pop();
        result
    }

    /// Prove a forced mate for the side to move; returns the mating move.
    pub fn set_goal(&mut self, goal: i32) {
        self.ms.goal = goal;
    }

    pub fn solve(&mut self, b: &Board, max_moves: u8) -> Option<Move> {
        let _ = self.attacker;
        let (pn, _) = self.mid(b, PN_INF - 1, PN_INF - 1, true, max_moves);
        if pn != 0 || self.ms.aborted {
            return None;
        }
        // the move to a proven child
        for (m, c) in self.children(b, true) {
            if Self::terminal(&c, false) == Some((0, PN_INF)) {
                return Some(m);
            }
            let k = Self::dkey(&c, max_moves - 1);
            let i = Self::didx(k);
            let e = if self.dtt[i].key == k { self.dtt[i] } else { self.dtt[i ^ 1] };
            if e.key == k && e.pn == 0 {
                return Some(m);
            }
        }
        None
    }

    pub fn nodes(&self) -> u64 {
        self.ms.nodes
    }
}

/// Proof-number mate search for the side to move with a time budget.
pub fn prove_mate(b: &Board, max_moves: u8, budget: std::time::Duration, stop: &AtomicBool, threat_depth: u8) -> Option<(Move, u64)> {
    let mut d = Dfpn::new(Instant::now() + budget, stop, threat_depth, b.side_to_move());
    if let Ok(g) = std::env::var("PNSGOAL") {
        d.set_goal(g.parse().unwrap());
    }
    if std::env::var("PNSLITE").is_ok() {
        d.ms.lite_threats = true;
    }
    let r = d.solve(b, max_moves);
    if std::env::var("PNSDBG").is_ok() {
        eprintln!("mids {} gens {} nodes {} proofs {}", d.mids, d.gens, d.nodes(), d.proofs().len());
        if let Ok(f) = std::env::var("PNSDUMP") {
            let v: Vec<String> = d.proofs().iter().map(|(h, o)| format!("{:016x} {}", h, *o as u8)).collect();
            std::fs::write(f, v.join("\n")).unwrap();
        }
    }
    r.map(|m| (m, d.nodes()))
}

/// Convenience: search for a forcing mate for the side to move with a time budget.
pub fn find_mate(b: &Board, max_n: u8, budget: std::time::Duration, stop: &AtomicBool, threat_depth: u8) -> Option<(Move, u8, u64)> {
    let mut ms = MateSearch::new(Instant::now() + budget, stop);
    ms.threat_depth = threat_depth;
    let r = ms.find(b, max_n);
    r.map(|(m, n)| (m, n, ms.nodes()))
}

#[allow(dead_code)]
pub fn is_checkmate(b: &Board) -> bool {
    is_mate(b)
}
