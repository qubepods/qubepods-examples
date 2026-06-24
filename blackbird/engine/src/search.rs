use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::board::{Board, Color, PieceKind, Square};
use crate::movegen::{is_in_check, legal_captures, legal_moves, mvv_lva_score};
use crate::nnue;
use crate::pst;
use crate::moves::{self, Move};
use crate::tt::{NodeType, TTEntry, TranspositionTable};
use crate::zobrist;

const MAX_PLY: usize = 128;

/// Monotonic clock for search timing. On wasm32-unknown-unknown (the browser
/// build) `std::time::Instant::now()` aborts, so a zero-duration stub stands
/// in: fixed-depth searches never consult the clock for control flow there,
/// only for the info log, which then reports time 1ms.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use std::time::Instant;

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#[derive(Clone, Copy)]
struct Instant;

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
impl Instant {
    fn now() -> Self {
        Instant
    }

    fn elapsed(&self) -> std::time::Duration {
        std::time::Duration::ZERO
    }
}

/// Evaluation: blend NNUE + PST.
/// NNUE provides the primary eval, PST is a safety net.
/// Full evaluation (NNUE + PST blend) for main search
fn evaluate(board: &Board) -> i32 {
    let nnue_score = nnue::nnue_evaluate(board);
    let pst_score = pst::evaluate(board);
    // Blend: 80% NNUE, 20% PST
    (nnue_score * 4 + pst_score) / 5
}

/// Fast evaluation (PST only) for quiescence search
fn evaluate_fast(board: &Board) -> i32 {
    pst::evaluate(board)
}

/// Search heuristics: killer moves + history table
struct SearchHeuristics {
    /// Two killer moves per ply (quiet moves that caused beta cutoff)
    killers: [[Option<Move>; 2]; MAX_PLY],
    /// History heuristic: indexed by [color][from][to], counts how often a move causes cutoff
    history: [[[i32; 64]; 64]; 2],
    /// Node counter
    nodes: u64,
}

impl SearchHeuristics {
    fn new() -> Self {
        Self {
            killers: [[None; 2]; MAX_PLY],
            history: [[[0; 64]; 64]; 2],
            nodes: 0,
        }
    }

    fn store_killer(&mut self, ply: usize, mv: Move) {
        if ply >= MAX_PLY { return; }
        // Don't store duplicates
        if self.killers[ply][0] == Some(mv) { return; }
        self.killers[ply][1] = self.killers[ply][0];
        self.killers[ply][0] = Some(mv);
    }

    fn is_killer(&self, ply: usize, mv: &Move) -> bool {
        if ply >= MAX_PLY { return false; }
        self.killers[ply][0].as_ref() == Some(mv) || self.killers[ply][1].as_ref() == Some(mv)
    }

    fn update_history(&mut self, color: Color, mv: &Move, depth: u32) {
        let ci = color.index();
        let from = mv.from.index();
        let to = mv.to.index();
        self.history[ci][from][to] += (depth * depth) as i32;
        // Cap to prevent overflow
        if self.history[ci][from][to] > 1_000_000 {
            // Age all history entries
            for c in 0..2 {
                for f in 0..64 {
                    for t in 0..64 {
                        self.history[c][f][t] /= 2;
                    }
                }
            }
        }
    }

    fn history_score(&self, color: Color, mv: &Move) -> i32 {
        self.history[color.index()][mv.from.index()][mv.to.index()]
    }
}

/// Check if the given side has any non-pawn material (for null move safety)
fn has_non_pawn_material(board: &Board, color: Color) -> bool {
    for sq in 0..64u8 {
        let s = Square::new(sq & 7, sq >> 3);
        if let Some(p) = board.piece_at(s) {
            if p.color == color && !matches!(p.kind, PieceKind::Pawn | PieceKind::King) {
                return true;
            }
        }
    }
    false
}

/// Convert a Move to the TT packed format
fn pack_move(mv: &Move) -> u32 {
    let from = mv.from.index() as u32;
    let to = mv.to.index() as u32;
    let promo = match mv.kind {
        moves::MoveKind::Promotion(k) => match k {
            PieceKind::Knight => 1,
            PieceKind::Bishop => 2,
            PieceKind::Rook => 3,
            PieceKind::Queen => 4,
            _ => 0,
        },
        _ => 0,
    };
    TTEntry::pack_move(from, to, promo)
}

