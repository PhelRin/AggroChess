use cozy_chess::Board;
use polyglot_book_rs::PolyglotBook;
use std::path::Path;
use std::time::SystemTime;

pub struct Book {
    book: Option<PolyglotBook>,
}

pub(crate) struct GambitPath {
    pub(crate) moves: &'static [&'static str],
    pub(crate) next_move: &'static str,
    pub(crate) weight: u32,
}

pub(crate) static GAMBIT_PATHS: &[GambitPath] = &[
    // White Opening Choices (starting pos)
    GambitPath { moves: &[], next_move: "e2e4", weight: 100 },
    GambitPath { moves: &[], next_move: "d2d4", weight: 50 },
    GambitPath { moves: &[], next_move: "c2c4", weight: 20 },
    GambitPath { moves: &[], next_move: "g1f3", weight: 20 },

    // Scandinavian / Danish Gambit (1. e4 d5 2. exd5 c6 3. dxc6 Nf6 4. cxb7 Bxb7)
    GambitPath { moves: &["e2e4"], next_move: "d7d5", weight: 100 },
    GambitPath { moves: &["e2e4", "d7d5"], next_move: "e4d5", weight: 100 },
    GambitPath { moves: &["e2e4", "d7d5", "e4d5"], next_move: "c7c6", weight: 100 },
    GambitPath { moves: &["e2e4", "d7d5", "e4d5", "c7c6"], next_move: "d5c6", weight: 100 },
    GambitPath { moves: &["e2e4", "d7d5", "e4d5", "c7c6", "d5c6"], next_move: "g8f6", weight: 100 },
    GambitPath { moves: &["e2e4", "d7d5", "e4d5", "c7c6", "d5c6", "g8f6"], next_move: "c6b7", weight: 100 },
    GambitPath { moves: &["e2e4", "d7d5", "e4d5", "c7c6", "d5c6", "g8f6", "c6b7"], next_move: "c8b7", weight: 100 },

    // King's Gambit (1. e4 e5 2. f4 exf4)
    GambitPath { moves: &["e2e4"], next_move: "e7e5", weight: 100 },
    GambitPath { moves: &["e2e4", "e7e5"], next_move: "f2f4", weight: 100 },
    GambitPath { moves: &["e2e4", "e7e5", "f2f4"], next_move: "e5f4", weight: 100 },

    // Queen's Gambit Accepted (1. d4 d5 2. c4 dxc4)
    GambitPath { moves: &["d2d4"], next_move: "d7d5", weight: 100 },
    GambitPath { moves: &["d2d4", "d7d5"], next_move: "c2c4", weight: 100 },
    GambitPath { moves: &["d2d4", "d7d5", "c2c4"], next_move: "d5c4", weight: 100 },

    // Bishop's Opening (1. e4 e5 2. Bc4)
    GambitPath { moves: &["e2e4", "e7e5"], next_move: "f1c4", weight: 50 },

    // Stafford Gambit (1. e4 e5 2. Nf3 Nf6 3. Nxe5 Nc6 4. Nxc6 dxc6)
    GambitPath { moves: &["e2e4", "e7e5"], next_move: "g1f3", weight: 100 },
    GambitPath { moves: &["e2e4", "e7e5", "g1f3"], next_move: "g8f6", weight: 100 },
    GambitPath { moves: &["e2e4", "e7e5", "g1f3", "g8f6"], next_move: "f3e5", weight: 100 },
    GambitPath { moves: &["e2e4", "e7e5", "g1f3", "g8f6", "f3e5"], next_move: "b8c6", weight: 100 },
    GambitPath { moves: &["e2e4", "e7e5", "g1f3", "g8f6", "f3e5", "b8c6"], next_move: "e5c6", weight: 100 },
    GambitPath { moves: &["e2e4", "e7e5", "g1f3", "g8f6", "f3e5", "b8c6", "e5c6"], next_move: "d7c6", weight: 100 },

    // Blackburne Shilling Gambit (1. e4 e5 2. Nf3 Nc6 3. Bc4 Nd4)
    GambitPath { moves: &["e2e4", "e7e5", "g1f3", "b8c6"], next_move: "f1c4", weight: 100 },
    GambitPath { moves: &["e2e4", "e7e5", "g1f3", "b8c6", "f1c4"], next_move: "c6d4", weight: 100 },

    // Fishing Pole Trap (1. e4 e5 2. Nf3 Nc6 3. Bb5 Nf6 4. O-O Ng4 5. h3 h5)
    GambitPath { moves: &["e2e4", "e7e5", "g1f3", "b8c6"], next_move: "f1b5", weight: 100 },
    GambitPath { moves: &["e2e4", "e7e5", "g1f3", "b8c6", "f1b5"], next_move: "g8f6", weight: 100 },
    GambitPath { moves: &["e2e4", "e7e5", "g1f3", "b8c6", "f1b5", "g8f6"], next_move: "e1g1", weight: 100 },
    GambitPath { moves: &["e2e4", "e7e5", "g1f3", "b8c6", "f1b5", "g8f6", "e1g1"], next_move: "f6g4", weight: 100 },
    GambitPath { moves: &["e2e4", "e7e5", "g1f3", "b8c6", "f1b5", "g8f6", "e1g1", "f6g4"], next_move: "h2h3", weight: 100 },
    GambitPath { moves: &["e2e4", "e7e5", "g1f3", "b8c6", "f1b5", "g8f6", "e1g1", "f6g4", "h2h3"], next_move: "h7h5", weight: 100 },
];

