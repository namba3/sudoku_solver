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
    for val in 1..=9 {
        if !manager.set(x, y, val) {
            continue;
        }
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

/// State Manager that controls whether a matrix cell can have a value
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

    pub const fn num_candidates(&self, x: usize, y: usize) -> u8 {
        let bits = self.row[y] | self.col[x] | self.sub_mtx[y / 3][x / 3];
        9 - bits.count_ones() as u8
    }

    const fn flag(val: u8) -> u16 {
        1u16 << val
    }

    const fn is_settable(&self, x: usize, y: usize, val: u8) -> bool {
        let flag = Self::flag(val);
        let bits = self.row[y] | self.col[x] | self.sub_mtx[y / 3][x / 3];
        (bits & flag) == 0
    }
}