/// Try to reconstruct a Move from TT packed format + current board legal moves
fn unpack_tt_move(packed: u32, board: &Board) -> Option<Move> {
    if packed == 0 {
        return None;
    }
    let from_idx = TTEntry::unpack_from(packed);
    let to_idx = TTEntry::unpack_to(packed);
    let from = Square::new((from_idx & 7) as u8, (from_idx >> 3) as u8);
    let to = Square::new((to_idx & 7) as u8, (to_idx >> 3) as u8);

    // Match against legal moves to get correct MoveKind
    let moves = legal_moves(board);
    moves.into_iter().find(|m| m.from == from && m.to == to)
}

/// Quiescence search — resolve captures to avoid horizon effect
fn quiesce(
    board: &Board,
    mut alpha: i32,
    beta: i32,
    stop: &AtomicBool,
) -> i32 {
    if stop.load(Ordering::Relaxed) {
        return 0;
    }

    let sign = if board.side_to_move == Color::White { 1 } else { -1 };
    let stand_pat = evaluate_fast(board) * sign;

    // Standing pat — if static eval is already good enough, we can stop
    if stand_pat >= beta {
        return beta;
    }
    if stand_pat > alpha {
        alpha = stand_pat;
    }

    // Generate and sort captures by MVV-LVA
    let mut captures = legal_captures(board);
    captures.sort_by(|a, b| mvv_lva_score(board, b).cmp(&mvv_lva_score(board, a)));

    for mv in &captures {
        let new_board = moves::make_move(board, mv);
        let score = -quiesce(&new_board, -beta, -alpha, stop);

        if stop.load(Ordering::Relaxed) {
            return 0;
        }

        if score >= beta {
            return beta;
        }
        if score > alpha {
            alpha = score;
        }
    }

    alpha
}

/// Check for draw by repetition (current hash appears in history) or 50-move rule
fn is_draw(board: &Board, position_history: &[u64]) -> bool {
    // 50-move rule: 100 half-moves without capture or pawn move
    if board.halfmove_clock >= 100 {
        return true;
    }
    // Threefold repetition: check if current hash appeared at least twice before
    // Only need to check back halfmove_clock positions (since last irreversible move)
    let current = board.hash;
    let lookback = board.halfmove_clock as usize;
    let start = if position_history.len() > lookback {
        position_history.len() - lookback
    } else {
        0
    };
    let mut count = 0;
    for &h in &position_history[start..] {
        if h == current {
            count += 1;
            if count >= 2 {
                return true; // This is the 3rd occurrence (2 in history + current)
            }
        }
    }
    false
}

