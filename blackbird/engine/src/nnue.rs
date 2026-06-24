//! NNUE inference for Black Bird Chess.
//!
//! Loads a binary weights file (embedded at compile time) and evaluates
//! positions using a simple feedforward network:
//!   768 inputs → 256 (ReLU) → 128 (ReLU) → 1 (Tanh)
//!
//! Input encoding: 12 piece types × 64 squares (one-hot).
//! Output: evaluation in [-1.0, 1.0] from white's perspective.

use crate::board::{Board, Color, PieceKind, Square};

/// Embedded weights (baked into the binary at compile time)
static WEIGHTS_DATA: &[u8] = include_bytes!("../../training/data/blackbird.nnue");

/// A dense layer: y = ReLU(Wx + b) or y = tanh(Wx + b)
struct DenseLayer {
    weights: Vec<f32>, // row-major [rows × cols]
    biases: Vec<f32>,  // [rows]
    rows: usize,
    cols: usize,
}

impl DenseLayer {
    fn forward_relu(&self, input: &[f32], output: &mut [f32]) {
        debug_assert_eq!(input.len(), self.cols);
        debug_assert_eq!(output.len(), self.rows);
        for r in 0..self.rows {
            let mut sum = self.biases[r];
            let row_start = r * self.cols;
            // Use chunks for better auto-vectorization
            let weights_row = &self.weights[row_start..row_start + self.cols];
            for (w, &x) in weights_row.iter().zip(input.iter()) {
                sum += w * x;
            }
            output[r] = sum.max(0.0); // ReLU
        }
    }

    fn forward_tanh(&self, input: &[f32], output: &mut [f32]) {
        debug_assert_eq!(input.len(), self.cols);
        debug_assert_eq!(output.len(), self.rows);
        for r in 0..self.rows {
            let mut sum = self.biases[r];
            let row_start = r * self.cols;
            let weights_row = &self.weights[row_start..row_start + self.cols];
            for (w, &x) in weights_row.iter().zip(input.iter()) {
                sum += w * x;
            }
            output[r] = sum.tanh();
        }
    }
}

/// The full NNUE network.
pub struct NnueNetwork {
    layer1: DenseLayer, // 768 → 256
    layer2: DenseLayer, // 256 → 128
    layer3: DenseLayer, // 128 → 1
}

/// Piece index for NNUE input encoding.
/// White: P=0, N=1, B=2, R=3, Q=4, K=5
/// Black: p=6, n=7, b=8, r=9, q=10, k=11
fn piece_index(color: Color, kind: PieceKind) -> usize {
    let base = match color {
        Color::White => 0,
        Color::Black => 6,
    };
    let kind_idx = match kind {
        PieceKind::Pawn => 0,
        PieceKind::Knight => 1,
        PieceKind::Bishop => 2,
        PieceKind::Rook => 3,
        PieceKind::Queen => 4,
        PieceKind::King => 5,
    };
    base + kind_idx
}

/// Parse a little-endian u32 from bytes.
fn read_u32(data: &[u8], offset: &mut usize) -> u32 {
    let val = u32::from_le_bytes([
        data[*offset],
        data[*offset + 1],
        data[*offset + 2],
        data[*offset + 3],
    ]);
    *offset += 4;
    val
}

/// Parse a little-endian f32 from bytes.
fn read_f32(data: &[u8], offset: &mut usize) -> f32 {
    let val = f32::from_le_bytes([
        data[*offset],
        data[*offset + 1],
        data[*offset + 2],
        data[*offset + 3],
    ]);
    *offset += 4;
    val
}

/// Read a dense layer from the binary data.
fn read_layer(data: &[u8], offset: &mut usize) -> DenseLayer {
    let rows = read_u32(data, offset) as usize;
    let cols = read_u32(data, offset) as usize;

    let num_weights = rows * cols;
    let mut weights = Vec::with_capacity(num_weights);
    for _ in 0..num_weights {
        weights.push(read_f32(data, offset));
    }

    let mut biases = Vec::with_capacity(rows);
    for _ in 0..rows {
        biases.push(read_f32(data, offset));
    }

    DenseLayer {
        weights,
        biases,
        rows,
        cols,
    }
}

