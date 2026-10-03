use crate::Matrix;

/// Solve the Sudoku
pub fn solve(mtx: &mut Matrix) -> bool {
    let mut manager = StateManager::new();
    let mut empty_cells = Vec::new();

    for y in 0..9 {
        for x in 0..9 {
            let val = &mut mtx[y][x];
            if !(1..=9).contains(val) {
                empty_cells.push((x, y));
                *val = 0;
                continue;
            }
            if !manager.set(x, y, *val) {
                return false;
            }
        }
    }

    fill(mtx, &mut empty_cells, &mut manager)
}

/// Return the allowed digits for an empty cell at column `x`, row `y`.
///
/// Coordinates outside the 9×9 board and already-filled cells have no candidates.
pub fn candidates_for(mtx: &Matrix, x: usize, y: usize) -> Vec<u8> {
    if x >= 9 || y >= 9 || (1..=9).contains(&mtx[y][x]) {
        return Vec::new();
    }

    let mut used = 0;
    for i in 0..9 {
        let box_y = (y / 3) * 3 + i / 3;
        let box_x = (x / 3) * 3 + i % 3;
        for value in [mtx[y][i], mtx[i][x], mtx[box_y][box_x]] {
            if (1..=9).contains(&value) {
                used |= StateManager::flag(value);
            }
        }
    }

    Candidates::new(used).collect()
}

/// Fill the Sudoku matrix with temporary placement method
fn fill(
    mtx: &mut Matrix,
    empty_cells: &mut Vec<(usize, usize)>,
    manager: &mut StateManager,
) -> bool {
    if empty_cells.is_empty() {
        return true;
    }

    // Select the cell with fewest number of candidates
    let (x, y) = {
        let min_idx = empty_cells
            .iter()
            .copied()
            .enumerate()
            .min_by_key(|&(_idx, (x, y))| manager.num_candidates(x, y))
            .map(|(idx, _)| idx)
            .unwrap();
        empty_cells.swap_remove(min_idx)
    };

    // Backtrack if the cell has no candidates
    if manager.num_candidates(x, y) <= 0 {
        empty_cells.push((x, y));
        return false;
    }

    // Temporarily place candidates and self-recurse
    for val in manager.candidates(x, y) {
        manager.set(x, y, val);
        mtx[y][x] = val;
        if fill(mtx, empty_cells, manager) {
            return true;
        }
        manager.remove(x, y, val);
    }

    // Backtrack if the temporary placement method fails
    empty_cells.push((x, y));
    mtx[y][x] = 0;
    false
}

/// A state manager that manages whether each cell in the matrix can have candidates
///
/// This uses bitwise operations to find candidates faster than using naive `for` statements.
struct StateManager {
    row: [u16; 9],
    col: [u16; 9],
    sub_mtx: [[u16; 3]; 3],
}
impl StateManager {
    pub const fn new() -> Self {
        Self {
            row: [0; 9],
            col: [0; 9],
            sub_mtx: [[0; 3]; 3],
        }
    }

    pub fn set(&mut self, x: usize, y: usize, val: u8) -> bool {
        if !self.is_settable(x, y, val) {
            return false;
        }

        let flag = Self::flag(val);
        self.row[y] |= flag;
        self.col[x] |= flag;
        self.sub_mtx[y / 3][x / 3] |= flag;

        true
    }

    pub fn remove(&mut self, x: usize, y: usize, val: u8) {
        let mask = !Self::flag(val);
        self.row[y] &= mask;
        self.col[x] &= mask;
        self.sub_mtx[y / 3][x / 3] &= mask;
    }

    pub const fn candidates(&self, x: usize, y: usize) -> Candidates {
        let bits = self.bits(x, y);
        Candidates::new(bits)
    }

    pub const fn num_candidates(&self, x: usize, y: usize) -> u8 {
        let bits = self.bits(x, y);
        9 - bits.count_ones() as u8
    }

    const fn flag(val: u8) -> u16 {
        1u16 << (val - 1)
    }