/// Negamax search with alpha-beta pruning, TT, null move pruning, and quiescence
fn negamax(
    board: &Board,
    depth: u32,
    mut alpha: i32,
    beta: i32,
    tt: &TranspositionTable,
    stop: &AtomicBool,
    allow_null: bool,
    ply: usize,
    heuristics: &mut SearchHeuristics,
    position_history: &mut Vec<u64>,
) -> i32 {
    heuristics.nodes += 1;
    
    // Check for stop signal
    if stop.load(Ordering::Relaxed) {
        return 0;
    }

    // Draw detection
    if ply > 0 && is_draw(board, position_history) {
        return 0;
    }

    let is_check = is_in_check(board, board.side_to_move);

    // TT probe
    let tt_move_packed;
    if let Some(entry) = tt.probe(board.hash) {
        tt_move_packed = entry.best_move;
        if entry.depth as u32 >= depth {
            match entry.node_type {
                NodeType::Exact => return entry.score as i32,
                NodeType::LowerBound => {
                    if entry.score as i32 >= beta {
                        return entry.score as i32;
                    }
                    if entry.score as i32 > alpha {
                        alpha = entry.score as i32;
                    }
                }
                NodeType::UpperBound => {
                    if (entry.score as i32) <= alpha {
                        return entry.score as i32;
                    }
                }
            }
        }
    } else {
        tt_move_packed = 0;
    }

    let moves = legal_moves(board);
    if moves.is_empty() {
        return if is_check {
            -100_000 + (100 - depth as i32) // Prefer faster mates
        } else {
            0 // Stalemate
        };
    }

    // At depth 0, drop into quiescence search
    if depth == 0 {
        return quiesce(board, alpha, beta, stop);
    }

    // Null move pruning: if we can skip our turn and still beat beta, prune
    // Don't do it: in check, at low depth, after a null move, or in pawn-only endgames
    if allow_null && !is_check && depth >= 3 && has_non_pawn_material(board, board.side_to_move) {
        let r = if depth >= 6 { 3 } else { 2 }; // adaptive reduction
        let null_board = board.make_null_move();
        position_history.push(null_board.hash);
        let null_score = -negamax(&null_board, depth - 1 - r, -beta, -beta + 1, tt, stop, false, ply + 1, heuristics, position_history);
        position_history.pop();
        if null_score >= beta {
            return beta;
        }
    }

    // Check extension: search one deeper when in check
    let search_depth = if is_check { depth } else { depth - 1 };

    // Move ordering: TT move > promotions > captures (MVV-LVA) > killers > history > rest
    let mut scored_moves: Vec<(Move, i32)> = moves.iter().map(|mv| {
        let mut score = 0i32;
        // TT move gets highest priority
        if tt_move_packed != 0 {
            if let Some(tt_mv) = unpack_tt_move(tt_move_packed, board) {
                if *mv == tt_mv {
                    score += 2_000_000;
                }
            }
        }
        // Promotions
        if matches!(mv.kind, moves::MoveKind::Promotion(_)) {
            score += 1_500_000;
        }
        // Captures scored by MVV-LVA
        if board.piece_at(mv.to).is_some() || matches!(mv.kind, moves::MoveKind::EnPassant) {
            score += 1_000_000 + mvv_lva_score(board, mv);
        } else {
            // Quiet move ordering: killers then history
            if heuristics.is_killer(ply, mv) {
                score += 500_000;
            }
            score += heuristics.history_score(board.side_to_move, mv);
        }
        (*mv, score)
    }).collect();
    scored_moves.sort_by(|a, b| b.1.cmp(&a.1));

    let mut best = -200_000i32;
    let mut best_move_packed = 0u32;
    let mut node_type = NodeType::UpperBound;

    for (i, (mv, mv_score)) in scored_moves.iter().enumerate() {
        let new_board = moves::make_move(board, mv);
        position_history.push(new_board.hash);

        let mut score;

        // Late Move Reductions: reduce depth for quiet moves searched late
        let is_capture = board.piece_at(mv.to).is_some()
            || matches!(mv.kind, moves::MoveKind::EnPassant);
        let is_promotion = matches!(mv.kind, moves::MoveKind::Promotion(_));
        let is_tactical = is_capture || is_promotion || *mv_score >= 1_000_000;

        if !is_check && !is_tactical && i >= 3 && search_depth >= 2 {
            // LMR: search at reduced depth first
            let reduction = if i >= 6 { 2 } else { 1 };
            let reduced_depth = search_depth.saturating_sub(reduction);
            score = -negamax(&new_board, reduced_depth, -beta, -alpha, tt, stop, true, ply + 1, heuristics, position_history);

            // If it beats alpha, re-search at full depth
            if score > alpha {
                score = -negamax(&new_board, search_depth, -beta, -alpha, tt, stop, true, ply + 1, heuristics, position_history);
            }
        } else {
            score = -negamax(&new_board, search_depth, -beta, -alpha, tt, stop, true, ply + 1, heuristics, position_history);
        }

        position_history.pop();

        if stop.load(Ordering::Relaxed) {
            return 0;
        }

        if score > best {
            best = score;
            best_move_packed = pack_move(mv);
        }
        if score > alpha {
            alpha = score;
            node_type = NodeType::Exact;
        }
        if alpha >= beta {
            // Beta cutoff — store killer and history for quiet moves
            if !is_capture && !is_promotion {
                heuristics.store_killer(ply, *mv);
                heuristics.update_history(board.side_to_move, mv, depth);
            }
            node_type = NodeType::LowerBound;
            break;
        }
    }

    // TT store
    tt.store(
        board.hash,
        depth as i8,
        best as i16,
        best_move_packed,
        node_type,
    );

    best
}

/// Find the best move using iterative deepening + TT + optional Lazy SMP.
/// Returns None if no legal moves (checkmate/stalemate).
pub fn best_move(board: &Board, depth: u32) -> Option<Move> {
    best_move_threaded(board, depth, 1)
}

/// Find the best move with the given number of threads (Lazy SMP).
pub fn best_move_threaded(board: &Board, depth: u32, threads: usize) -> Option<Move> {
    best_move_threaded_with_history(board, depth, threads, &[])
}