impl NnueNetwork {
    /// Load network from embedded binary weights.
    pub fn load() -> Self {
        let data = WEIGHTS_DATA;
        let mut offset = 0;

        // Skip magic "BBIRD" (5 bytes)
        offset += 5;

        // Number of layers
        let num_layers = read_u32(data, &mut offset);
        assert_eq!(num_layers, 3, "Expected 3 layers, got {}", num_layers);

        let layer1 = read_layer(data, &mut offset);
        let layer2 = read_layer(data, &mut offset);
        let layer3 = read_layer(data, &mut offset);

        assert_eq!(layer1.cols, 768, "Input layer must be 768");
        assert_eq!(layer3.rows, 1, "Output layer must be 1");

        Self {
            layer1,
            layer2,
            layer3,
        }
    }

    /// Evaluate a board position. Returns score in centipawns from white's perspective.
    pub fn evaluate(&self, board: &Board) -> i32 {
        // Build input features
        let mut input = [0.0f32; 768];
        for rank in 0..8u8 {
            for file in 0..8u8 {
                let sq = Square::new(file, rank);
                if let Some(piece) = board.piece_at(sq) {
                    let idx = piece_index(piece.color, piece.kind);
                    let sq_idx = sq.index(); // rank * 8 + file
                    input[idx * 64 + sq_idx] = 1.0;
                }
            }
        }

        // Forward pass
        let mut hidden1 = [0.0f32; 256];
        self.layer1.forward_relu(&input, &mut hidden1);

        let mut hidden2 = [0.0f32; 128];
        self.layer2.forward_relu(&hidden1, &mut hidden2);

        let mut output = [0.0f32; 1];
        self.layer3.forward_tanh(&hidden2, &mut output);

        // Convert tanh output [-1, 1] back to centipawns
        // Inverse of: val = cp / (|cp| + 400)
        // → cp = 400 * val / (1 - |val|)
        let val = output[0].clamp(-0.999, 0.999);
        let cp = 400.0 * val / (1.0 - val.abs());

        cp as i32
    }
}

/// Global NNUE instance (lazy-initialized, thread-safe).
use std::sync::OnceLock;
static NNUE: OnceLock<NnueNetwork> = OnceLock::new();

/// Get a reference to the global NNUE network.
pub fn get_nnue() -> &'static NnueNetwork {
    NNUE.get_or_init(|| NnueNetwork::load())
}

/// Evaluate a position using NNUE. Returns centipawns from white's perspective.
pub fn nnue_evaluate(board: &Board) -> i32 {
    get_nnue().evaluate(board)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Board;

    #[test]
    fn test_nnue_loads() {
        let net = NnueNetwork::load();
        assert_eq!(net.layer1.cols, 768);
        assert_eq!(net.layer1.rows, 256);
        assert_eq!(net.layer2.cols, 256);
        assert_eq!(net.layer2.rows, 128);
        assert_eq!(net.layer3.cols, 128);
        assert_eq!(net.layer3.rows, 1);
    }

    #[test]
    fn test_nnue_starting_position() {
        let board = Board::new();
        let score = nnue_evaluate(&board);
        // Starting position should be roughly equal (within ±200 cp)
        assert!(
            score.abs() < 200,
            "Starting position eval too extreme: {} cp",
            score
        );
    }

    #[test]
    fn test_nnue_material_advantage() {
        // White up a queen
        let board =
            Board::from_fen("rnb1kbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
                .unwrap();
        let score = nnue_evaluate(&board);
        // Should be positive (white is better)
        assert!(score > 0, "White up a queen should be positive: {} cp", score);
    }
}