    const fn bits(&self, x: usize, y: usize) -> u16 {
        self.row[y] | self.col[x] | self.sub_mtx[y / 3][x / 3]
    }

    const fn is_settable(&self, x: usize, y: usize, val: u8) -> bool {
        let flag = Self::flag(val);
        let bits = self.bits(x, y);
        (bits & flag) == 0
    }
}

pub struct Candidates {
    bits: u16,
    i: u8,
}
impl Candidates {
    pub const fn new(bits: u16) -> Self {
        Self { bits: !bits, i: 0 }
    }
}
impl Iterator for Candidates {
    type Item = u8;
    fn next(&mut self) -> Option<Self::Item> {
        while self.bits != 0 && self.i < 9 {
            let current = self.bits;
            self.bits >>= 1;
            self.i += 1;

            if current & 1 == 1 {
                return Some(self.i);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::{solve, Candidates, StateManager};
    use crate::Matrix;

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

    const SOLUTION: Matrix = [
        [5, 3, 4, 6, 7, 8, 9, 1, 2],
        [6, 7, 2, 1, 9, 5, 3, 4, 8],
        [1, 9, 8, 3, 4, 2, 5, 6, 7],
        [8, 5, 9, 7, 6, 1, 4, 2, 3],
        [4, 2, 6, 8, 5, 3, 7, 9, 1],
        [7, 1, 3, 9, 2, 4, 8, 5, 6],
        [9, 6, 1, 5, 3, 7, 2, 8, 4],
        [2, 8, 7, 4, 1, 9, 6, 3, 5],
        [3, 4, 5, 2, 8, 6, 1, 7, 9],
    ];

    #[test]
    fn solves_a_standard_puzzle() {
        let mut puzzle = PUZZLE;

        assert!(solve(&mut puzzle));
        assert_eq!(puzzle, SOLUTION);
    }

    #[test]
    fn rejects_duplicate_givens_in_a_row() {
        let mut puzzle = [[0; 9]; 9];
        puzzle[0][0] = 4;
        puzzle[0][1] = 4;

        assert!(!solve(&mut puzzle));
    }

    #[test]
    fn rejects_duplicate_givens_in_a_column() {
        let mut puzzle = [[0; 9]; 9];
        puzzle[0][0] = 4;
        puzzle[1][0] = 4;

        assert!(!solve(&mut puzzle));
    }

    #[test]
    fn rejects_duplicate_givens_in_a_subgrid() {
        let mut puzzle = [[0; 9]; 9];
        puzzle[0][0] = 4;
        puzzle[1][1] = 4;

        assert!(!solve(&mut puzzle));
    }

    #[test]
    fn candidates_yield_available_digits_in_order() {
        let blocked = (1 << (2 - 1)) | (1 << (5 - 1)) | (1 << (8 - 1));
        let candidates: Vec<_> = Candidates::new(blocked).collect();

        assert_eq!(candidates, vec![1, 3, 4, 6, 7, 9]);
    }

    #[test]
    fn state_manager_restores_candidates_after_removal() {
        let mut manager = StateManager::new();
        assert!(manager.set(0, 0, 1));
        assert!(!manager.is_settable(0, 1, 1));

        manager.remove(0, 0, 1);

        assert!(manager.is_settable(0, 1, 1));
    }

    #[test]
    fn candidates_for_excludes_digits_in_the_row_column_and_subgrid() {
        let mut puzzle = [[0; 9]; 9];
        puzzle[0][0] = 1;
        puzzle[3][2] = 2;
        puzzle[1][1] = 3;

        assert_eq!(super::candidates_for(&puzzle, 2, 2), vec![4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn candidates_for_returns_empty_for_filled_or_out_of_bounds_cells() {
        let mut puzzle = [[0; 9]; 9];
        puzzle[4][5] = 7;

        assert!(super::candidates_for(&puzzle, 5, 4).is_empty());
        assert!(super::candidates_for(&puzzle, 9, 4).is_empty());
        assert!(super::candidates_for(&puzzle, 5, 9).is_empty());
    }
}