/// Find the best move with the given number of threads and position history.
fn best_move_threaded_with_history(board: &Board, depth: u32, threads: usize, game_history: &[u64]) -> Option<Move> {
    zobrist::init();

    let moves = legal_moves(board);
    if moves.is_empty() {
        return None;
    }

    let tt = Arc::new(TranspositionTable::new(16)); // 16 MB default
    let stop = Arc::new(AtomicBool::new(false));
    let threads = threads.max(1);

    if threads == 1 {
        // Single-threaded: iterative deepening
        return search_root(board, depth, &tt, &stop, game_history, &SearchLimits::none());
    }

    // Lazy SMP: spawn helper threads searching at varied depths
    let mut handles = Vec::new();

    for t in 1..threads {
        let board_clone = board.clone();
        let tt_clone = Arc::clone(&tt);
        let stop_clone = Arc::clone(&stop);
        let history_clone = game_history.to_vec();
        // Helpers search at depth or depth+1 (odd threads go deeper)
        let helper_depth = if t % 2 == 0 { depth } else { depth + 1 };

        handles.push(std::thread::spawn(move || {
            search_root(&board_clone, helper_depth, &tt_clone, &stop_clone, &history_clone, &SearchLimits::none());
        }));
    }

    // Main thread does the real search
    let result = search_root(board, depth, &tt, &stop, game_history, &SearchLimits::none());

    // Signal helpers to stop
    stop.store(true, Ordering::Relaxed);
    for h in handles {
        let _ = h.join();
    }

    result
}

/// Soft time limit for smart time management
struct SearchLimits {
    /// Hard stop time (absolute deadline)
    hard_time: Option<std::time::Duration>,
    /// Soft stop time (can stop early if best move is stable)
    soft_time: Option<std::time::Duration>,
}

impl SearchLimits {
    fn none() -> Self { Self { hard_time: None, soft_time: None } }
    
    fn from_movetime(ms: u64) -> Self {
        Self {
            hard_time: Some(std::time::Duration::from_millis(ms)),
            soft_time: Some(std::time::Duration::from_millis(ms / 2)),
        }
    }
    
    fn from_clock(time_ms: u64, inc_ms: u64) -> Self {
        let allocated = (time_ms / 20) + (inc_ms / 2);
        let allocated = allocated.max(100).min(time_ms.saturating_sub(500));
        Self {
            hard_time: Some(std::time::Duration::from_millis(allocated)),
            soft_time: Some(std::time::Duration::from_millis(allocated / 3)),
        }
    }
}

