pub(super) struct BoardAnalysis {
    pub(super) conflicts: [[bool; 9]; 9],
    pub(super) candidate_masks: [[u16; 9]; 9],
    pub(super) no_candidates: [[bool; 9]; 9],
}

impl BoardAnalysis {
    pub(super) fn new(board: &sudoku_solver::Matrix) -> Self {
        const ALL_CANDIDATES: u16 = (1 << 9) - 1;

        let mut row_masks = [0u16; 9];
        let mut column_masks = [0u16; 9];
        let mut block_masks = [0u16; 9];
        let mut row_duplicates = [0u16; 9];
        let mut column_duplicates = [0u16; 9];
        let mut block_duplicates = [0u16; 9];

        for y in 0..9 {
            for x in 0..9 {
                let value = board[y][x];
                if !(1..=9).contains(&value) {
                    continue;
                }

                let bit = 1 << (value - 1);
                let block = (y / 3) * 3 + x / 3;
                if row_masks[y] & bit != 0 {
                    row_duplicates[y] |= bit;
                }
                if column_masks[x] & bit != 0 {
                    column_duplicates[x] |= bit;
                }
                if block_masks[block] & bit != 0 {
                    block_duplicates[block] |= bit;
                }
                row_masks[y] |= bit;
                column_masks[x] |= bit;
                block_masks[block] |= bit;
            }
        }

        let mut conflicts = [[false; 9]; 9];
        let mut candidate_masks = [[0u16; 9]; 9];
        let mut no_candidates = [[false; 9]; 9];
        for y in 0..9 {
            for x in 0..9 {
                let value = board[y][x];
                let block = (y / 3) * 3 + x / 3;
                if (1..=9).contains(&value) {
                    let bit = 1 << (value - 1);
                    conflicts[y][x] = row_duplicates[y] & bit != 0
                        || column_duplicates[x] & bit != 0
                        || block_duplicates[block] & bit != 0;
                } else {
                    let candidates =
                        ALL_CANDIDATES & !(row_masks[y] | column_masks[x] | block_masks[block]);
                    candidate_masks[y][x] = candidates;
                    no_candidates[y][x] = candidates == 0;
                }
            }
        }

        Self {
            conflicts,
            candidate_masks,
            no_candidates,
        }
    }

    pub(super) fn conflict_count(&self) -> usize {
        self.conflicts
            .iter()
            .flatten()
            .filter(|&&cell| cell)
            .count()
    }

    pub(super) fn no_candidate_count(&self) -> usize {
        self.no_candidates
            .iter()
            .flatten()
            .filter(|&&cell| cell)
            .count()
    }
}

pub(super) fn candidate_digits(mask: u16) -> impl Iterator<Item = u8> {
    (1..=9).filter(move |digit| mask & (1 << (digit - 1)) != 0)
}

#[cfg(test)]
mod tests {
    use super::{candidate_digits, BoardAnalysis};
    use sudoku_solver::Matrix;

    const PUZZLE: Matrix = [
        [5, 3, 0, 0, 7, 0, 0, 0, 0],
        [6, 0, 0, 1, 9, 5, 0, 0, 0],
        [0, 9, 8, 0, 0, 0, 0, 6, 0],
        [8, 0, 0, 0, 6, 0, 0, 0, 3],
        [4, 0, 0, 8, 0, 3, 0, 0, 1],
        [7, 0, 0, 0, 2, 0, 0, 0, 6],
        [0, 6, 0, 0, 0, 0, 2, 8, 0],
        [0, 0, 0, 4, 1, 9, 0, 0, 5],
        [0, 0, 0, 0, 8, 0, 0, 7, 9],
    ];

    #[test]
    fn candidate_masks_match_the_public_solver_api() {
        let analysis = BoardAnalysis::new(&PUZZLE);

        for y in 0..9 {
            for x in 0..9 {
                assert_eq!(
                    candidate_digits(analysis.candidate_masks[y][x]).collect::<Vec<_>>(),
                    sudoku_solver::candidates_for(&PUZZLE, x, y),
                    "candidate mismatch at ({y}, {x})"
                );
            }
        }
    }

    #[test]
    fn marks_all_cells_in_duplicate_units() {
        let mut board = Matrix::default();
        board[0][0] = 5;
        board[0][1] = 5;
        board[3][3] = 6;
        board[4][3] = 6;
        board[6][6] = 7;
        board[7][7] = 7;

        let analysis = BoardAnalysis::new(&board);
        for (y, x) in [(0, 0), (0, 1), (3, 3), (4, 3), (6, 6), (7, 7)] {
            assert!(analysis.conflicts[y][x], "expected conflict at ({y}, {x})");
        }
        assert!(!analysis.conflicts[0][2]);
        assert_eq!(analysis.conflict_count(), 6);
    }

    #[test]
    fn detects_empty_cells_with_no_legal_digits() {
        let mut board = Matrix::default();
        board[0] = [0, 1, 2, 3, 4, 5, 6, 7, 8];
        for (y, value) in [4, 6, 1, 2, 3, 5, 7, 8].into_iter().enumerate() {
            board[y + 1][0] = value;
        }
        board[1][1] = 9;

        let analysis = BoardAnalysis::new(&board);
        assert!(analysis.no_candidates[0][0]);
        assert_eq!(analysis.candidate_masks[0][0], 0);
        assert_eq!(analysis.no_candidate_count(), 1);
    }
}