impl Book {
    pub fn new(path: &str) -> Self {
        if Path::new(path).exists() {
            match PolyglotBook::load(path) {
                Ok(book) => {
                    println!("info string Loaded opening book from {}", path);
                    Self { book: Some(book) }
                }
                Err(e) => {
                    println!("info string Failed to load opening book: {:?}", e);
                    Self { book: None }
                }
            }
        } else {
            println!("info string Opening book file not found at {}", path);
            Self { book: None }
        }
    }


    pub fn get_move(&self, board: &Board) -> Option<String> {
        // First try the curated gambit opening book paths
        let mut candidates = Vec::new();
        
        for path in GAMBIT_PATHS {
            // Reconstruct path
            let mut temp_board = Board::default();
            let mut matched = false;
            
            if temp_board.hash() == board.hash() {
                if path.moves.is_empty() {
                    matched = true;
                }
            } else {
                for (i, &mv_str) in path.moves.iter().enumerate() {
                    if let Ok(mv) = cozy_chess::util::parse_uci_move(&temp_board, mv_str) {
                        if temp_board.is_legal(mv) {
                            temp_board.play(mv);
                            if temp_board.hash() == board.hash() && i + 1 == path.moves.len() {
                                matched = true;
                                break;
                            }
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
            
            if matched {
                // Verify if the next_move is legal in the current board
                if let Ok(mv) = cozy_chess::util::parse_uci_move(board, path.next_move) {
                    if board.is_legal(mv) {
                        candidates.push((path.next_move, path.weight));
                    }
                }
            }
        }
        
        if !candidates.is_empty() {
            // Select candidate
            let total_weight: u32 = candidates.iter().map(|c| c.1).sum();
            let mut prng = SimplePrng::new();
            if total_weight == 0 {
                let idx = prng.next_range(0, candidates.len() as u64) as usize;
                return Some(candidates[idx].0.to_string());
            }
            let r = prng.next_range(0, total_weight as u64) as u32;
            let mut sum = 0;
            for (mv_str, weight) in candidates {
                sum += weight;
                if r < sum {
                    return Some(mv_str.to_string());
                }
            }
        }

        // If no curated gambit matched, fall back to Polyglot book
        let book = self.book.as_ref()?;
        let fen = board.to_string();
        let entries = book.get_all_moves_from_fen(&fen);
        if entries.is_empty() {
            return None;
        }

        // Weighted random selection
        let total_weight: u32 = entries.iter().map(|e| e.weight as u32).sum();
        if total_weight == 0 {
            let mut prng = SimplePrng::new();
            let idx = prng.next_range(0, entries.len() as u64) as usize;
            return Some(entries[idx].move_string.clone());
        }

        let mut prng = SimplePrng::new();
        let r = prng.next_range(0, total_weight as u64) as u32;
        let mut sum = 0;
        for entry in &entries {
            sum += entry.weight as u32;
            if r < sum {
                return Some(entry.move_string.clone());
            }
        }

        Some(entries[0].move_string.clone())
    }

}

struct SimplePrng {
    state: u64,
}

impl SimplePrng {
    fn new() -> Self {
        let seed = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(88172645463325252);
        Self { state: if seed == 0 { 88172645463325252 } else { seed } }
    }

    fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn next_range(&mut self, min: u64, max: u64) -> u64 {
        if min >= max { return min; }
        min + (self.next() % (max - min))
    }
}