/// Root search with iterative deepening. Returns best move found.
fn search_root(
    board: &Board,
    max_depth: u32,
    tt: &TranspositionTable,
    stop: &AtomicBool,
    game_history: &[u64],
    limits: &SearchLimits,
) -> Option<Move> {
    let start_time = Instant::now();
    let moves = legal_moves(board);
    if moves.is_empty() {
        return None;
    }

    let mut best_mv = moves[0];
    let mut heuristics = SearchHeuristics::new();
    let mut prev_score = 0i32;
    let mut stable_count: u32 = 0;  // how many depths the best move hasn't changed
    let mut prev_best = best_mv;
    // Build mutable position history from game history
    let mut position_history: Vec<u64> = game_history.to_vec();

    // Iterative deepening with aspiration windows
    for d in 1..=max_depth {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        
        // Hard time check
        if let Some(hard) = limits.hard_time {
            if start_time.elapsed() >= hard {
                break;
            }
        }

        // Aspiration window: narrow window around previous score (skip at depth 1)
        let mut delta = 50i32;
        let mut alpha = if d >= 2 { prev_score - delta } else { -200_000 };
        let mut beta = if d >= 2 { prev_score + delta } else { 200_000 };

        let mut best_score;
        let mut current_best;

        loop {
            best_score = -200_000i32;
            current_best = moves[0];

            // Order moves: TT move first
            let mut ordered_moves = moves.clone();
            if let Some(entry) = tt.probe(board.hash) {
                if entry.best_move != 0 {
                    if let Some(tt_mv) = unpack_tt_move(entry.best_move, board) {
                        if let Some(idx) = ordered_moves.iter().position(|m| *m == tt_mv) {
                            ordered_moves.swap(0, idx);
                        }
                    }
                }
            }

            for mv in &ordered_moves {
                let new_board = moves::make_move(board, mv);
                position_history.push(new_board.hash);
                let score = -negamax(&new_board, d.saturating_sub(1), -beta, -alpha.max(best_score), tt, stop, true, 1, &mut heuristics, &mut position_history);
                position_history.pop();

                if stop.load(Ordering::Relaxed) {
                    break;
                }

                if score > best_score {
                    best_score = score;
                    current_best = *mv;
                }
            }

            if stop.load(Ordering::Relaxed) {
                break;
            }

            // Check if score fell outside aspiration window
            if best_score <= alpha {
                // Fail low — widen window down and re-search
                alpha = (alpha - delta).max(-200_000);
                delta *= 2;
            } else if best_score >= beta {
                // Fail high — widen window up and re-search
                beta = (beta + delta).min(200_000);
                delta *= 2;
            } else {
                // Score within window — done with this depth
                break;
            }
        }

        if !stop.load(Ordering::Relaxed) {
            best_mv = current_best;
            prev_score = best_score;
            // Store root result in TT
            tt.store(board.hash, d as i8, best_score as i16, pack_move(&best_mv), NodeType::Exact);
            
            // Track move stability
            if best_mv == prev_best {
                stable_count += 1;
            } else {
                stable_count = 0;
                prev_best = best_mv;
            }
            
            // Emit UCI info line
            let elapsed = start_time.elapsed();
            let elapsed_ms = elapsed.as_millis().max(1) as u64;
            let nodes = heuristics.nodes;
            let nps = (nodes as u64 * 1000) / elapsed_ms;
            
            // Build PV from TT
            let pv_str = extract_pv(board, tt, d, &best_mv);
            
            let score_str = if best_score > 99_000 {
                format!("score mate {}", (100_001 - best_score + 1) / 2)
            } else if best_score < -99_000 {
                format!("score mate -{}", (100_001 + best_score + 1) / 2)
            } else {
                format!("score cp {}", best_score)
            };
            
            {
                let stdout = std::io::stdout();
                let mut out = stdout.lock();
                writeln!(out, "info depth {} {} nodes {} nps {} time {} pv {}", 
                         d, score_str, nodes, nps, elapsed_ms, pv_str).ok();
                out.flush().ok();
            }
            
            // Smart time management: stop early if best move is stable past soft time
            if let Some(soft) = limits.soft_time {
                if start_time.elapsed() >= soft && d >= 4 {
                    // Stop if best move stable for 3+ depths, or if we found a mate
                    if stable_count >= 3 || best_score.abs() > 99_000 {
                        break;
                    }
                }
            }
        }
    }

    Some(best_mv)
}

/// Extract principal variation from TT
fn extract_pv(board: &Board, tt: &TranspositionTable, max_depth: u32, first_move: &Move) -> String {
    let mut pv = vec![first_move.to_uci()];
    let mut current = moves::make_move(board, first_move);
    let mut seen = std::collections::HashSet::new();
    seen.insert(board.hash);
    
    for _ in 1..max_depth.min(20) {
        if seen.contains(&current.hash) {
            break;
        }
        seen.insert(current.hash);
        
        if let Some(entry) = tt.probe(current.hash) {
            if entry.best_move != 0 {
                if let Some(tt_mv) = unpack_tt_move(entry.best_move, &current) {
                    pv.push(tt_mv.to_uci());
                    current = moves::make_move(&current, &tt_mv);
                } else {
                    break;
                }
            } else {
                break;
            }
        } else {
            break;
        }
    }
    
    pv.join(" ")
}

/// Shared search state for UCI (holds TT across searches)
pub struct SearchState {
    pub tt: Arc<TranspositionTable>,
    pub threads: usize,
}

impl SearchState {
    pub fn new(hash_mb: usize, threads: usize) -> Self {
        Self {
            tt: Arc::new(TranspositionTable::new(hash_mb)),
            threads: threads.max(1),
        }
    }

    pub fn resize_tt(&mut self, hash_mb: usize) {
        self.tt = Arc::new(TranspositionTable::new(hash_mb));
    }
    
    /// Clone search state for use on a background thread (shares TT via Arc)
    pub fn clone_for_search(&self) -> Self {
        Self {
            tt: Arc::clone(&self.tt),
            threads: self.threads,
        }
    }

    pub fn clear(&self) {
        self.tt.clear();
    }

