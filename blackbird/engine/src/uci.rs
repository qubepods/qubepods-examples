use crate::board::Board;
use crate::moves::Move;
use crate::search::SearchState;
use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const ENGINE_NAME: &str = "Black Bird Chess";
const ENGINE_AUTHOR: &str = "Plinken";

pub struct UciEngine {
    board: Board,
    search: SearchState,
    /// Zobrist hashes of all positions from game start (for repetition detection)
    position_history: Vec<u64>,
    /// Stop flag for infinite search
    stop: Arc<AtomicBool>,
    /// Handle for the background search thread
    search_handle: Option<std::thread::JoinHandle<Option<Move>>>,
}

impl UciEngine {
    pub fn new() -> Self {
        crate::zobrist::init();
        let board = Board::new();
        let position_history = vec![board.hash];
        Self {
            board,
            search: SearchState::new(16, 1),
            position_history,
            stop: Arc::new(AtomicBool::new(false)),
            search_handle: None,
        }
    }

    pub fn run(&mut self) {
        let stdin = io::stdin();
        let mut stdout = io::stdout();

        for line in stdin.lock().lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }

            let response = self.handle_command(&line);
            for r in &response {
                writeln!(stdout, "{}", r).ok();
            }
            stdout.flush().ok();

            if line == "quit" {
                break;
            }
        }
    }

    pub fn handle_command(&mut self, input: &str) -> Vec<String> {
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            return vec![];
        }

        match parts[0] {
            "uci" => vec![
                format!("id name {}", ENGINE_NAME),
                format!("id author {}", ENGINE_AUTHOR),
                format!("option name Hash type spin default 16 min 1 max 1024"),
                format!("option name Threads type spin default 1 min 1 max 256"),
                "uciok".to_string(),
            ],
            "isready" => vec!["readyok".to_string()],
            "setoption" => {
                self.handle_setoption(&parts[1..]);
                vec![]
            }
            "ucinewgame" => {
                self.board = Board::new();
                self.search.clear();
                vec![]
            }
            "position" => {
                self.handle_position(&parts[1..]);
                vec![]
            }
            "go" => {
                self.handle_go(&parts[1..]);
                vec![] // output is printed directly by search
            }
            "stop" => {
                self.handle_stop()
            }
            "d" => {
                vec![self.board.to_fen()]
            }
            "quit" => {
                self.stop.store(true, Ordering::Relaxed);
                vec![]
            }
            _ => vec![],
        }
    }

    fn handle_setoption(&mut self, args: &[&str]) {
        let name_idx = args.iter().position(|&s| s == "name");
        let value_idx = args.iter().position(|&s| s == "value");

        if let (Some(ni), Some(vi)) = (name_idx, value_idx) {
            let name: String = args[ni + 1..vi].join(" ");
            let value: String = args[vi + 1..].join(" ");

            match name.to_lowercase().as_str() {
                "hash" => {
                    if let Ok(mb) = value.parse::<usize>() {
                        self.search.resize_tt(mb.clamp(1, 1024));
                    }
                }
                "threads" => {
                    if let Ok(t) = value.parse::<usize>() {
                        self.search.threads = t.clamp(1, 256);
                    }
                }
                _ => {}
            }
        }
    }

    fn handle_position(&mut self, args: &[&str]) {
        if args.is_empty() {
            return;
        }

        let mut moves_start = None;
        if args[0] == "startpos" {
            self.board = Board::new();
            self.position_history = vec![self.board.hash];
            moves_start = args.iter().position(|&s| s == "moves");
        } else if args[0] == "fen" {
            let fen_parts: Vec<&str> = args[1..].iter()
                .take_while(|&&s| s != "moves")
                .copied()
                .collect();
            let fen = fen_parts.join(" ");
            self.board = Board::from_fen(&fen).unwrap_or_else(|_| Board::new());
            self.position_history = vec![self.board.hash];
            moves_start = args.iter().position(|&s| s == "moves");
        }

        if let Some(mi) = moves_start {
            for uci_move in &args[mi + 1..] {
                if let Some(mv) = Move::from_uci(uci_move, &self.board) {
                    self.board = crate::moves::make_move(&self.board, &mv);
                    self.position_history.push(self.board.hash);
                }
            }
        }
    }

    fn handle_go(&mut self, args: &[&str]) {
        let mut depth: Option<u32> = None;
        let mut movetime: Option<u64> = None;
        let mut wtime: Option<u64> = None;
        let mut btime: Option<u64> = None;
        let mut winc: Option<u64> = None;
        let mut binc: Option<u64> = None;
        let mut infinite = false;

        let mut i = 0;
        while i < args.len() {
            match args[i] {
                "depth" => { depth = args.get(i + 1).and_then(|s| s.parse().ok()); i += 2; }
                "movetime" => { movetime = args.get(i + 1).and_then(|s| s.parse().ok()); i += 2; }
                "wtime" => { wtime = args.get(i + 1).and_then(|s| s.parse().ok()); i += 2; }
                "btime" => { btime = args.get(i + 1).and_then(|s| s.parse().ok()); i += 2; }
                "winc" => { winc = args.get(i + 1).and_then(|s| s.parse().ok()); i += 2; }
                "binc" => { binc = args.get(i + 1).and_then(|s| s.parse().ok()); i += 2; }
                "infinite" => { infinite = true; i += 1; }
                _ => { i += 1; }
            }
        }

        let board = self.board.clone();
        let history = self.position_history.clone();
        
        if infinite {
            // Infinite search: run on background thread, wait for "stop"
            self.stop = Arc::new(AtomicBool::new(false));
            let stop = Arc::clone(&self.stop);
            let search = self.search.clone_for_search();
            
            self.search_handle = Some(std::thread::spawn(move || {
                search.search_infinite(&board, stop, &history)
            }));
        } else {
            // Blocking search
            let result = if let Some(d) = depth {
                self.search.search(&board, d, &history)
            } else if let Some(mt) = movetime {
                self.search.search_timed(&board, mt, &history)
            } else if wtime.is_some() || btime.is_some() {
                let (time, inc) = if self.board.side_to_move == crate::board::Color::White {
                    (wtime.unwrap_or(60000), winc.unwrap_or(0))
                } else {
                    (btime.unwrap_or(60000), binc.unwrap_or(0))
                };
                self.search.search_with_clock(&board, time, inc, &history)
            } else {
                self.search.search(&board, 10, &history)
            };

            self.print_result(result);
        }
    }

    fn handle_stop(&mut self) -> Vec<String> {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.search_handle.take() {
            let result = handle.join().ok().flatten();
            self.print_result(result);
        }
        vec![]
    }

    fn print_result(&self, result: Option<Move>) {
        let mut stdout = io::stdout();
        match result {
            Some(mv) => {
                let hashfull = self.search.tt.hashfull();
                writeln!(stdout, "info hashfull {} threads {}", hashfull, self.search.threads).ok();
                writeln!(stdout, "bestmove {}", mv.to_uci()).ok();
            }
            None => {
                writeln!(stdout, "bestmove 0000").ok();
            }
        }
        stdout.flush().ok();
    }
}

impl Default for UciEngine {
    fn default() -> Self {
        Self::new()
    }
}