    /// Search with shared TT and configured thread count
    pub fn search(&self, board: &Board, depth: u32, game_history: &[u64]) -> Option<Move> {
        zobrist::init();

        let moves = legal_moves(board);
        if moves.is_empty() {
            return None;
        }

        let stop = Arc::new(AtomicBool::new(false));
        let threads = self.threads;

        if threads <= 1 {
            return search_root(board, depth, &self.tt, &stop, game_history, &SearchLimits::none());
        }

        // Lazy SMP
        let mut handles = Vec::new();
        for t in 1..threads {
            let board_clone = board.clone();
            let tt_clone = Arc::clone(&self.tt);
            let stop_clone = Arc::clone(&stop);
            let history_clone = game_history.to_vec();
            let helper_depth = if t % 2 == 0 { depth } else { depth + 1 };

            handles.push(std::thread::spawn(move || {
                search_root(&board_clone, helper_depth, &tt_clone, &stop_clone, &history_clone, &SearchLimits::none());
            }));
        }

        let result = search_root(board, depth, &self.tt, &stop, game_history, &SearchLimits::none());

        stop.store(true, Ordering::Relaxed);
        for h in handles {
            let _ = h.join();
        }

        result
    }

    /// Search for a given number of milliseconds (time management)
    pub fn search_timed(&self, board: &Board, time_ms: u64, game_history: &[u64]) -> Option<Move> {
        zobrist::init();

        let moves = legal_moves(board);
        if moves.is_empty() {
            return None;
        }

        let stop = Arc::new(AtomicBool::new(false));
        let threads = self.threads;
        let max_depth = 64u32;
        let limits = SearchLimits::from_movetime(time_ms);

        // Timer thread: hard stop after time_ms
        let stop_timer = Arc::clone(&stop);
        let timer_handle = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(time_ms));
            stop_timer.store(true, Ordering::Relaxed);
        });

        if threads <= 1 {
            let result = search_root(board, max_depth, &self.tt, &stop, game_history, &limits);
            stop.store(true, Ordering::Relaxed);
            let _ = timer_handle.join();
            return result;
        }

        // Lazy SMP
        let mut handles = Vec::new();
        for t in 1..threads {
            let board_clone = board.clone();
            let tt_clone = Arc::clone(&self.tt);
            let stop_clone = Arc::clone(&stop);
            let history_clone = game_history.to_vec();
            let helper_depth = if t % 2 == 0 { max_depth } else { max_depth - 1 };

            handles.push(std::thread::spawn(move || {
                search_root(&board_clone, helper_depth, &tt_clone, &stop_clone, &history_clone, &SearchLimits::none());
            }));
        }

        let result = search_root(board, max_depth, &self.tt, &stop, game_history, &limits);

        stop.store(true, Ordering::Relaxed);
        let _ = timer_handle.join();
        for h in handles {
            let _ = h.join();
        }

        result
    }

    /// Search until an external stop signal (for `go infinite`)
    pub fn search_infinite(&self, board: &Board, stop: Arc<AtomicBool>, game_history: &[u64]) -> Option<Move> {
        zobrist::init();

        let moves = legal_moves(board);
        if moves.is_empty() {
            return None;
        }

        let max_depth = 64u32;
        let threads = self.threads;

        if threads <= 1 {
            return search_root(board, max_depth, &self.tt, &stop, game_history, &SearchLimits::none());
        }

        // Lazy SMP
        let mut handles = Vec::new();
        for t in 1..threads {
            let board_clone = board.clone();
            let tt_clone = Arc::clone(&self.tt);
            let stop_clone = Arc::clone(&stop);
            let history_clone = game_history.to_vec();
            let helper_depth = if t % 2 == 0 { max_depth } else { max_depth - 1 };

            handles.push(std::thread::spawn(move || {
                search_root(&board_clone, helper_depth, &tt_clone, &stop_clone, &history_clone, &SearchLimits::none());
            }));
        }

        let result = search_root(board, max_depth, &self.tt, &stop, game_history, &SearchLimits::none());

        stop.store(true, Ordering::Relaxed);
        for h in handles {
            let _ = h.join();
        }

        result
    }

    /// Search with time control: allocate time from remaining clock
    pub fn search_with_clock(&self, board: &Board, time_ms: u64, inc_ms: u64, game_history: &[u64]) -> Option<Move> {
        // Simple time allocation: use 1/20 of remaining time + 1/2 of increment
        let allocated = (time_ms / 20) + (inc_ms / 2);
        let allocated = allocated.max(100).min(time_ms.saturating_sub(500)); // leave buffer
        self.search_timed(board, allocated, game_history)
    }
}

impl Default for SearchState {
    fn default() -> Self {
        Self::new(16, 1)
    }
}
